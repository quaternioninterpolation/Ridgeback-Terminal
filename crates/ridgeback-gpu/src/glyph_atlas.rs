//! Glyph atlas — rasterizes glyphs with fontdue and packs them into a GPU texture
//! using a shelf-based rectangle packer (etagere).
//!
//! The atlas is a single-channel (R8) texture. Each glyph is rasterized to a
//! coverage bitmap and placed into the atlas. A lookup table maps
//! `(char, font_size)` → `GlyphInfo` (UV rect + metrics).

use std::collections::HashMap;

/// Metrics for a single rasterized glyph in the atlas.
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// UV coordinates in the atlas texture (top-left).
    pub uv_x: f32,
    pub uv_y: f32,
    /// UV extent.
    pub uv_w: f32,
    pub uv_h: f32,
    /// Pixel dimensions of the rasterized glyph.
    pub width: u32,
    pub height: u32,
    /// Horizontal offset from the pen position.
    pub bearing_x: f32,
    /// Vertical offset from the baseline.
    pub bearing_y: f32,
    /// Horizontal advance to the next glyph.
    pub advance: f32,
}

/// Key for looking up a glyph in the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    ch: char,
    size_tenths: u32, // font size * 10, to avoid float hashing
}

/// A CPU-side glyph atlas that can be uploaded to a GPU texture.
pub struct GlyphAtlas {
    /// Raw pixel data (R8 format, row-major).
    pub pixels: Vec<u8>,
    /// Atlas texture dimensions.
    pub width: u32,
    pub height: u32,
    /// Font used for rasterization.
    font: fontdue::Font,
    /// Current packing cursor (simple shelf packer).
    shelf_x: u32,
    shelf_y: u32,
    shelf_height: u32,
    /// Glyph lookup cache.
    cache: HashMap<GlyphKey, GlyphInfo>,
    /// Whether the atlas texture has been modified since last upload.
    pub dirty: bool,
}

impl GlyphAtlas {
    /// Create a new atlas with the given dimensions and font data.
    pub fn new(width: u32, height: u32, font_data: &[u8]) -> Self {
        let font = fontdue::Font::from_bytes(
            font_data,
            fontdue::FontSettings::default(),
        )
        .expect("Failed to parse font data");

        Self {
            pixels: vec![0u8; (width * height) as usize],
            width,
            height,
            font,
            shelf_x: 0,
            shelf_y: 0,
            shelf_height: 0,
            cache: HashMap::new(),
            dirty: true,
        }
    }

    /// Get or rasterize a glyph. Returns its atlas info.
    pub fn get_glyph(&mut self, ch: char, font_size: f32) -> GlyphInfo {
        let key = GlyphKey {
            ch,
            size_tenths: (font_size * 10.0) as u32,
        };

        if let Some(&info) = self.cache.get(&key) {
            return info;
        }

        // Rasterize with fontdue
        let (metrics, bitmap) = self.font.rasterize(ch, font_size);

        let glyph_w = metrics.width as u32;
        let glyph_h = metrics.height as u32;

        // Simple shelf packing
        if glyph_w == 0 || glyph_h == 0 {
            // Whitespace or empty glyph
            let info = GlyphInfo {
                uv_x: 0.0,
                uv_y: 0.0,
                uv_w: 0.0,
                uv_h: 0.0,
                width: 0,
                height: 0,
                bearing_x: metrics.xmin as f32,
                bearing_y: metrics.ymin as f32,
                advance: metrics.advance_width,
            };
            self.cache.insert(key, info);
            return info;
        }

        // Check if we need a new shelf row
        let padding = 1u32;
        if self.shelf_x + glyph_w + padding > self.width {
            self.shelf_y += self.shelf_height + padding;
            self.shelf_x = 0;
            self.shelf_height = 0;
        }

        // Check if atlas is full
        if self.shelf_y + glyph_h + padding > self.height {
            tracing::warn!("Glyph atlas full — cannot fit '{}'", ch);
            let info = GlyphInfo {
                uv_x: 0.0,
                uv_y: 0.0,
                uv_w: 0.0,
                uv_h: 0.0,
                width: glyph_w,
                height: glyph_h,
                bearing_x: metrics.xmin as f32,
                bearing_y: metrics.ymin as f32,
                advance: metrics.advance_width,
            };
            self.cache.insert(key, info);
            return info;
        }

        // Copy bitmap into atlas
        let atlas_x = self.shelf_x;
        let atlas_y = self.shelf_y;

        for row in 0..glyph_h {
            for col in 0..glyph_w {
                let src_idx = (row * glyph_w + col) as usize;
                let dst_idx = ((atlas_y + row) * self.width + atlas_x + col) as usize;
                if src_idx < bitmap.len() && dst_idx < self.pixels.len() {
                    self.pixels[dst_idx] = bitmap[src_idx];
                }
            }
        }

        let info = GlyphInfo {
            uv_x: atlas_x as f32 / self.width as f32,
            uv_y: atlas_y as f32 / self.height as f32,
            uv_w: glyph_w as f32 / self.width as f32,
            uv_h: glyph_h as f32 / self.height as f32,
            width: glyph_w,
            height: glyph_h,
            bearing_x: metrics.xmin as f32,
            bearing_y: metrics.ymin as f32,
            advance: metrics.advance_width,
        };

        self.shelf_x = atlas_x + glyph_w + padding;
        self.shelf_height = self.shelf_height.max(glyph_h);
        self.dirty = true;

        self.cache.insert(key, info);
        info
    }

    /// Pre-cache ASCII printable characters at a given font size.
    pub fn precache_ascii(&mut self, font_size: f32) {
        for ch in ' '..='~' {
            self.get_glyph(ch, font_size);
        }
    }

    /// Get the monospace cell dimensions for a given font size.
    /// Uses 'M' as the reference character.
    pub fn cell_size(&mut self, font_size: f32) -> (f32, f32) {
        let info = self.get_glyph('M', font_size);
        let cell_width = info.advance;
        let cell_height = font_size * 1.3; // line height factor
        (cell_width, cell_height)
    }

    /// Reset atlas (e.g., on font change).
    pub fn clear(&mut self) {
        self.pixels.fill(0);
        self.shelf_x = 0;
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.cache.clear();
        self.dirty = true;
    }
}
