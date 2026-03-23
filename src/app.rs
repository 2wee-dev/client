use std::time::{Duration, Instant};

pub fn open_in_browser(url: &str) -> String {
    #[cfg(target_os = "macos")]
    let cmd = std::process::Command::new("open").arg(url).spawn();

    #[cfg(target_os = "linux")]
    let cmd = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let cmd = std::process::Command::new("cmd").args(["/C", "start", url]).spawn();

    match cmd {
        Ok(_) => format!("Opened: {}", url),
        Err(e) => format!("Open error: {}", e),
    }
}

use crate::{
    date_parse::{self, DateOrder},
    number_format,
    shortcuts::ShortcutRegistry,
    time_parse,
    theme::{Theme, ThemeMode},
    ui::widgets::grid::GridState,
    ui::widgets::table::TableState,
};
use crossterm::event::KeyEvent;
use std::collections::HashMap;
use two_wee_shared::{ActionDef, ColumnDef, FieldType, Locale, OptionValues, ScreenContract, TableRow, UiStrings};

/// Maximum consecutive empty rows allowed at the bottom of the grid before auto-append stops.
const MAX_TRAILING_EMPTY_ROWS: usize = 5;

/// Apply input_mask to a character. Returns Some(transformed_char) or None if rejected.
fn apply_input_mask(ch: char, mask: Option<&str>) -> Option<char> {
    match mask {
        Some("uppercase") => Some(ch.to_uppercase().next().unwrap_or(ch)),
        Some("lowercase") => Some(ch.to_lowercase().next().unwrap_or(ch)),
        Some("digits_only") => if ch.is_ascii_digit() { Some(ch) } else { None },
        _ => Some(ch),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    Menu,
    List,
    Card,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderInputMode {
    Select,
    Edit,
}

/// F2 edit cycle — Navision-style three-stage edit entry.
///
/// Pressing F2 repeatedly cycles: Select All → Cursor End → Cursor Start → Select All …
/// Any other key resets the cycle to Idle.
/// This is a global UX pattern shared by card fields and grid cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCycle {
    Idle,
    SelectAll,
    CursorEnd,
    CursorStart,
}

impl EditCycle {
    /// Advance to the next stage in the cycle.
    pub fn next(self) -> Self {
        match self {
            Self::Idle | Self::CursorStart => Self::SelectAll,
            Self::SelectAll => Self::CursorEnd,
            Self::CursorEnd => Self::CursorStart,
        }
    }
}

/// Data for a lookup modal overlay (small-dataset inline lookup).
#[derive(Debug)]
pub struct LookupModalData {
    pub title: String,
    pub columns: Vec<ColumnDef>,
    pub all_rows: Vec<TableRow>,
    pub value_column: String,
    pub autofill: HashMap<String, String>,
    pub source_field: Option<String>,
    pub source_grid_cell: Option<(usize, usize)>,
    /// Pre-computed column widths (max of header + data, minimum 8 chars).
    pub col_widths: Vec<usize>,
    /// URL template for Ctrl+Enter drill-down into a row's detail view.
    pub on_drill: Option<String>,
}

/// Navigation intent stored on the action result modal, resolved on dismiss.
#[derive(Debug, Default)]
pub enum ActionResultNav {
    #[default]
    None,
    /// Screen already replaced inline — nothing to do on dismiss.
    HadScreen,
    /// Clear history and navigate to this URL (destructive redirect).
    Redirect(String),
    /// Push this URL onto the navigation stack (non-destructive redirect).
    Push(String),
}

/// Async actions the runtime should perform after processing a key event.
/// Some variants (FetchScreen, Quit) are not yet constructed but are part of the
/// action dispatch architecture and will be used as more features are wired up.
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppAction {
    None,
    FetchScreen(String),
    FetchCardAndPush(String),
    FetchDrilldown(String),
    /// Re-fetch the current list screen with search query appended as ?query=...
    SearchList,
    SaveCard(String, two_wee_shared::SaveChangeset),
    NewCard(String),
    DeleteCard(String, two_wee_shared::DeleteRequest),
    AuthSubmit(String, two_wee_shared::AuthRequest),
    /// Return from a lookup list: write the selected value + autofill values to the card.
    LookupReturn { value: String, autofill: HashMap<String, String> },
    /// Fetch lookup data and show as modal overlay instead of full-screen list.
    FetchModalLookup(String),
    /// Validate a lookup field value on blur (field_id, validate_url, value).
    ValidateLookup { field_id: String, url: String, value: String },
    /// Validate a grid cell lookup value on blur (row, col, validate_url, value).
    ValidateGridLookup { row: usize, col: usize, url: String, value: String },
    /// Reject a grid cell value with a client-side error (e.g. max_length exceeded).
    RejectGridCell { row: usize, col: usize, error: String },
    /// Execute a server-driven action (POST to endpoint).
    ExecuteAction { endpoint: String, request: two_wee_shared::ActionRequest },
    /// Fetch a screen URL; if the server returns 404, pop back instead of showing an error.
    FetchScreenOrPop(String),
    /// Navigate to a URL and clear all screen history (used for action redirects).
    /// Escape from the destination returns to the menu, not to the previous screen.
    FetchScreenClearHistory(String),
    Logout,
    Quit,
}

/// Result of popping the screen stack.
#[derive(Debug)]
pub enum PopResult {
    Popped,
    Empty,
    RefreshUrl(String, TableState),
}

/// An entry in the screen navigation stack (like browser history).
#[derive(Debug, Clone)]
struct ScreenStackEntry {
    screen: ScreenContract,
    table_state: TableState,
    url: Option<String>,
    search_query: String,
    needs_refresh: bool,
    menu_tab: usize,
    menu_selected: Vec<usize>,
    /// If this entry was pushed for a lookup, which field triggered it.
    lookup_source_field: Option<String>,
    /// If this entry was pushed for a grid lookup, which cell triggered it (row, col).
    lookup_source_grid_cell: Option<(usize, usize)>,
    /// Preserved card dirty state (original values before lookup was opened).
    card_original_values: HashMap<String, String>,
    /// Preserved card cursor position.
    card_field_index: usize,
    /// Preserved grid state (for grid lookup return).
    grid_state: GridState,
    /// Whether lines overlay was open.
    lines_overlay_open: bool,
    /// Preserved grid original rows (for dirty detection after lookup return).
    grid_original_rows: Vec<Vec<String>>,
    /// Whether the pushed screen was a new (unsaved) record.
    is_new_record: bool,
}

#[derive(Debug)]
pub struct App {
    pub mode: ScreenMode,
    pub message: String,
    pub message_is_error: bool,
    /// Validation error shown inline on forms (e.g. wrong password on login)
    pub form_error: String,
    pub quit_confirm_open: bool,
    pub theme_mode: ThemeMode,
    pub theme: Theme,
    pub theme_modal_open: bool,
    pub theme_modal_index: usize,
    pub header_input_mode: HeaderInputMode,
    pub header_cursor: usize,
    pub header_original_value: Option<String>,
    pub key_debug_enabled: bool,
    pub last_key_event: String,

    // --- Server-driven state ---
    /// Scheme + host only (e.g. "http://2wee.test") for resolving server-returned paths.
    pub host_url: String,
    pub current_screen: Option<ScreenContract>,
    pub current_screen_url: Option<String>,
    pub table_state: TableState,
    screen_stack: Vec<ScreenStackEntry>,

    // Card editing
    pub card_field_index: usize,
    pub card_fields_flat: Vec<CardFieldRef>,
    pub card_original_values: std::collections::HashMap<String, String>,
    pub save_confirm_open: bool,
    pub save_modal_index: usize,
    pub selection_anchor: Option<usize>,
    pub edit_cycle: EditCycle,

    // New record state
    pub is_new_record: bool,

    // Delete confirmation modal (card-level record delete)
    pub delete_confirm_open: bool,
    pub delete_modal_index: usize,

    // Grid row delete confirmation
    pub grid_delete_confirm_open: bool,
    pub grid_delete_modal_index: usize,

    // Option field modal
    pub option_modal_open: bool,
    pub option_modal_index: usize,

    // Lookup modal (small-dataset inline overlay)
    pub lookup_modal_index: usize,
    pub lookup_modal_filter: String,
    pub lookup_modal_data: Option<LookupModalData>,
    pub lookup_matcher: nucleo_matcher::Matcher,
    /// Visible row count in the lookup modal (set by the draw function each frame).
    pub lookup_modal_page_size: usize,

    // Locale and UI strings (from server)
    pub locale: Locale,
    pub ui_strings: UiStrings,
    pub date_error: Option<String>,

    // Menu state
    pub server_menu_tab: usize,
    pub server_menu_selected: Vec<usize>,
    pub menu_popup_open: bool,
    pub menu_popup_items: Vec<two_wee_shared::PopupItemDef>,
    pub menu_popup_selected: usize,

    // Auth state
    pub auth_token: Option<String>,
    pub user_display_name: Option<String>,
    /// Application name from the server (used in quit dialog etc.)
    pub app_name: String,

    // Lookup state
    pub pending_lookup_field: Option<String>,
    /// Grid cell that triggered a lookup (row, col).
    pub pending_lookup_grid_cell: Option<(usize, usize)>,
    /// Pending lookup validation to fire after the current key handler completes.
    pub pending_validate: Option<AppAction>,

    // List search
    pub list_search_query: String,

    // Quit modal selection (0 = Quit, 1 = Cancel, 2 = Log out)
    pub quit_modal_index: usize,

    // Timed messages — auto-clear after expiry
    pub message_expires: Option<Instant>,

    // Screen position of the focused field's value area (set during rendering)
    pub focused_field_rect: Option<(u16, u16, u16)>, // (x, y, width)

    // Lines overlay (Alt+L on HeaderLines cards)
    pub lines_overlay_open: bool,
    pub grid_state: GridState,
    /// Original grid rows snapshot for dirty detection and discard.
    pub grid_original_rows: Vec<Vec<String>>,

    // Action picker
    pub action_picker_open: bool,
    pub action_picker_index: usize,

    // Action form modal (Modal kind) — uses real Field objects for full card-style editing
    pub action_form_open: bool,
    pub action_form_def: Option<ActionDef>,
    pub action_form_fields: Vec<two_wee_shared::Field>,
    pub action_form_field_index: usize,
    pub action_form_input_mode: HeaderInputMode,
    pub action_form_cursor: usize,
    pub action_form_original_value: Option<String>,
    pub action_form_original_values: HashMap<String, String>,
    pub action_form_selection_anchor: Option<usize>,

    // Action confirm modal (Confirm kind)
    pub action_confirm_open: bool,
    pub action_confirm_def: Option<ActionDef>,
    pub action_confirm_index: usize,

    // Action result modal (shows server response, dismissed with Enter/Esc)
    pub action_result_open: bool,
    pub action_result_message: String,
    pub action_result_is_error: bool,
    /// Navigation intent resolved when the action result modal is dismissed.
    pub action_result_nav: ActionResultNav,

    // Shortcut registry
    pub shortcuts: ShortcutRegistry,
}

/// Reference to a field within a ScreenContract's sections.
#[derive(Debug, Clone)]
pub struct CardFieldRef {
    pub section_idx: usize,
    pub field_idx: usize,
}

impl App {
    pub fn new(host_url: String) -> Self {
        let default_theme_mode = if std::env::var("TERM_PROGRAM").as_deref() == Ok("Apple_Terminal") {
            ThemeMode::Color256
        } else {
            ThemeMode::Default
        };
        let default_theme = match default_theme_mode {
            ThemeMode::Color256 => Theme::color_256(),
            _ => Theme::default_dark(),
        };
        Self {
            mode: ScreenMode::Menu,
            message: String::from("Connecting..."),
            message_is_error: false,
            form_error: String::new(),
            quit_confirm_open: false,
            theme_mode: default_theme_mode,
            theme: default_theme,
            theme_modal_open: false,
            theme_modal_index: 0,
            header_input_mode: HeaderInputMode::Select,
            header_cursor: 0,
            header_original_value: None,
            key_debug_enabled: false,
            last_key_event: String::new(),
            host_url,
            current_screen: None,
            current_screen_url: None,
            table_state: TableState::new(),
            screen_stack: Vec::new(),
            card_field_index: 0,
            card_fields_flat: Vec::new(),
            card_original_values: std::collections::HashMap::new(),
            save_confirm_open: false,
            save_modal_index: 0,
            selection_anchor: None,
            edit_cycle: EditCycle::Idle,
            is_new_record: false,
            delete_confirm_open: false,
            delete_modal_index: 0,
            grid_delete_confirm_open: false,
            grid_delete_modal_index: 1, // default to Cancel
            option_modal_open: false,
            option_modal_index: 0,
            lookup_modal_index: 0,
            lookup_modal_filter: String::new(),
            lookup_modal_data: None,
            lookup_matcher: nucleo_matcher::Matcher::new(nucleo_matcher::Config::DEFAULT),
            lookup_modal_page_size: 0,
            locale: Locale::default(),
            ui_strings: Self::default_ui_strings(),
            date_error: None,
            server_menu_tab: 0,
            server_menu_selected: Vec::new(),
            menu_popup_open: false,
            menu_popup_items: Vec::new(),
            menu_popup_selected: 0,
            auth_token: None,
            user_display_name: None,
            app_name: String::new(),
            pending_lookup_field: None,
            pending_lookup_grid_cell: None,
            pending_validate: None,
            list_search_query: String::new(),
            quit_modal_index: 0,
            message_expires: None,
            focused_field_rect: None,
            lines_overlay_open: false,
            grid_state: GridState::new(),
            grid_original_rows: Vec::new(),
            action_picker_open: false,
            action_picker_index: 0,
            action_form_open: false,
            action_form_def: None,
            action_form_fields: Vec::new(),
            action_form_field_index: 0,
            action_form_input_mode: HeaderInputMode::Select,
            action_form_cursor: 0,
            action_form_original_value: None,
            action_form_original_values: HashMap::new(),
            action_form_selection_anchor: None,
            action_confirm_open: false,
            action_confirm_def: None,
            action_confirm_index: 0,
            action_result_open: false,
            action_result_message: String::new(),
            action_result_is_error: false,
            action_result_nav: ActionResultNav::None,
            shortcuts: ShortcutRegistry::new(),
        }
    }

    /// Set a transient info message that auto-clears after the given duration.
    pub fn set_timed_message(&mut self, msg: String, duration: Duration) {
        self.message = msg;
        self.message_is_error = false;
        self.message_expires = Some(Instant::now() + duration);
    }

    /// Set a persistent validation error message (red error bar).
    pub fn set_error_message(&mut self, msg: String) {
        self.message = msg;
        self.message_is_error = true;
        self.message_expires = None;
    }

    /// Clear the current message and error state.
    pub fn clear_message(&mut self) {
        self.message.clear();
        self.message_is_error = false;
    }

    /// Check if the timed message has expired and clear it.
    pub fn tick_message(&mut self) {
        if let Some(expires) = self.message_expires {
            if Instant::now() >= expires {
                self.clear_message();
                self.message_expires = None;
            }
        }
    }

    /// English fallback — used before the server sends its own ui_strings.
    fn default_ui_strings() -> UiStrings {
        UiStrings {
            save_confirm_title: "Unsaved changes".into(),
            save_confirm_message: "You have unsaved changes.".into(),
            save_confirm_save: "Save and continue".into(),
            save_confirm_discard: "Discard changes".into(),
            save_confirm_cancel: "Stay here".into(),
            quit_title: "Quit".into(),
            quit_message: "Quit 2wee?".into(),
            quit_yes: "Quit".into(),
            quit_no: "Cancel".into(),
            logout: "Log out".into(),
            created: "Created.".into(),
            deleted: "Deleted.".into(),
            saved: "Saved.".into(),
            saving: "Saving...".into(),
            loading: "Loading...".into(),
            cancelled: "Cancelled.".into(),
            no_changes: "No changes to save.".into(),
            copied: "Copied".into(),
            error_prefix: "Error".into(),
            save_error_prefix: "Save error".into(),
            login_error: "Login failed".into(),
            server_unavailable: "Server unavailable".into(),
            connecting: "Connecting...".into(),
        }
    }

    // ----- Screen contract methods -----

    pub fn set_screen(&mut self, screen: ScreenContract) {
        use two_wee_shared::LayoutKind;

        if let Some(ref status) = screen.status {
            self.message = status.clone();
        } else {
            self.message.clear();
        }
        self.message_is_error = false;
        if let Some(ref locale) = screen.locale {
            self.locale = locale.clone();
        }
        if let Some(ref ui_strings) = screen.ui_strings {
            self.ui_strings = ui_strings.clone();
        }
        if let Some(ref name) = screen.user_display_name {
            self.user_display_name = Some(name.clone());
        }
        // Capture app name from login screen title or menu top_left
        if screen.auth_action.is_some() && !screen.title.is_empty() {
            self.app_name = screen.title.clone();
        }
        if let Some(ref menu) = screen.menu {
            if let Some(ref top_left) = menu.top_left {
                self.app_name = top_left.clone();
            }
        }

        match screen.layout {
            LayoutKind::List => {
                self.mode = ScreenMode::List;
                self.table_state = TableState::new();
                self.grid_original_rows.clear();
            }
            LayoutKind::Card => {
                self.mode = ScreenMode::Card;
                self.header_input_mode = HeaderInputMode::Select;
                self.header_original_value = None;
                self.lines_overlay_open = false;
                self.rebuild_card_fields(&screen);
                self.card_field_index = self.resolve_initial_focus(&screen);
                self.snapshot_card_values(&screen);
                self.grid_original_rows.clear();
                self.save_confirm_open = false;
                self.delete_confirm_open = false;
                self.is_new_record = false;

                // Login forms: auto-enter edit mode on first field
                if screen.auth_action.is_some() && !self.card_fields_flat.is_empty() {
                    self.header_original_value = Some(String::new());
                    self.header_input_mode = HeaderInputMode::Edit;
                    self.header_cursor = 0;
                }
            }
            LayoutKind::HeaderLines => {
                self.mode = ScreenMode::Card;
                self.header_input_mode = HeaderInputMode::Select;
                self.header_original_value = None;
                self.rebuild_card_fields(&screen);
                self.card_field_index = self.resolve_initial_focus(&screen);
                self.snapshot_card_values(&screen);
                self.snapshot_grid_rows(&screen);
                self.save_confirm_open = false;
                self.delete_confirm_open = false;
                self.is_new_record = false;
                self.table_state = TableState::new();
                self.grid_state = GridState::new();
                self.lines_overlay_open = screen.lines_open;
            }
            LayoutKind::Grid => {
                self.mode = ScreenMode::Grid;
                self.snapshot_grid_rows(&screen);
                self.table_state = TableState::new();
                self.grid_state = GridState::new();
                self.grid_original_rows.clear();
                self.save_confirm_open = false;
                self.delete_confirm_open = false;
                self.is_new_record = false;
            }
            LayoutKind::Menu => {
                self.mode = ScreenMode::Menu;
                self.list_search_query.clear();
                self.grid_original_rows.clear();
                let tab_count = screen
                    .menu
                    .as_ref()
                    .map(|m| m.tabs.len())
                    .unwrap_or(0);
                self.server_menu_tab = 0;
                self.server_menu_selected = vec![0; tab_count];
            }
        }

        self.current_screen = Some(screen);
    }

    pub fn push_and_set_screen(&mut self, screen: ScreenContract) {
        if let Some(prev) = self.current_screen.take() {
            let search_query = std::mem::take(&mut self.list_search_query);
            let lookup_source_field = self.pending_lookup_field.take();
            let lookup_source_grid_cell = self.pending_lookup_grid_cell.take();
            self.screen_stack.push(ScreenStackEntry {
                screen: prev,
                table_state: self.table_state.clone(),
                url: self.current_screen_url.take(),
                search_query,
                needs_refresh: false,
                menu_tab: self.server_menu_tab,
                menu_selected: self.server_menu_selected.clone(),
                lookup_source_field,
                lookup_source_grid_cell,
                card_original_values: self.card_original_values.clone(),
                card_field_index: self.card_field_index,
                grid_state: self.grid_state.clone(),
                lines_overlay_open: self.lines_overlay_open,
                grid_original_rows: self.grid_original_rows.clone(),
                is_new_record: self.is_new_record,
            });
        }
        self.set_screen(screen);
    }

    /// Pre-select the row matching the current lookup field value on a full-screen lookup list.
    /// Call after push_and_set_screen when opening a lookup.
    pub fn preselect_lookup_row(&mut self) {
        // Get the lookup source field value from the parent screen on the stack
        let source_value = self.screen_stack.last()
            .and_then(|entry| {
                let field_id = entry.lookup_source_field.as_ref()?;
                // Find the value in the parent screen's fields
                for section in &entry.screen.sections {
                    for field in &section.fields {
                        if field.id == *field_id {
                            return Some(field.value.clone());
                        }
                    }
                }
                None
            });
        let value = match source_value {
            Some(v) if !v.is_empty() => v,
            _ => return,
        };
        // Find the matching row in the current screen's value_column
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return,
        };
        let lines = match &screen.lines {
            Some(l) => l,
            None => return,
        };
        let value_col = match &lines.value_column {
            Some(vc) => vc,
            None => return,
        };
        let col_idx = match lines.columns.iter().position(|c| c.id == *value_col) {
            Some(i) => i,
            None => return,
        };
        if let Some(row_idx) = lines.rows.iter().position(|r| {
            r.values.get(col_idx).map(|v| v == &value).unwrap_or(false)
        }) {
            self.table_state.selected = row_idx;
        }
    }

    pub fn clear_screen_history(&mut self) {
        self.screen_stack.clear();
    }

    pub fn mark_parent_needs_refresh(&mut self) {
        if let Some(entry) = self.screen_stack.last_mut() {
            entry.needs_refresh = true;
        }
    }

    pub fn pop_screen(&mut self) -> PopResult {
        let entry = match self.screen_stack.pop() {
            Some(e) => e,
            None => return PopResult::Empty,
        };

        self.current_screen_url = entry.url.clone();
        self.list_search_query = entry.search_query;

        if entry.needs_refresh {
            if let Some(url) = entry.url {
                return PopResult::RefreshUrl(url, entry.table_state);
            }
        }

        let saved_table_state = entry.table_state;
        let saved_menu_tab = entry.menu_tab;
        let saved_menu_selected = entry.menu_selected;
        let saved_card_original_values = entry.card_original_values;
        let saved_card_field_index = entry.card_field_index;
        let saved_grid_state = entry.grid_state;
        let saved_lines_overlay_open = entry.lines_overlay_open;
        let saved_grid_original_rows = entry.grid_original_rows;
        let saved_is_new_record = entry.is_new_record;
        self.set_screen(entry.screen);

        // Restore table state after set_screen (which resets it)
        self.table_state = saved_table_state;

        // Restore menu state after set_screen (which resets it)
        self.server_menu_tab = saved_menu_tab;
        self.server_menu_selected = saved_menu_selected;

        // Restore card state (dirty tracking + cursor) after set_screen reset them
        if !saved_card_original_values.is_empty() {
            self.card_original_values = saved_card_original_values;
            self.card_field_index = saved_card_field_index.min(
                self.card_fields_flat.len().saturating_sub(1),
            );
        }

        // Restore is_new_record (set_screen resets it to false)
        self.is_new_record = saved_is_new_record;

        // Restore grid state and overlay
        if saved_lines_overlay_open {
            self.grid_state = saved_grid_state;
            self.grid_original_rows = saved_grid_original_rows;
            self.lines_overlay_open = true;
        }

        PopResult::Popped
    }

    fn rebuild_card_fields(&mut self, screen: &ScreenContract) {
        self.card_fields_flat.clear();
        for (si, section) in screen.sections.iter().enumerate() {
            for (fi, field) in section.fields.iter().enumerate() {
                if field.field_type == two_wee_shared::FieldType::Separator {
                    continue;
                }
                self.card_fields_flat.push(CardFieldRef {
                    section_idx: si,
                    field_idx: fi,
                });
            }
        }
    }

    /// Initial focus: field with focus=true, or first quick_entry + editable field.
    fn resolve_initial_focus(&self, screen: &ScreenContract) -> usize {
        // Explicit focus field
        if let Some(idx) = self.card_fields_flat.iter().position(|r| {
            screen.sections.get(r.section_idx)
                .and_then(|s| s.fields.get(r.field_idx))
                .map(|f| f.focus)
                .unwrap_or(false)
        }) {
            return idx;
        }
        // Default: first quick_entry + editable field
        self.card_fields_flat.iter().position(|r| {
            screen.sections.get(r.section_idx)
                .and_then(|s| s.fields.get(r.field_idx))
                .map(|f| f.quick_entry && f.editable)
                .unwrap_or(false)
        }).unwrap_or(0)
    }

    fn snapshot_card_values(&mut self, screen: &ScreenContract) {
        self.card_original_values.clear();
        for section in &screen.sections {
            for field in &section.fields {
                if field.editable {
                    self.card_original_values
                        .insert(field.id.clone(), field.value.clone());
                }
            }
        }
    }

    fn snapshot_grid_rows(&mut self, screen: &ScreenContract) {
        self.grid_original_rows = screen
            .lines
            .as_ref()
            .map(|l| l.rows.iter().map(|r| r.values.clone()).collect())
            .unwrap_or_default();
    }

    /// Restore all editable field values from the snapshot, undoing any changes.
    pub fn discard_card_changes(&mut self) {
        if let Some(ref mut screen) = self.current_screen {
            for section in &mut screen.sections {
                for field in &mut section.fields {
                    if let Some(original) = self.card_original_values.get(&field.id) {
                        field.value = original.clone();
                    }
                }
            }
            // Restore grid lines from snapshot
            if let Some(ref mut lines) = screen.lines {
                lines.rows.clear();
                for (i, values) in self.grid_original_rows.iter().enumerate() {
                    lines.rows.push(two_wee_shared::TableRow {
                        index: i,
                        values: values.clone(),
                    });
                }
                lines.row_count = lines.rows.len();
            }
        }
        self.header_input_mode = HeaderInputMode::Select;
        self.header_original_value = None;
        self.grid_state = GridState::new();
    }

    pub fn is_card_dirty(&self) -> bool {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return false,
        };
        if self.is_new_record {
            // New record is dirty if any editable field differs from its initial value
            for section in &screen.sections {
                for field in &section.fields {
                    if field.editable {
                        let initial = self.card_original_values.get(&field.id)
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        if field.value != initial {
                            return true;
                        }
                    }
                }
            }
            return self.is_grid_dirty();
        }
        // Check card header fields
        for section in &screen.sections {
            for field in &section.fields {
                if field.editable {
                    if let Some(original) = self.card_original_values.get(&field.id) {
                        if *original != field.value {
                            return true;
                        }
                    }
                }
            }
        }
        // Check grid lines
        if self.is_grid_dirty() {
            return true;
        }
        false
    }

    /// Check if the grid lines differ from the original snapshot.
    pub fn is_grid_dirty(&self) -> bool {
        let rows = match self.current_screen.as_ref().and_then(|s| s.lines.as_ref()) {
            Some(l) => &l.rows,
            None => return !self.grid_original_rows.is_empty(),
        };
        if rows.len() != self.grid_original_rows.len() {
            return true;
        }
        rows.iter().zip(self.grid_original_rows.iter())
            .any(|(current, original)| current.values != *original)
    }

    pub fn is_field_dirty(&self, field_id: &str) -> bool {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return false,
        };
        for section in &screen.sections {
            for field in &section.fields {
                if field.id == field_id {
                    if let Some(original) = self.card_original_values.get(&field.id) {
                        return *original != field.value;
                    }
                    return false;
                }
            }
        }
        false
    }

    pub fn build_changeset(&self) -> Option<two_wee_shared::SaveChangeset> {
        let screen = self.current_screen.as_ref()?;
        let mut changes = std::collections::HashMap::new();

        let record_id = if self.is_new_record {
            String::new()
        } else {
            screen.sections.first()
                .and_then(|s| s.fields.first())
                .map(|f| f.value.clone())
                .unwrap_or_default()
        };

        for section in &screen.sections {
            for field in &section.fields {
                if field.editable {
                    if self.is_new_record {
                        // For new records, include all editable fields
                        changes.insert(field.id.clone(), field.value.clone());
                    } else if let Some(original) = self.card_original_values.get(&field.id) {
                        if *original != field.value {
                            changes.insert(field.id.clone(), field.value.clone());
                        }
                    }
                }
            }
        }

        // Build cleaned-up grid lines for save
        let lines = self.build_save_lines();
        let grid_dirty = self.is_grid_dirty();

        if changes.is_empty() && !grid_dirty && !self.is_new_record {
            return None;
        }

        let screen_id = screen.screen_id.clone();

        Some(two_wee_shared::SaveChangeset {
            screen_id,
            record_id,
            changes,
            action: if self.is_new_record { Some("create".into()) } else { None },
            lines,
        })
    }

    /// Build cleaned grid lines for save:
    /// - Rows without a "no" value get their type set to ""
    /// - Trailing fully-empty rows are trimmed
    fn build_save_lines(&self) -> Vec<Vec<String>> {
        let screen = match self.current_screen.as_ref() {
            Some(s) => s,
            None => return vec![],
        };
        let table = match screen.lines.as_ref() {
            Some(l) => l,
            None => return vec![],
        };
        // Find column indices
        let type_col = table.columns.iter().position(|c| c.id == "type");
        let no_col = table.columns.iter().position(|c| c.id == "no");

        let mut lines: Vec<Vec<String>> = table.rows.iter().map(|r| {
            let mut values = r.values.clone();
            // If "no" column is empty, clear type (orphan type or comment/spacer row)
            if let (Some(tc), Some(nc)) = (type_col, no_col) {
                let no_empty = values.get(nc).map(|v| v.is_empty()).unwrap_or(true);
                if no_empty {
                    if let Some(v) = values.get_mut(tc) {
                        *v = String::new();
                    }
                }
            }
            values
        }).collect();

        // Trim trailing fully-empty rows
        while let Some(last) = lines.last() {
            if last.iter().all(|v| v.is_empty()) {
                lines.pop();
            } else {
                break;
            }
        }
        lines
    }

    pub fn card_save_url(&self) -> Option<String> {
        let action_key = if self.is_new_record { "create" } else { "save" };
        self.resolve_action(action_key)
    }

    /// True when the current screen is a login form (has auth_action).
    pub fn is_auth_screen(&self) -> bool {
        self.current_screen
            .as_ref()
            .and_then(|s| s.auth_action.as_ref())
            .is_some()
    }

    /// Whether the current screen has lines (HeaderLines layout).
    pub fn has_lines(&self) -> bool {
        self.current_screen
            .as_ref()
            .and_then(|s| s.lines.as_ref())
            .is_some()
            && self.mode == ScreenMode::Card
    }

    // ----- Screen actions -----

    pub fn has_screen_actions(&self) -> bool {
        self.current_screen
            .as_ref()
            .map(|s| !s.screen_actions.is_empty())
            .unwrap_or(false)
    }

    pub fn screen_actions(&self) -> &[ActionDef] {
        self.current_screen
            .as_ref()
            .map(|s| s.screen_actions.as_slice())
            .unwrap_or(&[])
    }

    pub fn open_action_picker(&mut self) {
        self.action_picker_open = true;
        self.action_picker_index = 0;
    }

    pub fn close_action_picker(&mut self) {
        self.action_picker_open = false;
    }

    pub fn open_action_form(&mut self, def: ActionDef) {
        use two_wee_shared::Field;
        let mut fields = Vec::new();
        let mut originals = HashMap::new();
        for af in &def.fields {
            let field = Field {
                id: af.id.clone(),
                label: af.label.clone(),
                field_type: af.field_type.clone(),
                value: af.value.clone(),
                editable: true,
                width: None,
                validation: af.validation.clone(),
                color: None,
                bold: false,
                options: af.options.clone(),
                lookup: None,
                placeholder: af.placeholder.clone(),
                rows: af.rows,
                true_label: None, false_label: None, true_color: None, false_color: None,
                quick_entry: true, focus: false,
            };
            originals.insert(af.id.clone(), af.value.clone());
            fields.push(field);
        }
        self.action_form_fields = fields;
        self.action_form_original_values = originals;
        self.action_form_field_index = 0;
        self.action_form_input_mode = HeaderInputMode::Select;
        self.action_form_cursor = 0;
        self.action_form_original_value = None;
        self.action_form_selection_anchor = None;
        self.action_form_def = Some(def);
        self.action_form_open = true;
    }

    pub fn close_action_form(&mut self) {
        self.action_form_open = false;
        self.action_form_def = None;
        self.action_form_fields.clear();
        self.action_form_original_values.clear();
        self.action_form_input_mode = HeaderInputMode::Select;
    }

    pub fn open_action_confirm(&mut self, def: ActionDef) {
        self.action_confirm_index = 0;
        self.action_confirm_def = Some(def);
        self.action_confirm_open = true;
    }

    pub fn close_action_confirm(&mut self) {
        self.action_confirm_open = false;
        self.action_confirm_def = None;
    }

    pub fn build_action_request(&self, action_def: &ActionDef) -> two_wee_shared::ActionRequest {
        let screen_title = self.current_screen.as_ref().map(|s| s.title.clone()).unwrap_or_default();
        let record_id = self.current_screen.as_ref()
            .map(|s| s.record_id.clone())
            .filter(|id| !id.is_empty());
        two_wee_shared::ActionRequest {
            action_id: action_def.id.clone(),
            screen_title,
            record_id,
            fields: HashMap::new(),
        }
    }

    pub fn action_form_current_field(&self) -> Option<&two_wee_shared::Field> {
        self.action_form_fields.get(self.action_form_field_index)
    }

    pub fn action_form_current_field_mut(&mut self) -> Option<&mut two_wee_shared::Field> {
        self.action_form_fields.get_mut(self.action_form_field_index)
    }

    pub fn action_form_is_field_dirty(&self, field_id: &str) -> bool {
        self.action_form_fields.iter()
            .find(|f| f.id == field_id)
            .map(|f| {
                self.action_form_original_values.get(field_id)
                    .map(|orig| orig != &f.value)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    pub fn action_form_begin_edit(&mut self) {
        if let Some(field) = self.action_form_fields.get(self.action_form_field_index) {
            if field.field_type == FieldType::Option { return; }
            self.action_form_original_value = Some(field.value.clone());
            self.action_form_cursor = field.value.len();
            self.action_form_input_mode = HeaderInputMode::Edit;
            self.action_form_selection_anchor = None;
        }
    }


    pub fn action_form_insert_char(&mut self, ch: char) {
        // Read input_mask before mutable borrow
        let mask = self.action_form_fields.get(self.action_form_field_index)
            .and_then(|f| f.validation.as_ref())
            .and_then(|v| v.input_mask.as_deref())
            .map(|s| s.to_string());
        let ch = match apply_input_mask(ch, mask.as_deref()) {
            Some(c) => c,
            None => return,
        };
        if let Some(field) = self.action_form_fields.get_mut(self.action_form_field_index) {
            let byte_pos = field.value.char_indices()
                .nth(self.action_form_cursor)
                .map(|(i, _)| i)
                .unwrap_or(field.value.len());
            field.value.insert(byte_pos, ch);
            self.action_form_cursor += 1;
        }
    }

    pub fn action_form_delete_char(&mut self) {
        if self.action_form_cursor > 0 {
            if let Some(field) = self.action_form_fields.get_mut(self.action_form_field_index) {
                let byte_pos = field.value.char_indices()
                    .nth(self.action_form_cursor - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let end = field.value.char_indices()
                    .nth(self.action_form_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(field.value.len());
                field.value.replace_range(byte_pos..end, "");
                self.action_form_cursor -= 1;
            }
        }
    }

    pub fn action_form_confirm_edit(&mut self) {
        self.action_form_input_mode = HeaderInputMode::Select;
        self.action_form_cursor = 0;
        self.action_form_original_value = None;
        self.action_form_selection_anchor = None;
    }

    pub fn action_form_revert_edit(&mut self) {
        if let Some(original) = self.action_form_original_value.take() {
            if let Some(field) = self.action_form_fields.get_mut(self.action_form_field_index) {
                field.value = original;
            }
        }
        self.action_form_input_mode = HeaderInputMode::Select;
        self.action_form_cursor = 0;
        self.action_form_selection_anchor = None;
    }

    pub fn action_form_next_field(&mut self) {
        let count = self.action_form_fields.len();
        if count > 0 && self.action_form_field_index + 1 < count {
            self.action_form_field_index += 1;
        }
    }

    pub fn action_form_prev_field(&mut self) {
        if self.action_form_field_index > 0 {
            self.action_form_field_index -= 1;
        }
    }

    pub fn action_form_selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.action_form_selection_anchor?;
        let cursor = self.action_form_cursor;
        if anchor == cursor { return None; }
        Some((anchor.min(cursor), anchor.max(cursor)))
    }

    pub fn action_form_is_option_field(&self) -> bool {
        self.action_form_current_field()
            .map(|f| f.field_type == FieldType::Option)
            .unwrap_or(false)
    }

    pub fn action_form_is_boolean_field(&self) -> bool {
        self.action_form_current_field()
            .map(|f| f.field_type == FieldType::Boolean)
            .unwrap_or(false)
    }

    pub fn action_form_bool_toggle(&mut self) {
        if let Some(field) = self.action_form_current_field_mut() {
            if field.field_type != FieldType::Boolean { return; }
            field.value = if field.value == "true" { "false".to_string() } else { "true".to_string() };
        }
    }

    fn action_form_option_keys(&self) -> Vec<String> {
        self.action_form_current_field()
            .and_then(|f| f.options.as_ref())
            .map(|opts| match opts {
                OptionValues::Simple(vals) => vals.clone(),
                OptionValues::Labeled(pairs) => pairs.iter().map(|p| p.value.clone()).collect(),
            })
            .unwrap_or_default()
    }


    pub fn action_form_option_cycle(&mut self) {
        let keys = self.action_form_option_keys();
        if keys.is_empty() { return; }
        let current = self.action_form_current_field().map(|f| f.value.as_str()).unwrap_or("");
        let idx = keys.iter().position(|k| k == current).unwrap_or(0);
        let next_idx = (idx + 1) % keys.len();
        if let Some(field) = self.action_form_current_field_mut() {
            field.value = keys[next_idx].clone();
        }
    }

    pub fn build_action_request_with_form(&self, action_def: &ActionDef) -> two_wee_shared::ActionRequest {
        let mut req = self.build_action_request(action_def);
        for field in &self.action_form_fields {
            req.fields.insert(field.id.clone(), field.value.clone());
        }
        req
    }

    // ----- Grid cell editing (lines overlay) -----

    /// Get the current cell value from the lines table.
    pub fn grid_cell_value(&self) -> Option<&str> {
        let screen = self.current_screen.as_ref()?;
        let lines = screen.lines.as_ref()?;
        let row = lines.rows.get(self.grid_state.row)?;
        row.values.get(self.grid_state.col).map(|s| s.as_str())
    }

    /// Get a mutable reference to the current cell value.
    fn grid_cell_value_mut(&mut self) -> Option<&mut String> {
        let screen = self.current_screen.as_mut()?;
        let lines = screen.lines.as_mut()?;
        let row = lines.rows.get_mut(self.grid_state.row)?;
        row.values.get_mut(self.grid_state.col)
    }

    /// Confirm edit: record dirty state, exit edit mode.
    /// For decimal cells: parse locale input → raw format and normalize.
    pub fn grid_confirm_edit(&mut self) {
        if !self.grid_state.editing {
            return;
        }
        self.grid_state.editing = false;
        self.grid_state.selection_anchor = None;
        if self.message_is_error {
            self.clear_message();
        }

        // For decimal columns, parse locale → raw and normalize
        if self.grid_col_is_decimal() {
            self.grid_normalize_decimal_cell();
        }

        // For date columns, parse shorthand and normalize
        if self.grid_col_is_date() {
            self.grid_normalize_date_cell();
        }

        // Record the original value for dirty tracking
        if let Some(original) = self.grid_state.original_value.take() {
            let key = (self.grid_state.row, self.grid_state.col);
            if !self.grid_state.dirty_cells.contains_key(&key) {
                self.grid_state.dirty_cells.insert(key, original);
            }
        }

        // Recalculate line_amount if quantity or unit_price changed
        self.grid_recalculate_line_amount();
        self.grid_recalculate_totals();
    }

    /// Parse locale-formatted decimal in the current cell and normalize to raw format.
    /// E.g. "5,5" → "5.50", "1.200,50" → "1200.50"
    fn grid_normalize_decimal_cell(&mut self) {
        let dec_sep = self.locale.decimal_separator.clone();
        let thou_sep = self.locale.thousand_separator.clone();
        if let Some(cell) = self.grid_cell_value_mut() {
            if cell.is_empty() {
                *cell = "0.00".to_string();
                return;
            }
            match number_format::parse_locale_decimal(cell, &dec_sep, &thou_sep) {
                Ok(raw) if raw.is_empty() => {
                    *cell = "0.00".to_string();
                }
                Ok(raw) => {
                    // Normalize to 2 decimal places
                    if let Ok(v) = raw.parse::<f64>() {
                        *cell = format!("{:.2}", v);
                    } else {
                        *cell = raw;
                    }
                }
                Err(()) => {
                    // Invalid input — revert to 0
                    *cell = "0.00".to_string();
                }
            }
        }
    }

    /// Parse date shorthand in the current grid cell and normalize to full date format.
    fn grid_normalize_date_cell(&mut self) {
        let order = date_parse::DateOrder::from_format(&self.locale.date_format);
        let reference = self.reference_date();
        if let Some(cell) = self.grid_cell_value_mut() {
            if cell.trim().is_empty() {
                return;
            }
            match date_parse::parse_date_shorthand(cell, reference, order) {
                Ok(date) => {
                    *cell = date_parse::format_date(date, order);
                }
                Err(msg) => {
                    self.set_error_message(format!("Date error: {}", msg));
                }
            }
        }
    }

    /// Evaluate all formula columns for the current grid row.
    pub fn grid_recalculate_line_amount(&mut self) {
        let row = self.grid_state.row;
        let screen = match self.current_screen.as_ref() {
            Some(s) => s,
            None => return,
        };
        let lines = match screen.lines.as_ref() {
            Some(l) => l,
            None => return,
        };
        let row_data = match lines.rows.get(row) {
            Some(r) => r,
            None => return,
        };

        // Build column values map for formula evaluation
        let mut col_values: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
        for (ci, col) in lines.columns.iter().enumerate() {
            let val: f64 = row_data.values.get(ci)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            col_values.insert(&col.id, val);
        }

        // Collect formula columns and their results
        let mut updates: Vec<(usize, String)> = Vec::new();
        for (ci, col) in lines.columns.iter().enumerate() {
            if let Some(ref formula) = col.formula {
                let result = crate::formula::evaluate(formula, &col_values);
                let decimals = col.validation.as_ref()
                    .and_then(|v| v.decimals)
                    .unwrap_or(2) as usize;
                updates.push((ci, format!("{:.prec$}", result, prec = decimals)));
            }
        }

        // Apply results
        if updates.is_empty() { return; }
        let screen = self.current_screen.as_mut().unwrap();
        let lines = screen.lines.as_mut().unwrap();
        if let Some(row_data) = lines.rows.get_mut(row) {
            for (ci, value) in updates {
                if ci < row_data.values.len() {
                    let key = (row, ci);
                    if !self.grid_state.dirty_cells.contains_key(&key) {
                        self.grid_state.dirty_cells.insert(key, row_data.values[ci].clone());
                    }
                    row_data.values[ci] = value;
                }
            }
        }
    }

    /// Recalculate totals that have a `source_column` from current grid data.
    pub fn grid_recalculate_totals(&mut self) {
        let screen = match self.current_screen.as_mut() {
            Some(s) => s,
            None => return,
        };
        let lines = match screen.lines.as_ref() {
            Some(l) => l,
            None => return,
        };

        // Collect (total_index, column_index, aggregate, decimals) for live totals
        let live: Vec<(usize, usize, String, u8)> = screen.totals.iter().enumerate()
            .filter_map(|(ti, t)| {
                let col_id = t.source_column.as_deref()?;
                let col_idx = lines.columns.iter().position(|c| c.id == col_id)?;
                let agg = t.aggregate.clone().unwrap_or_else(|| "sum".to_string());
                let dec = t.decimals.unwrap_or(2);
                Some((ti, col_idx, agg, dec))
            })
            .collect();

        if live.is_empty() { return; }

        // Compute aggregated values
        let results: Vec<(usize, String)> = live.iter().map(|(ti, col_idx, agg, dec)| {
            let values: Vec<f64> = lines.rows.iter()
                .filter_map(|row| row.values.get(*col_idx))
                .filter(|v| !v.is_empty())
                .filter_map(|v| v.parse::<f64>().ok())
                .collect();

            let result = match agg.as_str() {
                "count" => values.len() as f64,
                "avg" => if values.is_empty() { 0.0 } else { values.iter().sum::<f64>() / values.len() as f64 },
                "min" => values.iter().copied().fold(f64::INFINITY, f64::min),
                "max" => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                _ /* sum */ => values.iter().sum::<f64>(),
            };
            let result = if !result.is_finite() { 0.0 } else { result };

            let formatted = number_format::format_decimal(
                &format!("{:.prec$}", result, prec = *dec as usize),
                &self.locale.decimal_separator,
                &self.locale.thousand_separator,
                Some(*dec),
            );
            (*ti, formatted)
        }).collect();

        // Write back
        for (ti, formatted) in results {
            screen.totals[ti].value = formatted;
        }
    }

    /// Revert cell to original value, exit edit mode.
    pub fn grid_revert_edit(&mut self) {
        if let Some(original) = self.grid_state.original_value.take() {
            if let Some(cell) = self.grid_cell_value_mut() {
                *cell = original;
            }
        }
        self.grid_state.editing = false;
        self.grid_state.selection_anchor = None;
        if self.message_is_error {
            self.clear_message();
        }
    }

    /// Delete the selected range in the grid cell (if any).
    /// Returns true if a non-empty selection was deleted.
    fn grid_delete_selection(&mut self) -> bool {
        let anchor = match self.grid_state.selection_anchor.take() {
            Some(a) => a,
            None => return false,
        };
        let start = anchor.min(self.grid_state.cursor);
        let end = anchor.max(self.grid_state.cursor);
        if start == end {
            return false;
        }
        if let Some(cell) = self.grid_cell_value_mut() {
            let (start_byte, end_byte) = char_byte_range(cell, start, end);
            cell.replace_range(start_byte..end_byte, "");
        }
        self.grid_state.cursor = start;
        true
    }

    /// Check if a character is valid for the current decimal grid cell.
    fn grid_decimal_char_allowed(&self, ch: char) -> bool {
        if ch.is_ascii_digit() {
            return true;
        }
        if ch == '-' {
            return true;
        }
        // Allow locale decimal separator
        let dec_sep = &self.locale.decimal_separator;
        if !dec_sep.is_empty() && ch.to_string() == *dec_sep {
            return true;
        }
        false
    }

    /// Convert raw value ("5.00") to locale format ("5,00") in the cell for editing.
    fn grid_raw_to_locale_edit(&mut self) {
        let dec_sep = self.locale.decimal_separator.clone();
        if let Some(cell) = self.grid_cell_value_mut() {
            // Simple replacement: "." → locale decimal separator (no thousand seps during edit)
            *cell = cell.replace('.', &dec_sep);
        }
    }

    /// Start typing in a grid cell. Two cases:
    /// - Not editing: enter edit mode, replace entire value with the typed char.
    /// - Already editing (e.g. F2 select-all): delete selection, insert at cursor.
    pub fn grid_begin_typing(&mut self, ch: char) {
        // For decimal columns, filter invalid chars
        if self.grid_col_is_decimal() && !self.grid_decimal_char_allowed(ch) {
            return;
        }
        if !self.grid_state.editing {
            if let Some(value) = self.grid_cell_value() {
                self.grid_state.original_value = Some(value.to_string());
            }
            self.grid_state.editing = true;
            self.grid_state.selection_anchor = None;
            if let Some(cell) = self.grid_cell_value_mut() {
                cell.clear();
                cell.push(ch);
            }
            self.grid_state.cursor = 1;
        } else {
            self.grid_delete_selection();
            self.grid_insert_char(ch);
        }
    }

    /// Insert a character at the cursor in the current grid cell.
    pub fn grid_insert_char(&mut self, ch: char) {
        // For decimal columns, filter invalid chars
        if self.grid_col_is_decimal() && !self.grid_decimal_char_allowed(ch) {
            return;
        }
        // Apply input_mask from column validation
        let mask = self.grid_current_col_def()
            .and_then(|c| c.validation.as_ref())
            .and_then(|v| v.input_mask.as_deref())
            .map(|s| s.to_string());
        let ch = match apply_input_mask(ch, mask.as_deref()) {
            Some(c) => c,
            None => return,
        };
        self.grid_delete_selection();
        let cursor = self.grid_state.cursor;
        if let Some(cell) = self.grid_cell_value_mut() {
            let byte_idx = char_to_byte(cell, cursor);
            cell.insert(byte_idx, ch);
        }
        self.grid_state.cursor += 1;
    }

    /// Backspace in grid cell.
    pub fn grid_backspace(&mut self) {
        if self.grid_delete_selection() {
            return;
        }
        if self.grid_state.cursor == 0 {
            return;
        }
        let cursor = self.grid_state.cursor;
        if let Some(cell) = self.grid_cell_value_mut() {
            let (start, end) = char_byte_range(cell, cursor - 1, cursor);
            cell.replace_range(start..end, "");
        }
        self.grid_state.cursor -= 1;
    }

    /// Delete character at cursor in grid cell.
    pub fn grid_delete(&mut self) {
        if self.grid_delete_selection() {
            return;
        }
        let cursor = self.grid_state.cursor;
        if let Some(cell) = self.grid_cell_value_mut() {
            if cursor >= cell.chars().count() {
                return;
            }
            let (start, end) = char_byte_range(cell, cursor, cursor + 1);
            cell.replace_range(start..end, "");
        }
    }


    /// Enter key in grid: advance to the next quick_entry column, skipping
    /// columns where quick_entry is false. Wraps to next row when past the last column.
    pub fn grid_advance_quick_entry(&mut self) {
        let col_count = self.grid_col_count();
        let row_count = self.grid_row_count();
        if col_count == 0 { return; }

        // Collect quick_entry column indices upfront (avoids borrow conflict with grid_append_row)
        let qe_cols: Vec<usize> = self.current_screen.as_ref()
            .and_then(|s| s.lines.as_ref())
            .map(|l| l.columns.iter().enumerate()
                .filter(|(_, c)| c.editable && c.quick_entry)
                .map(|(i, _)| i)
                .collect())
            .unwrap_or_default();

        if qe_cols.is_empty() { return; }

        // Search forward from current column
        if let Some(&c) = qe_cols.iter().find(|&&c| c > self.grid_state.col) {
            self.grid_state.col = c;
            return;
        }

        // Wrap to next row
        if self.grid_state.row + 1 >= row_count {
            if self.count_trailing_empty_rows() < MAX_TRAILING_EMPTY_ROWS {
                self.grid_append_row();
            } else {
                return;
            }
        } else {
            self.grid_state.row += 1;
        }

        // First quick_entry column on the new row
        self.grid_state.col = qe_cols[0];
    }

    /// Move grid focus down, appending a new row if at the bottom.
    /// Caps trailing empty rows to MAX_TRAILING_EMPTY_ROWS to prevent runaway scrolling.
    pub fn grid_move_down_or_append(&mut self) {
        let row_count = self.grid_row_count();
        if self.grid_state.row + 1 >= row_count {
            // Count trailing empty rows
            let trailing_empty = self.count_trailing_empty_rows();
            if trailing_empty < MAX_TRAILING_EMPTY_ROWS {
                self.grid_append_row();
            }
            // else: at cap, do nothing (stay on last row)
        } else {
            self.grid_state.move_down(row_count);
        }
    }

    /// Count how many consecutive fully-empty rows exist at the end of the grid.
    fn count_trailing_empty_rows(&self) -> usize {
        let rows = match self.current_screen.as_ref().and_then(|s| s.lines.as_ref()) {
            Some(l) => &l.rows,
            None => return 0,
        };
        let mut count = 0;
        for row in rows.iter().rev() {
            if row.values.iter().all(|v| v.is_empty()) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Open the grid row delete confirmation dialog.
    pub fn open_grid_delete_confirm(&mut self) {
        if self.grid_row_count() > 0 {
            self.grid_delete_confirm_open = true;
            self.grid_delete_modal_index = 1; // default to Cancel
        }
    }

    /// Number of columns in the lines table.
    pub fn grid_col_count(&self) -> usize {
        self.current_screen
            .as_ref()
            .and_then(|s| s.lines.as_ref())
            .map(|l| l.columns.len())
            .unwrap_or(0)
    }

    /// Number of rows in the lines table.
    pub fn grid_row_count(&self) -> usize {
        self.current_screen
            .as_ref()
            .and_then(|s| s.lines.as_ref())
            .map(|l| l.rows.len())
            .unwrap_or(0)
    }

    /// Get the value of an Option-type column from the current row (for default copying).
    fn grid_current_row_option_defaults(&self) -> Vec<(usize, String)> {
        let screen = match self.current_screen.as_ref() {
            Some(s) => s,
            None => return vec![],
        };
        let lines = match screen.lines.as_ref() {
            Some(l) => l,
            None => return vec![],
        };
        let row = match lines.rows.get(self.grid_state.row) {
            Some(r) => r,
            None => return vec![],
        };
        let mut defaults = vec![];
        for (ci, col) in lines.columns.iter().enumerate() {
            if col.col_type == FieldType::Option {
                if let Some(val) = row.values.get(ci) {
                    if !val.is_empty() {
                        defaults.push((ci, val.clone()));
                    }
                }
            }
        }
        defaults
    }

    /// If the grid has no rows, append one so the user can start editing.
    pub fn grid_ensure_row(&mut self) {
        if self.grid_col_count() == 0 { return; }
        if self.grid_row_count() == 0 {
            self.grid_append_row();
        }
    }

    /// Append an empty row at the end and move focus to it.
    /// Copies Option column values from the current row as defaults.
    pub fn grid_append_row(&mut self) {
        let col_count = self.grid_col_count();
        if col_count == 0 { return; }
        let defaults = self.grid_current_row_option_defaults();
        if let Some(lines) = self.current_screen.as_mut().and_then(|s| s.lines.as_mut()) {
            let idx = lines.rows.len();
            let mut values = vec![String::new(); col_count];
            for (ci, val) in &defaults {
                if *ci < values.len() {
                    values[*ci] = val.clone();
                }
            }
            lines.rows.push(two_wee_shared::TableRow {
                index: idx,
                values,
            });
            lines.row_count = lines.rows.len();
            self.grid_state.row = idx;
            self.grid_state.col = 0;
        }
    }

    /// Insert an empty row below the current row (F3) and move focus to it.
    /// Copies Option column values from the current row as defaults.
    pub fn grid_insert_row_below(&mut self) {
        let col_count = self.grid_col_count();
        if col_count == 0 { return; }
        let defaults = self.grid_current_row_option_defaults();
        if let Some(lines) = self.current_screen.as_mut().and_then(|s| s.lines.as_mut()) {
            let insert_at = (self.grid_state.row + 1).min(lines.rows.len());
            let mut values = vec![String::new(); col_count];
            for (ci, val) in &defaults {
                if *ci < values.len() {
                    values[*ci] = val.clone();
                }
            }
            lines.rows.insert(insert_at, two_wee_shared::TableRow {
                index: insert_at,
                values,
            });
            // Re-index rows after insertion
            for (i, row) in lines.rows.iter_mut().enumerate() {
                row.index = i;
            }
            lines.row_count = lines.rows.len();
            // Update dirty_cells keys: shift rows at or after insert_at
            let mut new_dirty = HashMap::new();
            for ((r, c), v) in self.grid_state.dirty_cells.drain() {
                let new_r = if r >= insert_at { r + 1 } else { r };
                new_dirty.insert((new_r, c), v);
            }
            self.grid_state.dirty_cells = new_dirty;
            self.grid_state.row = insert_at;
            self.grid_state.col = 0;
        }
        self.grid_recalculate_totals();
    }

    /// Delete the current row (F4).
    pub fn grid_delete_row(&mut self) {
        let row_count = self.grid_row_count();
        if row_count == 0 { return; }
        if let Some(lines) = self.current_screen.as_mut().and_then(|s| s.lines.as_mut()) {
            let del = self.grid_state.row;
            if del >= lines.rows.len() { return; }
            lines.rows.remove(del);
            // Re-index
            for (i, row) in lines.rows.iter_mut().enumerate() {
                row.index = i;
            }
            lines.row_count = lines.rows.len();
            // Update dirty_cells: remove entries for deleted row, shift down
            let mut new_dirty = HashMap::new();
            for ((r, c), v) in self.grid_state.dirty_cells.drain() {
                if r == del { continue; } // deleted row — drop its dirty entries
                let new_r = if r > del { r - 1 } else { r };
                new_dirty.insert((new_r, c), v);
            }
            self.grid_state.dirty_cells = new_dirty;
            // Adjust focus
            if lines.rows.is_empty() {
                self.grid_state.row = 0;
            } else if self.grid_state.row >= lines.rows.len() {
                self.grid_state.row = lines.rows.len() - 1;
            }
        }
        self.grid_recalculate_totals();
    }

    // ----- Grid option column support -----

    /// Get the ColumnDef for the currently focused grid column.
    fn grid_current_col_def(&self) -> Option<&two_wee_shared::ColumnDef> {
        self.current_screen
            .as_ref()?
            .lines
            .as_ref()?
            .columns
            .get(self.grid_state.col)
    }

    /// Check if the focused grid column is a Decimal field.
    pub fn grid_col_is_decimal(&self) -> bool {
        self.grid_current_col_def()
            .map(|c| c.col_type == FieldType::Decimal)
            .unwrap_or(false)
    }

    pub fn grid_col_is_date(&self) -> bool {
        self.grid_current_col_def()
            .map(|c| c.col_type == FieldType::Date)
            .unwrap_or(false)
    }

    /// Check if the focused grid cell is editable, considering both the column definition
    /// and the row's type. For empty-type rows, only "type" and "description" are editable.
    pub fn grid_cell_is_editable(&self) -> bool {
        let col = match self.grid_current_col_def() {
            Some(c) => c,
            None => return false,
        };
        // Option (type) column is always editable regardless of row type
        if col.col_type == FieldType::Option {
            return col.editable;
        }
        if !col.editable {
            return false;
        }
        // For empty-type rows, only description is editable
        if let Some(row_type) = self.grid_row_col_value("type") {
            if row_type.is_empty() && col.id != "description" {
                return false;
            }
        }
        true
    }

    /// Check if the focused grid column is an Option field.
    pub fn grid_col_is_option(&self) -> bool {
        self.grid_current_col_def()
            .map(|c| c.col_type == two_wee_shared::FieldType::Option)
            .unwrap_or(false)
    }

    /// Extract option keys from the focused grid column.
    fn grid_option_keys(&self) -> Vec<String> {
        let col = match self.grid_current_col_def() {
            Some(c) => c,
            None => return vec![],
        };
        match &col.options {
            Some(OptionValues::Simple(vals)) => vals.clone(),
            Some(OptionValues::Labeled(pairs)) => pairs.iter().map(|p| p.value.clone()).collect(),
            None => vec![],
        }
    }

    /// Extract option labels from the focused grid column.
    pub fn grid_option_labels(&self) -> Vec<String> {
        let col = match self.grid_current_col_def() {
            Some(c) => c,
            None => return vec![],
        };
        match &col.options {
            Some(OptionValues::Simple(vals)) => vals.clone(),
            Some(OptionValues::Labeled(pairs)) => pairs.iter().map(|p| p.label.clone()).collect(),
            None => vec![],
        }
    }

    /// Find the current cell value's index in the option list.
    fn grid_option_index(&self) -> usize {
        let keys = self.grid_option_keys();
        let value = self.grid_cell_value().unwrap_or("");
        keys.iter().position(|k| k == value).unwrap_or(0)
    }

    /// Space: cycle to the next option value.
    pub fn grid_option_cycle(&mut self) {
        let keys = self.grid_option_keys();
        if keys.is_empty() { return; }
        let old_value = self.grid_cell_value().unwrap_or("").to_string();
        let idx = match keys.iter().position(|k| *k == old_value) {
            Some(i) => (i + 1) % keys.len(),
            None => 0,
        };
        let key = (self.grid_state.row, self.grid_state.col);
        if !self.grid_state.dirty_cells.contains_key(&key) {
            self.grid_state.dirty_cells.insert(key, old_value.clone());
        }
        let new_value = keys[idx].clone();
        if let Some(cell) = self.grid_cell_value_mut() {
            *cell = new_value.clone();
        }
        if new_value != old_value {
            self.grid_on_type_changed();
        }
    }

    /// Backspace/Delete: reset option to first value.
    pub fn grid_option_reset(&mut self) {
        let keys = self.grid_option_keys();
        if keys.is_empty() { return; }
        let old_value = self.grid_cell_value().unwrap_or("").to_string();
        let key = (self.grid_state.row, self.grid_state.col);
        if !self.grid_state.dirty_cells.contains_key(&key) {
            self.grid_state.dirty_cells.insert(key, old_value.clone());
        }
        let new_value = keys[0].clone();
        if let Some(cell) = self.grid_cell_value_mut() {
            *cell = new_value.clone();
        }
        if new_value != old_value {
            self.grid_on_type_changed();
        }
    }

    /// When the "type" option column changes, clear dependent columns in the same row.
    fn grid_on_type_changed(&mut self) {
        // Only act if the current column is "type"
        let col_id = self.grid_current_col_def().map(|c| c.id.clone());
        if col_id.as_deref() != Some("type") {
            return;
        }
        let row = self.grid_state.row;
        let clear_cols = ["no", "description", "unit_of_measure", "unit_price", "line_amount"];
        for col_id in &clear_cols {
            self.set_grid_cell_value(row, col_id, String::new());
        }
    }

    /// F6: open option dropdown for the focused grid column.
    pub fn open_grid_option_modal(&mut self) {
        self.option_modal_index = self.grid_option_index();
        self.option_modal_open = true;
    }

    /// Confirm selection from option modal for grid cell.
    pub fn grid_option_modal_select(&mut self) {
        let keys = self.grid_option_keys();
        let old_value = self.grid_cell_value().unwrap_or("").to_string();
        if let Some(new_val) = keys.get(self.option_modal_index) {
            let key = (self.grid_state.row, self.grid_state.col);
            if !self.grid_state.dirty_cells.contains_key(&key) {
                self.grid_state.dirty_cells.insert(key, old_value.clone());
            }
            let changed = *new_val != old_value;
            if let Some(cell) = self.grid_cell_value_mut() {
                *cell = new_val.clone();
            }
            if changed {
                self.grid_on_type_changed();
            }
        }
        self.option_modal_open = false;
    }

    /// Build and return an AuthSubmit action from the current login form.
    /// Auto-confirms any in-progress edit first so typed values are captured.
    pub fn auth_submit_action(&mut self) -> AppAction {
        // Confirm any in-progress edit so the typed value is committed
        self.card_confirm_edit();

        let auth_url = match self.current_screen.as_ref().and_then(|s| s.auth_action.clone()) {
            Some(url) => url,
            None => return AppAction::None,
        };
        let mut fields = std::collections::HashMap::new();
        if let Some(ref screen) = self.current_screen {
            for section in &screen.sections {
                for field in &section.fields {
                    if field.editable {
                        fields.insert(field.id.clone(), field.value.clone());
                    }
                }
            }
        }
        let full_url = format!("{}{}", self.host_url, auth_url);
        AppAction::AuthSubmit(full_url, two_wee_shared::AuthRequest { fields })
    }

    pub fn save_action(&mut self) -> AppAction {
        // If this is a login form, delegate to auth_submit_action
        if self.is_auth_screen() {
            return self.auth_submit_action();
        }

        if let (Some(url), Some(changeset)) = (self.card_save_url(), self.build_changeset()) {
            self.message = self.ui_strings.saving.clone();
            AppAction::SaveCard(url, changeset)
        } else {
            self.set_timed_message(self.ui_strings.no_changes.clone(), Duration::from_millis(300));
            AppAction::None
        }
    }


    fn resolve_action(&self, key: &str) -> Option<String> {
        let screen = self.current_screen.as_ref()?;
        let path = screen.actions.get(key)?;
        Some(format!("{}{}", self.host_url, path))
    }

    pub fn refresh_action(&self) -> AppAction {
        match &self.current_screen_url {
            Some(url) => AppAction::FetchScreen(url.clone()),
            None => AppAction::None,
        }
    }

    pub fn new_card_action(&self) -> AppAction {
        match self.resolve_action("create") {
            Some(url) => AppAction::NewCard(url),
            None => AppAction::None,
        }
    }

    pub fn delete_action(&self) -> AppAction {
        if self.is_new_record {
            return AppAction::None;
        }
        match self.resolve_action("delete") {
            Some(url) => {
                let record_id = self.current_record_id();
                let screen_id = self.current_screen.as_ref()
                    .map(|s| s.screen_id.clone())
                    .unwrap_or_default();
                AppAction::DeleteCard(
                    url,
                    two_wee_shared::DeleteRequest {
                        screen_id,
                        record_id,
                    },
                )
            }
            None => AppAction::None,
        }
    }

    pub fn open_delete_confirm(&mut self) {
        if !self.is_new_record {
            self.delete_confirm_open = true;
            self.delete_modal_index = 1; // Default to Cancel
        }
    }

    pub fn close_delete_confirm(&mut self) {
        self.delete_confirm_open = false;
    }

    /// The primary key of the current record (first field in the first section).
    fn current_record_id(&self) -> String {
        self.current_screen.as_ref()
            .and_then(|s| s.sections.first())
            .and_then(|s| s.fields.first())
            .map(|f| f.value.clone())
            .unwrap_or_default()
    }

    /// F6 / Shift+Enter / Ctrl+O: behavior is derived from field properties.
    /// - Field with lookup → Lookup (modal or full-screen drilldown)
    /// - Email/Phone/URL → Open in OS (mailto:, tel:, https://)
    /// - Option → handled separately (open_option_modal), never reaches here
    /// - Everything else → no action
    pub fn lookup_or_drilldown_action(&mut self) -> AppAction {
        let field = match self.current_card_field() {
            Some(f) => f.clone(),
            None => return AppAction::None,
        };
        if let Some(ref lookup) = field.lookup {
            self.pending_lookup_field = Some(field.id.clone());
            let context_suffix = self.card_context_query_suffix(&lookup.context);
            let query = Self::append_selected_param(&context_suffix, &field.value);
            let url = format!("{}{}{}", self.host_url, lookup.endpoint, query);
            if lookup.display.as_deref() == Some("modal") {
                AppAction::FetchModalLookup(url)
            } else {
                AppAction::FetchDrilldown(url)
            }
        } else if matches!(field.field_type, FieldType::Email | FieldType::Phone | FieldType::URL) {
            self.open_field_in_os(&field.label, &field.field_type, &field.value);
            AppAction::None
        } else {
            AppAction::None
        }
    }

    /// If the current field has a lookup validate endpoint, build a ValidateLookup action.
    /// Call this after card_confirm_edit succeeds but before moving to the next field.
    pub fn lookup_validate_action(&self) -> AppAction {
        let field = match self.current_card_field() {
            Some(f) => f,
            None => return AppAction::None,
        };
        let validate_endpoint = match field.lookup.as_ref().and_then(|l| l.validate.as_ref()) {
            Some(v) => v,
            None => return AppAction::None,
        };
        let value = field.value.clone();
        // Skip validation for empty values and unchanged values
        if value.is_empty() {
            return AppAction::None;
        }
        if let Some(original) = self.card_original_values.get(&field.id) {
            if *original == value {
                return AppAction::None;
            }
        }
        let field_id = field.id.clone();
        let context_suffix = self.card_context_query_suffix(&field.lookup.as_ref().unwrap().context);
        let url = format!("{}{}/{}{}", self.host_url, validate_endpoint, value, context_suffix);
        AppAction::ValidateLookup { field_id, url, value }
    }

    /// Like `lookup_validate_action` but uses an explicit value and always validates
    /// (skips the "unchanged" check). Used after lookup modal returns a selected value.
    pub fn lookup_validate_action_for_value(&self, value: &str) -> AppAction {
        let field = match self.current_card_field() {
            Some(f) => f,
            None => return AppAction::None,
        };
        let validate_endpoint = match field.lookup.as_ref().and_then(|l| l.validate.as_ref()) {
            Some(v) => v,
            None => return AppAction::None,
        };
        if value.is_empty() {
            return AppAction::None;
        }
        let field_id = field.id.clone();
        let context_suffix = self.card_context_query_suffix(&field.lookup.as_ref().unwrap().context);
        let url = format!("{}{}/{}{}", self.host_url, validate_endpoint, value, context_suffix);
        AppAction::ValidateLookup { field_id, url, value: value.to_string() }
    }

    /// Navigate the card field cursor to a specific field by id.
    pub fn navigate_to_card_field(&mut self, field_id: &str) {
        if let Some(idx) = self.card_fields_flat.iter().position(|r| {
            self.current_screen.as_ref()
                .and_then(|s| s.sections.get(r.section_idx))
                .and_then(|sec| sec.fields.get(r.field_idx))
                .map(|f| f.id == field_id)
                .unwrap_or(false)
        }) {
            self.card_field_index = idx;
        }
    }

    /// Reject a field value after async validation (e.g. lookup validate).
    /// Navigates to the field, enters edit mode with text selected, shows the error,
    /// and sets the revert value so Esc restores the pre-edit original.
    pub fn reject_field_value(&mut self, field_id: &str, error: String) {
        self.set_error_message(error);
        if let Some(idx) = self.card_fields_flat.iter().position(|r| {
            self.current_screen.as_ref()
                .and_then(|s| s.sections.get(r.section_idx))
                .and_then(|sec| sec.fields.get(r.field_idx))
                .map(|f| f.id == field_id)
                .unwrap_or(false)
        }) {
            self.card_field_index = idx;
            self.reset_edit_cycle();
            self.card_f2_cycle(); // Enter edit with select-all
            self.header_original_value = self.card_original_values.get(field_id).cloned();
        }
    }

    /// Write a value to a card field by field ID (used by lookup return).
    pub fn set_card_field_value(&mut self, field_id: &str, value: String) {
        if let Some(screen) = self.current_screen.as_mut() {
            for section in &mut screen.sections {
                for field in &mut section.fields {
                    if field.id == field_id {
                        field.value = value;
                        return;
                    }
                }
            }
        }
    }

    /// Pop the lookup source field from the top of the screen stack.
    pub fn pop_lookup_source_field(&mut self) -> Option<String> {
        self.screen_stack.last_mut().and_then(|e| e.lookup_source_field.take())
    }

    /// Pop the lookup source grid cell from the top of the screen stack.
    pub fn pop_lookup_source_grid_cell(&mut self) -> Option<(usize, usize)> {
        self.screen_stack.last_mut().and_then(|e| e.lookup_source_grid_cell.take())
    }

    /// Check if the focused grid column has lookup info.
    /// Suppresses lookup for type-dependent columns when the row type disables it
    /// (e.g. unit_of_measure has no lookup when type is "Text").
    pub fn grid_col_is_lookup(&self) -> bool {
        let col_def = match self.grid_current_col_def() {
            Some(c) if c.lookup.is_some() => c,
            _ => return false,
        };
        // Suppress lookup for unit_of_measure when type is Text
        if col_def.id == "unit_of_measure" {
            if let Some(t) = self.grid_row_col_value("type") {
                if t == "Text" {
                    return false;
                }
            }
        }
        true
    }

    /// Read a column value from the current grid row by column ID.
    pub fn grid_row_col_value(&self, col_id: &str) -> Option<String> {
        let screen = self.current_screen.as_ref()?;
        let lines = screen.lines.as_ref()?;
        let ci = lines.columns.iter().position(|c| c.id == col_id)?;
        lines.rows.get(self.grid_state.row)?.values.get(ci).cloned()
    }

    /// Build a query string suffix from lookup context for grid columns.
    /// Reads context field values from the current grid row.
    pub fn grid_context_query_suffix(&self, context: &[two_wee_shared::LookupContext]) -> String {
        let mut params = Vec::new();
        for ctx in context {
            let param_name = ctx.param.as_deref().unwrap_or(&ctx.field);
            if let Some(val) = self.grid_row_col_value(&ctx.field) {
                if !val.is_empty() {
                    params.push(format!("{}={}", param_name, val));
                }
            }
        }
        if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) }
    }

    /// Build a query string suffix from lookup context for card fields.
    /// Reads context field values from other card fields.
    pub fn card_context_query_suffix(&self, context: &[two_wee_shared::LookupContext]) -> String {
        let mut params = Vec::new();
        for ctx in context {
            let param_name = ctx.param.as_deref().unwrap_or(&ctx.field);
            if let Some(val) = self.card_field_value(&ctx.field) {
                if !val.is_empty() {
                    params.push(format!("{}={}", param_name, val));
                }
            }
        }
        if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) }
    }

    /// Append a `selected=` parameter to a query string suffix.
    /// If suffix is empty, returns `?selected=value`. Otherwise appends `&selected=value`.
    fn append_selected_param(suffix: &str, value: &str) -> String {
        if value.is_empty() {
            return suffix.to_string();
        }
        if suffix.is_empty() {
            format!("?selected={}", value)
        } else {
            format!("{}&selected={}", suffix, value)
        }
    }

    /// Read a card field value by field ID.
    fn card_field_value(&self, field_id: &str) -> Option<String> {
        let screen = self.current_screen.as_ref()?;
        for section in &screen.sections {
            for field in &section.fields {
                if field.id == field_id {
                    return Some(field.value.clone());
                }
            }
        }
        None
    }

    /// F6 on a grid lookup column: store (row, col), return FetchDrilldown.
    pub fn grid_lookup_action(&mut self) -> AppAction {
        let col_def = match self.grid_current_col_def()
            .filter(|c| c.lookup.is_some())
        {
            Some(c) => c,
            None => return AppAction::None,
        };
        let lookup = col_def.lookup.as_ref().unwrap();
        let endpoint = lookup.endpoint.clone();
        let is_modal = lookup.display.as_deref() == Some("modal");
        let context_suffix = self.grid_context_query_suffix(&lookup.context);
        let current_value = self.grid_cell_value().unwrap_or("").to_string();
        let query = Self::append_selected_param(&context_suffix, &current_value);
        let url = format!("{}{}{}", self.host_url, endpoint, query);
        self.pending_lookup_grid_cell = Some((self.grid_state.row, self.grid_state.col));
        if is_modal {
            AppAction::FetchModalLookup(url)
        } else {
            AppAction::FetchDrilldown(url)
        }
    }

    /// Write a value to a specific grid cell by column ID in the given row.
    pub fn set_grid_cell_value(&mut self, row: usize, col_id: &str, value: String) {
        if let Some(lines) = self.current_screen.as_mut().and_then(|s| s.lines.as_mut()) {
            if let Some(col_idx) = lines.columns.iter().position(|c| c.id == col_id) {
                if let Some(row_data) = lines.rows.get_mut(row) {
                    if col_idx < row_data.values.len() {
                        let key = (row, col_idx);
                        if !self.grid_state.dirty_cells.contains_key(&key) {
                            self.grid_state.dirty_cells.insert(key, row_data.values[col_idx].clone());
                        }
                        row_data.values[col_idx] = value;
                    }
                }
            }
        }
    }

    /// Reject a grid cell value after validation: navigate back to cell, show error.
    pub fn reject_grid_cell_value(&mut self, row: usize, col: usize, error: String) {
        self.set_error_message(error);
        self.grid_state.row = row;
        self.grid_state.col = col;
        self.grid_state.editing = true;
        // Select all so user can retype
        let len = self.grid_cell_value().map(|v| v.chars().count()).unwrap_or(0);
        self.grid_state.cursor = len;
        self.grid_state.selection_anchor = Some(0);
        // Store original for revert on Esc
        if let Some(original) = self.grid_state.dirty_cells.get(&(row, col)) {
            self.grid_state.original_value = Some(original.clone());
        }
    }

    /// Context-sensitive hint for the currently focused card field.
    /// Shown on the left side of the bottom bar.
    pub fn field_context_hint(&self) -> Option<String> {
        use crate::shortcuts::Action;
        let field = self.current_card_field()?;
        if field.field_type == FieldType::Boolean {
            Some("Space  Toggle".to_string())
        } else if field.field_type == FieldType::Option {
            Some(format!("{} · {}", self.shortcuts.hint_for(Action::OptionCycle), self.shortcuts.hint_for(Action::OptionSelect)))
        } else if field.field_type == FieldType::TextArea && self.header_input_mode == HeaderInputMode::Edit {
            let line_count = field.value.chars().filter(|&c| c == '\n').count() + 1;
            let max_lines = field.rows.unwrap_or(4) as usize;
            Some(format!("{}/{}  Ctrl+Enter New line", line_count, max_lines))
        } else if field.lookup.is_some() {
            let action = if field.editable { Action::Lookup } else { Action::DrillDown };
            Some(self.shortcuts.hint_for(action))
        } else if matches!(field.field_type, FieldType::Email | FieldType::Phone | FieldType::URL) {
            Some(self.shortcuts.hint_for(Action::DrillDown))
        } else {
            None
        }
    }

    /// Context-sensitive hint for the currently focused grid column.
    pub fn grid_field_context_hint(&self) -> Option<String> {
        use crate::shortcuts::Action;
        let col = self.grid_current_col_def()?;
        if col.col_type == FieldType::Option {
            Some(format!("{} · {}", self.shortcuts.hint_for(Action::OptionCycle), self.shortcuts.hint_for(Action::OptionSelect)))
        } else if self.grid_cell_is_editable() && self.grid_col_is_lookup() {
            Some(self.shortcuts.hint_for(Action::Lookup))
        } else {
            None
        }
    }

    // ----- Card field methods -----

    pub fn current_card_field(&self) -> Option<&two_wee_shared::Field> {
        self.card_field_at(self.card_field_index)
    }

    /// Resolve an arbitrary flat index to its `Field`, or `None` if out of range.
    pub fn card_field_at(&self, flat_idx: usize) -> Option<&two_wee_shared::Field> {
        let screen = self.current_screen.as_ref()?;
        let fref = self.card_fields_flat.get(flat_idx)?;
        screen.sections.get(fref.section_idx)?.fields.get(fref.field_idx)
    }


    pub fn current_card_field_mut(&mut self) -> Option<&mut two_wee_shared::Field> {
        let fref = self.card_fields_flat.get(self.card_field_index)?.clone();
        let screen = self.current_screen.as_mut()?;
        screen.sections.get_mut(fref.section_idx)?.fields.get_mut(fref.field_idx)
    }

    pub fn card_field_editable(&self) -> bool {
        self.current_card_field()
            .map(|f| f.editable && f.field_type != FieldType::Option && f.field_type != FieldType::Boolean)
            .unwrap_or(false)
    }

    pub fn card_field_is_option(&self) -> bool {
        self.current_card_field()
            .map(|f| f.field_type == FieldType::Option)
            .unwrap_or(false)
    }

    pub fn card_field_is_boolean(&self) -> bool {
        self.current_card_field()
            .map(|f| f.field_type == FieldType::Boolean)
            .unwrap_or(false)
    }

    /// Toggle the current Boolean field between "true" and "false".
    pub fn card_bool_toggle(&mut self) {
        if let Some(field) = self.current_card_field_mut() {
            if field.field_type != FieldType::Boolean { return; }
            field.value = if field.value == "true" {
                "false".to_string()
            } else {
                "true".to_string()
            };
        }
    }

    /// Get the display labels for the current option field.
    pub fn current_option_labels(&self) -> Vec<String> {
        let field = match self.current_card_field() {
            Some(f) => f,
            None => return vec![],
        };
        match &field.options {
            Some(OptionValues::Simple(vals)) => vals.clone(),
            Some(OptionValues::Labeled(pairs)) => pairs.iter().map(|p| p.label.clone()).collect(),
            None => vec![],
        }
    }

    /// Get the keys for the current option field.
    fn current_option_keys(&self) -> Vec<String> {
        let field = match self.current_card_field() {
            Some(f) => f,
            None => return vec![],
        };
        match &field.options {
            Some(OptionValues::Simple(vals)) => vals.clone(),
            Some(OptionValues::Labeled(pairs)) => pairs.iter().map(|p| p.value.clone()).collect(),
            None => vec![],
        }
    }

    /// Find the current field value's index in its option list (by key).
    fn current_option_index(&self) -> usize {
        let keys = self.current_option_keys();
        let value = self.current_card_field().map(|f| f.value.as_str()).unwrap_or("");
        keys.iter().position(|k| k == value).unwrap_or(0)
    }

    /// Resolve the current field's stored key to its display label.
    pub fn current_option_display(&self) -> String {
        let field = match self.current_card_field() {
            Some(f) => f,
            None => return String::new(),
        };
        match &field.options {
            Some(OptionValues::Simple(_)) => field.value.clone(),
            Some(OptionValues::Labeled(pairs)) => {
                pairs.iter()
                    .find(|p| p.value == field.value)
                    .map(|p| p.label.clone())
                    .unwrap_or_else(|| field.value.clone())
            }
            None => field.value.clone(),
        }
    }

    /// Spacebar: cycle to the next option.
    pub fn card_option_cycle(&mut self) {
        let keys = self.current_option_keys();
        if keys.is_empty() {
            return;
        }
        let idx = (self.current_option_index() + 1) % keys.len();
        if let Some(field) = self.current_card_field_mut() {
            field.value = keys[idx].clone();
        }
    }

    /// Backspace/Delete: reset option to index 0.
    pub fn card_option_reset(&mut self) {
        let keys = self.current_option_keys();
        if keys.is_empty() {
            return;
        }
        if let Some(field) = self.current_card_field_mut() {
            field.value = keys[0].clone();
        }
    }

    /// F6: open the floating option modal.
    pub fn open_option_modal(&mut self) {
        self.option_modal_index = self.current_option_index();
        self.option_modal_open = true;
    }

    /// Confirm the selected option from the modal.
    pub fn option_modal_select(&mut self) {
        let keys = self.current_option_keys();
        if let Some(key) = keys.get(self.option_modal_index) {
            if let Some(field) = self.current_card_field_mut() {
                field.value = key.clone();
            }
        }
        self.option_modal_open = false;
    }

    pub fn close_option_modal(&mut self) {
        self.option_modal_open = false;
    }

    pub fn option_modal_next(&mut self) {
        let count = if self.lines_overlay_open {
            self.grid_option_keys().len()
        } else {
            self.current_option_keys().len()
        };
        if count > 0 && self.option_modal_index + 1 < count {
            self.option_modal_index += 1;
        }
    }

    pub fn option_modal_prev(&mut self) {
        if self.option_modal_index > 0 {
            self.option_modal_index -= 1;
        }
    }

    // ----- Lookup modal methods -----

    pub fn lookup_modal_open(&self) -> bool {
        self.lookup_modal_data.is_some()
    }

    /// Open the lookup modal with data from a fetched screen.
    pub fn open_lookup_modal(
        &mut self,
        screen: ScreenContract,
        source_field: Option<String>,
        source_grid_cell: Option<(usize, usize)>,
    ) {
        let lines = match screen.lines {
            Some(l) => l,
            None => return,
        };
        let value_column = lines.value_column.unwrap_or_default();
        // Pre-compute column widths (minimum 8 chars per column)
        let col_widths: Vec<usize> = lines.columns.iter().enumerate().map(|(ci, col)| {
            let header_w = col.label.chars().count();
            let max_data = lines.rows.iter()
                .map(|r| r.values.get(ci).map(|v| v.chars().count()).unwrap_or(0))
                .max()
                .unwrap_or(0);
            header_w.max(max_data).max(8)
        }).collect();
        // Get the current value from the source field/cell to pre-select
        let current_value = if let Some((row, col)) = source_grid_cell {
            self.current_screen.as_ref()
                .and_then(|s| s.lines.as_ref())
                .and_then(|l| l.rows.get(row))
                .and_then(|r| r.values.get(col))
                .cloned()
                .unwrap_or_default()
        } else if let Some(ref field_id) = source_field {
            self.current_screen.as_ref()
                .iter()
                .flat_map(|s| s.sections.iter())
                .flat_map(|sec| sec.fields.iter())
                .find(|f| f.id == *field_id)
                .map(|f| f.value.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Find the row matching the current value in the value column
        let initial_index = if !current_value.is_empty() {
            lines.columns.iter().position(|c| c.id == value_column)
                .and_then(|ci| {
                    lines.rows.iter().position(|r| {
                        r.values.get(ci).map(|v| v == &current_value).unwrap_or(false)
                    })
                })
                .unwrap_or(0)
        } else {
            0
        };

        self.lookup_modal_data = Some(LookupModalData {
            title: screen.title,
            columns: lines.columns,
            all_rows: lines.rows,
            value_column,
            autofill: lines.autofill,
            source_field,
            source_grid_cell,
            col_widths,
            on_drill: lines.on_drill,
        });
        self.lookup_modal_index = initial_index;
        self.lookup_modal_filter.clear();
    }

    pub fn close_lookup_modal(&mut self) {
        self.lookup_modal_data = None;
        self.lookup_modal_filter.clear();
    }

    /// Return indices of rows matching the current filter (fuzzy match on all columns).
    /// Results are sorted by best match score.
    pub fn lookup_modal_filtered_rows(&mut self) -> Vec<usize> {
        use nucleo_matcher::{
            pattern::{AtomKind, CaseMatching, Normalization, Pattern},
            Utf32Str,
        };

        let data = match &self.lookup_modal_data {
            Some(d) => d,
            None => return vec![],
        };
        if self.lookup_modal_filter.is_empty() {
            return (0..data.all_rows.len()).collect();
        }

        let pattern = Pattern::new(
            &self.lookup_modal_filter,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut buf = Vec::new();

        // Score each row by best match across all columns
        let mut scored: Vec<(usize, u32)> = data.all_rows.iter().enumerate().filter_map(|(i, row)| {
            let best = row.values.iter().filter_map(|v| {
                pattern.score(Utf32Str::new(v, &mut buf), &mut self.lookup_matcher)
            }).max()?;
            Some((i, best))
        }).collect();

        scored.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// Select the current row and return an action to apply value + autofill.
    pub fn lookup_modal_select(&mut self) -> AppAction {
        let filtered = self.lookup_modal_filtered_rows();
        let row_idx = match filtered.get(self.lookup_modal_index) {
            Some(&i) => i,
            None => { self.close_lookup_modal(); return AppAction::None; }
        };
        let data = match self.lookup_modal_data.take() {
            Some(d) => d,
            None => return AppAction::None,
        };
        let row = match data.all_rows.get(row_idx) {
            Some(r) => r,
            None => { self.close_lookup_modal(); return AppAction::None; }
        };

        // Extract value from value_column
        let col_index = data.columns.iter().position(|c| c.id == data.value_column);
        let value = col_index
            .and_then(|i| row.values.get(i))
            .cloned()
            .unwrap_or_default();

        // Extract autofill values
        let mut autofill_values = HashMap::new();
        for (col_id, target_field_id) in &data.autofill {
            if let Some(ci) = data.columns.iter().position(|c| c.id == *col_id) {
                if let Some(v) = row.values.get(ci) {
                    autofill_values.insert(target_field_id.clone(), v.clone());
                }
            }
        }

        // Apply directly (no screen stack involved)
        if let Some((grid_row, grid_col)) = data.source_grid_cell {
            // Grid lookup: write to grid cells
            if let Some(col_id) = self.current_screen.as_ref()
                .and_then(|s| s.lines.as_ref())
                .and_then(|l| l.columns.get(grid_col))
                .map(|c| c.id.clone())
            {
                self.set_grid_cell_value(grid_row, &col_id, value.clone());
            }
            for (target_col_id, target_value) in &autofill_values {
                self.set_grid_cell_value(grid_row, target_col_id, target_value.clone());
            }
            self.grid_state.row = grid_row;
            self.grid_state.col = grid_col;
            self.grid_recalculate_line_amount();
            self.grid_recalculate_totals();
        } else if let Some(ref field_id) = data.source_field {
            // Card lookup: write to card fields
            self.set_card_field_value(field_id, value.clone());
            for (target_field, target_value) in &autofill_values {
                self.set_card_field_value(target_field, target_value.clone());
            }
            // Trigger validate for full autofill
            self.navigate_to_card_field(field_id);
            let validate = self.lookup_validate_action_for_value(&value);
            if !matches!(validate, AppAction::None) {
                self.pending_validate = Some(validate);
            }
        }

        self.lookup_modal_filter.clear();
        AppAction::None
    }

    /// Ctrl+Enter on a lookup modal row: drill into the selected row using on_drill.
    pub fn lookup_modal_drill_action(&mut self) -> AppAction {
        // Extract on_drill and index before mutable borrow for filtered_rows
        let on_drill = match &self.lookup_modal_data {
            Some(d) => match &d.on_drill {
                Some(url) => url.clone(),
                None => return AppAction::None,
            },
            None => return AppAction::None,
        };
        let modal_index = self.lookup_modal_index;
        let filtered = self.lookup_modal_filtered_rows();
        let row_idx = match filtered.get(modal_index) {
            Some(&i) => i,
            None => return AppAction::None,
        };
        let data = self.lookup_modal_data.as_ref().unwrap();
        let row = match data.all_rows.get(row_idx) {
            Some(r) => r,
            None => return AppAction::None,
        };
        let mut url = on_drill;
        for (i, val) in row.values.iter().enumerate() {
            url = url.replace(&format!("{{{}}}", i), val);
        }
        let full_url = format!("{}{}", self.host_url, url);
        self.message = self.ui_strings.loading.clone();
        self.close_lookup_modal();
        AppAction::FetchCardAndPush(full_url)
    }

    pub fn lookup_modal_next(&mut self) {
        let count = self.lookup_modal_filtered_rows().len();
        if count > 0 && self.lookup_modal_index + 1 < count {
            self.lookup_modal_index += 1;
        }
    }

    pub fn lookup_modal_prev(&mut self) {
        if self.lookup_modal_index > 0 {
            self.lookup_modal_index -= 1;
        }
    }

    pub fn lookup_modal_type_char(&mut self, c: char) {
        self.lookup_modal_filter.push(c);
        self.lookup_modal_index = 0;
    }

    pub fn lookup_modal_backspace(&mut self) {
        self.lookup_modal_filter.pop();
        self.lookup_modal_index = 0;
    }

    pub fn lookup_modal_page_down(&mut self) {
        let count = self.lookup_modal_filtered_rows().len();
        if count == 0 { return; }
        let page = self.lookup_modal_page_size.max(1);
        self.lookup_modal_index = (self.lookup_modal_index + page).min(count - 1);
    }

    pub fn lookup_modal_page_up(&mut self) {
        let page = self.lookup_modal_page_size.max(1);
        self.lookup_modal_index = self.lookup_modal_index.saturating_sub(page);
    }

    /// Dispatch a key event to the lookup modal. Call only when `lookup_modal_open()`.
    pub fn handle_lookup_modal_key(&mut self, key: &KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        match key.code {
            KeyCode::Esc => self.close_lookup_modal(),
            KeyCode::Enter => { self.lookup_modal_select(); }
            KeyCode::Down => self.lookup_modal_next(),
            KeyCode::Up => self.lookup_modal_prev(),
            KeyCode::PageDown => self.lookup_modal_page_down(),
            KeyCode::PageUp => self.lookup_modal_page_up(),
            KeyCode::Backspace => self.lookup_modal_backspace(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.lookup_modal_type_char(c);
            }
            _ => {}
        }
    }

    pub fn card_field_count(&self) -> usize {
        self.card_fields_flat.len()
    }

    pub fn card_next_field(&mut self) {
        if self.card_field_index + 1 < self.card_field_count() {
            self.card_field_index += 1;
            self.reset_edit_cycle();
        }
    }

    pub fn card_prev_field(&mut self) {
        if self.card_field_index > 0 {
            self.card_field_index -= 1;
            self.reset_edit_cycle();
        }
    }

    /// Enter key: advance to the next quick_entry field, skipping fields
    /// where quick_entry is false. If no quick_entry field exists ahead,
    /// falls back to the next editable field so the cursor always moves forward.
    pub fn card_next_quick_entry(&mut self) {
        let mut quick_entry_target: Option<usize> = None;
        let mut fallback_target: Option<usize> = None;
        for i in (self.card_field_index + 1)..self.card_field_count() {
            if let Some(field) = self.card_field_at(i) {
                if field.editable {
                    if fallback_target.is_none() {
                        fallback_target = Some(i);
                    }
                    if field.quick_entry {
                        quick_entry_target = Some(i);
                        break;
                    }
                }
            }
        }
        if let Some(i) = quick_entry_target.or(fallback_target) {
            self.card_field_index = i;
            self.reset_edit_cycle();
        }
    }

    /// Move down spatially: next field in same section, or first field in
    /// the next section (same row_group next column, then next row_group).
    pub fn card_move_down(&mut self) {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return,
        };
        let fref = match self.card_fields_flat.get(self.card_field_index) {
            Some(r) => r.clone(),
            None => return,
        };
        let cur_section = match screen.sections.get(fref.section_idx) {
            Some(s) => s,
            None => return,
        };

        // Try next field within the same section
        let next_in_section = self.card_fields_flat.iter().enumerate()
            .find(|(_, r)| r.section_idx == fref.section_idx && r.field_idx > fref.field_idx)
            .map(|(i, _)| i);
        if let Some(idx) = next_in_section {
            self.card_field_index = idx;
            self.reset_edit_cycle();
            return;
        }

        // At bottom of section — try next column in same row_group first
        let next_col_si = screen.sections.iter().enumerate().find(|(_, s)| {
            s.row_group == cur_section.row_group && s.column > cur_section.column
        }).map(|(i, _)| i);

        if let Some(target_si) = next_col_si {
            if let Some(flat_idx) = self.card_fields_flat.iter().position(|r| r.section_idx == target_si) {
                self.card_field_index = flat_idx;
                self.reset_edit_cycle();
                return;
            }
        }

        // Then try next row_group
        if let Some(target_si) = self.find_vertical_section(screen, cur_section, true) {
            if let Some(flat_idx) = self.card_fields_flat.iter().position(|r| r.section_idx == target_si) {
                self.card_field_index = flat_idx;
                self.reset_edit_cycle();
            }
        }
    }

    /// Move up spatially: previous field in same section, or last field in
    /// the previous section (same row_group prev column, then previous row_group).
    pub fn card_move_up(&mut self) {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return,
        };
        let fref = match self.card_fields_flat.get(self.card_field_index) {
            Some(r) => r.clone(),
            None => return,
        };
        let cur_section = match screen.sections.get(fref.section_idx) {
            Some(s) => s,
            None => return,
        };

        // Try previous field within the same section
        if let Some((idx, _)) = self.card_fields_flat.iter().enumerate().rev().find(|(_, r)| {
            r.section_idx == fref.section_idx && r.field_idx < fref.field_idx
        }) {
            self.card_field_index = idx;
            self.reset_edit_cycle();
            return;
        }

        // At top of section — try previous column in same row_group first
        let prev_col_si = screen.sections.iter().enumerate().rev().find(|(_, s)| {
            s.row_group == cur_section.row_group && s.column < cur_section.column
        }).map(|(i, _)| i);

        if let Some(target_si) = prev_col_si {
            if let Some(flat_idx) = self.card_fields_flat.iter().enumerate().rev()
                .find(|(_, r)| r.section_idx == target_si)
                .map(|(i, _)| i)
            {
                self.card_field_index = flat_idx;
                self.reset_edit_cycle();
                return;
            }
        }

        // Then try previous row_group
        if let Some(target_si) = self.find_vertical_section(screen, cur_section, false) {
            if let Some(flat_idx) = self.card_fields_flat.iter().enumerate().rev()
                .find(|(_, r)| r.section_idx == target_si)
                .map(|(i, _)| i)
            {
                self.card_field_index = flat_idx;
                self.reset_edit_cycle();
            }
        }
    }

    /// Find the section in the adjacent row_group (up or down) that best matches
    /// the current column. Prefers exact column match, falls back to column 0.
    fn find_vertical_section(
        &self,
        screen: &ScreenContract,
        cur_section: &two_wee_shared::Section,
        forward: bool,
    ) -> Option<usize> {
        let cur_rg = cur_section.row_group;
        let cur_col = cur_section.column;

        // Collect candidate sections in adjacent row_groups
        let candidates: Vec<(usize, &two_wee_shared::Section)> = screen
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if forward { s.row_group > cur_rg } else { s.row_group < cur_rg }
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Find the closest row_group
        let closest_rg = if forward {
            candidates.iter().map(|(_, s)| s.row_group).min().unwrap()
        } else {
            candidates.iter().map(|(_, s)| s.row_group).max().unwrap()
        };

        let in_closest: Vec<(usize, &two_wee_shared::Section)> = candidates
            .into_iter()
            .filter(|(_, s)| s.row_group == closest_rg)
            .collect();

        // Prefer same column, fall back to column 0
        in_closest
            .iter()
            .find(|(_, s)| s.column == cur_col)
            .or_else(|| in_closest.iter().find(|(_, s)| s.column == 0))
            .or(in_closest.first())
            .map(|(si, _)| *si)
    }

    pub fn card_home_field(&mut self) {
        self.card_field_index = 0;
    }

    pub fn card_end_field(&mut self) {
        let count = self.card_field_count();
        if count > 0 {
            self.card_field_index = count - 1;
        }
    }

    pub fn card_move_left(&mut self) {
        self.card_move_horizontal(-1);
    }

    pub fn card_move_right(&mut self) {
        self.card_move_horizontal(1);
    }

    fn card_move_horizontal(&mut self, direction: i8) {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return,
        };
        let fref = match self.card_fields_flat.get(self.card_field_index) {
            Some(r) => r.clone(),
            None => return,
        };
        let cur_section = match screen.sections.get(fref.section_idx) {
            Some(s) => s,
            None => return,
        };
        let cur_rg = cur_section.row_group;
        let cur_col = cur_section.column as i16;
        let target_col = cur_col + direction as i16;
        if target_col < 0 {
            return;
        }

        let target_section_idx = screen
            .sections
            .iter()
            .position(|s| s.row_group == cur_rg && s.column == target_col as u8);
        let target_si = match target_section_idx {
            Some(si) => si,
            None => return,
        };
        let target_section = &screen.sections[target_si];
        if target_section.fields.is_empty() {
            return;
        }

        let target_fi = fref.field_idx.min(target_section.fields.len() - 1);

        // Find the nearest non-separator field in the target section.
        // Try the target index first, then scan outward (up and down).
        let mut best_fi: Option<usize> = None;
        for offset in 0..target_section.fields.len() {
            if target_fi + offset < target_section.fields.len() {
                if target_section.fields[target_fi + offset].field_type != FieldType::Separator {
                    best_fi = Some(target_fi + offset);
                    break;
                }
            }
            if offset > 0 && target_fi >= offset {
                if target_section.fields[target_fi - offset].field_type != FieldType::Separator {
                    best_fi = Some(target_fi - offset);
                    break;
                }
            }
        }

        let target_fi = match best_fi {
            Some(fi) => fi,
            None => return, // All fields are separators
        };

        if let Some(flat_idx) = self
            .card_fields_flat
            .iter()
            .position(|r| r.section_idx == target_si && r.field_idx == target_fi)
        {
            self.card_field_index = flat_idx;
        }
    }

    pub fn card_insert_char(&mut self, ch: char) {
        if self.header_input_mode != HeaderInputMode::Edit || !self.card_field_editable() {
            return;
        }

        // Read input_mask before mutable borrow
        let mask = self.current_card_field()
            .and_then(|f| f.validation.as_ref())
            .and_then(|v| v.input_mask.as_deref())
            .map(|s| s.to_string());
        let ch = match apply_input_mask(ch, mask.as_deref()) {
            Some(c) => c,
            None => return,
        };
        self.form_error.clear();
        self.delete_selection();
        let cursor = self.header_cursor;
        if let Some(field) = self.current_card_field_mut() {
            let mut chars: Vec<char> = field.value.chars().collect();
            if cursor <= chars.len() {
                chars.insert(cursor, ch);
                field.value = chars.iter().collect();
                self.header_cursor = cursor + 1;
            }
        }
    }

    pub fn card_backspace(&mut self) {
        if self.header_input_mode != HeaderInputMode::Edit || !self.card_field_editable() {
            return;
        }

        if self.delete_selection() {
            return;
        }
        if self.header_cursor == 0 {
            return;
        }
        let cursor = self.header_cursor;
        if let Some(field) = self.current_card_field_mut() {
            let mut chars: Vec<char> = field.value.chars().collect();
            if cursor <= chars.len() {
                chars.remove(cursor - 1);
                field.value = chars.iter().collect();
                self.header_cursor = cursor - 1;
            }
        }
    }

    pub fn card_delete(&mut self) {
        if self.header_input_mode != HeaderInputMode::Edit || !self.card_field_editable() {
            return;
        }
        if self.delete_selection() {
            return;
        }
        let cursor = self.header_cursor;
        if let Some(field) = self.current_card_field_mut() {
            let mut chars: Vec<char> = field.value.chars().collect();
            if cursor < chars.len() {
                chars.remove(cursor);
                field.value = chars.iter().collect();
            }
        }
    }

    pub fn card_begin_typing(&mut self, ch: char) {
        if !self.card_field_editable() {
            return;
        }
        self.selection_anchor = None;
        self.header_original_value = self.current_card_field().map(|f| f.value.clone());
        if let Some(field) = self.current_card_field_mut() {
            field.value.clear();
        }
        self.header_input_mode = HeaderInputMode::Edit;
        self.header_cursor = 0;
        self.card_insert_char(ch);
    }

    pub fn card_revert_edit(&mut self) {
        if self.header_input_mode != HeaderInputMode::Edit {
            return;
        }
        if let Some(original) = self.header_original_value.take() {
            if let Some(field) = self.current_card_field_mut() {
                field.value = original;
            }
        }
        self.header_input_mode = HeaderInputMode::Select;
        self.header_cursor = 0;
        self.selection_anchor = None;
        self.clear_message();
    }

    pub fn select_mode(&mut self) {
        self.header_input_mode = HeaderInputMode::Select;
        self.header_original_value = None;
        self.selection_anchor = None;
        self.date_error = None;
    }

    /// Confirm the current edit. Runs type-specific parsing (Date/Time) and
    /// validation (pattern, required, length) before allowing focus to leave.
    /// Returns `true` if confirmed, `false` if validation failed (stays in Edit mode).
    pub fn card_confirm_edit(&mut self) -> bool {
        if self.header_input_mode != HeaderInputMode::Edit {
            return true;
        }
        let field_type = self.current_card_field()
            .map(|f| f.field_type.clone())
            .unwrap_or(FieldType::Text);

        // Type-specific parsing first (Date/Time shorthand)
        let parsed_ok = match field_type {
            FieldType::Date => self.confirm_date_field(),
            FieldType::Time => self.confirm_time_field(),
            _ => true,
        };
        if !parsed_ok {
            return false;
        }

        // General validation from the Validation struct
        if let Some(err) = self.validate_current_field() {
            self.set_error_message(err);
            // Select all text so the user can immediately retype
            self.select_all();
            return false;
        }

        // All good — clear any previous error and commit
        self.clear_message();
        if field_type != FieldType::Date && field_type != FieldType::Time {
            self.select_mode();
        }

        // Queue lookup validation if this field has a validate endpoint
        let validate = self.lookup_validate_action();
        if !matches!(validate, AppAction::None) {
            self.pending_validate = Some(validate);
        }

        true
    }

    /// Validate the current field's value against its `Validation` rules.
    /// Returns `Some(error_message)` on failure, `None` on success.
    fn validate_current_field(&self) -> Option<String> {
        let field = self.current_card_field()?;
        let value = &field.value;
        let label = &field.label;
        let validation = field.validation.as_ref()?;

        // Required check
        if validation.required == Some(true) && value.trim().is_empty() {
            return Some(format!("{} is required.", label));
        }

        // Skip remaining checks on empty optional fields
        if value.is_empty() {
            return None;
        }

        // Length checks (count chars once)
        if validation.max_length.is_some() || validation.min_length.is_some() {
            let len = value.chars().count();
            if let Some(max) = validation.max_length {
                if len > max {
                    return Some(format!("{} must be at most {} characters.", label, max));
                }
            }
            if let Some(min) = validation.min_length {
                if len < min {
                    return Some(format!("{} must be at least {} characters.", label, min));
                }
            }
        }

        // Pattern (regex)
        if let Some(ref pattern) = validation.pattern {
            if let Ok(re) = regex::Regex::new(pattern) {
                if !re.is_match(value) {
                    return Some(format!("\"{}\" is not a valid {}.", value, label.to_lowercase()));
                }
            }
        }

        // Numeric bounds (parse once)
        if validation.min.is_some() || validation.max.is_some() {
            if let Ok(v) = value.parse::<f64>() {
                if let Some(min) = validation.min {
                    if v < min {
                        return Some(format!("{} must be at least {}.", label, min));
                    }
                }
                if let Some(max) = validation.max {
                    if v > max {
                        return Some(format!("{} must be at most {}.", label, max));
                    }
                }
            }
        }

        None
    }

    fn confirm_date_field(&mut self) -> bool {
        let raw = self.current_card_field().map(|f| f.value.clone()).unwrap_or_default();
        if raw.trim().is_empty() {
            self.select_mode();
            return true;
        }
        let order = DateOrder::from_format(&self.locale.date_format);
        let reference = self.reference_date();
        match date_parse::parse_date_shorthand(&raw, reference, order) {
            Ok(date) => {
                let formatted = date_parse::format_date(date, order);
                if let Some(field) = self.current_card_field_mut() {
                    field.value = formatted;
                }
                self.date_error = None;
                self.select_mode();
                true
            }
            Err(msg) => {
                self.date_error = Some(msg.clone());
                self.set_error_message(format!("Date error: {}", msg));
                false
            }
        }
    }

    fn confirm_time_field(&mut self) -> bool {
        let raw = self.current_card_field().map(|f| f.value.clone()).unwrap_or_default();
        if raw.trim().is_empty() {
            self.select_mode();
            return true;
        }
        let now = chrono::Local::now().time();
        match time_parse::parse_time_shorthand(&raw, now) {
            Ok(time) => {
                let formatted = time_parse::format_time(time);
                if let Some(field) = self.current_card_field_mut() {
                    field.value = formatted;
                }
                self.date_error = None;
                self.select_mode();
                true
            }
            Err(msg) => {
                self.date_error = Some(msg.clone());
                self.set_error_message(format!("Time error: {}", msg));
                false
            }
        }
    }

    /// The reference date for date shortcuts (work_date if set, else today).
    fn reference_date(&self) -> chrono::NaiveDate {
        if let Some(ref wd) = self.current_screen.as_ref().and_then(|s| s.work_date.clone()) {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(wd, "%Y-%m-%d") {
                return d;
            }
        }
        chrono::Local::now().date_naive()
    }

    /// Insert a newline at the current flat cursor in a card TextArea field.
    /// Rejects the insertion if the value already has `max_lines` or more lines.
    /// Returns false if at the line limit, true if inserted.
    pub fn card_textarea_newline(&mut self, max_lines: usize) -> bool {
        if self.header_input_mode != HeaderInputMode::Edit { return false; }
        let line_count = self.current_card_field()
            .map(|f| f.value.chars().filter(|&c| c == '\n').count() + 1)
            .unwrap_or(1);
        if line_count >= max_lines {
            self.set_error_message(format!("Maximum {} lines.", max_lines));
            return false;
        }
        let cursor = self.header_cursor;
        if let Some(field) = self.current_card_field_mut() {
            let mut chars: Vec<char> = field.value.chars().collect();
            if cursor <= chars.len() {
                chars.insert(cursor, '\n');
                field.value = chars.iter().collect();
            }
        }
        self.header_cursor += 1;
        true
    }

    /// Insert a newline at the current flat cursor in an action form TextArea field.
    /// Rejects the insertion if the value already has `max_lines` or more lines.
    /// Returns false if at the line limit, true if inserted.
    pub fn action_form_textarea_newline(&mut self, max_lines: usize) -> bool {
        let line_count = self.action_form_fields.get(self.action_form_field_index)
            .map(|f| f.value.chars().filter(|&c| c == '\n').count() + 1)
            .unwrap_or(1);
        if line_count >= max_lines {
            self.set_error_message(format!("Maximum {} lines.", max_lines));
            return false;
        }
        let cursor = self.action_form_cursor;
        if let Some(field) = self.action_form_fields.get_mut(self.action_form_field_index) {
            let mut chars: Vec<char> = field.value.chars().collect();
            if cursor <= chars.len() {
                chars.insert(cursor, '\n');
                field.value = chars.iter().collect();
            }
        }
        self.action_form_cursor += 1;
        true
    }

    pub fn move_cursor_left(&mut self) {
        if let Some((start, _)) = self.selection_range() {
            self.header_cursor = start;
            self.selection_anchor = None;
        } else {
            self.selection_anchor = None;
            if self.header_cursor > 0 {
                self.header_cursor -= 1;
            }
        }
    }

    pub fn move_cursor_right(&mut self) {
        if let Some((_, end)) = self.selection_range() {
            self.header_cursor = end;
            self.selection_anchor = None;
        } else {
            self.selection_anchor = None;
            let len = self.current_card_field()
                .map(|f| f.value.chars().count())
                .unwrap_or(0);
            if self.header_cursor < len {
                self.header_cursor += 1;
            }
        }
    }

    pub fn card_cursor_word_left(&mut self) {
        let chars: Vec<char> = self
            .current_card_field()
            .map(|f| f.value.chars().collect())
            .unwrap_or_default();
        self.selection_anchor = None;
        self.header_cursor = find_word_boundary_left(&chars, self.header_cursor);
    }

    pub fn card_cursor_word_right(&mut self) {
        let chars: Vec<char> = self
            .current_card_field()
            .map(|f| f.value.chars().collect())
            .unwrap_or_default();
        self.selection_anchor = None;
        self.header_cursor = find_word_boundary_right(&chars, self.header_cursor);
    }

    pub fn card_cursor_end(&mut self) {
        self.header_cursor = self
            .current_card_field()
            .map(|f| f.value.chars().count())
            .unwrap_or(0);
    }

    // ----- Word delete -----

    pub fn card_delete_word_back(&mut self) {
        if self.header_input_mode != HeaderInputMode::Edit || !self.card_field_editable() {
            return;
        }
        if self.delete_selection() {
            return;
        }
        let chars: Vec<char> = self
            .current_card_field()
            .map(|f| f.value.chars().collect())
            .unwrap_or_default();
        let boundary = find_word_boundary_left(&chars, self.header_cursor);
        if boundary == self.header_cursor {
            return;
        }
        let cursor = self.header_cursor;
        if let Some(field) = self.current_card_field_mut() {
            let mut chars: Vec<char> = field.value.chars().collect();
            chars.drain(boundary..cursor);
            field.value = chars.iter().collect();
            self.header_cursor = boundary;
        }
    }

    // ----- Selection methods -----

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        let cursor = self.header_cursor;
        if anchor == cursor {
            return None;
        }
        Some((anchor.min(cursor), anchor.max(cursor)))
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    pub fn delete_selection(&mut self) -> bool {
        let (start, end) = match self.selection_range() {
            Some(r) => r,
            None => {
                self.selection_anchor = None;
                return false;
            }
        };
        if let Some(field) = self.current_card_field_mut() {
            let mut chars: Vec<char> = field.value.chars().collect();
            chars.drain(start..end);
            field.value = chars.iter().collect();
        }
        self.header_cursor = start;
        self.selection_anchor = None;
        true
    }

    pub fn select_all(&mut self) {
        if self.header_input_mode != HeaderInputMode::Edit {
            return;
        }
        let len = self
            .current_card_field()
            .map(|f| f.value.chars().count())
            .unwrap_or(0);
        self.selection_anchor = Some(0);
        self.header_cursor = len;
    }

    fn extend_selection(&mut self, new_cursor: usize) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.header_cursor);
        }
        self.header_cursor = new_cursor;
    }

    pub fn extend_selection_left(&mut self) {
        if self.header_cursor > 0 {
            let new = self.header_cursor - 1;
            self.extend_selection(new);
        }
    }

    pub fn extend_selection_right(&mut self) {
        let len = self
            .current_card_field()
            .map(|f| f.value.chars().count())
            .unwrap_or(0);
        if self.header_cursor < len {
            let new = self.header_cursor + 1;
            self.extend_selection(new);
        }
    }

    pub fn extend_selection_home(&mut self) {
        self.extend_selection(0);
    }

    pub fn extend_selection_end(&mut self) {
        let len = self
            .current_card_field()
            .map(|f| f.value.chars().count())
            .unwrap_or(0);
        self.extend_selection(len);
    }

    pub fn extend_selection_word_left(&mut self) {
        let chars: Vec<char> = self
            .current_card_field()
            .map(|f| f.value.chars().collect())
            .unwrap_or_default();
        let new = find_word_boundary_left(&chars, self.header_cursor);
        self.extend_selection(new);
    }

    pub fn extend_selection_word_right(&mut self) {
        let chars: Vec<char> = self
            .current_card_field()
            .map(|f| f.value.chars().collect())
            .unwrap_or_default();
        let new = find_word_boundary_right(&chars, self.header_cursor);
        self.extend_selection(new);
    }

    /// Copy the current field value to the system clipboard (Ctrl+C).
    pub fn copy_current_field(&mut self) {
        let (label, value) = match self.current_card_field() {
            Some(f) => (f.label.clone(), f.value.clone()),
            None => return,
        };

        if value.is_empty() {
            self.message = format!("{}: (tom)", label);
            return;
        }

        match cli_clipboard::set_contents(value.clone()) {
            Ok(_) => self.message = format!("{}: {}", self.ui_strings.copied, value),
            Err(e) => self.message = format!("Copy error: {}", e),
        }
    }

    /// Open a field value with the OS (Email → mailto:, Phone → tel:, URL → https://).
    fn open_field_in_os(&mut self, label: &str, field_type: &FieldType, value: &str) {
        if value.is_empty() {
            self.message = format!("{}: (tom)", label);
            return;
        }

        let url = match field_type {
            FieldType::URL => {
                if value.contains("://") {
                    value.to_string()
                } else {
                    format!("https://{}", value)
                }
            }
            FieldType::Email => format!("mailto:{}", value),
            FieldType::Phone => format!("tel:{}", value),
            _ => return,
        };

        self.message = open_in_browser(&url);
    }

    // ----- Server-driven menu methods -----

    pub fn server_menu_tabs(&self) -> &[two_wee_shared::MenuTab] {
        self.current_screen
            .as_ref()
            .and_then(|s| s.menu.as_ref())
            .map(|m| m.tabs.as_slice())
            .unwrap_or(&[])
    }

    pub fn server_menu_tab_count(&self) -> usize {
        self.server_menu_tabs().len()
    }

    pub fn server_menu_item_count(&self) -> usize {
        self.server_menu_tabs()
            .get(self.server_menu_tab)
            .map(|t| t.items.len())
            .unwrap_or(0)
    }

    fn server_menu_item_is_separator(item: &two_wee_shared::MenuItemDef) -> bool {
        matches!(item.action, two_wee_shared::MenuActionDef::Separator)
    }

    pub fn server_menu_next_tab(&mut self) {
        let len = self.server_menu_tab_count();
        if len == 0 || self.server_menu_tab + 1 >= len {
            return;
        }
        let row = self.server_menu_selected.get(self.server_menu_tab).copied().unwrap_or(0);
        self.server_menu_tab += 1;
        let new_count = self.server_menu_item_count();
        if new_count > 0 {
            self.server_menu_selected[self.server_menu_tab] = row.min(new_count - 1);
        }
    }

    pub fn server_menu_prev_tab(&mut self) {
        if self.server_menu_tab == 0 {
            return;
        }
        let row = self.server_menu_selected.get(self.server_menu_tab).copied().unwrap_or(0);
        self.server_menu_tab -= 1;
        let new_count = self.server_menu_item_count();
        if new_count > 0 {
            self.server_menu_selected[self.server_menu_tab] = row.min(new_count - 1);
        }
    }

    pub fn server_menu_next_item(&mut self) {
        if self.server_menu_selected.is_empty() {
            return;
        }
        let tabs = self.server_menu_tabs();
        let items = match tabs.get(self.server_menu_tab) {
            Some(t) => &t.items,
            None => return,
        };
        let count = items.len();
        let mut idx = self.server_menu_selected[self.server_menu_tab];
        while idx + 1 < count {
            idx += 1;
            if !Self::server_menu_item_is_separator(&items[idx]) {
                self.server_menu_selected[self.server_menu_tab] = idx;
                return;
            }
        }
    }

    pub fn server_menu_prev_item(&mut self) {
        if self.server_menu_selected.is_empty() {
            return;
        }
        let tabs = self.server_menu_tabs();
        let items = match tabs.get(self.server_menu_tab) {
            Some(t) => &t.items,
            None => return,
        };
        let mut idx = self.server_menu_selected[self.server_menu_tab];
        while idx > 0 {
            idx -= 1;
            if !Self::server_menu_item_is_separator(&items[idx]) {
                self.server_menu_selected[self.server_menu_tab] = idx;
                return;
            }
        }
    }

    pub fn server_menu_selected_item(&self) -> Option<&two_wee_shared::MenuItemDef> {
        let tabs = self.server_menu_tabs();
        let tab = tabs.get(self.server_menu_tab)?;
        let idx = self.server_menu_selected.get(self.server_menu_tab).copied().unwrap_or(0);
        tab.items.get(idx)
    }

    pub fn server_menu_enter_action(&mut self) -> AppAction {
        let item = match self.server_menu_selected_item() {
            Some(i) => i.clone(),
            None => return AppAction::None,
        };
        match &item.action {
            two_wee_shared::MenuActionDef::OpenScreen { url }
            | two_wee_shared::MenuActionDef::OpenMenu { url } => {
                let full_url = format!("{}{}", self.host_url, url);
                self.message = self.ui_strings.loading.clone();
                AppAction::FetchCardAndPush(full_url)
            }
            two_wee_shared::MenuActionDef::OpenUrl { url } => {
                self.message = open_in_browser(url);
                AppAction::None
            }
            two_wee_shared::MenuActionDef::Message { text } => {
                self.message = text.clone();
                AppAction::None
            }
            two_wee_shared::MenuActionDef::Separator => AppAction::None,
            two_wee_shared::MenuActionDef::Popup { items } => {
                self.menu_popup_items = items.clone();
                self.menu_popup_selected = 0;
                self.menu_popup_open = true;
                AppAction::None
            }
        }
    }

    pub fn menu_popup_enter_action(&mut self) -> AppAction {
        let item = match self.menu_popup_items.get(self.menu_popup_selected) {
            Some(i) => i.clone(),
            None => return AppAction::None,
        };
        self.menu_popup_open = false;
        match &item.action {
            two_wee_shared::PopupActionDef::OpenScreen { url }
            | two_wee_shared::PopupActionDef::OpenMenu { url } => {
                let full_url = format!("{}{}", self.host_url, url);
                self.message = self.ui_strings.loading.clone();
                AppAction::FetchCardAndPush(full_url)
            }
            two_wee_shared::PopupActionDef::OpenUrl { url } => {
                self.message = open_in_browser(url);
                AppAction::None
            }
            two_wee_shared::PopupActionDef::Message { text } => {
                self.message = text.clone();
                AppAction::None
            }
        }
    }

    pub fn menu_popup_next(&mut self) {
        if self.menu_popup_selected + 1 < self.menu_popup_items.len() {
            self.menu_popup_selected += 1;
        }
    }

    pub fn menu_popup_prev(&mut self) {
        if self.menu_popup_selected > 0 {
            self.menu_popup_selected -= 1;
        }
    }

    /// When Enter is pressed on a list row.
    /// If the list has `value_column` set (it's a lookup), returns `LookupReturn`.
    /// Otherwise uses the server-driven `on_select` URL template.
    pub fn list_enter_action(&mut self) -> AppAction {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return AppAction::None,
        };
        let lines = match &screen.lines {
            Some(l) => l,
            None => return AppAction::None,
        };
        let row = match lines.rows.get(self.table_state.selected) {
            Some(r) => r,
            None => return AppAction::None,
        };

        // Lookup list: extract value + autofill from the selected row
        if let Some(ref value_col) = lines.value_column {
            let col_index = lines.columns.iter().position(|c| c.id == *value_col);
            let value = col_index
                .and_then(|i| row.values.get(i))
                .cloned()
                .unwrap_or_default();

            let mut autofill_values = HashMap::new();
            for (col_id, target_field_id) in &lines.autofill {
                if let Some(ci) = lines.columns.iter().position(|c| c.id == *col_id) {
                    if let Some(v) = row.values.get(ci) {
                        autofill_values.insert(target_field_id.clone(), v.clone());
                    }
                }
            }

            return AppAction::LookupReturn { value, autofill: autofill_values };
        }

        // Regular list: on_select URL template
        let on_select = match &lines.on_select {
            Some(url) => url.clone(),
            None => return AppAction::None,
        };
        // Replace {0}, {1}, etc. with column values from the selected row
        let mut url = on_select;
        for (i, val) in row.values.iter().enumerate() {
            url = url.replace(&format!("{{{}}}", i), val);
        }
        let full_url = format!("{}{}", self.host_url, url);
        self.message = self.ui_strings.loading.clone();
        AppAction::FetchCardAndPush(full_url)
    }

    /// Ctrl+Enter on a list row: open the drill-down view using on_drill URL template.
    pub fn list_drill_action(&mut self) -> AppAction {
        let screen = match &self.current_screen {
            Some(s) => s,
            None => return AppAction::None,
        };
        let lines = match &screen.lines {
            Some(l) => l,
            None => return AppAction::None,
        };
        let on_drill = match &lines.on_drill {
            Some(url) => url.clone(),
            None => return AppAction::None,
        };
        let row = match lines.rows.get(self.table_state.selected) {
            Some(r) => r,
            None => return AppAction::None,
        };
        let mut url = on_drill;
        for (i, val) in row.values.iter().enumerate() {
            url = url.replace(&format!("{{{}}}", i), val);
        }
        let full_url = format!("{}{}", self.host_url, url);
        self.message = self.ui_strings.loading.clone();
        AppAction::FetchCardAndPush(full_url)
    }

    // ----- Theme & UI methods -----

    pub fn open_theme_modal(&mut self) {
        if self.quit_confirm_open {
            return;
        }
        self.theme_modal_open = true;
        self.theme_modal_index = match self.theme_mode {
            ThemeMode::Default => 0,
            ThemeMode::Navision => 1,
            ThemeMode::IbmAS400 => 2,
            ThemeMode::Color256 => 3,
        };
    }

    pub fn close_theme_modal(&mut self) {
        self.theme_modal_open = false;
    }

    pub fn copy_screen_json_to_clipboard(&mut self) {
        let json = self.current_screen.as_ref()
            .and_then(|s| serde_json::to_string_pretty(s).ok())
            .unwrap_or_else(|| "{}".to_string());
        let msg = "Screen JSON copied to clipboard".to_string();
        self.copy_to_clipboard(json, msg);
    }

    pub fn copy_url_to_clipboard(&mut self) {
        let url = self.current_screen_url.as_deref().unwrap_or("(no URL)");
        let msg = format!("URL copied: {}", url);
        self.copy_to_clipboard(url.to_string(), msg);
    }

    fn copy_to_clipboard(&mut self, content: String, success_msg: String) {
        use std::io::Write;
        let result = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(content.as_bytes())?;
                }
                child.wait()
            });
        match result {
            Ok(_) => self.set_timed_message(success_msg, Duration::from_secs(3)),
            Err(_) => self.set_error_message("Failed to copy — pbcopy not available".to_string()),
        }
    }

    pub fn request_quit(&mut self) {
        self.theme_modal_open = false;
        self.quit_confirm_open = true;
        self.quit_modal_index = 0;
    }

    /// Number of options in the quit modal (2 or 3 depending on login state).
    pub fn quit_modal_option_count(&self) -> usize {
        if self.auth_token.is_some() { 3 } else { 2 }
    }

    pub fn cancel_quit(&mut self) {
        self.quit_confirm_open = false;
        self.message = self.ui_strings.cancelled.clone();
    }

    pub fn confirm_quit(&mut self) {
        self.quit_confirm_open = false;
    }

    pub fn move_theme_next(&mut self) {
        self.theme_modal_index = (self.theme_modal_index + 1) % 4;
    }

    pub fn move_theme_prev(&mut self) {
        self.theme_modal_index = if self.theme_modal_index == 0 { 3 } else { self.theme_modal_index - 1 };
    }

    pub fn apply_theme_selection(&mut self) {
        let selected = match self.theme_modal_index {
            0 => ThemeMode::Default,
            1 => ThemeMode::Navision,
            2 => ThemeMode::IbmAS400,
            _ => ThemeMode::Color256,
        };
        self.set_theme(selected);
    }

    pub fn set_theme(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        self.theme = match mode {
            ThemeMode::Default => Theme::default_dark(),
            ThemeMode::Navision => Theme::navision(),
            ThemeMode::IbmAS400 => Theme::ibm_as400(),
            ThemeMode::Color256 => Theme::color_256(),
        };
    }

    pub fn theme_label(mode: ThemeMode) -> &'static str {
        match mode {
            ThemeMode::Default => "Default Dark",
            ThemeMode::Navision => "Navision Classic",
            ThemeMode::IbmAS400 => "IBM AS/400",
            ThemeMode::Color256 => "256 Color",
        }
    }

    pub fn capture_key_debug(&mut self, key: &KeyEvent) {
        let mods = if key.modifiers.is_empty() {
            String::from("NONE")
        } else {
            format!("{:?}", key.modifiers)
        };
        self.last_key_event = format!(
            "Key: {:?} mod:{} bits:{:#x} kind:{:?} state:{:?}",
            key.code,
            mods,
            key.modifiers.bits(),
            key.kind,
            key.state
        );
    }

    pub fn toggle_key_debug(&mut self) {
        self.key_debug_enabled = !self.key_debug_enabled;
        self.message = if self.key_debug_enabled {
            String::from("Key debug: ON")
        } else {
            String::from("Key debug: OFF")
        };
    }

    /// Apply an EditCycle stage to a cursor position and selection anchor.
    /// Returns (cursor, selection_anchor) for the new stage.
    fn apply_edit_stage(stage: EditCycle, value_len: usize) -> (usize, Option<usize>) {
        match stage {
            EditCycle::SelectAll  => (value_len, Some(0)),
            EditCycle::CursorEnd  => (value_len, None),
            EditCycle::CursorStart => (0, None),
            EditCycle::Idle       => (value_len, None), // shouldn't happen, safe default
        }
    }

    /// F2 cycle for card fields.
    pub fn card_f2_cycle(&mut self) {
        if !self.card_field_editable() {
            return;
        }
        if self.header_original_value.is_none() {
            self.header_original_value = self.current_card_field().map(|f| f.value.clone());
        }
        self.header_input_mode = HeaderInputMode::Edit;

        let len = self.current_card_field()
            .map(|f| f.value.chars().count())
            .unwrap_or(0);

        let is_textarea = self.current_card_field()
            .map(|f| f.field_type == two_wee_shared::FieldType::TextArea)
            .unwrap_or(false);

        // TextArea skips SelectAll (which would wipe multi-line content on first keypress)
        // and cycles only between CursorEnd and CursorStart.
        if is_textarea {
            self.edit_cycle = match self.edit_cycle {
                EditCycle::Idle | EditCycle::SelectAll | EditCycle::CursorStart => EditCycle::CursorEnd,
                EditCycle::CursorEnd => EditCycle::CursorStart,
            };
        } else {
            self.edit_cycle = self.edit_cycle.next();
        }

        let (cursor, anchor) = Self::apply_edit_stage(self.edit_cycle, len);
        self.header_cursor = cursor;
        self.selection_anchor = anchor;
    }

    /// F2 cycle for grid cells.
    pub fn grid_f2_cycle(&mut self) {
        if !self.grid_state.editing {
            if let Some(value) = self.grid_cell_value() {
                self.grid_state.original_value = Some(value.to_string());
            }
            self.grid_state.editing = true;
            // For decimal cells, convert raw "5.00" to locale "5,00" for editing
            if self.grid_col_is_decimal() {
                self.grid_raw_to_locale_edit();
            }
        }

        let len = self.grid_cell_value()
            .map(|v| v.chars().count())
            .unwrap_or(0);

        self.edit_cycle = self.edit_cycle.next();
        let (cursor, anchor) = Self::apply_edit_stage(self.edit_cycle, len);
        self.grid_state.cursor = cursor;
        self.grid_state.selection_anchor = anchor;
    }

    /// Reset the F2 cycle — call when focus moves to a different field.
    pub fn reset_edit_cycle(&mut self) {
        self.edit_cycle = EditCycle::Idle;
    }
}

/// Convert a char index to a byte index in a string.
fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Convert a char range [start..end) to a byte range (start_byte, end_byte).
fn char_byte_range(s: &str, start: usize, end: usize) -> (usize, usize) {
    let start_byte = char_to_byte(s, start);
    let end_byte = char_to_byte(s, end);
    (start_byte, end_byte)
}

fn find_word_boundary_left(chars: &[char], from: usize) -> usize {
    if chars.is_empty() || from == 0 {
        return 0;
    }
    let mut i = from.min(chars.len());
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    while i > 0 && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

fn find_word_boundary_right(chars: &[char], from: usize) -> usize {
    if chars.is_empty() {
        return 0;
    }
    let len = chars.len();
    let mut i = from.min(len);
    while i < len && !chars[i].is_whitespace() {
        i += 1;
    }
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    i
}
