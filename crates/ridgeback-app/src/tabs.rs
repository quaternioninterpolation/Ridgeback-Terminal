use anyhow::Result;
use ridgeback_config::{Profile, ShaderEffectConfig, TypingParticlesConfig};
use ridgeback_core::Terminal;
use ridgeback_plugin::ParticleEvent;
use crate::find_overlay::FindOverlay;
use crate::command_query::CommandQueryOverlay;

// ── Terminal text selection (mouse) ───────────────────────────────────────────

/// A selection range across the terminal grid (rows are absolute: scrollback + visible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSelection {
    /// Anchor point (where mouse-down happened).
    pub anchor_row: usize,
    pub anchor_col: usize,
    /// Head point (where mouse currently is / was released).
    pub head_row: usize,
    pub head_col: usize,
}

impl TerminalSelection {
    /// Returns (start_row, start_col, end_row, end_col) in sorted order.
    pub fn sorted(&self) -> (usize, usize, usize, usize) {
        if (self.anchor_row, self.anchor_col) <= (self.head_row, self.head_col) {
            (self.anchor_row, self.anchor_col, self.head_row, self.head_col)
        } else {
            (self.head_row, self.head_col, self.anchor_row, self.anchor_col)
        }
    }

    /// Check if a given (row, col) is inside the selection.
    pub fn contains(&self, row: usize, col: usize) -> bool {
        let (sr, sc, er, ec) = self.sorted();
        if row < sr || row > er { return false; }
        if sr == er {
            // Single-row selection
            col >= sc && col < ec
        } else if row == sr {
            col >= sc
        } else if row == er {
            col < ec
        } else {
            true
        }
    }

    /// Extract selected text from scrollback lines + visible grid rows.
    pub fn selected_text(&self, scrollback_lines: &[String], visible_rows: &[Vec<ridgeback_core::cell::Cell>]) -> String {
        let (sr, sc, er, ec) = self.sorted();
        let mut result = String::new();
        let scrollback_count = scrollback_lines.len();

        for row in sr..=er {
            let line_text: String = if row < scrollback_count {
                scrollback_lines[row].clone()
            } else {
                let grid_row = row - scrollback_count;
                if grid_row < visible_rows.len() {
                    visible_rows[grid_row].iter().map(|c| if c.ch == '\0' { ' ' } else { c.ch }).collect()
                } else {
                    continue;
                }
            };

            let chars: Vec<char> = line_text.chars().collect();
            let start_col = if row == sr { sc } else { 0 };
            let end_col = if row == er { ec.min(chars.len()) } else { chars.len() };
            let start_col = start_col.min(chars.len());

            if start_col < end_col {
                let slice: String = chars[start_col..end_col].iter().collect();
                result.push_str(slice.trim_end());
            }
            if row < er {
                result.push('\n');
            }
        }
        result
    }
}

// ── Live particle state (plugin-driven) ───────────────────────────────────────

/// Runtime state for a single live particle.
#[derive(Clone)]
pub struct LiveParticle {
    pub event: ParticleEvent,
    /// Seconds elapsed since this particle was spawned.
    pub age: f32,
}

/// Per-tab particle simulation state. Fed by `ParticlePlugin::emit()`.
#[allow(dead_code)]
pub struct ParticleState {
    pub particles: Vec<LiveParticle>,
    pub accum: f32,
    pub rng: f32,
}

impl ParticleState {
    pub fn new() -> Self {
        Self { particles: Vec::with_capacity(256), accum: 0.0, rng: 0.0 }
    }

    /// Advance particle physics by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.rng = (self.rng * 1664525.0 + 1013904223.0) % 1_000_000.0;
        for lp in &mut self.particles {
            let p = &mut lp.event;
            lp.age += dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            if p.is_smoke {
                p.vy += -40.0 * dt;
                p.vx *= 1.0 - dt * 0.8;
                p.radius += dt * 8.0;
            } else {
                p.vy += 120.0 * dt; // embers fall
                p.heat = (p.heat - dt * 0.8).max(0.0);
            }
            p.life -= dt;
        }
        self.particles.retain(|lp| {
            lp.event.life > 0.0 && (!lp.event.is_smoke || lp.event.radius < 40.0)
        });
    }

    /// Spawn particles from a plugin emit result.
    pub fn spawn(&mut self, events: Vec<ParticleEvent>) {
        for e in events {
            self.particles.push(LiveParticle { event: e, age: 0.0 });
        }
    }
}

