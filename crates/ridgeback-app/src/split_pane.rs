//! Split-pane layout engine with tab-group leaves.
//!
//! A `SplitPane` is a recursive tree:
//!   - `Single(group_id)` — a leaf that renders one tab group (with its own tab bar)
//!   - `Horizontal(left, right, ratio)` — side-by-side, `ratio` is left fraction
//!   - `Vertical(top, bottom, ratio)` — top/bottom, `ratio` is top fraction
//!
//! Each leaf references a `TabGroup` by its stable ID (never renumbered).

use egui::{Rect, Ui};

/// Minimum pane size to prevent collapsing to nothing.
pub const MIN_PANE_SIZE: f32 = 120.0;
/// Width/height of the draggable splitter handle.
pub const SPLITTER_SIZE: f32 = 5.0;
/// Height of the per-group tab bar header.
pub const GROUP_TAB_BAR_HEIGHT: f32 = 28.0;

// ── Drop zones ───────────────────────────────────────────────────────────────

/// Which edge/region a drop target represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropSide {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// A drop target zone computed from the split tree layout.
#[derive(Debug, Clone)]
pub struct DropZone {
    pub group_id: usize,
    pub side: DropSide,
    pub rect: Rect,
}

// ── Tree node ────────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum SplitPane {
    /// A tab group (ID into `TabManager::groups`).
    Single(usize),
    /// Two panes side-by-side. `ratio` = fraction of width given to `left`.
    Horizontal {
        left: Box<SplitPane>,
        right: Box<SplitPane>,
        ratio: f32,
    },
    /// Two panes stacked. `ratio` = fraction of height given to `top`.
    Vertical {
        top: Box<SplitPane>,
        bottom: Box<SplitPane>,
        ratio: f32,
    },
}

#[allow(dead_code)]
impl SplitPane {
    pub fn new_single(group_id: usize) -> Self {
        SplitPane::Single(group_id)
    }

    /// Split a leaf (matching `target_group`) horizontally, inserting `new_group`
    /// on the right with a 50/50 ratio.
    pub fn split_horizontal(&mut self, target_group: usize, new_group: usize) -> bool {
        match self {
            SplitPane::Single(id) if *id == target_group => {
                let old = SplitPane::Single(*id);
                *self = SplitPane::Horizontal {
                    left: Box::new(old),
                    right: Box::new(SplitPane::Single(new_group)),
                    ratio: 0.5,
                };
                true
            }
            SplitPane::Horizontal { left, right, .. } => {
                left.split_horizontal(target_group, new_group)
                    || right.split_horizontal(target_group, new_group)
            }
            SplitPane::Vertical { top, bottom, .. } => {
                top.split_horizontal(target_group, new_group)
                    || bottom.split_horizontal(target_group, new_group)
            }
            _ => false,
        }
    }

    /// Split a leaf vertically, inserting `new_group` below.
    pub fn split_vertical(&mut self, target_group: usize, new_group: usize) -> bool {
        match self {
            SplitPane::Single(id) if *id == target_group => {
                let old = SplitPane::Single(*id);
                *self = SplitPane::Vertical {
                    top: Box::new(old),
                    bottom: Box::new(SplitPane::Single(new_group)),
                    ratio: 0.5,
                };
                true
            }
            SplitPane::Horizontal { left, right, .. } => {
                left.split_vertical(target_group, new_group)
                    || right.split_vertical(target_group, new_group)
            }
            SplitPane::Vertical { top, bottom, .. } => {
                top.split_vertical(target_group, new_group)
                    || bottom.split_vertical(target_group, new_group)
            }
            _ => false,
        }
    }

    /// Remove a group leaf, replacing this subtree with the sibling.
    pub fn remove_group(&mut self, target_group: usize) -> RemoveResult {
        match self {
            SplitPane::Single(id) if *id == target_group => RemoveResult::RemoveSelf,
            SplitPane::Single(_) => RemoveResult::NoChange,
            SplitPane::Horizontal { left, right, .. } => {
                match left.remove_group(target_group) {
                    RemoveResult::RemoveSelf => { *self = *right.clone(); RemoveResult::Modified }
                    RemoveResult::Modified => RemoveResult::Modified,
                    RemoveResult::NoChange => match right.remove_group(target_group) {
                        RemoveResult::RemoveSelf => { *self = *left.clone(); RemoveResult::Modified }
                        r => r,
                    }
                }
            }
            SplitPane::Vertical { top, bottom, .. } => {
                match top.remove_group(target_group) {
                    RemoveResult::RemoveSelf => { *self = *bottom.clone(); RemoveResult::Modified }
                    RemoveResult::Modified => RemoveResult::Modified,
                    RemoveResult::NoChange => match bottom.remove_group(target_group) {
                        RemoveResult::RemoveSelf => { *self = *top.clone(); RemoveResult::Modified }
                        r => r,
                    }
                }
            }
        }
    }

