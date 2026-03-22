use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use two_wee_shared::{ColumnAlign, ColumnWidth, FieldType, TableSpec};

use crate::number_format;
use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Grid state — cell-level focus + inline editing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GridState {
    pub row: usize,
    pub col: usize,
    pub scroll_offset: usize,

    // Editing
    pub editing: bool,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    pub original_value: Option<String>,

    // Dirty tracking: (row_index, col_index) -> original value
    pub dirty_cells: HashMap<(usize, usize), String>,

    // Render output: position of the focused cell (set by draw_grid)
    pub focused_cell_rect: Option<(u16, u16, u16)>, // (x, y, width)
}

impl GridState {
    pub fn new() -> Self {
        Self {
            row: 0,
            col: 0,
            scroll_offset: 0,
            editing: false,
            cursor: 0,
            selection_anchor: None,
            original_value: None,
            dirty_cells: HashMap::new(),
            focused_cell_rect: None,
        }
    }

    pub fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
        }
    }

    pub fn move_down(&mut self, row_count: usize) {
        if row_count > 0 && self.row + 1 < row_count {
            self.row += 1;
        }
    }

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        }
    }

    pub fn move_right(&mut self, col_count: usize) {
        if col_count > 0 && self.col + 1 < col_count {
            self.col += 1;
        }
    }

    /// Tab: move right, wrap to next row.
    pub fn tab_next(&mut self, col_count: usize, row_count: usize) {
        if col_count == 0 {
            return;
        }
        if self.col + 1 < col_count {
            self.col += 1;
        } else if self.row + 1 < row_count {
            self.col = 0;
            self.row += 1;
        }
    }

    /// BackTab: move left, wrap to previous row.
    pub fn tab_prev(&mut self, col_count: usize) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = col_count.saturating_sub(1);
        }
    }

    pub fn page_up(&mut self, visible_rows: usize) {
        self.row = self.row.saturating_sub(visible_rows);
    }

    pub fn page_down(&mut self, row_count: usize, visible_rows: usize) {
        if row_count > 0 {
            self.row = (self.row + visible_rows).min(row_count - 1);
        }
    }

    fn ensure_visible(&mut self, visible_rows: usize) {
        if self.row < self.scroll_offset {
            self.scroll_offset = self.row;
        } else if visible_rows > 0 && self.row >= self.scroll_offset + visible_rows {
            self.scroll_offset = self.row - visible_rows + 1;
        }
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty_cells.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Resolve column constraints to pixel widths for a given total width.
fn resolve_widths(spec: &TableSpec, total: u16) -> Vec<u16> {
    let mut widths = Vec::with_capacity(spec.columns.len());
    let mut fixed_total: u16 = 0;
    let mut fill_count: u16 = 0;
    // Account for column dividers: (n-1) dividers of 1 char each
    let dividers = if spec.columns.len() > 1 { (spec.columns.len() - 1) as u16 } else { 0 };
    let available = total.saturating_sub(dividers);

    for col in &spec.columns {
        match &col.width {
            ColumnWidth::Fixed(w) => {
                widths.push(*w);
                fixed_total += w;
            }
            ColumnWidth::Fill(_) => {
                widths.push(0); // placeholder
                fill_count += 1;
            }
        }
    }

    if fill_count > 0 {
        let remaining = available.saturating_sub(fixed_total);
        let per_fill = remaining / fill_count;
        let mut extra = remaining % fill_count;
        for (i, col) in spec.columns.iter().enumerate() {
            if matches!(&col.width, ColumnWidth::Fill(_)) {
                widths[i] = per_fill + if extra > 0 { extra -= 1; 1 } else { 0 };
            }
        }
    }

    widths
}

/// Draw an editable grid with cell-level focus and column dividers.
pub fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    spec: &TableSpec,
    state: &mut GridState,
    theme: &Theme,
    decimal_sep: &str,
    thousand_sep: &str,
) {
    // We draw the border manually so column dividers connect properly.
    let border_style = Style::default().bg(theme.grid_bg).fg(theme.grid_line);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if inner.height < 3 || inner.width < 4 || spec.columns.is_empty() {
        // Fall back to simple border
        let block = Block::default()
            .borders(Borders::ALL)
            .style(border_style);
        frame.render_widget(block, area);
        return;
    }

    let col_widths = resolve_widths(spec, inner.width);
    let col_count = spec.columns.len();

    // Guard: total content width (columns + dividers) must not exceed inner.width
    let dividers_w = if col_count > 1 { (col_count - 1) as u16 } else { 0 };
    let total_cols_w: u16 = col_widths.iter().sum::<u16>() + dividers_w;
    if total_cols_w > inner.width {
        // Terminal too narrow — just draw the border and bail
        let block = Block::default()
            .borders(Borders::ALL)
            .style(border_style);
        frame.render_widget(block, area);
        return;
    }

    // Layout: header (1 line) + separator (1 line) + data rows
    let header_y = inner.y;
    let sep_y = inner.y + 1;
    let data_y = inner.y + 2;
    let data_height = inner.height.saturating_sub(2);
    let visible_rows = data_height as usize;

    state.ensure_visible(visible_rows);

    // --- Top border with ┬ at column divider positions ---
    {
        let mut top = String::with_capacity(area.width as usize);
        top.push('┌');
        for (ci, _) in spec.columns.iter().enumerate() {
            let w = col_widths[ci] as usize;
            top.push_str(&"─".repeat(w));
            if ci + 1 < col_count {
                top.push('┬');
            }
        }
        top.push('┐');
        frame.render_widget(
            Paragraph::new(top).style(border_style),
            Rect { x: area.x, y: area.y, width: area.width, height: 1 },
        );
    }

    // --- Left border (column of │) ---
    for dy in 0..inner.height {
        frame.render_widget(
            Paragraph::new("│").style(border_style),
            Rect { x: area.x, y: inner.y + dy, width: 1, height: 1 },
        );
        frame.render_widget(
            Paragraph::new("│").style(border_style),
            Rect { x: area.x + area.width - 1, y: inner.y + dy, width: 1, height: 1 },
        );
    }

    // --- Bottom border with ┴ at column divider positions ---
    {
        let bottom_y = area.y + area.height - 1;
        let mut bot = String::with_capacity(area.width as usize);
        bot.push('└');
        for (ci, _) in spec.columns.iter().enumerate() {
            let w = col_widths[ci] as usize;
            bot.push_str(&"─".repeat(w));
            if ci + 1 < col_count {
                bot.push('┴');
            }
        }
        bot.push('┘');
        frame.render_widget(
            Paragraph::new(bot).style(border_style),
            Rect { x: area.x, y: bottom_y, width: area.width, height: 1 },
        );
    }

    // --- Header row ---
    let header_style = Style::default()
        .bg(theme.grid_bg)
        .fg(theme.grid_header_fg)
        .add_modifier(Modifier::BOLD);

    let mut x = inner.x;
    for (ci, col) in spec.columns.iter().enumerate() {
        let w = col_widths[ci];
        if w == 0 {
            continue;
        }
        let cell_rect = Rect { x, y: header_y, width: w, height: 1 };
        let label = &col.label;
        let text = fit_text(label, w as usize, col.align);
        frame.render_widget(
            Paragraph::new(text).style(header_style),
            cell_rect,
        );
        x += w;

        // Column divider
        if ci + 1 < col_count {
            frame.render_widget(
                Paragraph::new("│").style(border_style),
                Rect { x, y: header_y, width: 1, height: 1 },
            );
            x += 1;
        }
    }

    // --- Separator row with ├ ... ┼ ... ┤ ---
    {
        let mut sep = String::with_capacity(area.width as usize);
        for (ci, _) in spec.columns.iter().enumerate() {
            let w = col_widths[ci] as usize;
            sep.push_str(&"─".repeat(w));
            if ci + 1 < col_count {
                sep.push('┼');
            }
        }
        // Left junction ├ and right junction ┤
        frame.render_widget(
            Paragraph::new("├").style(border_style),
            Rect { x: area.x, y: sep_y, width: 1, height: 1 },
        );
        frame.render_widget(
            Paragraph::new(sep).style(border_style),
            Rect { x: inner.x, y: sep_y, width: inner.width, height: 1 },
        );
        frame.render_widget(
            Paragraph::new("┤").style(border_style),
            Rect { x: area.x + area.width - 1, y: sep_y, width: 1, height: 1 },
        );
    }

    // --- Data rows ---
    let _end = (state.scroll_offset + visible_rows).min(spec.rows.len());
    let normal_style = Style::default().bg(theme.grid_bg).fg(theme.grid_text);
    let focused_style = Style::default().bg(theme.grid_cell_focused_bg).fg(theme.grid_cell_focused_fg);
    let editing_style = Style::default().bg(theme.grid_cell_editing_bg).fg(theme.grid_cell_editing_fg);
    let divider_style = Style::default().bg(theme.grid_bg).fg(theme.grid_line);

    for vi in 0..visible_rows {
        let ri = state.scroll_offset + vi;
        let row_y = data_y + vi as u16;
        if row_y >= inner.y + inner.height {
            break;
        }

        let mut x = inner.x;
        for ci in 0..col_count {
            let w = col_widths[ci];
            if w == 0 {
                continue;
            }
            let cell_rect = Rect { x, y: row_y, width: w, height: 1 };

            let is_focused = ri == state.row && ci == state.col;
            if is_focused {
                state.focused_cell_rect = Some((x, row_y, w));
            }

            if ri < spec.rows.len() {
                let raw_value = spec.rows[ri].values.get(ci).map(|s| s.as_str()).unwrap_or("");
                let col_def = spec.columns.get(ci);
                let align = col_def.map(|c| c.align).unwrap_or(ColumnAlign::Left);
                let col_type = col_def.map(|c| &c.col_type);

                // Format for display (not during editing of this cell)
                let display_formatted;
                let value = if is_focused && state.editing {
                    raw_value // show raw user input during editing
                } else if matches!(col_type, Some(FieldType::Decimal)) {
                    display_formatted = number_format::format_decimal(
                        raw_value, decimal_sep, thousand_sep, None,
                    );
                    &display_formatted
                } else if matches!(col_type, Some(FieldType::Integer)) {
                    display_formatted = number_format::format_integer(
                        raw_value, thousand_sep,
                    );
                    &display_formatted
                } else {
                    raw_value
                };

                let style = if is_focused && state.editing {
                    editing_style
                } else if is_focused {
                    focused_style
                } else {
                    normal_style
                };

                if is_focused && state.editing && state.selection_anchor.is_some() {
                    // Selection active — render with inverted selection highlight
                    let anchor = state.selection_anchor.unwrap();
                    let sel_start = anchor.min(state.cursor);
                    let sel_end = anchor.max(state.cursor);
                    let text = render_cell_with_selection(value, sel_start, sel_end, w as usize, theme);
                    frame.render_widget(text, cell_rect);
                } else if is_focused && state.editing {
                    // Show value with cursor
                    let cursor_pos = state.cursor.min(value.chars().count());
                    let text = render_cell_with_cursor(value, cursor_pos, w as usize);
                    frame.render_widget(Paragraph::new(text).style(style), cell_rect);
                } else {
                    let text = fit_text(value, w as usize, align);
                    frame.render_widget(Paragraph::new(text).style(style), cell_rect);
                }
            } else {
                // Empty row — just fill with background
                let style = if is_focused { focused_style } else { normal_style };
                frame.render_widget(
                    Paragraph::new(" ".repeat(w as usize)).style(style),
                    cell_rect,
                );
            }

            x += w;

            // Column divider for data rows
            if ci + 1 < col_count {
                frame.render_widget(
                    Paragraph::new("│").style(divider_style),
                    Rect { x, y: row_y, width: 1, height: 1 },
                );
                x += 1;
            }
        }
    }
}

