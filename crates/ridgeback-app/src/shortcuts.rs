use egui;
use ridgeback_config::keybindings::{KeyBindings, ShortcutAction};


/// Checks for keyboard shortcuts and dispatches actions.
pub struct ShortcutManager {
    bindings: Vec<(ShortcutAction, KeyCombo)>,
}

/// A parsed key combination (e.g., Ctrl+Shift+T / Cmd+Shift+T).
#[derive(Debug, Clone)]
struct KeyCombo {
    key: egui::Key,
    /// Platform command key: Cmd on macOS, Ctrl on Windows/Linux.
    command: bool,
    shift: bool,
    alt: bool,
}

impl ShortcutManager {
    pub fn from_config(keybindings: &KeyBindings) -> Self {
        let mut bindings = Vec::new();

        let pairs = [
            (ShortcutAction::NewTab, &keybindings.new_tab),
            (ShortcutAction::CloseTab, &keybindings.close_tab),
            (ShortcutAction::NextTab, &keybindings.next_tab),
            (ShortcutAction::PrevTab, &keybindings.prev_tab),
            (ShortcutAction::OpenSettings, &keybindings.open_settings),
            (ShortcutAction::SaveSession, &keybindings.save_session),
            (ShortcutAction::FindInSession, &keybindings.find_in_session),
            (ShortcutAction::AiCommandQuery, &keybindings.ai_command_query),
            (ShortcutAction::SplitHorizontal, &keybindings.split_horizontal),
            (ShortcutAction::SplitVertical, &keybindings.split_vertical),
            (ShortcutAction::ClosePane, &keybindings.close_pane),
            (ShortcutAction::ReloadPlugins, &keybindings.reload_plugins),
            (ShortcutAction::FocusNextGroup, &keybindings.focus_next_group),
            (ShortcutAction::FocusPrevGroup, &keybindings.focus_prev_group),
            (ShortcutAction::MoveTabNextGroup, &keybindings.move_tab_to_next_group),
            (ShortcutAction::MoveTabPrevGroup, &keybindings.move_tab_to_prev_group),
        ];

        for (action, combo_str) in pairs {
            if let Some(combo) = parse_key_combo(combo_str) {
                bindings.push((action, combo));
            } else {
                tracing::warn!("Failed to parse keybinding: {} = {}", action.display_name(), combo_str);
            }
        }

        Self { bindings }
    }

    /// Check if any shortcut was triggered this frame. Returns the first matching action.
    pub fn check(&self, ctx: &egui::Context) -> Option<ShortcutAction> {
        ctx.input(|input| {
            for (action, combo) in &self.bindings {
                // Use `command` which maps to Cmd on macOS, Ctrl on Windows/Linux
                let modifiers_match = input.modifiers.command == combo.command
                    && input.modifiers.shift == combo.shift
                    && input.modifiers.alt == combo.alt;

                if modifiers_match && input.key_pressed(combo.key) {
                    return Some(*action);
                }
            }
            None
        })
    }
}

fn parse_key_combo(s: &str) -> Option<KeyCombo> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut command = false;
    let mut shift = false;
    let mut alt = false;
    let mut key = None;

    for part in parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "cmd" => command = true,
            "shift" => shift = true,
            "alt" => alt = true,
            "tab" => key = Some(egui::Key::Tab),
            "enter" => key = Some(egui::Key::Enter),
            "escape" | "esc" => key = Some(egui::Key::Escape),
            "backspace" => key = Some(egui::Key::Backspace),
            "delete" | "del" => key = Some(egui::Key::Delete),
            "home" => key = Some(egui::Key::Home),
            "end" => key = Some(egui::Key::End),
            "pageup" => key = Some(egui::Key::PageUp),
            "pagedown" => key = Some(egui::Key::PageDown),
            "up" => key = Some(egui::Key::ArrowUp),
            "down" => key = Some(egui::Key::ArrowDown),
            "left" => key = Some(egui::Key::ArrowLeft),
            "right" => key = Some(egui::Key::ArrowRight),
            "space" => key = Some(egui::Key::Space),
            "/" => key = Some(egui::Key::Slash),
            "," => key = Some(egui::Key::Comma),
            "." => key = Some(egui::Key::Period),
            s if s.len() == 1 => {
                let c = s.chars().next().unwrap();
                key = char_to_egui_key(c);
            }
            _ => {
                tracing::warn!("Unknown key in combo: {}", part);
            }
        }
    }

    key.map(|k| KeyCombo {
        key: k,
        command,
        shift,
        alt,
    })
}

fn char_to_egui_key(c: char) -> Option<egui::Key> {
    match c.to_ascii_lowercase() {
        'a' => Some(egui::Key::A),
        'b' => Some(egui::Key::B),
        'c' => Some(egui::Key::C),
        'd' => Some(egui::Key::D),
        'e' => Some(egui::Key::E),
        'f' => Some(egui::Key::F),
        'g' => Some(egui::Key::G),
        'h' => Some(egui::Key::H),
        'i' => Some(egui::Key::I),
        'j' => Some(egui::Key::J),
        'k' => Some(egui::Key::K),
        'l' => Some(egui::Key::L),
        'm' => Some(egui::Key::M),
        'n' => Some(egui::Key::N),
        'o' => Some(egui::Key::O),
        'p' => Some(egui::Key::P),
        'q' => Some(egui::Key::Q),
        'r' => Some(egui::Key::R),
        's' => Some(egui::Key::S),
        't' => Some(egui::Key::T),
        'u' => Some(egui::Key::U),
        'v' => Some(egui::Key::V),
        'w' => Some(egui::Key::W),
        'x' => Some(egui::Key::X),
        'y' => Some(egui::Key::Y),
        'z' => Some(egui::Key::Z),
        '0' => Some(egui::Key::Num0),
        '1' => Some(egui::Key::Num1),
        '2' => Some(egui::Key::Num2),
        '3' => Some(egui::Key::Num3),
        '4' => Some(egui::Key::Num4),
        '5' => Some(egui::Key::Num5),
        '6' => Some(egui::Key::Num6),
        '7' => Some(egui::Key::Num7),
        '8' => Some(egui::Key::Num8),
        '9' => Some(egui::Key::Num9),
        _ => None,
    }
}
