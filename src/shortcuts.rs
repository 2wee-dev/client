use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ---------------------------------------------------------------------------
// Action enum — every command shortcut in the app
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Global
    Quit,
    Save,
    Refresh,
    Escape,
    ToggleKeyDebug,
    ThemeModal,

    // List
    ListEnter,
    NewCard,

    // Card
    Delete,
    CopyField,
    OpenLines,
    EditCycle,

    // Context-dependent (F6 / Shift+Enter)
    Lookup,
    DrillDown,
    OptionCycle,
    OptionSelect,

    // Grid
    InsertRow,
    DeleteRow,

    // Actions
    ActionPicker,

    // Developer
    DebugJson,
    CopyUrl,
}

// ---------------------------------------------------------------------------
// KeyBinding — wraps crossterm key + modifiers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub const fn plain(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::NONE }
    }

    /// Format for display in the hint bar (e.g. "Ctrl+S", "F3", "Esc").
    pub fn display(&self) -> String {
        let mut result = String::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) { result.push_str("Ctrl+"); }
        if self.modifiers.contains(KeyModifiers::ALT) { result.push_str("Alt+"); }
        if self.modifiers.contains(KeyModifiers::SHIFT) { result.push_str("Shift+"); }
        match self.code {
            KeyCode::F(n) => result.push_str(&format!("F{}", n)),
            KeyCode::Char(c) => { for uc in c.to_uppercase() { result.push(uc); } }
            KeyCode::Enter => result.push_str("Enter"),
            KeyCode::Esc => result.push_str("Esc"),
            KeyCode::Tab => result.push_str("Tab"),
            KeyCode::Backspace => result.push_str("Backspace"),
            KeyCode::Delete => result.push_str("Delete"),
            KeyCode::Up => result.push('↑'),
            KeyCode::Down => result.push('↓'),
            KeyCode::Left => result.push('←'),
            KeyCode::Right => result.push('→'),
            KeyCode::Home => result.push_str("Home"),
            KeyCode::End => result.push_str("End"),
            KeyCode::PageUp => result.push_str("PgUp"),
            KeyCode::PageDown => result.push_str("PgDn"),
            _ => result.push_str(&format!("{:?}", self.code)),
        }
        result
    }
}

// ---------------------------------------------------------------------------
// ActionEntry — bindings + label for one action
// ---------------------------------------------------------------------------

struct ActionEntry {
    primary: KeyBinding,
    alternates: Vec<KeyBinding>,
    label: String,
    /// If set, this action also matches all bindings of the referenced action.
    aliases_of: Option<Action>,
}

// ---------------------------------------------------------------------------
// ShortcutRegistry
// ---------------------------------------------------------------------------

pub struct ShortcutRegistry {
    actions: HashMap<Action, ActionEntry>,
    /// Reverse lookup: key → action (only for unambiguous bindings).
    key_map: HashMap<KeyBinding, Action>,
}

impl std::fmt::Debug for ShortcutRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShortcutRegistry")
            .field("actions_count", &self.actions.len())
            .finish()
    }
}

