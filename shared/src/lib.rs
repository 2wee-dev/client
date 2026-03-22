use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Server-driven actions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Simple,
    Confirm,
    Modal,
}

fn default_action_kind() -> ActionKind {
    ActionKind::Simple
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_action_kind")]
    pub kind: ActionKind,
    #[serde(default)]
    pub fields: Vec<ActionField>,
    pub endpoint: String,
    #[serde(default)]
    pub confirm_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionField {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Option<OptionValues>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u8>,
    #[serde(default)]
    pub validation: Option<Validation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionRequest {
    pub action_id: String,
    /// Human-readable screen title (e.g. "Sales Order - SO-1001").
    /// Use `record_id` for the machine identifier of the current record.
    pub screen_title: String,
    #[serde(default)]
    pub record_id: Option<String>,
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

/// Response to a screen action (POST to ActionDef.endpoint).
///
/// Navigation priority when multiple fields are set (only one should be):
///   1. redirect_url — clears history, navigates to URL
///   2. push_url     — pushes URL onto stack
///   3. screen       — replaces current screen inline
///   4. (none)       — client refreshes current URL
///
/// On failure (success: false), all navigation fields are ignored.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionResponse {
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// Clears navigation history and navigates to this URL. Use when an action
    /// transforms the record into something else (e.g. posting an invoice).
    /// Escape from the destination is handled by that screen's parent_url.
    #[serde(default)]
    pub redirect_url: Option<String>,
    /// Pushes this URL onto the navigation stack. Use for non-destructive redirects
    /// (e.g. print preview, report). Escape returns to the current screen naturally.
    #[serde(default)]
    pub push_url: Option<String>,
    /// Replaces the current screen inline. Lower priority than redirect_url/push_url.
    #[serde(default)]
    pub screen: Option<Box<ScreenContract>>,
}

// ---------------------------------------------------------------------------
// Screen contract — the "HTML page" equivalent for 2wee
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScreenContract {
    pub layout: LayoutKind,
    pub title: String,
    /// Machine identifier for this screen (e.g. "customer_card", "sales_order").
    /// Echoed back by the client in SaveChangeset and DeleteRequest.
    /// Must be stable snake_case — never changes for a given resource type.
    #[serde(default)]
    pub screen_id: String,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub lines: Option<TableSpec>,
    #[serde(default)]
    pub menu: Option<MenuSpec>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub work_date: Option<String>,
    #[serde(default)]
    pub locale: Option<Locale>,
    #[serde(default)]
    pub ui_strings: Option<UiStrings>,
    /// When set, Ctrl+S posts fields to this URL as AuthRequest instead of SaveChangeset
    #[serde(default)]
    pub auth_action: Option<String>,
    /// Authenticated user's display name (for status bar)
    #[serde(default)]
    pub user_display_name: Option<String>,
    /// Action URLs keyed by name (e.g. "save", "create", "delete"). Values are always URLs.
    #[serde(default)]
    pub actions: HashMap<String, String>,
    /// The current record's machine identifier. Included in ActionRequest when executing
    /// screen actions. Empty string for screens with no record identity (menus, journals).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub record_id: String,
    /// Lines overlay height as percentage of body area (0–100). Default: 50.
    #[serde(default = "default_lines_overlay_pct")]
    pub lines_overlay_pct: u8,
    /// Summary totals rendered in a footer row below the grid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub totals: Vec<TotalField>,
    /// Server-driven contextual actions (send email, print, change status, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screen_actions: Vec<ActionDef>,
    /// The natural parent of this screen. When the client has no navigation history
    /// (e.g. after an action redirect), Escape fetches this URL instead of quitting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_url: Option<String>,
    /// When true, the lines overlay opens automatically on HeaderLines screens.
    /// Default: false (user opens with Ctrl+L).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lines_open: bool,
}

fn default_lines_overlay_pct() -> u8 { 50 }

/// A label+value pair for the totals footer row.
///
/// When `source_column` is set, the client computes the value live from grid
/// data instead of waiting for the next server response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TotalField {
    pub label: String,
    pub value: String,
    /// Column id to aggregate (e.g. "amount"). When present, the client
    /// computes this total live from grid data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_column: Option<String>,
    /// Aggregation function: "sum" (default), "count", "avg", "min", "max".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<String>,
    /// Formatting precision (number of decimal places). Defaults to 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Locale {
    /// Date display format: "DD-MM-YYYY" (Danish/European) or "MM-DD-YYYY" (US)
    #[serde(default = "default_date_format")]
    pub date_format: String,
    #[serde(default = "default_decimal_separator")]
    pub decimal_separator: String,
    #[serde(default = "default_thousand_separator")]
    pub thousand_separator: String,
}

fn default_date_format() -> String { "DD-MM-YYYY".to_string() }
fn default_decimal_separator() -> String { ",".to_string() }
fn default_thousand_separator() -> String { ".".to_string() }

impl Default for Locale {
    fn default() -> Self {
        Locale {
            date_format: default_date_format(),
            decimal_separator: default_decimal_separator(),
            thousand_separator: default_thousand_separator(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum LayoutKind {
    Card,
    List,
    HeaderLines,
    Grid,
    Menu,
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MenuSpec {
    pub panel_title: String,
    pub tabs: Vec<MenuTab>,
    #[serde(default)]
    pub top_left: Option<String>,
    #[serde(default)]
    pub top_right: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MenuTab {
    pub label: String,
    pub items: Vec<MenuItemDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MenuItemDef {
    pub label: String,
    pub action: MenuActionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MenuActionDef {
    /// Open a screen by fetching a URL (customer_list, customer_card, etc.)
    OpenScreen { url: String },
    /// Open a sub-menu by fetching a URL (pushes current menu onto screen stack)
    OpenMenu { url: String },
    /// Open a URL in the system default browser
    OpenUrl { url: String },
    /// Show a message in the status bar
    Message { text: String },
    /// Visual separator — non-selectable divider line between item groups
    Separator,
    /// Inline popup with a small list of items overlaid on the menu column
    Popup { items: Vec<PopupItemDef> },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PopupItemDef {
    pub label: String,
    pub action: PopupActionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PopupActionDef {
    OpenScreen { url: String },
    OpenMenu { url: String },
    OpenUrl { url: String },
    Message { text: String },
}

// ---------------------------------------------------------------------------
// Sections and Fields
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Section {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub column: u8,
    #[serde(default)]
    pub row_group: u8,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Field {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub editable: bool,
    #[serde(default)]
    pub width: Option<u16>,
    #[serde(default)]
    pub validation: Option<Validation>,
    /// Color name for this field's value text (e.g. "yellow", "red").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// When true, the field value is rendered bold.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default)]
    pub options: Option<OptionValues>,
    #[serde(default)]
    pub lookup: Option<LookupInfo>,
    #[serde(default)]
    pub placeholder: Option<String>,
    /// Number of visible rows for TextArea fields. Default: 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u8>,
    /// Text shown when the Boolean field is true. Default: "Yes".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub true_label: Option<String>,
    /// Text shown when the Boolean field is false. Default: "No".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub false_label: Option<String>,
    /// Color name for the true state (e.g. "green", "red"). Default: field value color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub true_color: Option<String>,
    /// Color name for the false state (e.g. "red"). Default: field value color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub false_color: Option<String>,
    /// When false, Enter skips this field (fast data entry path).
    /// Tab still visits all fields. Default: true.
    #[serde(default = "default_true")]
    pub quick_entry: bool,
    /// When true, the cursor starts on this field when the screen opens.
    /// Only one field per form should set this. Default: false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub focus: bool,
}

fn is_false(v: &bool) -> bool { !v }

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum FieldType {
    Text,
    Decimal,
    Integer,
    Date,
    Email,
    Phone,
    URL,
    Boolean,
    Option,
    Password,
    TextArea,
    DateRange,
    Time,
    Separator,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Validation {
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub min_length: Option<usize>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub input_mask: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub decimals: Option<u8>,
}


#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum OptionValues {
    Simple(Vec<String>),
    Labeled(Vec<OptionPair>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OptionPair {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LookupInfo {
    pub endpoint: String,
    #[serde(default)]
    pub display_field: Option<String>,
    /// When set, the client validates the field value on blur by calling this endpoint.
    /// e.g. "/validate/post_code" → GET /validate/post_code/{value}
    #[serde(default)]
    pub validate: Option<String>,
    /// "modal" for inline modal overlay, absent/None for full-screen list.
    #[serde(default)]
    pub display: Option<String>,
    /// Context fields whose current values are sent as query parameters.
    /// On cards: reads from other card fields. On grids: reads from same-row columns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<LookupContext>,
}

/// Maps a field/column value to a query parameter for context-dependent lookups.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LookupContext {
    /// The `id` of the field (card) or column (grid) to read the value from.
    pub field: String,
    /// The query parameter name. Defaults to `field` if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

/// Server response to a lookup field validation request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidateResponse {
    pub valid: bool,
    /// Autofill values to write to other card fields (only when valid).
    #[serde(default)]
    pub autofill: HashMap<String, String>,
    /// Error message to display (only when invalid).
    #[serde(default)]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableSpec {
    pub columns: Vec<ColumnDef>,
    #[serde(default)]
    pub rows: Vec<TableRow>,
    #[serde(default)]
    pub row_count: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    #[serde(default)]
    pub current_page: usize,
    #[serde(default)]
    pub selectable: bool,
    #[serde(default)]
    pub editable: bool,
    #[serde(default)]
    pub on_select: Option<String>,
    #[serde(default)]
    pub table_align: Option<String>,
    /// For lookup tables: which column's value is returned to the originating field.
    #[serde(default)]
    pub value_column: Option<String>,
    /// For lookup tables: maps column IDs in this table → field IDs on the originating card.
    #[serde(default)]
    pub autofill: HashMap<String, String>,
    /// URL template for Ctrl+Enter drill-down into a row's detail view.
    /// Uses {0}, {1}, etc. for column value substitution, same as on_select.
    #[serde(default)]
    pub on_drill: Option<String>,
}

fn default_page_size() -> usize {
    25
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ColumnDef {
    pub id: String,
    pub label: String,
    #[serde(rename = "type", default = "default_text_type")]
    pub col_type: FieldType,
    #[serde(default)]
    pub width: ColumnWidth,
    #[serde(default)]
    pub align: ColumnAlign,
    #[serde(default)]
    pub editable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<OptionValues>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup: Option<LookupInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<Validation>,
    /// When false, Enter skips this column. Tab still visits all columns. Default: true.
    #[serde(default = "default_true")]
    pub quick_entry: bool,
    /// Arithmetic formula referencing other column IDs. Evaluated client-side after each edit.
    /// E.g. "quantity * unit_price * (1 - line_discount_pct / 100)"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
}

fn default_text_type() -> FieldType {
    FieldType::Text
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(untagged)]
pub enum ColumnWidth {
    Fixed(u16),
    Fill(String),
}

impl Default for ColumnWidth {
    fn default() -> Self {
        ColumnWidth::Fixed(10)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ColumnAlign {
    #[default]
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableRow {
    pub index: usize,
    pub values: Vec<String>,
}

// ---------------------------------------------------------------------------
// UI strings (server-driven i18n)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiStrings {
    // Save confirmation modal
    #[serde(default)]
    pub save_confirm_title: String,
    #[serde(default)]
    pub save_confirm_message: String,
    #[serde(default)]
    pub save_confirm_save: String,
    #[serde(default)]
    pub save_confirm_discard: String,
    #[serde(default)]
    pub save_confirm_cancel: String,

    // Quit confirmation modal
    #[serde(default)]
    pub quit_title: String,
    #[serde(default)]
    pub quit_message: String,
    #[serde(default)]
    pub quit_yes: String,
    #[serde(default)]
    pub quit_no: String,

    // Logout
    #[serde(default)]
    pub logout: String,

    // Common status messages
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub deleted: String,
    #[serde(default)]
    pub saved: String,
    #[serde(default)]
    pub saving: String,
    #[serde(default)]
    pub loading: String,
    #[serde(default)]
    pub cancelled: String,
    #[serde(default)]
    pub no_changes: String,
    #[serde(default)]
    pub copied: String,

    // Error messages
    #[serde(default)]
    pub error_prefix: String,
    #[serde(default)]
    pub save_error_prefix: String,
    #[serde(default)]
    pub login_error: String,
    #[serde(default)]
    pub server_unavailable: String,
    #[serde(default)]
    pub connecting: String,
}

impl Default for UiStrings {
    fn default() -> Self {
        UiStrings {
            save_confirm_title: String::new(),
            save_confirm_message: String::new(),
            save_confirm_save: String::new(),
            save_confirm_discard: String::new(),
            save_confirm_cancel: String::new(),
            quit_title: String::new(),
            quit_message: String::new(),
            quit_yes: String::new(),
            quit_no: String::new(),
            logout: String::new(),
            created: String::new(),
            deleted: String::new(),
            saved: String::new(),
            saving: String::new(),
            loading: String::new(),
            cancelled: String::new(),
            no_changes: String::new(),
            copied: String::new(),
            error_prefix: String::new(),
            save_error_prefix: String::new(),
            login_error: String::new(),
            server_unavailable: String::new(),
            connecting: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Delete request (client → server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteRequest {
    pub screen_id: String,
    pub record_id: String,
}

// ---------------------------------------------------------------------------
// Save changeset (client → server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SaveChangeset {
    pub screen_id: String,
    pub record_id: String,
    pub changes: HashMap<String, String>,
    #[serde(default)]
    pub action: Option<String>,
    /// Grid lines data: each inner Vec<String> is one row's column values.
    #[serde(default)]
    pub lines: Vec<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Authentication (client ↔ server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthRequest {
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuthResponse {
    pub success: bool,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub screen: Option<ScreenContract>,
}

// ---------------------------------------------------------------------------
// JSON Schema generation
// ---------------------------------------------------------------------------

#[cfg(test)]
mod schema_tests {
    use super::*;
    use schemars::schema_for;
    use std::fs;
    use std::path::Path;

    #[test]
    fn generate_json_schemas() {
        let schema_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas");
        fs::create_dir_all(&schema_dir).expect("Failed to create schemas directory");

        let schemas: Vec<(&str, serde_json::Value)> = vec![
            (
                "screen_contract.schema.json",
                serde_json::to_value(schema_for!(ScreenContract)).unwrap(),
            ),
            (
                "auth_request.schema.json",
                serde_json::to_value(schema_for!(AuthRequest)).unwrap(),
            ),
            (
                "auth_response.schema.json",
                serde_json::to_value(schema_for!(AuthResponse)).unwrap(),
            ),
            (
                "save_changeset.schema.json",
                serde_json::to_value(schema_for!(SaveChangeset)).unwrap(),
            ),
            (
                "delete_request.schema.json",
                serde_json::to_value(schema_for!(DeleteRequest)).unwrap(),
            ),
            (
                "validate_response.schema.json",
                serde_json::to_value(schema_for!(ValidateResponse)).unwrap(),
            ),
            (
                "action_request.schema.json",
                serde_json::to_value(schema_for!(ActionRequest)).unwrap(),
            ),
            (
                "action_response.schema.json",
                serde_json::to_value(schema_for!(ActionResponse)).unwrap(),
            ),
        ];

        for (filename, schema) in &schemas {
            let path = schema_dir.join(filename);
            let json = serde_json::to_string_pretty(schema).unwrap();
            fs::write(&path, json).unwrap_or_else(|e| {
                panic!("Failed to write {}: {}", path.display(), e);
            });
            println!("Wrote {}", path.display());
        }
    }
}