    /// Collect all leaf group IDs.
    pub fn leaf_group_ids(&self) -> Vec<usize> {
        match self {
            SplitPane::Single(id) => vec![*id],
            SplitPane::Horizontal { left, right, .. } => {
                let mut v = left.leaf_group_ids();
                v.extend(right.leaf_group_ids());
                v
            }
            SplitPane::Vertical { top, bottom, .. } => {
                let mut v = top.leaf_group_ids();
                v.extend(bottom.leaf_group_ids());
                v
            }
        }
    }

    /// Hit-test: which leaf group contains the given point?
    pub fn hit_test(&self, rect: Rect, pos: egui::Pos2) -> Option<usize> {
        match self {
            SplitPane::Single(id) => {
                if rect.contains(pos) { Some(*id) } else { None }
            }
            SplitPane::Horizontal { left, right, ratio } => {
                let lw = (rect.width() * *ratio).max(MIN_PANE_SIZE)
                    .min(rect.width() - MIN_PANE_SIZE - SPLITTER_SIZE);
                let left_rect = Rect::from_min_size(rect.min, egui::vec2(lw, rect.height()));
                let right_rect = Rect::from_min_size(
                    egui::pos2(rect.min.x + lw + SPLITTER_SIZE, rect.min.y),
                    egui::vec2(rect.width() - lw - SPLITTER_SIZE, rect.height()),
                );
                left.hit_test(left_rect, pos).or_else(|| right.hit_test(right_rect, pos))
            }
            SplitPane::Vertical { top, bottom, ratio } => {
                let th = (rect.height() * *ratio).max(MIN_PANE_SIZE)
                    .min(rect.height() - MIN_PANE_SIZE - SPLITTER_SIZE);
                let top_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), th));
                let bot_rect = Rect::from_min_size(
                    egui::pos2(rect.min.x, rect.min.y + th + SPLITTER_SIZE),
                    egui::vec2(rect.width(), rect.height() - th - SPLITTER_SIZE),
                );
                top.hit_test(top_rect, pos).or_else(|| bottom.hit_test(bot_rect, pos))
            }
        }
    }

    /// Compute drop zones for drag-to-split.
    pub fn compute_drop_zones(&self, rect: Rect) -> Vec<DropZone> {
        let mut zones = Vec::new();
        self.collect_drop_zones(rect, &mut zones);
        zones
    }

    fn collect_drop_zones(&self, rect: Rect, zones: &mut Vec<DropZone>) {
        match self {
            SplitPane::Single(id) => {
                let w = rect.width();
                let h = rect.height();
                let edge_frac = 0.25;
                // Left
                zones.push(DropZone {
                    group_id: *id,
                    side: DropSide::Left,
                    rect: Rect::from_min_size(rect.min, egui::vec2(w * edge_frac, h)),
                });
                // Right
                zones.push(DropZone {
                    group_id: *id,
                    side: DropSide::Right,
                    rect: Rect::from_min_size(
                        egui::pos2(rect.min.x + w * (1.0 - edge_frac), rect.min.y),
                        egui::vec2(w * edge_frac, h),
                    ),
                });
                // Top
                zones.push(DropZone {
                    group_id: *id,
                    side: DropSide::Top,
                    rect: Rect::from_min_size(
                        egui::pos2(rect.min.x + w * edge_frac, rect.min.y),
                        egui::vec2(w * (1.0 - 2.0 * edge_frac), h * edge_frac),
                    ),
                });
                // Bottom
                zones.push(DropZone {
                    group_id: *id,
                    side: DropSide::Bottom,
                    rect: Rect::from_min_size(
                        egui::pos2(rect.min.x + w * edge_frac, rect.min.y + h * (1.0 - edge_frac)),
                        egui::vec2(w * (1.0 - 2.0 * edge_frac), h * edge_frac),
                    ),
                });
                // Center
                zones.push(DropZone {
                    group_id: *id,
                    side: DropSide::Center,
                    rect: Rect::from_min_size(
                        egui::pos2(rect.min.x + w * edge_frac, rect.min.y + h * edge_frac),
                        egui::vec2(w * (1.0 - 2.0 * edge_frac), h * (1.0 - 2.0 * edge_frac)),
                    ),
                });
            }
            SplitPane::Horizontal { left, right, ratio } => {
                let lw = (rect.width() * *ratio).max(MIN_PANE_SIZE)
                    .min(rect.width() - MIN_PANE_SIZE - SPLITTER_SIZE);
                let left_rect = Rect::from_min_size(rect.min, egui::vec2(lw, rect.height()));
                let right_rect = Rect::from_min_size(
                    egui::pos2(rect.min.x + lw + SPLITTER_SIZE, rect.min.y),
                    egui::vec2(rect.width() - lw - SPLITTER_SIZE, rect.height()),
                );
                left.collect_drop_zones(left_rect, zones);
                right.collect_drop_zones(right_rect, zones);
            }
            SplitPane::Vertical { top, bottom, ratio } => {
                let th = (rect.height() * *ratio).max(MIN_PANE_SIZE)
                    .min(rect.height() - MIN_PANE_SIZE - SPLITTER_SIZE);
                let top_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), th));
                let bot_rect = Rect::from_min_size(
                    egui::pos2(rect.min.x, rect.min.y + th + SPLITTER_SIZE),
                    egui::vec2(rect.width(), rect.height() - th - SPLITTER_SIZE),
                );
                top.collect_drop_zones(top_rect, zones);
                bottom.collect_drop_zones(bot_rect, zones);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum RemoveResult {
    RemoveSelf,
    Modified,
    NoChange,
}

