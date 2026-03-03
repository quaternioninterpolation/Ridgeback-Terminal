use std::sync::{Arc, Mutex};
use egui;
use ridgeback_config::{Config, keybindings::ShortcutAction};
use ridgeback_plugin::ShaderPluginHost;
use ridgeback_ai::AiService;
use ridgeback_ai::LocalModelManager;
use crate::tabs::TabManager;
use crate::shortcuts::ShortcutManager;
use crate::settings::SettingsWindow;
use crate::casting::CastManager;
use crate::toast::{Toast, ToastManager};
use crate::split_pane::SplitPaneManager;
use crate::tab_drag::TabDragState;
use crate::group_tab_bar::{self, TabBarAction};

/// Platform modifier key label: "Cmd" on macOS, "Ctrl" elsewhere.
#[cfg(target_os = "macos")]
const MOD: &str = "Cmd";
#[cfg(not(target_os = "macos"))]
const MOD: &str = "Ctrl";

/// Application version (read from build_constants.toml at compile time via build.rs).
const VERSION: &str = env!("RIDGEBACK_VERSION");
/// Build code derived from git commit count (set by build.rs).
const BUILD_CODE: &str = concat!("RBTC:", env!("RIDGEBACK_COMMIT_COUNT"));

pub struct RidgebackApp {
    pub config: Config,
    pub tabs: TabManager,
    pub shortcuts: ShortcutManager,
    pub settings_open: bool,
    pub settings: SettingsWindow,
    pub ai_service: AiService,
    pub local_model_manager: LocalModelManager,
    pub clipboard: Option<arboard::Clipboard>,
    pub cast_manager: CastManager,
    pub bg_texture: Option<egui::TextureHandle>,
    pub toasts: ToastManager,
    pub shader_host: Arc<Mutex<ShaderPluginHost>>,
    pub split_panes: SplitPaneManager,
    pub tab_drag: TabDragState,
    pub focused_profile_key: Option<String>,
    /// Profile key of the most recently opened tab (used as default for new-tab actions).
    pub last_opened_profile: Option<String>,
    /// FPS counter state: frame timestamps for the last second.
    fps_frames: Vec<std::time::Instant>,
    /// Last computed FPS value for display.
    fps_display: f32,
    /// Last time a shader repaint was allowed (for throttling).
    last_shader_repaint: std::time::Instant,
}
impl RidgebackApp {
    pub fn new(config: Config) -> Self {
        let shortcuts = ShortcutManager::from_config(&config.keybindings);
        let local_model_manager = LocalModelManager::new(&config.ai.backends.local);
        let ai_service = AiService::new(&config.ai, Some(&local_model_manager));
        let mut tabs = TabManager::new();

        // Create the initial group with the default profile
        let mut initial_profile_key: Option<String> = None;
        let initial_group_id = if let Some((name, profile)) = config.default_profile() {
            match tabs.new_group_with_tab(name, profile) {
                Ok(id) => {
                    initial_profile_key = Some(name.to_string());
                    id
                }
                Err(e) => {
                    tracing::error!("Failed to open default tab: {}", e);
                    tabs.new_group()
                }
            }
        } else {
            tabs.new_group()
        };
        tabs.set_focused_group(initial_group_id);

        let shaders_dir = ShaderPluginHost::find_shaders_dir();
        let plugins_dir = ShaderPluginHost::find_plugins_dir();
        let mut host = ShaderPluginHost::new(shaders_dir, plugins_dir);
        if let Err(e) = host.load_user_plugins() { tracing::warn!("Plugin load: {}", e); }
        let shader_host = Arc::new(Mutex::new(host));
        crate::particle_emit::set_host(Arc::clone(&shader_host));

        Self {
            settings: SettingsWindow::new(config.clone()),
            config, tabs, shortcuts,
            settings_open: false,
            ai_service,
            local_model_manager,
            clipboard: arboard::Clipboard::new().ok(),
            cast_manager: CastManager::new(),
            bg_texture: None,
            toasts: ToastManager::new(),
            shader_host,
            split_panes: SplitPaneManager::new(initial_group_id),
            tab_drag: TabDragState::default(),
            focused_profile_key: None,
            last_opened_profile: initial_profile_key,
            fps_frames: Vec::new(),
            fps_display: 0.0,
            last_shader_repaint: std::time::Instant::now(),
        }
    }

