use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use two_wee_shared::{FieldType, ScreenContract};

use crate::app::App;
use crate::number_format;
use crate::shortcuts::Action;
use crate::ui::widgets::bottom_bar::draw_bottom_bar;
use crate::ui::widgets::field::{InputField, TextAreaField, BooleanField, render_field, render_boolean, render_textarea, flat_cursor_to_row_col, draw_option_dropdown, draw_grid_option_dropdown};
use crate::ui::widgets::grid::draw_grid;

pub fn draw_card(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if app.current_screen.is_none() {
        let t = &app.theme;
        frame.render_widget(
            Paragraph::new("Loading...")
                .style(Style::default().fg(t.text).bg(t.content_bg)),
            area,
        );
        return;
    }

    // Login screens get a centered compact layout
    if app.is_auth_screen() {
        let screen = app.current_screen.as_ref().unwrap();
        draw_login_screen(frame, area, screen, app);
        return;
    }

    // Lines overlay active — split into compressed header + lines table
    if app.lines_overlay_open {
        draw_card_with_lines_overlay(frame, area, app);
        return;
    }

    // Immutable borrow scope — render card body, title, bottom bar
    let focused_rect = {
        let t = &app.theme;
        let screen = app.current_screen.as_ref().unwrap();

        // Title bar
        let dirty_mark = if app.is_card_dirty() { " *" } else { "" };
        frame.render_widget(
            Paragraph::new(format!(" {}{}", screen.title, dirty_mark))
                .style(Style::default().bg(t.bar_bg).fg(t.bar_text)),
            Rect { x: area.x, y: area.y, width: area.width, height: 1 },
        );

        // Card body
        let body_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(2),
        };
        let rect = draw_card_sections(frame, body_area, screen, app);

        // Bottom bar — message left, hotkeys right
        let bottom_rect = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        let mut card_actions = vec![
            Action::NewCard, Action::Delete, Action::Save,
        ];
        if app.has_lines() {
            card_actions.push(Action::OpenLines);
        }
        if app.has_screen_actions() {
            card_actions.push(Action::ActionPicker);
        }
        card_actions.push(Action::Escape);
        let hints = app.shortcuts.format_hints(&card_actions);
        let context_hint;
        let (left_text, left_highlight) = if !app.message.is_empty() {
            (Some(app.message.as_str()), true)
        } else {
            context_hint = app.field_context_hint();
            (context_hint.as_deref(), false)
        };
        draw_bottom_bar(frame, bottom_rect, t, left_text, left_highlight, app.message_is_error, &hints);

        rect
    };
    // Immutable borrows released — now safe to mutate app
    app.focused_field_rect = focused_rect;

    // Option dropdown overlay
    if app.option_modal_open {
        draw_option_dropdown(frame, area, app);
    }
}

/// Draw the card with the lines grid overlay on top.
/// The card is rendered full-screen first, then the grid is drawn over the bottom portion.
fn draw_card_with_lines_overlay(frame: &mut Frame, area: Rect, app: &mut App) {
    let t = &app.theme;
    let screen = app.current_screen.as_ref().unwrap();

    // Title bar
    let dirty_mark = if app.is_card_dirty() || app.grid_state.is_dirty() { " *" } else { "" };
    frame.render_widget(
        Paragraph::new(format!(" {}{}", screen.title, dirty_mark))
            .style(Style::default().bg(t.bar_bg).fg(t.bar_text)),
        Rect { x: area.x, y: area.y, width: area.width, height: 1 },
    );

    // Render the full card body behind the overlay
    let full_body = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(2),
    };
    draw_card_sections(frame, full_body, screen, app);

    // Overlay: grid drawn ON TOP of the card's lower portion
    let body_height = area.height.saturating_sub(2); // minus title + bottom bar
    let pct = screen.lines_overlay_pct.clamp(20, 90) as u16;
    let overlay_height = (body_height * pct / 100).max(5);

    let overlay_area = Rect {
        x: area.x,
        y: area.y + 1 + body_height.saturating_sub(overlay_height),
        width: area.width,
        height: overlay_height,
    };

    if let Some(ref lines) = screen.lines {
        draw_grid(frame, overlay_area, lines, &mut app.grid_state, &app.theme,
            &app.locale.decimal_separator, &app.locale.thousand_separator);
    }

    // Grid option dropdown (drawn on top of grid)
    if app.option_modal_open && app.lines_overlay_open {
        draw_grid_option_dropdown(frame, overlay_area, app);
    }

    // Bottom bar
    let bottom_rect = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let hints = app.shortcuts.format_hints(&[
        Action::Save, Action::Escape, Action::InsertRow, Action::DeleteRow,
    ]);
    let context_hint;
    let dirty_hint;
    let (left_text, left_highlight) = if !app.message.is_empty() {
        (Some(app.message.as_str()), true)
    } else if app.is_card_dirty() {
        dirty_hint = match app.grid_field_context_hint() {
            Some(h) => format!("[Modified] · {}", h),
            None => "[Modified]".to_string(),
        };
        (Some(dirty_hint.as_str()), true)
    } else {
        context_hint = app.grid_field_context_hint();
        (context_hint.as_deref(), false)
    };
    draw_bottom_bar(frame, bottom_rect, t, left_text, left_highlight, app.message_is_error, &hints);

    app.focused_field_rect = None;
}

