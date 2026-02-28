use arboard::Clipboard;
use egui;
use ridgeback_core::cell::Color;
use ridgeback_core::input_buffer::InputAction;
use ridgeback_core::search::SearchMatch;

use crate::tabs::{TabState, TerminalSelection};

/// Render the terminal viewport for one tab (or one split-pane cell).
///
/// `terminal_id` must be unique per terminal instance (used to scope egui widget IDs).
/// `is_focused_terminal` should be true only for the active tab of the focused group.
/// Keyboard input is ONLY processed when `is_focused_terminal` is true AND no overlay is active.
pub fn show_terminal(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    clipboard: &mut Option<Clipboard>,
    bg_texture: Option<&egui::TextureHandle>,
    allow_shader_repaint: bool,
    terminal_id: u64,
    is_focused_terminal: bool,
) {
    let available = ui.available_size();
    let font_size = 14.0;
    let bg_color = egui::Color32::from_rgb(0x1e, 0x1e, 0x2e);

    let default_fg = parse_hex_color(&tab.text_foreground)
        .unwrap_or(egui::Color32::from_gray(205));

    let shadow_enabled = tab.text_shadow_enabled;
    let shadow_alpha   = ((tab.text_shadow_alpha.clamp(0.0, 1.0)) * 200.0) as u8;

    let char_w = ui.ctx().fonts(|f| {
        f.glyph_width(&egui::FontId::monospace(font_size), ' ')
    });

    let term_rect = ui.available_rect_before_wrap();

    // Layer 1 — solid background
    ui.painter().rect_filled(term_rect, 0.0, bg_color);

    // Layer 2 — background image at 50% opacity
    if let Some(tex) = bg_texture {
        let (iw, ih) = (tex.size()[0] as f32, tex.size()[1] as f32);
        let (rw, rh) = (term_rect.width(), term_rect.height());
        let scale = (rw / iw).max(rh / ih);
        let (sw, sh) = (iw * scale, ih * scale);
        let (u0, v0) = ((sw - rw) / (2.0 * sw), (sh - rh) / (2.0 * sh));
        ui.painter().image(
            tex.id(), term_rect,
            egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(1.0 - u0, 1.0 - v0)),
            egui::Color32::from_white_alpha(128),
        );
    }

    let visible_cells   = tab.terminal.vt.visible_cells().to_vec();
    let scrollback_lines = tab.terminal.vt.scrollback.all_lines_as_strings();
    let cursor_row       = tab.terminal.vt.cursor_row;
    let cursor_col       = tab.terminal.vt.cursor_col;
    let scrollback_count = scrollback_lines.len();

    // Suppress keyboard input when an overlay dialog is active
    let overlay_active = tab.find_overlay.is_open || tab.command_query.is_open;

    // Gather find-overlay match data for highlighting
    let find_matches: Vec<SearchMatch> = tab.find_overlay.matches.clone();
    let find_current = tab.find_overlay.current_match;
    let _find_current_line = tab.find_overlay.current_match_line();

    // Line height for mouse hit-testing
    let _line_height = ui.ctx().fonts(|f| {
        f.row_height(&egui::FontId::monospace(font_size))
    });

    // Track cursor screen position for particles
    let mut cursor_screen_x = 0.0f32;
    let mut cursor_screen_y = 0.0f32;

    egui::ScrollArea::vertical()
        .id_salt(("terminal_output", terminal_id))
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_min_width(available.x);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.visuals_mut().widgets.noninteractive.bg_fill = egui::Color32::TRANSPARENT;
            ui.visuals_mut().extreme_bg_color = egui::Color32::TRANSPARENT;

            // Scrollback lines
            for (line_idx, line) in scrollback_lines.iter().enumerate() {
                let abs_row = line_idx; // absolute row index (scrollback rows are 0-based)
                let text = if line.is_empty() { " " } else { line.as_str() };
                let mut job = egui::text::LayoutJob::default();

                // Apply find-match highlighting on this scrollback line
                let line_matches: Vec<&SearchMatch> = find_matches.iter()
                    .filter(|m| m.line == abs_row)
                    .collect();
                if !line_matches.is_empty() {
                    build_highlighted_line(&mut job, text, &line_matches, &find_matches, find_current, font_size, default_fg);
                } else {
                    job.append(text, 0.0, egui::TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color: default_fg,
                        background: egui::Color32::TRANSPARENT,
                        ..Default::default()
                    });
                }

                let galley = ui.fonts(|f| f.layout_job(job));
                let (resp, painter) = ui.allocate_painter(galley.size(), egui::Sense::click_and_drag());
                if shadow_enabled && shadow_alpha > 0 {
                    painter.rect_filled(
                        resp.rect.expand2(egui::vec2(2.0, 1.0)), 2.0,
                        egui::Color32::from_black_alpha(shadow_alpha),
                    );
                }
                // Draw mouse selection highlight
                paint_selection_highlight(&painter, resp.rect, char_w, abs_row,
                    tab.terminal_selection.as_ref(), text.chars().count());
                painter.galley(resp.rect.min, galley, default_fg);

                // Mouse interaction for selection
                handle_row_mouse(&resp, resp.rect, char_w, abs_row, tab);
            }

            // Visible grid rows
            for (row_idx, row) in visible_cells.iter().enumerate() {
                let abs_row = scrollback_count + row_idx;
                let has_content = row.iter().any(|c| !c.is_empty());
                let is_cursor_row = row_idx == cursor_row;
                if !has_content && !is_cursor_row { continue; }

                if is_cursor_row {
                    // Build styled layout job for cursor row
                    let mut job = egui::text::LayoutJob::default();
                    let mut seg_text = String::new();
                    let mut seg_fg = default_fg;
                    let mut seg_bg = egui::Color32::TRANSPARENT;

                    // Collect find matches on this row
                    let row_find_matches: Vec<&SearchMatch> = find_matches.iter()
                        .filter(|m| m.line == abs_row)
                        .collect();

                    let flush = |job: &mut egui::text::LayoutJob, text: &str, fg: egui::Color32, bg: egui::Color32| {
                        if text.is_empty() { return; }
                        job.append(text, 0.0, egui::TextFormat {
                            font_id: egui::FontId::monospace(font_size),
                            color: fg, background: bg,
                            ..Default::default()
                        });
                    };

                    for (col_idx, cell) in row.iter().enumerate() {
                        let ch = if cell.ch == '\0' { ' ' } else { cell.ch };

                        // Check if this column is part of a find match
                        let find_bg = get_find_highlight_bg(abs_row, col_idx, &row_find_matches, &find_matches, find_current);

                        let (fg, bg) = if col_idx == cursor_col {
                            (bg_color, egui::Color32::from_rgb(205, 214, 244))
                        } else if let Some(fbg) = find_bg {
                            (egui::Color32::BLACK, fbg)
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
                            flush(&mut job, &seg_text, seg_fg, seg_bg);
                            seg_text.clear();
                            seg_fg = fg; seg_bg = bg;
                        }
                        seg_text.push(ch);
                    }
                    flush(&mut job, &seg_text, seg_fg, seg_bg);
                    if job.text.is_empty() {
                        job.append(" ", 0.0, egui::TextFormat {
                            font_id: egui::FontId::monospace(font_size),
                            color: egui::Color32::TRANSPARENT, ..Default::default()
                        });
                    }

                    let row_resp = ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        let galley = ui.fonts(|f| f.layout_job(job));
                        let (resp, painter) = ui.allocate_painter(galley.size(), egui::Sense::click_and_drag());
                        if shadow_enabled && shadow_alpha > 0 {
                            painter.rect_filled(
                                resp.rect.expand2(egui::vec2(2.0, 1.0)), 2.0,
                                egui::Color32::from_black_alpha(shadow_alpha),
                            );
                        }
                        // Draw mouse selection highlight
                        let row_text: String = row.iter().map(|c| if c.ch == '\0' { ' ' } else { c.ch }).collect();
                        paint_selection_highlight(&painter, resp.rect, char_w, abs_row,
                            tab.terminal_selection.as_ref(), row_text.chars().count());
                        painter.galley(resp.rect.min, galley, egui::Color32::WHITE);
                        // Mouse interaction for selection
                        handle_row_mouse(&resp, resp.rect, char_w, abs_row, tab);
                        resp
                    });

                    let row_rect = row_resp.response.rect;

                    // Cursor position in terminal-local coordinates (for particles)
                    cursor_screen_x = (row_rect.left() + cursor_col as f32 * char_w) - term_rect.left();
                    cursor_screen_y = row_rect.center().y - term_rect.top();

                    // Keyboard input — ONLY for the focused terminal AND when no overlay is active
                    if is_focused_terminal && !tab.terminal.exited && !overlay_active {
                        let events: Vec<egui::Event> = ui.ctx().input(|i| i.events.clone());
                        // Track which Ctrl+letter control codes we already forwarded via
                        // Event::Text so we don't double-send them from Event::Key.
                        let mut ctrl_chars_sent: Vec<u8> = Vec::new();
                        let mut paste_handled = false;

                        for ev in &events {
                            match ev {
                                egui::Event::Text(t) => {
                                    for ch in t.chars() {
                                        if ch.is_control() {
                                            // Forward Ctrl+letter control chars (e.g. \x03 for Ctrl+C)
                                            // directly to the PTY — some backends deliver them here
                                            // instead of (or in addition to) Event::Key.
                                            let byte = ch as u8;
                                            // Don't forward \r (Enter), \t (Tab), \x1b (Escape),
                                            // \x7f (Backspace/DEL) — these are handled explicitly
                                            // in the Event::Key arm below.
                                            if byte == 0x0d || byte == 0x09 || byte == 0x1b || byte == 0x7f || byte == 0x08 {
                                                continue;
                                            }
                                            ctrl_chars_sent.push(byte);
                                            let _ = tab.terminal.write_to_pty(&[byte]);
                                            // If this was Ctrl+C (\x03), also clear selection
                                            if byte == 0x03 {
                                                tab.terminal_selection = None;
                                            }
                                            continue;
                                        }
                                        let mut buf = [0u8; 4];
                                        let s = ch.encode_utf8(&mut buf);
                                        let _ = tab.terminal.write_to_pty(s.as_bytes());
                                        // Emit particles via the active particle plugin
                                        let spawned = crate::particle_emit::emit_for_tab(
                                            cursor_screen_x, cursor_screen_y, tab,
                                        );
                                        tab.particles.spawn(spawned);
                                    }
                                }
                                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                                    match key {
                                        egui::Key::Enter      => { let _ = tab.terminal.write_to_pty(b"\r"); }
                                        // Backspace: Ctrl+Backspace = delete word, plain = delete char
                                        egui::Key::Backspace if modifiers.ctrl || modifiers.alt => {
                                            // Ctrl+W (0x17) is the standard "erase word" in terminals
                                            if !ctrl_chars_sent.contains(&0x17) {
                                                let _ = tab.terminal.write_to_pty(b"\x17");
                                            }
                                        }
                                        egui::Key::Backspace  => {
                                            // Send DEL (0x7f) — correct single-char backspace for PTY shells
                                            let _ = tab.terminal.write_to_pty(b"\x7f");
                                        }
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
                                        // Ctrl+C: ALWAYS send SIGINT to PTY (standard terminal behaviour)
                                        egui::Key::C if modifiers.ctrl && !modifiers.shift => {
                                            if !ctrl_chars_sent.contains(&0x03) {
                                                let _ = tab.terminal.write_to_pty(b"\x03");
                                            }
                                            tab.terminal_selection = None;
                                        }
                                        // Ctrl+Shift+C: copy selection or visible screen
                                        egui::Key::C if modifiers.ctrl && modifiers.shift => {
                                            if let Some(ref sel) = tab.terminal_selection {
                                                let text = sel.selected_text(&scrollback_lines, &visible_cells);
                                                if !text.is_empty() {
                                                    if let Some(ref mut cb) = *clipboard { let _ = cb.set_text(text); }
                                                }
                                                tab.terminal_selection = None;
                                            } else {
                                                let lines: Vec<String> = visible_cells
                                                    .iter()
                                                    .map(|row| row.iter().map(|c| if c.ch == '\0' { ' ' } else { c.ch }).collect::<String>())
                                                    .collect();
                                                if let Some(ref mut cb) = *clipboard { let _ = cb.set_text(lines.join("\n")); }
                                            }
                                        }
                                        // Ctrl+letter combos — only send if not already sent via Event::Text
                                        egui::Key::D if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x04) { let _ = tab.terminal.write_to_pty(b"\x04"); } }
                                        egui::Key::L if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x0c) { let _ = tab.terminal.write_to_pty(b"\x0c"); } }
                                        egui::Key::U if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x15) { let _ = tab.terminal.write_to_pty(b"\x15"); } }
                                        egui::Key::A if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x01) { let _ = tab.terminal.write_to_pty(b"\x01"); } }
                                        egui::Key::E if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x05) { let _ = tab.terminal.write_to_pty(b"\x05"); } }
                                        egui::Key::K if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x0b) { let _ = tab.terminal.write_to_pty(b"\x0b"); } }
                                        egui::Key::W if modifiers.ctrl => { if !ctrl_chars_sent.contains(&0x17) { let _ = tab.terminal.write_to_pty(b"\x17"); } }
                                        egui::Key::V if modifiers.ctrl => {
                                            if !paste_handled {
                                                if let Some(ref mut cb) = *clipboard {
                                                    if let Ok(text) = cb.get_text() { let _ = tab.terminal.write_to_pty(text.as_bytes()); }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                // egui may convert Ctrl+C/X/V into these events instead
                                // of (or in addition to) Event::Key on some platforms.
                                egui::Event::Copy => {
                                    // Ctrl+C in a terminal must send SIGINT, not copy.
                                    if !ctrl_chars_sent.contains(&0x03) {
                                        let _ = tab.terminal.write_to_pty(b"\x03");
                                        ctrl_chars_sent.push(0x03);
                                    }
                                    tab.terminal_selection = None;
                                }
                                egui::Event::Cut => {
                                    // Ctrl+X → send \x18 to PTY
                                    let _ = tab.terminal.write_to_pty(b"\x18");
                                }
                                egui::Event::Paste(text) => {
                                    // Ctrl+V → paste into PTY
                                    let _ = tab.terminal.write_to_pty(text.as_bytes());
                                    paste_handled = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    continue;
                }

                // Non-cursor rows — styled label
                let mut job = egui::text::LayoutJob::default();
                let mut seg_text = String::new();
                let mut seg_fg = default_fg;
                let mut seg_bg = egui::Color32::TRANSPARENT;

                // Collect find matches on this row
                let row_find_matches: Vec<&SearchMatch> = find_matches.iter()
                    .filter(|m| m.line == abs_row)
                    .collect();

                let flush = |job: &mut egui::text::LayoutJob, text: &str, fg: egui::Color32, bg: egui::Color32| {
                    if text.is_empty() { return; }
                    job.append(text, 0.0, egui::TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color: fg, background: bg, ..Default::default()
                    });
                };
                for (col_idx, cell) in row.iter().enumerate() {
                    let find_bg = get_find_highlight_bg(abs_row, col_idx, &row_find_matches, &find_matches, find_current);
                    let fg = if find_bg.is_some() { egui::Color32::BLACK } else { color_to_egui(&cell.attrs.fg, default_fg) };
                    let bg = if let Some(fbg) = find_bg {
                        fbg
                    } else {
                        match &cell.attrs.bg {
                            Color::Default => egui::Color32::TRANSPARENT,
                            other => color_to_egui(other, egui::Color32::TRANSPARENT),
                        }
                    };
                    let ch = if cell.ch == '\0' { ' ' } else { cell.ch };
                    if fg != seg_fg || bg != seg_bg {
                        flush(&mut job, &seg_text, seg_fg, seg_bg);
                        seg_text.clear(); seg_fg = fg; seg_bg = bg;
                    }
                    seg_text.push(ch);
                }
                flush(&mut job, &seg_text, seg_fg, seg_bg);
                if job.text.is_empty() {
                    job.append(" ", 0.0, egui::TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color: egui::Color32::TRANSPARENT, ..Default::default()
                    });
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    let galley = ui.fonts(|f| f.layout_job(job));
                    let (resp, painter) = ui.allocate_painter(galley.size(), egui::Sense::click_and_drag());
                    if shadow_enabled && shadow_alpha > 0 {
                        painter.rect_filled(
                            resp.rect.expand2(egui::vec2(2.0, 1.0)), 2.0,
                            egui::Color32::from_black_alpha(shadow_alpha),
                        );
                    }
                    // Draw mouse selection highlight
                    let row_text: String = row.iter().map(|c| if c.ch == '\0' { ' ' } else { c.ch }).collect();
                    paint_selection_highlight(&painter, resp.rect, char_w, abs_row,
                        tab.terminal_selection.as_ref(), row_text.chars().count());
                    painter.galley(resp.rect.min, galley, egui::Color32::WHITE);
                    // Mouse interaction for selection
                    handle_row_mouse(&resp, resp.rect, char_w, abs_row, tab);
                });
            }
        });

    // Right-click context menu (rendered at top level, over everything)
    {
        let scrollback_lines_ref = tab.terminal.vt.scrollback.all_lines_as_strings();
        let visible_cells_ref = tab.terminal.vt.visible_cells().to_vec();
        show_context_menu(ui, tab, clipboard, &scrollback_lines_ref, &visible_cells_ref);
    }

    // Particle overlay (on top of text, below any final shader post-process)
    draw_particles_overlay(ui, term_rect, tab);

    // Shader overlay
    apply_shader_overlay(ui, term_rect, tab, allow_shader_repaint);
}

