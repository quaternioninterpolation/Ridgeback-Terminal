#![allow(dead_code)]
//! Tab drag-to-reorder and cross-group movement.
//!
//! `TabDragState` tracks whether the user is currently dragging a tab in a
//! group's tab bar. Horizontal movement reorders within the group.
//! Dragging to a different group's area moves the tab between groups.
//! Dragging to an edge creates a new group via split.

use egui::{Pos2, Rect};

/// Identifies which tab is being dragged and from where.
#[derive(Debug, Clone)]
pub struct DragSource {
    pub group_id: usize,
    pub tab_idx: usize,
}

/// Per-window tab drag state.
pub struct TabDragState {
    /// The tab currently being dragged, if any.
    pub dragging: Option<DragSource>,
    /// Pixel position where the drag started (screen coords).
    pub drag_origin: Pos2,
    /// Current drag position (screen coords).
    pub drag_current: Pos2,
    /// True once the cursor has moved past the threshold.
    pub threshold_passed: bool,
}

impl Default for TabDragState {
    fn default() -> Self {
        Self {
            dragging: None,
            drag_origin: Pos2::ZERO,
            drag_current: Pos2::ZERO,
            threshold_passed: false,
        }
    }
}

impl TabDragState {
    pub const DRAG_THRESHOLD: f32 = 6.0;

    /// Returns true if there is an active drag in progress.
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Start tracking a drag for a tab from a specific group.
    pub fn start(&mut self, group_id: usize, tab_idx: usize, origin: Pos2) {
        self.dragging = Some(DragSource { group_id, tab_idx });
        self.drag_origin = origin;
        self.drag_current = origin;
        self.threshold_passed = false;
    }

    /// Update the current drag position.
    pub fn update(&mut self, current: Pos2) {
        self.drag_current = current;
        let delta = current - self.drag_origin;
        if !self.threshold_passed && delta.length() >= Self::DRAG_THRESHOLD {
            self.threshold_passed = true;
        }
    }

    /// Check whether a drag should reorder tabs within the same group.
    /// Returns `Some((from, to))` if a swap should occur.
    pub fn check_reorder(&self, tab_rects: &[(usize, Rect)]) -> Option<(usize, usize)> {
        let source = self.dragging.as_ref()?;
        let dragging = source.tab_idx;
        if !self.threshold_passed {
            return None;
        }

        let cx = self.drag_current.x;
        for &(idx, rect) in tab_rects {
            if idx == dragging { continue; }
            let mid = rect.center().x;
            let my_rect = tab_rects.iter().find(|&&(i, _)| i == dragging)?.1;
            let my_mid = my_rect.center().x;

            if my_mid < mid && cx > mid {
                return Some((dragging, idx));
            }
            if my_mid > mid && cx < mid {
                return Some((dragging, idx));
            }
        }
        None
    }

    /// End the drag, clearing all state.
    pub fn end(&mut self) {
        self.dragging = None;
        self.threshold_passed = false;
    }

    /// Draw the floating ghost tab under the cursor during drag.
    pub fn draw_ghost(&self, ui: &mut egui::Ui, title: &str) {
        if !self.threshold_passed { return; }
        let pos = self.drag_current;
        let ghost_rect = egui::Rect::from_center_size(pos, egui::vec2(120.0, 28.0));
        ui.painter().rect_filled(
            ghost_rect, 4.0,
            egui::Color32::from_rgba_unmultiplied(55, 55, 55, 200),
        );
        ui.painter().text(
            ghost_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }
}
