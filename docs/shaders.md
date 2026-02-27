# Shader Effects

Ridgeback renders the terminal viewport through a custom **wgpu multi-pass shader pipeline**, enabling real-time visual effects without sacrificing text clarity. Shaders are written in **WGSL** (WebGPU Shading Language) and run on the GPU.

---

## Pipeline Overview

The rendering pipeline runs once per frame and consists of the following ordered passes:

```
┌─────────────┐
│ Glyph Atlas │  (fontdue rasterization, packed with etagere)
└──────┬──────┘
       │
       ▼
┌──────────────────┐
│ 1. Background    │  Fire shader / solid fill → background_texture
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│ 2. Text Render   │  Instanced quads sampling glyph atlas → text_texture
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│ 3. Shadow Pass   │  Darkened copy beneath text (fire mode) → shadow_texture
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│ 4. Composite     │  Layer: background + shadow + text → composite_texture
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│ 5. Bloom / Blur  │  Dual Kawase blur on bright pixels → bloom_texture
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│ 6. Post-Process  │  CRT (scanlines, curvature, aberration) → final_texture
└──────────────────┘
       │
       ▼
  egui::Image (displayed in the terminal viewport)
```

Each pass writes to its own offscreen texture. The final texture is uploaded to an egui `TextureHandle` and drawn as an image widget in the terminal panel.

---

## Shader Effect: CRT

The CRT post-process shader simulates a cathode ray tube display. It is applied as the final pass after compositing and bloom.

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `scanline_intensity` | `f32` | `0.15` | Darkness of horizontal scanlines (0 = off, 1 = fully black) |
| `curvature` | `f32` | `0.03` | Barrel distortion strength (0 = flat, 0.1 = heavy curve) |
| `bloom_strength` | `f32` | `0.3` | Intensity of the bloom/glow around bright text |
| `chromatic_aberration` | `f32` | `0.002` | RGB channel offset in UV space |

### Techniques

- **Scanlines**: Horizontal darkening bands using `sin(uv.y * rows * PI)`, modulated by `scanline_intensity`.
- **Barrel distortion**: UV coordinates are warped outward from center using a quadratic function scaled by `curvature`.
- **Chromatic aberration**: The R, G, and B channels are sampled at slightly offset UV coordinates, creating color fringing near edges.
- **Vignette**: Corners are darkened with a radial falloff as a side-effect of barrel distortion clamping.
- **Phosphor glow**: The bloom pass provides a soft glow around bright characters, simulating phosphor persistence.

### Config

```toml
[profiles.powershell]
shader_effect = "crt"

[profiles.powershell.shader_params]
scanline_intensity = 0.15
curvature = 0.03
bloom_strength = 0.3
chromatic_aberration = 0.002
```

---

## Shader Effect: Fire

The fire background shader renders an animated procedural fire beneath the terminal text. Characters that were recently typed generate "heat" in the fire simulation, making them appear to ignite.

### Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `fire_intensity` | `f32` | `0.6` | Overall brightness/scale of the fire effect |
| `fire_decay_rate` | `f32` | `0.95` | How quickly heat dissipates per frame (0.9 = fast, 0.99 = slow) |
| `fire_spread` | `f32` | `1.0` | How far heat spreads to neighboring cells |

### Techniques

- **Heat map**: The VT handler tracks "hot cells" — grid positions where new characters appeared this frame. These cells are written into a heat map texture with high values.
- **Diffusion**: Each frame, the heat map is blurred and decayed, spreading warmth upward and outward (biased toward the top, simulating convection).
- **Color ramp**: Heat values are mapped to a fire color palette: black → deep red → orange → yellow → white.
- **Text shadows**: A darkened copy of the text is rendered beneath the actual glyphs to ensure readability against the bright fire. The shadow is offset by 1-2 pixels downward.
- **Compositing**: Fire background → text shadow → text foreground, blended with alpha.

### Config

```toml
[profiles.powershell]
shader_effect = "fire"

[profiles.powershell.shader_params]
fire_intensity = 0.6
fire_decay_rate = 0.95
fire_spread = 1.0
```

---

## Shader Effect: None

When set to `"none"`, the pipeline skips the background and post-process passes entirely. Text is rendered with instanced quads on a solid background color. This is the most power-efficient mode.

```toml
[profiles.powershell]
shader_effect = "none"
```

---

## Performance & Battery

Shader rendering is governed by the `[rendering]` section of the config:

```toml
[rendering]
update_in_background = true   # Keep rendering when window is unfocused
max_shader_fps = 144           # Cap shader/animation frame rate
battery_aware = true           # Throttle on battery power
```

When `battery_aware` is enabled and the system is on battery:
- Shader FPS is capped at 30
- Fire/CRT effects are reduced in complexity
- Idle terminals drop to ~10 FPS

The **FramePacer** (`ridgeback-gpu::frame_pacer`) controls all timing. It detects:
- Window focus state (via the OS)
- Battery/AC status (via `GetSystemPowerStatus` on Windows)
- Terminal activity (PTY output recency)

---

## WGSL Shader Files

Shader source files are located in `crates/ridgeback-gpu/shaders/`:

| File | Purpose |
|---|---|
| `text.wgsl` | Instanced glyph quad vertex + fragment shader |
| `fire.wgsl` | Fire background compute/fragment shader with heat diffusion |
| `crt.wgsl` | CRT post-process (scanlines, curvature, aberration) |
| `shadow.wgsl` | Text shadow pass with offset and darkening |
| `blur.wgsl` | Dual Kawase blur for bloom |
| `composite.wgsl` | Layer blending (background + shadow + text + bloom) |

---

## Writing Custom Shaders

To add a new post-process effect:

1. Create a new `.wgsl` file in `crates/ridgeback-gpu/shaders/`
2. Add a variant to the `ShaderEffect` enum in `ridgeback-config/src/profile.rs`
3. Register the shader pass in `ridgeback-gpu/src/shader_pipeline.rs`
4. Add parameters to `ShaderParams` and wire them into the uniform buffer

The pipeline is designed to be extensible — each pass reads from the previous pass's output texture and writes to its own. You can insert passes at any point in the chain.
