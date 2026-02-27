use std::time::{Duration, Instant};

/// Controls frame pacing for shader/animation updates.
///
/// Separates text-content repaints (always immediate) from
/// shader animation updates (capped to max_fps).
pub struct FramePacer {
    max_fps: u32,
    last_shader_frame: Instant,
    last_activity: Instant,
    window_focused: bool,
    update_in_background: bool,
    battery_mode: bool,
    idle_timeout: Duration,
    battery_aware: bool,
    last_battery_check: Instant,
}

impl FramePacer {
    pub fn new(max_fps: u32, update_in_background: bool) -> Self {
        Self {
            max_fps,
            last_shader_frame: Instant::now(),
            last_activity: Instant::now(),
            window_focused: true,
            update_in_background,
            battery_mode: false,
            idle_timeout: Duration::from_millis(100),
            battery_aware: true,
            last_battery_check: Instant::now(),
        }
    }

    /// Update configuration at runtime.
    pub fn set_max_fps(&mut self, fps: u32) {
        self.max_fps = fps.clamp(1, 240);
    }

    pub fn set_update_in_background(&mut self, enabled: bool) {
        self.update_in_background = enabled;
    }

    pub fn set_window_focused(&mut self, focused: bool) {
        self.window_focused = focused;
    }

    pub fn set_battery_mode(&mut self, on_battery: bool) {
        self.battery_mode = on_battery;
    }

    /// Signal that activity occurred (PTY output, user input).
    pub fn signal_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Returns the effective FPS cap considering battery mode.
    pub fn effective_fps(&self) -> u32 {
        if self.battery_mode {
            self.max_fps.min(30)
        } else {
            self.max_fps
        }
    }

    /// Check if a shader frame should be rendered now.
    pub fn should_render_shader(&mut self) -> bool {
        // Don't render if window is unfocused and background updates are disabled
        if !self.window_focused && !self.update_in_background {
            return false;
        }

        let fps = self.effective_fps();
        let frame_interval = Duration::from_secs_f64(1.0 / fps as f64);

        // If idle for too long, reduce to idle rate
        let is_idle = self.last_activity.elapsed() > self.idle_timeout;
        let actual_interval = if is_idle && !self.battery_mode {
            // 10fps idle rate
            Duration::from_millis(100)
        } else {
            frame_interval
        };

        if self.last_shader_frame.elapsed() >= actual_interval {
            self.last_shader_frame = Instant::now();
            true
        } else {
            false
        }
    }

    /// Get the duration to sleep until the next shader frame.
    pub fn time_to_next_frame(&self) -> Duration {
        let fps = self.effective_fps();
        let frame_interval = Duration::from_secs_f64(1.0 / fps as f64);
        let elapsed = self.last_shader_frame.elapsed();
        frame_interval.saturating_sub(elapsed)
    }

    /// Whether bloom should be enabled (disabled on battery).
    pub fn bloom_enabled(&self) -> bool {
        !self.battery_mode
    }

    /// Enable or disable battery-aware mode.
    pub fn set_battery_aware(&mut self, aware: bool) {
        self.battery_aware = aware;
    }

    /// Poll the system battery status and update battery_mode.
    /// Only checks every 30 seconds to avoid performance impact.
    pub fn poll_battery_status(&mut self) {
        if !self.battery_aware {
            self.battery_mode = false;
            return;
        }
        if self.last_battery_check.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.last_battery_check = Instant::now();
        self.battery_mode = is_on_battery();
    }
}

/// Cross-platform battery status detection.
fn is_on_battery() -> bool {
    #[cfg(target_os = "windows")]
    {
        is_on_battery_windows()
    }
    #[cfg(target_os = "macos")]
    {
        is_on_battery_macos()
    }
    #[cfg(target_os = "linux")]
    {
        is_on_battery_linux()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false // Assume plugged in on unknown platforms
    }
}

#[cfg(target_os = "windows")]
fn is_on_battery_windows() -> bool {
    use std::mem::MaybeUninit;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct SYSTEM_POWER_STATUS {
        ACLineStatus: u8,
        BatteryFlag: u8,
        BatteryLifePercent: u8,
        SystemStatusFlag: u8,
        BatteryLifeTime: u32,
        BatteryFullLifeTime: u32,
    }

    extern "system" {
        fn GetSystemPowerStatus(lp: *mut SYSTEM_POWER_STATUS) -> i32;
    }

    unsafe {
        let mut status = MaybeUninit::<SYSTEM_POWER_STATUS>::zeroed().assume_init();
        if GetSystemPowerStatus(&mut status) != 0 {
            status.ACLineStatus == 0 // 0 = offline (on battery)
        } else {
            false
        }
    }
}

#[cfg(target_os = "macos")]
fn is_on_battery_macos() -> bool {
    // Use IOKit via pmset command as a simple cross-compile-friendly approach
    std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()
        .and_then(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            // pmset output contains "'Battery Power'" when on battery
            if text.contains("Battery Power") {
                Some(true)
            } else {
                Some(false)
            }
        })
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_on_battery_linux() -> bool {
    // Read from /sys/class/power_supply/
    let ac_path = std::path::Path::new("/sys/class/power_supply/AC/online");
    if ac_path.exists() {
        return std::fs::read_to_string(ac_path)
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .map(|v| v == 0) // 0 = not online = on battery
            .unwrap_or(false);
    }
    // Try BAT0 status as fallback
    let bat_path = std::path::Path::new("/sys/class/power_supply/BAT0/status");
    if bat_path.exists() {
        return std::fs::read_to_string(bat_path)
            .ok()
            .map(|s| s.trim() == "Discharging")
            .unwrap_or(false);
    }
    false // No battery info found — assume plugged in
}
