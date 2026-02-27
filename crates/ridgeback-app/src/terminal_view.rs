use egui;
use crate::tabs::TabState;
use ridgeback_core::input_buffer::InputAction;
use ridgeback_core::cell::Color;
use ridgeback_config::ShaderEffect;

/// Render the terminal viewport for the active tab.
pub fn show_terminal(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    clipboard: &mut Option<arboard::Clipboard>,
    bg_texture: Option<&egui::TextureHandle>,
    allow_shader_repaint: bool,
) {
    let available = ui.available_size();
    let font_size = 14.0;
    let bg_color = egui::Color32::from_rgb(0x1e, 0x1e, 0x2e);

    // Resolve text foreground from profile hex string
    let default_fg = parse_hex_color(&tab.text_foreground)
        .unwrap_or(egui::Color32::from_gray(205));

    // Shadow: a dark semi-transparent rect drawn under each row of text.
    // alpha = 0 means no shadow; 1.0 means fully black rect behind text.
    let shadow_enabled = tab.text_shadow_enabled;
    let shadow_alpha   = ((tab.text_shadow_alpha.clamp(0.0, 1.0)) * 200.0) as u8;

    // Measure the real advance width of one monospace character.
    let char_w = ui.ctx().fonts(|f| {
        f.glyph_width(&egui::FontId::monospace(font_size), ' ')
    });

    let term_rect = ui.available_rect_before_wrap();

    // ── Layer 1: solid background ─────────────────────────────────────────
    ui.painter().rect_filled(term_rect, 0.0, bg_color);

    // ── Layer 2: background image at 50% opacity, uniform scale (cover) ──
    if let Some(tex) = bg_texture {
        let img_w = tex.size()[0] as f32;
        let img_h = tex.size()[1] as f32;
        let rect_w = term_rect.width();
        let rect_h = term_rect.height();
        let scale = (rect_w / img_w).max(rect_h / img_h);
        let scaled_w = img_w * scale;
        let scaled_h = img_h * scale;
        let u0 = (scaled_w - rect_w) / (2.0 * scaled_w);
        let v0 = (scaled_h - rect_h) / (2.0 * scaled_h);
        ui.painter().image(
            tex.id(),
            term_rect,
            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(1.0 - u0, 1.0 - v0)),
            egui::Color32::from_white_alpha(128),
        );
    }

    // Get terminal content
    let visible_cells = tab.terminal.vt.visible_cells().to_vec();
    let scrollback_lines = tab.terminal.vt.scrollback.all_lines_as_strings();
    let cursor_row = tab.terminal.vt.cursor_row;
    let cursor_col = tab.terminal.vt.cursor_col;
    let _cursor_style = tab.terminal.vt.cursor_style;
    let _cursor_color = egui::Color32::from_rgb(205, 214, 244);

    egui::ScrollArea::vertical()
        .id_salt("terminal_output")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_min_width(available.x);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.visuals_mut().widgets.noninteractive.bg_fill = egui::Color32::TRANSPARENT;
            ui.visuals_mut().extreme_bg_color = egui::Color32::TRANSPARENT;

            // Render scrollback lines
            for line in &scrollback_lines {
                let text = if line.is_empty() { " " } else { line.as_str() };
                let mut job = egui::text::LayoutJob::default();
                job.append(text, 0.0, egui::TextFormat {
                    font_id: egui::FontId::monospace(font_size),
                    color: default_fg,
                    background: egui::Color32::TRANSPARENT,
                    ..Default::default()
                });
                let galley = ui.fonts(|f| f.layout_job(job));
                let (resp, painter) = ui.allocate_painter(galley.size(), egui::Sense::hover());
                let origin = resp.rect.min;
                // Dark underlay rect — darkens whatever is behind, no bloom
                if shadow_enabled && shadow_alpha > 0 {
                    painter.rect_filled(
                        resp.rect.expand2(egui::vec2(2.0, 1.0)),
                        2.0,
                        egui::Color32::from_black_alpha(shadow_alpha),
                    );
                }
                painter.galley(origin, galley, default_fg);
            }

            // Render visible grid rows
            for (row_idx, row) in visible_cells.iter().enumerate() {
                let has_content = row.iter().any(|c| !c.is_empty());
                let is_cursor_row = row_idx == cursor_row;
                if !has_content && !is_cursor_row {
                    continue;
                }

                // ── Cursor row ────────────────────────────────────────────────
                if is_cursor_row {
                    // Build the full row text from VT cells — the PTY echo is the
                    // source of truth for what's displayed. We do NOT maintain a
                    // separate local buffer for the visible text.
                    let mut job = egui::text::LayoutJob::default();
                    let mut seg_text = String::new();
                    let mut seg_fg = default_fg;
                    let mut seg_bg = egui::Color32::TRANSPARENT;

                    let flush_seg = |job: &mut egui::text::LayoutJob, text: &str, fg: egui::Color32, bg: egui::Color32| {
                        if text.is_empty() { return; }
                        job.append(text, 0.0, egui::TextFormat {
                            font_id: egui::FontId::monospace(font_size),
                            color: fg,
                            background: bg,
                            ..Default::default()
                        });
                    };

                    for (col_idx, cell) in row.iter().enumerate() {
                        let is_cur = col_idx == cursor_col;
                        let ch = if cell.ch == '\0' { ' ' } else { cell.ch };
                        let (fg, bg) = if is_cur {
                            // Block cursor: invert
                            (bg_color, egui::Color32::from_rgb(205, 214, 244))
                        } else {
                            (
                                color_to_egui(&cell.attrs.fg, default_fg),
                                match &cell.attrs.bg {
                                    Color::Default => egui::Color32::TRANSPARENT,
                                    other => color_to_egui(other, egui::Color32::TRANSPARENT),
                                },
                            )
                        };
                        if fg != seg_fg || bg != seg_bg {
                            flush_seg(&mut job, &seg_text, seg_fg, seg_bg);
                            seg_text.clear();
                            seg_fg = fg;
                            seg_bg = bg;
                        }
                        seg_text.push(ch);
                    }
                    flush_seg(&mut job, &seg_text, seg_fg, seg_bg);
                    if job.text.is_empty() {
                        job.append(" ", 0.0, egui::TextFormat {
                            font_id: egui::FontId::monospace(font_size),
                            color: egui::Color32::TRANSPARENT,
                            ..Default::default()
                        });
                    }

                    // Render the row text with dark underlay shadow
                    let row_resp = ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        let galley = ui.fonts(|f| f.layout_job(job));
                        let (resp, painter) = ui.allocate_painter(galley.size(), egui::Sense::hover());
                        let origin = resp.rect.min;
                        if shadow_enabled && shadow_alpha > 0 {
                            painter.rect_filled(
                                resp.rect.expand2(egui::vec2(2.0, 1.0)),
                                2.0,
                                egui::Color32::from_black_alpha(shadow_alpha),
                            );
                        }
                        painter.galley(origin, galley, egui::Color32::WHITE);
                        resp
                    });

                    // Invisible focus-capture overlay on the whole row rect.
                    let input_id = ui.id().with("inline_input");
                    let row_rect = row_resp.response.rect;
                    let focus_resp = ui.interact(row_rect, input_id, egui::Sense::click());

                    if focus_resp.clicked() {
                        ui.memory_mut(|m| m.request_focus(input_id));
                    }
                    ui.memory_mut(|m| {
                        if m.focused().is_none() {
                            m.request_focus(input_id);
                        }
                    });
                    let has_focus = ui.memory(|m| m.has_focus(input_id));

                    // ── Record cursor screen position for fire particles ───────
                    // row_rect is in screen space. term_rect.min is the top-left of
                    // the terminal panel. Subtracting it gives rect-relative coords
                    // that match what draw_fire_overlay uses (rect.left/top + p.x/y).
                    if tab.shader_effect == ShaderEffect::Fire {
                        let cursor_screen_x = row_rect.left() + cursor_col as f32 * char_w;
                        let cursor_screen_y = row_rect.center().y;
                        tab.fire.last_emit_x = cursor_screen_x - term_rect.left();
                        tab.fire.last_emit_y = cursor_screen_y - term_rect.top();
                    }

                    if has_focus && !tab.terminal.exited {
                        let events: Vec<egui::Event> = ui.ctx().input(|i| i.events.clone());
                        for ev in &events {
                            match ev {
                                egui::Event::Text(t) => {
                                    for ch in t.chars() {
                                        if ch.is_control() { continue; }
                                        let mut buf = [0u8; 4];
                                        let s = ch.encode_utf8(&mut buf);
                                        let _ = tab.terminal.write_to_pty(s.as_bytes());
                                        if tab.shader_effect == ShaderEffect::Fire {
                                            tab.fire.emit_keypress(
                                                tab.fire.last_emit_x,
                                                tab.fire.last_emit_y,
                                            );
                                        }
                                    }
                                }
                                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                                    match key {
                                        egui::Key::Enter      => { let _ = tab.terminal.write_to_pty(b"\r"); tab.inline_input.clear(); }
                                        egui::Key::Backspace  => { let _ = tab.terminal.write_to_pty(b"\x08"); }
                                        egui::Key::Delete     => { let _ = tab.terminal.write_to_pty(b"\x1b[3~"); }
                                        egui::Key::ArrowUp    => { let _ = tab.terminal.write_to_pty(b"\x1b[A"); }
                                        egui::Key::ArrowDown  => { let _ = tab.terminal.write_to_pty(b"\x1b[B"); }
                                        egui::Key::ArrowRight => { let _ = tab.terminal.write_to_pty(b"\x1b[C"); }
                                        egui::Key::ArrowLeft  => { let _ = tab.terminal.write_to_pty(b"\x1b[D"); }
                                        egui::Key::Home       => { let _ = tab.terminal.write_to_pty(b"\x1b[H"); }
                                        egui::Key::End        => { let _ = tab.terminal.write_to_pty(b"\x1b[F"); }
                                        egui::Key::PageUp     => { let _ = tab.terminal.write_to_pty(b"\x1b[5~"); }
                                        egui::Key::PageDown   => { let _ = tab.terminal.write_to_pty(b"\x1b[6~"); }
                                        egui::Key::Escape     => { let _ = tab.terminal.write_to_pty(b"\x1b"); }
                                        egui::Key::Tab if !modifiers.shift => { let _ = tab.terminal.write_to_pty(b"\t"); }
                                        egui::Key::Tab        => { let _ = tab.terminal.write_to_pty(b"\x1b[Z"); }
                                        egui::Key::C if modifiers.ctrl && !modifiers.shift => {
                                            // Ctrl+C → SIGINT (interrupt current process)
                                            let _ = tab.terminal.write_to_pty(b"\x03");
                                        }
                                        egui::Key::C if modifiers.ctrl && modifiers.shift => {
                                            // Ctrl+Shift+C → copy to clipboard
                                            // TODO: copy selected text; for now copy visible screen text
                                            let lines: Vec<String> = tab.terminal.vt.visible_cells()
                                                .iter()
                                                .map(|row| row.iter().map(|c| if c.ch == '\0' { ' ' } else { c.ch }).collect::<String>())
                                                .collect();
                                            let text = lines.join("\n");
                                            if let Some(ref mut cb) = *clipboard {
                                                let _ = cb.set_text(text);
                                            }
                                        }
                                        egui::Key::D if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x04"); }
                                        egui::Key::L if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x0c"); }
                                        egui::Key::U if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x15"); }
                                        egui::Key::A if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x01"); }
                                        egui::Key::E if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x05"); }
                                        egui::Key::K if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x0b"); }
                                        egui::Key::W if modifiers.ctrl => { let _ = tab.terminal.write_to_pty(b"\x17"); }
                                        egui::Key::V if modifiers.ctrl => {
                                            if let Some(ref mut cb) = *clipboard {
                                                if let Ok(text) = cb.get_text() {
                                                    let _ = tab.terminal.write_to_pty(text.as_bytes());
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }


                    continue;
                }

                // ── Non-cursor rows: styled label ─────────────────────────────
                let mut job = egui::text::LayoutJob::default();
                let mut seg_text = String::new();
                let mut seg_fg = default_fg;
                let mut seg_bg = egui::Color32::TRANSPARENT;

                let flush = |job: &mut egui::text::LayoutJob, text: &str, fg: egui::Color32, bg: egui::Color32| {
                    if text.is_empty() { return; }
                    job.append(text, 0.0, egui::TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color: fg,
                        background: bg,
                        ..Default::default()
                    });
                };

                for cell in row.iter() {
                    let (fg, bg) = (
                        color_to_egui(&cell.attrs.fg, default_fg),
                        match &cell.attrs.bg {
                            Color::Default => egui::Color32::TRANSPARENT,
                            other => color_to_egui(other, egui::Color32::TRANSPARENT),
                        },
                    );
                    let ch = if cell.ch == '\0' { ' ' } else { cell.ch };
                    if fg != seg_fg || bg != seg_bg {
                        flush(&mut job, &seg_text, seg_fg, seg_bg);
                        seg_text.clear();
                        seg_fg = fg;
                        seg_bg = bg;
                    }
                    seg_text.push(ch);
                }
                flush(&mut job, &seg_text, seg_fg, seg_bg);

                if job.text.is_empty() {
                    job.append(" ", 0.0, egui::TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color: egui::Color32::TRANSPARENT,
                        ..Default::default()
                    });
                }

                // Lay out once, paint shadow rect then real text
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    let galley = ui.fonts(|f| f.layout_job(job));
                    let (resp, painter) = ui.allocate_painter(galley.size(), egui::Sense::hover());
                    let origin = resp.rect.min;
                    // Dark underlay behind the text — darkens background, no bloom
                    if shadow_enabled && shadow_alpha > 0 {
                        painter.rect_filled(
                            resp.rect.expand2(egui::vec2(2.0, 1.0)),
                            2.0,
                            egui::Color32::from_black_alpha(shadow_alpha),
                        );
                    }
                    painter.galley(origin, galley, egui::Color32::WHITE);
                });
            }
        });

    // Apply shader overlay on top of terminal content
    apply_shader_overlay(ui, term_rect, tab, allow_shader_repaint);

    // Input is handled by the inline custom input field on the cursor row above.
    // handle_keyboard_input is kept only for when the tab loses focus (e.g. overlays open).
    if !tab.command_query.is_open && !tab.find_overlay.is_open {
        handle_keyboard_input(ui, tab, clipboard, char_w);
    }
}

