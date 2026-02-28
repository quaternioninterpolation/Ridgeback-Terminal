//! Per-group tab bar rendering with context menu and overflow.
//!
//! Each tab group in the split layout gets its own tab bar header.
//! This module draws the tab strip, handles clicks, context menus,
//! and the overflow "⋯" menu for hidden tabs.

use egui;
use crate::tabs::TabGroup;
use crate::tab_drag::TabDragState;

/// Actions that the tab bar can produce, processed by `app.rs` after rendering.
#[derive(Debug, Clone)]
pub enum TabBarAction {
    ClickTab(usize),
    CloseTab(usize),
    CloseOtherTabs(usize),
    CloseTabsToRight(usize),
    DragStart { tab_idx: usize, origin: egui::Pos2 },
    NewTab { profile_key: String },
    SplitRight(usize),
    SplitDown(usize),
    TearOff(usize),
    CloseGroup,
}

/// Draw the tab bar for a single group inside `header_rect`.
/// Returns a list of actions to be processed by the caller.
pub fn draw_group_tab_bar(
    ui: &mut egui::Ui,
    header_rect: egui::Rect,
    group: &TabGroup,
    drag_state: &TabDragState,
    is_focused: bool,
    profile_names: &[(String, String)], // (key, display_name) pairs
) -> Vec<TabBarAction> {
    let mut actions = Vec::new();

    // Background
    let bg = if is_focused {
        egui::Color32::from_gray(28)
    } else {
        egui::Color32::from_gray(20)
    };
    ui.painter().rect_filled(header_rect, 0.0, bg);

    // Focused indicator line at top
    if is_focused {
        let bar = egui::Rect::from_min_size(
            header_rect.min,
            egui::vec2(header_rect.width(), 2.0),
        );
        ui.painter().rect_filled(bar, 0.0, egui::Color32::from_rgba_unmultiplied(137, 180, 250, 180));
    }

    let active = group.active_index();
    let tab_count = group.count();

    // Reserve space for + button and overflow
    let button_w = 28.0;
    let overflow_w = 24.0;
    let available_tabs_w = header_rect.width() - button_w - 4.0;

    // Compute tab widths and determine which fit
    let mut tab_widths: Vec<f32> = Vec::new();
    let mut total_w = 0.0;

    for i in 0..tab_count {
        let Some(td) = group.tab(i) else { continue };
        let open_ease = 1.0 - (1.0 - td.open_anim).powi(3);
        let close_ease = td.close_anim.powi(2);
        let anim = if td.closing { 1.0 - close_ease } else { open_ease };
        let full_w = (td.tab_title.len() as f32 * 7.5 + 52.0).min(200.0).max(80.0);
        let w = full_w * anim;
        tab_widths.push(w);
        total_w += w + 2.0; // 2px spacing
    }

    // Check if we need overflow
    let needs_overflow = total_w > available_tabs_w;
    let tabs_area_w = if needs_overflow { available_tabs_w - overflow_w } else { available_tabs_w };

    // Draw tabs
    let mut x = header_rect.left() + 2.0;
    let mut tab_rects: Vec<(usize, egui::Rect)> = Vec::new();
    let mut hidden_tabs: Vec<usize> = Vec::new();

    for i in 0..tab_count {
        let Some(td) = group.tab(i) else { continue };
        let w = tab_widths.get(i).copied().unwrap_or(80.0);

        if x + w > header_rect.left() + tabs_area_w + 2.0 && needs_overflow {
            hidden_tabs.push(i);
            continue;
        }

        let (title, open_t, close_t, is_closing) = (
            td.tab_title.clone(), td.open_anim, td.close_anim, td.closing,
        );
        let is_active = i == active;
        let is_dragged = match &drag_state.dragging {
            Some(ds) => ds.tab_idx == i,
            None => false,
        };
        let open_ease = 1.0 - (1.0 - open_t).powi(3);
        let close_ease = close_t.powi(2);
        let anim = if is_closing { 1.0 - close_ease } else { open_ease };
        let alpha = (anim * 255.0) as u8;

        let (tab_bg, tab_fg) = if is_active || is_dragged {
            (egui::Color32::from_gray(55), egui::Color32::WHITE)
        } else {
            (egui::Color32::from_gray(35), egui::Color32::from_gray(170))
        };

        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(x, header_rect.top()),
            egui::vec2(w, header_rect.height()),
        );
        tab_rects.push((i, tab_rect));

        if ui.is_rect_visible(tab_rect) && w > 4.0 {
            // Tab background
            let bg_a = egui::Color32::from_rgba_unmultiplied(
                tab_bg.r(), tab_bg.g(), tab_bg.b(), ((tab_bg.a() as f32) * anim) as u8,
            );
            ui.painter().rect_filled(tab_rect, 4.0, bg_a);

            // Active underline
            if is_active && !is_closing {
                let bar = egui::Rect::from_min_size(
                    egui::pos2(tab_rect.left() + 4.0, tab_rect.bottom() - 2.0),
                    egui::vec2((tab_rect.width() - 8.0) * open_ease, 2.0),
                );
                ui.painter().rect_filled(bar, 0.0,
                    egui::Color32::from_rgba_unmultiplied(137, 180, 250, alpha));
            }

            // Title text area (clickable + draggable)
            let title_rect = egui::Rect::from_min_max(
                tab_rect.min,
                egui::pos2(tab_rect.right() - 20.0, tab_rect.bottom()),
            );
            let tr = ui.interact(
                title_rect,
                egui::Id::new(("gt", group.id, i)),
                egui::Sense::click_and_drag(),
            );
            ui.painter().text(
                egui::pos2(title_rect.left() + 8.0, title_rect.center().y),
                egui::Align2::LEFT_CENTER,
                &title,
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgba_unmultiplied(tab_fg.r(), tab_fg.g(), tab_fg.b(), alpha),
            );

            if tr.clicked() && !is_closing {
                actions.push(TabBarAction::ClickTab(i));
            }
            if tr.middle_clicked() {
                actions.push(TabBarAction::CloseTab(i));
            }
            if tr.drag_started() && !is_closing {
                let origin = tr.interact_pointer_pos().unwrap_or(tab_rect.center());
                actions.push(TabBarAction::DragStart { tab_idx: i, origin });
            }

            // Context menu
            tr.context_menu(|ui| {
                if ui.button("Close Tab").clicked() {
                    actions.push(TabBarAction::CloseTab(i));
                    ui.close_menu();
                }
                if ui.button("Close Other Tabs").clicked() {
                    actions.push(TabBarAction::CloseOtherTabs(i));
                    ui.close_menu();
                }
                if ui.button("Close Tabs to the Right").clicked() {
                    actions.push(TabBarAction::CloseTabsToRight(i));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Split Right").clicked() {
                    actions.push(TabBarAction::SplitRight(i));
                    ui.close_menu();
                }
                if ui.button("Split Down").clicked() {
                    actions.push(TabBarAction::SplitDown(i));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Tear Off to New Group").clicked() {
                    actions.push(TabBarAction::TearOff(i));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Close Group").clicked() {
                    actions.push(TabBarAction::CloseGroup);
                    ui.close_menu();
                }
            });

            // Close × button
            let cr = egui::Rect::from_center_size(
                egui::pos2(tab_rect.right() - 12.0, tab_rect.center().y),
                egui::vec2(16.0, 16.0),
            );
            let xr = ui.interact(cr, egui::Id::new(("gx", group.id, i)), egui::Sense::click());
            if xr.hovered() {
                ui.painter().rect_filled(cr, 3.0, egui::Color32::from_gray(80));
            }
            ui.painter().text(
                cr.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::X,
                egui::FontId::proportional(10.0),
                if xr.hovered() {
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                } else {
                    egui::Color32::from_rgba_unmultiplied(140, 140, 140, alpha)
                },
            );
            if xr.clicked() {
                actions.push(TabBarAction::CloseTab(i));
            }
        }

        x += w + 2.0;
    }

    // Overflow ⋯ button
    if needs_overflow && !hidden_tabs.is_empty() {
        let overflow_rect = egui::Rect::from_min_size(
            egui::pos2(header_rect.left() + tabs_area_w + 2.0, header_rect.top()),
            egui::vec2(overflow_w, header_rect.height()),
        );
        let or = ui.interact(
            overflow_rect,
            egui::Id::new(("overflow", group.id)),
            egui::Sense::click(),
        );
        let overflow_color = if or.hovered() {
            egui::Color32::from_gray(200)
        } else {
            egui::Color32::from_gray(140)
        };
        ui.painter().text(
            overflow_rect.center(),
            egui::Align2::CENTER_CENTER,
            "⋯",
            egui::FontId::proportional(14.0),
            overflow_color,
        );

        if or.clicked() {
            ui.memory_mut(|m| m.toggle_popup(or.id));
        }
        egui::popup_below_widget(
            ui,
            or.id,
            &or,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui: &mut egui::Ui| {
                ui.set_min_width(180.0);
                ui.set_max_height(300.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for i in 0..tab_count {
                        let Some(td) = group.tab(i) else { continue };
                        if td.closing { continue; }
                        let is_hidden = hidden_tabs.contains(&i);
                        let is_active = i == active;
                        let label = if is_active {
                            format!("▸ {}", td.tab_title)
                        } else {
                            td.tab_title.clone()
                        };
                        let text = if is_hidden {
                            egui::RichText::new(&label).color(egui::Color32::WHITE)
                        } else {
                            egui::RichText::new(&label).color(egui::Color32::from_gray(100))
                        };
                        if ui.button(text).clicked() {
                            actions.push(TabBarAction::ClickTab(i));
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                });
            },
        );
    }

    // + new tab button (with profile picker popup)
    let plus_rect = egui::Rect::from_min_size(
        egui::pos2(header_rect.right() - button_w, header_rect.top()),
        egui::vec2(button_w, header_rect.height()),
    );
    let pr = ui.interact(plus_rect, egui::Id::new(("gplus", group.id)), egui::Sense::click());
    let plus_color = if pr.hovered() {
        egui::Color32::from_gray(220)
    } else {
        egui::Color32::from_gray(160)
    };
    ui.painter().text(
        plus_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::PLUS,
        egui::FontId::proportional(14.0),
        plus_color,
    );
    if pr.clicked() {
        if profile_names.len() == 1 {
            // Only one profile — open directly without popup
            actions.push(TabBarAction::NewTab { profile_key: profile_names[0].0.clone() });
        } else {
            ui.memory_mut(|m| m.toggle_popup(pr.id));
        }
    }
    if profile_names.len() > 1 {
        egui::popup_below_widget(
            ui,
            pr.id,
            &pr,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui: &mut egui::Ui| {
                ui.set_min_width(160.0);
                for (key, display_name) in profile_names {
                    if ui.button(display_name).clicked() {
                        actions.push(TabBarAction::NewTab { profile_key: key.clone() });
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            },
        );
    }

    actions
}

/// Draw the drag-to-split drop zone preview overlay.
/// Call this after all panes are rendered, while a drag is active.
pub fn draw_drop_zone_preview(
    ui: &mut egui::Ui,
    zones: &[crate::split_pane::DropZone],
    pointer_pos: egui::Pos2,
    source_group_id: usize,
) -> Option<crate::split_pane::DropZone> {
    let mut hovered_zone: Option<&crate::split_pane::DropZone> = None;

    for zone in zones {
        if zone.rect.contains(pointer_pos) {
            // Don't show center drop on our own group
            if zone.group_id == source_group_id && zone.side == crate::split_pane::DropSide::Center {
                continue;
            }
            hovered_zone = Some(zone);
            break;
        }
    }

    if let Some(zone) = hovered_zone {
        // Compute the preview rect (the half that would become the new pane)
        let preview_rect = match zone.side {
            crate::split_pane::DropSide::Left => {
                egui::Rect::from_min_size(zone.rect.min, egui::vec2(zone.rect.width(), zone.rect.height()))
            }
            crate::split_pane::DropSide::Right => zone.rect,
            crate::split_pane::DropSide::Top => zone.rect,
            crate::split_pane::DropSide::Bottom => zone.rect,
            crate::split_pane::DropSide::Center => zone.rect,
        };

        // Animated fade-in
        let anim_id = egui::Id::new(("drop_zone_anim", zone.group_id, zone.side as u8));
        let alpha = ui.ctx().animate_value_with_time(anim_id, 0.35, 0.15);

        let color = egui::Color32::from_rgba_unmultiplied(
            137, 180, 250, (alpha * 255.0) as u8,
        );
        let stroke_color = egui::Color32::from_rgba_unmultiplied(
            137, 180, 250, (alpha * 255.0 * 2.0).min(255.0) as u8,
        );

        ui.painter().rect_filled(preview_rect, 4.0, color);
        ui.painter().rect_stroke(preview_rect, 4.0, egui::Stroke::new(2.0, stroke_color));

        return Some(zone.clone());
    }

    None
}


