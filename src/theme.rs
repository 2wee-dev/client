use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Default,
    Navision,
    IbmAS400,
    Color256,
}

/// Every color in the UI is a semantic token. To restyle the entire application,
/// change only the values here — no component code needs to change.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // All tokens are part of the design system, even if unused by current components
pub struct Theme {
    // --- Global ---
    /// The "desktop" chrome behind menus, login screen, and modals
    pub desktop: Color,
    /// Background of content areas (cards, tables, lists)
    pub content_bg: Color,
    /// Default text on the background
    pub text: Color,

    // --- Bars (top bar, status bar, hotkey bar) ---
    pub bar_bg: Color,
    pub bar_text: Color,
    /// Status messages in the status bar (often highlighted/bold)
    pub status_text: Color,
    /// Form validation error text (e.g. wrong password on login)
    pub form_error_text: Color,
    /// Error bar background (replaces entire bottom bar on validation error)
    pub error_bar_bg: Color,
    /// Error bar text (bright, high-contrast on error_bar_bg)
    pub error_bar_fg: Color,

    // --- Card ---
    /// Card/section border lines
    pub card_border: Color,
    /// Section title text (e.g., "Customer Card")
    pub card_title: Color,

    // --- Fields ---
    /// Label text and dot-padding (e.g., "Name........:")
    pub label: Color,
    /// Normal editable field value text
    pub field_value: Color,
    /// Read-only field value text (dimmed)
    pub field_readonly: Color,
    /// Dirty field indicator color (the * prefix)
    pub field_dirty: Color,

    // --- Field interaction states ---
    // Three states: focused (Select mode) → editing (Edit mode) → text selected (Shift+arrow)
    // Each is a progression, so the colors should feel related.

    /// Focused field in Select mode — the field "cursor" (arrow keys move between fields)
    pub field_focused_bg: Color,
    pub field_focused_fg: Color,
    /// Active field in Edit mode — typing into this field (Enter to start editing)
    pub field_editing_bg: Color,
    pub field_editing_fg: Color,
    /// Text selection within an active edit field (Shift+arrow to select text)
    pub field_text_selected_bg: Color,
    pub field_text_selected_fg: Color,

    // --- Table / List ---
    /// Table cell text
    pub table_text: Color,
    /// Table header text (background inherits from content_bg)
    pub table_header_fg: Color,
    /// Selected/highlighted row in a table
    pub table_selected_bg: Color,
    pub table_selected_fg: Color,

    // --- Grid (editable lines overlay) ---
    /// Grid background (blue in Navision)
    pub grid_bg: Color,
    /// Column/row dividers
    pub grid_line: Color,
    /// Normal cell text
    pub grid_text: Color,
    /// Grid header text (background inherits from grid_bg)
    pub grid_header_fg: Color,
    /// Cell in Select mode (focused but not editing)
    pub grid_cell_focused_bg: Color,
    pub grid_cell_focused_fg: Color,
    /// Cell in Edit mode (typing)
    pub grid_cell_editing_bg: Color,
    pub grid_cell_editing_fg: Color,

    // --- Menu ---
    /// Tab header strip
    pub tab_header_bg: Color,
    pub tab_header_text: Color,
    /// Selected menu item
    pub menu_selected_bg: Color,
    pub menu_selected_text: Color,
    /// Menu grid borders/separators
    pub menu_grid: Color,

    // --- Modals ---
    /// Option picker / dropdown modal
    pub modal_bg: Color,
    pub modal_text: Color,
    pub modal_border: Color,
    pub modal_selected_bg: Color,
    pub modal_selected_fg: Color,

}

