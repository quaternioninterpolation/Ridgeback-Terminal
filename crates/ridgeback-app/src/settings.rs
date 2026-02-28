use egui;
use ridgeback_config::Config;
use ridgeback_plugin::{ShaderPluginHost, shader_plugin::ParamType};
use crate::casting::CastManager;

/// Settings window state.
pub struct SettingsWindow {
    active_tab: SettingsTab,
    edited_config: Config,
    /// The profile key currently selected in the left list.
    pub selected_profile: Option<String>,
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

        match self.active_tab {
            SettingsTab::Profiles => self.show_profiles(ui, config, shader_host),
            SettingsTab::Shortcuts => self.show_shortcuts(ui, config),
            SettingsTab::Rendering => self.show_rendering(ui, config),
            SettingsTab::Ai => self.show_ai(ui, config),
            SettingsTab::Plugins => self.show_plugins(ui, shader_host),
            SettingsTab::CastShare => crate::casting::show_cast_panel(ui, cast_manager),
        }

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

        let available = ui.available_size();

        ui.horizontal_top(|ui| {
            // ── Left: profile list ────────────────────────────────────────
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

            // ── Right: editor (vertical column) ─────────────────────────
            let right_width = (available.x - 180.0).max(300.0);
            ui.vertical(|ui| {
                ui.set_min_width(right_width);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
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

                                // ── Shader Effect section ─────────────────
                                ui.collapsing("🎨 Shader Effect", |ui| {
                                    show_shader_effect_section(ui, &mut profile.shader_effect, key, shader_host);
                                });

                                ui.add_space(8.0);

                                // ── Typing Particles section ──────────────
                                ui.collapsing("✨ Typing Particles", |ui| {
                                    show_typing_particles_section(ui, &mut profile.typing_particles, key, shader_host);
                                });
                            }
                        } else {
                            ui.label(egui::RichText::new("Select a profile on the left to edit it.")
                                .color(egui::Color32::from_gray(120)));
                        }
                    });
            });
        });
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
        ui.label("Continue rendering when the window is not focused.");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max shader FPS:");
            ui.add(egui::Slider::new(&mut rendering.max_shader_fps, 1..=240).text("fps"));
        });
        ui.add_space(8.0);

        ui.checkbox(&mut rendering.battery_aware, "Battery-aware mode");
        ui.label("Reduces shader effects and frame rate when on battery power.");
    }

    // ── AI tab ────────────────────────────────────────────────────────────────

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
                    egui::Grid::new("local_settings").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label("HuggingFace Repo:"); ui.text_edit_singleline(&mut ai.backends.local.model_repo); ui.end_row();
                        ui.label("Quantization:"); ui.text_edit_singleline(&mut ai.backends.local.quantization); ui.end_row();
                        ui.label("Device:");
                        egui::ComboBox::from_id_salt("local_device").selected_text(&ai.backends.local.device).show_ui(ui, |ui| {
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
            ui.horizontal(|ui| { ui.label("Debounce:"); ui.add(egui::Slider::new(&mut ai.autocomplete.debounce_ms, 100..=500).text("ms")); });
            ui.horizontal(|ui| { ui.label("Temperature:"); ui.add(egui::Slider::new(&mut ai.autocomplete.temperature, 0.0..=1.0)); });
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Command Query").strong());
            ui.checkbox(&mut ai.command_query.enabled, "Enable command query (Ctrl+/)");
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
        egui::Grid::new("particle_plugins_grid").num_columns(2).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
            ui.label(egui::RichText::new("ID").strong());
            ui.label(egui::RichText::new("Display Name").strong());
            ui.end_row();
            for p in shader_host.particle_plugins() {
                ui.label(p.id());
                ui.label(p.display_name());
                ui.end_row();
            }
        });

        ui.add_space(8.0);
        ui.label(egui::RichText::new(
            "Press Ctrl+Shift+P to reload plugins without restarting."
        ).color(egui::Color32::from_gray(140)).italics());
    }
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

// ── Typing particles section ──────────────────────────────────────────────────

fn show_typing_particles_section(
    ui: &mut egui::Ui,
    particles: &mut ridgeback_config::TypingParticlesConfig,
    profile_key: &str,
    shader_host: &ShaderPluginHost,
) {
    ui.horizontal(|ui| {
        ui.label("Particle Effect:");
        egui::ComboBox::from_id_salt(format!("particle_combo_{}", profile_key))
            .selected_text(particle_display_name(&particles.plugin_id, shader_host))
            .show_ui(ui, |ui| {
                if ui.selectable_value(&mut particles.plugin_id, "none".to_string(), "None").clicked() {
                    particles.params.clear();
                }
                for plugin in shader_host.particle_plugins() {
                    let id = plugin.id().to_string();
                    if ui.selectable_value(&mut particles.plugin_id, id.clone(), plugin.display_name()).clicked() {
                        plugin.fill_defaults(&mut particles.params);
                    }
                }
            });
    });

    if particles.plugin_id == "none" { return; }

    if let Some(plugin) = shader_host.get_particle_plugin(&particles.plugin_id) {
        plugin.fill_defaults(&mut particles.params);
        ui.add_space(6.0);
        egui::Grid::new(format!("particle_params_{}", profile_key))
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for desc in plugin.param_schema() {
                    ui.label(&desc.label);
                    show_param_editor(ui, &desc.key, &desc.ty, &mut particles.params,
                        format!("pp_{}_{}", profile_key, &desc.key));
                    ui.end_row();
                }
            });
    }
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
