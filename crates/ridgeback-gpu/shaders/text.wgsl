// text.wgsl — Instanced glyph quad vertex + fragment shader
//
// Each glyph is an instanced quad. Per-instance data provides the glyph's
// screen position, atlas UV rectangle, and foreground/background colors.

struct VertexInput {
    @location(0) position: vec2<f32>,  // Unit quad corner (0,0) to (1,1)
};

struct InstanceInput {
    @location(1) offset: vec2<f32>,       // Screen-space top-left of the glyph cell
    @location(2) size: vec2<f32>,         // Cell size in pixels
    @location(3) uv_offset: vec2<f32>,    // Top-left UV in the glyph atlas
    @location(4) uv_size: vec2<f32>,      // UV extent of the glyph in the atlas
    @location(5) fg_color: vec4<f32>,     // Foreground (text) color
    @location(6) bg_color: vec4<f32>,     // Background color
};

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fg_color: vec4<f32>,
    @location(2) bg_color: vec4<f32>,
};

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    // Compute pixel position of this corner
    let pixel_pos = inst.offset + vert.position * inst.size;

    // Convert to NDC: [0, viewport] -> [-1, 1], flip Y
    let ndc = vec2<f32>(
        (pixel_pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pixel_pos.y / uniforms.viewport_size.y) * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = inst.uv_offset + vert.position * inst.uv_size;
    out.fg_color = inst.fg_color;
    out.bg_color = inst.bg_color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(atlas_texture, atlas_sampler, in.uv).r;

    // Blend: background behind glyph, foreground where glyph has coverage
    let color = mix(in.bg_color.rgb, in.fg_color.rgb, alpha);
    let out_alpha = in.bg_color.a + alpha * (1.0 - in.bg_color.a);

    return vec4<f32>(color, out_alpha);
}
