use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::HeaderInputMode;
use crate::theme::Theme;

/// All the data needed to render a single labeled field row.
pub struct InputField<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub max_label_width: usize,
    pub input_width: usize,
    pub focused: bool,
    pub mode: HeaderInputMode,
    pub cursor: usize,
    pub read_only: bool,
    pub color: Option<&'a str>,
    pub bold: bool,
    pub is_password: bool,
    pub dirty: bool,
    pub selection: Option<(usize, usize)>,
    pub theme: &'a Theme,
}

/// All the data needed to render a multi-line TextArea field.
pub struct TextAreaField<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub max_label_width: usize,
    pub input_width: usize,
    pub focused: bool,
    pub mode: HeaderInputMode,
    /// Flat char offset of the cursor within `value` (newlines count as 1).
    pub cursor_flat: usize,
    /// Number of visible rows to display.
    pub row_count: usize,
    pub dirty: bool,
    pub theme: &'a Theme,
    /// Flat (start, end) char range for text selection highlight.
    pub selection: Option<(usize, usize)>,
}

/// Render a field as a single styled Line: `*Label.........: [value]`
pub fn render_field(field: InputField<'_>) -> Line<'static> {
    let t = field.theme;
    let dirty_prefix = if field.dirty { " *" } else { "  " };
    let label_len = field.label.chars().count();
    let dots = ".".repeat(field.max_label_width.saturating_sub(label_len) + 2);
    let label_with_dots = format!("{}{}{}:", dirty_prefix, field.label, dots);

    let mut value_chars: Vec<char> = field.value.chars().collect();
    if value_chars.len() > field.input_width {
        value_chars.truncate(field.input_width);
    }

    let edit_style = Style::default().bg(t.field_editing_bg).fg(t.field_editing_fg);
    let select_style = Style::default().bg(t.field_focused_bg).fg(t.field_focused_fg);
    let selection_highlight = Style::default()
        .bg(t.field_text_selected_bg)
        .fg(t.field_text_selected_fg);

    // Password masking: always show bullet for password fields
    let masked_chars: Vec<char>;
    let display_chars = if field.is_password {
        masked_chars = vec!['\u{00B7}'; value_chars.len()]; // middle dot ·
        &masked_chars
    } else {
        &value_chars
    };

    // Build value spans based on mode + selection
    let value_spans: Vec<Span<'static>> =
        if field.focused && field.mode == HeaderInputMode::Edit && field.is_password {
            // Password: no cursor, just masked dots with edit background
            vec![Span::styled(pad_to_width(display_chars, field.input_width), edit_style)]
        } else if field.focused && field.mode == HeaderInputMode::Edit && field.selection.is_some() {
            render_selection(display_chars, field.selection.unwrap(), field.input_width, edit_style, selection_highlight)
        } else if field.focused && field.mode == HeaderInputMode::Edit {
            render_cursor(display_chars, field.cursor, field.input_width, edit_style)
        } else {
            render_display(display_chars, &field, select_style, t)
        };

    let mut spans = vec![
        Span::styled(label_with_dots, Style::default().fg(t.label)),
        Span::styled(" ", Style::default().fg(t.label)),
    ];
    spans.extend(value_spans);
    Line::from(spans)
}

/// Render a TextArea field as multiple lines.
///
/// Returns `row_count` lines: the first shows the label, subsequent lines show
/// the blank label area. When the cursor has scrolled below row 0 an `↑`
/// indicator replaces the label on the first visible line.
pub fn render_textarea(field: TextAreaField<'_>) -> Vec<Line<'static>> {
    let t = field.theme;

    let dirty_prefix = if field.dirty { " *" } else { "  " };
    let label_len = field.label.chars().count();
    let dots = ".".repeat(field.max_label_width.saturating_sub(label_len) + 2);
    let label_with_dots = format!("{}{}{}:", dirty_prefix, field.label, dots);

    let edit_style = Style::default().bg(t.field_editing_bg).fg(t.field_editing_fg);
    let select_style = Style::default().bg(t.field_focused_bg).fg(t.field_focused_fg);
    let selection_highlight = Style::default()
        .bg(t.field_text_selected_bg)
        .fg(t.field_text_selected_fg);
    let label_style = Style::default().fg(t.label);
    let value_style = Style::default().fg(t.field_value);

    // blank label area for continuation lines — same width as label_with_dots
    let blank_label = " ".repeat(label_with_dots.chars().count());

    // Single pass: split value into lines, compute cursor position, and optionally
    // flat start offsets (only needed when a selection is active).
    let mut value_lines: Vec<&str> = Vec::new();
    // Only track flat offsets when a selection is active (needed for overlap computation).
    let track_offsets = field.selection.is_some();
    let mut row_flat_starts: Vec<usize> = Vec::new();
    let mut cursor_row = 0usize;
    let mut cursor_col = 0usize;
    {
        let mut flat_offset = 0usize;
        let mut remaining = field.cursor_flat;
        let mut cursor_found = false;
        for line in field.value.split('\n') {
            let line_char_len = line.chars().count();
            if track_offsets {
                row_flat_starts.push(flat_offset);
            }
            value_lines.push(line);

            if !cursor_found {
                if remaining <= line_char_len {
                    cursor_row = value_lines.len() - 1;
                    cursor_col = remaining;
                    cursor_found = true;
                } else {
                    remaining -= line_char_len + 1; // +1 for '\n'
                }
            }

            flat_offset += line_char_len + 1;
        }
        // If value is empty or cursor is at the very end (past last '\n')
        if !cursor_found {
            cursor_row = value_lines.len().saturating_sub(1);
            cursor_col = remaining;
        }
    }

    // Pad value_lines to row_count with empty slices
    while value_lines.len() < field.row_count {
        if track_offsets {
            row_flat_starts.push(*row_flat_starts.last().unwrap_or(&0));
        }
        value_lines.push("");
    }

    // Scroll: keep cursor_row in view
    let scroll_offset = if cursor_row >= field.row_count {
        cursor_row - (field.row_count - 1)
    } else {
        0
    };

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(field.row_count);

    for visible_idx in 0..field.row_count {
        let data_idx = visible_idx + scroll_offset;
        let row_text = value_lines.get(data_idx).copied().unwrap_or("");
        let chars: Vec<char> = row_text.chars().collect();

        let is_active_row = field.focused && field.mode == HeaderInputMode::Edit && data_idx == cursor_row;
        let is_select_mode = field.focused && field.mode == HeaderInputMode::Select;

        // Compute per-row selection overlap (flat → row-local col range)
        let row_sel: Option<(usize, usize)> = field.selection.and_then(|(sel_start, sel_end)| {
            let row_start = row_flat_starts.get(data_idx).copied().unwrap_or(usize::MAX);
            let row_end = row_start + chars.len();
            // +1 on row_end to include the newline character in selection coverage
            if sel_end <= row_start || sel_start > row_end {
                None
            } else {
                let local_start = sel_start.saturating_sub(row_start).min(chars.len());
                let local_end = (sel_end - row_start).min(chars.len());
                if local_start < local_end { Some((local_start, local_end)) } else { None }
            }
        });

        let value_spans: Vec<Span<'static>> = if let Some(sel) = row_sel {
            // Text selection: before / selected / after
            let base_style = if field.focused && field.mode == HeaderInputMode::Edit { edit_style } else { value_style };
            render_selection(&chars, sel, field.input_width, base_style, selection_highlight)
        } else if is_active_row {
            render_cursor(&chars, cursor_col, field.input_width, edit_style)
        } else {
            // Select mode, other edit rows, and normal display all differ only by style
            let style = if is_select_mode {
                select_style
            } else if field.focused && field.mode == HeaderInputMode::Edit {
                edit_style
            } else {
                value_style
            };
            vec![Span::styled(pad_to_width(&chars, field.input_width), style)]
        };

        // Build the label/indent area for this row
        let label_text = if visible_idx == 0 && scroll_offset == 0 {
            label_with_dots.clone()
        } else if visible_idx == 0 {
            // Scrolled: show ↑ indicator in place of label
            if blank_label.len() >= 2 {
                format!("↑ {}", &blank_label[2..])
            } else {
                blank_label.clone()
            }
        } else {
            blank_label.clone()
        };

        let mut line_spans = vec![
            Span::styled(label_text, label_style),
            Span::styled(" ", label_style),
        ];
        line_spans.extend(value_spans);
        lines.push(Line::from(line_spans));
    }

    lines
}

/// All the data needed to render a single Boolean toggle field row.
pub struct BooleanField<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub true_label: Option<&'a str>,
    pub false_label: Option<&'a str>,
    pub true_color: Option<&'a str>,
    pub false_color: Option<&'a str>,
    pub max_label_width: usize,
    pub input_width: usize,
    pub focused: bool,
    pub dirty: bool,
    pub theme: &'a Theme,
}