// ── Shader overlay dispatcher ─────────────────────────────────────────────────

fn apply_shader_overlay(ui: &mut egui::Ui, rect: egui::Rect, tab: &mut TabState, allow_repaint: bool) {
    match tab.shader_effect.plugin_id.as_str() {
        "none" | "" => {}
        "crt"  => draw_crt_overlay(ui, rect, &tab.shader_effect.clone()),
        "fire" => draw_fire_base_overlay(ui, rect, &tab.shader_effect.clone(), allow_repaint),
        _ => {
            // Unknown plugin — placeholder tint
            ui.painter_at(rect).rect_filled(
                rect, 0.0,
                egui::Color32::from_rgba_unmultiplied(80, 40, 160, 30),
            );
        }
    }
}

// ── CRT overlay ───────────────────────────────────────────────────────────────

fn draw_crt_overlay(ui: &mut egui::Ui, rect: egui::Rect, effect: &ridgeback_config::ShaderEffectConfig) {
    let scanline_intensity = effect.param_f32("scanline_intensity", 0.3);
    let curvature          = effect.param_f32("curvature", 0.1);
    let bloom_strength     = effect.param_f32("bloom_strength", 0.15);
    let painter = ui.painter_at(rect);

    let scanline_alpha = (scanline_intensity * 80.0) as u8;
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(scanline_alpha)),
        );
        y += 3.0;
    }
    let va = (curvature * 160.0) as u8;
    // Four vignette edges
    for r in [
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * 0.04, rect.height())),
        egui::Rect::from_min_size(egui::pos2(rect.right() - rect.width() * 0.04, rect.top()), egui::vec2(rect.width() * 0.04, rect.height())),
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), rect.height() * 0.03)),
        egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom() - rect.height() * 0.03), egui::vec2(rect.width(), rect.height() * 0.03)),
    ] {
        painter.rect_filled(r, 0.0, egui::Color32::from_black_alpha(va));
    }
    let bloom_alpha = (bloom_strength * 18.0) as u8;
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0, 255, 80, bloom_alpha));
    painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(15));
}

