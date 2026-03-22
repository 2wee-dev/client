use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::shortcuts::Action;
use crate::ui::layout_profile::main_menu_layout;
use crate::ui::widgets::bottom_bar::draw_bottom_bar;
use two_wee_shared::MenuActionDef;

pub fn draw_server_menu(frame: &mut Frame, app: &App) {
    let screen = match &app.current_screen {
        Some(s) => s,
        None => return,
    };
    let menu = match &screen.menu {
        Some(m) => m,
        None => return,
    };
    let tabs = &menu.tabs;
    if tabs.is_empty() {
        return;
    }

    let full = frame.area();
    let layout = main_menu_layout(full);

    // Top bar
    frame.render_widget(
        Paragraph::new(" ".repeat(layout.top_bar.width as usize))
            .style(Style::default().bg(app.theme.bar_bg).fg(app.theme.bar_text)),
        layout.top_bar,
    );

    // Top left text
    if let Some(ref top_left) = menu.top_left {
        frame.render_widget(
            Paragraph::new(format!(" {}", top_left))
                .style(Style::default().bg(app.theme.bar_bg).fg(app.theme.bar_text)),
            Rect {
                x: full.x,
                y: full.y,
                width: (top_left.len() as u16 + 2).min(full.width),
                height: 1,
            },
        );
    }

    // Top right text — prefer user display name, fall back to server-provided top_right
    let top_right_text: Option<String> = app
        .user_display_name
        .clone()
        .or_else(|| menu.top_right.clone());

    if let Some(ref top_right) = top_right_text {
        let right_x = full
            .x
            .saturating_add(full.width.saturating_sub(top_right.len() as u16 + 1));
        frame.render_widget(
            Paragraph::new(top_right.as_str())
                .style(Style::default().bg(app.theme.bar_bg).fg(app.theme.bar_text)),
            Rect {
                x: right_x,
                y: full.y,
                width: top_right.len() as u16,
                height: 1,
            },
        );
    }

    // Second line (blank with background)
    frame.render_widget(
        Paragraph::new(" ".repeat(layout.top_menu_line.width as usize))
            .style(Style::default().bg(app.theme.desktop).fg(app.theme.text)),
        layout.top_menu_line,
    );

    // Bottom bar — message left, hotkeys right
    let hints = app.shortcuts.format_hints(&[Action::Escape]);
    let left_text = if app.message.is_empty() { None } else { Some(app.message.as_str()) };
    let bottom_bar_area = Rect {
        x: full.x,
        y: layout.bottom_bar.y,
        width: full.width,
        height: 1,
    };
    draw_bottom_bar(frame, bottom_bar_area, &app.theme, left_text, true, false, &hints);

    // Grid area — cap column widths and center
    let layout_grid = layout.grid;
    let cols_area_layout = layout.cols_area;
    let layout_panel = layout.panel;

    let col_count = tabs.len() as u16;
    let inner_avail = layout_grid.width.saturating_sub(2); // inside border
    let max_col_width: u16 = 28;
    let col_width = (inner_avail / col_count.max(1)).min(max_col_width);
    let total_grid_width = col_width * col_count + 2; // +2 for border
    let grid_x = layout_grid.x + (layout_grid.width.saturating_sub(total_grid_width)) / 2;

    // Shrink the panel border to wrap the centered grid (with padding)
    let panel_padding: u16 = 4; // 2 chars each side
    let panel_width = (total_grid_width + panel_padding * 2).min(layout_panel.width);
    let panel_x = layout_panel.x + (layout_panel.width.saturating_sub(panel_width)) / 2;
    let panel = Rect {
        x: panel_x,
        y: layout_panel.y,
        width: panel_width,
        height: layout_panel.height,
    };

    // Panel border
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(app.theme.desktop).fg(app.theme.bar_bg)),
        panel,
    );

    // Panel title
    let title = menu.panel_title.as_str();
    let title_x = panel
        .x
        .saturating_add(panel.width.saturating_sub(title.len() as u16) / 2);
    frame.render_widget(
        Paragraph::new(title).style(
            Style::default()
                .bg(app.theme.desktop)
                .fg(app.theme.card_title)
                .add_modifier(Modifier::BOLD),
        ),
        Rect {
            x: title_x,
            y: panel.y.saturating_add(1),
            width: title.len() as u16,
            height: 1,
        },
    );

    let grid = Rect {
        x: grid_x,
        y: layout_grid.y,
        width: total_grid_width,
        height: layout_grid.height,
    };
    let cols_area = Rect {
        x: grid_x,
        y: cols_area_layout.y,
        width: total_grid_width,
        height: 1,
    };

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(app.theme.desktop).fg(app.theme.menu_grid)),
        grid,
    );

    let inner_left = grid.x.saturating_add(1);

    // Collect popup anchor during item rendering so we can draw it last (on top)
    let mut popup_anchor: Option<(u16, u16)> = None; // (x, y) of the anchor item row

    // Render tabs and items
    for (tab_i, tab) in tabs.iter().enumerate() {
        let x = inner_left + tab_i as u16 * col_width;
        let width = col_width;

        // Tab header
        frame.render_widget(
            Paragraph::new(format!(" {}", tab.label)).style(
                Style::default()
                    .bg(app.theme.tab_header_bg)
                    .fg(app.theme.tab_header_text)
                    .add_modifier(Modifier::BOLD),
            ),
            Rect {
                x,
                y: cols_area.y,
                width,
                height: 1,
            },
        );

        // Menu items
        let selected_idx = app
            .server_menu_selected
            .get(tab_i)
            .copied()
            .unwrap_or(0);

        for (row_idx, item) in tab.items.iter().enumerate() {
            let y = grid.y.saturating_add(1 + row_idx as u16);
            if y >= grid.y.saturating_add(grid.height.saturating_sub(1)) {
                break;
            }

            let is_separator = matches!(item.action, MenuActionDef::Separator);

            if is_separator {
                frame.render_widget(
                    Paragraph::new("").style(Style::default().bg(app.theme.desktop)),
                    Rect { x, y, width: width.saturating_sub(1), height: 1 },
                );
                continue;
            }

            let is_selected = tab_i == app.server_menu_tab && row_idx == selected_idx;

            if is_selected && app.menu_popup_open
                && matches!(item.action, MenuActionDef::Popup { .. })
            {
                popup_anchor = Some((x, y));
            }

            let style = if is_selected {
                Style::default()
                    .bg(app.theme.menu_selected_bg)
                    .fg(app.theme.menu_selected_text)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(app.theme.desktop)
                    .fg(app.theme.text)
            };

            frame.render_widget(
                Paragraph::new(format!(" {}", item.label)).style(style),
                Rect { x, y, width: width.saturating_sub(1), height: 1 },
            );
        }

    }

    // Column separators — drawn before popup so popup paints on top
    for i in 1..col_count {
        let x = inner_left + i * col_width;
        let sep_rows = grid.height.saturating_sub(2) as usize;
        let mut sep = String::with_capacity(sep_rows * 4); // '│' = 3 bytes + '\n'
        for _ in 0..sep_rows {
            sep.push('│');
            sep.push('\n');
        }
        frame.render_widget(
            Paragraph::new("┬")
                .style(Style::default().bg(app.theme.desktop).fg(app.theme.menu_grid)),
            Rect {
                x,
                y: grid.y,
                width: 1,
                height: 1,
            },
        );
        frame.render_widget(
            Paragraph::new(sep)
                .style(Style::default().bg(app.theme.desktop).fg(app.theme.menu_grid)),
            Rect {
                x,
                y: grid.y.saturating_add(1),
                width: 1,
                height: grid.height.saturating_sub(2),
            },
        );
        frame.render_widget(
            Paragraph::new("┴")
                .style(Style::default().bg(app.theme.desktop).fg(app.theme.menu_grid)),
            Rect {
                x,
                y: grid.y.saturating_add(grid.height.saturating_sub(1)),
                width: 1,
                height: 1,
            },
        );
    }

    // Draw popup last so it paints over separators and neighboring columns
    if app.menu_popup_open {
        if let Some((anchor_x, anchor_y)) = popup_anchor {
            let items = &app.menu_popup_items;

            // Wider than the column — extends past both edges for a clear overlay feel
            let min_inner = col_width + 4;
            let popup_inner_width = items.iter()
                .map(|i| i.label.len() + 2)
                .max()
                .unwrap_or(0)
                .max(min_inner as usize) as u16;
            let popup_width = popup_inner_width + 2;
            let popup_height = items.len() as u16 + 2;

            // Centre over the column, float upward from anchor row
            let col_centre = anchor_x + col_width / 2;
            let popup_x = col_centre.saturating_sub(popup_width / 2)
                .max(full.x + 1)
                .min(full.x + full.width.saturating_sub(popup_width + 1));
            let popup_y = anchor_y.saturating_sub(popup_height.saturating_sub(1))
                .max(grid.y + 1);

            let popup_rect = Rect { x: popup_x, y: popup_y, width: popup_width, height: popup_height };

            // Clear popup rect first — erases any separators or content bleeding through
            frame.render_widget(Clear, popup_rect);

            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::default().bg(app.theme.modal_bg).fg(app.theme.modal_border)),
                popup_rect,
            );

            for (i, popup_item) in items.iter().enumerate() {
                let item_y = popup_rect.y + 1 + i as u16;
                if item_y >= popup_rect.y + popup_rect.height.saturating_sub(1) {
                    break;
                }
                let is_sel = i == app.menu_popup_selected;
                let style = if is_sel {
                    Style::default()
                        .bg(app.theme.menu_selected_bg)
                        .fg(app.theme.menu_selected_text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .bg(app.theme.modal_bg)
                        .fg(app.theme.modal_text)
                };
                frame.render_widget(
                    Paragraph::new(format!(" {}", popup_item.label)).style(style),
                    Rect {
                        x: popup_rect.x + 1,
                        y: item_y,
                        width: popup_inner_width,
                        height: 1,
                    },
                );
            }
        }
    }

}
