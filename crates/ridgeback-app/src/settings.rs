use egui;
use ridgeback_config::Config;
use ridgeback_plugin::{ShaderPluginHost, shader_plugin::{ParamType, TriggerMode}};
use crate::casting::CastManager;
use ridgeback_ai::LocalModelManager;
use ridgeback_ai::local_manager::{LocalModelStatus, detect_devices};
use crate::toast::{Toast, ToastManager};

/// Platform modifier key label: "Cmd" on macOS, "Ctrl" elsewhere.
#[cfg(target_os = "macos")]
const MOD: &str = "Cmd";
#[cfg(not(target_os = "macos"))]
const MOD: &str = "Ctrl";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaddingMode {
    Uniform,
    WidthHeight,
    Individual,
}

/// Settings window state.
pub struct SettingsWindow {
    active_tab: SettingsTab,
    edited_config: Config,
    /// The profile key currently selected in the left list.
    pub selected_profile: Option<String>,
    /// Profile key pending delete confirmation (shown in a modal dialog).
    pending_delete: Option<String>,
    /// Current padding edit mode per profile.
    padding_mode: PaddingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Profiles,
    Shortcuts,
    Rendering,
    Ai,
    Plugins,
    CastShare,
}

impl SettingsWindow {
    pub fn new(config: Config) -> Self {
        let first = config.profiles.keys().next().cloned();
        Self {
            active_tab: SettingsTab::Profiles,
            edited_config: config,
            selected_profile: first,
            pending_delete: None,
            padding_mode: PaddingMode::Uniform,
        }
    }

