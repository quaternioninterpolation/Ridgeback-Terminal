// shadow.wgsl — Text shadow pass
//
// Renders a darkened, offset copy of the text for readability
// against bright fire backgrounds.

struct ShadowUniforms {
    viewport_size: vec2<f32>,
    shadow_offset: vec2<f32>,  // Pixel offset (typically 1-2px down-right)
    shadow_opacity: f32,
    _padding: vec3<f32>,
};

@group(0) @binding(0) var<uniform> params: ShadowUniforms;
@group(0) @binding(1) var text_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample text texture at offset position
    let offset_uv = in.uv - params.shadow_offset / params.viewport_size;
    let text_alpha = textureSample(text_texture, tex_sampler, offset_uv).a;

    // Dark shadow color with modulated alpha
    return vec4<f32>(0.0, 0.0, 0.0, text_alpha * params.shadow_opacity);
}
