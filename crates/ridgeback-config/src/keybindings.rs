use serde::{Deserialize, Serialize};

/// Keyboard shortcut bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBindings {
    pub new_tab: String,
    pub close_tab: String,
    pub next_tab: String,
    pub prev_tab: String,
    pub open_settings: String,
    pub save_session: String,
    pub find_in_session: String,
    pub ai_command_query: String,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            new_tab: "Ctrl+T".to_string(),
            close_tab: "Ctrl+W".to_string(),
            next_tab: "Ctrl+Tab".to_string(),
            prev_tab: "Ctrl+Shift+Tab".to_string(),
            open_settings: "Ctrl+,".to_string(),
            save_session: "Ctrl+S".to_string(),
            find_in_session: "Ctrl+F".to_string(),
            ai_command_query: "Ctrl+/".to_string(),
        }
    }
}

/// Actions that can be bound to shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    OpenSettings,
    SaveSession,
    FindInSession,
    AiCommandQuery,
}

impl ShortcutAction {
    pub fn all() -> &'static [ShortcutAction] {
        &[
            ShortcutAction::NewTab,
            ShortcutAction::CloseTab,
            ShortcutAction::NextTab,
            ShortcutAction::PrevTab,
            ShortcutAction::OpenSettings,
            ShortcutAction::SaveSession,
            ShortcutAction::FindInSession,
            ShortcutAction::AiCommandQuery,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ShortcutAction::NewTab => "New Terminal",
            ShortcutAction::CloseTab => "Close Terminal",
            ShortcutAction::NextTab => "Next Tab",
            ShortcutAction::PrevTab => "Previous Tab",
            ShortcutAction::OpenSettings => "Open Settings",
            ShortcutAction::SaveSession => "Save Session",
            ShortcutAction::FindInSession => "Find in Session",
            ShortcutAction::AiCommandQuery => "AI Command Query",
        }
    }
}
