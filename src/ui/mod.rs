pub mod layout_profile;
pub mod screens;
pub mod widgets;

use ratatui::{
    style::Style,
    widgets::Block,
    Frame,
};

use crate::app::{App, ScreenMode};

pub fn draw_ui(frame: &mut Frame, app: &mut App) {
    // Key debug overlay — show last key event in the bottom bar
    if app.key_debug_enabled && !app.last_key_event.is_empty() {
        app.message = app.last_key_event.clone();
        app.message_is_error = false;
    }

    let area = frame.area();
    let bg = match app.mode {
        ScreenMode::Menu => app.theme.desktop,
        ScreenMode::List | ScreenMode::Card | ScreenMode::Grid => app.theme.content_bg,
    };
    frame.render_widget(Block::default().style(Style::default().bg(bg)), area);

    match app.mode {
        ScreenMode::Menu => screens::server_menu::draw_server_menu(frame, app),
        ScreenMode::List => screens::list::draw_list(frame, app),
        ScreenMode::Card => screens::card::draw_card(frame, app),
        ScreenMode::Grid => screens::grid::draw_grid_screen(frame, app),
    }

    if app.theme_modal_open {
        widgets::modal::draw_theme_modal(frame, app);
    }
    if app.delete_confirm_open {
        widgets::modal::draw_delete_confirm_modal(frame, app);
    }
    if app.grid_delete_confirm_open {
        widgets::modal::draw_grid_delete_confirm_modal(frame, app);
    }
    if app.save_confirm_open {
        widgets::modal::draw_save_confirm_modal(frame, app);
    }
    if app.quit_confirm_open {
        widgets::modal::draw_quit_confirm_modal(frame, app);
    }
    if app.lookup_modal_open() {
        let filtered = app.lookup_modal_filtered_rows();
        let page_size = widgets::modal::draw_lookup_modal(frame, app, &filtered);
        app.lookup_modal_page_size = page_size;
    }
    if app.action_picker_open {
        widgets::modal::draw_action_picker_modal(frame, app);
    }
    if app.action_form_open {
        widgets::modal::draw_action_form_modal(frame, app);
    }
    if app.action_confirm_open {
        widgets::modal::draw_action_confirm_modal(frame, app);
    }
    if app.action_result_open {
        widgets::modal::draw_action_result_modal(frame, app);
    }
}
