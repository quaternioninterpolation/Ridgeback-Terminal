// fire.wgsl — Fire background shader with heat diffusion
//
// Two entry points:
//   - cs_diffuse: Compute shader that diffuses + decays the heat map
//   - fs_render: Fragment shader that maps heat values to fire colors

struct FireUniforms {
    viewport_size: vec2<f32>,
    fire_intensity: f32,
    fire_decay_rate: f32,
    fire_spread: f32,
    time: f32,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> params: FireUniforms;

// ── Heat diffusion compute pass ──────────────────────────────────────

@group(0) @binding(1) var heat_in: texture_2d<f32>;
@group(0) @binding(2) var heat_out: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn cs_diffuse(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(heat_in);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }

    let coord = vec2<i32>(i32(gid.x), i32(gid.y));

    // Sample current + neighbors (biased upward for convection)
    let center = textureLoad(heat_in, coord, 0).r;
    let left   = textureLoad(heat_in, coord + vec2<i32>(-1, 0), 0).r;
    let right  = textureLoad(heat_in, coord + vec2<i32>( 1, 0), 0).r;
    let above  = textureLoad(heat_in, coord + vec2<i32>( 0,-1), 0).r;
    let below  = textureLoad(heat_in, coord + vec2<i32>( 0, 1), 0).r;

    // Weighted average biased upward (heat rises)
    let spread = params.fire_spread * 0.25;
    let diffused = center * (1.0 - spread * 4.0)
                 + left   * spread
                 + right  * spread
                 + above  * spread * 0.5   // Less contribution from above
                 + below  * spread * 1.5;  // More from below (heat rises)

    // Decay
    let result = diffused * (1.0 - params.fire_decay_rate);

    textureStore(heat_out, coord, vec4<f32>(result, 0.0, 0.0, 1.0));
}

// ── Fire color rendering fragment pass ────────────────────────────────

@group(0) @binding(3) var heat_texture: texture_2d<f32>;
@group(0) @binding(4) var heat_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Full-screen triangle vertex shader
@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    // Generate a full-screen triangle from vertex index
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// Fire color ramp: black → dark red → orange → yellow → white
fn fire_color(heat: f32) -> vec3<f32> {
    let h = clamp(heat * params.fire_intensity, 0.0, 1.0);

    if h < 0.25 {
        let t = h / 0.25;
        return mix(vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(0.5, 0.0, 0.0), t);
    } else if h < 0.5 {
        let t = (h - 0.25) / 0.25;
        return mix(vec3<f32>(0.5, 0.0, 0.0), vec3<f32>(1.0, 0.4, 0.0), t);
    } else if h < 0.75 {
        let t = (h - 0.5) / 0.25;
        return mix(vec3<f32>(1.0, 0.4, 0.0), vec3<f32>(1.0, 0.9, 0.2), t);
    } else {
        let t = (h - 0.75) / 0.25;
        return mix(vec3<f32>(1.0, 0.9, 0.2), vec3<f32>(1.0, 1.0, 1.0), t);
    }
}

@fragment
fn fs_render(in: VertexOutput) -> @location(0) vec4<f32> {
    let heat = textureSample(heat_texture, heat_sampler, in.uv).r;
    let color = fire_color(heat);
    return vec4<f32>(color, 1.0);
}
