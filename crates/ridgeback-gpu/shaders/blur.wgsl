// blur.wgsl — Dual Kawase blur for bloom
//
// Two passes: downsample and upsample. Run alternately at
// decreasing/increasing resolutions to produce a smooth bloom.

struct BlurUniforms {
    texel_size: vec2<f32>,   // 1.0 / texture_dimensions
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> params: BlurUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
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

// Kawase downsample — samples 5 points
@fragment
fn fs_downsample(in: VertexOutput) -> @location(0) vec4<f32> {
    let hs = params.texel_size * 0.5;  // Half texel

    let center = textureSample(input_texture, tex_sampler, in.uv);
    let tl = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(-hs.x,  hs.y));
    let tr = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>( hs.x,  hs.y));
    let bl = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(-hs.x, -hs.y));
    let br = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>( hs.x, -hs.y));

    return (center * 4.0 + tl + tr + bl + br) / 8.0;
}

// Kawase upsample — samples 8 points
@fragment
fn fs_upsample(in: VertexOutput) -> @location(0) vec4<f32> {
    let ts = params.texel_size;

    let sample0 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(-ts.x,  ts.y));
    let sample1 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(  0.0,  ts.y)) * 2.0;
    let sample2 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>( ts.x,  ts.y));
    let sample3 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(-ts.x,   0.0)) * 2.0;
    let sample4 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>( ts.x,   0.0)) * 2.0;
    let sample5 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(-ts.x, -ts.y));
    let sample6 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>(  0.0, -ts.y)) * 2.0;
    let sample7 = textureSample(input_texture, tex_sampler, in.uv + vec2<f32>( ts.x, -ts.y));

    return (sample0 + sample1 + sample2 + sample3 + sample4 + sample5 + sample6 + sample7) / 12.0;
}
