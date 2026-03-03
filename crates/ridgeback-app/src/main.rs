mod app;
mod tabs;
mod terminal_view;
mod shortcuts;
mod settings;
mod find_overlay;
mod command_query;
mod casting;
mod toast;
mod tab_drag;
mod split_pane;
mod group_tab_bar;
mod particle_emit;
mod crt_postprocess;

use anyhow::Result;

fn main() -> Result<()> {
    // Initialize tracing for structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting Ridgeback terminal");

    // Load configuration
    let config = ridgeback_config::Config::load()?;

    // ── Application icon ──────────────────────────────────────────────────
    // Embedded at compile time so it works regardless of working directory.
    static ICON_PNG: &[u8] = include_bytes!("../../../assets/images/icon.png");
    let icon = load_icon(ICON_PNG);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ridgeback")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([400.0, 300.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "Ridgeback",
        native_options,
        Box::new(move |cc| {
            // ── System dark/light mode detection ─────────────────────────
            // Follow the OS preference; fall back to dark if unavailable.
            let prefer_dark = system_prefers_dark();
            if prefer_dark {
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
            } else {
                cc.egui_ctx.set_visuals(egui::Visuals::light());
            }

            // ── Font setup: include Phosphor icons for all icon glyphs ───
            let mut fonts = egui::FontDefinitions::default();

            // Phosphor regular icon font — gives us codepoints like
            // egui_phosphor::regular::PLUS, GEAR, X etc.
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(app::RidgebackApp::new(config)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}

/// Decode a PNG byte slice into an `egui::IconData` (RGBA8, any size).
fn load_icon(png_bytes: &[u8]) -> egui::IconData {
    match image::load_from_memory(png_bytes) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(e) => {
            eprintln!("Failed to decode icon: {e}");
            // Fallback: 1×1 transparent pixel
            egui::IconData { rgba: vec![0, 0, 0, 0], width: 1, height: 1 }
        }
    }
}

/// Detect whether the OS prefers dark mode.
/// Returns `true` for dark, `false` for light.  Defaults to `true` if unknown.
fn system_prefers_dark() -> bool {
    #[cfg(target_os = "windows")]
    {
        // Read the AppsUseLightTheme registry key (0 = dark, 1 = light)
        use std::process::Command;
        let out = Command::new("reg")
            .args(["query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "/v", "AppsUseLightTheme"])
            .output();
        if let Ok(out) = out {
            let s = String::from_utf8_lossy(&out.stdout);
            // Value is hex "0x1" for light, "0x0" for dark
            if s.contains("0x1") { return false; }
            if s.contains("0x0") { return true; }
        }
        true // default dark
    }
    #[cfg(not(target_os = "windows"))]
    { true }
}