impl Theme {
    /// Classic Navision 4.0 color scheme.
    ///
    /// Derived from the original DOS/Windows Navision palette:
    /// - Cyan (#00AAAA) bars and labels
    /// - Black (#000000) card/table background
    /// - White (#FFFFFF) field values
    /// - Yellow (#FFFF55) status and dirty indicator
    /// - Gray (#AAAAAA) selection highlight (inverted text)
    pub fn navision() -> Self {
        let cyan    = Color::Rgb(0x00, 0xAA, 0xAA);  // #00AAAA — the signature Navision teal
        let black   = Color::Rgb(0x00, 0x00, 0x00);  // #000000
        let white   = Color::Rgb(0xFF, 0xFF, 0xFF);  // #FFFFFF
        let yellow  = Color::Rgb(0xFF, 0xFF, 0x55);  // #FFFF55
        let blue    = Color::Rgb(0x00, 0x00, 0xAA);  // #0000AA — classic Navision desktop blue
        let dk_blue = Color::Rgb(0x00, 0x55, 0xAA);  // #0055AA — card/section borders
        let gray    = Color::Rgb(0xAA, 0xAA, 0xAA);  // #AAAAAA — selection highlight
        let dk_gray = Color::Rgb(0x55, 0x55, 0x55);  // #555555
        let deep_blue = Color::Rgb(0x00, 0x33, 0x77);  // #003377 — text selection (darker edit blue)
        let red     = Color::Rgb(0xAA, 0x00, 0x00);  // #AA0000 — menu selection
        let bright_red = Color::Rgb(0xFF, 0x00, 0x00); // #FF0000 — error bar background
        let navy    = Color::Rgb(0x00, 0x00, 0x77);  // #000077 — grid overlay (darker than desktop blue)
        let modal_dk = Color::Rgb(0x00, 0x00, 0x64);  // #000064 — modal background

        Self {
            // Global
            desktop: blue,
            content_bg: black,
            text: white,

            // Bars
            bar_bg: cyan,
            bar_text: black,
            status_text: yellow,
            form_error_text: red,
            error_bar_bg: bright_red,
            error_bar_fg: yellow,

            // Card
            card_border: dk_blue,
            card_title: white,

            // Fields
            label: cyan,
            field_value: white,
            field_readonly: dk_gray,

            field_dirty: yellow,

            // Field interaction states (focused → editing → text selected)
            field_focused_bg: gray,
            field_focused_fg: black,
            field_editing_bg: dk_blue,
            field_editing_fg: white,
            field_text_selected_bg: deep_blue,
            field_text_selected_fg: white,

            // Table
            table_text: white,
            table_header_fg: white,
            table_selected_bg: cyan,
            table_selected_fg: black,

            // Grid (editable lines overlay)
            grid_bg: navy,
            grid_line: cyan,
            grid_text: white,
            grid_header_fg: yellow,
            grid_cell_focused_bg: cyan,
            grid_cell_focused_fg: black,
            grid_cell_editing_bg: dk_blue,
            grid_cell_editing_fg: white,

            // Menu
            tab_header_bg: gray,
            tab_header_text: black,
            menu_selected_bg: red,
            menu_selected_text: white,
            menu_grid: cyan,

            // Modals
            modal_bg: modal_dk,
            modal_text: white,
            modal_border: white,
            modal_selected_bg: cyan,
            modal_selected_fg: black,

        }
    }

    /// IBM AS/400 (IBM i) theme — 5250 terminal palette, modernized.
    ///
    /// - Background:    #000000 — black screen
    /// - Green:         #6CFB45 — default text, labels, content
    /// - Soft Blue:     #678CF8 — borders, separators, titles, status highlights
    /// - White:         #FFFFFF — high-intensity / emphasized text
    /// - Orange Cursor: #F09B56 — dirty indicator, cursor/edit accent
    /// - Gray:          #A0A0A0 — muted / read-only text
    pub fn ibm_as400() -> Self {
        let bg     = Color::Rgb(0x00, 0x00, 0x00);  // #000000 — screen background
        let green  = Color::Rgb(0x6C, 0xFB, 0x45);  // #6CFB45 — default text / labels
        let blue   = Color::Rgb(0x67, 0x8C, 0xF8);  // #678CF8 — borders, titles, status
        let white  = Color::Rgb(0xFF, 0xFF, 0xFF);  // #FFFFFF — high-intensity text
        let orange = Color::Rgb(0xF0, 0x9B, 0x56);  // #F09B56 — dirty indicator / edit accent
        let gray   = Color::Rgb(0xA0, 0xA0, 0xA0);  // #A0A0A0 — muted / read-only
        let red    = Color::Rgb(0xFF, 0x00, 0x00);  // #FF0000 — errors / alerts

        let subtle  = Color::Rgb(0x00, 0x22, 0x00);  // dark green selection highlight
        let editing = Color::Rgb(0x1A, 0x10, 0x00);  // dark orange tint for edit mode

        Self {
            // Global
            desktop: bg,
            content_bg: bg,
            text: green,

            // Bars
            bar_bg: bg,
            bar_text: white,
            status_text: blue,
            form_error_text: red,
            error_bar_bg: red,
            error_bar_fg: white,

            // Card
            card_border: blue,
            card_title: blue,

            // Fields
            label: green,
            field_value: white,
            field_readonly: gray,

            field_dirty: orange,

            // Field interaction states
            field_focused_bg: subtle,
            field_focused_fg: white,
            field_editing_bg: editing,
            field_editing_fg: orange,
            field_text_selected_bg: blue,
            field_text_selected_fg: bg,

            // Table
            table_text: green,
            table_header_fg: white,
            table_selected_bg: subtle,
            table_selected_fg: green,

            // Menu
            tab_header_bg: bg,
            tab_header_text: white,
            menu_selected_bg: subtle,
            menu_selected_text: green,
            menu_grid: blue,

            // Modals
            modal_bg: bg,
            modal_text: green,
            modal_border: blue,
            modal_selected_bg: subtle,
            modal_selected_fg: green,

            // Grid
            grid_bg: bg,
            grid_line: blue,
            grid_text: green,
            grid_header_fg: white,
            grid_cell_focused_bg: subtle,
            grid_cell_focused_fg: green,
            grid_cell_editing_bg: editing,
            grid_cell_editing_fg: orange,

        }
    }