fn apply_shader_overlay(ui: &mut egui::Ui, rect: egui::Rect, tab: &mut TabState, allow_repaint: bool) {
    match tab.shader_effect {
        ShaderEffect::None => {}
        ShaderEffect::Crt  => draw_crt_overlay(ui, rect, &tab.shader_params),
        ShaderEffect::Fire => draw_fire_overlay(ui, rect, tab, allow_repaint),
    }
}

/// CRT effect: horizontal scanlines + edge vignette + subtle chromatic tint.
fn draw_crt_overlay(ui: &mut egui::Ui, rect: egui::Rect, params: &ridgeback_config::ShaderParams) {
    let painter = ui.painter_at(rect);

    // ── Scanlines ─────────────────────────────────────────────────────────
    let scanline_alpha = (params.scanline_intensity * 80.0) as u8;
    let scanline_color = egui::Color32::from_black_alpha(scanline_alpha);
    let line_spacing = 3.0_f32;
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, scanline_color),
        );
        y += line_spacing;
    }

    // ── Vignette (darkened edges for curvature illusion) ──────────────────
    let vignette_alpha = (params.curvature * 160.0) as u8;
    // Left edge
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * 0.04, rect.height())),
        0.0,
        egui::Color32::from_black_alpha(vignette_alpha),
    );
    // Right edge
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - rect.width() * 0.04, rect.top()),
            egui::vec2(rect.width() * 0.04, rect.height()),
        ),
        0.0,
        egui::Color32::from_black_alpha(vignette_alpha),
    );
    // Top edge
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), rect.height() * 0.03)),
        0.0,
        egui::Color32::from_black_alpha(vignette_alpha),
    );
    // Bottom edge
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.bottom() - rect.height() * 0.03),
            egui::vec2(rect.width(), rect.height() * 0.03),
        ),
        0.0,
        egui::Color32::from_black_alpha(vignette_alpha),
    );

    // ── Bloom: faint green tint overlay ───────────────────────────────────
    let bloom_alpha = (params.bloom_strength * 18.0) as u8;
    painter.rect_filled(
        rect,
        0.0,
        egui::Color32::from_rgba_unmultiplied(0, 255, 80, bloom_alpha),
    );

    // ── Overall glassy tint ───────────────────────────────────────────────
    painter.rect_filled(
        rect,
        0.0,
        egui::Color32::from_black_alpha(15),
    );
}

