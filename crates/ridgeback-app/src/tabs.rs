use anyhow::Result;
use ridgeback_config::{Profile, ShaderEffect, ShaderParams};
use ridgeback_core::Terminal;
use crate::find_overlay::FindOverlay;
use crate::command_query::CommandQueryOverlay;

// ── Fire / smoke particle system ─────────────────────────────────────────────

/// A single fire or smoke particle spawned from the cursor on keypress.
#[derive(Clone)]
pub struct FireParticle {
    /// World position (pixels relative to terminal rect origin).
    pub x: f32,
    pub y: f32,
    /// Velocity.
    pub vx: f32,
    pub vy: f32,
    /// Remaining lifetime in seconds (0 = dead).
    pub life: f32,
    /// Initial lifetime, used to compute alpha fade.
    pub max_life: f32,
    /// Radius of the particle.
    pub radius: f32,
    /// True = smoke, false = fire ember.
    pub is_smoke: bool,
    /// Heat value 0..1 – drives colour for fire, opacity for smoke.
    pub heat: f32,
}

/// Doom-style 1-D fire simulation row used for the bottom-edge base flame.
/// One cell per pixel column (downsampled to every 4 px).
pub struct FireSim {
    /// Width in cells.
    pub w: usize,
    /// Heat buffer: `buf[row * w + col]`, row 0 = top, row H-1 = bottom (source).
    pub buf: Vec<f32>,
    /// Height in cells.
    pub h: usize,
}

impl FireSim {
    pub fn new(w: usize, h: usize) -> Self {
        let mut buf = vec![0.0f32; w * h];
        // Seed the bottom row with full heat.
        for x in 0..w {
            buf[(h - 1) * w + x] = 1.0;
        }
        Self { w, h, buf }
    }

    /// Step the simulation one tick.
    pub fn step(&mut self, decay: f32, spread: f32, rng_seed: f32) {
        let w = self.w;
        let h = self.h;
        // Re-seed the bottom row with some flicker.
        for x in 0..w {
            let flicker = ((rng_seed * 17.3 + x as f32 * 0.7).sin() * 0.5 + 0.5) * 0.2;
            self.buf[(h - 1) * w + x] = (0.85 + flicker).min(1.0);
        }
        // Propagate upward.
        for row in 1..h {
            for col in 0..w {
                let left  = if col > 0 { self.buf[row * w + col - 1] } else { self.buf[row * w + col] };
                let right = if col < w - 1 { self.buf[row * w + col + 1] } else { self.buf[row * w + col] };
                let below = self.buf[row * w + col];
                // Spread sideways and decay upward
                let spread_noise = ((rng_seed * 31.1 + col as f32 * 1.3 + row as f32 * 0.9).sin() * 0.5 + 0.5) * spread;
                let avg = (left * 0.2 + right * 0.2 + below * 0.6) + spread_noise * 0.05;
                self.buf[(row - 1) * w + col] = (avg - decay * 0.03).max(0.0);
            }
        }
    }
}

/// All particle + simulation state for the fire shader on one tab.
pub struct FireState {
    pub sim: FireSim,
    pub particles: Vec<FireParticle>,
    /// Pixel position of the last keypress emission (relative to term rect).
    pub last_emit_x: f32,
    pub last_emit_y: f32,
    /// Accumulated time delta for simulation stepping.
    pub accum: f32,
    /// Pseudo-random seed that advances each frame.
    pub rng: f32,
}

impl FireState {
    pub fn new() -> Self {
        Self {
            sim: FireSim::new(80, 20),
            particles: Vec::with_capacity(512),
            last_emit_x: 0.0,
            last_emit_y: 0.0,
            accum: 0.0,
            rng: 0.0,
        }
    }

