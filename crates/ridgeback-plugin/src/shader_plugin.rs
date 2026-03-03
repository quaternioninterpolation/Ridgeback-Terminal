//! Trait-based shader & particle plugin API.
//!
//! A **ShaderPlugin** describes a background visual effect:
//!   - Which WGSL shader file to load
//!   - What parameters it exposes (with types, labels, min/max)
//!   - An optional Lua `on_frame(dt, params)` hook for CPU-side animation
//!
//! A **ParticlePlugin** describes a typing-particle effect:
//!   - A Lua `on_keypress(x, y, params)` hook that returns a list of particle events
//!   - Parameter schema (same as shader plugins)

use std::collections::HashMap;
use std::path::PathBuf;
use serde_json::Value;

// ── Parameter descriptor ────────────────────────────────────────────────────

/// The value type for a plugin parameter, used to drive the settings UI.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    /// A floating-point slider with min/max range.
    Float { min: f32, max: f32 },
    /// A whole-number slider.
    Int { min: i64, max: i64 },
    /// A boolean checkbox.
    Bool,
    /// A colour picker, value stored as `"#RRGGBB"` string.
    Color,
    /// A free-form text field (for e.g. file paths).
    Text,
}

/// Metadata for one adjustable parameter exposed by a plugin.
#[derive(Debug, Clone)]
pub struct ParamDescriptor {
    /// Internal key used in `ShaderEffectConfig::params`.
    pub key: String,
    /// Human-readable label shown in the settings UI.
    pub label: String,
    /// Value type — drives which widget to show.
    pub ty: ParamType,
    /// Default value when the profile has no explicit override.
    pub default: Value,
    /// Optional tooltip shown in the settings UI.
    pub hint: Option<String>,
}

impl ParamDescriptor {
    pub fn float(key: &str, label: &str, min: f32, max: f32, default: f32) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            ty: ParamType::Float { min, max },
            default: serde_json::json!(default),
            hint: None,
        }
    }
    pub fn color(key: &str, label: &str, default: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            ty: ParamType::Color,
            default: serde_json::json!(default),
            hint: None,
        }
    }
    pub fn bool(key: &str, label: &str, default: bool) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            ty: ParamType::Bool,
            default: serde_json::json!(default),
            hint: None,
        }
    }
    pub fn with_hint(mut self, hint: &str) -> Self {
        self.hint = Some(hint.to_string());
        self
    }
}

// ── Trigger mode ──────────────────────────────────────────────────────────────

/// When a particle plugin should fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerMode {
    /// Emit on every keypress (cursor position).
    Keypress,
    /// Emit when Enter / newline is pressed.
    Newline,
    /// Emit every frame (snow, bubbles, ambient effects).
    Fullscreen,
}

// ── Particle event ───────────────────────────────────────────────────────────

/// A single particle spawned by a particle plugin.
///
/// Colour is specified directly as RGBA (0.0–1.0 each).
/// The renderer fades alpha automatically based on remaining life.
#[derive(Debug, Clone)]
pub struct ParticleEvent {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// Initial lifetime in seconds.
    pub life: f32,
    pub radius: f32,
    /// RGBA colour (0.0–1.0 per component). `a` = base opacity before life-fade.
    pub color: [f32; 4],
    /// Gravity multiplier (default 1.0). 0 = no gravity, negative = float up.
    pub gravity: f32,
    /// Drag coefficient (default 0.5). Higher = more air resistance.
    pub drag: f32,
}

// ── Shader plugin trait ──────────────────────────────────────────────────────

/// A registered background shader effect.
pub trait ShaderPlugin: Send + Sync {
    /// Unique identifier (matches `ShaderEffectConfig::plugin_id`).
    fn id(&self) -> &str;
    /// Human-readable display name shown in settings.
    fn display_name(&self) -> &str;
    /// Absolute path to the `.wgsl` shader file.
    fn wgsl_path(&self) -> &PathBuf;
    /// Ordered list of adjustable parameters.
    fn param_schema(&self) -> &[ParamDescriptor];
    /// Fill `params` map with defaults for any missing keys.
    fn fill_defaults(&self, params: &mut HashMap<String, Value>) {
        for desc in self.param_schema() {
            params.entry(desc.key.clone()).or_insert_with(|| desc.default.clone());
        }
    }
}

// ── Particle plugin trait ────────────────────────────────────────────────────

