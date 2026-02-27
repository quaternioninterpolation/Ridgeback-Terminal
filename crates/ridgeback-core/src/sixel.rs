//! Sixel graphics protocol support.
//!
//! Implements decoding of Sixel DCS sequences into RGBA pixel buffers,
//! and provides an image layer for inline terminal graphics.
//!
//! Sixel format encodes 6 vertical pixels per character. Each data byte
//! (0x3F-0x7E) represents a 6-bit column, with bits mapping bottom-to-top.
//!
//! Reference: <https://vt100.net/docs/vt3xx-gp/chapter14.html>

/// An RGBA image decoded from a Sixel sequence.
#[derive(Debug, Clone)]
pub struct SixelImage {
    /// RGBA pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

/// Inline image placement in the terminal grid.
#[derive(Debug, Clone)]
pub struct InlineImage {
    /// Decoded image data.
    pub image: SixelImage,
    /// Row where the image starts (in grid coordinates).
    pub start_row: usize,
    /// Column where the image starts.
    pub start_col: usize,
    /// Width in cell columns the image spans.
    pub cell_cols: usize,
    /// Height in cell rows the image spans.
    pub cell_rows: usize,
    /// Unique ID for texture caching.
    pub id: u64,
}

/// Decoder for Sixel DCS sequences.
///
/// Processes the byte stream between `DCS P1;P2;P3 q` and `ST`,
/// building an RGBA image buffer.
pub struct SixelDecoder {
    /// Color palette (index → (r, g, b)).
    palette: Vec<(u8, u8, u8)>,
    /// Current active color index.
    current_color: usize,
    /// Pixel buffer being constructed.
    pixels: Vec<u8>,
    /// Current raster width.
    width: u32,
    /// Current raster height.
    height: u32,
    /// X cursor position (in pixels).
    x: u32,
    /// Y cursor position (in pixels, top of current 6-pixel band).
    y: u32,
    /// Maximum X reached (determines final width).
    max_x: u32,
    /// Maximum Y reached (determines final height).
    max_y: u32,
    /// Parser state machine.
    state: DecoderState,
    /// Accumulator for numeric parameters.
    param_buf: String,
    /// Collected parameters for current control sequence.
    params: Vec<u32>,
    /// Repeat count from `!` repeat introducer.
    repeat_count: u32,
    /// Background mode: 0 = device default, 1 = no background, 2 = explicit color.
    background_mode: u8,
    /// Raster attributes set flag.
    raster_attrs_set: bool,
    /// Declared aspect ratio (not commonly used).
    _aspect_ratio: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderState {
    /// Waiting for data.
    Normal,
    /// Inside a `#` color command.
    ColorSelect,
    /// Inside a `!` repeat command.
    Repeat,
    /// Inside `"` raster attributes.
    RasterAttrs,
}

impl SixelDecoder {
    /// Create a new Sixel decoder with default 256-color palette.
    pub fn new() -> Self {
        let mut palette = Vec::with_capacity(256);
        // Initialize with VGA-like default palette
        Self::init_default_palette(&mut palette);

        Self {
            palette,
            current_color: 0,
            pixels: Vec::new(),
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            max_x: 0,
            max_y: 0,
            state: DecoderState::Normal,
            param_buf: String::new(),
            params: Vec::new(),
            repeat_count: 1,
            background_mode: 0,
            raster_attrs_set: false,
            _aspect_ratio: (1, 1),
        }
    }

    /// Initialize the default 256-color palette.
    fn init_default_palette(palette: &mut Vec<(u8, u8, u8)>) {
        // Standard 16 ANSI colors
        let ansi16: [(u8, u8, u8); 16] = [
            (0, 0, 0),       // 0  Black
            (128, 0, 0),     // 1  Red
            (0, 128, 0),     // 2  Green
            (128, 128, 0),   // 3  Yellow
            (0, 0, 128),     // 4  Blue
            (128, 0, 128),   // 5  Magenta
            (0, 128, 128),   // 6  Cyan
            (192, 192, 192), // 7  White
            (128, 128, 128), // 8  Bright Black
            (255, 0, 0),     // 9  Bright Red
            (0, 255, 0),     // 10 Bright Green
            (255, 255, 0),   // 11 Bright Yellow
            (0, 0, 255),     // 12 Bright Blue
            (255, 0, 255),   // 13 Bright Magenta
            (0, 255, 255),   // 14 Bright Cyan
            (255, 255, 255), // 15 Bright White
        ];
        for color in &ansi16 {
            palette.push(*color);
        }

        // 216-color cube (6x6x6)
        for r in 0u8..6 {
            for g in 0u8..6 {
                for b in 0u8..6 {
                    let rv = if r == 0 { 0 } else { 55 + 40 * r };
                    let gv = if g == 0 { 0 } else { 55 + 40 * g };
                    let bv = if b == 0 { 0 } else { 55 + 40 * b };
                    palette.push((rv, gv, bv));
                }
            }
        }

        // 24 grayscale
        for i in 0u8..24 {
            let v = 8 + 10 * i;
            palette.push((v, v, v));
        }
    }