/// Render a Boolean toggle field as a single styled Line.
///
/// Shows `true_label` (default "Yes") or `false_label` (default "No").
/// Colors come from `true_color`/`false_color` name strings; if omitted the
/// normal field value color is used.
pub fn render_boolean(field: BooleanField<'_>) -> Line<'static> {
    let t = field.theme;
    let is_true = field.value == "true";

    let dirty_prefix = if field.dirty { " *" } else { "  " };
    let label_len = field.label.chars().count();
    let dots = ".".repeat(field.max_label_width.saturating_sub(label_len) + 2);
    let label_with_dots = format!("{}{}{}:", dirty_prefix, field.label, dots);

    let text = if is_true {
        field.true_label.unwrap_or("Yes")
    } else {
        field.false_label.unwrap_or("No")
    };

    let chars: Vec<char> = text.chars().collect();
    let padded = pad_to_width(&chars, field.input_width);

    let value_span: Span<'static> = if field.focused {
        Span::styled(padded, Style::default().bg(t.field_focused_bg).fg(t.field_focused_fg))
    } else {
        let color_name = if is_true { field.true_color } else { field.false_color };
        let style = match color_name {
            Some(name) => Style::default().fg(color_from_name(name)),
            None => Style::default().fg(t.field_value),
        };
        Span::styled(padded, style)
    };

    Line::from(vec![
        Span::styled(label_with_dots, Style::default().fg(t.label)),
        Span::styled(" ", Style::default().fg(t.label)),
        value_span,
    ])
}