// ── Fire base flame overlay (bottom edge cellular automaton) ──────────────────

fn draw_fire_base_overlay(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    effect: &ridgeback_config::ShaderEffectConfig,
    allow_repaint: bool,
) {
    let intensity   = effect.param_f32("intensity", 1.0);
    let height_frac = effect.param_f32("height", 0.25);

    // Parse fire colour ramp from params
    let col_base = parse_param_color(effect.param_color("color_base"), 0x1a, 0x00, 0x00);
    let col_mid  = parse_param_color(effect.param_color("color_mid"),  0xff, 0x44, 0x00);
    let col_top  = parse_param_color(effect.param_color("color_top"),  0xff, 0xdd, 0x00);

    let painter = ui.painter_at(rect);
    let flame_h = rect.height() * height_frac * intensity;
    let num_cols = (rect.width() / 4.0).round() as usize;
    let dt = ui.ctx().input(|i| i.unstable_dt).min(0.05) as f64;

    // Simple per-frame noise-based base flame (no stored simulation state needed
    // for the background layer — the tab's ParticleState handles the burst particles)
    let time = ui.ctx().input(|i| i.time);
    let col_w = rect.width() / num_cols as f32;
    for col in 0..num_cols {
        // noise: combine sin waves for organic flicker
        let n = (((time * 4.3 + col as f64 * 0.7).sin()
                + (time * 2.1 + col as f64 * 1.3).sin()) * 0.5 + 0.5) as f32;
        let h = (n * flame_h).max(2.0);
        let num_steps = 8usize;
        for step in 0..num_steps {
            let frac = step as f32 / num_steps as f32;
            let heat = (1.0 - frac) * n;
            if heat < 0.05 { continue; }
            let (r, g, b) = blend_fire_ramp(heat, col_base, col_mid, col_top);
            let alpha = (heat * intensity * 200.0).min(255.0) as u8;
            let py = rect.bottom() - frac * h;
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + col as f32 * col_w, py),
                    egui::vec2(col_w + 0.5, h / num_steps as f32 + 0.5),
                ),
                0.0,
                egui::Color32::from_rgba_unmultiplied(r, g, b, alpha),
            );
        }
    }

    // Bottom shimmer line
    let shimmer = (intensity * 40.0) as u8;
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom() - 3.0), egui::vec2(rect.width(), 3.0)),
        0.0,
        egui::Color32::from_rgba_unmultiplied(col_mid.0, col_mid.1, col_mid.2, shimmer),
    );

    let _ = dt; // suppress unused
    if allow_repaint { ui.ctx().request_repaint(); }
}

