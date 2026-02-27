//! WSL (Windows Subsystem for Linux) detection and profile generation.
//!
//! Provides utilities for discovering installed WSL distributions and
//! generating terminal profiles for each of them.

use crate::profile::Profile;

/// Information about an installed WSL distribution.
#[derive(Debug, Clone)]
pub struct WslDistro {
    /// Distribution name (e.g., "Ubuntu", "Debian").
    pub name: String,
    /// Whether this is the default WSL distribution.
    pub is_default: bool,
    /// WSL version (1 or 2).
    pub version: u8,
}

/// Detect installed WSL distributions by running `wsl.exe --list --verbose`.
///
/// Returns an empty vec on non-Windows platforms or if WSL is not installed.
pub fn detect_wsl_distros() -> Vec<WslDistro> {
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }

    #[cfg(target_os = "windows")]
    {
        detect_wsl_distros_windows()
    }
}

#[cfg(target_os = "windows")]
fn detect_wsl_distros_windows() -> Vec<WslDistro> {
    use std::process::Command;

    let output = match Command::new("wsl.exe")
        .args(["--list", "--verbose"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    // WSL outputs UTF-16LE on Windows; decode it
    let stdout_bytes = &output.stdout;
    let text = if stdout_bytes.len() >= 2 && stdout_bytes[0] == 0xFF && stdout_bytes[1] == 0xFE {
        // BOM-prefixed UTF-16LE
        let u16s: Vec<u16> = stdout_bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        // Fall back to lossy UTF-8
        String::from_utf8_lossy(stdout_bytes).to_string()
    };

    let mut distros = Vec::new();

    for line in text.lines().skip(1) {
        // skip header row
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let is_default = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();

        // Format: "NAME   STATE   VERSION"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let name = parts[0].to_string();
            let version = parts[2].parse::<u8>().unwrap_or(2);
            distros.push(WslDistro {
                name,
                is_default,
                version,
            });
        } else if !parts.is_empty() {
            // Minimal: just get the name
            distros.push(WslDistro {
                name: parts[0].to_string(),
                is_default,
                version: 2,
            });
        }
    }

    distros
}

/// Generate profiles for all detected WSL distributions.
pub fn wsl_profiles() -> Vec<(String, Profile)> {
    detect_wsl_distros()
        .into_iter()
        .map(|distro| {
            let key = format!("wsl-{}", distro.name.to_lowercase());
            let profile = Profile::wsl_distro(&distro.name);
            (key, profile)
        })
        .collect()
}

/// Check whether WSL is available on this system.
pub fn is_wsl_available() -> bool {
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("wsl.exe")
            .arg("--status")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
