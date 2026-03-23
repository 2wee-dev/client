use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{app::App, theme::ThemeMode};
use super::field::{InputField, TextAreaField, BooleanField, render_boolean, render_field, render_textarea};

/// Center a fixed-size rect (cols x rows) within the given area.
fn centered_fixed(cols: u16, rows: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(cols) / 2;
    let y = area.y + area.height.saturating_sub(rows) / 2;
    Rect {
        x,
        y,
        width: cols.min(area.width),
        height: rows.min(area.height),
    }
}

/// Draw a modal with centered title, border, and content rendered by a callback.
/// Returns the inner rect for the caller to render content into.
fn draw_modal_frame(frame: &mut Frame, app: &App, width: u16, height: u16, title: &str) -> Rect {
    let modal_area = centered_fixed(width, height, frame.area());
    frame.render_widget(Clear, modal_area);

    let bg = app.theme.modal_bg;

    // Border
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(bg).fg(app.theme.modal_border));
    frame.render_widget(block, modal_area);

    // Centered title on the first inner line
    let title_y = modal_area.y + 1;
    frame.render_widget(
        Paragraph::new(title)
            .alignment(Alignment::Center)
            .style(Style::default().bg(bg).fg(app.theme.card_title)
                .add_modifier(Modifier::BOLD)),
        Rect {
            x: modal_area.x + 1,
            y: title_y,
            width: modal_area.width.saturating_sub(2),
            height: 1,
        },
    );

    // Content area starts after title + blank line
    Rect {
        x: modal_area.x + 1,
        y: title_y + 2,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(4), // border(2) + title(1) + gap(1)
    }
}

/// Render a selectable option line — highlight spans full width.
fn option_line(label: &str, selected: bool, width: u16, app: &App) -> Line<'static> {
    let style = if selected {
        Style::default()
            .bg(app.theme.modal_selected_bg)
            .fg(app.theme.modal_selected_fg)
    } else {
        Style::default()
            .bg(app.theme.modal_bg)
            .fg(app.theme.modal_text)
    };
    // Pad to full width for full-row highlight
    let text = format!("  {}", label);
    let padded = format!("{:<width$}", text, width = width as usize);
    Line::from(Span::styled(padded, style))
}

pub fn draw_theme_modal(frame: &mut Frame, app: &App) {
    let themes = [ThemeMode::Default, ThemeMode::Navision, ThemeMode::IbmAS400, ThemeMode::Color256];
    // Height: border(2) + title(1) + gap(1) + options(3) + gap(1) + current(1)
    let h = 2 + 1 + 1 + themes.len() as u16 + 1 + 1;

    let inner = draw_modal_frame(frame, app, 40, h, "Theme");

    let mut lines: Vec<Line> = Vec::new();
    for (index, mode) in themes.iter().enumerate() {
        lines.push(option_line(
            App::theme_label(*mode),
            app.theme_modal_index == index,
            inner.width,
            app,
        ));
    }

    // Current theme indicator
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  Current: {}", App::theme_label(app.theme_mode)),
        Style::default().fg(app.theme.field_readonly),
    )));

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.modal_bg)),
        inner,
    );
}

pub fn draw_quit_confirm_modal(frame: &mut Frame, app: &App) {
    let has_logout = app.auth_token.is_some();
    let opt_count = app.quit_modal_option_count();
    // Height: border(2) + title(1) + gap(1) + options
    let h = (4 + opt_count) as u16;

    let title = if app.app_name.is_empty() {
        app.ui_strings.quit_message.clone()
    } else {
        format!("{} {}?", app.ui_strings.quit_title, app.app_name)
    };
    let inner = draw_modal_frame(frame, app, 40, h, &title);

    // Selectable options
    let mut options: Vec<&str> = vec![&app.ui_strings.quit_yes, &app.ui_strings.quit_no];
    if has_logout {
        options.push(&app.ui_strings.logout);
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, label) in options.iter().enumerate() {
        lines.push(option_line(label, i == app.quit_modal_index, inner.width, app));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.modal_bg)),
        inner,
    );
}

