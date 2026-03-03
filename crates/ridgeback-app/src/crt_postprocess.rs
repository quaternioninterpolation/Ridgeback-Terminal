//! CRT barrel-distortion post-process effect.
//!
//! Renders the terminal cell grid to a CPU pixel buffer using `fontdue` for
//! proper per-pixel glyph rasterization, uploads it as an egui texture, then
//! displays it through a barrel-distorted egui `Mesh`.
//!
//! This gives real per-pixel barrel distortion of the text content, with
//! scanlines and vignette baked into the mesh vertex colours.

use egui;
use std::collections::HashMap;
use std::cell::RefCell;

// ── Barrel distortion ─────────────────────────────────────────────────────────

/// Barrel-distort a normalised coordinate (0..1, 0..1) around the centre.
fn barrel_distort(nx: f32, ny: f32, amount: f32) -> (f32, f32) {
    let cx = nx - 0.5;
    let cy = ny - 0.5;
    let r2 = cx * cx + cy * cy;
    let scale = 1.0 + amount * r2;
    (cx * scale + 0.5, cy * scale + 0.5)
}

// ── Per-tab CRT state ─────────────────────────────────────────────────────────

pub struct CrtRasterState {
    pub texture: Option<egui::TextureHandle>,
    pub last_size: (usize, usize),
}

impl CrtRasterState {
    pub fn new() -> Self {
        Self { texture: None, last_size: (0, 0) }
    }
}

// ── Fontdue glyph cache ───────────────────────────────────────────────────────

struct FontCache {
    font: fontdue::Font,
    /// (char, font_size_in_tenths) → (metrics, coverage bitmap)
    glyphs: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl FontCache {
    fn new() -> Self {
        let font = Self::load_system_monospace()
            .expect("No monospace font found for CRT rasterization");
        Self { font, glyphs: HashMap::new() }
    }

    fn load_system_monospace() -> Option<fontdue::Font> {
        let candidates: &[&str] = &[
            // macOS
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/SFMono-Regular.otf",
            "/System/Library/Fonts/Monaco.dfont",
            "/Library/Fonts/Courier New.ttf",
            // Linux
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
            // Windows
            "C:\\Windows\\Fonts\\consola.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
        ];

        for path in candidates {
            if let Ok(data) = std::fs::read(path) {
                let settings = fontdue::FontSettings {
                    collection_index: 0,
                    scale: 40.0,
                    ..fontdue::FontSettings::default()
                };
                if let Ok(font) = fontdue::Font::from_bytes(data, settings) {
                    return Some(font);
                }
            }
        }
        None
    }

    fn rasterize(&mut self, ch: char, font_size: f32) -> (fontdue::Metrics, Vec<u8>) {
        let key = (ch, (font_size * 10.0) as u32);
        self.glyphs.entry(key).or_insert_with(|| {
            self.font.rasterize(ch, font_size)
        }).clone()
    }
}

thread_local! {
    static FONT_CACHE: RefCell<Option<FontCache>> = RefCell::new(None);
}

fn with_font_cache<R>(f: impl FnOnce(&mut FontCache) -> R) -> R {
    FONT_CACHE.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(FontCache::new());
        }
        f(opt.as_mut().unwrap())
    })
}

// ── Terminal rasterization ────────────────────────────────────────────────────

