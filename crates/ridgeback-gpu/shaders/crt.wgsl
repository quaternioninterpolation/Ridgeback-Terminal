// crt.wgsl — CRT post-process shader
//
// Simulates a CRT display with scanlines, barrel distortion,
// chromatic aberration, and vignette.

struct CrtUniforms {
    viewport_size: vec2<f32>,
    scanline_intensity: f32,
    curvature: f32,
    bloom_strength: f32,
    chromatic_aberration: f32,
    time: f32,
    _padding: f32,
};

@group(0) @binding(0) var<uniform> params: CrtUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var bloom_texture: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

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

// Barrel distortion — warps UV outward from center
fn barrel_distort(uv: vec2<f32>, amount: f32) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    let r2 = dot(centered, centered);
    let distorted = centered * (1.0 + amount * r2);
    return distorted + vec2<f32>(0.5, 0.5);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Apply barrel distortion
    let uv = barrel_distort(in.uv, params.curvature);

    // Discard pixels outside the barrel
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Chromatic aberration — offset R and B channels
    let ca = params.chromatic_aberration;
    let dir = (uv - vec2<f32>(0.5, 0.5)) * ca;
    let r = textureSample(input_texture, tex_sampler, uv + dir).r;
    let g = textureSample(input_texture, tex_sampler, uv).g;
    let b = textureSample(input_texture, tex_sampler, uv - dir).b;
    var color = vec3<f32>(r, g, b);

    // Add bloom
    let bloom = textureSample(bloom_texture, tex_sampler, uv).rgb;
    color = color + bloom * params.bloom_strength;

    // Scanlines — horizontal darkening bands
    let scanline = sin(uv.y * params.viewport_size.y * 3.14159) * 0.5 + 0.5;
    let scanline_factor = 1.0 - params.scanline_intensity * (1.0 - scanline);
    color = color * scanline_factor;

    // Vignette — darken edges (natural side-effect of barrel distortion)
    let centered_uv = uv - vec2<f32>(0.5, 0.5);
    let vignette = 1.0 - dot(centered_uv, centered_uv) * 2.0;
    let vignette_clamped = clamp(vignette, 0.0, 1.0);
    color = color * vignette_clamped;

    return vec4<f32>(color, 1.0);
}
