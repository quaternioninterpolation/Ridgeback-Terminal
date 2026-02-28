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
    /// Split active pane horizontally (side by side).
    pub split_horizontal: String,
    /// Split active pane vertically (top/bottom).
    pub split_vertical: String,
    /// Close / unsplit active pane (keep the other side).
    pub close_pane: String,
    /// Reload all Lua plugins without restarting.
    pub reload_plugins: String,
    /// Focus the next tab group.
    pub focus_next_group: String,
    /// Focus the previous tab group.
    pub focus_prev_group: String,
    /// Move the active tab to the next group.
    pub move_tab_to_next_group: String,
    /// Move the active tab to the previous group.
    pub move_tab_to_prev_group: String,
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
            split_horizontal: "Ctrl+Shift+D".to_string(),
            split_vertical: "Ctrl+Shift+E".to_string(),
            close_pane: "Ctrl+Shift+W".to_string(),
            reload_plugins: "Ctrl+Shift+P".to_string(),
            focus_next_group: "Ctrl+Alt+Right".to_string(),
            focus_prev_group: "Ctrl+Alt+Left".to_string(),
            move_tab_to_next_group: "Ctrl+Alt+Shift+Right".to_string(),
            move_tab_to_prev_group: "Ctrl+Alt+Shift+Left".to_string(),
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
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ReloadPlugins,
    FocusNextGroup,
    FocusPrevGroup,
    MoveTabNextGroup,
    MoveTabPrevGroup,
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
            ShortcutAction::SplitHorizontal,
            ShortcutAction::SplitVertical,
            ShortcutAction::ClosePane,
            ShortcutAction::ReloadPlugins,
            ShortcutAction::FocusNextGroup,
            ShortcutAction::FocusPrevGroup,
            ShortcutAction::MoveTabNextGroup,
            ShortcutAction::MoveTabPrevGroup,
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
            ShortcutAction::SplitHorizontal => "Split Horizontal",
            ShortcutAction::SplitVertical => "Split Vertical",
            ShortcutAction::ClosePane => "Close Pane",
            ShortcutAction::ReloadPlugins => "Reload Plugins",
            ShortcutAction::FocusNextGroup => "Focus Next Group",
            ShortcutAction::FocusPrevGroup => "Focus Previous Group",
            ShortcutAction::MoveTabNextGroup => "Move Tab to Next Group",
            ShortcutAction::MoveTabPrevGroup => "Move Tab to Previous Group",
        }
    }
}


