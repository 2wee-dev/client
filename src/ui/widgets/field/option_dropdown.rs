use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Clear, List, ListItem},
    Frame,
};

use crate::app::App;

/// Draw an option dropdown anchored to a given position.
///
/// Shared renderer used by both card fields and grid cells.
fn draw_dropdown_at(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    options: &[String],
    selected_index: usize,
    anchor: (u16, u16, u16), // (x, y, width)
) {
    let t = &app.theme;
    if options.is_empty() {
        return;
    }

    let (field_x, field_y, field_w) = anchor;

    let popup_w = field_w;
    let max_visible = 8u16;
    let item_count = options.len() as u16;
    let popup_h = item_count.min(max_visible);

    // Place below the anchor; flip above if not enough space
    let below_y = field_y + 1;
    let space_below = area.y + area.height - below_y;
    let space_above = field_y.saturating_sub(area.y);
    let (popup_y, visible_h) = if space_below >= popup_h {
        (below_y, popup_h)
    } else if space_above >= popup_h {
        (field_y.saturating_sub(popup_h), popup_h)
    } else if space_below >= space_above {
        (below_y, space_below.min(popup_h))
    } else {
        (field_y.saturating_sub(space_above.min(popup_h)), space_above.min(popup_h))
    };

    let popup_x = field_x.min(area.x + area.width.saturating_sub(popup_w));
    let popup_area = Rect::new(popup_x, popup_y, popup_w, visible_h);
    frame.render_widget(Clear, popup_area);

    // Scroll so the selected item is visible
    let scroll_offset = if selected_index >= visible_h as usize {
        selected_index - visible_h as usize + 1
    } else {
        0
    };

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_h as usize)
        .map(|(i, opt)| {
            let style = if i == selected_index {
                Style::default().bg(t.modal_selected_bg).fg(t.modal_selected_fg)
            } else {
                Style::default().fg(t.field_value).bg(t.field_editing_bg)
            };
            let char_len = opt.chars().count();
            let padded: String = if char_len < popup_w as usize {
                format!("{}{}", opt, " ".repeat(popup_w as usize - char_len))
            } else {
                opt.chars().take(popup_w as usize).collect()
            };
            ListItem::new(padded).style(style)
        })
        .collect();

    let list = List::new(items)
        .style(Style::default().bg(t.field_editing_bg));

    frame.render_widget(list, popup_area);
}

/// Draw an option dropdown for a card field.
pub fn draw_option_dropdown(frame: &mut Frame, area: Rect, app: &App) {
    let options = app.current_option_labels();
    let anchor = app.focused_field_rect.unwrap_or((
        area.x + area.width / 4,
        area.y + area.height / 2,
        30,
    ));
    draw_dropdown_at(frame, area, app, &options, app.option_modal_index, anchor);
}

/// Draw an option dropdown for a grid cell.
pub fn draw_grid_option_dropdown(frame: &mut Frame, area: Rect, app: &App) {
    let options = app.grid_option_labels();
    let anchor = app.grid_state.focused_cell_rect.unwrap_or((
        area.x + area.width / 4,
        area.y + area.height / 2,
        14,
    ));
    draw_dropdown_at(frame, area, app, &options, app.option_modal_index, anchor);
}
