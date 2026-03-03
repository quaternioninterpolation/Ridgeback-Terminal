//! Runtime registry for shader and particle plugins.
use anyhow::Result;
use mlua::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::shader_plugin::{
    ParamDescriptor, ParamType, ParticleEvent, ParticlePlugin, ShaderPlugin, TriggerMode,
    BuiltinFireShaderPlugin, BuiltinCrtShaderPlugin,
};

// ── Lua-backed shader plugin ──────────────────────────────────────────────────

struct LuaShaderPlugin {
    plugin_id: String,
    plugin_display_name: String,
    wgsl_path: PathBuf,
    schema: Vec<ParamDescriptor>,
}

impl ShaderPlugin for LuaShaderPlugin {
    fn id(&self) -> &str { &self.plugin_id }
    fn display_name(&self) -> &str { &self.plugin_display_name }
    fn wgsl_path(&self) -> &PathBuf { &self.wgsl_path }
    fn param_schema(&self) -> &[ParamDescriptor] { &self.schema }
}

// ── Lua-backed particle plugin ────────────────────────────────────────────────

struct LuaParticlePlugin {
    plugin_id: String,
    plugin_display_name: String,
    schema: Vec<ParamDescriptor>,
    lua_source: String,
    triggers: Vec<TriggerMode>,
}

/// Helper: create a sandboxed Lua VM, load source, push params table.
fn lua_vm_with_params(source: &str, params: &HashMap<String, serde_json::Value>) -> Option<(Lua, LuaTable)> {
    let lua = Lua::new_with(
        LuaStdLib::MATH | LuaStdLib::TABLE | LuaStdLib::STRING,
        LuaOptions::default(),
    ).ok()?;
    lua.load(source).exec().ok()?;
    let lua_params = lua.create_table().ok()?;
    for (k, v) in params {
        let lua_val: LuaValue = match v {
            serde_json::Value::Number(n) => LuaValue::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::Bool(b) => LuaValue::Boolean(*b),
            serde_json::Value::String(s) => lua.create_string(s.as_str()).map(LuaValue::String).unwrap_or(LuaValue::Nil),
            _ => LuaValue::Nil,
        };
        let _ = lua_params.set(k.as_str(), lua_val);
    }
    Some((lua, lua_params))
}

/// Helper: parse a list of particle tables from a Lua return value.
fn parse_particle_table(result: LuaTable) -> Vec<ParticleEvent> {
    let mut events = Vec::new();
    let mut idx = 1i64;
    loop {
        let entry: LuaValue = match result.get(idx) {
            Ok(v) => v,
            Err(_) => break,
        };
        let tbl = match entry { LuaValue::Table(t) => t, _ => break };
        let gf = |key: &str| -> f32 { tbl.get::<f64>(key).unwrap_or(0.0) as f32 };

        // Parse color sub-table {r, g, b, a} (0-1 floats)
        let color = match tbl.get::<LuaTable>("color") {
            Ok(ct) => {
                let r = ct.get::<f64>("r").unwrap_or(1.0) as f32;
                let g = ct.get::<f64>("g").unwrap_or(1.0) as f32;
                let b = ct.get::<f64>("b").unwrap_or(1.0) as f32;
                let a = ct.get::<f64>("a").unwrap_or(1.0) as f32;
                [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a.clamp(0.0, 1.0)]
            }
            Err(_) => [1.0, 1.0, 1.0, 1.0],
        };

        events.push(ParticleEvent {
            x: gf("x"), y: gf("y"), vx: gf("vx"), vy: gf("vy"),
            life: gf("life"), radius: gf("radius"),
            color,
            gravity: tbl.get::<f64>("gravity").map(|v| v as f32).unwrap_or(1.0),
            drag: tbl.get::<f64>("drag").map(|v| v as f32).unwrap_or(0.5),
        });
        idx += 1;
    }
    events
}

impl ParticlePlugin for LuaParticlePlugin {
    fn id(&self) -> &str { &self.plugin_id }
    fn display_name(&self) -> &str { &self.plugin_display_name }
    fn param_schema(&self) -> &[ParamDescriptor] { &self.schema }
    fn trigger_modes(&self) -> &[TriggerMode] { &self.triggers }