fn draw_login_screen(frame: &mut Frame, area: Rect, screen: &ScreenContract, app: &App) {
    let t = &app.theme;

    // Fill the entire screen with the desktop background
    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(t.desktop)),
        area,
    );

    // Top bar
    frame.render_widget(
        Paragraph::new(" ".repeat(area.width as usize))
            .style(Style::default().bg(t.bar_bg).fg(t.bar_text)),
        Rect { x: area.x, y: area.y, width: area.width, height: 1 },
    );

    // Bottom bar
    let bottom_y = area.y + area.height.saturating_sub(1);
    frame.render_widget(
        Paragraph::new(" ".repeat(area.width as usize))
            .style(Style::default().bg(t.bar_bg).fg(t.bar_text)),
        Rect { x: area.x, y: bottom_y, width: area.width, height: 1 },
    );

    // Compute field dimensions
    let max_label = screen
        .sections
        .iter()
        .flat_map(|s| s.fields.iter())
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(0);

    let input_width: usize = 28;
    let field_count = screen
        .sections
        .iter()
        .map(|s| s.fields.len())
        .sum::<usize>() as u16;

    // Box sizing: fields get equal padding above and below.
    // inner_h = field_count + 2*padding, where padding = field_count (so fields use ~1/3 of inner height)
    let padding = field_count;
    let inner_h = field_count + padding * 2;
    let box_h = inner_h + 2; // +2 for border
    let content_w = (1 + max_label + 3 + input_width) as u16;
    let box_w = (content_w + 6).clamp(48, area.width.saturating_sub(8)); // +6 for border + inner padding

    // Usable area (between top/bottom bars)
    let usable_y = area.y + 1;
    let usable_h = area.height.saturating_sub(2);

    // Center the box
    let x = area.x + area.width.saturating_sub(box_w) / 2;
    let y = usable_y + usable_h.saturating_sub(box_h) / 2;
    let box_area = Rect::new(x, y, box_w, box_h.min(usable_h));

    // Draw the border with title
    let block = Block::default()
        .title(format!(" {} ", screen.title))
        .borders(Borders::ALL)
        .style(Style::default().bg(t.desktop).fg(t.card_border));
    frame.render_widget(block, box_area);

    // Inner area (inside border, with horizontal padding)
    let inner = Rect {
        x: box_area.x + 3,
        y: box_area.y + 1,
        width: box_area.width.saturating_sub(6),
        height: box_area.height.saturating_sub(2),
    };

    // Build field lines
    let mut field_lines: Vec<Line> = Vec::new();
    let mut field_idx: usize = 0;
    for section in &screen.sections {
        for field in &section.fields {
            let is_focused = field_idx == app.card_field_index;
            let is_dirty = app.is_field_dirty(&field.id);
            let selection = if is_focused { app.selection_range() } else { None };
            let iw = inner.width.saturating_sub((max_label as u16) + 5).clamp(10, 32) as usize;

            field_lines.push(render_field(InputField {
                label: field.label.as_str(),
                value: field.value.as_str(),
                max_label_width: max_label,
                input_width: iw,
                focused: is_focused,
                mode: app.header_input_mode,
                cursor: app.header_cursor,
                read_only: !field.editable,
                color: field.color.as_deref(),
                bold: field.bold,
                is_password: field.field_type == FieldType::Password,
                dirty: is_dirty,
                selection,
                theme: t,
            }));
            field_idx += 1;
        }
    }

    // Place fields with equal padding above and below
    let top_pad = inner.height.saturating_sub(field_count) / 2;
    let fields_area = Rect {
        x: inner.x,
        y: inner.y + top_pad,
        width: inner.width,
        height: field_count.min(inner.height),
    };

    frame.render_widget(
        Paragraph::new(field_lines).style(Style::default().bg(t.desktop)),
        fields_area,
    );

    // Form validation error inside the box, below the fields, centered
    if !app.form_error.is_empty() {
        let msg_y = fields_area.y + field_count + 1;
        if msg_y < box_area.y + box_area.height.saturating_sub(1) {
            frame.render_widget(
                Paragraph::new(app.form_error.as_str())
                    .alignment(ratatui::layout::Alignment::Center)
                    .style(Style::default().bg(t.desktop).fg(t.form_error_text)
                        .add_modifier(Modifier::BOLD)),
                Rect { x: inner.x, y: msg_y, width: inner.width, height: 1 },
            );
        }
    }
}