use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for unique terminal instance IDs.
static NEXT_TERMINAL_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a unique terminal ID (never reused).
fn next_terminal_id() -> u64 {
    NEXT_TERMINAL_ID.fetch_add(1, Ordering::Relaxed)
}

// ── Tab state ─────────────────────────────────────────────────────────────────

/// State for a single terminal tab.
#[allow(dead_code)]
pub struct TabState {
    /// Unique ID for this terminal instance (used for egui widget ID scoping).
    pub terminal_id: u64,
    pub terminal: Terminal,
    pub find_overlay: FindOverlay,
    pub command_query: CommandQueryOverlay,
    pub scroll_offset: usize,
    /// Plugin-driven shader effect for this tab.
    pub shader_effect: ShaderEffectConfig,
    /// Plugin-driven typing particles for this tab.
    pub typing_particles: TypingParticlesConfig,
    /// Display title shown in the tab bar.
    pub tab_title: String,
    /// Mouse text selection state.
    pub terminal_selection: Option<TerminalSelection>,
    /// Whether a mouse drag selection is currently in progress.
    pub selection_in_progress: bool,
    /// Position where right-click context menu was triggered.
    pub context_menu_pos: Option<egui::Pos2>,
    /// Live particle simulation state (fed by particle plugins).
    pub particles: ParticleState,
    /// Open animation progress 0.0 → 1.0.
    pub open_anim: f32,
    /// Close animation progress 0.0 → 1.0.
    pub close_anim: f32,
    /// True once close has been requested.
    pub closing: bool,
    /// Draw a dark shadow behind text for readability over bright backgrounds.
    pub text_shadow_enabled: bool,
    /// Shadow darkness 0.0–1.0.
    pub text_shadow_alpha: f32,
    /// Default text foreground colour ("#RRGGBB").
    pub text_foreground: String,
}

// ── Tab Group ─────────────────────────────────────────────────────────────────

/// A group of tabs sharing a single pane in the split layout.
/// Each group has its own tab bar header and active tab selection —
/// like an editor group in VS Code.
#[allow(dead_code)]
pub struct TabGroup {
    /// Unique stable identifier (never renumbered).
    pub id: usize,
    tabs: Vec<TabState>,
    active: usize,
}

#[allow(dead_code)]
impl TabGroup {
    pub fn new(id: usize) -> Self {
        Self { id, tabs: Vec::new(), active: 0 }
    }

    pub fn open_tab(&mut self, profile_name: &str, profile: &Profile) -> Result<()> {
        let terminal = Terminal::spawn(profile_name, profile, 24, 80)?;

        let base_name = &profile.name;
        let existing = self.tabs.iter()
            .filter(|t| t.terminal.profile_name == profile_name)
            .count();
        let tab_title = if existing == 0 {
            base_name.clone()
        } else {
            format!("{} {}", base_name, existing + 1)
        };

        let tab = TabState {
            terminal_id: next_terminal_id(),
            terminal,
            find_overlay: FindOverlay::new(),
            command_query: CommandQueryOverlay::new(),
            scroll_offset: 0,
            shader_effect: profile.shader_effect.clone(),
            typing_particles: profile.typing_particles.clone(),
            tab_title,
            terminal_selection: None,
            selection_in_progress: false,
            context_menu_pos: None,
            particles: ParticleState::new(),
            open_anim: 0.0,
            close_anim: 0.0,
            closing: false,
            text_shadow_enabled: profile.text_shadow_enabled,
            text_shadow_alpha: profile.text_shadow_alpha,
            text_foreground: profile.text_foreground.clone(),
        };
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        Ok(())
    }