    /// Call this when the settings window is opened with an active terminal.
    /// Switches to the Profiles tab and selects the matching profile.
    pub fn set_focused_profile(&mut self, profile_key: Option<&str>) {
        if let Some(key) = profile_key {
            if self.edited_config.profiles.contains_key(key) {
                self.selected_profile = Some(key.to_string());
                self.active_tab = SettingsTab::Profiles;
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut Config,
        cast_manager: &mut CastManager,
        shader_host: &ShaderPluginHost,
        local_model_manager: &LocalModelManager,
        toasts: &mut ToastManager,
    ) -> Vec<String> {
        let mut saved_profile_keys: Vec<String> = Vec::new();

        // Tab selector
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, SettingsTab::Profiles, "Profiles");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Shortcuts, "Shortcuts");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Rendering, "Rendering");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Ai, "AI");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Plugins, "Plugins");
            ui.selectable_value(&mut self.active_tab, SettingsTab::CastShare, "Cast / Share");
        });
        ui.separator();

        // Reserve fixed space for the bottom Save/Reset bar.
        let bottom_bar_height = 36.0;
        let scroll_height = (ui.available_height() - bottom_bar_height).max(100.0);

        // Allocate a fixed-size rect for the scrollable body.
        // This prevents any content inside from influencing the parent size.
        let (body_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), scroll_height),
            egui::Sense::hover(),
        );

        // Render tab content inside a child UI pinned to body_rect.
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
        egui::ScrollArea::vertical()
            .max_height(scroll_height)
            .show(&mut body_ui, |ui| {
                match self.active_tab {
                    SettingsTab::Profiles => self.show_profiles(ui, config, shader_host),
                    SettingsTab::Shortcuts => self.show_shortcuts(ui, config),
                    SettingsTab::Rendering => self.show_rendering(ui, config),
                    SettingsTab::Ai => self.show_ai(ui, config, local_model_manager, toasts),
                    SettingsTab::Plugins => self.show_plugins(ui, shader_host),
                    SettingsTab::CastShare => crate::casting::show_cast_panel(ui, cast_manager),
                }
            });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                for (key, new_profile) in &self.edited_config.profiles {
                    let changed = config.profiles.get(key)
                        .map(|old| old != new_profile)
                        .unwrap_or(true);
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

    // ── Profiles tab ─────────────────────────────────────────────────────────

    fn show_profiles(&mut self, ui: &mut egui::Ui, _config: &Config, shader_host: &ShaderPluginHost) {
        let profile_keys: Vec<String> = self.edited_config.profiles.keys().cloned().collect();

        if self.selected_profile.is_none()
            || !self.edited_config.profiles.contains_key(self.selected_profile.as_deref().unwrap_or(""))
        {
            self.selected_profile = profile_keys.first().cloned();
        }

        ui.horizontal_top(|ui| {
            // ── Left: profile list (fixed 160px) ─────────────────────────
            ui.vertical(|ui| {
                ui.set_min_width(160.0);
                ui.set_max_width(160.0);
                ui.label(egui::RichText::new("Profiles").strong());
                ui.add_space(4.0);

                for key in &profile_keys {
                    let display_name = self.edited_config.profiles[key].name.clone();
                    let is_selected = self.selected_profile.as_deref() == Some(key.as_str());

                    let bg = if is_selected { egui::Color32::from_rgb(50, 70, 130) } else { egui::Color32::TRANSPARENT };
                    let fg = if is_selected { egui::Color32::WHITE } else { egui::Color32::from_gray(200) };

                    let resp = ui.add(
                        egui::Button::new(egui::RichText::new(&display_name).color(fg))
                            .fill(bg)
                            .stroke(egui::Stroke::NONE)
                            .min_size(egui::vec2(150.0, 24.0)),
                    );
                    if resp.clicked() {
                        self.selected_profile = Some(key.clone());
                    }

                    // Right-click context menu
                    resp.context_menu(|ui| {
                        if ui.button("📋 Duplicate").clicked() {
                            if let Some(profile) = self.edited_config.profiles.get(key) {
                                let mut new_profile = profile.clone();
                                let base_name = profile.name.clone();

                                // Count existing profiles whose names match
                                // "Name", "Name (1)", "Name (2)", etc.
                                let mut count = 1u32;
                                loop {
                                    let candidate = format!("{} ({})", base_name, count);
                                    let exists = self.edited_config.profiles.values()
                                        .any(|p| p.name == candidate);
                                    if !exists { break; }
                                    count += 1;
                                }
                                new_profile.name = format!("{} ({})", base_name, count);

                                // Generate a unique map key
                                let new_key = {
                                    let mut k = format!("{}_{}", key, count);
                                    while self.edited_config.profiles.contains_key(&k) {
                                        count += 1;
                                        k = format!("{}_{}", key, count);
                                    }
                                    k
                                };

                                self.edited_config.profiles.insert(new_key.clone(), new_profile);
                                self.selected_profile = Some(new_key);
                            }
                            ui.close_menu();
                        }

                        let can_delete = self.edited_config.profiles.len() > 1;
                        if ui.add_enabled(can_delete, egui::Button::new("🗑 Remove")).clicked() {
                            self.pending_delete = Some(key.clone());
                            ui.close_menu();
                        }
                        if !can_delete {
                            ui.label(
                                egui::RichText::new("Cannot remove the last profile")
                                    .color(egui::Color32::from_gray(120))
                                    .size(10.0)
                            );
                        }
                    });
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

            // ── Right: editor ────────────────────────────────────────────
            // No width forcing — the parent rect is already hard-constrained
            // by allocate_exact_size, so available_width can never grow.
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .show(ui, |ui| {
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
                                            profile.args = args_str.split_whitespace().map(String::from).collect();
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

                                        ui.label("Text Colour:");
                                        ui.text_edit_singleline(&mut profile.text_foreground);
                                        ui.end_row();

                                        ui.label("Text Shadow:");
                                        ui.checkbox(&mut profile.text_shadow_enabled, "Enabled");
                                        ui.end_row();

                                        ui.label("Shadow Strength:");
                                        ui.add(
                                            egui::Slider::new(&mut profile.text_shadow_alpha, 0.0..=1.0)
                                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                                        );
                                        ui.end_row();
                                    });

                                ui.add_space(12.0);
                                ui.separator();

                                // ── Terminal Padding section ──────────────
                                ui.collapsing("📐 Terminal Padding", |ui| {
                                    show_padding_section(ui, &mut profile.padding, &mut self.padding_mode);
                                });

                                // ── Shader Effect section ─────────────────
                                ui.collapsing("🎨 Shader Effect", |ui| {
                                    show_shader_effect_section(ui, &mut profile.shader_effect, key, shader_host);
                                });

                                ui.add_space(8.0);

                                // ── Particle Effects section ──────────────
                                ui.collapsing("✨ Particle Effects", |ui| {
                                    show_particle_effects_section(ui, &mut profile.particle_effects, key, shader_host);
                                });
                            }
                        } else {
                            ui.label(egui::RichText::new("Select a profile on the left to edit it.")
                                .color(egui::Color32::from_gray(120)));
                        }
                    });
            });
        });

        // ── Delete confirmation dialog ───────────────────────────────────
        if let Some(delete_key) = self.pending_delete.clone() {
            let profile_name = self.edited_config.profiles
                .get(&delete_key)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| delete_key.clone());

            let mut open = true;
            egui::Window::new("Delete Profile?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.add_space(4.0);
                    ui.label(format!(
                        "Are you sure you want to remove the profile \"{}\"?",
                        profile_name
                    ));
                    ui.label(
                        egui::RichText::new("This action cannot be undone.")
                            .color(egui::Color32::from_rgb(255, 160, 100))
                            .size(11.0),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.pending_delete = None;
                        }
                        ui.add_space(8.0);
                        if ui.button(
                            egui::RichText::new("🗑 Delete")
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        ).clicked() {
                            self.edited_config.profiles.remove(&delete_key);
                            // If the deleted profile was selected, select another
                            if self.selected_profile.as_deref() == Some(delete_key.as_str()) {
                                self.selected_profile = self.edited_config.profiles.keys().next().cloned();
                            }
                            // If the default profile was deleted, update it
                            if self.edited_config.general.default_profile == delete_key {
                                self.edited_config.general.default_profile =
                                    self.edited_config.profiles.keys().next().cloned()
                                        .unwrap_or_default();
                            }
                            self.pending_delete = None;
                        }
                    });
                });
            // If the user closed the dialog via the X button
            if !open {
                self.pending_delete = None;
            }
        }
    }

    // ── Shortcuts tab ─────────────────────────────────────────────────────────

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
                shortcut_row(ui, "Split Horizontal", &mut kb.split_horizontal);
                shortcut_row(ui, "Split Vertical", &mut kb.split_vertical);
                shortcut_row(ui, "Close Pane", &mut kb.close_pane);
                shortcut_row(ui, "Focus Next Group", &mut kb.focus_next_group);
                shortcut_row(ui, "Focus Previous Group", &mut kb.focus_prev_group);
                shortcut_row(ui, "Move Tab to Next Group", &mut kb.move_tab_to_next_group);
                shortcut_row(ui, "Move Tab to Previous Group", &mut kb.move_tab_to_prev_group);
                shortcut_row(ui, "Reload Plugins", &mut kb.reload_plugins);
            });
    }

    // ── Rendering tab ─────────────────────────────────────────────────────────

    fn show_rendering(&mut self, ui: &mut egui::Ui, _config: &Config) {
        ui.label(egui::RichText::new("Rendering Settings").strong());
        ui.add_space(8.0);

        let rendering = &mut self.edited_config.rendering;

        ui.checkbox(&mut rendering.update_in_background, "Update terminals in background");
        ui.label("Continue rendering at full speed when the window is not focused. When off, limits to 1 FPS.");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max shader FPS:");
            ui.add(egui::Slider::new(&mut rendering.max_shader_fps, 1..=240).text("fps"));
        });
        ui.add_space(8.0);

        ui.checkbox(&mut rendering.battery_aware, "Battery-aware mode");
        ui.label("Reduces shader effects and frame rate when on battery power.");
        ui.add_space(8.0);

        ui.checkbox(&mut rendering.show_fps_overlay, "Show FPS counter");
        ui.label("Display a frames-per-second counter in the top-right corner.");
    }

    // ── AI tab ────────────────────────────────────────────────────────────────

    fn show_ai(&mut self, ui: &mut egui::Ui, _config: &Config, local_model_manager: &LocalModelManager, toasts: &mut ToastManager) {
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
                    ui.selectable_value(&mut ai.default_backend, ridgeback_config::ai::AiBackendType::LmStudio, "LM Studio");
                    ui.selectable_value(&mut ai.default_backend, ridgeback_config::ai::AiBackendType::OpenAi, "OpenAI");
                    ui.selectable_value(&mut ai.default_backend, ridgeback_config::ai::AiBackendType::Claude, "Claude");
                    ui.selectable_value(&mut ai.default_backend, ridgeback_config::ai::AiBackendType::Local, "Local Model");
                });
            ui.add_space(8.0);

            match ai.default_backend {
                ridgeback_config::ai::AiBackendType::LmStudio => {
                    egui::Grid::new("lm_studio_settings").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label("Base URL:"); ui.text_edit_singleline(&mut ai.backends.lm_studio.base_url); ui.end_row();
                        ui.label("Model:"); ui.text_edit_singleline(&mut ai.backends.lm_studio.model); ui.end_row();
                    });
                }
                ridgeback_config::ai::AiBackendType::OpenAi => {
                    egui::Grid::new("openai_settings").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label("API Key:"); ui.add(egui::TextEdit::singleline(&mut ai.backends.openai.api_key).password(true)); ui.end_row();
                        ui.label("Model:"); ui.text_edit_singleline(&mut ai.backends.openai.model); ui.end_row();
                    });
                }
                ridgeback_config::ai::AiBackendType::Claude => {
                    egui::Grid::new("claude_settings").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label("API Key:"); ui.add(egui::TextEdit::singleline(&mut ai.backends.claude.api_key).password(true)); ui.end_row();
                        ui.label("Model:"); ui.text_edit_singleline(&mut ai.backends.claude.model); ui.end_row();
                    });
                }
                ridgeback_config::ai::AiBackendType::Local => {
                    show_local_ai_section(ui, &mut ai.backends.local, local_model_manager, toasts);
                }
            }

            ui.add_space(12.0);
            ui.label(egui::RichText::new("Autocomplete").strong());
            ui.checkbox(&mut ai.autocomplete.enabled, "Enable autocomplete");
            ui.horizontal(|ui| { ui.label("Debounce:"); ui.add(egui::Slider::new(&mut ai.autocomplete.debounce_ms, 100..=500).text("ms")); });
            ui.horizontal(|ui| { ui.label("Temperature:"); ui.add(egui::Slider::new(&mut ai.autocomplete.temperature, 0.0..=1.0)); });
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Command Query").strong());
            ui.checkbox(&mut ai.command_query.enabled, format!("Enable command query ({MOD}+/)"));
            ui.horizontal(|ui| { ui.label("Max suggestions:"); ui.add(egui::Slider::new(&mut ai.command_query.max_suggestions, 1..=5)); });
            ui.horizontal(|ui| { ui.label("Temperature:"); ui.add(egui::Slider::new(&mut ai.command_query.temperature, 0.0..=1.0)); });
        }
    }

    // ── Plugins tab ───────────────────────────────────────────────────────────

    fn show_plugins(&mut self, ui: &mut egui::Ui, shader_host: &ShaderPluginHost) {
        ui.label(egui::RichText::new("Shader & Particle Plugins").strong());
        ui.add_space(4.0);
        ui.label(egui::RichText::new(
            "Drop .lua plugin files into the plugins directory to add custom shader and particle effects."
        ).color(egui::Color32::from_gray(140)));
        ui.add_space(8.0);

        // Open plugins dir button
        if ui.button("📂 Open Plugins Folder").clicked() {
            let dir = ridgeback_plugin::ShaderPluginHost::find_plugins_dir();
            let _ = open_folder(&dir);
        }

        ui.add_space(8.0);
        ui.separator();

        ui.label(egui::RichText::new("Registered Shader Plugins").strong());
        egui::Grid::new("shader_plugins_grid").num_columns(2).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
            ui.label(egui::RichText::new("ID").strong());
            ui.label(egui::RichText::new("Display Name").strong());
            ui.end_row();
            for p in shader_host.shader_plugins() {
                ui.label(p.id());
                ui.label(p.display_name());
                ui.end_row();
            }
        });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Registered Particle Plugins").strong());
        egui::Grid::new("particle_plugins_grid").num_columns(3).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
            ui.label(egui::RichText::new("ID").strong());
            ui.label(egui::RichText::new("Display Name").strong());
            ui.label(egui::RichText::new("Triggers").strong());
            ui.end_row();
            for p in shader_host.particle_plugins() {
                ui.label(p.id());
                ui.label(p.display_name());
                let triggers: Vec<&str> = p.trigger_modes().iter().map(|t| match t {
                    TriggerMode::Keypress => "⌨ keypress",
                    TriggerMode::Newline => "↵ newline",
                    TriggerMode::Fullscreen => "🖥 fullscreen",
                }).collect();
                ui.label(triggers.join(", "));
                ui.end_row();
            }
        });

        ui.add_space(8.0);
        ui.label(egui::RichText::new(
            format!("Press {MOD}+Shift+P to reload plugins without restarting.")
        ).color(egui::Color32::from_gray(140)).italics());
    }
}