fn draw_card_sections(frame: &mut Frame, area: Rect, screen: &ScreenContract, app: &App) -> Option<(u16, u16, u16)> {
    let t = &app.theme;

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(t.content_bg).fg(t.card_border));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let max_row_group = screen.sections.iter().map(|s| s.row_group).max().unwrap_or(0);
    // For each row_group, compute the minimum height needed (border + fields).
    // TextArea fields count their `rows` instead of 1 row each.
    let row_constraints: Vec<Constraint> = (0..=max_row_group)
        .map(|rg| {
            // Sum field heights per section, then take the max across sections in this row_group
            let field_rows: u16 = screen
                .sections
                .iter()
                .filter(|s| s.row_group == rg)
                .map(|s| s.fields.iter().map(field_visual_rows).sum::<u16>())
                .max()
                .unwrap_or(1);
            // +2 for section border top/bottom, +2 for padding
            Constraint::Length(field_rows + 4)
        })
        .collect();
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    let max_label = screen
        .sections
        .iter()
        .flat_map(|s| s.fields.iter())
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(0);

    let focused_fref = app.card_fields_flat.get(app.card_field_index);
    let mut focused_rect: Option<(u16, u16, u16)> = None;

    for rg in 0..=max_row_group {
        let rg_sections: Vec<(usize, &_)> = screen.sections.iter().enumerate().filter(|(_, s)| s.row_group == rg).collect();
        if rg_sections.is_empty() {
            continue;
        }

        let row_area = row_areas[rg as usize];

        let cols_in_group = rg_sections.iter().map(|(_, s)| s.column).max().unwrap_or(0) + 1;
        let col_constraints: Vec<Constraint> = (0..cols_in_group)
            .map(|_| Constraint::Ratio(1, cols_in_group as u32))
            .collect();
        let col_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for &(si, section) in &rg_sections {
            let col_area = col_areas[section.column as usize];

            let section_block = Block::default()
                .title(format!(" {} ", section.label))
                .borders(Borders::ALL)
                .style(Style::default().bg(t.content_bg).fg(t.card_border));
            frame.render_widget(section_block, col_area);

            let section_inner = Rect {
                x: col_area.x + 1,
                y: col_area.y + 1,
                width: col_area.width.saturating_sub(2),
                height: col_area.height.saturating_sub(2),
            };

            let mut lines: Vec<Line> = Vec::new();
            let mut visual_row: u16 = 0;
            for (fi, field) in section.fields.iter().enumerate() {
                // Separator renders as a blank line (not navigable)
                if field.field_type == FieldType::Separator {
                    lines.push(Line::from(""));
                    visual_row += 1;
                    continue;
                }

                let is_focused = focused_fref
                    .map(|r| r.section_idx == si && r.field_idx == fi)
                    .unwrap_or(false);
                if is_focused {
                    let input_w = section_inner
                        .width
                        .saturating_sub((max_label as u16) + 8) // +6 left + 2 right padding
                        .max(10);
                    let value_x = section_inner.x + (max_label as u16) + 6;
                    let value_y = section_inner.y + visual_row;
                    focused_rect = Some((value_x, value_y, input_w));
                }
                let input_width = section_inner
                    .width
                    .saturating_sub((max_label as u16) + 8) // +6 left + 2 right padding
                    .max(10) as usize;

                // TextArea: render multiple rows
                if field.field_type == FieldType::TextArea {
                    let row_count = field.rows.unwrap_or(4) as usize;
                    let cursor_flat = if is_focused { app.header_cursor } else { 0 };
                    let is_dirty = app.is_field_dirty(&field.id);
                    let ta_lines = render_textarea(TextAreaField {
                        label: field.label.as_str(),
                        value: field.value.as_str(),
                        max_label_width: max_label,
                        input_width,
                        focused: is_focused,
                        mode: app.header_input_mode,
                        cursor_flat,
                        row_count,
                        dirty: is_dirty,
                        theme: t,
                        selection: if is_focused { app.selection_range() } else { None },
                    });
                    lines.extend(ta_lines);
                    // Update focused_rect to point at the cursor row (accounts for scroll)
                    if is_focused {
                        let (cursor_row, _) = flat_cursor_to_row_col(field.value.as_str(), cursor_flat);
                        let scroll_offset = if cursor_row >= row_count { cursor_row - (row_count - 1) } else { 0 };
                        let visible_cursor = cursor_row.saturating_sub(scroll_offset) as u16;
                        let input_w = section_inner.width.saturating_sub((max_label as u16) + 8).max(10);
                        focused_rect = Some((
                            section_inner.x + (max_label as u16) + 6,
                            section_inner.y + visual_row + visible_cursor,
                            input_w,
                        ));
                    }
                    visual_row += row_count as u16;
                    continue; // skip the normal single-line render below
                }

                let is_dirty = app.is_field_dirty(&field.id);

                // Boolean: rendered as a toggle dot, no edit mode
                if field.field_type == FieldType::Boolean {
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
                        theme: t,
                    }));
                    visual_row += 1;
                    continue;
                }

                let selection = if is_focused { app.selection_range() } else { None };
                let is_editing = is_focused && app.header_input_mode == crate::app::HeaderInputMode::Edit;

                let display_value;
                let value_str = if is_editing {
                    field.value.as_str()
                } else if is_focused && field.field_type == FieldType::Option {
                    display_value = app.current_option_display();
                    display_value.as_str()
                } else if field.field_type == FieldType::Option {
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
                } else if field.field_type == FieldType::Decimal {
                    display_value = number_format::format_decimal(
                        &field.value,
                        &app.locale.decimal_separator,
                        &app.locale.thousand_separator,
                        None,
                    );
                    display_value.as_str()
                } else if field.field_type == FieldType::Integer {
                    display_value = number_format::format_integer(
                        &field.value,
                        &app.locale.thousand_separator,
                    );
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
                    mode: app.header_input_mode,
                    cursor: app.header_cursor,
                    read_only: !field.editable,
                    color: field.color.as_deref(),
                    bold: field.bold,
                    is_password: field.field_type == FieldType::Password,
                    dirty: is_dirty,
                    selection,
                    theme: t,
                }));
                visual_row += 1;
            }

            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(t.content_bg)),
                section_inner,
            );

            // Overflow indicator: if not all fields fit, replace the bottom border line
            if visual_row > section_inner.height {
                let hidden = count_hidden_fields(section, section_inner.height, visual_row);
                if hidden > 0 {
                    let label = format!(" ▼ {} fields not shown ", hidden);
                    frame.render_widget(
                        Paragraph::new(label)
                            .alignment(Alignment::Center)
                            .style(Style::default().bg(t.content_bg).fg(t.card_border)),
                        Rect {
                            x: col_area.x + 1,
                            y: col_area.y + col_area.height.saturating_sub(1),
                            width: col_area.width.saturating_sub(2),
                            height: 1,
                        },
                    );
                }
            }
        }
    }
    focused_rect
}

/// Visual row height of a single field (TextArea uses its configured rows, all others use 1).
fn field_visual_rows(field: &two_wee_shared::Field) -> u16 {
    if field.field_type == FieldType::TextArea {
        field.rows.unwrap_or(4) as u16
    } else {
        1
    }
}

/// Count how many fields in a section are not visible given the available height.
fn count_hidden_fields(section: &two_wee_shared::Section, available_rows: u16, total_rows: u16) -> usize {
    if total_rows <= available_rows {
        return 0;
    }
    // Walk fields from the end, accumulating row heights, until we exceed the overflow
    let overflow = total_rows - available_rows;
    let mut hidden_rows: u16 = 0;
    let mut hidden_count: usize = 0;
    for field in section.fields.iter().rev() {
        if field.field_type == FieldType::Separator {
            continue;
        }
        hidden_rows += field_visual_rows(field);
        hidden_count += 1;
        if hidden_rows >= overflow {
            break;
        }
    }
    hidden_count
}