    /// Returns the preferred profile for new-tab actions: last-opened if valid,
    /// otherwise the configured default, otherwise the first available.
    fn preferred_profile(&self) -> Option<(String, ridgeback_config::Profile)> {
        // 1. Last opened profile
        if let Some(key) = &self.last_opened_profile {
            if let Some(p) = self.config.profiles.get(key) {
                return Some((key.clone(), p.clone()));
            }
        }
        // 2. Configured default profile
        if let Some((name, p)) = self.config.default_profile() {
            return Some((name.to_string(), p.clone()));
        }
        // 3. First available
        self.config.profiles.iter().next().map(|(k, v)| (k.clone(), v.clone()))
    }

    fn handle_shortcut(&mut self, ctx: &egui::Context, action: ShortcutAction) {
        match action {
            ShortcutAction::NewTab => {
                tracing::info!("NewTab shortcut triggered!");
                if let Some((name, profile)) = self.preferred_profile() {
                    let _ = self.tabs.open_tab_in_focused(&name, &profile);
                    self.last_opened_profile = Some(name);
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::CloseTab => {
                if let Some(g) = self.tabs.focused_group_mut() {
                    g.close_active_tab();
                }
            }
            ShortcutAction::ClosePane => self.close_focused_group(),
            ShortcutAction::NextTab => {
                if let Some(g) = self.tabs.focused_group_mut() {
                    g.next_tab();
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::PrevTab => {
                if let Some(g) = self.tabs.focused_group_mut() {
                    g.prev_tab();
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::OpenSettings => {
                let key = self.focused_profile_key.clone();
                self.settings_open = !self.settings_open;
                if self.settings_open { self.settings.set_focused_profile(key.as_deref()); }
            }
            ShortcutAction::SaveSession => self.save_session(),
            ShortcutAction::FindInSession => {
                if let Some(t) = self.tabs.active_tab_mut() { t.find_overlay.toggle(); }
            }
            ShortcutAction::AiCommandQuery => {
                if let Some(t) = self.tabs.active_tab_mut() { t.command_query.toggle(); }
            }
            ShortcutAction::SplitHorizontal => {
                if let Some((name, profile)) = self.preferred_profile() {
                    if let Ok(new_id) = self.tabs.new_group_with_tab(&name, &profile) {
                        self.split_panes.split_horizontal(new_id);
                        self.tabs.set_focused_group(new_id);
                        self.last_opened_profile = Some(name);
                    }
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::SplitVertical => {
                if let Some((name, profile)) = self.preferred_profile() {
                    if let Ok(new_id) = self.tabs.new_group_with_tab(&name, &profile) {
                        self.split_panes.split_vertical(new_id);
                        self.tabs.set_focused_group(new_id);
                        self.last_opened_profile = Some(name);
                    }
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::FocusNextGroup => {
                if let Some(id) = self.tabs.next_group_id_from_focused() {
                    self.tabs.set_focused_group(id);
                    self.split_panes.set_focused_group(id);
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::FocusPrevGroup => {
                if let Some(id) = self.tabs.prev_group_id_from_focused() {
                    self.tabs.set_focused_group(id);
                    self.split_panes.set_focused_group(id);
                }
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::MoveTabNextGroup => {
                self.move_active_tab_to_adjacent_group(true);
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::MoveTabPrevGroup => {
                self.move_active_tab_to_adjacent_group(false);
                self.focus_active_terminal(ctx);
            }
            ShortcutAction::ReloadPlugins => {
                if let Ok(mut h) = self.shader_host.lock() {
                    match h.reload() {
                        Ok(n) => self.toasts.push(Toast::info(format!("Reloaded {} plugin(s).", n))),
                        Err(e) => self.toasts.push(Toast::error(format!("Reload error: {}", e))),
                    }
                }
                crate::particle_emit::set_host(Arc::clone(&self.shader_host));
            }
        }
    }

    /// Request egui input focus for the active terminal of the currently focused group.
    fn focus_active_terminal(&self, ctx: &egui::Context) {
        if let Some(g) = self.tabs.focused_group() {
            if let Some(tab) = g.active_tab() {
                let input_id = egui::Id::new(("inline_input", tab.terminal_id));
                ctx.memory_mut(|m| m.request_focus(input_id));
            }
        }
    }

    fn close_focused_group(&mut self) {
        let _id = self.tabs.focused_group_id();
        if let Some(g) = self.tabs.focused_group_mut() {
            g.close_all_tabs();
        }
        // Will be cleaned up when animations finish and group is empty
    }

    fn move_active_tab_to_adjacent_group(&mut self, next: bool) {
        let target_id = if next {
            self.tabs.next_group_id_from_focused()
        } else {
            self.tabs.prev_group_id_from_focused()
        };
        if let Some(target_id) = target_id {
            let src_id = self.tabs.focused_group_id();
            let tab_idx = self.tabs.focused_group().map(|g| g.active_index()).unwrap_or(0);
            let src_empty = self.tabs.move_tab(src_id, tab_idx, target_id);
            self.tabs.set_focused_group(target_id);
            self.split_panes.set_focused_group(target_id);
            if src_empty {
                self.split_panes.on_group_removed(src_id);
                self.tabs.remove_group(src_id);
            }
        }
    }

    fn save_session(&mut self) {
        if let Some(tab) = self.tabs.active_tab() {
            let log = tab.terminal.full_log();
            let name = format!("ridgeback_{}.txt", {
                let d = std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap_or_default();
                d.as_secs()
            });
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("Text", &["txt"]).save_file() {
                    let _ = std::fs::write(path, log);
                }
            });
        }
    }

    /// Process tab bar actions that were collected during rendering.
    fn process_tab_bar_actions(&mut self, ctx: &egui::Context, group_id: usize, actions: Vec<TabBarAction>) {
        for action in actions {
            match action {
                TabBarAction::ClickTab(idx) => {
                    if let Some(g) = self.tabs.group_by_id_mut(group_id) {
                        g.set_active(idx);
                        // Request focus for the newly-active terminal's input
                        if let Some(tab) = g.active_tab() {
                            let input_id = egui::Id::new(("inline_input", tab.terminal_id));
                            ctx.memory_mut(|m| m.request_focus(input_id));
                        }
                    }
                    self.tabs.set_focused_group(group_id);
                    self.split_panes.set_focused_group(group_id);
                }
                TabBarAction::CloseTab(idx) => {
                    if let Some(g) = self.tabs.group_by_id_mut(group_id) {
                        g.close_tab(idx);
                    }
                }
                TabBarAction::CloseOtherTabs(idx) => {
                    if let Some(g) = self.tabs.group_by_id_mut(group_id) {
                        g.close_other_tabs(idx);
                    }
                }
                TabBarAction::CloseTabsToRight(idx) => {
                    if let Some(g) = self.tabs.group_by_id_mut(group_id) {
                        g.close_tabs_to_right(idx);
                    }
                }
                TabBarAction::DragStart { tab_idx, origin } => {
                    self.tab_drag.start(group_id, tab_idx, origin);
                }
                TabBarAction::NewTab { profile_key } => {
                    tracing::info!("TabBarAction::NewTab triggered for profile '{}'", profile_key);
                    if let Some(profile) = self.config.profiles.get(&profile_key).cloned() {
                        if let Some(g) = self.tabs.group_by_id_mut(group_id) {
                            let _ = g.open_tab(&profile_key, &profile);
                        }
                        self.last_opened_profile = Some(profile_key);
                    }
                    self.tabs.set_focused_group(group_id);
                    self.split_panes.set_focused_group(group_id);
                }
                TabBarAction::SplitRight(tab_idx) => {
                    self.tear_tab_to_new_group(group_id, tab_idx, true);
                }
                TabBarAction::SplitDown(tab_idx) => {
                    self.tear_tab_to_new_group(group_id, tab_idx, false);
                }
                TabBarAction::TearOff(tab_idx) => {
                    // Tear off to a new group beside the source (horizontal split)
                    self.tear_tab_to_new_group(group_id, tab_idx, true);
                }
                TabBarAction::CloseGroup => {
                    if let Some(g) = self.tabs.group_by_id_mut(group_id) {
                        g.close_all_tabs();
                    }
                }
            }
        }
    }

    /// Move a tab from a group into a brand new group, splitting the tree.
    fn tear_tab_to_new_group(&mut self, source_group_id: usize, tab_idx: usize, horizontal: bool) {
        let new_group_id = self.tabs.new_group();
        let src_empty = self.tabs.move_tab(source_group_id, tab_idx, new_group_id);

        if horizontal {
            self.split_panes.split_group_horizontal(source_group_id, new_group_id);
        } else {
            self.split_panes.split_group_vertical(source_group_id, new_group_id);
        }

        self.tabs.set_focused_group(new_group_id);
        self.split_panes.set_focused_group(new_group_id);

        if src_empty {
            self.split_panes.on_group_removed(source_group_id);
            self.tabs.remove_group(source_group_id);
        }
    }

    /// Clean up empty groups after animation ticks.
    fn cleanup_empty_groups(&mut self) {
        let empty_ids: Vec<usize> = self.tabs.group_ids_ordered()
            .into_iter()
            .filter(|&id| self.tabs.is_group_empty(id))
            .collect();
        // Keep at least one group
        let total = self.tabs.group_count();
        let can_remove = total.saturating_sub(1);
        for (i, id) in empty_ids.into_iter().enumerate() {
            if i >= can_remove { break; }
            self.split_panes.on_group_removed(id);
            self.tabs.remove_group(id);
        }
    }
}

impl eframe::App for RidgebackApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── FPS tracking ──────────────────────────────────────────────────
        let now = std::time::Instant::now();
        self.fps_frames.push(now);
        // Keep only frames from the last second
        let one_sec_ago = now - std::time::Duration::from_secs(1);
        self.fps_frames.retain(|&t| t >= one_sec_ago);
        self.fps_display = self.fps_frames.len() as f32;

        if self.bg_texture.is_none() { self.bg_texture = load_background_texture(ctx); }
        let window_focused = ctx.input(|i| i.focused);
        let update_in_bg   = self.config.rendering.update_in_background;
        let dt = ctx.input(|i| i.unstable_dt).min(0.1);
        let still_animating = self.tabs.tick_animations(dt);

        // ── Shader FPS throttle ───────────────────────────────────────────
        // Determine effective max FPS: 1 FPS when in background and
        // update_in_background is off, otherwise the configured max.
        let effective_max_fps = if !window_focused && !update_in_bg {
            1u32
        } else {
            self.config.rendering.effective_shader_fps()
        };
        let shader_interval = std::time::Duration::from_secs_f64(1.0 / effective_max_fps as f64);
        let allow_shader_repaint = now.duration_since(self.last_shader_repaint) >= shader_interval;
        if allow_shader_repaint {
            self.last_shader_repaint = now;
        }

        // Clean up groups that became empty after close animations finished
        self.cleanup_empty_groups();

        let mut any_changed = false;
        for tab in self.tabs.all_tabs_mut() {
            if tab.terminal.process_pty_output() { any_changed = true; }
        }
        if let Some(action) = self.shortcuts.check(ctx) { self.handle_shortcut(ctx, action); }
        crate::particle_emit::set_host(Arc::clone(&self.shader_host));

        // ── Top toolbar (settings gear, split buttons, profile picker) ────
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            self.show_toolbar(ui);
        });

        if self.settings_open {
            let mut still_open = true;
            let mut saved_keys: Vec<String> = Vec::new();
            let screen_rect = ctx.screen_rect();
            let max_w = (screen_rect.width() - 40.0).max(400.0);
            let max_h = (screen_rect.height() - 40.0).max(300.0);
            let default_w = 700.0_f32.min(max_w);
            let default_h = 480.0_f32.min(max_h);

            egui::Window::new("Settings")
                .open(&mut still_open)
                .resizable(true)
                .constrain(true)
                .scroll(false)
                .default_size([default_w, default_h])
                .min_size([400.0, 300.0])
                .max_size([max_w, max_h])
                .show(ctx, |ui| {
                let host_guard = self.shader_host.lock().unwrap();
                saved_keys = self.settings.show(ui, &mut self.config, &mut self.cast_manager, &host_guard, &self.local_model_manager, &mut self.toasts);
            });

            if !still_open {
                self.settings_open = false;
            }
            for key in &saved_keys {
                if let Some(profile) = self.config.profiles.get(key).cloned() {
                    let mut updated = false;
                    for tab in self.tabs.all_tabs_mut() {
                        if tab.terminal.profile_name != *key { continue; }
                        updated = true;
                        tab.shader_effect = profile.shader_effect.clone();
                        tab.typing_particles = profile.typing_particles.clone();
                        tab.text_shadow_enabled = profile.text_shadow_enabled;
                        tab.text_shadow_alpha = profile.text_shadow_alpha;
                        tab.text_foreground = profile.text_foreground.clone();
                        tab.padding = profile.padding.clone();
                    }
                    let msg = if updated {
                        format!("\"{}\" — settings applied to open tabs.", profile.name)
                    } else {
                        format!("\"{}\" profile saved.", profile.name)
                    };
                    self.toasts.push(Toast::info(msg));
                }
            }
        }

        // ── Central panel with split panes ────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_gray(15)))
            .show(ctx, |ui| {
                if self.tabs.any_active() {
                    let bg_tex = self.bg_texture.clone();
                    let allow_repaint = allow_shader_repaint;
                    let available = ui.available_rect_before_wrap();

                    // Pre-compute profile names list for tab bar + buttons
                    let profile_names: Vec<(String, String)> = self.config.profiles.iter()
                        .map(|(k, v)| (k.clone(), v.name.clone()))
                        .collect();

                    // Preferred profile key for left-click new-tab (last-opened or default)
                    let preferred_key: Option<String> = self.preferred_profile().map(|(k, _)| k);

                    let clipboard = &mut self.clipboard as *mut _;
                    let focused_profile = &mut self.focused_profile_key;
                    let tabs_ptr = &mut self.tabs as *mut TabManager;
                    let ai_service = &self.ai_service;
                    let local_mgr = &self.local_model_manager;
                    let tab_drag = &self.tab_drag;
                    let config_ptr = &self.config as *const Config;

                    // Collect actions from tab bars to process after rendering
                    let mut deferred_actions: Vec<(usize, Vec<TabBarAction>)> = Vec::new();

                    // Pre-check: if the user clicked anywhere, determine which pane was clicked
                    // and update focus BEFORE rendering so the correct pane gets keyboard input.
                    let click_pos = ui.ctx().input(|i| {
                        if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None }
                    });
                    if let Some(pos) = click_pos {
                        if let Some(clicked_group) = self.split_panes.hit_test(available, pos) {
                            if clicked_group != self.split_panes.focused_group_id {
                                self.split_panes.set_focused_group(clicked_group);
                                self.tabs.set_focused_group(clicked_group);
                            }
                        }
                    }

                    self.split_panes.show(ui, available, &mut |ui, header_rect, body_rect, group_id, is_focused| -> bool {
                        let tabs = unsafe { &mut *tabs_ptr };
                        let clipboard = unsafe { &mut *clipboard };
                        let _config = unsafe { &*config_ptr };

                        if let Some(group) = tabs.group_by_id(group_id) {
                            // Draw the group's tab bar header
                            let actions = group_tab_bar::draw_group_tab_bar(
                                ui, header_rect, group, tab_drag, is_focused, &profile_names, preferred_key.as_deref(),
                            );
                            if !actions.is_empty() {
                                deferred_actions.push((group_id, actions));
                            }
                        }

                        // Render the group's active tab terminal in the body
                        if let Some(group) = tabs.group_by_id_mut(group_id) {
                            if let Some(tab) = group.active_tab_mut() {
                                let tid = tab.terminal_id;
                                if is_focused {
                                    *focused_profile = Some(tab.terminal.profile_name.clone());
                                    ui.painter().rect_stroke(body_rect.shrink(1.0), 0.0,
                                        egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(137,180,250,120)));
                                }


                                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body_rect), |ui| {
                                    if tab.command_query.is_open {
                                        let st = tab.terminal.shell_type;
                                        // Use the terminal's title (often contains CWD from shell integration)
                                        // or fall back to the profile's configured working directory.
                                        let cwd = tab.terminal.title()
                                            .map(|t| t.to_string())
                                            .unwrap_or_else(|| {
                                                _config.profiles.get(&tab.terminal.profile_name)
                                                    .map(|p| p.working_directory.to_string_lossy().to_string())
                                                    .unwrap_or_else(|| "~".to_string())
                                            });
                                        let hist = tab.terminal.last_n_lines(5);
                                        match tab.command_query.show(ui, &mut tab.terminal.input, ai_service, local_mgr, st, cwd, hist) {
                                            Some(crate::command_query::CommandAction::Execute(ref cmd)) => {
                                                tracing::info!("CommandQuery: EXECUTE '{}'", cmd);
                                                // Write command + Return to PTY to execute
                                                let _ = tab.terminal.write_to_pty(cmd.as_bytes());
                                                let _ = tab.terminal.write_to_pty(b"\r");
                                                // Redirect focus to terminal so no button captures stale keys
                                                let input_id = egui::Id::new(("inline_input", tid));
                                                ui.ctx().memory_mut(|m| m.request_focus(input_id));
                                            }
                                            Some(crate::command_query::CommandAction::Insert(ref cmd)) => {
                                                tracing::info!("CommandQuery: INSERT '{}'", cmd);
                                                // Write command to PTY without Return so it appears
                                                // on the shell input line for the user to review/edit
                                                let _ = tab.terminal.write_to_pty(cmd.as_bytes());
                                                // Redirect focus to terminal so no button captures stale keys
                                                let input_id = egui::Id::new(("inline_input", tid));
                                                ui.ctx().memory_mut(|m| m.request_focus(input_id));
                                            }
                                            None => {}
                                        }
                                    }
                                    if tab.find_overlay.is_open {
                                        tab.find_overlay.show(ui, &tab.terminal);
                                    }
                                    crate::terminal_view::show_terminal(ui, tab, clipboard, bg_tex.as_ref(), allow_repaint, tid, is_focused);
                                });
                            }
                        }

                        // Focus is pre-computed via hit_test before show() is called
                        false
                    });

                    // Process deferred tab bar actions
                    for (group_id, actions) in deferred_actions {
                        self.process_tab_bar_actions(ctx, group_id, actions);
                    }

                    // Handle drag-to-split drop zones
                    if self.tab_drag.is_dragging() {
                        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                            self.tab_drag.update(pos);

                            // Draw ghost
                            if let Some(source) = &self.tab_drag.dragging {
                                let title = self.tabs.group_by_id(source.group_id)
                                    .and_then(|g| g.tab(source.tab_idx))
                                    .map(|t| t.tab_title.clone())
                                    .unwrap_or_default();
                                self.tab_drag.draw_ghost(ui, &title);
                            }

                            // Drop zone preview
                            if self.tab_drag.threshold_passed {
                                let zones = self.split_panes.compute_drop_zones(available);
                                let source_group = self.tab_drag.dragging.as_ref().map(|s| s.group_id).unwrap_or(usize::MAX);
                                let hovered = group_tab_bar::draw_drop_zone_preview(ui, &zones, pos, source_group);

                                // Handle drop
                                if ctx.input(|i| i.pointer.any_released()) {
                                    if let (Some(zone), Some(source)) = (hovered, self.tab_drag.dragging.clone()) {
                                        match zone.side {
                                            crate::split_pane::DropSide::Center => {
                                                // Move tab into target group
                                                let src_empty = self.tabs.move_tab(source.group_id, source.tab_idx, zone.group_id);
                                                self.tabs.set_focused_group(zone.group_id);
                                                self.split_panes.set_focused_group(zone.group_id);
                                                if src_empty {
                                                    self.split_panes.on_group_removed(source.group_id);
                                                    self.tabs.remove_group(source.group_id);
                                                }
                                            }
                                            crate::split_pane::DropSide::Left | crate::split_pane::DropSide::Right => {
                                                let new_gid = self.tabs.new_group();
                                                let src_empty = self.tabs.move_tab(source.group_id, source.tab_idx, new_gid);
                                                if zone.side == crate::split_pane::DropSide::Left {
                                                    // Insert new group to the LEFT of target: split target, new goes left
                                                    self.split_panes.split_group_horizontal(zone.group_id, new_gid);
                                                    // swap so new is on left
                                                    self.split_panes.swap_groups(zone.group_id, new_gid);
                                                } else {
                                                    self.split_panes.split_group_horizontal(zone.group_id, new_gid);
                                                }
                                                self.tabs.set_focused_group(new_gid);
                                                self.split_panes.set_focused_group(new_gid);
                                                if src_empty {
                                                    self.split_panes.on_group_removed(source.group_id);
                                                    self.tabs.remove_group(source.group_id);
                                                }
                                            }
                                            crate::split_pane::DropSide::Top | crate::split_pane::DropSide::Bottom => {
                                                let new_gid = self.tabs.new_group();
                                                let src_empty = self.tabs.move_tab(source.group_id, source.tab_idx, new_gid);
                                                if zone.side == crate::split_pane::DropSide::Top {
                                                    self.split_panes.split_group_vertical(zone.group_id, new_gid);
                                                    self.split_panes.swap_groups(zone.group_id, new_gid);
                                                } else {
                                                    self.split_panes.split_group_vertical(zone.group_id, new_gid);
                                                }
                                                self.tabs.set_focused_group(new_gid);
                                                self.split_panes.set_focused_group(new_gid);
                                                if src_empty {
                                                    self.split_panes.on_group_removed(source.group_id);
                                                    self.tabs.remove_group(source.group_id);
                                                }
                                            }
                                        }
                                    }
                                    self.tab_drag.end();
                                }
                            }
                        }
                        if ctx.input(|i| i.pointer.any_released()) {
                            self.tab_drag.end();
                        }
                    }
                } else {
                    show_empty_state(ui, self.bg_texture.as_ref());
                }
            });

        self.toasts.show(ctx);

        // ── Repaint scheduling with FPS limiting ──────────────────────────
        // ALL repaints go through request_repaint_after to enforce the max FPS cap.
        // Using request_repaint() (immediate) would bypass the limit.
        let needs_repaint = any_changed || still_animating
            || self.tabs.all_tabs_ref().any(|t| t.shader_effect.plugin_id != "none")
            || self.tabs.any_active();

        if needs_repaint {
            if !window_focused && !update_in_bg {
                // Background with update_in_background off: 1 FPS
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            } else {
                // Enforce the configured max FPS for everything
                ctx.request_repaint_after(shader_interval);
            }
        }

        // ── FPS overlay ───────────────────────────────────────────────────
        if self.config.rendering.show_fps_overlay {
            let screen = ctx.screen_rect();
            let fps_text = format!("{:.0} FPS", self.fps_display);
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("fps_overlay"),
            ));
            let font_id = egui::FontId::monospace(11.0);
            let galley = ctx.fonts(|f| f.layout_no_wrap(fps_text.clone(), font_id.clone(), egui::Color32::WHITE));
            let text_size = galley.size();
            let pill_rect = egui::Rect::from_min_size(
                egui::pos2(screen.right() - text_size.x - 20.0, screen.bottom() - text_size.y - 12.0),
                egui::vec2(text_size.x + 12.0, text_size.y + 4.0),
            );
            painter.rect_filled(pill_rect, 4.0, egui::Color32::from_black_alpha(180));
            painter.text(
                pill_rect.center(),
                egui::Align2::CENTER_CENTER,
                &fps_text,
                font_id,
                egui::Color32::from_rgb(0x80, 0xff, 0x80),
            );
        }
    }
}

impl RidgebackApp {
    /// Compact toolbar at the top: settings gear, split buttons, profile picker.
    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = 4.0;
            // Disable keyboard focus for toolbar buttons — prevents Enter from
            // accidentally activating the + button after overlay closes.
            ui.style_mut().interaction.selectable_labels = false;