fn blend_fire_ramp(heat: f32, base: (u8,u8,u8), mid: (u8,u8,u8), top: (u8,u8,u8)) -> (u8,u8,u8) {
    let h = heat.clamp(0.0, 1.0);
    if h < 0.5 {
        let t = h / 0.5;
        let r = lerp(base.0, mid.0, t);
        let g = lerp(base.1, mid.1, t);
        let b = lerp(base.2, mid.2, t);
        (r, g, b)
    } else {
        let t = (h - 0.5) / 0.5;
        let r = lerp(mid.0, top.0, t);
        let g = lerp(mid.1, top.1, t);
        let b = lerp(mid.2, top.2, t);
        (r, g, b)
    }
}

fn parse_param_color(hex: Option<&str>, dr: u8, dg: u8, db: u8) -> (u8,u8,u8) {
    let hex = match hex { Some(h) => h, None => return (dr, dg, db) };
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return (dr, dg, db); }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(dr);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(dg);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(db);
    (r, g, b)
}

// ── Particle overlay ──────────────────────────────────────────────────────────

/// Draw all live particles for the tab on top of the terminal text.
pub fn draw_particles_overlay(ui: &mut egui::Ui, rect: egui::Rect, tab: &mut TabState) {
    if tab.particles.particles.is_empty() { return; }
    let dt = ui.ctx().input(|i| i.unstable_dt).min(0.05);
    tab.particles.update(dt);

    let painter = ui.painter_at(rect);

    // Smoke first (behind embers)
    for lp in &tab.particles.particles {
        let p = &lp.event;
        if !p.is_smoke { continue; }
        let life_frac = (p.life / 1.6_f32).clamp(0.0, 1.0);
        let alpha = ((life_frac * life_frac) * 55.0) as u8;
        if alpha < 3 { continue; }
        let grey = (60.0 + (1.0 - life_frac) * 80.0) as u8;
        painter.circle_filled(
            egui::pos2(rect.left() + p.x, rect.top() + p.y), p.radius,
            egui::Color32::from_rgba_unmultiplied(grey, grey, grey, alpha),
        );
    }
    // Embers on top
    for lp in &tab.particles.particles {
        let p = &lp.event;
        if p.is_smoke { continue; }
        let life_frac = (p.life / 0.9_f32).clamp(0.0, 1.0);
        let alpha = ((life_frac * life_frac) * 200.0) as u8;
        if alpha < 5 { continue; }
        let heat = p.heat.clamp(0.0, 1.0);
        let (r, g, b) = heat_to_rgb(heat);
        painter.circle_filled(
            egui::pos2(rect.left() + p.x, rect.top() + p.y), p.radius,
            egui::Color32::from_rgba_unmultiplied(r, g, b, alpha),
        );
        painter.circle_filled(
            egui::pos2(rect.left() + p.x, rect.top() + p.y), p.radius * 2.2,
            egui::Color32::from_rgba_unmultiplied(r, g / 2, 0, alpha / 6),
        );
    }

    if !tab.particles.particles.is_empty() {
        ui.ctx().request_repaint();
    }
}