/// Fire effect: Doom-style cellular automaton base flame + particle embers + smoke.
fn draw_fire_overlay(ui: &mut egui::Ui, rect: egui::Rect, tab: &mut TabState, allow_repaint: bool) {
    let dt = ui.ctx().input(|i| i.unstable_dt).min(0.05);
    let params = tab.shader_params.clone();

    // Step simulation and particles
    tab.fire.update(dt, &params);

    let painter = ui.painter_at(rect);

    // ── Doom-style cellular automaton base flame ──────────────────────────
    // The sim is in cell-space; map to pixel-space along the bottom edge.
    let cell_h = 20usize;
    let cell_w = tab.fire.sim.w;
    // We render the top N rows of the sim as flame height
    let flame_pixel_height = rect.height() * 0.28 * params.fire_intensity;
    let cell_px_w = rect.width() / cell_w as f32;
    let cell_px_h = flame_pixel_height / cell_h as f32;

    for row in 0..cell_h {
        for col in 0..cell_w {
            let heat = tab.fire.sim.buf[row * cell_w + col];
            if heat < 0.04 { continue; }

            // Map heat 0..1 → fire palette
            // 0.0–0.25 → black→dark red, 0.25–0.55 → dark red→orange,
            // 0.55–0.80 → orange→yellow, 0.80–1.0 → yellow→white
            let (r, g, b, a) = heat_to_rgba(heat, params.fire_intensity);

            let px = rect.left() + col as f32 * cell_px_w;
            let py = rect.bottom() - (cell_h - row) as f32 * cell_px_h;

            // Each cell is a slightly rounded rect for a more organic look
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(px - 0.5, py - 0.5),
                    egui::vec2(cell_px_w + 1.0, cell_px_h + 1.0),
                ),
                1.0,
                egui::Color32::from_rgba_unmultiplied(r, g, b, a),
            );
        }
    }

    // ── Smoke particles (drawn below embers so embers appear on top) ──────
    for p in &tab.fire.particles {
        if !p.is_smoke { continue; }
        let t = 1.0 - (p.life / p.max_life);
        let alpha = ((1.0 - t) * (1.0 - t) * 60.0) as u8; // 0.5 × 120
        if alpha < 3 { continue; }
        let grey = (60 + (t * 80.0) as u8).min(160);
        painter.circle_filled(
            egui::pos2(rect.left() + p.x, rect.top() + p.y),
            p.radius,
            egui::Color32::from_rgba_unmultiplied(grey, grey, grey, alpha),
        );
    }

    // ── Fire ember particles ──────────────────────────────────────────────
    for p in &tab.fire.particles {
        if p.is_smoke { continue; }
        let t = 1.0 - (p.life / p.max_life);
        let alpha = ((1.0 - t * t) * 115.0) as u8; // 0.5 × 230
        if alpha < 5 { continue; }
        let heat = (p.heat).max(0.0).min(1.0);
        let (r, g, b, _) = heat_to_rgba(heat, 1.0);
        // Core bright spot
        painter.circle_filled(
            egui::pos2(rect.left() + p.x, rect.top() + p.y),
            p.radius,
            egui::Color32::from_rgba_unmultiplied(r, g, b, alpha),
        );
        // Soft glow halo
        painter.circle_filled(
            egui::pos2(rect.left() + p.x, rect.top() + p.y),
            p.radius * 2.2,
            egui::Color32::from_rgba_unmultiplied(r, g / 2, 0, alpha / 6),
        );
    }

    // ── Subtle ambient heat shimmer at the very bottom ────────────────────
    let shimmer_alpha = (params.fire_intensity * 40.0) as u8;
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.bottom() - 3.0),
            egui::vec2(rect.width(), 3.0),
        ),
        0.0,
        egui::Color32::from_rgba_unmultiplied(255, 140, 0, shimmer_alpha),
    );

    // Keep animating only when allowed (respects update_in_background setting)
    if allow_repaint {
        ui.ctx().request_repaint();
    }
}

