use egui;
use crate::tabs::TabManager;
use crate::shortcuts::ShortcutManager;
use ridgeback_config::keybindings::ShortcutAction;
use crate::settings::SettingsWindow;
use crate::casting::CastManager;
use crate::toast::{Toast, ToastManager};
use ridgeback_config::Config;
use ridgeback_ai::AiService;

/// Main application state.
pub struct RidgebackApp {
    pub config: Config,
    pub tabs: TabManager,
    pub shortcuts: ShortcutManager,
    pub settings_open: bool,
    pub settings: SettingsWindow,
    pub ai_service: AiService,
    pub clipboard: Option<arboard::Clipboard>,
    pub cast_manager: CastManager,
    /// Background image texture (assets/images/background.png), loaded once.
    pub bg_texture: Option<egui::TextureHandle>,
    /// Toast notification queue.
    pub toasts: ToastManager,
}

impl RidgebackApp {
    pub fn new(config: Config) -> Self {
        let shortcuts = ShortcutManager::from_config(&config.keybindings);
        let ai_service = AiService::new(&config.ai);
        let mut tabs = TabManager::new();

        // Open a default tab
        if let Some((name, profile)) = config.default_profile() {
            if let Err(e) = tabs.open_tab(name, profile) {
                tracing::error!("Failed to open default tab: {}", e);
            }
        }

        Self {
            settings: SettingsWindow::new(config.clone()),
            config,
            tabs,
            shortcuts,
            settings_open: false,
            ai_service,
            clipboard: arboard::Clipboard::new().ok(),
            cast_manager: CastManager::new(),
            bg_texture: None,
            toasts: ToastManager::new(),
        }
    }

    fn handle_shortcut(&mut self, action: ShortcutAction) {
        match action {
            ShortcutAction::NewTab => {
                if let Some((name, profile)) = self.config.default_profile() {
                    if let Err(e) = self.tabs.open_tab(name, profile) {
                        tracing::error!("Failed to open tab: {}", e);
                    }
                }
            }
            ShortcutAction::CloseTab => {
                self.tabs.close_active_tab();
            }
            ShortcutAction::NextTab => {
                self.tabs.next_tab();
            }
            ShortcutAction::PrevTab => {
                self.tabs.prev_tab();
            }
            ShortcutAction::OpenSettings => {
                self.settings_open = !self.settings_open;
            }
            ShortcutAction::SaveSession => {
                self.save_session();
            }
            ShortcutAction::FindInSession => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.find_overlay.toggle();
                }
            }
            ShortcutAction::AiCommandQuery => {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    tab.command_query.toggle();
                }
            }
        }
    }

    fn save_session(&mut self) {
        if let Some(tab) = self.tabs.active_tab() {
            let log = tab.terminal.full_log();
            let timestamp = chrono_now();
            let default_name = format!("ridgeback_session_{}.txt", timestamp);

            std::thread::spawn(move || {
                let file = rfd::FileDialog::new()
                    .set_title("Save Session")
                    .set_file_name(&default_name)
                    .add_filter("Text Files", &["txt"])
                    .add_filter("All Files", &["*"])
                    .save_file();

                if let Some(path) = file {
                    if let Err(e) = std::fs::write(&path, log) {
                        tracing::error!("Failed to save session: {}", e);
                    } else {
                        tracing::info!("Session saved to {}", path.display());
                    }
                }
            });
        }
    }

    fn copy_to_clipboard(&mut self, text: &str) {
        if let Some(ref mut clipboard) = self.clipboard {
            let _ = clipboard.set_text(text);
        }
    }

    fn get_clipboard_text(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }
}