pub fn rasterize_terminal(
    ctx: &egui::Context,
    bg_color: egui::Color32,
    visible_cells: &[Vec<ridgeback_core::cell::Cell>],
    scrollback_lines: &[String],
    cursor_row: usize,
    cursor_col: usize,
    default_fg: egui::Color32,
    font_size: f32,
    width: usize,
    height: usize,
    _bg_texture: Option<&egui::TextureHandle>,
    padding: &ridgeback_config::TerminalPadding,
) -> egui::ColorImage {
    let char_w = ctx.fonts(|f| f.glyph_width(&egui::FontId::monospace(font_size), ' '));
    let line_h = ctx.fonts(|f| f.row_height(&egui::FontId::monospace(font_size)));

    let mut pixels = vec![bg_color; width * height];

    // Compute pixel padding offsets from percentages.
    // Percentages reference the smaller of width/height for uniform pixel padding.
    let min_dim = (width as f32).min(height as f32);
    let pad_left   = (min_dim * padding.left   / 100.0) as usize;
    let pad_top    = (min_dim * padding.top    / 100.0) as usize;
    let pad_right  = (min_dim * padding.right  / 100.0) as usize;
    let pad_bottom = (min_dim * padding.bottom / 100.0) as usize;

    // Content area dimensions after padding
    let _content_w = width.saturating_sub(pad_left + pad_right);
    let content_h = height.saturating_sub(pad_top + pad_bottom);

    let total_lines = scrollback_lines.len() + visible_cells.len();
    let max_visible_rows = (content_h as f32 / line_h).ceil() as usize;
    let start_line = total_lines.saturating_sub(max_visible_rows);

    for (view_row, abs_row) in (start_line..total_lines).enumerate() {
        let py = pad_top + (view_row as f32 * line_h) as usize;
        if py >= height.saturating_sub(pad_bottom) { break; }

        let is_scrollback = abs_row < scrollback_lines.len();

        if is_scrollback {
            let line = &scrollback_lines[abs_row];
            for (col, ch) in line.chars().enumerate() {
                if ch == ' ' || ch == '\0' { continue; }
                let px = pad_left + (col as f32 * char_w) as usize;
                rasterize_char(&mut pixels, width, height, px, py,
                               ch, default_fg, bg_color, font_size, line_h);
            }
        } else {
            let grid_row = abs_row - scrollback_lines.len();
            if grid_row >= visible_cells.len() { continue; }
            let row = &visible_cells[grid_row];

            for (col, cell) in row.iter().enumerate() {
                let px = pad_left + (col as f32 * char_w) as usize;
                let ch = if cell.ch == '\0' { ' ' } else { cell.ch };

                let cell_bg = cell_color_to_egui(cell.attrs.bg, bg_color);
                if cell_bg != bg_color {
                    fill_rect(&mut pixels, width, height, px, py,
                              char_w as usize + 1, line_h as usize, cell_bg);
                }

                let is_cursor = grid_row == cursor_row && col == cursor_col;
                if is_cursor {
                    fill_rect(&mut pixels, width, height, px, py,
                              char_w as usize + 1, line_h as usize,
                              egui::Color32::from_white_alpha(180));
                }

                if ch == ' ' { continue; }

                let fg = cell_color_to_egui(cell.attrs.fg, default_fg);
                let draw_fg = if is_cursor { bg_color } else { fg };
                let draw_bg = if is_cursor {
                    egui::Color32::from_white_alpha(180)
                } else if cell_bg != bg_color {
                    cell_bg
                } else {
                    bg_color
                };

                rasterize_char(&mut pixels, width, height, px, py,
                               ch, draw_fg, draw_bg, font_size, line_h);
            }
        }
    }

    egui::ColorImage { size: [width, height], pixels }
}