    /// 256-color variant of the default dark theme.
    ///
    /// Uses only xterm `Color::Indexed(n)` values (0–255) so it renders correctly
    /// in terminals that don't support true color — including macOS Terminal.app.
    /// Each color is the closest xterm-256 match to the Default Dark RGB palette.
    ///
    /// Palette mapping (Default Dark RGB → xterm-256 index):
    /// - base       #1e1e2e → 235 #262626  (nearest dark grey)
    /// - base_deeper#141420 → 233 #121212  (deeper dark)
    /// - base_raised#282840 → 237 #3a3a3a  (slightly raised)
    /// - teal       #56b6c2 →  73 #5fafaf  (closest cyan-teal)
    /// - teal_dark  #2e6b73 →  66 #5f8787  (muted teal; no darker match in cube)
    /// - blue_muted #3d5a80 →  60 #5f5f87  (muted blue)
    /// - blue_deep  #2a3f5f →  17 #00005f  (deep blue)
    /// - text       #e0e0e0 → 253 #dadada  (off-white)
    /// - text_dim   #808890 → 102 #878787  (grey)
    /// - gold       #e0c080 → 179 #d7af5f  (warm yellow)
    /// - highlight  #3a3a5c → 237 #3a3a3a  (dark selection bg)
    /// - red_soft   #e06c75 → 167 #d75f5f  (soft red)
    /// - error_bg   #C02B2B → 160 #d70000  (bold red)
    /// - error_fg   #FFE080 → 221 #ffd75f  (warm bright yellow)
    pub fn color_256() -> Self {
        let base        = Color::Indexed(235);  // #262626 ≈ #1e1e2e
        let base_deeper = Color::Indexed(233);  // #121212 ≈ #141420
        let base_raised = Color::Indexed(237);  // #3a3a3a ≈ #282840

        let teal        = Color::Indexed(73);   // #5fafaf ≈ #56b6c2
        let teal_dark   = Color::Indexed(66);   // #5f8787 ≈ #2e6b73
        let blue_muted  = Color::Indexed(60);   // #5f5f87 ≈ #3d5a80
        let blue_deep   = Color::Indexed(17);   // #00005f ≈ #2a3f5f

        let text        = Color::Indexed(253);  // #dadada ≈ #e0e0e0
        let text_dim    = Color::Indexed(102);  // #878787 ≈ #808890
        let gold        = Color::Indexed(179);  // #d7af5f ≈ #e0c080

        let highlight   = Color::Indexed(237);  // #3a3a3a ≈ #3a3a5c
        let red_soft    = Color::Indexed(167);  // #d75f5f ≈ #e06c75

        Self {
            desktop: base,
            content_bg: base,
            text,

            bar_bg: teal_dark,
            bar_text: text,
            status_text: gold,
            form_error_text: red_soft,
            error_bar_bg: Color::Indexed(160),  // #d70000 ≈ #C02B2B
            error_bar_fg: Color::Indexed(221),  // #ffd75f ≈ #FFE080

            card_border: teal_dark,
            card_title: teal,

            label: teal,
            field_value: text,
            field_readonly: text_dim,
            field_dirty: gold,

            field_focused_bg: highlight,
            field_focused_fg: text,
            field_editing_bg: blue_muted,
            field_editing_fg: text,
            field_text_selected_bg: blue_deep,
            field_text_selected_fg: text,

            table_text: text,
            table_header_fg: text,
            table_selected_bg: teal,
            table_selected_fg: base,

            grid_bg: base_deeper,
            grid_line: teal_dark,
            grid_text: text,
            grid_header_fg: text,
            grid_cell_focused_bg: highlight,
            grid_cell_focused_fg: text,
            grid_cell_editing_bg: blue_muted,
            grid_cell_editing_fg: text,

            tab_header_bg: teal_dark,
            tab_header_text: text,
            menu_selected_bg: red_soft,
            menu_selected_text: text,
            menu_grid: teal_dark,

            modal_bg: base_raised,
            modal_text: text,
            modal_border: teal_dark,
            modal_selected_bg: teal,
            modal_selected_fg: base,
        }
    }

