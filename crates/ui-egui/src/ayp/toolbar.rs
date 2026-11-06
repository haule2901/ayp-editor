//! The strip above the Timeline: big, labelled buttons for the everyday edits.
use crate::FilmcraftApp;
use crate::icons::Icon;
use egui::{Rect, Sense, vec2};

/// (label, command, params, icon). Export is drawn separately, flush right.
pub(crate) const BUTTONS: [(&str, &str, &str, Icon); 10] = [
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
pub(crate) const EXPORT: (&str, &str, &str, Icon) = ("Xuất", "file.exportMedia", "{}", Icon::Export);
const H: f32 = 28.0;

fn button(app: &mut FilmcraftApp, ui: &mut egui::Ui, ctx: &egui::Context, r: Rect, b: &(&str, &str, &str, Icon), on: bool, accent: bool, label: bool) {
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
    if label {
        ui.painter().text(egui::pos2(r.left() + 29.0, r.center().y), egui::Align2::LEFT_CENTER, b.0, egui::FontId::proportional(12.5), ink);
    }
    if resp.clicked() {
        let _ = super::run(app, ctx, b.1, b.2);
    }
}

fn width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter().layout_no_wrap(label.into(), egui::FontId::proportional(12.5), egui::Color32::WHITE).size().x + 29.0 + 10.0
}

/// Indices into BUTTONS that lose their label first when the row is too wide (lowest priority first).
const SHRINK: [usize; 5] = [9, 8, 7, 5, 4];
const ICON_W: f32 = 32.0;

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    let y = rect.center().y - H / 2.0;
    let ex_w = width(ui, EXPORT.0) + 8.0;
    let ex_left = rect.right() - 6.0 - ex_w;
    let limit = ex_left - 4.0;
    let mut icon_only = [false; 10];
    let widths = |io: &[bool; 10]| -> Vec<f32> { BUTTONS.iter().enumerate().map(|(i, b)| if io[i] { ICON_W } else { width(ui, b.0) }).collect() };
    let total = |w: &[f32]| rect.left() + 6.0 + w.iter().map(|x| x + 4.0).sum::<f32>() - 4.0;
    let mut w = widths(&icon_only);
    for i in SHRINK {
        if total(&w) <= limit {
            break;
        }
        icon_only[i] = true;
        w = widths(&icon_only);
    }
    let mut x = rect.left() + 6.0;
    for (i, b) in BUTTONS.iter().enumerate() {
        if x + w[i] > limit {
            break; // ponytail: buttons that cannot fit are dropped, not scrolled; the command palette still reaches them
        }
        let on = b.1 == "sequence.snap" && app.session.state.snapping;
        button(app, ui, &ctx, Rect::from_min_size(egui::pos2(x, y), vec2(w[i], H)), b, on, false, !icon_only[i]);
        x += w[i] + 4.0;
    }
    button(app, ui, &ctx, Rect::from_min_size(egui::pos2(ex_left, y), vec2(ex_w, H)), &EXPORT, false, true, true);
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_button_has_json_object_params() {
        for b in super::BUTTONS.iter().chain([&super::EXPORT]) {
            let v: serde_json::Value = serde_json::from_str(b.2).unwrap_or_else(|e| panic!("{}: {e}", b.1));
            assert!(v.is_object(), "{}", b.1);
        }
    }
}