// ── Local AI settings section (free function to avoid borrow issues) ──────────

fn show_local_ai_section(
    ui: &mut egui::Ui,
    local: &mut ridgeback_config::ai::LocalModelConfig,
    manager: &LocalModelManager,
    toasts: &mut ToastManager,
) {
    // ── HuggingFace URL ──────────────────────────────────────────
    ui.label(egui::RichText::new("🤗 HuggingFace Model").strong());
    ui.add_space(4.0);

    egui::Grid::new("local_ai_url").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        ui.label("URL:");
        let url_response = ui.add(
            egui::TextEdit::singleline(&mut local.huggingface_url)
                .desired_width(400.0)
                .hint_text("https://huggingface.co/owner/model")
        );
        if url_response.changed() {
            manager.update_config(local);
        }
        ui.end_row();
    });

    ui.add_space(4.0);

    // ── Device selection ─────────────────────────────────────────
    let devices = detect_devices();
    ui.horizontal(|ui| {
        ui.label("Device:");
        let current_label = if local.device == "auto" {
            let best = devices.iter().find(|d| d.recommended);
            format!("Auto ({})", best.map(|d| d.label.as_str()).unwrap_or("CPU"))
        } else {
            devices.iter().find(|d| d.id == local.device)
                .map(|d| d.label.clone())
                .unwrap_or_else(|| local.device.clone())
        };

        egui::ComboBox::from_id_salt("local_device_combo")
            .selected_text(&current_label)
            .show_ui(ui, |ui| {
                let auto_label = {
                    let best = devices.iter().find(|d| d.recommended);
                    format!("Auto ({})", best.map(|d| d.label.as_str()).unwrap_or("CPU"))
                };
                ui.selectable_value(&mut local.device, "auto".to_string(), auto_label);
                for dev in &devices {
                    let label = if dev.recommended {
                        format!("{} ✦ recommended", dev.label)
                    } else {
                        dev.label.clone()
                    };
                    ui.selectable_value(&mut local.device, dev.id.clone(), label);
                }
            });
    });

    ui.add_space(8.0);
    ui.separator();

    // ── Model status ─────────────────────────────────────────────
    let status = manager.status();

    match &status {
        LocalModelStatus::NotDownloaded => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("○ Not downloaded").color(egui::Color32::from_gray(140)));
            });

            // Estimate size warning
            ui.label(
                egui::RichText::new("⚠ The Qwen2.5-Coder-1.5B model is ~3 GB. Ensure sufficient disk space and bandwidth.")
                    .color(egui::Color32::from_rgb(200, 180, 100))
                    .size(11.0)
            );
            ui.add_space(4.0);

            if ui.button("⬇  Download Model").clicked() {
                // Push a sticky progress toast
                let progress_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let toast = Toast::progress(
                    "Downloading model…",
                    std::sync::Arc::clone(&manager.downloaded_bytes),
                    std::sync::Arc::clone(&manager.total_bytes),
                    std::sync::Arc::clone(&progress_done),
                );
                toasts.push(toast);

                // Start the download
                let ctx = ui.ctx().clone();
                let done_flag = progress_done;
                manager.start_download(move || {
                    ctx.request_repaint();
                });

                // Spawn a watcher to set the done flag when download finishes
                let mgr_clone = manager.clone();
                let ctx2 = ui.ctx().clone();
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        if !mgr_clone.is_downloading() {
                            done_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                            ctx2.request_repaint();
                            break;
                        }
                    }
                });
            }
        }

        LocalModelStatus::Downloading { downloaded_bytes, total_bytes } => {
            let fraction = if *total_bytes > 0 {
                *downloaded_bytes as f32 / *total_bytes as f32
            } else {
                0.0
            };
            let pct = (fraction * 100.0) as u32;

            ui.label(egui::RichText::new("⬇ Downloading…").color(egui::Color32::from_rgb(100, 200, 160)));
            ui.add_space(4.0);

            ui.add(
                egui::ProgressBar::new(fraction)
                    .text(format!("{}% — {}/{}", pct, format_size(*downloaded_bytes), format_size(*total_bytes)))
                    .animate(true)
            );

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Download in progress. You can close this panel — progress is shown in the toast bar.")
                    .size(11.0).color(egui::Color32::from_gray(130))
            );

            ui.ctx().request_repaint();
        }

        LocalModelStatus::Downloaded { date, size_bytes } => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("● Downloaded").color(egui::Color32::from_rgb(100, 200, 130)));
            });

            egui::Grid::new("local_model_info").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label(egui::RichText::new("Size:").color(egui::Color32::from_gray(160)));
                ui.label(format_size(*size_bytes));
                ui.end_row();

                ui.label(egui::RichText::new("Downloaded:").color(egui::Color32::from_gray(160)));
                ui.label(date);
                ui.end_row();

                if let Some(path) = local.model_path() {
                    ui.label(egui::RichText::new("Location:").color(egui::Color32::from_gray(160)));
                    ui.label(egui::RichText::new(path.display().to_string()).size(10.0).color(egui::Color32::from_gray(120)));
                    ui.end_row();
                }
            });

            ui.add_space(4.0);
            if ui.button("🗑  Delete Model").clicked() {
                manager.delete_model();
            }
        }

        LocalModelStatus::Starting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Starting…").color(egui::Color32::from_rgb(180, 200, 255)));
                ui.label(egui::RichText::new("— loading model into memory").color(egui::Color32::from_gray(140)).size(11.0));
            });
            ui.ctx().request_repaint();
        }

        LocalModelStatus::Running => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("● Running").color(egui::Color32::from_rgb(80, 220, 120)));
                ui.label(egui::RichText::new("— model loaded and ready for inference").color(egui::Color32::from_gray(140)).size(11.0));
            });
        }

        LocalModelStatus::Error(msg) => {
            ui.label(egui::RichText::new(format!("✖ Error: {}", msg)).color(egui::Color32::from_rgb(255, 120, 120)));
            ui.add_space(4.0);
            if ui.button("↻  Retry").clicked() {
                manager.detect();
            }
        }
    }

    ui.add_space(8.0);

    // ── Inference controls (only when downloaded, starting, or running) ─────
    let show_inference_controls = matches!(
        &status,
        LocalModelStatus::Downloaded { .. } | LocalModelStatus::Starting | LocalModelStatus::Running
    );

    if show_inference_controls {
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Inference Server").strong());
        ui.add_space(4.0);

        match &status {
            LocalModelStatus::Starting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Starting…").color(egui::Color32::from_rgb(180, 200, 255)));
                });
                ui.label(
                    egui::RichText::new("Loading model weights — this may take a moment.")
                        .size(11.0).color(egui::Color32::from_gray(130))
                );
                ui.ctx().request_repaint();
            }
            LocalModelStatus::Running => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("● Running").color(egui::Color32::from_rgb(80, 220, 120)));
                    if ui.button("■  Stop").clicked() {
                        manager.stop_inference();
                    }
                });
            }
            LocalModelStatus::Downloaded { .. } => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("○ Stopped").color(egui::Color32::from_gray(140)));
                    if ui.button("▶  Start").clicked() {
                        manager.start_inference();
                        ui.ctx().request_repaint();
                    }
                });
                ui.label(
                    egui::RichText::new("Start the inference server to use local AI for autocomplete and command queries.")
                        .size(11.0).color(egui::Color32::from_gray(130))
                );
            }
            _ => {}
        }
    }

    // Context length control
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Context length:");
        ui.add(egui::Slider::new(&mut local.context_length, 256..=8192).text("tokens"));
    });
}