impl eframe::App for RidgebackApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Lazy-load background image ────────────────────────────────────
        if self.bg_texture.is_none() {
            self.bg_texture = load_background_texture(ctx);
        }

        // ── Window focus ──────────────────────────────────────────────────
        let window_focused = ctx.input(|i| i.focused);
        let update_in_bg   = self.config.rendering.update_in_background;

        // ── Tab open/close animations ─────────────────────────────────────
        let dt = ctx.input(|i| i.unstable_dt).min(0.1);
        let still_animating = self.tabs.tick_animations(dt);

        // Process PTY output for all tabs
        let mut any_changed = false;
        for tab in self.tabs.tabs_mut() {
            if tab.terminal.process_pty_output() {
                any_changed = true;
            }
        }

        // Check shortcuts
        let action = self.shortcuts.check(ctx);
        if let Some(action) = action {
            self.handle_shortcut(action);
        }

        // Top panel: Tab bar
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 2.0;
                ui.style_mut().spacing.button_padding = egui::vec2(8.0, 4.0);

                let active = self.tabs.active_index();
                let tab_count = self.tabs.count();

                let mut clicked_tab: Option<usize> = None;
                let mut close_tab: Option<usize> = None;

                for i in 0..tab_count {
                    let Some(tab_data) = self.tabs.tab(i) else { continue };
                    let (title, open_t, close_t, is_closing) = (
                        tab_data.tab_title.clone(),
                        tab_data.open_anim,
                        tab_data.close_anim,
                        tab_data.closing,
                    );
                    let is_active = i == active;

                    // Ease-out curve for smooth feel
                    let open_ease  = 1.0 - (1.0 - open_t).powi(3);
                    let close_ease = close_t.powi(2);
                    // Combined animation factor: 0 = invisible, 1 = fully open
                    let anim = if is_closing { 1.0 - close_ease } else { open_ease };

                    let (bg, fg) = if is_active {
                        (egui::Color32::from_gray(55), egui::Color32::WHITE)
                    } else {
                        (egui::Color32::from_gray(28), egui::Color32::from_gray(170))
                    };

                    let full_w = (title.len() as f32 * 7.5 + 52.0).min(220.0).max(100.0);
                    let animated_w = full_w * anim;
                    let alpha = (anim * 255.0) as u8;

                    let desired_size = egui::vec2(animated_w, 28.0);
                    let (tab_rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

                    if ui.is_rect_visible(tab_rect) && animated_w > 4.0 {
                        // Tint bg with animation alpha
                        let bg_a = egui::Color32::from_rgba_unmultiplied(
                            bg.r(), bg.g(), bg.b(), ((bg.a() as f32) * anim) as u8
                        );
                        ui.painter().rect_filled(tab_rect, 4.0, bg_a);

                        // Active indicator bar at bottom
                        if is_active && !is_closing {
                            let bar = egui::Rect::from_min_size(
                                egui::pos2(tab_rect.left() + 4.0, tab_rect.bottom() - 2.0),
                                egui::vec2((tab_rect.width() - 8.0) * open_ease, 2.0),
                            );
                            let bar_color = egui::Color32::from_rgba_unmultiplied(137, 180, 250, alpha);
                            ui.painter().rect_filled(bar, 0.0, bar_color);
                        }

                        // Title label
                        let title_rect = egui::Rect::from_min_max(
                            tab_rect.min,
                            egui::pos2(tab_rect.right() - 24.0, tab_rect.bottom()),
                        );
                        let title_resp = ui.interact(title_rect, egui::Id::new(("tab_title", i)), egui::Sense::click());
                        let fg_a = egui::Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), alpha);
                        ui.painter().text(
                            egui::pos2(title_rect.left() + 8.0, title_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &title,
                            egui::FontId::proportional(12.0),
                            fg_a,
                        );
                        if title_resp.clicked() && !is_closing {
                            clicked_tab = Some(i);
                        }
                        if title_resp.middle_clicked() {
                            close_tab = Some(i);
                        }

                        // Close button "x"
                        let close_size = 16.0;
                        let close_rect = egui::Rect::from_center_size(
                            egui::pos2(tab_rect.right() - 14.0, tab_rect.center().y),
                            egui::vec2(close_size, close_size),
                        );
                        let close_resp = ui.interact(close_rect, egui::Id::new(("tab_close", i)), egui::Sense::click());
                        let close_color = if close_resp.hovered() {
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(140, 140, 140, alpha)
                        };
                        if close_resp.hovered() {
                            ui.painter().rect_filled(close_rect, 3.0, egui::Color32::from_gray(80));
                        }
                        ui.painter().text(
                            close_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            egui_phosphor::regular::X,
                            egui::FontId::proportional(12.0),
                            close_color,
                        );
                        if close_resp.clicked() {
                            close_tab = Some(i);
                        }
                    }
                }

                // Handle events — close takes priority
                if let Some(idx) = close_tab {
                    self.tabs.close_tab(idx);
                } else if let Some(idx) = clicked_tab {
                    self.tabs.set_active(idx);
                }

                // New tab "+" button using Phosphor icon
                ui.add_space(4.0);
                let new_tab_resp = ui.add(
                    egui::Button::new(
                        egui::RichText::new(egui_phosphor::regular::PLUS)
                            .size(16.0)
                            .color(egui::Color32::from_gray(200)),
                    )
                    .fill(egui::Color32::from_gray(28))
                    .stroke(egui::Stroke::NONE)
                    .min_size(egui::vec2(28.0, 28.0)),
                );
                if new_tab_resp.clicked() {
                    ui.memory_mut(|mem| mem.toggle_popup(new_tab_resp.id));
                }

                egui::popup_below_widget(
                    ui,
                    new_tab_resp.id,
                    &new_tab_resp,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui: &mut egui::Ui| {
                        ui.set_min_width(160.0);
                        let profiles: Vec<(String, ridgeback_config::Profile)> = self
                            .config
                            .profiles
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        for (name, profile) in profiles {
                            if ui.button(&profile.name).clicked() {
                                if let Err(e) = self.tabs.open_tab(&name, &profile) {
                                    tracing::error!("Failed to open tab: {}", e);
                                }
                                ui.memory_mut(|mem: &mut egui::Memory| mem.close_popup());
                            }
                        }
                    },
                );

                // Settings button — right-aligned, Phosphor gear icon
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let settings_resp = ui.add(
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::GEAR)
                                .size(16.0)
                                .color(egui::Color32::from_gray(180)),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                        .min_size(egui::vec2(28.0, 28.0)),
                    );
                    if settings_resp.clicked() {
                        self.settings_open = !self.settings_open;
                    }
                    settings_resp.on_hover_text("Settings (Ctrl+,)");
                });
            }); // end horizontal
        }); // end TopBottomPanel

        // Settings window
        if self.settings_open {
            let mut still_open = true;
            let mut saved_keys: Vec<String> = Vec::new();
            egui::Window::new("Settings")
                .open(&mut still_open)
                .resizable(true)
                .default_size([600.0, 500.0])
                .show(ctx, |ui| {
                    saved_keys = self.settings.show(ui, &mut self.config, &mut self.cast_manager);
                });
            if !still_open {
                self.settings_open = false;
            }

            // Apply live-applicable profile changes to open tabs
            if !saved_keys.is_empty() {
                for key in &saved_keys {
                    if let Some(profile) = self.config.profiles.get(key) {
                        let profile = profile.clone();
                        let mut any_tab_updated = false;
                        // Shell/args can't be hot-reloaded — track for toast
                        let mut shell_changed = false;

                        for tab in self.tabs.tabs_mut() {
                            if &tab.terminal.profile_name != key { continue; }
                            any_tab_updated = true;

                            // ── Live-applicable ──────────────────────────────
                            tab.shader_effect = profile.shader_effect;
                            tab.shader_params = profile.shader_params.clone();
                            tab.terminal.vt.cursor_style = profile.cursor_style;
                            tab.terminal.vt.scrollback.set_capacity(profile.scrollback_limit);
                            tab.text_shadow_enabled = profile.text_shadow_enabled;
                            tab.text_shadow_alpha = profile.text_shadow_alpha;
                            tab.text_foreground = profile.text_foreground.clone();
                            tab.terminal.shell_type = profile.shell_type;

                            // Flag if shell executable changed (needs new tab)
                            shell_changed = true; // we can't inspect PtySession's exe; always warn
                        }

                        let profile_name = profile.name.clone();
                        if !any_tab_updated {
                            // No open tabs for this profile — just confirm save
                            self.toasts.push(Toast::info(
                                format!("\"{}\" profile saved.", profile_name)
                            ));
                        } else {
                            self.toasts.push(Toast::info(
                                format!("\"{}\" — shader, cursor & scrollback applied to open tabs.", profile_name)
                            ));
                            if shell_changed {
                                self.toasts.push(Toast::warning(
                                    format!(
                                        "\"{}\" — shell changes take effect in new tabs.",
                                        profile_name
                                    )
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Central panel: Active terminal
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_gray(15)))
            .show(ctx, |ui| {
                if let Some(tab) = self.tabs.active_tab_mut() {
                    // Gather context for AI query (before borrowing tab mutably for overlays)
                    let shell_type = tab.terminal.shell_type;
                    let cwd = std::env::current_dir()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let history = tab.terminal.last_n_lines(5);

                    // Command query overlay (Ctrl+/)
                    if tab.command_query.is_open {
                        let accepted = tab.command_query.show(
                            ui,
                            &mut tab.terminal.input,
                            &self.ai_service,
                            shell_type,
                            cwd,
                            history,
                        );
                        // If a suggestion was accepted, write it straight to the PTY
                        // so it appears inline in the shell just like typed text.
                        if let Some(cmd) = accepted {
                            let _ = tab.terminal.write_to_pty(cmd.as_bytes());
                        }
                    }

                    // Find overlay (Ctrl+F)
                    if tab.find_overlay.is_open {
                        tab.find_overlay.show(ui, &tab.terminal);
                    }

                    // Terminal viewport
                    crate::terminal_view::show_terminal(
                        ui, tab, &mut self.clipboard,
                        self.bg_texture.as_ref(),
                        window_focused || update_in_bg,
                    );
                } else {
                    // ── Empty state: show background image then overlay text ──
                    let empty_rect = ui.available_rect_before_wrap();
                    let bg_color = egui::Color32::from_rgb(0x1e, 0x1e, 0x2e);
                    ui.painter().rect_filled(empty_rect, 0.0, bg_color);

                    if let Some(tex) = self.bg_texture.as_ref() {
                        let img_w = tex.size()[0] as f32;
                        let img_h = tex.size()[1] as f32;
                        let scale = (empty_rect.width() / img_w).max(empty_rect.height() / img_h);
                        let sw = img_w * scale;
                        let sh = img_h * scale;
                        let u0 = (sw - empty_rect.width())  / (2.0 * sw);
                        let v0 = (sh - empty_rect.height()) / (2.0 * sh);
                        ui.painter().image(
                            tex.id(),
                            empty_rect,
                            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(1.0 - u0, 1.0 - v0)),
                            egui::Color32::from_white_alpha(128),
                        );
                    }

                    // Text drawn on top — centred in the panel
                    let center = empty_rect.center();
                    let painter = ui.painter();
                    let shadow = egui::Color32::from_black_alpha(180);
                    let msg = format!("{} Open a new terminal tab",
                        egui_phosphor::regular::TERMINAL_WINDOW);
                    let hint = format!("Press  {}  Ctrl+T  or click  {}  in the tab bar",
                        egui_phosphor::regular::KEYBOARD,
                        egui_phosphor::regular::PLUS);

                    // Shadow then text for each line
                    for (dy, text, size, fg) in [
                        (0.0f32, msg.as_str(), 20.0f32, egui::Color32::from_gray(220)),
                        (34.0,   hint.as_str(), 13.0,   egui::Color32::from_gray(140)),
                    ] {
                        let pos = egui::pos2(center.x, center.y + dy);
                        painter.text(egui::pos2(pos.x + 1.0, pos.y + 1.0),
                            egui::Align2::CENTER_CENTER, text,
                            egui::FontId::proportional(size), shadow);
                        painter.text(pos, egui::Align2::CENTER_CENTER, text,
                            egui::FontId::proportional(size), fg);
                    }

                    // Consume the space so egui doesn't complain
                    ui.allocate_rect(empty_rect, egui::Sense::hover());
                }
            });

        // Toast notifications overlay
        self.toasts.show(ctx);

        // ── Repaint scheduling ────────────────────────────────────────────
        // Always repaint when PTY data changed (text output).
        if any_changed {
            ctx.request_repaint();
        }
        // Repaint for animations (tab open/close) regardless of focus.
        if still_animating {
            ctx.request_repaint();
        }
        // Shader / fire animations only when focused OR background update is on.
        let has_shader = self.tabs.tabs_ref().any(|t| t.shader_effect != ridgeback_config::ShaderEffect::None);
        if has_shader && (window_focused || update_in_bg) {
            ctx.request_repaint_after(self.config.rendering.shader_frame_interval());
        } else if self.tabs.any_active() && (window_focused || update_in_bg) {
            // Poll for PTY output at 30 fps
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

/// Compile-time embedded fallback for the background image.
static BACKGROUND_PNG: &[u8] = include_bytes!("../../../assets/images/background.png");

/// Load `assets/images/background.png`.
/// Tries the filesystem first (next to exe or working dir), then falls back to
/// the compile-time embedded bytes so it always works.
fn load_background_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    // Try loading from disk first (allows hot-swapping the image)
    let disk_candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets/images/background.png"))),
        Some(std::path::PathBuf::from("assets/images/background.png")),
    ];

    let png_bytes: std::borrow::Cow<[u8]> =
        if let Some(path) = disk_candidates.into_iter().flatten().find(|p| p.exists()) {
            match std::fs::read(&path) {
                Ok(bytes) => std::borrow::Cow::Owned(bytes),
                Err(_) => std::borrow::Cow::Borrowed(BACKGROUND_PNG),
            }
        } else {
            std::borrow::Cow::Borrowed(BACKGROUND_PNG)
        };

    let img = match image::load_from_memory(&png_bytes) {
        Ok(i) => i.into_rgba8(),
        Err(e) => {
            tracing::warn!("Could not decode background image: {}", e);
            return None;
        }
    };

    let (w, h) = img.dimensions();
    let pixels: Vec<egui::Color32> = img
        .pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();

    Some(ctx.load_texture(
        "bg_image",
        egui::ColorImage { size: [w as usize, h as usize], pixels },
        egui::TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification:  egui::TextureFilter::Linear,
            ..Default::default()
        },
    ))
}