/// Map a heat value 0..1 to an RGBA fire colour.
/// Palette (inspired by real fire blackbody radiation):
///   0.00–0.20 → black → deep red
///   0.20–0.50 → deep red → bright orange
///   0.50–0.75 → bright orange → yellow
///   0.75–1.00 → yellow → white
fn heat_to_rgba(heat: f32, intensity: f32) -> (u8, u8, u8, u8) {
    let h = heat.max(0.0).min(1.0);
    let (r, g, b) = if h < 0.20 {
        let t = h / 0.20;
        (lerp(0, 160, t), 0u8, 0u8)
    } else if h < 0.50 {
        let t = (h - 0.20) / 0.30;
        (lerp(160, 255, t), lerp(0, 100, t), 0u8)
    } else if h < 0.75 {
        let t = (h - 0.50) / 0.25;
        (255u8, lerp(100, 220, t), lerp(0, 20, t))
    } else {
        let t = (h - 0.75) / 0.25;
        (255u8, lerp(220, 255, t), lerp(20, 200, t))
    };
    let alpha = ((h * 0.85 + 0.15) * intensity * 220.0).min(255.0) as u8;
    (r, g, b, alpha)
}

#[inline(always)]
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8
}


fn handle_keyboard_input(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    _clipboard: &mut Option<arboard::Clipboard>,
    _char_w: f32,
) {
    // The inline custom input on the cursor row handles all events when it has
    // focus (which is almost always).  Only fall through here for edge cases
    // where the terminal has no cursor row rendered (e.g. exited process).
    let inline_id = ui.id().with("inline_input");
    if ui.memory(|m| m.has_focus(inline_id)) {
        return; // already handled
    }
    if tab.terminal.exited { return; }

    // Ctrl-only sequences that should work even without inline focus
    let events: Vec<egui::Event> = ui.ctx().input(|i| i.events.clone());
    for event in &events {
        if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
            let bytes: Option<&[u8]> = match key {
                egui::Key::C if modifiers.ctrl && !modifiers.shift => Some(b"\x03"),
                egui::Key::D if modifiers.ctrl => Some(b"\x04"),
                egui::Key::L if modifiers.ctrl => Some(b"\x0c"),
                _ => None,
            };
            if let Some(b) = bytes {
                let _ = tab.terminal.write_to_pty(b);
            }
        }
    }
}

