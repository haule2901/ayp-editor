//! The strip above the Timeline: big, labelled buttons for the everyday edits.
use super::vi;
use crate::FilmcraftApp;
use crate::icons::Icon;
use egui::{Rect, Sense, vec2};

/// (label, command, params, icon). Export is drawn separately, flush right.
const BUTTONS: [(&str, &str, &str, Icon); 10] = [
    ("Hoàn tác", "edit.undo", "{}", Icon::Undo),
    ("Làm lại", "edit.redo", "{}", Icon::Redo),
    ("Tách", "sequence.addEdit", "{}", Icon::Razor),
    ("Xoá", "edit.rippleDelete", "{}", Icon::Trash),
    ("Dừng hình", "clip.frameHold", "{}", Icon::Camera),
    ("Liên kết", "clip.link", "{}", Icon::Link),
    ("Hút", "sequence.snap", "{}", Icon::Magnet),
    ("Marker", "markers.add", r#"{"target":"program"}"#, Icon::Marker),
    ("Thu nhỏ", "view.zoomOut", "{}", Icon::Zoom),
    ("Phóng to", "view.zoomIn", "{}", Icon::Zoom),
];
const EXPORT: (&str, &str, &str, Icon) = ("Xuất", "file.exportMedia", "{}", Icon::Export);
const H: f32 = 28.0;

fn run(app: &mut FilmcraftApp, ctx: &egui::Context, cmd: &str, params: &str) {
    let p = serde_json::from_str(params).unwrap_or_default();
    if let Err(e) = crate::menus::invoke(app, ctx, cmd, p) {
        app.ui.status = format!("Chưa làm được: {}", vi::reason(&e));
    }
}

fn button(app: &mut FilmcraftApp, ui: &mut egui::Ui, ctx: &egui::Context, r: Rect, b: &(&str, &str, &str, Icon), on: bool, accent: bool) {
    let t = app.tokens;
    let resp = ui.interact(r, egui::Id::new(("ayp.tool", b.1)), Sense::click()).on_hover_text(b.0);
    app.auto.add_response(&format!("ayp.tool.{}", b.1), &resp, b.0);
    // A command that cannot run now is dimmed but stays clickable, so the click explains why.
    let live = app.session.is_enabled(b.1);
    if accent || on {
        ui.painter().rect_filled(r, 6.0, t.accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 6.0, t.hover);
    }
    let ink = if accent || on { egui::Color32::WHITE } else if live { t.text } else { t.text_dim };
    let icon = if accent || on { egui::Color32::WHITE } else if live { t.icon } else { t.text_dim };
    crate::icons::paint(ui.painter(), Rect::from_center_size(egui::pos2(r.left() + 16.0, r.center().y), vec2(16.0, 16.0)), b.3, icon);
    ui.painter().text(egui::pos2(r.left() + 29.0, r.center().y), egui::Align2::LEFT_CENTER, b.0, egui::FontId::proportional(12.5), ink);
    if resp.clicked() {
        run(app, ctx, b.1, b.2);
    }
}

fn width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter().layout_no_wrap(label.into(), egui::FontId::proportional(12.5), egui::Color32::WHITE).size().x + 29.0 + 10.0
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    let y = rect.center().y - H / 2.0;
    let mut x = rect.left() + 6.0;
    for b in &BUTTONS {
        let w = width(ui, b.0);
        let on = b.1 == "sequence.snap" && app.session.state.snapping;
        button(app, ui, &ctx, Rect::from_min_size(egui::pos2(x, y), vec2(w, H)), b, on, false);
        x += w + 4.0;
    }
    let w = width(ui, EXPORT.0) + 8.0;
    button(app, ui, &ctx, Rect::from_min_size(egui::pos2(rect.right() - 6.0 - w, y), vec2(w, H)), &EXPORT, false, true);
}