    /// Feed a single byte from the Sixel data stream.
    pub fn feed(&mut self, byte: u8) {
        match self.state {
            DecoderState::Normal => self.feed_normal(byte),
            DecoderState::ColorSelect => self.feed_color_select(byte),
            DecoderState::Repeat => self.feed_repeat(byte),
            DecoderState::RasterAttrs => self.feed_raster_attrs(byte),
        }
    }

    /// Feed a slice of bytes from the Sixel data stream.
    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.feed(b);
        }
    }

    fn feed_normal(&mut self, byte: u8) {
        match byte {
            // Sixel data bytes (0x3F - 0x7E): each represents a 6-pixel column
            0x3F..=0x7E => {
                let sixel = byte - 0x3F;
                for _ in 0..self.repeat_count {
                    self.draw_sixel(sixel);
                }
                self.repeat_count = 1;
            }
            // `$` — Graphics carriage return (go to left margin of current 6-pixel band)
            b'$' => {
                self.x = 0;
            }
            // `-` — Graphics new line (go to left margin of next 6-pixel band)
            b'-' => {
                self.x = 0;
                self.y += 6;
            }
            // `#` — Color select introducer
            b'#' => {
                self.state = DecoderState::ColorSelect;
                self.param_buf.clear();
                self.params.clear();
            }
            // `!` — Repeat introducer
            b'!' => {
                self.state = DecoderState::Repeat;
                self.param_buf.clear();
            }
            // `"` — Raster attributes
            b'"' => {
                self.state = DecoderState::RasterAttrs;
                self.param_buf.clear();
                self.params.clear();
            }
            _ => {
                // Ignore unrecognized bytes
            }
        }
    }

