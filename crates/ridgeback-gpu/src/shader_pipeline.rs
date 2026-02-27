//! Multi-pass wgpu shader pipeline for the terminal viewport.
//!
//! Pipeline order:
//!   1. Background (fire shader or solid fill)
//!   2. Text render (instanced glyph quads)
//!   3. Shadow pass (offset darkened text copy, fire mode only)
//!   4. Composite (layer background + shadow + text)
//!   5. Bloom/blur (dual Kawase, optional)
//!   6. Post-process (CRT scanlines/curvature/aberration, optional)
//!
//! The final output is written to a texture that the app reads back as an
//! `egui::TextureHandle` for display.

use std::sync::Arc;
use wgpu::util::DeviceExt;
use ridgeback_config::{ShaderEffect, ShaderParams};
use crate::glyph_atlas::GlyphAtlas;

// ── Per-instance vertex data for text rendering ────────────────────────

/// Instance data sent to the GPU for each glyph.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphInstance {
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_size: [f32; 2],
    pub fg_color: [f32; 4],
    pub bg_color: [f32; 4],
}

/// Uniform buffer shared across passes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniforms {
    viewport_size: [f32; 2],
    _padding: [f32; 2],
}

/// CRT post-process uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrtUniforms {
    viewport_size: [f32; 2],
    scanline_intensity: f32,
    curvature: f32,
    bloom_strength: f32,
    chromatic_aberration: f32,
    time: f32,
    _padding: f32,
}

/// Fire shader uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FireUniforms {
    viewport_size: [f32; 2],
    fire_intensity: f32,
    fire_decay_rate: f32,
    fire_spread: f32,
    time: f32,
    _padding: [f32; 2],
}

/// Shadow pass uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowUniforms {
    viewport_size: [f32; 2],
    shadow_offset: [f32; 2],
    shadow_opacity: f32,
    _padding: [f32; 3],
}

/// Blur pass uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniforms {
    texel_size: [f32; 2],
    _padding: [f32; 2],
}

/// Composite pass uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeUniforms {
    viewport_size: [f32; 2],
    _padding: [f32; 2],
}

// ── Offscreen texture helper ───────────────────────────────────────────

fn create_offscreen_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

// ── The main pipeline ──────────────────────────────────────────────────

/// Complete wgpu multi-pass shader pipeline.
pub struct ShaderPipeline {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    width: u32,
    height: u32,

    // Textures
    text_texture: wgpu::Texture,
    text_view: wgpu::TextureView,
    bg_texture: wgpu::Texture,
    bg_view: wgpu::TextureView,
    shadow_texture: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    composite_texture: wgpu::Texture,
    composite_view: wgpu::TextureView,
    bloom_texture: wgpu::Texture,
    bloom_view: wgpu::TextureView,
    final_texture: wgpu::Texture,
    final_view: wgpu::TextureView,

    // Atlas GPU texture
    atlas_texture: wgpu::Texture,
    atlas_view: wgpu::TextureView,

    // Sampler
    linear_sampler: wgpu::Sampler,

    // Pipelines
    text_pipeline: wgpu::RenderPipeline,
    text_bind_group_layout: wgpu::BindGroupLayout,
    text_uniform_buf: wgpu::Buffer,

    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_uniform_buf: wgpu::Buffer,

    blur_down_pipeline: wgpu::RenderPipeline,
    blur_up_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    blur_uniform_buf: wgpu::Buffer,

    crt_pipeline: Option<wgpu::RenderPipeline>,
    crt_bind_group_layout: Option<wgpu::BindGroupLayout>,
    crt_uniform_buf: Option<wgpu::Buffer>,

    fire_render_pipeline: Option<wgpu::RenderPipeline>,
    fire_bind_group_layout: Option<wgpu::BindGroupLayout>,
    fire_uniform_buf: Option<wgpu::Buffer>,

    shadow_pipeline: Option<wgpu::RenderPipeline>,
    shadow_bind_group_layout: Option<wgpu::BindGroupLayout>,
    shadow_uniform_buf: Option<wgpu::Buffer>,

    // Quad vertex buffer (unit quad for instanced rendering)
    quad_vbo: wgpu::Buffer,
    quad_index_buf: wgpu::Buffer,

    // Instance buffer (rebuilt each frame)
    instance_buf: wgpu::Buffer,
    instance_capacity: usize,

    // Current configuration
    pub effect: ShaderEffect,
    pub params: ShaderParams,
    time: f32,
}

// Vertex for the unit quad
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    position: [f32; 2],
}

const QUAD_VERTICES: [QuadVertex; 4] = [
    QuadVertex { position: [0.0, 0.0] }, // top-left
    QuadVertex { position: [1.0, 0.0] }, // top-right
    QuadVertex { position: [0.0, 1.0] }, // bottom-left
    QuadVertex { position: [1.0, 1.0] }, // bottom-right
];