fn heat_to_rgb(heat: f32) -> (u8, u8, u8) {
    let h = heat.clamp(0.0, 1.0);
    if h < 0.25      { (lerp(0, 160, h / 0.25), 0, 0) }
    else if h < 0.55 { (lerp(160, 255, (h - 0.25) / 0.30), lerp(0, 100, (h - 0.25) / 0.30), 0) }
    else if h < 0.80 { (255, lerp(100, 220, (h - 0.55) / 0.25), lerp(0, 20, (h - 0.55) / 0.25)) }
    else             { (255, lerp(220, 255, (h - 0.80) / 0.20), lerp(20, 200, (h - 0.80) / 0.20)) }
}

#[inline(always)]
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8
}


// ── Mouse selection helpers ───────────────────────────────────────────────────

/// Handle mouse press / drag / release on a single row for text selection.
fn handle_row_mouse(
    resp: &egui::Response,
    row_rect: egui::Rect,
    char_w: f32,
    abs_row: usize,
    tab: &mut TabState,
) {
    let col_from_pos = |pos: egui::Pos2| -> usize {
        ((pos.x - row_rect.left()) / char_w).max(0.0) as usize
    };

    // Primary button pressed → start selection
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let col = col_from_pos(pos);
            tab.terminal_selection = Some(TerminalSelection {
                anchor_row: abs_row,
                anchor_col: col,
                head_row: abs_row,
                head_col: col,
            });
            tab.selection_in_progress = true;
        }
    }

    // Dragging → extend selection
    if resp.dragged_by(egui::PointerButton::Primary) && tab.selection_in_progress {
        if let Some(pos) = resp.interact_pointer_pos() {
            let col = col_from_pos(pos);
            if let Some(ref mut sel) = tab.terminal_selection {
                sel.head_row = abs_row;
                sel.head_col = col;
            }
        }
    }

    // Released → finalize selection
    if resp.drag_stopped_by(egui::PointerButton::Primary) {
        tab.selection_in_progress = false;
        // If anchor == head, clear selection (just a click, not a drag)
        if let Some(ref sel) = tab.terminal_selection {
            if sel.anchor_row == sel.head_row && sel.anchor_col == sel.head_col {
                tab.terminal_selection = None;
            }
        }
    }

    // Right-click → store position for context menu
    if resp.secondary_clicked() {
        tab.context_menu_pos = resp.interact_pointer_pos();
    }
}