    /// Remove a tab immediately (used after close animation finishes).
    pub fn remove_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active >= self.tabs.len() && !self.tabs.is_empty() {
                self.active = self.tabs.len() - 1;
            }
        }
    }

    /// Begin the close animation for a tab.
    pub fn close_tab(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.closing = true;
        }
    }

    pub fn close_active_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.close_tab(self.active);
        }
    }

    /// Swap two tabs by index (drag-to-reorder).
    pub fn swap_tabs(&mut self, a: usize, b: usize) {
        let len = self.tabs.len();
        if a < len && b < len && a != b {
            self.tabs.swap(a, b);
            if self.active == a { self.active = b; }
            else if self.active == b { self.active = a; }
        }
    }

    /// Remove and return a tab by index (for moving between groups).
    pub fn take_tab(&mut self, index: usize) -> Option<TabState> {
        if index >= self.tabs.len() { return None; }
        let tab = self.tabs.remove(index);
        if self.active >= self.tabs.len() && !self.tabs.is_empty() {
            self.active = self.tabs.len() - 1;
        }
        Some(tab)
    }

    /// Insert a tab at a specific index.
    pub fn insert_tab(&mut self, index: usize, tab: TabState) {
        let idx = index.min(self.tabs.len());
        self.tabs.insert(idx, tab);
        self.active = idx;
    }

    /// Advance open/close animations by `dt` seconds.
    /// Returns true if any animation is still running.
    pub fn tick_animations(&mut self, dt: f32) -> bool {
        let speed = 8.0_f32;
        let mut animating = false;
        let mut to_remove: Vec<usize> = Vec::new();

        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if !tab.closing && tab.open_anim < 1.0 {
                tab.open_anim = (tab.open_anim + dt * speed).min(1.0);
                animating = true;
            }
            if tab.closing {
                tab.close_anim = (tab.close_anim + dt * speed).min(1.0);
                animating = true;
                if tab.close_anim >= 1.0 {
                    to_remove.push(i);
                }
            }
        }
        for i in to_remove.into_iter().rev() {
            self.tabs.remove(i);
            if self.active >= self.tabs.len() && !self.tabs.is_empty() {
                self.active = self.tabs.len() - 1;
            }
        }
        animating
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = if self.active == 0 {
                self.tabs.len() - 1
            } else {
                self.active - 1
            };
        }
    }

    pub fn count(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn any_active(&self) -> bool {
        !self.tabs.is_empty()
    }

    pub fn tabs_ref(&self) -> impl Iterator<Item = &TabState> {
        self.tabs.iter()
    }

    pub fn active_tab(&self) -> Option<&TabState> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tabs.get_mut(self.active)
    }

    pub fn tab(&self, index: usize) -> Option<&TabState> {
        self.tabs.get(index)
    }

    pub fn tab_mut(&mut self, index: usize) -> Option<&mut TabState> {
        self.tabs.get_mut(index)
    }

    pub fn tabs_mut(&mut self) -> &mut [TabState] {
        &mut self.tabs
    }

    /// Close all tabs except the one at `keep_index`.
    pub fn close_other_tabs(&mut self, keep_index: usize) {
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i != keep_index && !tab.closing {
                tab.closing = true;
            }
        }
    }

    /// Close all tabs to the right of `index`.
    pub fn close_tabs_to_right(&mut self, index: usize) {
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i > index && !tab.closing {
                tab.closing = true;
            }
        }
    }

    /// Close all tabs in this group (start close animation).
    pub fn close_all_tabs(&mut self) {
        for tab in &mut self.tabs {
            if !tab.closing {
                tab.closing = true;
            }
        }
    }
}

// ── Tab Manager ───────────────────────────────────────────────────────────────

/// Manages all tab groups. Each group is a pane in the split layout.
#[allow(dead_code)]
pub struct TabManager {
    groups: Vec<TabGroup>,
    focused_group_id: usize,
    next_group_id: usize,
}

