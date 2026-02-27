//! Ridgeback GPU rendering pipeline.
//!
//! This crate contains the wgpu-based multi-pass shader pipeline:
//! - Glyph atlas (fontdue rasterization, shelf-packed) and instanced text renderer
//! - Background shader passes (fire with heat diffusion)
//! - Post-process shader passes (CRT scanlines/curvature/aberration, bloom/blur)
//! - FramePacer for FPS capping and battery-aware rendering
//!
//! Pipeline order: Background → Text → Shadow → Composite → Bloom → Post-process
//! All shaders are written in WGSL and located in `shaders/`.

pub mod frame_pacer;
pub mod glyph_atlas;
pub mod shader_pipeline;

pub use frame_pacer::FramePacer;
pub use glyph_atlas::{GlyphAtlas, GlyphInfo};
pub use shader_pipeline::{ShaderPipeline, GlyphInstance};