            // Profile picker (+ new tab in focused group)
            let nr = ui.add(egui::Button::new(
                egui::RichText::new(egui_phosphor::regular::PLUS).size(16.0).color(egui::Color32::from_gray(200)))
                .fill(egui::Color32::from_gray(28)).stroke(egui::Stroke::NONE).min_size(egui::vec2(28.0, 28.0))
                .sense(egui::Sense::click()));
            // Left-click: open preferred (last-opened / default) profile directly
            if nr.clicked() {
                tracing::info!("Toolbar + button clicked!");
                if let Some((name, profile)) = self.preferred_profile() {
                    let _ = self.tabs.open_tab_in_focused(&name, &profile);
                    self.last_opened_profile = Some(name);
                }
            }
            // Right-click or secondary-click: show profile picker popup
            if nr.secondary_clicked() {
                ui.memory_mut(|m| m.toggle_popup(nr.id));
            }
            let nr = nr.on_hover_text(format!("New Tab ({MOD}+T) · Right-click for profile picker"));
            egui::popup_below_widget(ui, nr.id, &nr, egui::PopupCloseBehavior::CloseOnClickOutside, |ui: &mut egui::Ui| {
                ui.set_min_width(160.0);
                let profiles: Vec<(String, ridgeback_config::Profile)> = self.config.profiles.iter().map(|(k,v)|(k.clone(),v.clone())).collect();
                for (name, profile) in profiles {
                    if ui.button(&profile.name).clicked() {
                        let _ = self.tabs.open_tab_in_focused(&name, &profile);
                        self.last_opened_profile = Some(name);
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            });

            // Split horizontal
            let sh = ui.add(egui::Button::new(
                egui::RichText::new(egui_phosphor::regular::SPLIT_HORIZONTAL).size(16.0).color(egui::Color32::from_gray(180)))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                .min_size(egui::vec2(28.0, 28.0)));
            if sh.clicked() { self.handle_shortcut(ui.ctx(), ShortcutAction::SplitHorizontal); }
            sh.on_hover_text(format!("Split Right ({MOD}+Shift+D)"));

            // Split vertical
            let sv = ui.add(egui::Button::new(
                egui::RichText::new(egui_phosphor::regular::SPLIT_VERTICAL).size(16.0).color(egui::Color32::from_gray(180)))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                .min_size(egui::vec2(28.0, 28.0)));
            if sv.clicked() { self.handle_shortcut(ui.ctx(), ShortcutAction::SplitVertical); }
            sv.on_hover_text(format!("Split Down ({MOD}+Shift+E)"));

            // Settings gear (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gr = ui.add(egui::Button::new(
                    egui::RichText::new(egui_phosphor::regular::GEAR).size(16.0).color(egui::Color32::from_gray(180)))
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                    .min_size(egui::vec2(28.0, 28.0)));
                if gr.clicked() {
                    let key = self.focused_profile_key.clone();
                    self.settings_open = !self.settings_open;
                    if self.settings_open { self.settings.set_focused_profile(key.as_deref()); }
                }
                gr.on_hover_text(format!("Settings ({MOD}+,)"));
            });
        });
    }
}