/// Fit text into a fixed width with alignment, truncating if needed.
fn fit_text(value: &str, width: usize, align: ColumnAlign) -> String {
    let len = value.chars().count();
    if len >= width {
        value.chars().take(width).collect()
    } else {
        let pad = width - len;
        match align {
            ColumnAlign::Right => format!("{}{}", " ".repeat(pad), value),
            ColumnAlign::Center => {
                let left = pad / 2;
                let right = pad - left;
                format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
            }
            _ => format!("{}{}", value, " ".repeat(pad)),
        }
    }
}

/// Render cell text with a visible cursor character (block cursor).
fn render_cell_with_cursor(value: &str, cursor: usize, width: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut result = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        if i == cursor {
            result.push('█'); // block cursor placeholder — visible cursor position
        }
        result.push(ch);
    }
    if cursor >= chars.len() {
        result.push('█');
    }
    // Pad or truncate to width
    let len = result.chars().count();
    if len < width {
        result.push_str(&" ".repeat(width - len));
    } else if len > width {
        result = result.chars().take(width).collect();
    }
    result
}

/// Render cell text with a selection highlight (used during F2 select-all state).
fn render_cell_with_selection<'a>(
    value: &str,
    sel_start: usize,
    sel_end: usize,
    width: usize,
    theme: &Theme,
) -> Paragraph<'a> {
    let edit_style = Style::default().bg(theme.grid_cell_editing_bg).fg(theme.grid_cell_editing_fg);
    let sel_style = Style::default().bg(theme.field_text_selected_bg).fg(theme.field_text_selected_fg);

    // Pad value to width
    let char_len = value.chars().count();
    let padded: String = if char_len < width {
        format!("{}{}", value, " ".repeat(width - char_len))
    } else {
        value.chars().take(width).collect()
    };

    let chars: Vec<char> = padded.chars().collect();
    let mut spans: Vec<Span<'a>> = Vec::new();

    // Split into before / selected / after
    let before: String = chars[..sel_start.min(chars.len())].iter().collect();
    let selected: String = chars[sel_start.min(chars.len())..sel_end.min(chars.len())].iter().collect();
    let after: String = chars[sel_end.min(chars.len())..].iter().collect();

    if !before.is_empty() {
        spans.push(Span::styled(before, edit_style));
    }
    if !selected.is_empty() {
        spans.push(Span::styled(selected, sel_style));
    }
    if !after.is_empty() {
        spans.push(Span::styled(after, edit_style));
    }

    Paragraph::new(Line::from(spans))
}