    /// Spawn fire + smoke burst from (x, y) in terminal-rect-local pixels.
    pub fn emit_keypress(&mut self, x: f32, y: f32) {
        self.last_emit_x = x;
        self.last_emit_y = y;

        let rng = &mut self.rng;
        let mut next = || { *rng = (*rng * 1664525.0 + 1013904223.0) % 1_000_000.0; *rng / 1_000_000.0 };

        // Fire embers – 8 particles
        for _ in 0..8 {
            let angle = next() * std::f32::consts::TAU;
            let speed = 20.0 + next() * 60.0;
            self.particles.push(FireParticle {
                x, y,
                vx: angle.cos() * speed * 0.4,
                vy: -(30.0 + next() * 80.0),   // upward
                life: 0.4 + next() * 0.5,
                max_life: 0.9,
                radius: 2.0 + next() * 3.0,
                is_smoke: false,
                heat: 0.7 + next() * 0.3,
            });
        }
        // Smoke puffs – 5 particles
        for _ in 0..5 {
            self.particles.push(FireParticle {
                x: x + (next() - 0.5) * 10.0,
                y,
                vx: (next() - 0.5) * 15.0,
                vy: -(10.0 + next() * 25.0),
                life: 0.8 + next() * 0.8,
                max_life: 1.6,
                radius: 4.0 + next() * 6.0,
                is_smoke: true,
                heat: 0.0,
            });
        }
    }

    /// Advance the particle simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32, params: &ShaderParams) {
        self.rng = (self.rng * 1664525.0 + 1013904223.0) % 1_000_000.0;
        let rng_seed = self.rng / 1_000_000.0;

        self.accum += dt;
        let step_dt = 1.0 / 30.0;
        while self.accum >= step_dt {
            self.sim.step(params.fire_decay_rate, params.fire_spread, rng_seed + self.accum);
            self.accum -= step_dt;
        }

        // Update particles
        for p in &mut self.particles {
            p.x  += p.vx * dt;
            p.y  += p.vy * dt;
            // Gravity: fire rises, smoke drifts
            if p.is_smoke {
                p.vy -= 5.0 * dt;   // gentle upward drift
                p.vx *= 1.0 - dt * 0.8;
                p.radius += dt * 8.0; // smoke expands
            } else {
                p.vy += 20.0 * dt;  // embers arc then fall
                p.heat -= dt * 0.8;
            }
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0 && (!p.is_smoke || p.radius < 40.0));
    }
}

// ── Tab state ─────────────────────────────────────────────────────────────────

/// State for a single terminal tab.
pub struct TabState {
    pub terminal: Terminal,
    pub find_overlay: FindOverlay,
    pub command_query: CommandQueryOverlay,
    pub scroll_offset: usize,
    pub shader_effect: ShaderEffect,
    pub shader_params: ShaderParams,
    /// Display title shown in the tab bar.
    pub tab_title: String,
    /// Fire particle + simulation state (only used when shader_effect == Fire).
    pub fire: FireState,
    /// The current inline input line buffer (mirrors what the user is typing).
    pub inline_input: String,
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

/// Manages all open tabs.
pub struct TabManager {
    tabs: Vec<TabState>,
    active: usize,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn open_tab(&mut self, profile_name: &str, profile: &Profile) -> Result<()> {
        let terminal = Terminal::spawn(profile_name, profile, 24, 80)?;

        // Count how many tabs with this same display name are already open.
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
            terminal,
            find_overlay: FindOverlay::new(),
            command_query: CommandQueryOverlay::new(),
            scroll_offset: 0,
            shader_effect: profile.shader_effect,
            shader_params: profile.shader_params.clone(),
            tab_title,
            fire: FireState::new(),
            inline_input: String::new(),
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

    // ...existing code...


    /// Immediately remove a tab (used after close animation finishes).
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

    /// Advance open/close animations by `dt` seconds.
    /// Returns true if any animation is still running (caller should request repaint).
    pub fn tick_animations(&mut self, dt: f32) -> bool {
        let speed = 8.0_f32; // complete in ~0.125 s
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
        // Remove finished-closing tabs in reverse order so indices stay valid
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

    pub fn any_active(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// Immutable iterator over all tabs.
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

    pub fn tabs_mut(&mut self) -> &mut [TabState] {
        &mut self.tabs
    }
}