    fn feed_color_select(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.param_buf.push(byte as char);
            }
            b';' => {
                let val = self.param_buf.parse::<u32>().unwrap_or(0);
                self.params.push(val);
                self.param_buf.clear();
            }
            _ => {
                // End of color command — parse accumulated params
                let val = self.param_buf.parse::<u32>().unwrap_or(0);
                self.params.push(val);
                self.param_buf.clear();

                if self.params.len() == 1 {
                    // Color select: #<index>
                    self.current_color = self.params[0] as usize;
                } else if self.params.len() >= 5 {
                    // Color definition: #<index>;<type>;<c1>;<c2>;<c3>
                    let idx = self.params[0] as usize;
                    let color_type = self.params[1];
                    let c1 = self.params[2];
                    let c2 = self.params[3];
                    let c3 = self.params[4];

                    let (r, g, b) = match color_type {
                        1 => {
                            // HLS (Hue, Lightness, Saturation) — in Sixel, ranges 0-360, 0-100, 0-100
                            hls_to_rgb(c1, c2, c3)
                        }
                        2 | _ => {
                            // RGB, values in 0-100 range
                            (
                                ((c1 * 255) / 100) as u8,
                                ((c2 * 255) / 100) as u8,
                                ((c3 * 255) / 100) as u8,
                            )
                        }
                    };

                    // Extend palette if needed
                    while self.palette.len() <= idx {
                        self.palette.push((0, 0, 0));
                    }
                    self.palette[idx] = (r, g, b);
                    self.current_color = idx;
                }

                self.state = DecoderState::Normal;
                // Re-process the terminating byte in normal mode
                self.feed_normal(byte);
            }
        }
    }

    fn feed_repeat(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.param_buf.push(byte as char);
            }
            _ => {
                // End of repeat count
                self.repeat_count = self.param_buf.parse::<u32>().unwrap_or(1).max(1);
                self.param_buf.clear();
                self.state = DecoderState::Normal;
                // Re-process the terminating byte (should be a sixel data byte)
                self.feed_normal(byte);
            }
        }
    }

    fn feed_raster_attrs(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.param_buf.push(byte as char);
            }
            b';' => {
                let val = self.param_buf.parse::<u32>().unwrap_or(0);
                self.params.push(val);
                self.param_buf.clear();
            }
            _ => {
                let val = self.param_buf.parse::<u32>().unwrap_or(0);
                self.params.push(val);
                self.param_buf.clear();

                // Raster attributes: "<Pan>;<Pad>;<Ph>;<Pv>
                // Pan/Pad = pixel aspect ratio, Ph = width, Pv = height
                if self.params.len() >= 4 {
                    let _pan = self.params[0];
                    let _pad = self.params[1];
                    let ph = self.params[2];
                    let pv = self.params[3];

                    if ph > 0 && pv > 0 && ph <= 4096 && pv <= 4096 {
                        self.width = ph;
                        self.height = pv;
                        self.pixels.resize((ph * pv * 4) as usize, 0);
                        self.raster_attrs_set = true;
                    }
                }

                self.state = DecoderState::Normal;
                self.feed_normal(byte);
            }
        }
    }

    /// Draw a single sixel column (6 vertical pixels) at the current cursor.
    fn draw_sixel(&mut self, sixel: u8) {
        let (r, g, b) = self
            .palette
            .get(self.current_color)
            .copied()
            .unwrap_or((255, 255, 255));

        // Ensure buffer is large enough
        let needed_width = self.x + 1;
        let needed_height = self.y + 6;

        if !self.raster_attrs_set || needed_width > self.width || needed_height > self.height {
            let new_w = self.width.max(needed_width);
            let new_h = self.height.max(needed_height);
            self.grow_buffer(new_w, new_h);
        }

        // Plot the 6 vertical pixels encoded in the sixel byte
        for bit in 0u8..6 {
            if sixel & (1 << bit) != 0 {
                let px = self.x;
                let py = self.y + bit as u32;
                if px < self.width && py < self.height {
                    let offset = ((py * self.width + px) * 4) as usize;
                    if offset + 3 < self.pixels.len() {
                        self.pixels[offset] = r;
                        self.pixels[offset + 1] = g;
                        self.pixels[offset + 2] = b;
                        self.pixels[offset + 3] = 255;
                    }
                }
            }
        }

        self.x += 1;
        self.max_x = self.max_x.max(self.x);
        self.max_y = self.max_y.max(self.y + 6);
    }

    /// Grow the pixel buffer to accommodate a larger image.
    fn grow_buffer(&mut self, new_w: u32, new_h: u32) {
        if new_w == self.width && new_h == self.height {
            return;
        }

        let mut new_pixels = vec![0u8; (new_w * new_h * 4) as usize];

        // Copy existing rows
        for row in 0..self.height.min(new_h) {
            let src_start = (row * self.width * 4) as usize;
            let src_end = src_start + (self.width.min(new_w) * 4) as usize;
            let dst_start = (row * new_w * 4) as usize;
            let copy_len = src_end - src_start;
            if src_end <= self.pixels.len() && dst_start + copy_len <= new_pixels.len() {
                new_pixels[dst_start..dst_start + copy_len]
                    .copy_from_slice(&self.pixels[src_start..src_end]);
            }
        }

        self.pixels = new_pixels;
        self.width = new_w;
        self.height = new_h;
    }

    /// Finalize decoding and return the image.
    ///
    /// Call this after all Sixel data has been fed (on DCS unhook).
    pub fn finish(mut self) -> Option<SixelImage> {
        if self.max_x == 0 || self.max_y == 0 {
            return None;
        }

        // Trim to actual content size if raster attrs were not set
        let final_w = if self.raster_attrs_set {
            self.width
        } else {
            self.max_x
        };
        let final_h = if self.raster_attrs_set {
            self.height
        } else {
            self.max_y
        };

        // If we need to trim, create a properly sized buffer
        if final_w != self.width || final_h != self.height {
            self.grow_buffer(final_w, final_h);
        }

        Some(SixelImage {
            pixels: self.pixels,
            width: final_w,
            height: final_h,
        })
    }

    /// Reset the decoder state for a new image.
    pub fn reset(&mut self) {
        self.pixels.clear();
        self.width = 0;
        self.height = 0;
        self.x = 0;
        self.y = 0;
        self.max_x = 0;
        self.max_y = 0;
        self.current_color = 0;
        self.repeat_count = 1;
        self.state = DecoderState::Normal;
        self.raster_attrs_set = false;
        self.param_buf.clear();
        self.params.clear();
    }
}