// ── Terminal padding section ──────────────────────────────────────────────────

fn show_padding_section(
    ui: &mut egui::Ui,
    padding: &mut ridgeback_config::TerminalPadding,
    mode: &mut PaddingMode,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Mode:");
        ui.selectable_value(mode, PaddingMode::Uniform, "Uniform");
        ui.selectable_value(mode, PaddingMode::WidthHeight, "W × H");
        ui.selectable_value(mode, PaddingMode::Individual, "Individual");
    });
    ui.add_space(4.0);

    // Pixel preview: percentages reference the smaller viewport dimension
    let screen = ui.ctx().screen_rect();
    let min_dim = screen.width().min(screen.height());

    let fmt = |v: f64, _| {
        let px = min_dim as f64 * v / 100.0;
        format!("{:.1}% ({:.0}px)", v, px)
    };

    match *mode {
        PaddingMode::Uniform => {
            let mut val = padding.top;
            ui.horizontal(|ui| {
                ui.label("All sides:");
                if ui.add(
                    egui::Slider::new(&mut val, 0.0..=25.0)
                        .custom_formatter(fmt)
                        .suffix("%")
                ).changed() {
                    padding.top = val;
                    padding.bottom = val;
                    padding.left = val;
                    padding.right = val;
                }
            });
        }
        PaddingMode::WidthHeight => {
            let mut horiz = padding.left;
            let mut vert = padding.top;
            ui.horizontal(|ui| {
                ui.label("Horizontal:");
                if ui.add(
                    egui::Slider::new(&mut horiz, 0.0..=25.0)
                        .custom_formatter(fmt)
                        .suffix("%")
                ).changed() {
                    padding.left = horiz;
                    padding.right = horiz;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Vertical:");
                if ui.add(
                    egui::Slider::new(&mut vert, 0.0..=25.0)
                        .custom_formatter(fmt)
                        .suffix("%")
                ).changed() {
                    padding.top = vert;
                    padding.bottom = vert;
                }
            });
        }
        PaddingMode::Individual => {
            egui::Grid::new("padding_individual")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Top:");
                    ui.add(egui::Slider::new(&mut padding.top, 0.0..=25.0)
                        .custom_formatter(fmt).suffix("%"));
                    ui.end_row();

                    ui.label("Bottom:");
                    ui.add(egui::Slider::new(&mut padding.bottom, 0.0..=25.0)
                        .custom_formatter(fmt).suffix("%"));
                    ui.end_row();

                    ui.label("Left:");
                    ui.add(egui::Slider::new(&mut padding.left, 0.0..=25.0)
                        .custom_formatter(fmt).suffix("%"));
                    ui.end_row();

                    ui.label("Right:");
                    ui.add(egui::Slider::new(&mut padding.right, 0.0..=25.0)
                        .custom_formatter(fmt).suffix("%"));
                    ui.end_row();
                });
        }
    }

    // Show pixel summary
    let px_top = min_dim * (padding.top / 100.0);
    let px_bot = min_dim * (padding.bottom / 100.0);
    let px_left = min_dim * (padding.left / 100.0);
    let px_right = min_dim * (padding.right / 100.0);
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(format!(
            "Pixels: ↑{:.0}  ↓{:.0}  ←{:.0}  →{:.0}",
            px_top, px_bot, px_left, px_right
        ))
        .color(egui::Color32::from_gray(130))
        .small(),
    );
}