/// Paint a translucent highlight rectangle over the selected columns in a row.
fn paint_selection_highlight(
    painter: &egui::Painter,
    row_rect: egui::Rect,
    char_w: f32,
    abs_row: usize,
    selection: Option<&TerminalSelection>,
    row_char_count: usize,
) {
    let sel = match selection {
        Some(s) => s,
        None => return,
    };
    let (sr, sc, er, ec) = sel.sorted();
    if abs_row < sr || abs_row > er { return; }

    let start_col = if abs_row == sr { sc } else { 0 };
    let end_col = if abs_row == er { ec } else { row_char_count };

    if start_col >= end_col { return; }

    let x0 = row_rect.left() + start_col as f32 * char_w;
    let x1 = row_rect.left() + end_col as f32 * char_w;
    let sel_rect = egui::Rect::from_min_max(
        egui::pos2(x0, row_rect.top()),
        egui::pos2(x1, row_rect.bottom()),
    );
    painter.rect_filled(
        sel_rect, 0.0,
        egui::Color32::from_rgba_unmultiplied(89, 180, 250, 80),
    );
}

/// Show right-click context menu for a terminal tab. Call after scroll area.
pub fn show_context_menu(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    clipboard: &mut Option<Clipboard>,
    scrollback_lines: &[String],
    visible_cells: &[Vec<ridgeback_core::cell::Cell>],
) {
    if let Some(pos) = tab.context_menu_pos.take() {
        let menu_id = ui.id().with("term_ctx_menu");
        ui.memory_mut(|m| m.open_popup(menu_id));

        let area_resp = egui::Area::new(menu_id)
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(120.0);

                    let has_selection = tab.terminal_selection.is_some();

                    if has_selection {
                        if ui.button("📋 Copy").clicked() {
                            if let Some(ref sel) = tab.terminal_selection {
                                let text = sel.selected_text(scrollback_lines, visible_cells);
                                if !text.is_empty() {
                                    if let Some(ref mut cb) = *clipboard {
                                        let _ = cb.set_text(text);
                                    }
                                }
                            }
                            tab.terminal_selection = None;
                            ui.memory_mut(|m| m.close_popup());
                        }

                        if ui.button("🔍 Search").clicked() {
                            if let Some(ref sel) = tab.terminal_selection {
                                let text = sel.selected_text(scrollback_lines, visible_cells);
                                if !text.is_empty() {
                                    tab.find_overlay.query = text;
                                    if !tab.find_overlay.is_open {
                                        tab.find_overlay.toggle();
                                    }
                                }
                            }
                            tab.terminal_selection = None;
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }

                    if ui.button("📄 Paste").clicked() {
                        if let Some(ref mut cb) = *clipboard {
                            if let Ok(text) = cb.get_text() {
                                let _ = tab.terminal.write_to_pty(text.as_bytes());
                            }
                        }
                        ui.memory_mut(|m| m.close_popup());
                    }

                    if ui.button("🔎 Find…").clicked() {
                        if !tab.find_overlay.is_open {
                            tab.find_overlay.toggle();
                        }
                        ui.memory_mut(|m| m.close_popup());
                    }
                });
            });

        // Close menu if user clicks elsewhere
        let menu_rect = area_resp.response.rect;
        if ui.ctx().input(|i| i.pointer.any_pressed()) {
            if let Some(click_pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                if !menu_rect.contains(click_pos) {
                    ui.memory_mut(|m| m.close_popup());
                }
            }
        }
    }
}