// ── Manager ──────────────────────────────────────────────────────────────────

/// Manages the split pane tree for one window.
pub struct SplitPaneManager {
    root: SplitPane,
    /// Group ID of the currently focused pane.
    pub focused_group_id: usize,
}

#[allow(dead_code)]
impl SplitPaneManager {
    pub fn new(initial_group_id: usize) -> Self {
        Self {
            root: SplitPane::Single(initial_group_id),
            focused_group_id: initial_group_id,
        }
    }

    pub fn root(&self) -> &SplitPane { &self.root }

    /// Split the focused group horizontally; new_group appears on the right.
    pub fn split_horizontal(&mut self, new_group_id: usize) {
        self.root.split_horizontal(self.focused_group_id, new_group_id);
        self.focused_group_id = new_group_id;
    }

    /// Split the focused group vertically; new_group appears at the bottom.
    pub fn split_vertical(&mut self, new_group_id: usize) {
        self.root.split_vertical(self.focused_group_id, new_group_id);
        self.focused_group_id = new_group_id;
    }

    /// Split a specific group (not necessarily focused).
    pub fn split_group_horizontal(&mut self, target_group_id: usize, new_group_id: usize) {
        self.root.split_horizontal(target_group_id, new_group_id);
    }

    pub fn split_group_vertical(&mut self, target_group_id: usize, new_group_id: usize) {
        self.root.split_vertical(target_group_id, new_group_id);
    }

    /// Called after a group is removed from TabManager.
    pub fn on_group_removed(&mut self, group_id: usize) {
        self.root.remove_group(group_id);
        let leaves = self.root.leaf_group_ids();
        if self.focused_group_id == group_id {
            self.focused_group_id = leaves.first().copied().unwrap_or(0);
        }
    }

    pub fn set_focused_group(&mut self, group_id: usize) {
        self.focused_group_id = group_id;
    }

    /// Swap two group IDs in the tree.
    pub fn swap_groups(&mut self, a: usize, b: usize) {
        swap_in_tree(&mut self.root, a, b);
        if self.focused_group_id == a { self.focused_group_id = b; }
        else if self.focused_group_id == b { self.focused_group_id = a; }
    }

    /// Get all leaf group IDs.
    pub fn leaf_group_ids(&self) -> Vec<usize> {
        self.root.leaf_group_ids()
    }

    /// Compute drop zones for drag-to-split.
    pub fn compute_drop_zones(&self, available: Rect) -> Vec<DropZone> {
        self.root.compute_drop_zones(available)
    }

    /// Hit-test: which leaf group contains the given point?
    pub fn hit_test(&self, available: Rect, pos: egui::Pos2) -> Option<usize> {
        self.root.hit_test(available, pos)
    }

    /// Render the split tree, calling `render_group` for each leaf.
    ///
    /// `render_group(ui, header_rect, body_rect, group_id, is_focused)` → returns true if the pane was clicked (should gain focus)
    pub fn show<F>(&mut self, ui: &mut Ui, available: Rect, render_group: &mut F)
    where
        F: FnMut(&mut Ui, Rect, Rect, usize, bool) -> bool,
    {
        let focused = self.focused_group_id;
        let new_focus = show_node(ui, available, &mut self.root, focused, render_group);
        if let Some(f) = new_focus {
            self.focused_group_id = f;
        }
    }
}

// ── Recursive rendering ───────────────────────────────────────────────────────