impl ShortcutRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            actions: HashMap::new(),
            key_map: HashMap::new(),
        };
        reg.register_defaults();
        reg
    }

    /// Check if a key event matches a specific action (primary, alternates, or
    /// any binding inherited from the aliased parent action).
    pub fn matches(&self, action: Action, key: &KeyEvent) -> bool {
        let binding = key_event_to_binding(key);
        if let Some(entry) = self.actions.get(&action) {
            if entry.primary == binding || entry.alternates.contains(&binding) {
                return true;
            }
            // Follow alias chain: also match all bindings of the parent action.
            if let Some(parent) = entry.aliases_of {
                if let Some(parent_entry) = self.actions.get(&parent) {
                    if parent_entry.primary == binding || parent_entry.alternates.contains(&binding) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// O(1) reverse lookup: which action does this key trigger?
    /// Only returns unambiguous bindings (excludes F3/F4/F6 etc.).
    pub fn action_for(&self, key: &KeyEvent) -> Option<Action> {
        let binding = key_event_to_binding(key);
        self.key_map.get(&binding).copied()
    }

    /// Returns a display hint for a single action, e.g. "Ctrl+S Save".
    pub fn hint_for(&self, action: Action) -> String {
        if let Some(entry) = self.actions.get(&action) {
            format!("{} {}", entry.primary.display(), entry.label)
        } else {
            String::new()
        }
    }

    /// Returns formatted hints for multiple actions, e.g. "Ctrl+S Save  Esc Close  F3 Insert".
    /// Optionally override one action's label (e.g. Escape → "Clear" when searching).
    pub fn format_hints(&self, actions: &[Action]) -> String {
        self.format_hints_inner(actions, None)
    }

    /// Like `format_hints` but overrides the label for one specific action.
    pub fn format_hints_with_override(&self, actions: &[Action], override_action: Action, override_label: &str) -> String {
        self.format_hints_inner(actions, Some((override_action, override_label)))
    }

    fn format_hints_inner(&self, actions: &[Action], label_override: Option<(Action, &str)>) -> String {
        let mut result = String::new();
        for (i, &action) in actions.iter().enumerate() {
            if let Some(entry) = self.actions.get(&action) {
                if i > 0 { result.push_str("  "); }
                result.push_str(&entry.primary.display());
                result.push(' ');
                match label_override {
                    Some((oa, ol)) if oa == action => result.push_str(ol),
                    _ => result.push_str(&entry.label),
                }
            }
        }
        result.push_str("  ");
        result
    }

    // -----------------------------------------------------------------------
    // Default registrations
    // -----------------------------------------------------------------------

    fn register_defaults(&mut self) {
        use Action::*;

        // Global
        self.register(Quit, KeyBinding::plain(KeyCode::Char('q')),
            vec![KeyBinding::plain(KeyCode::Char('Q'))],
            "Quit");
        self.register(Save, KeyBinding::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            vec![], "Save");
        self.register(Refresh, KeyBinding::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            vec![KeyBinding::new(KeyCode::Char('R'), KeyModifiers::CONTROL)], "Refresh");
        self.register(Escape, KeyBinding::plain(KeyCode::Esc),
            vec![], "Close");
        self.register(ToggleKeyDebug, KeyBinding::plain(KeyCode::F(9)),
            vec![], "Debug");
        self.register(ThemeModal, KeyBinding::plain(KeyCode::F(12)),
            vec![], "Theme");

        // List
        self.register(ListEnter, KeyBinding::plain(KeyCode::Enter),
            vec![], "View");
        // NewCard: primary is Ctrl+N (shown in hint bar).
        // F3 and Ctrl+I are alternates. Ctrl+N and Ctrl+I are unambiguous → in key_map.
        self.register_ambiguous(NewCard, KeyBinding::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            vec![
                KeyBinding::plain(KeyCode::F(3)),
                KeyBinding::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
            ],
            &[
                KeyBinding::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                KeyBinding::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
            ], "New");

        // Card
        // Delete: primary is Ctrl+D (shown in hint bar).
        // F4 is an alternate. Ctrl+D is unambiguous → in key_map.
        self.register_ambiguous(Delete, KeyBinding::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            vec![KeyBinding::plain(KeyCode::F(4))],
            &[KeyBinding::new(KeyCode::Char('d'), KeyModifiers::CONTROL)], "Delete");
        self.register(CopyField, KeyBinding::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            vec![], "Copy");
        self.register(OpenLines, KeyBinding::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
            vec![
                KeyBinding::new(KeyCode::Char('L'), KeyModifiers::CONTROL),
                KeyBinding::new(KeyCode::Char('l'), KeyModifiers::ALT),
                KeyBinding::new(KeyCode::Char('L'), KeyModifiers::ALT),
                KeyBinding::plain(KeyCode::Char('¬')),  // macOS Alt+L
            ], "Lines");
        self.register(EditCycle, KeyBinding::plain(KeyCode::F(2)),
            vec![], "Edit");

        // Context-dependent — these share keys so are NOT in the reverse key_map.
        // Use matches() instead of action_for() for these.
        self.register_ambiguous(Lookup,
            KeyBinding::new(KeyCode::Enter, KeyModifiers::CONTROL),
            vec![KeyBinding::new(KeyCode::Enter, KeyModifiers::SHIFT), KeyBinding::new(KeyCode::Enter, KeyModifiers::ALT),
                 KeyBinding::new(KeyCode::Down, KeyModifiers::ALT),
                 KeyBinding::plain(KeyCode::F(6)), KeyBinding::new(KeyCode::Char('o'), KeyModifiers::CONTROL)], &[], "Lookup");
        self.register_ambiguous(DrillDown,
            KeyBinding::new(KeyCode::Enter, KeyModifiers::CONTROL),
            vec![KeyBinding::new(KeyCode::Enter, KeyModifiers::SHIFT), KeyBinding::new(KeyCode::Enter, KeyModifiers::ALT),
                 KeyBinding::new(KeyCode::Down, KeyModifiers::ALT),
                 KeyBinding::plain(KeyCode::F(6)), KeyBinding::new(KeyCode::Char('o'), KeyModifiers::CONTROL)], &[], "View");
        self.register_ambiguous(OptionCycle,
            KeyBinding::plain(KeyCode::Char(' ')),
            vec![KeyBinding::new(KeyCode::Down, KeyModifiers::ALT)], &[], "Cycle");
        self.register_ambiguous(OptionSelect,
            KeyBinding::new(KeyCode::Enter, KeyModifiers::CONTROL),
            vec![KeyBinding::new(KeyCode::Enter, KeyModifiers::SHIFT), KeyBinding::new(KeyCode::Enter, KeyModifiers::ALT),
                 KeyBinding::plain(KeyCode::F(6)), KeyBinding::plain(KeyCode::F(2))], &[], "Select");

        // Grid — aliases of NewCard/Delete, inheriting all their key bindings.
        self.register_alias(InsertRow, NewCard, KeyBinding::new(KeyCode::Char('n'), KeyModifiers::CONTROL), "Insert");
        self.register_alias(DeleteRow, Delete, KeyBinding::new(KeyCode::Char('d'), KeyModifiers::CONTROL), "Delete");

        // Actions
        self.register(ActionPicker, KeyBinding::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
            vec![KeyBinding::plain(KeyCode::F(8))], "Actions");

        // Developer
        self.register(DebugJson,
            KeyBinding::new(KeyCode::Char('j'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            vec![], "JSON");
        self.register(CopyUrl,
            KeyBinding::new(KeyCode::Char('u'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            vec![], "Copy URL");
    }

    /// Register an action with its bindings and add to the reverse key_map.
    fn register(&mut self, action: Action, primary: KeyBinding, alternates: Vec<KeyBinding>, label: &str) {
        // Add to reverse map
        self.key_map.insert(primary, action);
        for alt in &alternates {
            self.key_map.insert(*alt, action);
        }
        self.actions.insert(action, ActionEntry {
            primary,
            alternates,
            label: label.to_string(),
            aliases_of: None,
        });
    }

    /// Register an action WITHOUT adding the primary key to the reverse key_map
    /// (for ambiguous keys like F3/F4/F6). Optionally add specific unambiguous
    /// alternates to the key_map for O(1) lookup via `action_for()`.
    fn register_ambiguous(&mut self, action: Action, primary: KeyBinding, alternates: Vec<KeyBinding>,
                          unambiguous_keys: &[KeyBinding], label: &str) {
        for k in unambiguous_keys {
            self.key_map.insert(*k, action);
        }
        self.actions.insert(action, ActionEntry {
            primary,
            alternates,
            label: label.to_string(),
            aliases_of: None,
        });
    }

    /// Register a context-specific alias that inherits all bindings from a parent action.
    /// The alias gets its own primary/label (for hint display) but `matches()` also
    /// accepts any key that matches the parent.
    fn register_alias(&mut self, action: Action, parent: Action, primary: KeyBinding, label: &str) {
        self.actions.insert(action, ActionEntry {
            primary,
            alternates: vec![],
            label: label.to_string(),
            aliases_of: Some(parent),
        });
    }
}

/// Convert a crossterm KeyEvent to our KeyBinding (strips kind/state, keeps code+modifiers).
fn key_event_to_binding(key: &KeyEvent) -> KeyBinding {
    KeyBinding {
        code: key.code,
        modifiers: key.modifiers,
    }
}
