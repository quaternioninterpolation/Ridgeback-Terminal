use egui;
use ridgeback_config::Config;
use crate::casting::CastManager;

/// Settings window state.
pub struct SettingsWindow {
    active_tab: SettingsTab,
    edited_config: Config,
    /// The profile key currently selected in the left list.
    selected_profile: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Profiles,
    Shortcuts,
    Rendering,
    Ai,
    CastShare,
}

impl SettingsWindow {
    pub fn new(config: Config) -> Self {
        let first = config.profiles.keys().next().cloned();
        Self {
            active_tab: SettingsTab::Profiles,
            edited_config: config,
            selected_profile: first,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, config: &mut Config, cast_manager: &mut CastManager) -> Vec<String> {
        let mut saved_profile_keys: Vec<String> = Vec::new();

        // Tab selector
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, SettingsTab::Profiles, "Profiles");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Shortcuts, "Shortcuts");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Rendering, "Rendering");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Ai, "AI");
            ui.selectable_value(&mut self.active_tab, SettingsTab::CastShare, "Cast / Share");
        });
        ui.separator();

        match self.active_tab {
            SettingsTab::Profiles => self.show_profiles(ui, config),
            SettingsTab::Shortcuts => self.show_shortcuts(ui, config),
            SettingsTab::Rendering => self.show_rendering(ui, config),
            SettingsTab::Ai => self.show_ai(ui, config),
            SettingsTab::CastShare => crate::casting::show_cast_panel(ui, cast_manager),
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                // Collect which profile keys changed before overwriting config
                for (key, new_profile) in &self.edited_config.profiles {
                    let changed = config.profiles.get(key)
                        .map(|old| old != new_profile)
                        .unwrap_or(true); // new profile
                    if changed {
                        saved_profile_keys.push(key.clone());
                    }
                }
                *config = self.edited_config.clone();
                if let Err(e) = config.save() {
                    tracing::error!("Failed to save config: {}", e);
                }
            }
            if ui.button("Reset to Defaults").clicked() {
                self.edited_config = Config::default();
            }
        });

        saved_profile_keys
    }

    fn show_profiles(&mut self, ui: &mut egui::Ui, _config: &Config) {
        // Collect keys once so we can borrow edited_config mutably later.
        let profile_keys: Vec<String> = self.edited_config.profiles.keys().cloned().collect();

        // Auto-select the first profile if nothing is selected yet.
        if self.selected_profile.is_none() || !self.edited_config.profiles.contains_key(self.selected_profile.as_deref().unwrap_or("")) {
            self.selected_profile = profile_keys.first().cloned();
        }

        ui.horizontal(|ui| {
            // ── Left panel: profile list ──────────────────────────────────
            ui.vertical(|ui| {
                ui.set_min_width(160.0);
                ui.set_max_width(160.0);
                ui.label(egui::RichText::new("Profiles").strong());
                ui.add_space(4.0);

                for key in &profile_keys {
                    let display_name = self.edited_config.profiles[key].name.clone();
                    let is_selected = self.selected_profile.as_deref() == Some(key.as_str());

                    // Draw a highlighted row for the selected profile.
                    let bg = if is_selected {
                        egui::Color32::from_rgb(50, 70, 130)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let fg = if is_selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_gray(200)
                    };

                    let resp = ui.add(
                        egui::Button::new(egui::RichText::new(&display_name).color(fg))
                            .fill(bg)
                            .stroke(egui::Stroke::NONE)
                            .min_size(egui::vec2(150.0, 24.0)),
                    );
                    if resp.clicked() {
                        self.selected_profile = Some(key.clone());
                    }
                }

                ui.add_space(8.0);
                if ui.button("+ New Profile").clicked() {
                    let mut new_profile = ridgeback_config::Profile::default_powershell();
                    new_profile.name = "New Profile".to_string();
                    let key = format!("profile_{}", profile_keys.len() + 1);
                    self.edited_config.profiles.insert(key.clone(), new_profile);
                    self.selected_profile = Some(key);
                }
            });

            ui.separator();

            // ── Right panel: editor for the selected profile ──────────────
            ui.vertical(|ui| {
                let sel = self.selected_profile.clone();
                if let Some(ref key) = sel {
                    if let Some(profile) = self.edited_config.profiles.get_mut(key) {
                        egui::Grid::new("profile_editor")
                            .num_columns(2)
                            .spacing([10.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Name:");
                                ui.text_edit_singleline(&mut profile.name);
                                ui.end_row();

                                ui.label("Shell:");
                                ui.text_edit_singleline(&mut profile.shell);
                                ui.end_row();

                                ui.label("Args:");
                                let mut args_str = profile.args.join(" ");
                                if ui.text_edit_singleline(&mut args_str).changed() {
                                    profile.args = args_str
                                        .split_whitespace()
                                        .map(String::from)
                                        .collect();
                                }
                                ui.end_row();

                                ui.label("Working Dir:");
                                let mut wd = profile.working_directory.to_string_lossy().to_string();
                                if ui.text_edit_singleline(&mut wd).changed() {
                                    profile.working_directory = std::path::PathBuf::from(&wd);
                                }
                                ui.end_row();

                                ui.label("Scrollback:");
                                ui.add(egui::DragValue::new(&mut profile.scrollback_limit).range(100..=100_000));
                                ui.end_row();

                                ui.label("Cursor Style:");
                                // Use key in the id_salt so the combo resets when profile changes.
                                egui::ComboBox::from_id_salt(format!("cursor_style_{}", key))
                                    .selected_text(format!("{:?}", profile.cursor_style))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut profile.cursor_style, ridgeback_config::CursorStyle::Block, "Block");
                                        ui.selectable_value(&mut profile.cursor_style, ridgeback_config::CursorStyle::Bar, "Bar");
                                        ui.selectable_value(&mut profile.cursor_style, ridgeback_config::CursorStyle::Underline, "Underline");
                                    });
                                ui.end_row();

                                ui.label("Cursor Blink:");
                                ui.checkbox(&mut profile.cursor_blink, "");
                                ui.end_row();

                                ui.label("Shader Effect:");
                                egui::ComboBox::from_id_salt(format!("shader_effect_{}", key))
                                    .selected_text(format!("{:?}", profile.shader_effect))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut profile.shader_effect, ridgeback_config::ShaderEffect::None, "None");
                                        ui.selectable_value(&mut profile.shader_effect, ridgeback_config::ShaderEffect::Crt, "CRT");
                                        ui.selectable_value(&mut profile.shader_effect, ridgeback_config::ShaderEffect::Fire, "Fire");
                                    });
                                ui.end_row();

                                ui.label("Text Colour:");
                                ui.text_edit_singleline(&mut profile.text_foreground);
                                ui.end_row();

                                ui.label("Text Shadow:");
                                ui.checkbox(&mut profile.text_shadow_enabled, "Enabled");
                                ui.end_row();

                                ui.label("Shadow Strength:");
                                ui.add(
                                    egui::Slider::new(&mut profile.text_shadow_alpha, 0.0..=1.0)
                                        .text("")
                                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                                );
                                ui.end_row();
                            });
                    }
                } else {
                    ui.label(egui::RichText::new("Select a profile on the left to edit it.")
                        .color(egui::Color32::from_gray(120)));
                }
            });
        });
    }

    fn show_shortcuts(&mut self, ui: &mut egui::Ui, _config: &Config) {
        ui.label(egui::RichText::new("Keyboard Shortcuts").strong());
        ui.add_space(8.0);

        egui::Grid::new("shortcuts_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Action").strong());
                ui.label(egui::RichText::new("Shortcut").strong());
                ui.end_row();

                let kb = &mut self.edited_config.keybindings;

                shortcut_row(ui, "New Terminal", &mut kb.new_tab);
                shortcut_row(ui, "Close Terminal", &mut kb.close_tab);
                shortcut_row(ui, "Next Tab", &mut kb.next_tab);
                shortcut_row(ui, "Previous Tab", &mut kb.prev_tab);
                shortcut_row(ui, "Open Settings", &mut kb.open_settings);
                shortcut_row(ui, "Save Session", &mut kb.save_session);
                shortcut_row(ui, "Find in Session", &mut kb.find_in_session);
                shortcut_row(ui, "AI Command Query", &mut kb.ai_command_query);
            });
    }

    fn show_rendering(&mut self, ui: &mut egui::Ui, _config: &Config) {
        ui.label(egui::RichText::new("Rendering Settings").strong());
        ui.add_space(8.0);

        let rendering = &mut self.edited_config.rendering;

        ui.checkbox(
            &mut rendering.update_in_background,
            "Update terminals in background",
        );
        ui.label("Continue rendering when the window is not focused.");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max shader FPS:");
            ui.add(
                egui::Slider::new(&mut rendering.max_shader_fps, 1..=240)
                    .text("fps"),
            );
        });
        ui.add_space(8.0);

        ui.checkbox(
            &mut rendering.battery_aware,
            "Battery-aware mode",
        );
        ui.label("Reduces shader effects and frame rate when on battery power.");
    }

    fn show_ai(&mut self, ui: &mut egui::Ui, _config: &Config) {
        ui.label(egui::RichText::new("AI Settings").strong());
        ui.add_space(8.0);

        let ai = &mut self.edited_config.ai;

        ui.checkbox(&mut ai.enabled, "Enable AI features");
        ui.add_space(8.0);

        if ai.enabled {
            ui.label("Backend:");
            egui::ComboBox::from_id_salt("ai_backend")
                .selected_text(format!("{:?}", ai.default_backend))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut ai.default_backend,
                        ridgeback_config::ai::AiBackendType::LmStudio,
                        "LM Studio",
                    );
                    ui.selectable_value(
                        &mut ai.default_backend,
                        ridgeback_config::ai::AiBackendType::OpenAi,
                        "OpenAI",
                    );
                    ui.selectable_value(
                        &mut ai.default_backend,
                        ridgeback_config::ai::AiBackendType::Claude,
                        "Claude",
                    );
                    ui.selectable_value(
                        &mut ai.default_backend,
                        ridgeback_config::ai::AiBackendType::Local,
                        "Local Model",
                    );
                });
            ui.add_space(8.0);

            // Backend-specific settings
            match ai.default_backend {
                ridgeback_config::ai::AiBackendType::LmStudio => {
                    egui::Grid::new("lm_studio_settings")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Base URL:");
                            ui.text_edit_singleline(&mut ai.backends.lm_studio.base_url);
                            ui.end_row();

                            ui.label("Model:");
                            ui.text_edit_singleline(&mut ai.backends.lm_studio.model);
                            ui.end_row();
                        });
                }
                ridgeback_config::ai::AiBackendType::OpenAi => {
                    egui::Grid::new("openai_settings")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("API Key:");
                            ui.add(egui::TextEdit::singleline(&mut ai.backends.openai.api_key).password(true));
                            ui.end_row();

                            ui.label("Model:");
                            ui.text_edit_singleline(&mut ai.backends.openai.model);
                            ui.end_row();
                        });
                }
                ridgeback_config::ai::AiBackendType::Claude => {
                    egui::Grid::new("claude_settings")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("API Key:");
                            ui.add(egui::TextEdit::singleline(&mut ai.backends.claude.api_key).password(true));
                            ui.end_row();

                            ui.label("Model:");
                            ui.text_edit_singleline(&mut ai.backends.claude.model);
                            ui.end_row();
                        });
                }
                ridgeback_config::ai::AiBackendType::Local => {
                    egui::Grid::new("local_settings")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("HuggingFace Repo:");
                            ui.text_edit_singleline(&mut ai.backends.local.model_repo);
                            ui.end_row();

                            ui.label("Quantization:");
                            ui.text_edit_singleline(&mut ai.backends.local.quantization);
                            ui.end_row();

                            ui.label("Device:");
                            egui::ComboBox::from_id_salt("local_device")
                                .selected_text(&ai.backends.local.device)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut ai.backends.local.device, "auto".to_string(), "Auto");
                                    ui.selectable_value(&mut ai.backends.local.device, "cpu".to_string(), "CPU");
                                    ui.selectable_value(&mut ai.backends.local.device, "cuda".to_string(), "CUDA");
                                });
                            ui.end_row();
                        });
                }
            }

            ui.add_space(12.0);
            ui.label(egui::RichText::new("Autocomplete").strong());
            ui.checkbox(&mut ai.autocomplete.enabled, "Enable autocomplete");
            ui.horizontal(|ui| {
                ui.label("Debounce:");
                ui.add(
                    egui::Slider::new(&mut ai.autocomplete.debounce_ms, 100..=500)
                        .text("ms"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Temperature:");
                ui.add(
                    egui::Slider::new(&mut ai.autocomplete.temperature, 0.0..=1.0),
                );
            });

            ui.add_space(12.0);
            ui.label(egui::RichText::new("Command Query").strong());
            ui.checkbox(&mut ai.command_query.enabled, "Enable command query (Ctrl+/)");
            ui.horizontal(|ui| {
                ui.label("Max suggestions:");
                ui.add(
                    egui::Slider::new(&mut ai.command_query.max_suggestions, 1..=5),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Temperature:");
                ui.add(
                    egui::Slider::new(&mut ai.command_query.temperature, 0.0..=1.0),
                );
            });
        }
    }
}

fn shortcut_row(ui: &mut egui::Ui, name: &str, binding: &mut String) {
    ui.label(name);
    ui.text_edit_singleline(binding);
    ui.end_row();
}