// ── Shader effect section ─────────────────────────────────────────────────────

fn show_shader_effect_section(
    ui: &mut egui::Ui,
    effect: &mut ridgeback_config::ShaderEffectConfig,
    profile_key: &str,
    shader_host: &ShaderPluginHost,
) {
    ui.horizontal(|ui| {
        ui.label("Effect:");
        egui::ComboBox::from_id_salt(format!("shader_effect_combo_{}", profile_key))
            .selected_text(shader_display_name(&effect.plugin_id, shader_host))
            .show_ui(ui, |ui| {
                // "None" option
                if ui.selectable_value(&mut effect.plugin_id, "none".to_string(), "None").clicked() {
                    effect.params.clear();
                }
                for plugin in shader_host.shader_plugins() {
                    let id = plugin.id().to_string();
                    if ui.selectable_value(&mut effect.plugin_id, id.clone(), plugin.display_name()).clicked() {
                        // Fill default params for the newly selected plugin
                        plugin.fill_defaults(&mut effect.params);
                    }
                }
            });
    });

    if effect.plugin_id == "none" {
        return;
    }

    if let Some(plugin) = shader_host.get_shader_plugin(&effect.plugin_id) {
        plugin.fill_defaults(&mut effect.params);
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Parameters").strong().size(12.0));
        egui::Grid::new(format!("shader_params_{}", profile_key))
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for desc in plugin.param_schema() {
                    ui.label(&desc.label);
                    show_param_editor(ui, &desc.key, &desc.ty, &mut effect.params,
                        format!("sp_{}_{}", profile_key, &desc.key));
                    ui.end_row();
                }
            });

        // Custom WGSL override
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Custom WGSL (optional)").size(11.0).color(egui::Color32::from_gray(150)));
        ui.text_edit_singleline(&mut effect.wgsl_override);
        ui.label(egui::RichText::new(
            "Leave empty to use the plugin's built-in shader file."
        ).size(10.0).color(egui::Color32::from_gray(110)));
    } else {
        ui.label(
            egui::RichText::new(format!("⚠ Plugin '{}' not found — reload plugins or check ID.", effect.plugin_id))
                .color(egui::Color32::from_rgb(255, 180, 60))
        );
    }
}