/// Rasterize a single character using fontdue with proper alpha coverage.
fn rasterize_char(
    pixels: &mut [egui::Color32],
    buf_w: usize,
    buf_h: usize,
    px: usize,
    py: usize,
    ch: char,
    fg: egui::Color32,
    _bg: egui::Color32,
    font_size: f32,
    line_h: f32,
) {
    let (metrics, bitmap) = with_font_cache(|cache| cache.rasterize(ch, font_size));

    if metrics.width == 0 || metrics.height == 0 { return; }

    // Position the glyph within the cell.
    // metrics.ymin is the offset from the baseline to the bottom of the glyph bbox.
    // The baseline sits at roughly py + ascent, where ascent ≈ 80% of line height.
    let ascent = (line_h * 0.8) as i32;
    let glyph_top = py as i32 + ascent - (metrics.height as i32 + metrics.ymin);
    let glyph_left = px as i32 + metrics.xmin;

    for gy in 0..metrics.height {
        for gx in 0..metrics.width {
            let coverage = bitmap[gy * metrics.width + gx];
            if coverage == 0 { continue; }

            let sx = glyph_left + gx as i32;
            let sy = glyph_top + gy as i32;
            if sx < 0 || sy < 0 { continue; }
            let sx = sx as usize;
            let sy = sy as usize;
            if sx >= buf_w || sy >= buf_h { continue; }

            let idx = sy * buf_w + sx;
            let alpha = coverage as f32 / 255.0;

            // Blend foreground over existing pixel using glyph coverage as alpha
            let dst = pixels[idx];
            let r = (fg.r() as f32 * alpha + dst.r() as f32 * (1.0 - alpha)) as u8;
            let g = (fg.g() as f32 * alpha + dst.g() as f32 * (1.0 - alpha)) as u8;
            let b = (fg.b() as f32 * alpha + dst.b() as f32 * (1.0 - alpha)) as u8;
            pixels[idx] = egui::Color32::from_rgb(r, g, b);
        }
    }
}

fn fill_rect(
    pixels: &mut [egui::Color32],
    buf_w: usize, buf_h: usize,
    x: usize, y: usize, w: usize, h: usize,
    color: egui::Color32,
) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < buf_w && py < buf_h {
                pixels[py * buf_w + px] = color;
            }
        }
    }
}

// ── Colour helpers ────────────────────────────────────────────────────────────

fn cell_color_to_egui(c: ridgeback_core::cell::Color, default: egui::Color32) -> egui::Color32 {
    match c {
        ridgeback_core::cell::Color::Default => default,
        ridgeback_core::cell::Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
        ridgeback_core::cell::Color::Indexed(idx) => ansi_index_to_color(idx),
    }
}

fn ansi_index_to_color(idx: u8) -> egui::Color32 {
    match idx {
        0  => egui::Color32::from_rgb(0x00, 0x00, 0x00),
        1  => egui::Color32::from_rgb(0xcd, 0x00, 0x00),
        2  => egui::Color32::from_rgb(0x00, 0xcd, 0x00),
        3  => egui::Color32::from_rgb(0xcd, 0xcd, 0x00),
        4  => egui::Color32::from_rgb(0x00, 0x00, 0xee),
        5  => egui::Color32::from_rgb(0xcd, 0x00, 0xcd),
        6  => egui::Color32::from_rgb(0x00, 0xcd, 0xcd),
        7  => egui::Color32::from_rgb(0xe5, 0xe5, 0xe5),
        8  => egui::Color32::from_rgb(0x7f, 0x7f, 0x7f),
        9  => egui::Color32::from_rgb(0xff, 0x00, 0x00),
        10 => egui::Color32::from_rgb(0x00, 0xff, 0x00),
        11 => egui::Color32::from_rgb(0xff, 0xff, 0x00),
        12 => egui::Color32::from_rgb(0x5c, 0x5c, 0xff),
        13 => egui::Color32::from_rgb(0xff, 0x00, 0xff),
        14 => egui::Color32::from_rgb(0x00, 0xff, 0xff),
        15 => egui::Color32::from_rgb(0xff, 0xff, 0xff),
        16..=231 => {
            let i = idx - 16;
            let r = (i / 36) % 6;
            let g = (i / 6) % 6;
            let b = i % 6;
            let to_val = |c: u8| if c == 0 { 0u8 } else { 55 + 40 * c };
            egui::Color32::from_rgb(to_val(r), to_val(g), to_val(b))
        }
        232..=255 => {
            let v = 8 + 10 * (idx - 232);
            egui::Color32::from_rgb(v, v, v)
        }
    }
}

// ── Barrel-distorted mesh ─────────────────────────────────────────────────────

