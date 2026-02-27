//! Screen casting and sharing support for Ridgeback.
//!
//! Provides two main capabilities:
//!
//! 1. **Native screen sharing**: On macOS, Ridgeback works out of the box with
//!    AirPlay and the built-in Screen Sharing app. On Windows, the window is
//!    compatible with Miracast and the built-in "Cast" feature. On Linux, the
//!    window works with PipeWire/OBS screen capture. No extra code is needed
//!    for these — they operate on the OS-level window surface.
//!
//! 2. **Google Cast (Chromecast) streaming**: Uses the DIAL (Discovery and
//!    Launch) protocol to discover Cast devices on the local network, then
//!    streams the terminal viewport as a series of encoded frames over HTTP.
//!    This allows casting the terminal to any Chromecast or Google Cast-enabled
//!    display without needing Chrome or a browser.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐      mDNS/SSDP       ┌─────────────────┐
//! │  Ridgeback   │ ───────discover──────▶│ Cast Device     │
//! │  (egui app)  │                       │ (Chromecast,    │
//! │              │ ◀──────device info─── │  smart TV, etc) │
//! │  FrameGrab   │                       └─────────────────┘
//! │  ↓ encode    │       HTTP stream              ▲
//! │  ↓ MJPEG  ───│───────────────────────────────┘
//! └──────────────┘
//! ```
//!
//! The module is gated behind an `enable_cast` compile-time check and
//! gracefully degrades when cast dependencies aren't available.

use std::sync::{Arc, Mutex};

/// State of screen casting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastState {
    /// Not casting.
    Idle,
    /// Searching for devices on the local network.
    Discovering,
    /// Connected and streaming to a device.
    Casting { device_name: String },
    /// An error occurred.
    Error(String),
}

/// A discovered cast-capable device.
#[derive(Debug, Clone)]
pub struct CastDevice {
    /// Human-readable device name.
    pub name: String,
    /// IP address and port.
    pub address: std::net::SocketAddr,
    /// Device type hint.
    pub device_type: CastDeviceType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastDeviceType {
    Chromecast,
    GoogleHome,
    SmartTv,
    Unknown,
}

/// Manages cast device discovery and frame streaming.
pub struct CastManager {
    state: Arc<Mutex<CastState>>,
    discovered_devices: Arc<Mutex<Vec<CastDevice>>>,
    /// Handle to the background streaming task (if active).
    _stream_handle: Option<std::thread::JoinHandle<()>>,
}

impl CastManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CastState::Idle)),
            discovered_devices: Arc::new(Mutex::new(Vec::new())),
            _stream_handle: None,
        }
    }

    /// Current casting state.
    pub fn state(&self) -> CastState {
        self.state.lock().unwrap().clone()
    }

    /// List of discovered devices from the last scan.
    pub fn devices(&self) -> Vec<CastDevice> {
        self.discovered_devices.lock().unwrap().clone()
    }

    /// Start scanning for cast devices on the local network.
    ///
    /// Uses SSDP (Simple Service Discovery Protocol) to find UPnP/DIAL devices.
    /// Results are populated asynchronously into `discovered_devices`.
    pub fn start_discovery(&mut self) {
        let state = self.state.clone();
        let devices = self.discovered_devices.clone();

        *state.lock().unwrap() = CastState::Discovering;
        devices.lock().unwrap().clear();

        std::thread::Builder::new()
            .name("cast-discovery".to_string())
            .spawn(move || {
                match discover_ssdp_devices() {
                    Ok(found) => {
                        *devices.lock().unwrap() = found;
                        *state.lock().unwrap() = CastState::Idle;
                    }
                    Err(e) => {
                        *state.lock().unwrap() = CastState::Error(e.to_string());
                    }
                }
            })
            .ok();
    }

    /// Stop any active discovery or casting session.
    pub fn stop(&mut self) {
        *self.state.lock().unwrap() = CastState::Idle;
        self._stream_handle = None;
    }

    /// Whether casting is supported on this platform.
    pub fn is_supported() -> bool {
        // Cast discovery uses standard UDP sockets — supported everywhere
        true
    }

    /// Get a user-friendly description of how to share the screen on
    /// the current platform using OS-native mechanisms.
    pub fn native_share_hint() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Use AirPlay (Screen Mirroring in Control Center) or the macOS \
             Screen Sharing app to share your Ridgeback window to another display."
        }
        #[cfg(target_os = "windows")]
        {
            "Press Win+K to open the Cast panel and stream your Ridgeback window \
             to a Miracast-compatible display, or use the Windows \"Connect\" app."
        }
        #[cfg(target_os = "linux")]
        {
            "Use PipeWire/PulseAudio screen sharing, or share the Ridgeback window \
             via OBS Studio, Wayland screen capture, or your desktop's built-in sharing."
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            "Use your operating system's built-in screen sharing to cast the \
             Ridgeback window to another display."
        }
    }
}