impl Default for SixelDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Manage inline images displayed in the terminal.
pub struct ImageLayer {
    /// Active inline images, keyed by a unique ID.
    images: Vec<InlineImage>,
    /// Counter for generating unique image IDs.
    next_id: u64,
}

impl ImageLayer {
    pub fn new() -> Self {
        Self {
            images: Vec::new(),
            next_id: 1,
        }
    }

    /// Place a decoded Sixel image at the given grid position.
    ///
    /// `cell_width` and `cell_height` are the pixel dimensions of a single cell,
    /// used to calculate how many grid cells the image occupies.
    pub fn place_image(
        &mut self,
        image: SixelImage,
        row: usize,
        col: usize,
        cell_width: f32,
        cell_height: f32,
    ) -> u64 {
        let cell_cols = (image.width as f32 / cell_width).ceil() as usize;
        let cell_rows = (image.height as f32 / cell_height).ceil() as usize;
        let id = self.next_id;
        self.next_id += 1;

        self.images.push(InlineImage {
            image,
            start_row: row,
            start_col: col,
            cell_cols,
            cell_rows,
            id,
        });

        id
    }

    /// Get all visible images that overlap the given viewport.
    pub fn visible_images(&self, first_row: usize, last_row: usize) -> Vec<&InlineImage> {
        self.images
            .iter()
            .filter(|img| {
                let img_end = img.start_row + img.cell_rows;
                img.start_row <= last_row && img_end >= first_row
            })
            .collect()
    }

    /// Remove images that have scrolled out of view.
    pub fn gc(&mut self, scrollback_top: usize) {
        self.images
            .retain(|img| img.start_row + img.cell_rows > scrollback_top);
    }

    /// Scroll all images up by `n` rows (called when the terminal scrolls).
    pub fn scroll_up(&mut self, n: usize) {
        for img in &mut self.images {
            img.start_row = img.start_row.saturating_sub(n);
        }
    }

    /// Remove all images.
    pub fn clear(&mut self) {
        self.images.clear();
    }

    /// Get image by ID.
    pub fn get_image(&self, id: u64) -> Option<&InlineImage> {
        self.images.iter().find(|img| img.id == id)
    }

    /// Number of active inline images.
    pub fn count(&self) -> usize {
        self.images.len()
    }
}

impl Default for ImageLayer {
    fn default() -> Self {
        Self::new()
    }
}

// ── iTerm2 Inline Image Protocol ────────────────────────────────────────

/// Decode an iTerm2 inline image from an OSC 1337 sequence.
///
/// Format: `ESC ] 1337 ; File=[params]:base64data BEL`
/// Params: `name=<base64name>;size=<bytes>;inline=1;width=<cols>;height=<rows>`
pub fn decode_iterm2_image(params_str: &str) -> Option<SixelImage> {
    // Find the colon separator between params and data
    let colon_pos = params_str.find(':')?;
    let (params_part, data_part) = params_str.split_at(colon_pos);
    let data_part = &data_part[1..]; // skip the ':'

    // Check that inline=1 is present
    let is_inline = params_part
        .split(';')
        .any(|p| p.trim() == "inline=1");
    if !is_inline {
        return None;
    }

    // Decode base64 data
    let decoded = base64_decode(data_part)?;

    // Try to detect image format and decode
    decode_image_bytes(&decoded)
}

/// Decode an image from raw bytes (PNG, JPEG, GIF, BMP).
///
/// Uses a minimal decoder — supports the most common PNG format.
fn decode_image_bytes(data: &[u8]) -> Option<SixelImage> {
    // Check for PNG signature
    if data.len() > 8 && data[0..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        return decode_png_simple(data);
    }

    // For other formats, we treat as raw RGBA if it looks right,
    // otherwise return None (a full image decoder like `image` crate
    // would be needed for JPEG/GIF support)
    None
}

