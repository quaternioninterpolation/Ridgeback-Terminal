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
        // Use Cmd on macOS, Ctrl on other platforms
        #[cfg(target_os = "macos")]
        const MOD: &str = "Cmd";
        #[cfg(not(target_os = "macos"))]
        const MOD: &str = "Ctrl";

        Self {
            new_tab: format!("{MOD}+T"),
            close_tab: format!("{MOD}+W"),
            next_tab: format!("{MOD}+Tab"),
            prev_tab: format!("{MOD}+Shift+Tab"),
            open_settings: format!("{MOD}+,"),
            save_session: format!("{MOD}+S"),
            find_in_session: format!("{MOD}+F"),
            ai_command_query: format!("{MOD}+/"),
            split_horizontal: format!("{MOD}+Shift+D"),
            split_vertical: format!("{MOD}+Shift+E"),
            close_pane: format!("{MOD}+Shift+W"),
            reload_plugins: format!("{MOD}+Shift+P"),
            focus_next_group: format!("{MOD}+Alt+Right"),
            focus_prev_group: format!("{MOD}+Alt+Left"),
            move_tab_to_next_group: format!("{MOD}+Alt+Shift+Right"),
            move_tab_to_prev_group: format!("{MOD}+Alt+Shift+Left"),
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


