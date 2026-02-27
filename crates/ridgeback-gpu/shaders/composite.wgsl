// composite.wgsl — Layer blending shader
//
// Composites: background + shadow + text layers into a single texture.

struct CompositeUniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> params: CompositeUniforms;
@group(0) @binding(1) var bg_texture: texture_2d<f32>;     // Background (fire or solid)
@group(0) @binding(2) var shadow_texture: texture_2d<f32>; // Text shadow
@group(0) @binding(3) var text_texture: texture_2d<f32>;   // Text foreground
@group(0) @binding(4) var tex_sampler: sampler;

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

// Alpha-over compositing: A over B
fn alpha_over(below: vec4<f32>, above: vec4<f32>) -> vec4<f32> {
    let out_a = above.a + below.a * (1.0 - above.a);
    if out_a < 0.001 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let out_rgb = (above.rgb * above.a + below.rgb * below.a * (1.0 - above.a)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let bg = textureSample(bg_texture, tex_sampler, in.uv);
    let shadow = textureSample(shadow_texture, tex_sampler, in.uv);
    let text = textureSample(text_texture, tex_sampler, in.uv);

    // Layer: background → shadow → text
    var result = alpha_over(bg, shadow);
    result = alpha_over(result, text);

    return result;
}