#[allow(dead_code)]
impl TabManager {
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            focused_group_id: 0,
            next_group_id: 0,
        }
    }

    /// Create a new empty group and return its ID.
    pub fn new_group(&mut self) -> usize {
        let id = self.next_group_id;
        self.next_group_id += 1;
        self.groups.push(TabGroup::new(id));
        id
    }

    /// Create a new group with a tab already opened and return the group ID.
    pub fn new_group_with_tab(&mut self, profile_name: &str, profile: &Profile) -> Result<usize> {
        let id = self.new_group();
        self.group_by_id_mut(id).unwrap().open_tab(profile_name, profile)?;
        Ok(id)
    }

    pub fn focused_group_id(&self) -> usize {
        self.focused_group_id
    }

    pub fn set_focused_group(&mut self, id: usize) {
        if self.groups.iter().any(|g| g.id == id) {
            self.focused_group_id = id;
        }
    }

    pub fn focused_group(&self) -> Option<&TabGroup> {
        self.groups.iter().find(|g| g.id == self.focused_group_id)
    }

    pub fn focused_group_mut(&mut self) -> Option<&mut TabGroup> {
        let id = self.focused_group_id;
        self.groups.iter_mut().find(|g| g.id == id)
    }

    pub fn group_by_id(&self, id: usize) -> Option<&TabGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn group_by_id_mut(&mut self, id: usize) -> Option<&mut TabGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// Remove a group by ID. Returns true if removed.
    pub fn remove_group(&mut self, id: usize) -> bool {
        let pos = self.groups.iter().position(|g| g.id == id);
        if let Some(idx) = pos {
            self.groups.remove(idx);
            if self.focused_group_id == id {
                self.focused_group_id = self.groups.first().map(|g| g.id).unwrap_or(0);
            }
            true
        } else {
            false
        }
    }

    /// Ordered list of group IDs.
    pub fn group_ids_ordered(&self) -> Vec<usize> {
        self.groups.iter().map(|g| g.id).collect()
    }

    /// Number of groups.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// True if any group has any tab.
    pub fn any_active(&self) -> bool {
        self.groups.iter().any(|g| !g.is_empty())
    }

    /// Flat mutable iterator over ALL tabs in ALL groups.
    pub fn all_tabs_mut(&mut self) -> impl Iterator<Item = &mut TabState> {
        self.groups.iter_mut().flat_map(|g| g.tabs.iter_mut())
    }

    /// Flat immutable iterator over all tabs.
    pub fn all_tabs_ref(&self) -> impl Iterator<Item = &TabState> {
        self.groups.iter().flat_map(|g| g.tabs.iter())
    }

    /// Tick animations in all groups.
    pub fn tick_animations(&mut self, dt: f32) -> bool {
        let mut any = false;
        for g in &mut self.groups {
            if g.tick_animations(dt) { any = true; }
        }
        any
    }

    /// Open a tab in the focused group.
    pub fn open_tab_in_focused(&mut self, profile_name: &str, profile: &Profile) -> Result<()> {
        if let Some(g) = self.focused_group_mut() {
            g.open_tab(profile_name, profile)?;
        }
        Ok(())
    }

    /// Active tab of the focused group.
    pub fn active_tab(&self) -> Option<&TabState> {
        self.focused_group().and_then(|g| g.active_tab())
    }

    /// Active tab of the focused group (mutable).
    pub fn active_tab_mut(&mut self) -> Option<&mut TabState> {
        self.focused_group_mut().and_then(|g| g.active_tab_mut())
    }

    /// Move a tab from one group to another.
    /// Returns `true` if the source group is now empty.
    pub fn move_tab(&mut self, from_group_id: usize, tab_idx: usize, to_group_id: usize) -> bool {
        if from_group_id == to_group_id { return false; }
        let tab = {
            let src = self.groups.iter_mut().find(|g| g.id == from_group_id);
            match src {
                Some(g) => g.take_tab(tab_idx),
                None => None,
            }
        };
        if let Some(tab) = tab {
            let dst = self.groups.iter_mut().find(|g| g.id == to_group_id);
            if let Some(g) = dst {
                let idx = g.count();
                g.insert_tab(idx, tab);
            }
        }
        self.groups.iter().find(|g| g.id == from_group_id).map_or(true, |g| g.is_empty())
    }

    /// Get the next group ID relative to the current focused group.
    pub fn next_group_id_from_focused(&self) -> Option<usize> {
        let ids = self.group_ids_ordered();
        if ids.len() <= 1 { return None; }
        let pos = ids.iter().position(|&id| id == self.focused_group_id)?;
        Some(ids[(pos + 1) % ids.len()])
    }

    /// Get the previous group ID relative to the current focused group.
    pub fn prev_group_id_from_focused(&self) -> Option<usize> {
        let ids = self.group_ids_ordered();
        if ids.len() <= 1 { return None; }
        let pos = ids.iter().position(|&id| id == self.focused_group_id)?;
        Some(ids[if pos == 0 { ids.len() - 1 } else { pos - 1 }])
    }

    /// Check if a group with the given ID exists and is empty.
    pub fn is_group_empty(&self, id: usize) -> bool {
        self.groups.iter().find(|g| g.id == id).map_or(true, |g| g.is_empty())
    }

    /// Iterate groups immutably.
    pub fn groups_ref(&self) -> impl Iterator<Item = &TabGroup> {
        self.groups.iter()
    }

    /// Iterate groups mutably.
    pub fn groups_mut(&mut self) -> impl Iterator<Item = &mut TabGroup> {
        self.groups.iter_mut()
    }
}