// ── Find-match highlight helpers ──────────────────────────────────────────────

/// Get the highlight background colour for a cell if it falls within a find match.
/// Returns the "current match" colour for the active match, and a dimmer colour for others.
fn get_find_highlight_bg(
    abs_row: usize,
    col: usize,
    row_matches: &[&SearchMatch],
    all_matches: &[SearchMatch],
    current_match_idx: usize,
) -> Option<egui::Color32> {
    for m in row_matches {
        if m.line == abs_row && col >= m.col && col < m.col + m.len {
            // Is this the current (focused) match?
            let is_current = all_matches.get(current_match_idx)
                .map_or(false, |cm| cm.line == m.line && cm.col == m.col && cm.len == m.len);
            if is_current {
                return Some(egui::Color32::from_rgb(250, 180, 50)); // bright orange for active
            } else {
                return Some(egui::Color32::from_rgba_unmultiplied(250, 220, 100, 120)); // dim yellow
            }
        }
    }
    None
}

/// Build a LayoutJob for a scrollback line with find-match highlighting applied.
fn build_highlighted_line(
    job: &mut egui::text::LayoutJob,
    text: &str,
    line_matches: &[&SearchMatch],
    all_matches: &[SearchMatch],
    current_match_idx: usize,
    font_size: f32,
    default_fg: egui::Color32,
) {
    let chars: Vec<char> = text.chars().collect();
    let mut col = 0usize; // char position

    for ch in &chars {
        let highlight = get_find_highlight_bg(
            line_matches.first().map_or(0, |m| m.line),
            col,
            line_matches,
            all_matches,
            current_match_idx,
        );
        let (fg, bg) = if let Some(hbg) = highlight {
            (egui::Color32::BLACK, hbg)
        } else {
            (default_fg, egui::Color32::TRANSPARENT)
        };
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        job.append(s, 0.0, egui::TextFormat {
            font_id: egui::FontId::monospace(font_size),
            color: fg,
            background: bg,
            ..Default::default()
        });
        col += 1;
    }

    if job.text.is_empty() {
        job.append(" ", 0.0, egui::TextFormat {
            font_id: egui::FontId::monospace(font_size),
            color: default_fg,
            background: egui::Color32::TRANSPARENT,
            ..Default::default()
        });
    }
}