/// Reusable confirm dialog with side-by-side buttons (left/right navigation).
///
/// Renders a centered modal with a title, message, and horizontal button row.
/// Navigate with Left/Right arrows (also Tab/BackTab, Up/Down); Enter confirms.
/// Width auto-sizes to fit the message with padding, clamped to screen width.
pub fn draw_confirm_dialog(
    frame: &mut Frame,
    app: &App,
    title: &str,
    message: &str,
    buttons: &[&str],
    selected: usize,
) {
    // Height: border(2) + title(1) + gap(1) + message(1) + gap(1) + buttons(1) = 7
    let h: u16 = 7;
    // Auto-size width: message + padding, at least 30, at most screen width - 4
    let msg_len = message.chars().count() as u16;
    let title_len = title.chars().count() as u16;
    let content_w = msg_len.max(title_len) + 6; // +6 for border + inner padding
    let max_w = frame.area().width.saturating_sub(4);
    let w = content_w.clamp(30, max_w);
    let inner = draw_modal_frame(frame, app, w, h, title);

    let msg_len = message.chars().count();
    let msg_pad = (inner.width as usize).saturating_sub(msg_len) / 2;
    let msg_line = Line::from(Span::styled(
        format!("{}{}", " ".repeat(msg_pad), message),
        Style::default().fg(app.theme.modal_text).bg(app.theme.modal_bg),
    ));

    // Build side-by-side button spans
    let btn_width = (inner.width as usize) / buttons.len().max(1);
    let mut btn_spans: Vec<Span> = Vec::new();
    for (i, label) in buttons.iter().enumerate() {
        let style = if i == selected {
            Style::default().bg(app.theme.modal_selected_bg).fg(app.theme.modal_selected_fg)
        } else {
            Style::default().bg(app.theme.modal_bg).fg(app.theme.modal_text)
        };
        // Center label within its button area
        let label_len = label.chars().count();
        let pad_total = btn_width.saturating_sub(label_len);
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        let padded = format!("{}{}{}", " ".repeat(pad_left), label, " ".repeat(pad_right));
        btn_spans.push(Span::styled(padded, style));
    }

    let lines: Vec<Line> = vec![
        msg_line,
        Line::from(""),
        Line::from(btn_spans),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.modal_bg)),
        inner,
    );
}

pub fn draw_delete_confirm_modal(frame: &mut Frame, app: &App) {
    let record_name = app
        .current_screen
        .as_ref()
        .map(|s| s.title.clone())
        .unwrap_or_default();
    draw_confirm_dialog(
        frame, app,
        "Delete Record",
        &format!("Do you want to delete {}?", record_name),
        &["Yes", "No"],
        app.delete_modal_index,
    );
}

pub fn draw_grid_delete_confirm_modal(frame: &mut Frame, app: &App) {
    let row_num = app.grid_state.row + 1;
    draw_confirm_dialog(
        frame, app,
        "Delete Line",
        &format!("Do you want to delete line {}?", row_num),
        &["Yes", "No"],
        app.grid_delete_modal_index,
    );
}

pub fn draw_save_confirm_modal(frame: &mut Frame, app: &App) {
    // Height: border(2) + title(1) + gap(1) + 3 options
    let h: u16 = 7;

    let inner = draw_modal_frame(frame, app, 44, h, &app.ui_strings.save_confirm_message.clone());

    let options = [
        app.ui_strings.save_confirm_save.as_str(),
        app.ui_strings.save_confirm_discard.as_str(),
        app.ui_strings.save_confirm_cancel.as_str(),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (i, label) in options.iter().enumerate() {
        lines.push(option_line(label, i == app.save_modal_index, inner.width, app));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.modal_bg)),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Action picker modal
// ---------------------------------------------------------------------------

pub fn draw_action_picker_modal(frame: &mut Frame, app: &App) {
    let actions = app.screen_actions();
    if actions.is_empty() {
        return;
    }

    let bg = app.theme.modal_bg;
    let border_fg = app.theme.modal_border;
    let text_fg = app.theme.modal_text;

    let item_count = actions.len();
    // Height: border(2) + title(1) + gap(1) + items + gap(1) + hint(1)
    let h = (4 + item_count + 2) as u16;

    // Draw the action picker with its own distinct styling
    let modal_area = centered_fixed(50, h, frame.area());
    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_fg))
        .style(Style::default().bg(bg).fg(border_fg));
    frame.render_widget(block, modal_area);

    // Title
    let title_y = modal_area.y + 1;
    frame.render_widget(
        Paragraph::new("Posting")
            .alignment(Alignment::Center)
            .style(Style::default().bg(bg).fg(text_fg)
                .add_modifier(Modifier::BOLD)),
        Rect {
            x: modal_area.x + 1,
            y: title_y,
            width: modal_area.width.saturating_sub(2),
            height: 1,
        },
    );

    let inner = Rect {
        x: modal_area.x + 1,
        y: title_y + 2,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(4),
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, action) in actions.iter().enumerate() {
        let style = if i == app.action_picker_index {
            Style::default()
                .bg(app.theme.modal_selected_bg)
                .fg(app.theme.modal_selected_fg)
        } else {
            Style::default()
                .bg(bg)
                .fg(text_fg)
        };
        let text = format!("  {}", action.label);
        let padded = format!("{:<width$}", text, width = inner.width as usize);
        lines.push(Line::from(Span::styled(padded, style)));
    }

    // Blank line + hint
    let hint = "Enter Select  Esc Cancel";
    let hint_pad = (inner.width as usize).saturating_sub(hint.len()) / 2;
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{}{}", " ".repeat(hint_pad), hint),
        Style::default().fg(app.theme.field_readonly).bg(bg),
    )));

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(bg)),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Action form modal (Modal kind) — uses real Field rendering
// ---------------------------------------------------------------------------

