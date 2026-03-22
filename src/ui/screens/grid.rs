use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::shortcuts::Action;
use crate::ui::widgets::bottom_bar::draw_bottom_bar;
use crate::ui::widgets::field::draw_grid_option_dropdown;
use crate::ui::widgets::grid::draw_grid;

pub fn draw_grid_screen(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if app.current_screen.is_none() {
        let t = &app.theme;
        frame.render_widget(
            Paragraph::new("Loading...")
                .style(Style::default().fg(t.text).bg(t.grid_bg)),
            area,
        );
        return;
    }

    let t = &app.theme;
    let screen = app.current_screen.as_ref().unwrap();
    let has_totals = !screen.totals.is_empty();
    let footer_rows: u16 = if has_totals { 2 } else { 1 }; // totals row + bottom bar

    // Title bar
    let dirty_mark = if app.grid_state.is_dirty() { " *" } else { "" };
    frame.render_widget(
        Paragraph::new(format!(" {}{}", screen.title, dirty_mark))
            .style(Style::default().bg(t.bar_bg).fg(t.bar_text)),
        Rect { x: area.x, y: area.y, width: area.width, height: 1 },
    );

    // Grid — fills the space between title and footer
    let grid_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1 + footer_rows),
    };

    if let Some(ref lines) = screen.lines {
        draw_grid(frame, grid_area, lines, &mut app.grid_state, &app.theme,
            &app.locale.decimal_separator, &app.locale.thousand_separator);
    }

    // Grid option dropdown
    if app.option_modal_open {
        draw_grid_option_dropdown(frame, grid_area, app);
    }

    // Totals row — right-aligned label:value pairs
    if has_totals {
        let totals_y = area.y + area.height.saturating_sub(2);
        let totals_rect = Rect {
            x: area.x,
            y: totals_y,
            width: area.width,
            height: 1,
        };

        let mut spans: Vec<Span> = Vec::new();
        for (i, total) in screen.totals.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("   ", Style::default().fg(t.text).bg(t.grid_bg)));
            }
            spans.push(Span::styled(
                &total.label,
                Style::default().fg(t.text).bg(t.grid_bg),
            ));
            spans.push(Span::styled(
                " ",
                Style::default().fg(t.text).bg(t.grid_bg),
            ));
            spans.push(Span::styled(
                &total.value,
                Style::default().fg(t.text).bg(t.grid_bg),
            ));
        }

        // Trailing space so the last value aligns with the grid column edge
        spans.push(Span::styled(" ", Style::default().fg(t.text).bg(t.grid_bg)));

        let line = Line::from(spans);
        frame.render_widget(
            Paragraph::new(line)
                .alignment(ratatui::layout::Alignment::Right)
                .style(Style::default().bg(t.grid_bg)),
            totals_rect,
        );
    }

    // Bottom bar
    let bottom_rect = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let mut grid_actions = vec![
        Action::Save, Action::InsertRow, Action::DeleteRow,
    ];
    if app.has_screen_actions() {
        grid_actions.push(Action::ActionPicker);
    }
    grid_actions.push(Action::Escape);
    let hints = app.shortcuts.format_hints(&grid_actions);
    let status_hint;
    let (left_text, left_highlight) = if !app.message.is_empty() {
        (Some(app.message.as_str()), true)
    } else {
        status_hint = screen.status.as_deref();
        (status_hint, false)
    };
    draw_bottom_bar(frame, bottom_rect, t, left_text, left_highlight, app.message_is_error, &hints);
}