fn show_empty_state(ui: &mut egui::Ui, bg_texture: Option<&egui::TextureHandle>) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(0x1e,0x1e,0x2e));
    if let Some(tex) = bg_texture {
        let (iw,ih)=(tex.size()[0] as f32,tex.size()[1] as f32);
        let scale=(rect.width()/iw).max(rect.height()/ih);
        let (sw,sh)=(iw*scale,ih*scale);
        let (u0,v0)=((sw-rect.width())/(2.0*sw),(sh-rect.height())/(2.0*sh));
        ui.painter().image(tex.id(),rect,
            egui::Rect::from_min_max(egui::pos2(u0,v0),egui::pos2(1.0-u0,1.0-v0)),
            egui::Color32::from_white_alpha(128));
    }
    let c=rect.center();
    let shadow=egui::Color32::from_black_alpha(180);
    let split_hint = format!("{MOD}+T  /  {MOD}+Shift+D/E to split");
    for (dy,text,sz,fg) in [
        (0.0f32, "Open a new terminal tab", 20.0f32, egui::Color32::from_gray(220)),
        (34.0,   split_hint.as_str(), 13.0, egui::Color32::from_gray(140)),
    ] {
        let pos=egui::pos2(c.x,c.y+dy);
        ui.painter().text(egui::pos2(pos.x+1.0,pos.y+1.0),egui::Align2::CENTER_CENTER,text,egui::FontId::proportional(sz),shadow);
        ui.painter().text(pos,egui::Align2::CENTER_CENTER,text,egui::FontId::proportional(sz),fg);
    }

    // Version and build code at the bottom
    let version_text = format!("v{}  ·  {}", VERSION, BUILD_CODE);
    let version_pos = egui::pos2(rect.center().x, rect.bottom() - 16.0);
    ui.painter().text(
        egui::pos2(version_pos.x + 1.0, version_pos.y + 1.0),
        egui::Align2::CENTER_CENTER,
        &version_text,
        egui::FontId::proportional(11.0),
        shadow,
    );
    ui.painter().text(
        version_pos,
        egui::Align2::CENTER_CENTER,
        &version_text,
        egui::FontId::proportional(11.0),
        egui::Color32::from_gray(90),
    );

    ui.allocate_rect(rect,egui::Sense::hover());
}