pub fn draw_action_form_modal(frame: &mut Frame, app: &App) {
    let def = match &app.action_form_def {
        Some(d) => d,
        None => return,
    };

    // Height: border(2) + title(1) + gap(1) + fields (accounting for TextArea rows) + gap(1) + hint(1)
    let total_visual_rows: usize = app.action_form_fields.iter()
        .map(|f| if f.field_type == two_wee_shared::FieldType::TextArea {
            f.rows.unwrap_or(4) as usize
        } else {
            1
        })
        .sum();
    let h = (6 + total_visual_rows) as u16;
    let w: u16 = 46;

    let inner = draw_modal_frame(frame, app, w, h, &def.label);

    let max_label = app.action_form_fields.iter()
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(0);

    let input_width = inner.width
        .saturating_sub((max_label as u16) + 8)
        .max(10) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for (i, field) in app.action_form_fields.iter().enumerate() {
        let is_focused = i == app.action_form_field_index;
        let is_dirty = app.action_form_is_field_dirty(&field.id);
        let selection = if is_focused { app.action_form_selection_range() } else { None };

        // TextArea: render multiple rows
        if field.field_type == two_wee_shared::FieldType::TextArea {
            let row_count = field.rows.unwrap_or(4) as usize;
            let cursor_flat = if is_focused { app.action_form_cursor } else { 0 };
            let ta_lines = render_textarea(TextAreaField {
                label: field.label.as_str(),
                value: field.value.as_str(),
                max_label_width: max_label,
                input_width,
                focused: is_focused,
                mode: app.action_form_input_mode,
                cursor_flat,
                row_count,
                dirty: is_dirty,
                theme: &app.theme,
                selection,
            });
            for line in ta_lines {
                lines.push(line);
            }
            continue;
        }

        // Boolean: render as toggle dot, skip normal edit path
        if field.field_type == two_wee_shared::FieldType::Boolean {
            lines.push(render_boolean(BooleanField {
                label: field.label.as_str(),
                value: field.value.as_str(),
                true_label: field.true_label.as_deref(),
                false_label: field.false_label.as_deref(),
                true_color: field.true_color.as_deref(),
                false_color: field.false_color.as_deref(),
                max_label_width: max_label,
                input_width,
                focused: is_focused,
                dirty: is_dirty,
                theme: &app.theme,
            }));
            continue;
        }

        let is_editing = is_focused && app.action_form_input_mode == crate::app::HeaderInputMode::Edit;

        let display_value;
        let value_str = if is_editing {
            field.value.as_str()
        } else if field.field_type == two_wee_shared::FieldType::Option {
            display_value = match &field.options {
                Some(two_wee_shared::OptionValues::Labeled(pairs)) => {
                    pairs.iter()
                        .find(|p| p.value == field.value)
                        .map(|p| p.label.clone())
                        .unwrap_or_else(|| field.value.clone())
                }
                _ => field.value.clone(),
            };
            display_value.as_str()
        } else {
            field.value.as_str()
        };

        lines.push(render_field(InputField {
            label: field.label.as_str(),
            value: value_str,
            max_label_width: max_label,
            input_width,
            focused: is_focused,
            mode: app.action_form_input_mode,
            cursor: app.action_form_cursor,
            read_only: false,
            color: field.color.as_deref(),
            bold: field.bold,
            is_password: field.field_type == two_wee_shared::FieldType::Password,
            dirty: is_dirty,
            selection,
            theme: &app.theme,
        }));
    }

    // Hint (centered) — show textarea shortcut when editing a TextArea field
    let focused_is_textarea_edit = app.action_form_input_mode == crate::app::HeaderInputMode::Edit
        && app.action_form_fields.get(app.action_form_field_index)
            .map(|f| f.field_type == two_wee_shared::FieldType::TextArea)
            .unwrap_or(false);
    let textarea_hint_owned;
    let hint = if focused_is_textarea_edit {
        let field = app.action_form_fields.get(app.action_form_field_index).unwrap();
        let line_count = field.value.chars().filter(|&c| c == '\n').count() + 1;
        let max_lines = field.rows.unwrap_or(4) as usize;
        textarea_hint_owned = format!("{}/{}  Ctrl+Enter New line  Esc Cancel", line_count, max_lines);
        textarea_hint_owned.as_str()
    } else {
        "Enter Next  Esc Cancel"
    };
    let hint_pad = (inner.width as usize).saturating_sub(hint.len()) / 2;
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{}{}", " ".repeat(hint_pad), hint),
        Style::default().fg(app.theme.field_readonly).bg(app.theme.modal_bg),
    )));

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.theme.modal_bg)),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Action confirm modal (Confirm kind)
// ---------------------------------------------------------------------------