    /// Default dark theme — inspired by Catppuccin Mocha / One Dark / VS Code Dark+,
    /// with modernized Navision teal as the accent color.
    pub fn default_dark() -> Self {
        let base        = Color::Rgb(0x1e, 0x1e, 0x2e);  // #1e1e2e — off-black
        let base_deeper = Color::Rgb(0x14, 0x14, 0x20);  // #141420 — grid overlay (darker than base)
        let base_raised = Color::Rgb(0x28, 0x28, 0x40);  // #282840 — modal elevation

        let teal        = Color::Rgb(0x56, 0xb6, 0xc2);  // #56b6c2 — modernized Navision cyan
        let teal_dark   = Color::Rgb(0x2e, 0x6b, 0x73);  // #2e6b73 — borders, muted accent
        let blue_muted  = Color::Rgb(0x3d, 0x5a, 0x80);  // #3d5a80 — edit field background
        let blue_deep   = Color::Rgb(0x2a, 0x3f, 0x5f);  // #2a3f5f — text selection

        let text        = Color::Rgb(0xe0, 0xe0, 0xe0);  // #e0e0e0 — primary text (off-white)
        let text_dim    = Color::Rgb(0x80, 0x88, 0x90);  // #808890 — read-only / secondary
        let gold        = Color::Rgb(0xe0, 0xc0, 0x80);  // #e0c080 — dirty indicator, status

        let highlight   = Color::Rgb(0x3a, 0x3a, 0x5c);  // #3a3a5c — focused field (Select mode)
        let red_soft    = Color::Rgb(0xe0, 0x6c, 0x75);  // #e06c75 — menu selection accent

        Self {
            // Global — everything shares one dark base
            desktop: base,
            content_bg: base,
            text,

            // Bars
            bar_bg: teal_dark,
            bar_text: text,
            status_text: gold,
            form_error_text: red_soft,
            error_bar_bg: Color::Rgb(0xC0, 0x2B, 0x2B),  // #C02B2B — bold red, not neon
            error_bar_fg: Color::Rgb(0xFF, 0xE0, 0x80),   // #FFE080 — warm bright yellow

            // Card
            card_border: teal_dark,
            card_title: teal,

            // Fields
            label: teal,
            field_value: text,
            field_readonly: text_dim,

            field_dirty: gold,

            // Field interaction states
            field_focused_bg: highlight,
            field_focused_fg: text,
            field_editing_bg: blue_muted,
            field_editing_fg: text,
            field_text_selected_bg: blue_deep,
            field_text_selected_fg: text,

            // Table
            table_text: text,
            table_header_fg: text,
            table_selected_bg: teal,
            table_selected_fg: base,

            // Grid
            grid_bg: base_deeper,
            grid_line: teal_dark,
            grid_text: text,
            grid_header_fg: text,
            grid_cell_focused_bg: highlight,
            grid_cell_focused_fg: text,
            grid_cell_editing_bg: blue_muted,
            grid_cell_editing_fg: text,

            // Menu
            tab_header_bg: teal_dark,
            tab_header_text: text,
            menu_selected_bg: red_soft,
            menu_selected_text: text,
            menu_grid: teal_dark,

            // Modals — slightly raised for elevation
            modal_bg: base_raised,
            modal_text: text,
            modal_border: teal_dark,
            modal_selected_bg: teal,
            modal_selected_fg: base,

        }
    }
}