// ── Particle effects list section ─────────────────────────────────────────────

fn show_particle_effects_section(
    ui: &mut egui::Ui,
    effects: &mut Vec<ridgeback_config::ParticleEffectEntry>,
    profile_key: &str,
    shader_host: &ShaderPluginHost,
) {
    ui.label(
        egui::RichText::new("Add typing, screen, or ambient particle effects. Each effect runs its own Lua plugin.")
            .color(egui::Color32::from_gray(140))
            .small(),
    );
    ui.add_space(4.0);

    // ── Render each effect entry ──────────────────────────────────────────
    let mut remove_idx: Option<usize> = None;

    for (idx, entry) in effects.iter_mut().enumerate() {
        let entry_id = format!("pe_{}_{}", profile_key, idx);
        let display = particle_display_name(&entry.plugin_id, shader_host).to_string();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Enabled toggle
                    ui.checkbox(&mut entry.enabled, "");

                    // Plugin selector combo
                    egui::ComboBox::from_id_salt(format!("pe_combo_{}", entry_id))
                        .selected_text(display)
                        .show_ui(ui, |ui| {
                            for plugin in shader_host.particle_plugins() {
                                let id = plugin.id().to_string();
                                if ui.selectable_value(&mut entry.plugin_id, id.clone(), plugin.display_name()).clicked() {
                                    entry.params.clear();
                                    plugin.fill_defaults(&mut entry.params);
                                }
                            }
                        });

                    // Trigger mode badges
                    if let Some(plugin) = shader_host.get_particle_plugin(&entry.plugin_id) {
                        for mode in plugin.trigger_modes() {
                            let badge = match mode {
                                TriggerMode::Keypress => "⌨",
                                TriggerMode::Newline => "↵",
                                TriggerMode::Fullscreen => "🖥",
                            };
                            ui.small(badge);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("🗑").on_hover_text("Remove this effect").clicked() {
                            remove_idx = Some(idx);
                        }
                    });
                });

                // Per-effect params (collapsible)
                if entry.enabled {
                    if let Some(plugin) = shader_host.get_particle_plugin(&entry.plugin_id) {
                        plugin.fill_defaults(&mut entry.params);
                        let schema = plugin.param_schema();
                        if !schema.is_empty() {
                            ui.add_space(4.0);
                            egui::Grid::new(format!("pe_params_{}", entry_id))
                                .num_columns(2)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    for desc in schema {
                                        ui.label(&desc.label);
                                        show_param_editor(
                                            ui, &desc.key, &desc.ty, &mut entry.params,
                                            format!("pe_p_{}_{}", entry_id, &desc.key),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                    }
                }
            });
        ui.add_space(2.0);
    }

    // Process removal
    if let Some(idx) = remove_idx {
        effects.remove(idx);
    }

    // ── Add new effect button ─────────────────────────────────────────────
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("➕ Add Effect").clicked() {
            // Default to first available plugin
            let first_id = shader_host.particle_plugins().first()
                .map(|p| p.id().to_string())
                .unwrap_or_else(|| "none".to_string());
            let mut new_entry = ridgeback_config::ParticleEffectEntry::new(&first_id);
            if let Some(plugin) = shader_host.get_particle_plugin(&first_id) {
                plugin.fill_defaults(&mut new_entry.params);
            }
            effects.push(new_entry);
        }
        if effects.is_empty() {
            ui.label(
                egui::RichText::new("No particle effects active.")
                    .color(egui::Color32::from_gray(120))
                    .italics(),
            );
        }
    });
}

