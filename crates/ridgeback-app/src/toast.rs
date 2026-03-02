#![allow(dead_code)]
/// Toast notification system for transient, non-blocking messages.
///
/// Supports Info, Warning, Error, and Progress severity levels with auto-dismiss.

use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{Duration, Instant};

/// Severity level for a toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
    /// Sticky progress toast — stays until explicitly dismissed.
    Progress,
}

/// A single toast notification.
#[derive(Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
    pub duration: Duration,
    /// For Progress toasts: shared counter of bytes downloaded.
    pub progress_bytes: Option<Arc<AtomicU64>>,
    /// For Progress toasts: shared counter of total bytes.
    pub progress_total: Option<Arc<AtomicU64>>,
    /// For Progress toasts: flag to dismiss when done.
    pub progress_done: Option<Arc<AtomicBool>>,
    /// Unique ID for toast management.
    pub id: u64,
}

impl Toast {
    /// Counter for unique IDs.
    fn next_id() -> u64 {
        use std::sync::atomic::AtomicU64;
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Info,
            created: Instant::now(),
            duration: Duration::from_secs(3),
            progress_bytes: None,
            progress_total: None,
            progress_done: None,
            id: Self::next_id(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Warning,
            created: Instant::now(),
            duration: Duration::from_secs(5),
            progress_bytes: None,
            progress_total: None,
            progress_done: None,
            id: Self::next_id(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Error,
            created: Instant::now(),
            duration: Duration::from_secs(6),
            progress_bytes: None,
            progress_total: None,
            progress_done: None,
            id: Self::next_id(),
        }
    }

    /// Create a sticky progress toast that tracks download progress.
    /// It will stay visible until `progress_done` is set to true.
    pub fn progress(
        message: impl Into<String>,
        downloaded: Arc<AtomicU64>,
        total: Arc<AtomicU64>,
        done: Arc<AtomicBool>,
    ) -> Self {
        Self {
            message: message.into(),
            level: ToastLevel::Progress,
            created: Instant::now(),
            duration: Duration::from_secs(3600), // effectively infinite
            progress_bytes: Some(downloaded),
            progress_total: Some(total),
            progress_done: Some(done),
            id: Self::next_id(),
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
#[derive(Default)]
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
        // Remove expired toasts. Progress toasts are removed only when done.
        self.toasts.retain(|t| {
            if t.level == ToastLevel::Progress {
                // Keep until the done flag is set
                !t.progress_done.as_ref().map_or(false, |d| d.load(Ordering::Relaxed))
            } else {
                !t.is_expired()
            }
        });
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
                let alpha = if toast.level == ToastLevel::Progress {
                    230u8 // Progress toasts stay opaque
                } else {
                    (toast.remaining_fraction() * 255.0) as u8
                };

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
                    ToastLevel::Progress => (
                        egui::Color32::from_rgba_premultiplied(25, 80, 60, alpha),
                        "⬇",
                        egui::Color32::from_rgba_premultiplied(180, 240, 210, alpha),
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
                let bar_fraction = if toast.level == ToastLevel::Progress {
                    // Show download progress
                    let dl = toast.progress_bytes.as_ref().map_or(0, |b| b.load(Ordering::Relaxed));
                    let total = toast.progress_total.as_ref().map_or(1, |t| t.load(Ordering::Relaxed));
                    if total > 0 { dl as f32 / total as f32 } else { 0.0 }
                } else {
                    toast.remaining_fraction()
                };
                let bar_w = rect.width() * bar_fraction;
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

                // For progress toasts, append percentage info
                let display_message = if toast.level == ToastLevel::Progress {
                    let dl = toast.progress_bytes.as_ref().map_or(0, |b| b.load(Ordering::Relaxed));
                    let total = toast.progress_total.as_ref().map_or(0, |t| t.load(Ordering::Relaxed));
                    let pct = if total > 0 { (dl as f64 / total as f64 * 100.0) as u32 } else { 0 };
                    format!("{} — {}% ({}/{})",
                        toast.message,
                        pct,
                        format_bytes(dl),
                        format_bytes(total),
                    )
                } else {
                    toast.message.clone()
                };

                let galley = ui.fonts(|f| f.layout(
                    display_message,
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

/// Format byte count in human-readable form.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