fn handle_key(
    _key: egui::Key,
    _modifiers: egui::Modifiers,
    _input: &mut ridgeback_core::InputBuffer,
    _clipboard: &mut Option<arboard::Clipboard>,
) -> InputAction {
    InputAction::None
}

fn color_to_egui(color: &Color, default: egui::Color32) -> egui::Color32 {
    match color {
        Color::Default => default,
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(*r, *g, *b),
        Color::Indexed(idx) => indexed_color_to_rgb(*idx),
    }
}

fn indexed_color_to_rgb(idx: u8) -> egui::Color32 {
    match idx {
        0  => egui::Color32::from_rgb(0x45, 0x47, 0x5a),
        1  => egui::Color32::from_rgb(0xf3, 0x8b, 0xa8),
        2  => egui::Color32::from_rgb(0xa6, 0xe3, 0xa1),
        3  => egui::Color32::from_rgb(0xf9, 0xe2, 0xaf),
        4  => egui::Color32::from_rgb(0x89, 0xb4, 0xfa),
        5  => egui::Color32::from_rgb(0xf5, 0xc2, 0xe7),
        6  => egui::Color32::from_rgb(0x94, 0xe2, 0xd5),
        7  => egui::Color32::from_rgb(0xba, 0xc2, 0xde),
        8  => egui::Color32::from_rgb(0x58, 0x5b, 0x70),
        9  => egui::Color32::from_rgb(0xf3, 0x8b, 0xa8),
        10 => egui::Color32::from_rgb(0xa6, 0xe3, 0xa1),
        11 => egui::Color32::from_rgb(0xf9, 0xe2, 0xaf),
        12 => egui::Color32::from_rgb(0x89, 0xb4, 0xfa),
        13 => egui::Color32::from_rgb(0xf5, 0xc2, 0xe7),
        14 => egui::Color32::from_rgb(0x94, 0xe2, 0xd5),
        15 => egui::Color32::from_rgb(0xa6, 0xad, 0xc8),
        16..=231 => {
            let n = idx - 16;
            let b = (n % 6) * 51;
            let g = ((n / 6) % 6) * 51;
            let r = (n / 36) * 51;
            egui::Color32::from_rgb(r, g, b)
        }
        232..=255 => {
            let v = (idx - 232) * 10 + 8;
            egui::Color32::from_gray(v)
        }
    }
}

/// Parse a "#RRGGBB" hex colour string into an egui Color32.
/// Returns None if the string is not a valid 6-digit hex colour.
fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 { return None; }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

