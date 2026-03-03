pub mod api;
pub mod lua_host;
pub mod shader_plugin;
pub mod shader_host;

pub use api::{TerminalQuery, SearchMatch as PluginSearchMatch, SaveFormatPlugin, StyledLine, StyledSpan};
pub use lua_host::{LuaPluginHost, PluginScript, PluginResult, TerminalSnapshot};
pub use lua_host::{HtmlExporter, MarkdownExporter, JsonExporter};
pub use shader_plugin::{
    ParamDescriptor, ParamType, ParticleEvent, TriggerMode,
    ShaderPlugin, ParticlePlugin,
    BuiltinFireShaderPlugin, BuiltinCrtShaderPlugin,
};
pub use shader_host::ShaderPluginHost;
