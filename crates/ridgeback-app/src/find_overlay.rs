use egui;
use ridgeback_core::Terminal;
use ridgeback_core::search::{SearchOptions, SearchMatch};

/// The Find in Session overlay (Ctrl+F).
pub struct FindOverlay {
    pub is_open: bool,
    pub query: String,
    pub use_regex: bool,
    pub ignore_case: bool,
    pub matches: Vec<SearchMatch>,
    pub current_match: usize,
    /// True on the first frame after opening — used to request focus exactly once.
    just_opened: bool,
}

impl FindOverlay {
    pub fn new() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            use_regex: false,
            ignore_case: true,
            matches: Vec::new(),
            current_match: 0,
            just_opened: false,
        }
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.just_opened = true;
        } else {
            self.matches.clear();
        }
    }

    /// Returns the absolute line number of the current match (for scroll-to),
    /// or `None` if there are no matches.
    pub fn current_match_line(&self) -> Option<usize> {
        if self.matches.is_empty() { return None; }
        Some(self.matches[self.current_match].line)
    }

    pub fn show(&mut self, ui: &mut egui::Ui, terminal: &Terminal) {
        egui::Frame::none()
            .fill(egui::Color32::from_rgba_premultiplied(30, 30, 40, 240))
            .rounding(8.0)
            .inner_margin(8.0)
            .outer_margin(egui::Margin::symmetric(50.0, 0.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Search input
                    let te_id = ui.id().with("find_input");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(te_id)
                            .desired_width(300.0)
                            .hint_text("Search...")
                            .font(egui::FontId::monospace(13.0)),
                    );

                    // Request focus only on the first frame after opening
                    if self.just_opened {
                        response.request_focus();
                        self.just_opened = false;
                    }

                    // Search when text changes or enter is pressed
                    if response.changed() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        self.perform_search(terminal);
                    }

                    // Navigate matches
                    if ui.button("▲").clicked() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.shift)) {
                        if !self.matches.is_empty() {
                            self.current_match = if self.current_match == 0 {
                                self.matches.len() - 1
                            } else {
                                self.current_match - 1
                            };
                        }
                    }
                    if ui.button("▼").clicked() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)) {
                        if !self.matches.is_empty() {
                            self.current_match = (self.current_match + 1) % self.matches.len();
                        }
                    }

                    // Match count
                    if self.matches.is_empty() && !self.query.is_empty() {
                        ui.label(
                            egui::RichText::new("No matches")
                                .color(egui::Color32::from_rgb(200, 100, 100))
                                .size(12.0),
                        );
                    } else if !self.matches.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} of {}",
                                self.current_match + 1,
                                self.matches.len()
                            ))
                            .size(12.0)
                            .color(egui::Color32::from_gray(180)),
                        );
                    }

                    // Toggle buttons
                    let case_btn = if self.ignore_case { "Aa" } else { "Aa" };
                    let case_color = if self.ignore_case {
                        egui::Color32::from_gray(100)
                    } else {
                        egui::Color32::from_rgb(100, 180, 255)
                    };
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(case_btn).color(case_color).size(12.0),
                        ))
                        .on_hover_text("Toggle case sensitivity")
                        .clicked()
                    {
                        self.ignore_case = !self.ignore_case;
                        self.perform_search(terminal);
                    }

                    let regex_color = if self.use_regex {
                        egui::Color32::from_rgb(100, 180, 255)
                    } else {
                        egui::Color32::from_gray(100)
                    };
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(".*").color(regex_color).size(12.0),
                        ))
                        .on_hover_text("Toggle regex mode")
                        .clicked()
                    {
                        self.use_regex = !self.use_regex;
                        self.perform_search(terminal);
                    }

                    // Close button
                    if ui.button("✕").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.is_open = false;
                        self.matches.clear();
                    }
                });
            });
    }

    fn perform_search(&mut self, terminal: &Terminal) {
        let options = SearchOptions {
            pattern: self.query.clone(),
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
        };
        self.matches = terminal.search(&options);
        self.current_match = 0;
    }
}