/// Convert a flat char offset in `value` (where `\n` counts as 1) to `(row, col)`.
///
/// Used both inside `render_textarea` and by callers that need the cursor row
/// to compute `focused_rect` scroll position.
pub fn flat_cursor_to_row_col(value: &str, cursor_flat: usize) -> (usize, usize) {
    let mut remaining = cursor_flat;
    let mut row = 0;
    for (i, line) in value.split('\n').enumerate() {
        let line_len = line.chars().count();
        if remaining <= line_len {
            return (i, remaining);
        }
        remaining -= line_len + 1;
        row = i + 1;
    }
    (row, remaining)
}

/// Pad or truncate `chars` to exactly `width` display columns.
fn pad_to_width(chars: &[char], width: usize) -> String {
    let len = chars.len();
    if len == width {
        chars.iter().collect()
    } else if len < width {
        let mut s: String = chars.iter().collect();
        s.extend(std::iter::repeat(' ').take(width - len));
        s
    } else {
        chars[..width].iter().collect()
    }
}

/// Edit mode with text selection (Shift+arrow or Ctrl+A).
///
/// Renders before / selected / after sections with distinct highlight styles.
fn render_selection(
    chars: &[char],
    selection: (usize, usize),
    width: usize,
    edit_style: Style,
    sel_style: Style,
) -> Vec<Span<'static>> {
    let len = chars.len();
    let s = selection.0.min(len);
    let e = selection.1.min(len);
    let before: String = chars[..s].iter().collect();
    let selected: String = chars[s..e].iter().collect();
    let after: String = chars[e..].iter().collect();
    let padding = " ".repeat(width.saturating_sub(len));
    vec![
        Span::styled(before, edit_style),
        Span::styled(selected, sel_style),
        Span::styled(format!("{}{}", after, padding), edit_style),
    ]
}

/// Edit mode with block cursor (█) at `cursor` position.
fn render_cursor(
    chars: &[char],
    cursor: usize,
    width: usize,
    edit_style: Style,
) -> Vec<Span<'static>> {
    let cursor = cursor.min(width).min(chars.len());
    let left: String = chars[..cursor].iter().collect();
    let right: String = chars[cursor..].iter().collect();
    let display = format!("{}█{}", left, right);
    // The cursor block adds one char; pad from that length
    let display_len = display.chars().count();
    let padded = if display_len < width {
        format!("{}{}", display, " ".repeat(width - display_len))
    } else {
        display.chars().take(width).collect()
    };
    vec![Span::styled(padded, edit_style)]
}

/// Select mode / read-only display.
fn render_display(
    chars: &[char],
    field: &InputField<'_>,
    select_style: Style,
    t: &Theme,
) -> Vec<Span<'static>> {
    let style = if field.focused && field.mode == HeaderInputMode::Select {
        select_style
    } else if field.color.is_some() || field.bold {
        let mut s = Style::default();
        if let Some(name) = field.color {
            s = s.fg(color_from_name(name));
        } else {
            s = s.fg(t.field_value);
        }
        if field.bold {
            s = s.add_modifier(Modifier::BOLD);
        }
        s
    } else if field.read_only {
        Style::default().fg(t.field_readonly)
    } else {
        Style::default().fg(t.field_value)
    };

    vec![Span::styled(pad_to_width(chars, field.input_width), style)]
}

/// Map a color name from FieldStyle to a ratatui Color.
fn color_from_name(name: &str) -> ratatui::style::Color {
    use ratatui::style::Color;
    if name.eq_ignore_ascii_case("yellow") { Color::Yellow }
    else if name.eq_ignore_ascii_case("red") { Color::Red }
    else if name.eq_ignore_ascii_case("green") { Color::Green }
    else if name.eq_ignore_ascii_case("blue") { Color::Blue }
    else if name.eq_ignore_ascii_case("cyan") { Color::Cyan }
    else if name.eq_ignore_ascii_case("magenta") { Color::Magenta }
    else if name.eq_ignore_ascii_case("white") { Color::White }
    else if name.eq_ignore_ascii_case("black") { Color::Black }
    else if name.eq_ignore_ascii_case("gray") || name.eq_ignore_ascii_case("grey") { Color::Gray }
    else { Color::White }
}
