use egui;
use ridgeback_core::InputBuffer;
use std::sync::mpsc;

/// Result from background AI query thread.
pub enum QueryResult {
    Suggestions(Vec<String>),
    Error(String),
}

/// The AI command query overlay (Ctrl+/).
pub struct CommandQueryOverlay {
    pub is_open: bool,
    pub query: String,
    pub suggestions: Vec<String>,
    pub selected_index: usize,
    pub is_loading: bool,
    pub error_message: Option<String>,
    /// Channel to receive AI results from background thread.
    result_rx: Option<mpsc::Receiver<QueryResult>>,
    /// True on first frame after opening — used to request focus exactly once.
    just_opened: bool,
}

impl CommandQueryOverlay {
    pub fn new() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            suggestions: Vec::new(),
            selected_index: 0,
            is_loading: false,
            error_message: None,
            result_rx: None,
            just_opened: false,
        }
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.query.clear();
            self.suggestions.clear();
            self.selected_index = 0;
            self.error_message = None;
            self.is_loading = false;
            self.result_rx = None;
            self.just_opened = true;
        }
    }

    /// Poll for AI results from background thread. Call each frame.
    pub fn poll_results(&mut self) {
        if let Some(ref rx) = self.result_rx {
            match rx.try_recv() {
                Ok(QueryResult::Suggestions(suggestions)) => {
                    self.is_loading = false;
                    if suggestions.is_empty() {
                        self.error_message = Some(
                            "No suggestions — check LM Studio is running at localhost:1234".to_string()
                        );
                    } else {
                        self.suggestions = suggestions;
                        self.selected_index = 0;
                        self.error_message = None;
                    }
                    self.result_rx = None;
                }
                Ok(QueryResult::Error(e)) => {
                    self.is_loading = false;
                    self.error_message = Some(format!("AI error: {}", e));
                    self.result_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {} // still loading
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.is_loading = false;
                    self.result_rx = None;
                }
            }
        }
    }

    /// Fire off a background AI query.
    pub fn submit_query(
        &mut self,
        ai_service: &ridgeback_ai::AiService,
        shell_type: ridgeback_config::ShellType,
        cwd: String,
        history: Vec<String>,
    ) {
        if self.query.is_empty() || self.is_loading {
            return;
        }

        self.suggestions.clear();
        self.error_message = None;
        self.is_loading = true;

        let query = self.query.clone();
        let (tx, rx) = mpsc::channel();
        self.result_rx = Some(rx);

        // Build what we need before the thread (AiService isn't Send, so we clone config)
        if !ai_service.is_available() {
            // AI not connected — show placeholder immediately
            self.is_loading = false;
            self.suggestions = vec![
                format!("echo \"{}\"", query),
                format!("# AI not connected — start LM Studio at localhost:1234"),
            ];
            self.error_message = Some("LM Studio not running — showing placeholder commands".to_string());
            self.result_rx = None;
            return;
        }

        // Clone config for the thread
        let config = ai_service.config_clone();
        std::thread::Builder::new()
            .name("ai-query".to_string())
            .spawn(move || {
                // Build a fresh service on the thread
                let svc = ridgeback_ai::AiService::new(&config);
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match rt {
                    Ok(rt) => {
                        let result = rt.block_on(svc.query_command(&query, shell_type, &cwd, &history));
                        match result {
                            Ok(suggestions) => { let _ = tx.send(QueryResult::Suggestions(suggestions)); }
                            Err(e) => { let _ = tx.send(QueryResult::Error(e.to_string())); }
                        }
                    }
                    Err(e) => { let _ = tx.send(QueryResult::Error(e.to_string())); }
                }
            })
            .ok();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        _input_buffer: &mut InputBuffer,
        ai_service: &ridgeback_ai::AiService,
        shell_type: ridgeback_config::ShellType,
        cwd: String,
        history: Vec<String>,
    ) -> Option<String> {
        // Poll for background results each frame
        self.poll_results();

        let mut accepted: Option<String> = None;

        let available_width = ui.available_width();
        let overlay_width = (available_width * 0.7).min(600.0).max(300.0);
        let margin = (available_width - overlay_width) / 2.0;

        egui::Frame::none()
            .fill(egui::Color32::from_rgba_premultiplied(25, 25, 35, 245))
            .rounding(12.0)
            .inner_margin(12.0)
            .outer_margin(egui::Margin {
                left: margin,
                right: margin,
                top: 20.0,
                bottom: 0.0,
            })
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
            .show(ui, |ui| {
                ui.set_min_width(overlay_width - 24.0);

                // Input field
                let te_id = ui.id().with("ai_query_input");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(te_id)
                        .desired_width(ui.available_width())
                        .hint_text("Describe what you want to do...")
                        .font(egui::FontId::monospace(14.0)),
                );

                // Request focus only on the first frame after opening
                if self.just_opened {
                    response.request_focus();
                    self.just_opened = false;
                }

                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));

                if enter_pressed && !self.query.is_empty() && self.suggestions.is_empty() && !self.is_loading {
                    self.submit_query(ai_service, shell_type, cwd.clone(), history.clone());
                } else if enter_pressed && !self.suggestions.is_empty() {
                    let selected = self.suggestions[self.selected_index].clone();
                    accepted = Some(selected);
                    self.is_open = false;
                }

                // Handle Escape
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.is_open = false;
                }

                // Arrow key navigation
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && !self.suggestions.is_empty() {
                    self.selected_index = (self.selected_index + 1) % self.suggestions.len();
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && !self.suggestions.is_empty() {
                    self.selected_index = if self.selected_index == 0 {
                        self.suggestions.len() - 1
                    } else {
                        self.selected_index - 1
                    };
                }

                // Loading indicator
                if self.is_loading {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Asking AI...")
                                .color(egui::Color32::from_gray(120))
                                .italics(),
                        );
                    });
                }

                // Error message
                if let Some(ref msg) = self.error_message {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(msg)
                            .color(egui::Color32::from_rgb(200, 150, 80))
                            .size(11.0)
                            .italics(),
                    );
                }

                // Suggestions list
                if !self.suggestions.is_empty() {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    for (i, suggestion) in self.suggestions.clone().iter().enumerate() {
                        let is_selected = i == self.selected_index;
                        let bg = if is_selected {
                            egui::Color32::from_rgba_premultiplied(50, 70, 120, 200)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let fg = if is_selected { egui::Color32::WHITE } else { egui::Color32::from_gray(200) };

                        let frame = egui::Frame::none()
                            .fill(bg)
                            .rounding(4.0)
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0));

                        let resp = frame.show(ui, |ui| {
                            let prefix = if is_selected { "▸ " } else { "  " };
                            ui.label(
                                egui::RichText::new(format!("{}{}", prefix, suggestion))
                                    .monospace()
                                    .size(13.0)
                                    .color(fg),
                            );
                        }).response.interact(egui::Sense::click());

                        if resp.clicked() {
                            accepted = Some(suggestion.clone());
                            self.is_open = false;
                        }
                        if resp.hovered() {
                            self.selected_index = i;
                        }
                    }
                }

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Enter to submit · ↑↓ navigate · Esc to close")
                        .size(10.0)
                        .color(egui::Color32::from_gray(80)),
                );
            });

        accepted
    }
}