pub fn draw_action_confirm_modal(frame: &mut Frame, app: &App) {
    let def = match &app.action_confirm_def {
        Some(d) => d,
        None => return,
    };

    let message = def.confirm_message.as_deref().unwrap_or("Execute this action?");
    draw_confirm_dialog(
        frame, app,
        &def.label,
        message,
        &["Yes", "No"],
        app.action_confirm_index,
    );
}

// ---------------------------------------------------------------------------
// Action result modal (shows server response)
// ---------------------------------------------------------------------------

pub fn draw_action_result_modal(frame: &mut Frame, app: &App) {
    let title = if app.action_result_is_error { "Error" } else { "Done" };
    let message = &app.action_result_message;

    // Height: border(2) + title(1) + gap(1) + message(1) + gap(1) + hint(1) = 7
    let h: u16 = 7;
    let msg_len = message.chars().count() as u16;
    let title_len = title.chars().count() as u16;
    let content_w = msg_len.max(title_len) + 6;
    let max_w = frame.area().width.saturating_sub(4);
    let w = content_w.clamp(30, max_w);

    let inner = draw_modal_frame(frame, app, w, h, title);
    let bg = app.theme.modal_bg;

    let msg_pad = (inner.width as usize).saturating_sub(message.chars().count()) / 2;
    let msg_fg = if app.action_result_is_error {
        ratatui::style::Color::Red
    } else {
        app.theme.modal_text
    };

    let lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("{}{}", " ".repeat(msg_pad), message),
            Style::default().fg(msg_fg).bg(bg),
        )),
        Line::from(""),
        Line::from(Span::styled({
            let hint = "Enter OK";
            let pad = (inner.width as usize).saturating_sub(hint.len()) / 2;
            format!("{}{}", " ".repeat(pad), hint)
        }, Style::default().fg(app.theme.field_readonly).bg(bg))),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(bg)),
        inner,
    );
}

