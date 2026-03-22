use std::{error::Error, io::{self, Stdout}, time::Duration};

use crossterm::{
    event::{
        self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    app::{open_in_browser, App, AppAction, ActionResultNav, HeaderInputMode, PopResult, ScreenMode},
    http::{HttpClient, HttpError},
    shortcuts::Action,
    token_store,
    ui::draw_ui,
};

/// Resolve a server-returned path to a full URL, passing through absolute URLs unchanged.
fn resolve_url(path: &str, http: &HttpClient) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        http.resolve(path)
    }
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/// Clear auth state and redirect to the server's login screen.
fn redirect_to_login(app: &mut App, http: &mut HttpClient) {
    app.auth_token = None;
    app.user_display_name = None;
    http.set_token(None);
    token_store::clear_token(&http.base_url);
    app.clear_screen_history();

    let login_url = format!("{}/auth/login", http.base_url);
    match http.get_screen(&login_url) {
        Ok(screen) => {
            app.current_screen_url = Some(login_url);
            app.set_screen(screen);
        }
        Err(e) => {
            app.message = format!("{}: {}", app.ui_strings.error_prefix, e);
        }
    }
}

/// Duration for transient success messages (e.g. "Saved.").
const MESSAGE_DURATION: Duration = Duration::from_millis(300);