pub fn build_crt_mesh(
    texture_id: egui::TextureId,
    rect: egui::Rect,
    curvature: f32,
    scanline_intensity: f32,
    vignette_strength: f32,
    tex_height: usize,
) -> egui::Mesh {
    let grid_x: usize = 64;
    let grid_y: usize = 64;
    let k = curvature * 4.0;

    let mut mesh = egui::Mesh::with_texture(texture_id);

    for iy in 0..=grid_y {
        for ix in 0..=grid_x {
            let nx = ix as f32 / grid_x as f32;
            let ny = iy as f32 / grid_y as f32;

            let uv = egui::pos2(nx, ny);

            let (dx, dy) = barrel_distort(nx, ny, k);
            let screen_pos = egui::pos2(
                rect.left() + dx * rect.width(),
                rect.top()  + dy * rect.height(),
            );

            // Vignette
            let cx = nx - 0.5;
            let cy = ny - 0.5;
            let r2 = cx * cx + cy * cy;
            let vignette = (1.0 - r2 * vignette_strength * 4.0).max(0.0);

            // Scanlines
            let tex_y = ny * tex_height as f32;
            let scanline = (tex_y * std::f32::consts::PI).sin() * 0.5 + 0.5;
            let scanline_factor = 1.0 - scanline_intensity * (1.0 - scanline);

            let brightness = (vignette * scanline_factor * 255.0).clamp(0.0, 255.0) as u8;

            mesh.vertices.push(egui::epaint::Vertex {
                pos: screen_pos,
                uv,
                color: egui::Color32::from_rgba_unmultiplied(brightness, brightness, brightness, 255),
            });
        }
    }

    for iy in 0..grid_y {
        for ix in 0..grid_x {
            let row_w = (grid_x + 1) as u32;
            let tl = (iy as u32) * row_w + ix as u32;
            let tr = tl + 1;
            let bl = tl + row_w;
            let br = bl + 1;
            mesh.indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }

    mesh
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub fn draw_crt_postprocess(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    effect: &ridgeback_config::ShaderEffectConfig,
    visible_cells: &[Vec<ridgeback_core::cell::Cell>],
    scrollback_lines: &[String],
    cursor_row: usize,
    cursor_col: usize,
    default_fg: egui::Color32,
    bg_color: egui::Color32,
    font_size: f32,
    bg_texture: Option<&egui::TextureHandle>,
    crt_state: &mut CrtRasterState,
    padding: &ridgeback_config::TerminalPadding,
) {
    let scanline_intensity = effect.param_f32("scanline_intensity", 0.3);
    let curvature          = effect.param_f32("curvature", 0.0);
    let bloom_strength     = effect.param_f32("bloom_strength", 0.0);

    let tex_w = (rect.width() as usize).max(1);
    let tex_h = (rect.height() as usize).max(1);

    let image = rasterize_terminal(
        ui.ctx(), bg_color, visible_cells, scrollback_lines,
        cursor_row, cursor_col, default_fg, font_size,
        tex_w, tex_h, bg_texture, padding,
    );

    let tex_opts = egui::TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        ..Default::default()
    };
    let needs_new = crt_state.texture.is_none() || crt_state.last_size != (tex_w, tex_h);
    if needs_new {
        crt_state.texture = Some(ui.ctx().load_texture("crt_terminal", image, tex_opts));
        crt_state.last_size = (tex_w, tex_h);
    } else if let Some(ref mut handle) = crt_state.texture {
        handle.set(image, tex_opts);
    }

    let texture_id = crt_state.texture.as_ref().unwrap().id();
    let painter = ui.painter_at(rect);

    // Black bezel background
    painter.rect_filled(rect, 0.0, egui::Color32::BLACK);

    // Barrel-distorted mesh
    let mesh = build_crt_mesh(
        texture_id, rect, curvature, scanline_intensity,
        curvature * 2.0,
        tex_h,
    );
    painter.add(egui::Shape::mesh(mesh));

    // Bloom tint
    let bloom_alpha = (bloom_strength * 18.0) as u8;
    if bloom_alpha > 0 {
        painter.rect_filled(rect, 0.0,
            egui::Color32::from_rgba_unmultiplied(0, 255, 80, bloom_alpha));
    }
}





