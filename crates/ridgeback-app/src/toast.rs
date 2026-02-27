/// Toast notification system for transient, non-blocking messages.
///
/// Supports Info, Warning, and Error severity levels with auto-dismiss.

use std::time::{Duration, Instant};

/// Severity level for a toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Info,
            created: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Warning,
            created: Instant::now(),
            duration: Duration::from_secs(5),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Error,
            created: Instant::now(),
            duration: Duration::from_secs(6),
        }
    }

    /// Returns true if the toast has expired and should be removed.
    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= self.duration
    }

    /// Returns the remaining fraction (1.0 = full, 0.0 = expired).
    pub fn remaining_fraction(&self) -> f32 {
        let elapsed = self.created.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32();
        (1.0 - (elapsed / total)).clamp(0.0, 1.0)
    }
}

/// Manages a queue of active toasts.
#[derive(Debug, Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self { toasts: Vec::new() }
    }

    /// Push a new toast notification.
    pub fn push(&mut self, toast: Toast) {
        // Keep at most 5 toasts visible
        if self.toasts.len() >= 5 {
            self.toasts.remove(0);
        }
        self.toasts.push(toast);
    }

    /// Convenience: push an info toast.
    pub fn info(&mut self, message: impl Into<String>) {
        self.push(Toast::info(message));
    }

    /// Convenience: push a warning toast.
    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(Toast::warning(message));
    }

    /// Convenience: push an error toast.
    pub fn error(&mut self, message: impl Into<String>) {
        self.push(Toast::error(message));
    }

    /// Render all active toasts in the bottom-right of the screen.
    /// Each toast expands vertically to fit its full message text.
    pub fn show(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| !t.is_expired());
        if self.toasts.is_empty() {
            return;
        }

        let screen_rect = ctx.screen_rect();
        let toast_width = 360.0_f32;
        let margin      = 12.0_f32;
        let spacing     =  6.0_f32;
        let h_pad       = 10.0_f32; // horizontal padding inside toast
        let v_pad       =  8.0_f32; // vertical padding inside toast
        let font_id     = egui::FontId::proportional(13.0);
        let text_width  = toast_width - h_pad * 2.0 - 20.0; // leave room for icon

        // ── Measure each toast's wrapped text height ──────────────────────
        // We do this outside the Area so we can compute the correct Y origin.
        let heights: Vec<f32> = ctx.fonts(|f| {
            self.toasts.iter().map(|toast| {
                let full_text = format!("  {} ", toast.message); // icon placeholder width
                let galley = f.layout(
                    full_text,
                    font_id.clone(),
                    egui::Color32::WHITE,
                    text_width,
                );
                galley.size().y + v_pad * 2.0
            }).collect()
        });

        let total_h: f32 = heights.iter().sum::<f32>()
            + spacing * (self.toasts.len().saturating_sub(1)) as f32;

        let area_y = screen_rect.bottom() - margin - total_h;

        let area = egui::Area::new(egui::Id::new("toast_area"))
            .fixed_pos(egui::pos2(screen_rect.right() - toast_width - margin, area_y))
            .order(egui::Order::Foreground)
            .interactable(false);

        area.show(ctx, |ui| {
            ui.set_max_width(toast_width);

            for (toast, &h) in self.toasts.iter().zip(heights.iter()) {
                let alpha = (toast.remaining_fraction() * 255.0) as u8;

                let (bg_color, icon, text_color) = match toast.level {
                    ToastLevel::Info => (
                        egui::Color32::from_rgba_premultiplied(30, 70, 120, alpha),
                        "ℹ",
                        egui::Color32::from_rgba_premultiplied(200, 225, 255, alpha),
                    ),
                    ToastLevel::Warning => (
                        egui::Color32::from_rgba_premultiplied(110, 90, 20, alpha),
                        "⚠",
                        egui::Color32::from_rgba_premultiplied(255, 230, 140, alpha),
                    ),
                    ToastLevel::Error => (
                        egui::Color32::from_rgba_premultiplied(120, 35, 35, alpha),
                        "✖",
                        egui::Color32::from_rgba_premultiplied(255, 175, 175, alpha),
                    ),
                };

                // Allocate the measured height so the background rect matches
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(toast_width, h),
                    egui::Sense::hover(),
                );

                // Background
                ui.painter().rect_filled(rect, 6.0, bg_color);

                // Progress bar along bottom edge
                let bar_w = rect.width() * toast.remaining_fraction();
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), rect.bottom() - 2.0),
                    egui::vec2(bar_w, 2.0),
                );
                let bar_color = egui::Color32::from_rgba_premultiplied(
                    text_color.r(), text_color.g(), text_color.b(), (alpha as f32 * 0.7) as u8
                );
                ui.painter().rect_filled(bar_rect, 1.0, bar_color);

                // Icon — drawn at top-left of the padded area
                let icon_pos = egui::pos2(rect.left() + h_pad, rect.top() + v_pad);
                ui.painter().text(
                    icon_pos,
                    egui::Align2::LEFT_TOP,
                    icon,
                    font_id.clone(),
                    text_color,
                );

                // Message text — wrapped, starting after the icon column
                let text_start = egui::pos2(rect.left() + h_pad + 18.0, rect.top() + v_pad);
                let wrap_width = toast_width - h_pad - 18.0 - h_pad;
                let galley = ui.fonts(|f| f.layout(
                    toast.message.clone(),
                    font_id.clone(),
                    text_color,
                    wrap_width,
                ));
                ui.painter().galley(text_start, galley, text_color);

                ui.add_space(spacing);
            }
        });

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