// ── Shared param editor widget ────────────────────────────────────────────────

fn show_param_editor(
    ui: &mut egui::Ui,
    key: &str,
    ty: &ParamType,
    params: &mut std::collections::HashMap<String, serde_json::Value>,
    _id_salt: String,
) {
    let val = params.entry(key.to_string()).or_insert(serde_json::json!(0.0));

    match ty {
        ParamType::Float { min, max } => {
            let mut f = val.as_f64().unwrap_or(0.0) as f32;
            if ui.add(egui::Slider::new(&mut f, *min..=*max)).changed() {
                *val = serde_json::json!(f as f64);
            }
        }
        ParamType::Int { min, max } => {
            let mut i = val.as_i64().unwrap_or(0);
            if ui.add(egui::Slider::new(&mut i, *min..=*max)).changed() {
                *val = serde_json::json!(i);
            }
        }
        ParamType::Bool => {
            let mut b = val.as_bool().unwrap_or(false);
            if ui.checkbox(&mut b, "").changed() {
                *val = serde_json::json!(b);
            }
        }
        ParamType::Color => {
            let hex = val.as_str().unwrap_or("#ffffff").to_string();
            let [r, g, b] = hex_to_rgb(&hex);
            let mut color = egui::Color32::from_rgb(r, g, b);
            if ui.color_edit_button_srgba(&mut color).changed() {
                *val = serde_json::json!(rgb_to_hex(color.r(), color.g(), color.b()));
            }
        }
        ParamType::Text => {
            let mut s = val.as_str().unwrap_or("").to_string();
            if ui.text_edit_singleline(&mut s).changed() {
                *val = serde_json::json!(s);
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn shader_display_name<'a>(id: &'a str, host: &'a ShaderPluginHost) -> &'a str {
    if id == "none" { return "None"; }
    host.get_shader_plugin(id).map(|p| p.display_name()).unwrap_or(id)
}

fn particle_display_name<'a>(id: &'a str, host: &'a ShaderPluginHost) -> &'a str {
    if id == "none" { return "None"; }
    host.get_particle_plugin(id).map(|p| p.display_name()).unwrap_or(id)
}

fn hex_to_rgb(hex: &str) -> [u8; 3] {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        [r, g, b]
    } else {
        [255, 255, 255]
    }
}

fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn shortcut_row(ui: &mut egui::Ui, name: &str, binding: &mut String) {
    ui.label(name);
    ui.text_edit_singleline(binding);
    ui.end_row();
}

fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}

/// Format byte count in human-readable form.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