    fn emit(&self, x: f32, y: f32, params: &HashMap<String, serde_json::Value>) -> Vec<ParticleEvent> {
        let (lua, lua_params) = match lua_vm_with_params(&self.lua_source, params) {
            Some(v) => v,
            None => return vec![],
        };
        let func: LuaFunction = match lua.globals().get("on_keypress") {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        match func.call::<LuaTable>((x as f64, y as f64, lua_params)) {
            Ok(t) => parse_particle_table(t),
            Err(_) => vec![],
        }
    }

    fn emit_newline(&self, x: f32, y: f32, params: &HashMap<String, serde_json::Value>) -> Vec<ParticleEvent> {
        let (lua, lua_params) = match lua_vm_with_params(&self.lua_source, params) {
            Some(v) => v,
            None => return vec![],
        };
        let func: LuaFunction = match lua.globals().get("on_newline") {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        match func.call::<LuaTable>((x as f64, y as f64, lua_params)) {
            Ok(t) => parse_particle_table(t),
            Err(_) => vec![],
        }
    }

    fn emit_frame(&self, dt: f32, width: f32, height: f32, params: &HashMap<String, serde_json::Value>) -> Vec<ParticleEvent> {
        let (lua, lua_params) = match lua_vm_with_params(&self.lua_source, params) {
            Some(v) => v,
            None => return vec![],
        };
        let func: LuaFunction = match lua.globals().get("on_frame") {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        match func.call::<LuaTable>((dt as f64, width as f64, height as f64, lua_params)) {
            Ok(t) => parse_particle_table(t),
            Err(_) => vec![],
        }
    }
}

// ── Host ──────────────────────────────────────────────────────────────────────

/// Central registry for all shader and particle plugins.
pub struct ShaderPluginHost {
    shader_plugins: Vec<Box<dyn ShaderPlugin>>,
    particle_plugins: Vec<Box<dyn ParticlePlugin>>,
    shaders_dir: PathBuf,
    plugins_dir: PathBuf,
}

impl ShaderPluginHost {
    pub fn new(shaders_dir: PathBuf, plugins_dir: PathBuf) -> Self {
        let mut host = Self {
            shader_plugins: Vec::new(),
            particle_plugins: Vec::new(),
            shaders_dir: shaders_dir.clone(),
            plugins_dir,
        };
        host.register_builtins();
        host
    }

    fn register_builtins(&mut self) {
        self.shader_plugins.push(Box::new(BuiltinFireShaderPlugin::new(&self.shaders_dir)));
        self.shader_plugins.push(Box::new(BuiltinCrtShaderPlugin::new(&self.shaders_dir)));
        // Particle plugins are now entirely Lua-driven (loaded from plugins dir).
    }

    pub fn load_user_plugins(&mut self) -> Result<usize> {
        let mut count = 0;

        // Scan bundled plugins first, then user plugins (user overrides bundled by ID)
        let bundled = Self::find_bundled_plugins_dir();
        let user = self.plugins_dir.clone();
        let dirs: Vec<PathBuf> = vec![bundled, user];

        for dir in &dirs {
            if !dir.exists() { continue; }
            for entry in std::fs::read_dir(dir)?.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("lua") { continue; }
                let source = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => { tracing::warn!("Plugin {}: {}", path.display(), e); continue; }
                };
                let plugin_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

                if let Ok(Some(p)) = self.try_load_lua_shader(&source, &plugin_dir) {
                    tracing::info!("Loaded shader plugin: {}", p.id());
                    self.shader_plugins.retain(|e| e.id() != p.id());
                    self.shader_plugins.push(p);
                    count += 1;
                }
                if let Ok(Some(p)) = self.try_load_lua_particle(&source) {
                    tracing::info!("Loaded particle plugin: {}", p.id());
                    self.particle_plugins.retain(|e| e.id() != p.id());
                    self.particle_plugins.push(p);
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    fn try_load_lua_shader(&self, source: &str, plugin_dir: &Path) -> Result<Option<Box<dyn ShaderPlugin>>> {
        let lua = Lua::new_with(LuaStdLib::MATH | LuaStdLib::TABLE | LuaStdLib::STRING, LuaOptions::default())?;
        lua.load(source).exec()?;
        let tbl: LuaTable = match lua.globals().get("RIDGEBACK_SHADER_PLUGIN") {
            Ok(LuaValue::Table(t)) => t,
            _ => return Ok(None),
        };
        let id: String = tbl.get("id")?;
        let display_name: String = tbl.get("display_name").unwrap_or_else(|_| id.clone());
        let wgsl_rel: String = tbl.get("wgsl")?;
        let wgsl_path = plugin_dir.join(&wgsl_rel);
        if !wgsl_path.exists() {
            anyhow::bail!("wgsl not found: {}", wgsl_path.display());
        }
        let schema = parse_param_schema(&tbl)?;
        Ok(Some(Box::new(LuaShaderPlugin { plugin_id: id, plugin_display_name: display_name, wgsl_path, schema })))
    }

    fn try_load_lua_particle(&self, source: &str) -> Result<Option<Box<dyn ParticlePlugin>>> {
        let lua = Lua::new_with(LuaStdLib::MATH | LuaStdLib::TABLE | LuaStdLib::STRING, LuaOptions::default())?;
        lua.load(source).exec()?;
        let tbl: LuaTable = match lua.globals().get("RIDGEBACK_PARTICLE_PLUGIN") {
            Ok(LuaValue::Table(t)) => t,
            _ => return Ok(None),
        };
        let id: String = tbl.get("id")?;
        let display_name: String = tbl.get("display_name").unwrap_or_else(|_| id.clone());
        let schema = parse_param_schema(&tbl)?;

        // Parse trigger modes from the "trigger" field.
        // Accepts a single string or an array of strings.
        let mut triggers = Vec::new();
        match tbl.get::<LuaValue>("trigger") {
            Ok(LuaValue::String(s)) => {
                if let Some(t) = parse_trigger_str(&s.to_string_lossy()) {
                    triggers.push(t);
                }
            }
            Ok(LuaValue::Table(arr)) => {
                let mut i = 1i64;
                loop {
                    match arr.get::<LuaValue>(i) {
                        Ok(LuaValue::String(s)) => {
                            if let Some(t) = parse_trigger_str(&s.to_string_lossy()) {
                                triggers.push(t);
                            }
                        }
                        _ => break,
                    }
                    i += 1;
                }
            }
            _ => {}
        }

        // Auto-detect triggers from function presence if not declared
        if triggers.is_empty() {
            if lua.globals().get::<LuaFunction>("on_keypress").is_ok() {
                triggers.push(TriggerMode::Keypress);
            }
            if lua.globals().get::<LuaFunction>("on_newline").is_ok() {
                triggers.push(TriggerMode::Newline);
            }
            if lua.globals().get::<LuaFunction>("on_frame").is_ok() {
                triggers.push(TriggerMode::Fullscreen);
            }
        }

        if triggers.is_empty() {
            anyhow::bail!("Particle plugin '{}' has no trigger functions (on_keypress / on_newline / on_frame)", id);
        }

        Ok(Some(Box::new(LuaParticlePlugin {
            plugin_id: id, plugin_display_name: display_name, schema,
            lua_source: source.to_string(),
            triggers,
        })))
    }

    pub fn shader_plugins(&self) -> &[Box<dyn ShaderPlugin>] { &self.shader_plugins }
    pub fn particle_plugins(&self) -> &[Box<dyn ParticlePlugin>] { &self.particle_plugins }

    pub fn get_shader_plugin(&self, id: &str) -> Option<&dyn ShaderPlugin> {
        self.shader_plugins.iter().find(|p| p.id() == id).map(|p| p.as_ref())
    }
    pub fn get_particle_plugin(&self, id: &str) -> Option<&dyn ParticlePlugin> {
        self.particle_plugins.iter().find(|p| p.id() == id).map(|p| p.as_ref())
    }

    pub fn reload(&mut self) -> Result<usize> {
        self.shader_plugins.retain(|p| matches!(p.id(), "fire" | "crt"));
        self.particle_plugins.clear(); // All particles are Lua-driven now
        self.load_user_plugins()
    }

    pub fn find_shaders_dir() -> PathBuf {
        let candidates = [
            std::env::current_exe().ok()
                .and_then(|p| p.parent().map(|d| d.join("shaders"))),
            Some(PathBuf::from("crates/ridgeback-gpu/shaders")),
            Some(PathBuf::from("shaders")),
        ];
        for c in candidates.into_iter().flatten() {
            if c.exists() { return c; }
        }
        PathBuf::from("shaders")
    }

    pub fn find_plugins_dir() -> PathBuf {
        if let Some(dirs) = directories::ProjectDirs::from("", "", "Ridgeback") {
            let p = dirs.config_dir().join("plugins");
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
        PathBuf::from("plugins")
    }

    /// Locate the bundled plugins directory shipped with the application.
    pub fn find_bundled_plugins_dir() -> PathBuf {
        let candidates = [
            // Next to the executable (release / installed)
            std::env::current_exe().ok()
                .and_then(|p| p.parent().map(|d| d.join("plugins"))),
            // macOS .app bundle: Contents/Resources/plugins
            std::env::current_exe().ok()
                .and_then(|p| p.parent().and_then(|d| d.parent()).map(|d| d.join("Resources").join("plugins"))),
            // Dev: assets/plugins relative to working directory
            Some(PathBuf::from("assets/plugins")),
        ];
        for c in candidates.into_iter().flatten() {
            if c.exists() { return c; }
        }
        PathBuf::from("assets/plugins")
    }
}

fn parse_trigger_str(s: &str) -> Option<TriggerMode> {
    match s.trim().to_lowercase().as_str() {
        "keypress" | "typing" => Some(TriggerMode::Keypress),
        "newline" | "enter" => Some(TriggerMode::Newline),
        "fullscreen" | "ambient" | "frame" => Some(TriggerMode::Fullscreen),
        _ => None,
    }
}

fn parse_param_schema(plugin_tbl: &LuaTable) -> Result<Vec<ParamDescriptor>> {
    let params_val: LuaValue = plugin_tbl.get("params").unwrap_or(LuaValue::Nil);
    let params_tbl = match params_val {
        LuaValue::Table(t) => t,
        _ => return Ok(Vec::new()),
    };
    let mut schema = Vec::new();
    let mut idx = 1i64;
    loop {
        let entry: LuaValue = match params_tbl.get(idx) {
            Ok(v) => v,
            Err(_) => break,
        };
        let tbl = match entry { LuaValue::Table(t) => t, _ => break };
        let key: String = tbl.get("key")?;
        let label: String = tbl.get("label").unwrap_or_else(|_| key.clone());
        let type_str: String = tbl.get("type").unwrap_or_else(|_| "float".to_string());
        let hint: Option<String> = tbl.get("hint").ok();
        let (ty, default) = match type_str.as_str() {
            "color" => {
                let def: String = tbl.get("default").unwrap_or_else(|_| "#ffffff".to_string());
                (ParamType::Color, serde_json::json!(def))
            }
            "bool" => {
                let def: bool = tbl.get("default").unwrap_or(false);
                (ParamType::Bool, serde_json::json!(def))
            }
            "int" => {
                let min: i64 = tbl.get("min").unwrap_or(0);
                let max: i64 = tbl.get("max").unwrap_or(100);
                let def: i64 = tbl.get("default").unwrap_or(0);
                (ParamType::Int { min, max }, serde_json::json!(def))
            }
            _ => {
                let min: f64 = tbl.get("min").unwrap_or(0.0);
                let max: f64 = tbl.get("max").unwrap_or(1.0);
                let def: f64 = tbl.get("default").unwrap_or(0.0);
                (ParamType::Float { min: min as f32, max: max as f32 }, serde_json::json!(def))
            }
        };
        schema.push(ParamDescriptor { key, label, ty, default, hint });
        idx += 1;
    }
    Ok(schema)
}