/// A registered particle effect — fully Lua-driven.
pub trait ParticlePlugin: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn param_schema(&self) -> &[ParamDescriptor];
    /// Which events this plugin responds to.
    fn trigger_modes(&self) -> &[TriggerMode];
    fn fill_defaults(&self, params: &mut HashMap<String, Value>) {
        for desc in self.param_schema() {
            params.entry(desc.key.clone()).or_insert_with(|| desc.default.clone());
        }
    }
    /// Emit particles for a keypress at pixel position (x, y).
    fn emit(&self, x: f32, y: f32, params: &HashMap<String, Value>) -> Vec<ParticleEvent> {
        let _ = (x, y, params);
        vec![]
    }
    /// Emit particles when Enter/newline is pressed at (x, y).
    fn emit_newline(&self, x: f32, y: f32, params: &HashMap<String, Value>) -> Vec<ParticleEvent> {
        let _ = (x, y, params);
        vec![]
    }
    /// Emit particles every frame (for fullscreen ambient effects).
    /// `dt` = frame delta, `width`/`height` = viewport size.
    fn emit_frame(&self, dt: f32, width: f32, height: f32, params: &HashMap<String, Value>) -> Vec<ParticleEvent> {
        let _ = (dt, width, height, params);
        vec![]
    }
}

// ── Built-in fire shader plugin ──────────────────────────────────────────────

/// Built-in fire background shader plugin (backed by `fire.wgsl`).
pub struct BuiltinFireShaderPlugin {
    wgsl_path: PathBuf,
    schema: Vec<ParamDescriptor>,
}

impl BuiltinFireShaderPlugin {
    pub fn new(shaders_dir: &std::path::Path) -> Self {
        let wgsl_path = shaders_dir.join("fire.wgsl");
        let schema = vec![
            ParamDescriptor::float("intensity", "Intensity", 0.0, 2.0, 1.0)
                .with_hint("Overall brightness of the fire"),
            ParamDescriptor::float("decay_rate", "Decay Rate", 0.001, 0.2, 0.03)
                .with_hint("How quickly flames die upward"),
            ParamDescriptor::float("spread", "Spread", 0.0, 1.0, 0.5)
                .with_hint("Horizontal spread of flames"),
            ParamDescriptor::float("height", "Height (0-1 of screen)", 0.05, 0.9, 0.25)
                .with_hint("How high from the bottom the fire reaches"),
            ParamDescriptor::float("particle_multiplier", "Particle Multiplier", 0.0, 5.0, 1.0)
                .with_hint("Scales the number of typing particles emitted"),
            ParamDescriptor::color("color_base", "Base Colour", "#1a0000"),
            ParamDescriptor::color("color_mid", "Mid Colour", "#ff4400"),
            ParamDescriptor::color("color_top", "Top Colour", "#ffdd00"),
        ];
        Self { wgsl_path, schema }
    }
}

impl ShaderPlugin for BuiltinFireShaderPlugin {
    fn id(&self) -> &str { "fire" }
    fn display_name(&self) -> &str { "🔥 Fire" }
    fn wgsl_path(&self) -> &PathBuf { &self.wgsl_path }
    fn param_schema(&self) -> &[ParamDescriptor] { &self.schema }
}

// ── Built-in CRT shader plugin ───────────────────────────────────────────────

/// Built-in CRT post-process shader plugin (backed by `crt.wgsl`).
pub struct BuiltinCrtShaderPlugin {
    wgsl_path: PathBuf,
    schema: Vec<ParamDescriptor>,
}

impl BuiltinCrtShaderPlugin {
    pub fn new(shaders_dir: &std::path::Path) -> Self {
        let wgsl_path = shaders_dir.join("crt.wgsl");
        let schema = vec![
            ParamDescriptor::float("scanline_intensity", "Scanline Intensity", 0.0, 1.0, 1.0),
            ParamDescriptor::float("curvature", "Curvature", 0.0, 0.5, 0.1),
            ParamDescriptor::float("bloom_strength", "Bloom Strength", 0.0, 1.0, 0.20),
            ParamDescriptor::float("chromatic_aberration", "Chromatic Aberration", 0.0, 0.02, 0.013),
        ];
        Self { wgsl_path, schema }
    }
}

impl ShaderPlugin for BuiltinCrtShaderPlugin {
    fn id(&self) -> &str { "crt" }
    fn display_name(&self) -> &str { "📺 CRT" }
    fn wgsl_path(&self) -> &PathBuf { &self.wgsl_path }
    fn param_schema(&self) -> &[ParamDescriptor] { &self.schema }
}