static BACKGROUND_PNG: &[u8] = include_bytes!("../../../assets/images/background.png");

fn load_background_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let png: &[u8] = if let Some(p) = std::env::current_exe().ok()
        .and_then(|e| e.parent().map(|d| d.join("assets/images/background.png")))
        .filter(|p| p.exists())
        .or_else(|| Some(std::path::PathBuf::from("assets/images/background.png")).filter(|p| p.exists()))
    {
        return {
            let bytes = std::fs::read(&p).ok()?;
            let img = image::load_from_memory(&bytes).ok()?.into_rgba8();
            let (w,h)=img.dimensions();
            let pixels=img.pixels().map(|p|egui::Color32::from_rgba_unmultiplied(p[0],p[1],p[2],p[3])).collect();
            Some(ctx.load_texture("bg",egui::ColorImage{size:[w as usize,h as usize],pixels},
                egui::TextureOptions{magnification:egui::TextureFilter::Linear,minification:egui::TextureFilter::Linear,..Default::default()}))
        };
    } else { BACKGROUND_PNG };
    let img = image::load_from_memory(png).ok()?.into_rgba8();
    let (w,h)=img.dimensions();
    let pixels=img.pixels().map(|p|egui::Color32::from_rgba_unmultiplied(p[0],p[1],p[2],p[3])).collect();
    Some(ctx.load_texture("bg",egui::ColorImage{size:[w as usize,h as usize],pixels},
        egui::TextureOptions{magnification:egui::TextureFilter::Linear,minification:egui::TextureFilter::Linear,..Default::default()}))
}