/// Returns the newly-focused group ID if the user clicked a pane.
fn show_node<F>(
    ui: &mut Ui,
    rect: Rect,
    node: &mut SplitPane,
    focused: usize,
    render_group: &mut F,
) -> Option<usize>
where
    F: FnMut(&mut Ui, Rect, Rect, usize, bool) -> bool,
{
    match node {
        SplitPane::Single(group_id) => {
            let is_focused = *group_id == focused;

            // Split rect into header (tab bar) and body (terminal)
            let header_rect = Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width(), GROUP_TAB_BAR_HEIGHT),
            );
            let body_rect = Rect::from_min_size(
                egui::pos2(rect.min.x, rect.min.y + GROUP_TAB_BAR_HEIGHT),
                egui::vec2(rect.width(), (rect.height() - GROUP_TAB_BAR_HEIGHT).max(0.0)),
            );

            let clicked = render_group(ui, header_rect, body_rect, *group_id, is_focused);

            if clicked && !is_focused {
                return Some(*group_id);
            }
            None
        }
        SplitPane::Horizontal { left, right, ratio } => {
            let lw = (rect.width() * *ratio).max(MIN_PANE_SIZE)
                .min(rect.width() - MIN_PANE_SIZE - SPLITTER_SIZE);
            let left_rect = Rect::from_min_size(rect.min, egui::vec2(lw, rect.height()));
            let splitter_rect = Rect::from_min_size(
                egui::pos2(rect.min.x + lw, rect.min.y),
                egui::vec2(SPLITTER_SIZE, rect.height()),
            );
            let right_rect = Rect::from_min_size(
                egui::pos2(rect.min.x + lw + SPLITTER_SIZE, rect.min.y),
                egui::vec2(rect.width() - lw - SPLITTER_SIZE, rect.height()),
            );

            draw_splitter(ui, splitter_rect, false);

            let splitter_resp = ui.interact(
                splitter_rect,
                egui::Id::new(("hsplitter", rect.min.x as i32, rect.min.y as i32)),
                egui::Sense::drag(),
            );
            if splitter_resp.dragged() {
                let new_lw = (lw + splitter_resp.drag_delta().x)
                    .max(MIN_PANE_SIZE).min(rect.width() - MIN_PANE_SIZE - SPLITTER_SIZE);
                *ratio = new_lw / rect.width();
            }

            let f1 = show_node(ui, left_rect, left, focused, render_group);
            let f2 = show_node(ui, right_rect, right, focused, render_group);
            f1.or(f2)
        }
        SplitPane::Vertical { top, bottom, ratio } => {
            let th = (rect.height() * *ratio).max(MIN_PANE_SIZE)
                .min(rect.height() - MIN_PANE_SIZE - SPLITTER_SIZE);
            let top_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), th));
            let splitter_rect = Rect::from_min_size(
                egui::pos2(rect.min.x, rect.min.y + th),
                egui::vec2(rect.width(), SPLITTER_SIZE),
            );
            let bot_rect = Rect::from_min_size(
                egui::pos2(rect.min.x, rect.min.y + th + SPLITTER_SIZE),
                egui::vec2(rect.width(), rect.height() - th - SPLITTER_SIZE),
            );

            draw_splitter(ui, splitter_rect, true);

            let splitter_resp = ui.interact(
                splitter_rect,
                egui::Id::new(("vsplitter", rect.min.x as i32, rect.min.y as i32)),
                egui::Sense::drag(),
            );
            if splitter_resp.dragged() {
                let new_th = (th + splitter_resp.drag_delta().y)
                    .max(MIN_PANE_SIZE).min(rect.height() - MIN_PANE_SIZE - SPLITTER_SIZE);
                *ratio = new_th / rect.height();
            }

            let f1 = show_node(ui, top_rect, top, focused, render_group);
            let f2 = show_node(ui, bot_rect, bottom, focused, render_group);
            f1.or(f2)
        }
    }
}

fn draw_splitter(ui: &mut Ui, rect: Rect, _horizontal: bool) {
    let base = egui::Color32::from_gray(40);
    let hover_col = egui::Color32::from_gray(80);
    let resp = ui.interact(rect, egui::Id::new(("split_visual", rect.min.x as i32, rect.min.y as i32)), egui::Sense::hover());
    let col = if resp.hovered() { hover_col } else { base };
    ui.painter().rect_filled(rect, 0.0, col);

    // Grip dots
    let dots = 3;
    let step = rect.height().min(rect.width()) / (dots + 1) as f32;
    for i in 1..=dots {
        let pos = if rect.width() > rect.height() {
            egui::pos2(rect.min.x + step * i as f32, rect.center().y)
        } else {
            egui::pos2(rect.center().x, rect.min.y + step * i as f32)
        };
        ui.painter().circle_filled(pos, 2.0, egui::Color32::from_gray(100));
    }
}

fn swap_in_tree(node: &mut SplitPane, a: usize, b: usize) {
    match node {
        SplitPane::Single(id) => {
            if *id == a { *id = b; }
            else if *id == b { *id = a; }
        }
        SplitPane::Horizontal { left, right, .. } => {
            swap_in_tree(left, a, b);
            swap_in_tree(right, a, b);
        }
        SplitPane::Vertical { top, bottom, .. } => {
            swap_in_tree(top, a, b);
            swap_in_tree(bottom, a, b);
        }
    }
}