/// Draw the lookup modal overlay. Returns the visible row count (page size for PageUp/PageDown).
pub fn draw_lookup_modal(frame: &mut Frame, app: &App, filtered: &[usize]) -> usize {
    let data = match &app.lookup_modal_data {
        Some(d) => d,
        None => return 0,
    };

    let col_widths = &data.col_widths;
    let total_col_w: usize = col_widths.iter().sum::<usize>() + (col_widths.len().saturating_sub(1)) * 2;

    // Modal dimensions — wider by default (at least 50% of screen)
    let screen_w = frame.area().width;
    let screen_h = frame.area().height;
    let min_w = (screen_w * 2 / 5) as usize;
    let content_w = total_col_w + 4;
    let max_w = screen_w.saturating_sub(4) as usize;
    let w = content_w.max(min_w).clamp(30, max_w) as u16;
    // Height: min 50% of screen, max 80% so it always looks like a modal
    let min_h = (screen_h / 2).max(10);
    let max_h = screen_h * 4 / 5;
    // border(2) + blank(1) + header(1) + separator(1) + rows
    let h = min_h.max(5 + data.all_rows.len() as u16).min(max_h);

    let bg = app.theme.modal_bg;
    let border_fg = app.theme.modal_border;

    // Position and clear
    let modal_area = centered_fixed(w, h, frame.area());
    frame.render_widget(Clear, modal_area);

    // Draw border with title embedded in top-left
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(bg).fg(border_fg));
    frame.render_widget(block, modal_area);

    // Title in top border — " Title ─────"  (Navision style)
    let title_text = format!(" {} ", data.title);
    let title_len = title_text.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Span::styled(
            title_text,
            Style::default().fg(border_fg).bg(bg),
        )),
        Rect {
            x: modal_area.x + 1,
            y: modal_area.y,
            width: title_len.min(modal_area.width.saturating_sub(2)),
            height: 1,
        },
    );

    // Filter in bottom border — only shown when active
    if !app.lookup_modal_filter.is_empty() {
        let filter_text = format!(" Filter: {} ", app.lookup_modal_filter);
        let filter_len = filter_text.chars().count() as u16;
        frame.render_widget(
            Paragraph::new(Span::styled(
                filter_text,
                Style::default().fg(app.theme.field_readonly).bg(bg),
            )),
            Rect {
                x: modal_area.x + 1,
                y: modal_area.y + modal_area.height - 1,
                width: filter_len.min(modal_area.width.saturating_sub(2)),
                height: 1,
            },
        );
    }

    // Inner content area (1-char horizontal padding)
    let inner = Rect {
        x: modal_area.x + 2,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(4),
        height: modal_area.height.saturating_sub(2),
    };
    let inner_w = inner.width as usize;

    // Column headers (after one blank line for spacing from title border)
    let header_y = inner.y + 1;
    let header_line = build_lookup_row_line(&data.columns, &col_widths, None, true, app);
    frame.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(bg)),
        Rect { x: inner.x, y: header_y, width: inner.width, height: 1 },
    );

    // Separator
    let sep_y = header_y + 1;
    let sep = "─".repeat(inner_w);
    frame.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(border_fg).bg(bg))),
        Rect { x: inner.x, y: sep_y, width: inner.width, height: 1 },
    );

    // Data rows with scroll
    let rows_y = sep_y + 1;
    let rows_h = inner.height.saturating_sub(rows_y - inner.y) as usize;
    let scroll_offset = if app.lookup_modal_index >= rows_h {
        app.lookup_modal_index - rows_h + 1
    } else {
        0
    };

    for (vi, &row_idx) in filtered.iter().skip(scroll_offset).take(rows_h).enumerate() {
        let is_selected = scroll_offset + vi == app.lookup_modal_index;
        let row = &data.all_rows[row_idx];
        let style = if is_selected {
            Style::default().bg(app.theme.modal_selected_bg).fg(app.theme.modal_selected_fg)
        } else {
            Style::default().bg(bg).fg(app.theme.modal_text)
        };
        let line = build_lookup_row_line(&data.columns, &col_widths, Some(&row.values), false, app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        let padded = format!("{:<w$}", text, w = inner_w);
        frame.render_widget(
            Paragraph::new(Span::styled(padded, style)),
            Rect { x: inner.x, y: rows_y + vi as u16, width: inner.width, height: 1 },
        );
    }

    // Pagination status in bottom-right border
    let total_rows = filtered.len();
    if total_rows > 0 {
        let total_pages = (total_rows + rows_h.max(1) - 1) / rows_h.max(1);
        let current_page = scroll_offset / rows_h.max(1) + 1;
        let status = format!(" {}/{}  Page {} of {} ", app.lookup_modal_index + 1, total_rows, current_page, total_pages);
        let status_len = status.chars().count() as u16;
        let status_x = modal_area.x + modal_area.width.saturating_sub(status_len + 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                status,
                Style::default().fg(app.theme.field_readonly).bg(bg),
            )),
            Rect {
                x: status_x,
                y: modal_area.y + modal_area.height - 1,
                width: status_len.min(modal_area.width.saturating_sub(2)),
                height: 1,
            },
        );
    }

    rows_h
}

fn build_lookup_row_line<'a>(
    columns: &[two_wee_shared::ColumnDef],
    col_widths: &[usize],
    values: Option<&[String]>,
    is_header: bool,
    app: &App,
) -> Line<'a> {
    let mut parts: Vec<Span<'a>> = vec![Span::raw(" ")];
    for (ci, col) in columns.iter().enumerate() {
        let w = col_widths[ci];
        let text = if is_header {
            &col.label
        } else {
            values.and_then(|v| v.get(ci)).map(|s| s.as_str()).unwrap_or("")
        };
        let formatted = if text.chars().count() > w {
            text.chars().take(w).collect::<String>()
        } else {
            format!("{:<w$}", text, w = w)
        };
        if ci > 0 {
            parts.push(Span::raw("  "));
        }
        let style = if is_header {
            Style::default().fg(app.theme.table_header_fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.modal_text)
        };
        parts.push(Span::styled(formatted, style));
    }
    Line::from(parts)
}