const QUAD_INDICES: [u16; 6] = [0, 2, 1, 1, 2, 3];

impl ShaderPipeline {
    /// Create the full pipeline on the given device.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        effect: ShaderEffect,
        params: ShaderParams,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Self {
        let tex_format = wgpu::TextureFormat::Rgba8UnormSrgb;

        // Create offscreen textures
        let (text_texture, text_view) = create_offscreen_texture(&device, width, height, tex_format, "text");
        let (bg_texture, bg_view) = create_offscreen_texture(&device, width, height, tex_format, "background");
        let (shadow_texture, shadow_view) = create_offscreen_texture(&device, width, height, tex_format, "shadow");
        let (composite_texture, composite_view) = create_offscreen_texture(&device, width, height, tex_format, "composite");
        let (bloom_texture, bloom_view) = create_offscreen_texture(&device, width, height, tex_format, "bloom");
        let (final_texture, final_view) = create_offscreen_texture(&device, width, height, tex_format, "final");

        // Atlas texture (R8)
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d { width: atlas_width, height: atlas_height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Quad geometry
        let quad_vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vbo"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_indices"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Instance buffer (start with capacity for 8000 glyphs)
        let instance_capacity = 8000;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_instances"),
            size: (instance_capacity * std::mem::size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Text pipeline ──

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/text.wgsl").into()),
        });

        let text_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text_uniforms"),
            contents: bytemuck::cast_slice(&[ViewportUniforms {
                viewport_size: [width as f32, height as f32],
                _padding: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let text_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text_pipeline_layout"),
            bind_group_layouts: &[&text_bind_group_layout],
            push_constant_ranges: &[],
        });

        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text_pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Vertex buffer (unit quad)
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    },
                    // Instance buffer
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GlyphInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            1 => Float32x2, // offset
                            2 => Float32x2, // size
                            3 => Float32x2, // uv_offset
                            4 => Float32x2, // uv_size
                            5 => Float32x4, // fg_color
                            6 => Float32x4, // bg_color
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: tex_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Composite pipeline (fullscreen triangle) ──

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/composite.wgsl").into()),
        });

        let composite_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("composite_uniforms"),
            contents: bytemuck::cast_slice(&[CompositeUniforms {
                viewport_size: [width as f32, height as f32],
                _padding: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let composite_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // bg, shadow, text textures + sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let composite_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite_pipeline_layout"),
            bind_group_layouts: &[&composite_bind_group_layout],
            push_constant_ranges: &[],
        });

        let composite_pipeline = Self::create_fullscreen_pipeline(
            &device,
            &composite_shader,
            "fs_main",
            &composite_pipeline_layout,
            tex_format,
            "composite_pipeline",
        );

        // ── Blur pipelines ──

        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/blur.wgsl").into()),
        });

        let blur_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur_uniforms"),
            contents: bytemuck::cast_slice(&[BlurUniforms {
                texel_size: [1.0 / width as f32, 1.0 / height as f32],
                _padding: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let blur_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pipeline_layout"),
            bind_group_layouts: &[&blur_bind_group_layout],
            push_constant_ranges: &[],
        });

        let blur_down_pipeline = Self::create_fullscreen_pipeline(
            &device,
            &blur_shader,
            "fs_downsample",
            &blur_pipeline_layout,
            tex_format,
            "blur_down",
        );

        let blur_up_pipeline = Self::create_fullscreen_pipeline(
            &device,
            &blur_shader,
            "fs_upsample",
            &blur_pipeline_layout,
            tex_format,
            "blur_up",
        );

        // ── Optional CRT pipeline ──

        let (crt_pipeline, crt_bind_group_layout, crt_uniform_buf) = if effect == ShaderEffect::Crt {
            let (p, bgl, buf) = Self::create_crt_pipeline(&device, width, height, &params, tex_format);
            (Some(p), Some(bgl), Some(buf))
        } else {
            (None, None, None)
        };

        // ── Optional fire pipeline ──

        let (fire_render_pipeline, fire_bind_group_layout, fire_uniform_buf) = if effect == ShaderEffect::Fire {
            let (p, bgl, buf) = Self::create_fire_render_pipeline(&device, width, height, &params, tex_format);
            (Some(p), Some(bgl), Some(buf))
        } else {
            (None, None, None)
        };

        // ── Optional shadow pipeline ──

        let (shadow_pipeline, shadow_bind_group_layout, shadow_uniform_buf) = if effect == ShaderEffect::Fire {
            let (p, bgl, buf) = Self::create_shadow_pipeline(&device, width, height, tex_format);
            (Some(p), Some(bgl), Some(buf))
        } else {
            (None, None, None)
        };

        Self {
            device,
            queue,
            width,
            height,
            text_texture,
            text_view,
            bg_texture,
            bg_view,
            shadow_texture,
            shadow_view,
            composite_texture,
            composite_view,
            bloom_texture,
            bloom_view,
            final_texture,
            final_view,
            atlas_texture,
            atlas_view,
            linear_sampler,
            text_pipeline,
            text_bind_group_layout,
            text_uniform_buf,
            composite_pipeline,
            composite_bind_group_layout,
            composite_uniform_buf,
            blur_down_pipeline,
            blur_up_pipeline,
            blur_bind_group_layout,
            blur_uniform_buf,
            crt_pipeline,
            crt_bind_group_layout,
            crt_uniform_buf,
            fire_render_pipeline,
            fire_bind_group_layout,
            fire_uniform_buf,
            shadow_pipeline,
            shadow_bind_group_layout,
            shadow_uniform_buf,
            quad_vbo,
            quad_index_buf,
            instance_buf,
            instance_capacity,
            effect,
            params,
            time: 0.0,
        }
    }

    // ── Helper: create a fullscreen triangle render pipeline ──

    fn create_fullscreen_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        fs_entry: &str,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    fn create_crt_pipeline(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        params: &ShaderParams,
        format: wgpu::TextureFormat,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Buffer) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crt_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/crt.wgsl").into()),
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("crt_uniforms"),
            contents: bytemuck::cast_slice(&[CrtUniforms {
                viewport_size: [width as f32, height as f32],
                scanline_intensity: params.scanline_intensity,
                curvature: params.curvature,
                bloom_strength: params.bloom_strength,
                chromatic_aberration: params.chromatic_aberration,
                time: 0.0,
                _padding: 0.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crt_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crt_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = Self::create_fullscreen_pipeline(device, &shader, "fs_main", &layout, format, "crt");

        (pipeline, bgl, uniform_buf)
    }

    fn create_fire_render_pipeline(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        params: &ShaderParams,
        format: wgpu::TextureFormat,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Buffer) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fire_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/fire.wgsl").into()),
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fire_uniforms"),
            contents: bytemuck::cast_slice(&[FireUniforms {
                viewport_size: [width as f32, height as f32],
                fire_intensity: params.fire_intensity,
                fire_decay_rate: params.fire_decay_rate,
                fire_spread: params.fire_spread,
                time: 0.0,
                _padding: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fire_render_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // heat_texture (binding 3 in shader, but we map to 1 here)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fire_render_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = Self::create_fullscreen_pipeline(device, &shader, "fs_render", &layout, format, "fire_render");

        (pipeline, bgl, uniform_buf)
    }

    fn create_shadow_pipeline(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Buffer) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shadow.wgsl").into()),
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shadow_uniforms"),
            contents: bytemuck::cast_slice(&[ShadowUniforms {
                viewport_size: [width as f32, height as f32],
                shadow_offset: [1.5, 1.5],
                shadow_opacity: 0.7,
                _padding: [0.0; 3],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = Self::create_fullscreen_pipeline(device, &shader, "fs_main", &layout, format, "shadow");

        (pipeline, bgl, uniform_buf)
    }

    // ── Public API ─────────────────────────────────────────────────────

    /// Upload the glyph atlas pixel data to the GPU texture.
    pub fn upload_atlas(&self, atlas: &GlyphAtlas) {
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width),
                rows_per_image: Some(atlas.height),
            },
            wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Upload glyph instances for this frame.
    pub fn upload_instances(&mut self, instances: &[GlyphInstance]) {
        // Grow buffer if needed
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph_instances"),
                size: (self.instance_capacity * std::mem::size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        self.queue.write_buffer(
            &self.instance_buf,
            0,
            bytemuck::cast_slice(instances),
        );
    }

    /// Advance time (call each frame with delta seconds).
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
    }

    /// Resize all offscreen textures.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;

        let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;

        let (t, v) = create_offscreen_texture(&self.device, width, height, fmt, "text");
        self.text_texture = t; self.text_view = v;
        let (t, v) = create_offscreen_texture(&self.device, width, height, fmt, "background");
        self.bg_texture = t; self.bg_view = v;
        let (t, v) = create_offscreen_texture(&self.device, width, height, fmt, "shadow");
        self.shadow_texture = t; self.shadow_view = v;
        let (t, v) = create_offscreen_texture(&self.device, width, height, fmt, "composite");
        self.composite_texture = t; self.composite_view = v;
        let (t, v) = create_offscreen_texture(&self.device, width, height, fmt, "bloom");
        self.bloom_texture = t; self.bloom_view = v;
        let (t, v) = create_offscreen_texture(&self.device, width, height, fmt, "final");
        self.final_texture = t; self.final_view = v;
    }

    /// Execute the full multi-pass pipeline and return the final texture for readback.
    pub fn render_frame(
        &mut self,
        instances: &[GlyphInstance],
        bg_color: [f32; 4],
    ) -> &wgpu::Texture {
        self.upload_instances(instances);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("shader_pipeline_encoder"),
        });

        // Pass 1: Background
        self.render_background(&mut encoder, bg_color);

        // Pass 2: Text render
        self.render_text(&mut encoder, instances.len() as u32);

        // Pass 3: Shadow (fire mode only)
        if self.effect == ShaderEffect::Fire {
            self.render_shadow(&mut encoder);
        }

        // Pass 4: Composite
        self.render_composite(&mut encoder);

        // Pass 5: Bloom/blur
        self.render_bloom(&mut encoder);

        // Pass 6: Post-process (CRT)
        match self.effect {
            ShaderEffect::Crt => self.render_crt_postprocess(&mut encoder),
            _ => self.copy_to_final(&mut encoder),
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        &self.final_texture
    }

    /// Get the final texture view for egui integration.
    pub fn final_texture_view(&self) -> &wgpu::TextureView {
        &self.final_view
    }

    /// Get output dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    // ── Private render passes ──────────────────────────────────────────

    fn render_background(&self, encoder: &mut wgpu::CommandEncoder, bg_color: [f32; 4]) {
        // For fire mode, we'd render the fire shader here.
        // For none/crt, just clear with the background color.
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bg_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.bg_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: bg_color[0] as f64,
                        g: bg_color[1] as f64,
                        b: bg_color[2] as f64,
                        a: bg_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        // If fire, we would set the fire pipeline and draw here.
        // For now the fire pipeline bind groups would be set and a fullscreen
        // triangle drawn. The compute diffusion pass would run beforehand.
    }

    fn render_text(&self, encoder: &mut wgpu::CommandEncoder, instance_count: u32) {
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text_bg"),
            layout: &self.text_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.text_uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.text_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(&self.text_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vbo.slice(..));
        pass.set_vertex_buffer(1, self.instance_buf.slice(..));
        pass.set_index_buffer(self.quad_index_buf.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..instance_count);
    }

    fn render_shadow(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(pipeline), Some(bgl), Some(ubuf)) =
            (&self.shadow_pipeline, &self.shadow_bind_group_layout, &self.shadow_uniform_buf)
        else { return; };

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow_bg"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.text_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.linear_sampler) },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.shadow_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1); // Fullscreen triangle
    }

    fn render_composite(&self, encoder: &mut wgpu::CommandEncoder) {
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite_bg"),
            layout: &self.composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.composite_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.bg_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.shadow_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&self.text_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.linear_sampler) },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.composite_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn render_bloom(&self, encoder: &mut wgpu::CommandEncoder) {
        // Simple single-pass bloom: downsample composite into bloom texture
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_down_bg"),
            layout: &self.blur_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.blur_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.composite_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.linear_sampler) },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.bloom_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(&self.blur_down_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn render_crt_postprocess(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(pipeline), Some(bgl), Some(ubuf)) =
            (&self.crt_pipeline, &self.crt_bind_group_layout, &self.crt_uniform_buf)
        else {
            self.copy_to_final(encoder);
            return;
        };

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crt_bg"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.composite_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.bloom_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.linear_sampler) },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("crt_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.final_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn copy_to_final(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &self.composite_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &self.final_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Update shader parameters at runtime.
    pub fn update_params(&mut self, params: &ShaderParams) {
        self.params = params.clone();

        // Update CRT uniforms if active
        if let Some(ref buf) = self.crt_uniform_buf {
            self.queue.write_buffer(
                buf,
                0,
                bytemuck::cast_slice(&[CrtUniforms {
                    viewport_size: [self.width as f32, self.height as f32],
                    scanline_intensity: params.scanline_intensity,
                    curvature: params.curvature,
                    bloom_strength: params.bloom_strength,
                    chromatic_aberration: params.chromatic_aberration,
                    time: self.time,
                    _padding: 0.0,
                }]),
            );
        }

        // Update fire uniforms if active
        if let Some(ref buf) = self.fire_uniform_buf {
            self.queue.write_buffer(
                buf,
                0,
                bytemuck::cast_slice(&[FireUniforms {
                    viewport_size: [self.width as f32, self.height as f32],
                    fire_intensity: params.fire_intensity,
                    fire_decay_rate: params.fire_decay_rate,
                    fire_spread: params.fire_spread,
                    time: self.time,
                    _padding: [0.0; 2],
                }]),
            );
        }
    }
}
