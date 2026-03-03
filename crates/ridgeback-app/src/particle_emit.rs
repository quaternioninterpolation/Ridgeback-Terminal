//! Bridge between `TabState` and the active `ParticlePlugin`s.
//!
//! This module is intentionally thin — it reads the tab's `particle_effects`
//! list, looks up each plugin in the global `ShaderPluginHost`, calls the
//! appropriate emit function, and returns the combined `ParticleEvent` list.
//! The caller is responsible for handing them to `tab.particles.spawn()`.
//!
//! Three emit functions are provided:
//!   - `emit_for_tab` — keypress events (cursor position)
//!   - `emit_newline_for_tab` — Enter/newline events
//!   - `emit_frame_for_tab` — per-frame ambient/fullscreen effects
//!
//! The `ShaderPluginHost` is stored on `RidgebackApp` and passed in here via a
//! thread-local so that `terminal_view.rs` can call this without needing to
//! plumb the host through every render call.

use std::sync::{Arc, Mutex};
use ridgeback_plugin::{ShaderPluginHost, ParticleEvent};
use crate::tabs::TabState;

// Thread-local host reference — set once per frame in app.rs before rendering.
use std::cell::RefCell;

thread_local! {
    static PLUGIN_HOST: RefCell<Option<Arc<Mutex<ShaderPluginHost>>>> = RefCell::new(None);
}

/// Install the `ShaderPluginHost` for use by `emit_for_tab` this frame.
pub fn set_host(host: Arc<Mutex<ShaderPluginHost>>) {
    PLUGIN_HOST.with(|cell| {
        *cell.borrow_mut() = Some(host);
    });
}

/// Helper: run a closure against every enabled particle effect, collecting results.
fn with_effects<F>(tab: &TabState, mut f: F) -> Vec<ParticleEvent>
where
    F: FnMut(&dyn ridgeback_plugin::ParticlePlugin, &std::collections::HashMap<String, serde_json::Value>) -> Vec<ParticleEvent>,
{
    let effects = &tab.particle_effects;
    if effects.is_empty() {
        return vec![];
    }

    PLUGIN_HOST.with(|cell| {
        let guard = cell.borrow();
        let arc = match guard.as_ref() {
            Some(a) => a,
            None => return vec![],
        };
        let host = match arc.lock() {
            Ok(h) => h,
            Err(_) => return vec![],
        };

        let mut all = Vec::new();
        for entry in effects {
            if !entry.enabled { continue; }
            if entry.plugin_id == "none" || entry.plugin_id.is_empty() { continue; }
            if let Some(plugin) = host.get_particle_plugin(&entry.plugin_id) {
                let mut result = f(plugin, &entry.params);
                all.append(&mut result);
            }
        }
        all
    })
}

/// Emit particles for a keypress at terminal-local position `(x, y)`.
pub fn emit_for_tab(x: f32, y: f32, tab: &TabState) -> Vec<ParticleEvent> {
    with_effects(tab, |plugin, params| plugin.emit(x, y, params))
}

/// Emit particles when Enter is pressed at terminal-local position `(x, y)`.
pub fn emit_newline_for_tab(x: f32, y: f32, tab: &TabState) -> Vec<ParticleEvent> {
    with_effects(tab, |plugin, params| plugin.emit_newline(x, y, params))
}

/// Emit particles every frame for fullscreen/ambient effects.
pub fn emit_frame_for_tab(dt: f32, width: f32, height: f32, tab: &TabState) -> Vec<ParticleEvent> {
    with_effects(tab, |plugin, params| plugin.emit_frame(dt, width, height, params))
}