// ── Colour helpers ────────────────────────────────────────────────────────────

fn color_to_egui(color: &Color, default: egui::Color32) -> egui::Color32 {
    match color {
        Color::Default => default,
        Color::Rgb(r, g, b) => egui::Color32::from_rgb(*r, *g, *b),
        Color::Indexed(idx) => indexed_color_to_rgb(*idx),
    }
}

fn indexed_color_to_rgb(idx: u8) -> egui::Color32 {
    match idx {
        0  => egui::Color32::from_rgb(0x45,0x47,0x5a),
        1  => egui::Color32::from_rgb(0xf3,0x8b,0xa8),
        2  => egui::Color32::from_rgb(0xa6,0xe3,0xa1),
        3  => egui::Color32::from_rgb(0xf9,0xe2,0xaf),
        4  => egui::Color32::from_rgb(0x89,0xb4,0xfa),
        5  => egui::Color32::from_rgb(0xf5,0xc2,0xe7),
        6  => egui::Color32::from_rgb(0x94,0xe2,0xd5),
        7  => egui::Color32::from_rgb(0xba,0xc2,0xde),
        8  => egui::Color32::from_rgb(0x58,0x5b,0x70),
        9  => egui::Color32::from_rgb(0xf3,0x8b,0xa8),
        10 => egui::Color32::from_rgb(0xa6,0xe3,0xa1),
        11 => egui::Color32::from_rgb(0xf9,0xe2,0xaf),
        12 => egui::Color32::from_rgb(0x89,0xb4,0xfa),
        13 => egui::Color32::from_rgb(0xf5,0xc2,0xe7),
        14 => egui::Color32::from_rgb(0x94,0xe2,0xd5),
        15 => egui::Color32::from_rgb(0xa6,0xad,0xc8),
        16..=231 => { let n = idx-16; egui::Color32::from_rgb((n/36)*51, ((n/6)%6)*51, (n%6)*51) }
        232..=255 => { let v = (idx-232)*10+8; egui::Color32::from_gray(v) }
    }
}

fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 { return None; }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

// Keep InputAction in scope (used by handle_key stub)
#[allow(dead_code)]
fn _unused_input_action() -> InputAction { InputAction::None }