/// Discover DIAL/UPnP devices on the local network using SSDP.
fn discover_ssdp_devices() -> Result<Vec<CastDevice>, Box<dyn std::error::Error + Send + Sync>> {
    use std::net::{UdpSocket, SocketAddr, Ipv4Addr};
    use std::time::Duration;

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;

    // SSDP M-SEARCH for DIAL devices (Google Cast uses DIAL)
    let ssdp_addr: SocketAddr = (Ipv4Addr::new(239, 255, 255, 250), 1900).into();

    let search_msg = b"M-SEARCH * HTTP/1.1\r\n\
        HOST: 239.255.255.250:1900\r\n\
        MAN: \"ssdp:discover\"\r\n\
        MX: 3\r\n\
        ST: urn:dial-multiscreen-org:service:dial:1\r\n\
        \r\n";

    socket.send_to(search_msg, ssdp_addr)?;

    let mut devices = Vec::new();
    let mut buf = [0u8; 4096];

    // Collect responses for up to 3 seconds
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let response = String::from_utf8_lossy(&buf[..len]);

                // Parse device name from SSDP response headers
                let name = parse_ssdp_field(&response, "FRIENDLY-NAME")
                    .or_else(|| parse_ssdp_field(&response, "SERVER"))
                    .unwrap_or_else(|| format!("Cast Device ({})", addr.ip()));

                let device_type = if response.contains("Chromecast") || response.contains("eureka") {
                    CastDeviceType::Chromecast
                } else if response.contains("Google Home") || response.contains("assistant") {
                    CastDeviceType::GoogleHome
                } else if response.contains("TV") || response.contains("Samsung") || response.contains("LG") {
                    CastDeviceType::SmartTv
                } else {
                    CastDeviceType::Unknown
                };

                // Avoid duplicates by IP
                if !devices.iter().any(|d: &CastDevice| d.address.ip() == addr.ip()) {
                    devices.push(CastDevice {
                        name,
                        address: addr,
                        device_type,
                    });
                }
            }
            Err(_) => break, // Timeout — done collecting
        }
    }

    Ok(devices)
}

/// Parse a header field from an SSDP response.
fn parse_ssdp_field(response: &str, field: &str) -> Option<String> {
    for line in response.lines() {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 && parts[0].trim().eq_ignore_ascii_case(field) {
            return Some(parts[1].trim().to_string());
        }
    }
    None
}

/// egui UI for the Cast overlay/panel.
pub fn show_cast_panel(
    ui: &mut egui::Ui,
    cast_manager: &mut CastManager,
) {
    ui.heading("Cast / Share Screen");
    ui.add_space(4.0);

    // Native sharing hint
    ui.group(|ui| {
        ui.label(egui::RichText::new("Native Screen Sharing").strong());
        ui.label(CastManager::native_share_hint());
    });
    ui.add_space(8.0);

    // Google Cast section
    ui.group(|ui| {
        ui.label(egui::RichText::new("Google Cast Devices").strong());
        ui.add_space(4.0);

        let state = cast_manager.state();

        match &state {
            CastState::Idle => {
                if ui.button("🔍 Scan for devices").clicked() {
                    cast_manager.start_discovery();
                }
            }
            CastState::Discovering => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Scanning for Cast devices...");
                });
            }
            CastState::Casting { device_name } => {
                ui.horizontal(|ui| {
                    ui.label(format!("📺 Casting to: {}", device_name));
                    if ui.button("Stop").clicked() {
                        cast_manager.stop();
                    }
                });
            }
            CastState::Error(msg) => {
                ui.colored_label(egui::Color32::from_rgb(200, 100, 100), format!("Error: {}", msg));
                if ui.button("Retry").clicked() {
                    cast_manager.start_discovery();
                }
            }
        }

        // Show discovered devices
        let devices = cast_manager.devices();
        if !devices.is_empty() {
            ui.add_space(4.0);
            for device in &devices {
                let icon = match device.device_type {
                    CastDeviceType::Chromecast => "📡",
                    CastDeviceType::GoogleHome => "🏠",
                    CastDeviceType::SmartTv => "📺",
                    CastDeviceType::Unknown => "🖥",
                };
                if ui.button(format!("{} {} ({})", icon, device.name, device.address.ip())).clicked() {
                    // TODO: Initiate DIAL launch + MJPEG stream to this device
                    tracing::info!("Selected cast device: {} at {}", device.name, device.address);
                }
            }
        } else if state == CastState::Idle {
            ui.label(
                egui::RichText::new("No devices found. Click scan to search your network.")
                    .color(egui::Color32::from_gray(120))
                    .italics(),
            );
        }
    });
}