/// Minimal PNG decoder for uncompressed/simple PNGs.
/// For full PNG support, the `image` crate should be used.
fn decode_png_simple(_data: &[u8]) -> Option<SixelImage> {
    // A production implementation would use the `image` or `png` crate.
    // This stub returns None; when the `image` crate is added, this
    // function can delegate to it.
    //
    // For now, Sixel is the primary supported inline image protocol,
    // and iTerm2 support is a framework for future expansion.
    None
}

/// Simple base64 decoder.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let input: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    };

    let bytes = input.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = table(bytes[i])?;
        let b = table(bytes[i + 1])?;
        let c = table(bytes[i + 2])?;
        let d = table(bytes[i + 3])?;

        output.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if bytes[i + 3] != b'=' {
            output.push((c << 6) | d);
        }
        i += 4;
    }

    Some(output)
}

/// Convert HLS (Hue/Lightness/Saturation) to RGB.
///
/// Sixel HLS: H=0-360, L=0-100, S=0-100.
fn hls_to_rgb(h: u32, l: u32, s: u32) -> (u8, u8, u8) {
    if s == 0 {
        let v = ((l * 255) / 100) as u8;
        return (v, v, v);
    }

    let h = (h % 360) as f64;
    let l = l as f64 / 100.0;
    let s = s as f64 / 100.0;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_sixel_decode() {
        let mut decoder = SixelDecoder::new();

        // Select color 1 (red in default palette)
        decoder.feed_bytes(b"#1");

        // Draw a simple sixel: '?' = 0x3F = 0b000000 (no pixels)
        // '@' = 0x40 = 0b000001 (bottom pixel only)
        // '~' = 0x7E = 0b111111 (all 6 pixels)
        decoder.feed_bytes(b"~");

        let image = decoder.finish().unwrap();
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 6);

        // All 6 pixels should be colored (red)
        for row in 0..6 {
            let offset = (row * 4) as usize;
            assert_eq!(image.pixels[offset + 3], 255, "pixel at row {} should be opaque", row);
        }
    }

    #[test]
    fn test_sixel_repeat() {
        let mut decoder = SixelDecoder::new();
        decoder.feed_bytes(b"#0!5~");
        let image = decoder.finish().unwrap();
        assert_eq!(image.width, 5);
        assert_eq!(image.height, 6);
    }

    #[test]
    fn test_sixel_newline() {
        let mut decoder = SixelDecoder::new();
        decoder.feed_bytes(b"#0~-~");
        let image = decoder.finish().unwrap();
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 12); // Two 6-pixel bands
    }

    #[test]
    fn test_sixel_color_definition() {
        let mut decoder = SixelDecoder::new();
        // Define color 0 as pure red via RGB (type 2), percentages
        decoder.feed_bytes(b"#0;2;100;0;0~");
        let image = decoder.finish().unwrap();
        let offset = 0;
        assert_eq!(image.pixels[offset], 255); // R
        assert_eq!(image.pixels[offset + 1], 0); // G
        assert_eq!(image.pixels[offset + 2], 0); // B
    }

    #[test]
    fn test_hls_to_rgb() {
        // Pure red: H=0, L=50, S=100
        let (r, g, b) = hls_to_rgb(0, 50, 100);
        assert_eq!(r, 255);
        assert_eq!(g, 0);
        assert_eq!(b, 0);

        // Grayscale: S=0, L=50
        let (r, g, b) = hls_to_rgb(0, 50, 0);
        assert_eq!(r, 128); // 0.5 * 255 ≈ 128
        assert_eq!(g, 128);
        assert_eq!(b, 128);
    }

    #[test]
    fn test_image_layer() {
        let mut layer = ImageLayer::new();
        let image = SixelImage {
            pixels: vec![255; 4 * 80 * 24],
            width: 80,
            height: 24,
        };
        let id = layer.place_image(image, 5, 0, 8.0, 16.0);
        assert!(id > 0);
        assert_eq!(layer.count(), 1);

        let visible = layer.visible_images(0, 10);
        assert_eq!(visible.len(), 1);

        layer.scroll_up(3);
        assert_eq!(layer.images[0].start_row, 2);
    }

    #[test]
    fn test_base64_decode() {
        let decoded = base64_decode("SGVsbG8=").unwrap();
        assert_eq!(decoded, b"Hello");
    }
}
