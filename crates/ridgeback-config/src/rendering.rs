use serde::{Deserialize, Serialize};

/// Rendering performance settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderingConfig {
    /// Continue rendering terminal & shaders when the window is not focused.
    pub update_in_background: bool,
    /// Maximum frames per second for shader/animation updates (1-240).
    pub max_shader_fps: u32,
    /// Automatically reduce effects and FPS when on battery power.
    pub battery_aware: bool,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            update_in_background: true,
            max_shader_fps: 144,
            battery_aware: true,
        }
    }
}

impl RenderingConfig {
    /// Clamp shader FPS to valid range.
    pub fn effective_shader_fps(&self) -> u32 {
        self.max_shader_fps.clamp(1, 240)
    }

    /// Duration between shader frames.
    pub fn shader_frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(1.0 / self.effective_shader_fps() as f64)
    }
}
