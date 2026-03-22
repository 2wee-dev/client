use ratatui::{
    layout::Rect,
    style::Style,
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::shortcuts::Action;
use crate::ui::widgets::bottom_bar::draw_bottom_bar;
use crate::ui::widgets::table::draw_table;

pub fn draw_list(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let screen = match &app.current_screen {
        Some(s) => s,
        None => {
            frame.render_widget(
                Paragraph::new("Loading...")
                    .style(Style::default().fg(app.theme.text).bg(app.theme.content_bg)),
                area,
            );
            return;
        }
    };

    let lines = match &screen.lines {
        Some(l) => l,
        None => return,
    };

    // Title bar
    let title_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(format!(" {}", screen.title))
            .style(Style::default().bg(app.theme.bar_bg).fg(app.theme.bar_text)),
        title_area,
    );

    // Table area
    let table_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(2),
    };

    draw_table(frame, table_area, lines, &mut app.table_state, &app.theme, None);

    // Empty state message (e.g. "No customers matching '...'")
    if lines.rows.is_empty() && !app.message.is_empty() {
        let data_top = table_area.y + 3; // below header + separator
        let data_height = table_area.height.saturating_sub(3);
        let msg_y = data_top + data_height / 3; // upper third of empty area
        if msg_y < table_area.y + table_area.height {
            frame.render_widget(
                Paragraph::new(app.message.as_str())
                    .alignment(ratatui::layout::Alignment::Center)
                    .style(Style::default().fg(app.theme.field_readonly).bg(app.theme.content_bg)),
                Rect {
                    x: table_area.x + 1,
                    y: msg_y,
                    width: table_area.width.saturating_sub(2),
                    height: 1,
                },
            );
        }
    }

    // Bottom bar — message/search left, hotkeys right
    let bottom_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let has_search = !app.list_search_query.is_empty();
    let is_lookup = app.current_screen.as_ref()
        .and_then(|s| s.lines.as_ref())
        .and_then(|l| l.value_column.as_ref())
        .is_some();
    let has_drill = app.current_screen.as_ref()
        .and_then(|s| s.lines.as_ref())
        .and_then(|l| l.on_drill.as_ref())
        .is_some();

    let mut list_actions = Vec::new();
    if !is_lookup {
        list_actions.push(Action::NewCard);
    }
    list_actions.push(Action::ListEnter);
    if has_drill && is_lookup {
        list_actions.push(Action::DrillDown);
    }
    if app.has_screen_actions() {
        list_actions.push(Action::ActionPicker);
    }
    list_actions.push(Action::Escape);

    let hints = if is_lookup {
        let h = app.shortcuts.format_hints_with_override(&list_actions, Action::ListEnter, "Select");
        if has_search {
            h // on lookups, Esc label stays "Close"
        } else {
            h
        }
    } else if has_search {
        app.shortcuts.format_hints_with_override(&list_actions, Action::Escape, "Clear")
    } else {
        app.shortcuts.format_hints(&list_actions)
    };

    let search_text;
    let left_text: Option<&str> = if has_search {
        search_text = format!("Search: {}█", app.list_search_query);
        Some(&search_text)
    } else if !app.message.is_empty() {
        Some(app.message.as_str())
    } else {
        None
    };

    draw_bottom_bar(frame, bottom_area, &app.theme, left_text, true, false, &hints);
}
