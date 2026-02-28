//! Bridge between `TabState` and the active `ParticlePlugin`.
//!
//! This module is intentionally thin — it reads the tab's `typing_particles`
//! config, looks up the plugin in the global `ShaderPluginHost`, calls `emit()`,
//! and returns the resulting `ParticleEvent` list.  The caller is responsible
//! for handing them to `tab.particles.spawn()`.
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

/// Emit particles for a keypress at terminal-local position `(x, y)`.
/// Returns an empty vec if the tab has no active particle plugin or the host
/// is not installed.
pub fn emit_for_tab(x: f32, y: f32, tab: &TabState) -> Vec<ParticleEvent> {
    let plugin_id = &tab.typing_particles.plugin_id;
    if plugin_id == "none" || plugin_id.is_empty() {
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
        match host.get_particle_plugin(plugin_id) {
            Some(plugin) => plugin.emit(x, y, &tab.typing_particles.params),
            None => vec![],
        }
    })
}