fn execute_action(app: &mut App, http: &mut HttpClient, action: AppAction) -> bool {
    match action {
        AppAction::None => false,
        AppAction::Quit => true,
        AppAction::FetchScreen(url) => {
            let is_refresh = app.current_screen_url.as_deref() == Some(&url);
            match http.get_screen(&url) {
                Ok(screen) => {
                    app.current_screen_url = Some(url);
                    app.set_screen(screen);
                    if is_refresh {
                        app.set_timed_message("Reloaded".to_string(), MESSAGE_DURATION);
                    }
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::FetchModalLookup(url) => {
            match http.get_screen(&url) {
                Ok(screen) => {
                    let source_field = app.pending_lookup_field.take();
                    let source_grid_cell = app.pending_lookup_grid_cell.take();
                    app.open_lookup_modal(screen, source_field, source_grid_cell);
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::FetchCardAndPush(url) => {
            match http.get_screen(&url) {
                Ok(screen) => {
                    app.push_and_set_screen(screen);
                    app.current_screen_url = Some(url);
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::FetchDrilldown(url) => {
            match http.get_screen(&url) {
                Ok(screen) => {
                    app.push_and_set_screen(screen);
                    app.preselect_lookup_row();
                    app.current_screen_url = Some(url);
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::SearchList => {
            if let Some(ref base_url) = app.current_screen_url {
                let url = HttpClient::search_url(base_url, &app.list_search_query);
                match http.get_screen(&url) {
                    Ok(screen) => {
                        app.table_state = crate::ui::widgets::table::TableState::new();
                        if let Some(ref status) = screen.status {
                            app.message = status.clone();
                        } else {
                            app.message.clear();
                        }
                        app.current_screen = Some(screen);
                    }
                    Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                    Err(err) => {
                        app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                    }
                }
            }
            false
        }
        AppAction::SaveCard(url, changeset) => {
            let was_new = app.is_new_record;
            let was_grid_mode = app.mode == ScreenMode::Grid;
            match http.post_save(&url, &changeset) {
                Ok(screen) => {
                    // Check if this is a validation error — keep the user's input
                    let is_validation_error = screen.status.as_deref()
                        .is_some_and(|s| s.starts_with("Error:"));

                    if is_validation_error {
                        // Don't replace the screen — just show the error
                        let error_msg = screen.status.unwrap_or_default();
                        app.set_error_message(error_msg);
                    } else {
                        let msg = if was_new {
                            app.ui_strings.created.clone()
                        } else {
                            app.ui_strings.saved.clone()
                        };
                        let saved_field_idx = app.card_field_index;
                        let was_overlay_open = app.lines_overlay_open;
                        let saved_grid_state = app.grid_state.clone();
                        app.set_screen(screen);
                        app.card_field_index = saved_field_idx.min(app.card_fields_flat.len().saturating_sub(1));
                        if was_grid_mode || was_overlay_open {
                            if was_overlay_open {
                                app.lines_overlay_open = true;
                            }
                            let mut restored = saved_grid_state;
                            restored.dirty_cells.clear();
                            app.grid_state = restored;
                        }
                        app.mark_parent_needs_refresh();
                        app.set_timed_message(msg, MESSAGE_DURATION);
                    }
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.save_error_prefix, err);
                }
            }
            false
        }
        AppAction::NewCard(url) => {
            match http.get_screen(&url) {
                Ok(screen) => {
                    if app.mode == ScreenMode::Card {
                        // From a card: replace the current card so Esc goes back to the list
                        app.mark_parent_needs_refresh();
                        app.set_screen(screen);
                    } else {
                        // From a list: push normally
                        app.push_and_set_screen(screen);
                    }
                    app.current_screen_url = Some(url);
                    app.is_new_record = true;
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::DeleteCard(url, req) => {
            match http.post_delete(&url, &req) {
                Ok(screen) => {
                    let deleted_msg = app.ui_strings.deleted.clone();
                    // Pop current card, then set the returned list screen
                    app.pop_screen();
                    app.set_screen(screen);
                    app.set_timed_message(deleted_msg, MESSAGE_DURATION);
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::ValidateLookup { field_id, url, value } => {
            if value.is_empty() {
                // Empty value: skip validation, just clear autofill fields
                return false;
            }
            match http.get_validate(&url) {
                Ok(resp) if resp.valid => {
                    // Apply autofill values to card fields
                    for (target_field, target_value) in &resp.autofill {
                        app.set_card_field_value(target_field, target_value.clone());
                    }
                }
                Ok(resp) => {
                    let error_msg = resp.error.unwrap_or_else(|| {
                        format!("'{}' is not valid.", value)
                    });
                    app.reject_field_value(&field_id, error_msg);
                }
                Err(err) => {
                    // Network error — don't block the user, just warn
                    app.set_error_message(format!("Validation failed: {}", err));
                }
            }
            false
        }
        AppAction::LookupReturn { value, autofill } => {
            // Check if this is a grid lookup return
            let grid_cell = app.pop_lookup_source_grid_cell();
            let source_field = app.pop_lookup_source_field();
            app.pop_screen();
            if let Some((row, _col)) = grid_cell {
                // Grid lookup: write value + autofill to the same grid row
                // Find which column the lookup was on and write the value
                if let Some(col_id) = app.current_screen.as_ref()
                    .and_then(|s| s.lines.as_ref())
                    .and_then(|l| l.columns.get(_col))
                    .map(|c| c.id.clone())
                {
                    app.set_grid_cell_value(row, &col_id, value.clone());
                }
                for (target_col_id, target_value) in &autofill {
                    app.set_grid_cell_value(row, target_col_id, target_value.clone());
                }
                // Restore grid focus and overlay, recalculate line amount
                app.lines_overlay_open = true;
                app.grid_state.row = row;
                app.grid_state.col = _col;
                app.grid_recalculate_line_amount();
                app.grid_recalculate_totals();
            } else {
                // Card lookup: write to card fields
                if let Some(ref field_id) = source_field {
                    app.set_card_field_value(field_id, value.clone());
                }
                for (target_field, target_value) in &autofill {
                    app.set_card_field_value(target_field, target_value.clone());
                }
                // Trigger validate to get full autofill (lookup table may not have all columns)
                if let Some(ref field_id) = source_field {
                    app.navigate_to_card_field(field_id);
                    let validate = app.lookup_validate_action_for_value(&value);
                    if !matches!(validate, AppAction::None) {
                        app.pending_validate = Some(validate);
                    }
                }
            }
            app.set_timed_message(value, MESSAGE_DURATION);
            false
        }
        AppAction::ValidateGridLookup { row, col, url, value } => {
            if value.is_empty() {
                return false;
            }
            match http.get_validate(&url) {
                Ok(resp) if resp.valid => {
                    // Apply autofill values to the same grid row
                    for (target_col_id, target_value) in &resp.autofill {
                        app.set_grid_cell_value(row, target_col_id, target_value.clone());
                    }
                    app.grid_recalculate_line_amount();
                    app.grid_recalculate_totals();
                }
                Ok(resp) => {
                    let error_msg = resp.error.unwrap_or_else(|| {
                        format!("'{}' is not valid.", value)
                    });
                    app.reject_grid_cell_value(row, col, error_msg);
                }
                Err(err) => {
                    app.set_error_message(format!("Validation failed: {}", err));
                }
            }
            false
        }
        AppAction::RejectGridCell { row, col, error } => {
            app.reject_grid_cell_value(row, col, error);
            false
        }
        AppAction::ExecuteAction { endpoint, request } => {
            let url = http.resolve(&endpoint);
            match http.post_action(&url, &request) {
                Ok(resp) if resp.success => {
                    let had_screen = resp.screen.is_some();
                    if let Some(screen) = resp.screen {
                        app.set_screen(*screen);
                    }
                    let msg = resp.message.unwrap_or_else(|| "Done.".to_string());
                    app.action_result_message = msg;
                    app.action_result_is_error = false;
                    app.action_result_nav = if let Some(url) = resp.redirect_url {
                        ActionResultNav::Redirect(resolve_url(&url, http))
                    } else if let Some(url) = resp.push_url {
                        ActionResultNav::Push(http.resolve(&url))
                    } else if had_screen {
                        ActionResultNav::HadScreen
                    } else {
                        ActionResultNav::None
                    };
                    app.action_result_open = true;
                }
                Ok(resp) => {
                    let msg = resp.error.unwrap_or_else(|| "Action failed.".to_string());
                    app.action_result_message = msg;
                    app.action_result_is_error = true;
                    app.action_result_open = true;
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.action_result_message = format!("{}: {}", app.ui_strings.error_prefix, err);
                    app.action_result_is_error = true;
                    app.action_result_open = true;
                }
            }
            false
        }
        AppAction::FetchScreenOrPop(url) => {
            match http.get_screen(&url) {
                Ok(screen) => {
                    app.current_screen_url = Some(url);
                    app.set_screen(screen);
                }
                Err(HttpError::NotFound) => {
                    pop_screen_with_refresh(app, http);
                }
                Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            false
        }
        AppAction::FetchScreenClearHistory(url) => {
            if !url.starts_with(&http.host_url) {
                app.message = open_in_browser(&url);
            } else {
                match http.get_screen(&url) {
                    Ok(screen) => {
                        app.clear_screen_history();
                        app.current_screen_url = Some(url);
                        app.set_screen(screen);
                    }
                    Err(HttpError::Unauthorized) => redirect_to_login(app, http),
                    Err(err) => {
                        app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                    }
                }
            }
            false
        }
        AppAction::Logout => {
            redirect_to_login(app, http);
            app.message = String::new();
            false
        }
        AppAction::AuthSubmit(url, request) => {
            match http.post_auth(&url, &request) {
                Ok(resp) if resp.success => {
                    match resp.token {
                        Some(token) => {
                            http.set_token(Some(token.clone()));
                            token_store::store_token(&http.base_url, &token);
                            app.auth_token = Some(token);
                            if let Some(screen) = resp.screen {
                                app.set_screen(screen);
                            }
                            app.message = String::new();
                            app.form_error = String::new();
                        }
                        None => {
                            app.form_error = app.ui_strings.login_error.clone();
                        }
                    }
                }
                Ok(resp) => {
                    app.form_error = resp.error.unwrap_or_else(|| app.ui_strings.login_error.clone());
                }
                Err(e) => {
                    app.form_error = format!("{}: {}", app.ui_strings.error_prefix, e);
                }
            }
            false
        }
    }
}

/// Pop the screen stack, refreshing the parent if needed.
/// If the stack is empty but the current screen declares a parent_url,
/// navigate there instead of returning false (which would trigger quit).
/// Follows parent_url chains up to MAX_PARENT_DEPTH levels to prevent infinite loops.
const MAX_PARENT_DEPTH: usize = 5;

fn pop_screen_with_refresh(app: &mut App, http: &HttpClient) -> bool {
    match app.pop_screen() {
        PopResult::Popped => true,
        PopResult::Empty => {
            // No history — follow parent_url chain, guarded by depth limit and
            // visited-URL tracking to prevent cycles (A → B → A).
            let mut visited: Vec<String> = Vec::new();

            for _ in 0..MAX_PARENT_DEPTH {
                let parent_url = app.current_screen
                    .as_ref()
                    .and_then(|s| s.parent_url.as_ref())
                    .map(|p| resolve_url(p, http));

                let url = match parent_url {
                    Some(u) => u,
                    None => return false, // no parent declared — trigger quit
                };

                if visited.contains(&url) {
                    app.message = "Navigation cycle detected.".to_string();
                    return true;
                }
                visited.push(url.clone());

                match http.get_screen(&url) {
                    Ok(screen) => {
                        app.current_screen_url = Some(url);
                        app.set_screen(screen);
                        return true;
                    }
                    Err(err) => {
                        app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                        return true; // don't quit on fetch error
                    }
                }
            }

            app.message = "Navigation limit reached.".to_string();
            true
        }
        PopResult::RefreshUrl(url, saved_table_state) => {
            let fetch_url = HttpClient::search_url(&url, &app.list_search_query);
            match http.get_screen(&fetch_url) {
                Ok(screen) => {
                    app.current_screen_url = Some(url);
                    app.set_screen(screen);
                    app.table_state = saved_table_state;
                }
                Err(err) => {
                    app.message = format!("{}: {}", app.ui_strings.error_prefix, err);
                }
            }
            true
        }
    }
}

/// Handle a key event while the lookup modal is open. Returns true if consumed.
/// Fires pending_validate directly (main loop's catch-all is skipped via `continue`).
fn handle_lookup_modal_key(
    app: &mut App,
    http: &mut HttpClient,
    key: &crossterm::event::KeyEvent,
) -> bool {
    if !app.lookup_modal_open() {
        return false;
    }
    // Ctrl+Enter: drill-down into selected row (if on_drill is set)
    if app.shortcuts.matches(Action::DrillDown, key) {
        let action = app.lookup_modal_drill_action();
        if !matches!(action, AppAction::None) {
            execute_action(app, http, action);
            return true;
        }
    }
    app.handle_lookup_modal_key(key);
    // Fire any pending validate queued by lookup_modal_select
    if let Some(validate_action) = app.pending_validate.take() {
        execute_action(app, http, validate_action);
    }
    true
}

/// Fetch the main menu from the server on startup.
pub fn fetch_initial_menu(app: &mut App, http: &HttpClient) {
    let url = format!("{}/menu/main", http.base_url);
    match http.get_screen(&url) {
        Ok(screen) => {
            app.current_screen_url = Some(url);
            app.set_screen(screen);
        }
        Err(HttpError::Unauthorized) => {
            // No valid token — fetch login screen
            let login_url = format!("{}/auth/login", http.base_url);
            match http.get_screen(&login_url) {
                Ok(screen) => {
                    app.current_screen_url = Some(login_url);
                    app.set_screen(screen);
                }
                Err(e) => {
                    app.message = format!("{}: {}", app.ui_strings.server_unavailable, e);
                }
            }
        }
        Err(err) => {
            app.message = format!("{}: {}", app.ui_strings.server_unavailable, err);
        }
    }
}

// ---------------------------------------------------------------------------
// Modifier normalization — treat Super (Cmd) and Control as the same
// ---------------------------------------------------------------------------

fn normalize_modifiers(mut key: crossterm::event::KeyEvent) -> crossterm::event::KeyEvent {
    if key.modifiers.contains(KeyModifiers::SUPER) {
        key.modifiers.remove(KeyModifiers::SUPER);
        key.modifiers.insert(KeyModifiers::CONTROL);
    }
    key
}

// ---------------------------------------------------------------------------
// Main event loop
// ---------------------------------------------------------------------------

pub fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    http: &mut HttpClient,
) -> Result<(), Box<dyn Error>> {
    loop {
        app.tick_message();
        if http.take_needs_redraw() {
            terminal.clear()?;
        }
        terminal.draw(|frame| draw_ui(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(raw_key) = event::read()? {
                if raw_key.kind == KeyEventKind::Release {
                    continue;
                }
                app.capture_key_debug(&raw_key);

                let key = normalize_modifiers(raw_key);

                // --- Modal handling ---
                if app.quit_confirm_open {
                    let opt_count = app.quit_modal_option_count();
                    match key.code {
                        KeyCode::Enter => {
                            match app.quit_modal_index {
                                0 => {
                                    // Quit
                                    app.confirm_quit();
                                    return Ok(());
                                }
                                1 => {
                                    // Cancel
                                    app.cancel_quit();
                                }
                                2 => {
                                    // Log out
                                    app.quit_confirm_open = false;
                                    let action = AppAction::Logout;
                                    execute_action(app, http, action);
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Down | KeyCode::Tab => {
                            app.quit_modal_index = (app.quit_modal_index + 1) % opt_count;
                        }
                        KeyCode::Up | KeyCode::BackTab => {
                            app.quit_modal_index = if app.quit_modal_index == 0 {
                                opt_count - 1
                            } else {
                                app.quit_modal_index - 1
                            };
                        }
                        KeyCode::Esc => app.cancel_quit(),
                        _ => {}
                    }
                    continue;
                }

                if app.save_confirm_open {
                    // Save modal: 0=Save, 1=Discard, 2=Cancel
                    // Arrow navigation
                    match key.code {
                        KeyCode::Down | KeyCode::Tab => {
                            app.save_modal_index = (app.save_modal_index + 1) % 3;
                        }
                        KeyCode::Up | KeyCode::BackTab => {
                            app.save_modal_index = if app.save_modal_index == 0 { 2 } else { app.save_modal_index - 1 };
                        }
                        KeyCode::Enter => {
                            match app.save_modal_index {
                                0 => {
                                    // Save
                                    app.save_confirm_open = false;
                                    let action = app.save_action();
                                    if execute_action(app, http, action) {
                                        return Ok(());
                                    }
                                    if !pop_screen_with_refresh(app, http) {
                                        app.request_quit();
                                    }
                                }
                                1 => {
                                    // Discard — revert changes and stay on the card
                                    app.save_confirm_open = false;
                                    app.discard_card_changes();
                                }
                                _ => {
                                    // Cancel
                                    app.save_confirm_open = false;
                                    app.set_timed_message(app.ui_strings.cancelled.clone(), MESSAGE_DURATION);
                                }
                            }
                        }
                        KeyCode::Esc => {
                            app.save_confirm_open = false;
                            app.set_timed_message(app.ui_strings.cancelled.clone(), MESSAGE_DURATION);
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.delete_confirm_open {
                    match key.code {
                        KeyCode::Right | KeyCode::Tab | KeyCode::Down => {
                            app.delete_modal_index = (app.delete_modal_index + 1) % 2;
                        }
                        KeyCode::Left | KeyCode::BackTab | KeyCode::Up => {
                            app.delete_modal_index = if app.delete_modal_index == 0 { 1 } else { 0 };
                        }
                        KeyCode::Enter => {
                            if app.delete_modal_index == 0 {
                                // Delete confirmed
                                app.close_delete_confirm();
                                let action = app.delete_action();
                                if execute_action(app, http, action) {
                                    return Ok(());
                                }
                            } else {
                                app.close_delete_confirm();
                                app.set_timed_message(app.ui_strings.cancelled.clone(), MESSAGE_DURATION);
                            }
                        }
                        KeyCode::Esc => {
                            app.close_delete_confirm();
                            app.set_timed_message(app.ui_strings.cancelled.clone(), MESSAGE_DURATION);
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.grid_delete_confirm_open {
                    match key.code {
                        KeyCode::Right | KeyCode::Tab | KeyCode::Down => {
                            app.grid_delete_modal_index = (app.grid_delete_modal_index + 1) % 2;
                        }
                        KeyCode::Left | KeyCode::BackTab | KeyCode::Up => {
                            app.grid_delete_modal_index = if app.grid_delete_modal_index == 0 { 1 } else { 0 };
                        }
                        KeyCode::Enter => {
                            if app.grid_delete_modal_index == 0 {
                                app.grid_delete_confirm_open = false;
                                app.grid_delete_row();
                            } else {
                                app.grid_delete_confirm_open = false;
                                app.set_timed_message(app.ui_strings.cancelled.clone(), MESSAGE_DURATION);
                            }
                        }
                        KeyCode::Esc => {
                            app.grid_delete_confirm_open = false;
                            app.set_timed_message(app.ui_strings.cancelled.clone(), MESSAGE_DURATION);
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.option_modal_open && !app.lines_overlay_open {
                    match key.code {
                        KeyCode::Esc => app.close_option_modal(),
                        KeyCode::Enter => {
                            app.option_modal_select();
                            app.card_next_quick_entry();
                        }
                        KeyCode::Down | KeyCode::Tab => app.option_modal_next(),
                        KeyCode::Up | KeyCode::BackTab => app.option_modal_prev(),
                        _ => {}
                    }
                    continue;
                }

                if handle_lookup_modal_key(app, http, &key) {
                    continue;
                }

                // --- Action result modal (dismiss with Enter/Esc) ---
                if app.action_result_open {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            let was_success = !app.action_result_is_error;
                            let nav = std::mem::replace(&mut app.action_result_nav, ActionResultNav::None);
                            app.action_result_open = false;
                            app.action_result_message.clear();
                            if was_success {
                                match nav {
                                    ActionResultNav::Redirect(url) => {
                                        execute_action(app, http, AppAction::FetchScreenClearHistory(url));
                                    }
                                    ActionResultNav::Push(url) => {
                                        execute_action(app, http, AppAction::FetchCardAndPush(url));
                                    }
                                    ActionResultNav::None => {
                                        if let Some(url) = app.current_screen_url.clone() {
                                            execute_action(app, http, AppAction::FetchScreenOrPop(url));
                                        }
                                    }
                                    ActionResultNav::HadScreen => {
                                        // Screen already replaced inline — nothing to do.
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    continue;
                }


                if app.theme_modal_open {
                    match key.code {
                        KeyCode::Esc => app.close_theme_modal(),
                        KeyCode::Enter => {
                            app.apply_theme_selection();
                            app.close_theme_modal();
                        }
                        KeyCode::Tab | KeyCode::Down | KeyCode::Right => app.move_theme_next(),
                        KeyCode::BackTab | KeyCode::Up | KeyCode::Left => app.move_theme_prev(),
                        _ => {}
                    }
                    continue;
                }

                // --- Action picker modal ---
                if app.action_picker_open {
                    let count = app.screen_actions().len();
                    match key.code {
                        KeyCode::Esc => app.close_action_picker(),
                        KeyCode::Down | KeyCode::Tab => {
                            app.action_picker_index = (app.action_picker_index + 1) % count;
                        }
                        KeyCode::Up | KeyCode::BackTab => {
                            app.action_picker_index = if app.action_picker_index == 0 { count - 1 } else { app.action_picker_index - 1 };
                        }
                        KeyCode::Enter => {
                            let selected = app.screen_actions()[app.action_picker_index].clone();
                            app.close_action_picker();
                            match selected.kind {
                                two_wee_shared::ActionKind::Simple => {
                                    let request = app.build_action_request(&selected);
                                    let endpoint = selected.endpoint.clone();
                                    let action = AppAction::ExecuteAction { endpoint, request };
                                    if execute_action(app, http, action) {
                                        return Ok(());
                                    }
                                }
                                two_wee_shared::ActionKind::Confirm => {
                                    app.open_action_confirm(selected);
                                }
                                two_wee_shared::ActionKind::Modal => {
                                    app.open_action_form(selected);
                                }
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // --- Action confirm modal ---
                if app.action_confirm_open {
                    match key.code {
                        KeyCode::Right | KeyCode::Tab | KeyCode::Down => {
                            app.action_confirm_index = (app.action_confirm_index + 1) % 2;
                        }
                        KeyCode::Left | KeyCode::BackTab | KeyCode::Up => {
                            app.action_confirm_index = if app.action_confirm_index == 0 { 1 } else { 0 };
                        }
                        KeyCode::Enter => {
                            if app.action_confirm_index == 0 {
                                // Yes — execute
                                if let Some(def) = app.action_confirm_def.clone() {
                                    let request = app.build_action_request(&def);
                                    let endpoint = def.endpoint.clone();
                                    app.close_action_confirm();
                                    let action = AppAction::ExecuteAction { endpoint, request };
                                    if execute_action(app, http, action) {
                                        return Ok(());
                                    }
                                }
                            } else {
                                app.close_action_confirm();
                            }
                        }
                        KeyCode::Esc => app.close_action_confirm(),
                        _ => {}
                    }
                    continue;
                }

                // --- Action form modal ---
                if app.action_form_open {
                    if app.action_form_def.is_some() {
                        let field_count = app.action_form_def.as_ref().map(|d| d.fields.len()).unwrap_or(0);

                        // Ctrl+Enter: submit from any mode, unless editing a TextArea (where it inserts a newline)
                        let editing_textarea = app.action_form_input_mode == HeaderInputMode::Edit
                            && app.action_form_fields.get(app.action_form_field_index)
                                .map(|f| f.field_type == two_wee_shared::FieldType::TextArea)
                                .unwrap_or(false);
                        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL) && !editing_textarea {
                            app.action_form_confirm_edit();
                            let (endpoint, request) = {
                                let def = app.action_form_def.as_ref().unwrap();
                                (def.endpoint.clone(), app.build_action_request_with_form(def))
                            };
                            app.close_action_form();
                            let action = AppAction::ExecuteAction { endpoint, request };
                            if execute_action(app, http, action) {
                                return Ok(());
                            }
                            continue;
                        }

                        // Edit mode
                        if app.action_form_input_mode == HeaderInputMode::Edit {
                            match key.code {
                                KeyCode::Esc => {
                                    app.action_form_revert_edit();
                                }
                                KeyCode::Enter if key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
                                    let is_ta = app.action_form_fields.get(app.action_form_field_index)
                                        .map(|f| f.field_type == two_wee_shared::FieldType::TextArea)
                                        .unwrap_or(false);
                                    if is_ta {
                                        let max = app.action_form_fields.get(app.action_form_field_index)
                                            .and_then(|f| f.rows).unwrap_or(4) as usize;
                                        app.action_form_textarea_newline(max);
                                    }
                                }
                                KeyCode::Enter => {
                                    app.action_form_confirm_edit();
                                    // Advance or submit
                                    if app.action_form_field_index + 1 >= field_count {
                                        let (endpoint, request) = {
                                            let def = app.action_form_def.as_ref().unwrap();
                                            (def.endpoint.clone(), app.build_action_request_with_form(def))
                                        };
                                        app.close_action_form();
                                        let action = AppAction::ExecuteAction { endpoint, request };
                                        if execute_action(app, http, action) {
                                            return Ok(());
                                        }
                                    } else {
                                        app.action_form_next_field();
                                    }
                                }
                                KeyCode::Tab => {
                                    app.action_form_confirm_edit();
                                    app.action_form_next_field();
                                }
                                KeyCode::BackTab => {
                                    app.action_form_confirm_edit();
                                    app.action_form_prev_field();
                                }
                                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    let len = app.action_form_current_field()
                                        .map(|f| f.value.chars().count())
                                        .unwrap_or(0);
                                    app.action_form_selection_anchor = Some(0);
                                    app.action_form_cursor = len;
                                }
                                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL)
                                    && !key.modifiers.contains(KeyModifiers::ALT) =>
                                {
                                    // Delete selection if any, then insert
                                    if let Some((start, end)) = app.action_form_selection_range() {
                                        if let Some(field) = app.action_form_current_field_mut() {
                                            let s: String = field.value.chars().take(start).collect();
                                            let e: String = field.value.chars().skip(end).collect();
                                            field.value = format!("{}{}", s, e);
                                            app.action_form_cursor = start;
                                        }
                                        app.action_form_selection_anchor = None;
                                    }
                                    app.action_form_insert_char(c);
                                }
                                KeyCode::Backspace => {
                                    if let Some((start, end)) = app.action_form_selection_range() {
                                        if let Some(field) = app.action_form_current_field_mut() {
                                            let s: String = field.value.chars().take(start).collect();
                                            let e: String = field.value.chars().skip(end).collect();
                                            field.value = format!("{}{}", s, e);
                                            app.action_form_cursor = start;
                                        }
                                        app.action_form_selection_anchor = None;
                                    } else {
                                        app.action_form_delete_char();
                                    }
                                }
                                KeyCode::Delete => {
                                    let cursor = app.action_form_cursor;
                                    if let Some(field) = app.action_form_current_field_mut() {
                                        let len = field.value.chars().count();
                                        if cursor < len {
                                            let byte_pos = field.value.char_indices()
                                                .nth(cursor)
                                                .map(|(i, _)| i)
                                                .unwrap_or(field.value.len());
                                            let end = field.value.char_indices()
                                                .nth(cursor + 1)
                                                .map(|(i, _)| i)
                                                .unwrap_or(field.value.len());
                                            field.value.replace_range(byte_pos..end, "");
                                        }
                                    }
                                }
                                KeyCode::Left => {
                                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                                        if app.action_form_selection_anchor.is_none() {
                                            app.action_form_selection_anchor = Some(app.action_form_cursor);
                                        }
                                    } else {
                                        app.action_form_selection_anchor = None;
                                    }
                                    if app.action_form_cursor > 0 {
                                        app.action_form_cursor -= 1;
                                    }
                                }
                                KeyCode::Right => {
                                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                                        if app.action_form_selection_anchor.is_none() {
                                            app.action_form_selection_anchor = Some(app.action_form_cursor);
                                        }
                                    } else {
                                        app.action_form_selection_anchor = None;
                                    }
                                    let len = app.action_form_current_field()
                                        .map(|f| f.value.chars().count())
                                        .unwrap_or(0);
                                    if app.action_form_cursor < len {
                                        app.action_form_cursor += 1;
                                    }
                                }
                                KeyCode::Home => {
                                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                                        if app.action_form_selection_anchor.is_none() {
                                            app.action_form_selection_anchor = Some(app.action_form_cursor);
                                        }
                                    } else {
                                        app.action_form_selection_anchor = None;
                                    }
                                    app.action_form_cursor = 0;
                                }
                                KeyCode::End => {
                                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                                        if app.action_form_selection_anchor.is_none() {
                                            app.action_form_selection_anchor = Some(app.action_form_cursor);
                                        }
                                    } else {
                                        app.action_form_selection_anchor = None;
                                    }
                                    app.action_form_cursor = app.action_form_current_field()
                                        .map(|f| f.value.chars().count())
                                        .unwrap_or(0);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        // Select mode
                        match key.code {
                            KeyCode::Esc => app.close_action_form(),
                            KeyCode::Tab | KeyCode::Down => {
                                app.action_form_next_field();
                            }
                            KeyCode::BackTab | KeyCode::Up => {
                                app.action_form_prev_field();
                            }
                            KeyCode::Enter | KeyCode::Char(' ') if app.action_form_is_boolean_field() => {
                                app.action_form_bool_toggle();
                            }
                            KeyCode::Enter => {
                                if app.action_form_is_option_field() {
                                    app.action_form_option_cycle();
                                } else {
                                    app.action_form_next_field();
                                }
                            }
                            KeyCode::F(2) if app.action_form_is_boolean_field() => {
                                app.action_form_bool_toggle();
                            }
                            KeyCode::F(2) => {
                                app.action_form_begin_edit();
                            }
                            KeyCode::Char(' ') if app.action_form_is_option_field() => {
                                app.action_form_option_cycle();
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL)
                                && !key.modifiers.contains(KeyModifiers::ALT) =>
                            {
                                if !app.action_form_is_option_field() {
                                    app.action_form_begin_edit();
                                    app.action_form_insert_char(c);
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                // Any key press clears transient messages (user acknowledged by acting)
                if !app.message.is_empty() && app.message_expires.is_none() {
                    app.message.clear();
                }

                // --- Screen-specific key handling ---
                let action = match app.mode {
                    ScreenMode::List => handle_list_key(app, http, &key),
                    ScreenMode::Card => handle_card_key(app, http, &key),
                    ScreenMode::Grid => handle_grid_key(app, http, &key),
                    ScreenMode::Menu => handle_menu_key(app, http, &key),
                };

                if execute_action(app, http, action) {
                    return Ok(());
                }

                // Fire any pending lookup validation queued by card_confirm_edit
                if let Some(validate_action) = app.pending_validate.take() {
                    if execute_action(app, http, validate_action) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// List key handler
// ---------------------------------------------------------------------------

fn handle_list_key(app: &mut App, http: &HttpClient, key: &crossterm::event::KeyEvent) -> AppAction {
    let row_count = app
        .current_screen
        .as_ref()
        .and_then(|s| s.lines.as_ref())
        .map(|l| l.rows.len())
        .unwrap_or(0);

    // Registry-based command shortcuts
    if let Some(action) = app.shortcuts.action_for(key) {
        match action {
            Action::Quit if app.list_search_query.is_empty() => {
                app.request_quit();
                return AppAction::None;
            }
            Action::NewCard => return app.new_card_action(),
            Action::Refresh => return app.refresh_action(),
            Action::ToggleKeyDebug => { app.toggle_key_debug(); return AppAction::None; }
            Action::ThemeModal => { app.open_theme_modal(); return AppAction::None; }
            Action::DebugJson => { app.copy_screen_json_to_clipboard(); return AppAction::None; }
            Action::CopyUrl => { app.copy_url_to_clipboard(); return AppAction::None; }
            Action::ActionPicker => {
                if app.has_screen_actions() {
                    app.open_action_picker();
                } else {
                    app.set_timed_message("No actions available".to_string(), Duration::from_secs(2));
                }
                return AppAction::None;
            }
            _ => {}
        }
    }
    // F3 is ambiguous (shared with InsertRow) so not in key_map — check via matches()
    if app.shortcuts.matches(Action::NewCard, key) {
        return app.new_card_action();
    }

    // Escape handling (in registry but needs special logic)
    if app.shortcuts.matches(Action::Escape, key) {
        return if !app.list_search_query.is_empty() {
            app.list_search_query.clear();
            AppAction::SearchList
        } else if !pop_screen_with_refresh(app, http) {
            app.request_quit();
            AppAction::None
        } else {
            AppAction::None
        };
    }

    // Ctrl+Enter: drill-down into selected row (if on_drill is set)
    if app.shortcuts.matches(Action::DrillDown, key) {
        let action = app.list_drill_action();
        if !matches!(action, AppAction::None) {
            return action;
        }
    }

    // Raw key handling (navigation, type-to-search)
    match key.code {
        KeyCode::Enter => app.list_enter_action(),
        KeyCode::Down | KeyCode::Tab => {
            app.table_state.select_next(row_count);
            AppAction::None
        }
        KeyCode::Up | KeyCode::BackTab => {
            app.table_state.select_prev();
            AppAction::None
        }
        KeyCode::Home => {
            app.table_state.select_first();
            AppAction::None
        }
        KeyCode::End => {
            app.table_state.select_last(row_count);
            AppAction::None
        }
        KeyCode::PageDown => {
            app.table_state.page_down(row_count, 20);
            AppAction::None
        }
        KeyCode::PageUp => {
            app.table_state.page_up(20);
            AppAction::None
        }
        // Type-to-search: any printable character appends to search query
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            app.list_search_query.push(c);
            AppAction::SearchList
        }
        KeyCode::Backspace => {
            if !app.list_search_query.is_empty() {
                app.list_search_query.pop();
                AppAction::SearchList
            } else {
                AppAction::None
            }
        }
        _ => AppAction::None,
    }
}

// ---------------------------------------------------------------------------
// Card key handler
// ---------------------------------------------------------------------------

fn handle_card_key(app: &mut App, http: &HttpClient, key: &crossterm::event::KeyEvent) -> AppAction {
    // Lines overlay intercept — when open, all keys go to the overlay handler
    if app.lines_overlay_open {
        return handle_lines_overlay_key(app, key);
    }

    fn is_word_jump(modifiers: KeyModifiers) -> bool {
        modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT)
    }

    // Ctrl+L / Alt+L: open lines overlay (only on HeaderLines screens)
    if app.shortcuts.matches(Action::OpenLines, key) && app.has_lines() {
        if app.header_input_mode == HeaderInputMode::Edit {
            app.card_confirm_edit();
        }
        app.lines_overlay_open = true;
        return AppAction::None;
    }

    // Global shortcuts (must be checked before edit mode eats the char)
    if let Some(action) = app.shortcuts.action_for(key) {
        match action {
            Action::Save => {
                if app.header_input_mode == HeaderInputMode::Edit && !app.card_confirm_edit() {
                    return AppAction::None;
                }
                return app.save_action();
            }
            Action::NewCard => return app.new_card_action(),
            Action::CopyField => {
                app.copy_current_field();
                return AppAction::None;
            }
            Action::ActionPicker => {
                // Don't intercept Ctrl+A while editing — let it select-all in the field
                if app.header_input_mode != HeaderInputMode::Edit {
                    if app.has_screen_actions() {
                        app.open_action_picker();
                    } else {
                        app.set_timed_message("No actions available".to_string(), Duration::from_secs(2));
                    }
                    return AppAction::None;
                }
            }
            _ => {}
        }
    }
    // Ctrl+D delete — caught here so it works even during edit mode.
    // F4 delete is handled later in select mode (after edit-mode validation).
    if app.shortcuts.action_for(key) == Some(Action::Delete) {
        if !app.is_new_record {
            app.open_delete_confirm();
        }
        return AppAction::None;
    }

    // Reset F2 cycle on any key except F2
    if key.code != KeyCode::F(2) {
        app.reset_edit_cycle();
    }

    // Edit mode key handling
    if app.header_input_mode == HeaderInputMode::Edit {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let word = is_word_jump(key.modifiers);

        match key.code {
            KeyCode::Esc => {
                app.card_revert_edit();
                return AppAction::None;
            }
            KeyCode::Enter if key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
                if app.current_card_field().map(|f| f.field_type == two_wee_shared::FieldType::TextArea).unwrap_or(false) {
                    let max = app.current_card_field().and_then(|f| f.rows).unwrap_or(4) as usize;
                    app.card_textarea_newline(max);
                }
                return AppAction::None;
            }
            KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
                if !app.card_confirm_edit() {
                    return AppAction::None; // Validation failed — stay in field
                }
                // On login forms: submit if on last field
                if app.is_auth_screen() {
                    let is_last = app.card_field_index + 1 >= app.card_fields_flat.len();
                    if is_last {
                        return app.auth_submit_action();
                    }
                }
                // Advance to the next quick_entry field (Enter = fast data entry path)
                app.card_next_quick_entry();
                return AppAction::None;
            }
            KeyCode::F(2) => {
                // F2 in edit mode cycles through the stages
                app.card_f2_cycle();
                return AppAction::None;
            }
            KeyCode::Tab => {
                if app.card_confirm_edit() {
                    // On login forms: Tab on last field submits
                    if app.is_auth_screen() && app.card_field_index + 1 >= app.card_fields_flat.len() {
                        return app.auth_submit_action();
                    }
                    app.card_next_field();
                }
                return AppAction::None;
            }
            KeyCode::BackTab => {
                if app.card_confirm_edit() {
                    app.card_prev_field();
                }
                return AppAction::None;
            }
            KeyCode::Left if shift && word => {
                app.extend_selection_word_left();
                return AppAction::None;
            }
            KeyCode::Left if shift => {
                app.extend_selection_left();
                return AppAction::None;
            }
            KeyCode::Left if word => {
                app.card_cursor_word_left();
                return AppAction::None;
            }
            KeyCode::Left => {
                app.move_cursor_left();
                return AppAction::None;
            }
            KeyCode::Right if shift && word => {
                app.extend_selection_word_right();
                return AppAction::None;
            }
            KeyCode::Right if shift => {
                app.extend_selection_right();
                return AppAction::None;
            }
            KeyCode::Right if word => {
                app.card_cursor_word_right();
                return AppAction::None;
            }
            KeyCode::Right => {
                app.move_cursor_right();
                return AppAction::None;
            }
            KeyCode::Char('b') | KeyCode::Char('B') if word => {
                app.card_cursor_word_left();
                return AppAction::None;
            }
            KeyCode::Char('f') | KeyCode::Char('F') if word => {
                app.card_cursor_word_right();
                return AppAction::None;
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.select_all();
                return AppAction::None;
            }
            KeyCode::Home if shift => {
                app.extend_selection_home();
                return AppAction::None;
            }
            KeyCode::Home => {
                app.header_cursor = 0;
                app.clear_selection();
                return AppAction::None;
            }
            KeyCode::End if shift => {
                app.extend_selection_end();
                return AppAction::None;
            }
            KeyCode::End => {
                app.card_cursor_end();
                app.clear_selection();
                return AppAction::None;
            }
            KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT)
                || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.card_delete_word_back();
                return AppAction::None;
            }
            // macOS terminals may send Option+Backspace as DEL (U+007F) with ALT modifier
            KeyCode::Char('\u{7f}') if key.modifiers.contains(KeyModifiers::ALT) => {
                app.card_delete_word_back();
                return AppAction::None;
            }
            KeyCode::Backspace | KeyCode::Char('\u{7f}') => {
                app.card_backspace();
                return AppAction::None;
            }
            KeyCode::Delete => {
                app.card_delete();
                return AppAction::None;
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.card_insert_char(c);
                return AppAction::None;
            }
            // Keys that leave the field: validate first, then fall through
            // to the select-mode handler below.
            KeyCode::Down | KeyCode::Up | KeyCode::Enter
            | KeyCode::F(3) | KeyCode::F(4) | KeyCode::F(6)
            | KeyCode::Char('q') | KeyCode::Char('Q') => {
                if !app.card_confirm_edit() {
                    return AppAction::None;
                }
                // Fall through to select-mode keys
            }
            _ => {
                // Unhandled key in edit mode — ignore without validating
                return AppAction::None;
            }
        }
    }

    // Option field handling in Select mode (before general keys)
    if app.header_input_mode == HeaderInputMode::Select && app.card_field_is_option() {
        match key.code {
            // Enter always advances — never cycles on an option field
            KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
                app.card_next_quick_entry();
                return AppAction::None;
            }
            KeyCode::Backspace | KeyCode::Delete => {
                app.card_option_reset();
                return AppAction::None;
            }
            _ => {}
        }
        if app.shortcuts.matches(Action::OptionCycle, key) {
            app.card_option_cycle();
            return AppAction::None;
        }
        if app.shortcuts.matches(Action::OptionSelect, key) {
            app.open_option_modal();
            return AppAction::None;
        }
    }

    // Boolean toggle field handling in Select mode
    if app.header_input_mode == HeaderInputMode::Select && app.card_field_is_boolean() {
        match key.code {
            KeyCode::Char(' ') | KeyCode::F(2) => {
                app.card_bool_toggle();
                return AppAction::None;
            }
            _ => {}
        }
    }

    // Select mode / general keys — registry-based commands
    if app.shortcuts.matches(Action::NewCard, key) {
        return app.new_card_action();
    }
    if app.shortcuts.matches(Action::Delete, key) {
        if !app.is_new_record {
            app.open_delete_confirm();
        }
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::Quit, key) {
        app.request_quit();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::Escape, key) {
        return if app.is_auth_screen() {
            app.request_quit();
            AppAction::None
        } else if app.is_new_record && !app.is_card_dirty() {
            pop_screen_with_refresh(app, http);
            AppAction::None
        } else if app.is_card_dirty() {
            app.save_confirm_open = true;
            app.save_modal_index = 0;
            AppAction::None
        } else if !pop_screen_with_refresh(app, http) {
            app.request_quit();
            AppAction::None
        } else {
            AppAction::None
        };
    }
    if app.shortcuts.matches(Action::Lookup, key) || app.shortcuts.matches(Action::DrillDown, key) {
        return app.lookup_or_drilldown_action();
    }
    if app.shortcuts.matches(Action::EditCycle, key) {
        if app.card_field_editable() {
            app.card_f2_cycle();
        }
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::Refresh, key) {
        return app.refresh_action();
    }
    if app.shortcuts.matches(Action::ToggleKeyDebug, key) {
        app.toggle_key_debug();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::ThemeModal, key) {
        app.open_theme_modal();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::DebugJson, key) {
        app.copy_screen_json_to_clipboard();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::CopyUrl, key) {
        app.copy_url_to_clipboard();
        return AppAction::None;
    }

    match key.code {
        KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
            if app.is_auth_screen() {
                let is_last = app.card_field_index + 1 >= app.card_fields_flat.len();
                if is_last {
                    return app.auth_submit_action();
                }
            }
            app.card_next_quick_entry();
            AppAction::None
        }
        KeyCode::Down => {
            if key.modifiers.contains(KeyModifiers::ALT) {
                if app.card_field_is_option() {
                    app.card_option_cycle();
                } else {
                    return app.lookup_or_drilldown_action();
                }
            } else {
                app.select_mode();
                app.card_move_down();
            }
            AppAction::None
        }
        KeyCode::Up => {
            app.select_mode();
            app.card_move_up();
            AppAction::None
        }
        KeyCode::Tab => {
            app.select_mode();
            app.card_next_field();
            AppAction::None
        }
        KeyCode::BackTab => {
            app.select_mode();
            app.card_prev_field();
            AppAction::None
        }
        KeyCode::Left => {
            app.select_mode();
            app.card_move_left();
            AppAction::None
        }
        KeyCode::Right => {
            app.select_mode();
            app.card_move_right();
            AppAction::None
        }
        KeyCode::Home => {
            app.card_home_field();
            AppAction::None
        }
        KeyCode::End => {
            app.card_end_field();
            AppAction::None
        }
        KeyCode::Char(c)
            if app.header_input_mode == HeaderInputMode::Select
                && !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
                && !app.card_field_is_option()
                && c != 'q'
                && c != 'Q' =>
        {
            app.card_begin_typing(c);
            AppAction::None
        }
        _ => AppAction::None,
    }
}

// ---------------------------------------------------------------------------
// Lines overlay key handler — editable grid with cell-level navigation
// ---------------------------------------------------------------------------

fn handle_lines_overlay_key(app: &mut App, key: &crossterm::event::KeyEvent) -> AppAction {
    // Option dropdown modal intercept — takes priority over everything
    if app.option_modal_open {
        match key.code {
            KeyCode::Esc => app.close_option_modal(),
            KeyCode::Enter => app.grid_option_modal_select(),
            KeyCode::Down | KeyCode::Tab => app.option_modal_next(),
            KeyCode::Up | KeyCode::BackTab => app.option_modal_prev(),
            _ => {}
        }
        return AppAction::None;
    }

    // Lookup modal intercept (grid context)
    if app.lookup_modal_open() {
        // Ctrl+Enter: drill-down into selected row
        if app.shortcuts.matches(Action::DrillDown, key) {
            let action = app.lookup_modal_drill_action();
            if !matches!(action, AppAction::None) {
                return action;
            }
        }
        app.handle_lookup_modal_key(key);
        return AppAction::None;
    }

    let row_count = app.grid_row_count();
    let col_count = app.grid_col_count();

    // Reset F2 cycle on any key except F2
    if !app.shortcuts.matches(Action::EditCycle, key) {
        app.reset_edit_cycle();
    }

    // --- Ctrl+S save (works in both edit and select mode) ---
    if app.shortcuts.matches(Action::Save, key) {
        if app.grid_state.editing {
            let pre = grid_pre_confirm_state(app);
            app.grid_confirm_edit();
            app.pending_validate = grid_validate_from_pre(app, pre);
        }
        return app.save_action();
    }

    // --- Ctrl+A / F8 action picker ---
    if app.shortcuts.matches(Action::ActionPicker, key) {
        if app.has_screen_actions() {
            app.open_action_picker();
        } else {
            app.set_timed_message("No actions available".to_string(), Duration::from_secs(2));
        }
        return AppAction::None;
    }

    // --- Edit mode ---
    if app.grid_state.editing {
        // Any non-printable key (except F2) clears the selection.
        match key.code {
            KeyCode::F(2) | KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete => {}
            _ => { app.grid_state.selection_anchor = None; }
        }
        match key.code {
            KeyCode::Esc => {
                app.grid_revert_edit();
                return AppAction::None;
            }
            KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_advance_quick_entry();
                return AppAction::None;
            }
            KeyCode::Tab => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_state.tab_next(col_count, row_count);
                return AppAction::None;
            }
            KeyCode::BackTab => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_state.tab_prev(col_count);
                return AppAction::None;
            }
            KeyCode::Up => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_state.move_up();
                return AppAction::None;
            }
            KeyCode::Down => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_move_down_or_append();
                return AppAction::None;
            }
            KeyCode::Left => {
                if app.grid_state.cursor > 0 {
                    app.grid_state.cursor -= 1;
                }
                return AppAction::None;
            }
            KeyCode::Right => {
                let len = app.grid_cell_value().map(|v| v.chars().count()).unwrap_or(0);
                if app.grid_state.cursor < len {
                    app.grid_state.cursor += 1;
                }
                return AppAction::None;
            }
            KeyCode::Home => {
                app.grid_state.cursor = 0;
                return AppAction::None;
            }
            KeyCode::End => {
                let len = app.grid_cell_value().map(|v| v.chars().count()).unwrap_or(0);
                app.grid_state.cursor = len;
                return AppAction::None;
            }
            KeyCode::Backspace => {
                app.grid_backspace();
                return AppAction::None;
            }
            KeyCode::Delete => {
                app.grid_delete();
                return AppAction::None;
            }
            _ => {}
        }
        // Edit-mode registry commands (after raw key handling)
        if app.shortcuts.matches(Action::InsertRow, key) {
            app.grid_confirm_edit();
            app.grid_insert_row_below();
            return AppAction::None;
        }
        if app.shortcuts.matches(Action::EditCycle, key) {
            app.grid_f2_cycle();
            return AppAction::None;
        }
        if app.grid_col_is_lookup() && app.shortcuts.matches(Action::Lookup, key) {
            app.grid_confirm_edit();
            return app.grid_lookup_action();
        }
        if app.shortcuts.matches(Action::DeleteRow, key) {
            app.grid_confirm_edit();
            app.open_grid_delete_confirm();
            return AppAction::None;
        }
        match key.code {
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.grid_insert_char(c);
                return AppAction::None;
            }
            _ => return AppAction::None,
        }
    }

    // --- Option column intercept (select mode) ---
    if app.grid_col_is_option() {
        if app.shortcuts.matches(Action::OptionCycle, key) {
            app.grid_ensure_row();
            app.grid_option_cycle();
            return AppAction::None;
        }
        if app.grid_row_count() > 0 {
            match key.code {
                KeyCode::Backspace | KeyCode::Delete => {
                    app.grid_option_reset();
                    return AppAction::None;
                }
                _ => {}
            }
        }
        if app.shortcuts.matches(Action::OptionSelect, key) {
            app.grid_ensure_row();
            app.open_grid_option_modal();
            return AppAction::None;
        }
    }

    // --- Lookup column intercept (select mode) ---
    if app.grid_col_is_lookup() && !app.grid_col_is_option() {
        if app.shortcuts.matches(Action::Lookup, key) {
            app.grid_ensure_row();
            return app.grid_lookup_action();
        }
    }

    // --- Select mode ---
    if app.shortcuts.matches(Action::Escape, key) {
        app.lines_overlay_open = false;
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::EditCycle, key) {
        if !app.grid_col_is_option() && app.grid_cell_is_editable() {
            app.grid_ensure_row();
            app.grid_f2_cycle();
        }
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::InsertRow, key) {
        app.grid_insert_row_below();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::DeleteRow, key) {
        app.open_grid_delete_confirm();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::ToggleKeyDebug, key) {
        app.toggle_key_debug();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::ThemeModal, key) {
        app.open_theme_modal();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::DebugJson, key) {
        app.copy_screen_json_to_clipboard();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::CopyUrl, key) {
        app.copy_url_to_clipboard();
        return AppAction::None;
    }

    match key.code {
        KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
            app.grid_ensure_row();
            app.grid_advance_quick_entry();
            AppAction::None
        }
        KeyCode::Up => {
            app.grid_state.move_up();
            AppAction::None
        }
        KeyCode::Down => {
            app.grid_move_down_or_append();
            AppAction::None
        }
        KeyCode::Left => {
            app.grid_state.move_left();
            AppAction::None
        }
        KeyCode::Right => {
            app.grid_state.move_right(col_count);
            AppAction::None
        }
        KeyCode::Tab => {
            app.grid_state.tab_next(col_count, row_count);
            AppAction::None
        }
        KeyCode::BackTab => {
            app.grid_state.tab_prev(col_count);
            AppAction::None
        }
        KeyCode::Home => {
            app.grid_state.col = 0;
            AppAction::None
        }
        KeyCode::End => {
            app.grid_state.col = col_count.saturating_sub(1);
            AppAction::None
        }
        KeyCode::PageDown => {
            app.grid_state.page_down(row_count, 20);
            AppAction::None
        }
        KeyCode::PageUp => {
            app.grid_state.page_up(20);
            AppAction::None
        }
        // Type-to-edit: any printable char starts editing
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
                && !app.grid_col_is_option()
                && app.grid_cell_is_editable() =>
        {
            app.grid_ensure_row();
            app.grid_begin_typing(c);
            AppAction::None
        }
        _ => AppAction::None,
    }
}

/// Pre-confirm state for grid lookup validation.
/// Captures row, col, value, and validate endpoint before grid_confirm_edit().
struct GridPreConfirm {
    row: usize,
    col: usize,
    value: String,
    validate_endpoint: Option<String>,
    max_length: Option<usize>,
    context_suffix: String,
}

/// Capture grid cell state before confirm_edit (so we know what to validate).
fn grid_pre_confirm_state(app: &App) -> GridPreConfirm {
    let row = app.grid_state.row;
    let col = app.grid_state.col;
    let value = app.grid_cell_value().unwrap_or("").to_string();
    let col_def = app.current_screen.as_ref()
        .and_then(|s| s.lines.as_ref())
        .and_then(|l| l.columns.get(col));
    let validate_endpoint = col_def
        .and_then(|c| c.lookup.as_ref())
        .and_then(|l| l.validate.as_ref())
        .cloned();
    let max_length = col_def
        .and_then(|c| c.validation.as_ref())
        .and_then(|v| v.max_length);
    let context = col_def
        .and_then(|c| c.lookup.as_ref())
        .map(|l| l.context.as_slice())
        .unwrap_or(&[]);
    let context_suffix = app.grid_context_query_suffix(context);
    GridPreConfirm { row, col, value, validate_endpoint, max_length, context_suffix }
}

/// After confirm_edit, validate the cell: max_length (client-side) or lookup (server-side).
fn grid_validate_from_pre(app: &App, pre: GridPreConfirm) -> Option<AppAction> {
    if pre.value.is_empty() {
        return None;
    }
    // Check max_length client-side (no server call needed)
    if let Some(max_len) = pre.max_length {
        if pre.value.len() > max_len {
            return Some(AppAction::RejectGridCell {
                row: pre.row,
                col: pre.col,
                error: format!("Maximum {} characters allowed.", max_len),
            });
        }
    }
    // Server-side lookup validation
    let endpoint = pre.validate_endpoint?;
    // After confirm_edit, the dirty_cells entry should exist if the value changed
    let key = (pre.row, pre.col);
    if let Some(original) = app.grid_state.dirty_cells.get(&key) {
        if *original == pre.value {
            return None; // Value didn't actually change
        }
    } else {
        return None; // Not dirty
    }
    let url = format!("{}{}/{}{}", app.host_url, endpoint, pre.value, pre.context_suffix);
    Some(AppAction::ValidateGridLookup {
        row: pre.row,
        col: pre.col,
        url,
        value: pre.value,
    })
}

// ---------------------------------------------------------------------------
// Grid key handler — full-screen editable grid with Escape to go back
// ---------------------------------------------------------------------------

fn handle_grid_key(app: &mut App, http: &HttpClient, key: &crossterm::event::KeyEvent) -> AppAction {
    // Option dropdown modal intercept
    if app.option_modal_open {
        match key.code {
            KeyCode::Esc => app.close_option_modal(),
            KeyCode::Enter => app.grid_option_modal_select(),
            KeyCode::Down | KeyCode::Tab => app.option_modal_next(),
            KeyCode::Up | KeyCode::BackTab => app.option_modal_prev(),
            _ => {}
        }
        return AppAction::None;
    }

    // Lookup modal intercept
    if app.lookup_modal_open() {
        // Ctrl+Enter: drill-down into selected row
        if app.shortcuts.matches(Action::DrillDown, key) {
            let action = app.lookup_modal_drill_action();
            if !matches!(action, AppAction::None) {
                return action;
            }
        }
        app.handle_lookup_modal_key(key);
        return AppAction::None;
    }

    let row_count = app.grid_row_count();
    let col_count = app.grid_col_count();

    // Reset F2 cycle on any key except F2
    if !app.shortcuts.matches(Action::EditCycle, key) {
        app.reset_edit_cycle();
    }

    // Ctrl+S save
    if app.shortcuts.matches(Action::Save, key) {
        if app.grid_state.editing {
            let pre = grid_pre_confirm_state(app);
            app.grid_confirm_edit();
            app.pending_validate = grid_validate_from_pre(app, pre);
        }
        return app.save_action();
    }

    // Ctrl+A / F8 action picker
    if app.shortcuts.matches(Action::ActionPicker, key) {
        if app.has_screen_actions() {
            app.open_action_picker();
        } else {
            app.set_timed_message("No actions available".to_string(), Duration::from_secs(2));
        }
        return AppAction::None;
    }

    // Edit mode — delegate to grid handler for cell editing
    if app.grid_state.editing {
        // Any non-printable key (except F2) clears the selection.
        match key.code {
            KeyCode::F(2) | KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete => {}
            _ => { app.grid_state.selection_anchor = None; }
        }
        match key.code {
            KeyCode::Esc => {
                app.grid_revert_edit();
                return AppAction::None;
            }
            KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_advance_quick_entry();
                return AppAction::None;
            }
            KeyCode::Tab => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_state.tab_next(col_count, row_count);
                return AppAction::None;
            }
            KeyCode::BackTab => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_state.tab_prev(col_count);
                return AppAction::None;
            }
            KeyCode::Up => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_state.move_up();
                return AppAction::None;
            }
            KeyCode::Down => {
                let pre = grid_pre_confirm_state(app);
                app.grid_confirm_edit();
                app.pending_validate = grid_validate_from_pre(app, pre);
                app.grid_move_down_or_append();
                return AppAction::None;
            }
            KeyCode::Left => {
                if app.grid_state.cursor > 0 {
                    app.grid_state.cursor -= 1;
                }
                return AppAction::None;
            }
            KeyCode::Right => {
                let len = app.grid_cell_value().map(|v| v.chars().count()).unwrap_or(0);
                if app.grid_state.cursor < len {
                    app.grid_state.cursor += 1;
                }
                return AppAction::None;
            }
            KeyCode::Home => {
                app.grid_state.cursor = 0;
                return AppAction::None;
            }
            KeyCode::End => {
                let len = app.grid_cell_value().map(|v| v.chars().count()).unwrap_or(0);
                app.grid_state.cursor = len;
                return AppAction::None;
            }
            KeyCode::Backspace => {
                app.grid_backspace();
                return AppAction::None;
            }
            KeyCode::Delete => {
                app.grid_delete();
                return AppAction::None;
            }
            _ => {}
        }
        if app.shortcuts.matches(Action::InsertRow, key) {
            app.grid_confirm_edit();
            app.grid_insert_row_below();
            return AppAction::None;
        }
        if app.shortcuts.matches(Action::EditCycle, key) {
            app.grid_f2_cycle();
            return AppAction::None;
        }
        if app.grid_col_is_lookup() && app.shortcuts.matches(Action::Lookup, key) {
            app.grid_confirm_edit();
            return app.grid_lookup_action();
        }
        if app.shortcuts.matches(Action::DeleteRow, key) {
            app.grid_confirm_edit();
            app.open_grid_delete_confirm();
            return AppAction::None;
        }
        match key.code {
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.grid_insert_char(c);
                return AppAction::None;
            }
            _ => return AppAction::None,
        }
    }

    // --- Option column intercept (select mode) ---
    if app.grid_col_is_option() {
        if app.shortcuts.matches(Action::OptionCycle, key) {
            app.grid_ensure_row();
            app.grid_option_cycle();
            return AppAction::None;
        }
        if app.grid_row_count() > 0 {
            match key.code {
                KeyCode::Backspace | KeyCode::Delete => {
                    app.grid_option_reset();
                    return AppAction::None;
                }
                _ => {}
            }
        }
        if app.shortcuts.matches(Action::OptionSelect, key) {
            app.grid_ensure_row();
            app.open_grid_option_modal();
            return AppAction::None;
        }
    }

    // --- Lookup column intercept (select mode) ---
    if app.grid_col_is_lookup() && !app.grid_col_is_option() {
        if app.shortcuts.matches(Action::Lookup, key) {
            app.grid_ensure_row();
            return app.grid_lookup_action();
        }
    }

    // --- Select mode ---
    if app.shortcuts.matches(Action::Escape, key) {
        // Escape goes straight back to menu
        if app.grid_state.is_dirty() {
            app.save_confirm_open = true;
            app.save_modal_index = 0;
        } else {
            pop_screen_with_refresh(app, http);
        }
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::EditCycle, key) {
        if !app.grid_col_is_option() && app.grid_cell_is_editable() {
            app.grid_ensure_row();
            app.grid_f2_cycle();
        }
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::InsertRow, key) {
        app.grid_insert_row_below();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::DeleteRow, key) {
        app.open_grid_delete_confirm();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::ToggleKeyDebug, key) {
        app.toggle_key_debug();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::ThemeModal, key) {
        app.open_theme_modal();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::DebugJson, key) {
        app.copy_screen_json_to_clipboard();
        return AppAction::None;
    }
    if app.shortcuts.matches(Action::CopyUrl, key) {
        app.copy_url_to_clipboard();
        return AppAction::None;
    }

    match key.code {
        KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL) => {
            app.grid_ensure_row();
            app.grid_advance_quick_entry();
            AppAction::None
        }
        KeyCode::Up => {
            app.grid_state.move_up();
            AppAction::None
        }
        KeyCode::Down => {
            app.grid_move_down_or_append();
            AppAction::None
        }
        KeyCode::Left => {
            if app.grid_state.col > 0 {
                app.grid_state.col -= 1;
            }
            AppAction::None
        }
        KeyCode::Right => {
            if app.grid_state.col + 1 < col_count {
                app.grid_state.col += 1;
            }
            AppAction::None
        }
        KeyCode::Tab => {
            app.grid_state.tab_next(col_count, row_count);
            AppAction::None
        }
        KeyCode::BackTab => {
            app.grid_state.tab_prev(col_count);
            AppAction::None
        }
        KeyCode::Home => {
            app.grid_state.col = 0;
            AppAction::None
        }
        KeyCode::End => {
            app.grid_state.col = col_count.saturating_sub(1);
            AppAction::None
        }
        KeyCode::PageDown => {
            app.grid_state.page_down(row_count, 20);
            AppAction::None
        }
        KeyCode::PageUp => {
            app.grid_state.page_up(20);
            AppAction::None
        }
        // Type-to-edit
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
                && !app.grid_col_is_option()
                && app.grid_cell_is_editable() =>
        {
            app.grid_ensure_row();
            app.grid_begin_typing(c);
            AppAction::None
        }
        _ => AppAction::None,
    }
}

// ---------------------------------------------------------------------------
// Menu key handler
// ---------------------------------------------------------------------------

fn handle_menu_key(app: &mut App, http: &HttpClient, key: &crossterm::event::KeyEvent) -> AppAction {
    // Popup intercept — when open, all keys go to the popup
    if app.menu_popup_open {
        match key.code {
            KeyCode::Esc => {
                app.menu_popup_open = false;
            }
            KeyCode::Enter => return app.menu_popup_enter_action(),
            KeyCode::Down | KeyCode::Tab => app.menu_popup_next(),
            KeyCode::Up | KeyCode::BackTab => app.menu_popup_prev(),
            _ => {}
        }
        return AppAction::None;
    }

    // Registry-based commands
    if let Some(action) = app.shortcuts.action_for(key) {
        match action {
            Action::Quit => {
                app.request_quit();
                return AppAction::None;
            }
            Action::Refresh => return app.refresh_action(),
            Action::ToggleKeyDebug => { app.toggle_key_debug(); return AppAction::None; }
            Action::ThemeModal => { app.open_theme_modal(); return AppAction::None; }
            Action::DebugJson => { app.copy_screen_json_to_clipboard(); return AppAction::None; }
            Action::CopyUrl => { app.copy_url_to_clipboard(); return AppAction::None; }
            _ => {}
        }
    }
    if app.shortcuts.matches(Action::Escape, key) {
        if !pop_screen_with_refresh(app, http) {
            app.request_quit();
        }
        return AppAction::None;
    }

    match key.code {
        KeyCode::Enter => app.server_menu_enter_action(),
        KeyCode::Right => {
            app.server_menu_next_tab();
            AppAction::None
        }
        KeyCode::Left => {
            app.server_menu_prev_tab();
            AppAction::None
        }
        KeyCode::Tab | KeyCode::Down => {
            app.server_menu_next_item();
            AppAction::None
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.server_menu_prev_item();
            AppAction::None
        }
        _ => AppAction::None,
    }
}

// ---------------------------------------------------------------------------
// Terminal setup / teardown
// ---------------------------------------------------------------------------

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>, Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    )?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        PopKeyboardEnhancementFlags,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}
