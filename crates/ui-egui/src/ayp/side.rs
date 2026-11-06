//! The AYP right column: Thông số (the classic Properties panel) | AI (placeholder until plan 2).
use crate::FilmcraftApp;
use crate::state::AypSideTab;
use egui::{Rect, vec2};

const TAB_H: f32 = 28.0;

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = app.tokens;
    let ctx = ui.ctx().clone();
    let w = (rect.width() - 12.0) / 2.0;
    for (i, (tab, id, label)) in [(AypSideTab::Props, "ayp.side.props", "Thông số"), (AypSideTab::Ai, "ayp.side.ai", "AI")].into_iter().enumerate() {
        let r = Rect::from_min_size(egui::pos2(rect.left() + 6.0 + i as f32 * w, rect.top() + 4.0), vec2(w, TAB_H)).shrink(2.0);
        let active = app.ui.ayp_side == tab;
        let ink = if active { egui::Color32::WHITE } else { t.text };
        let b = egui::Button::new(egui::RichText::new(label).color(ink)).fill(if active { t.accent } else { egui::Color32::TRANSPARENT });
        let resp = ui.put(r, b);
        app.auto.add_response(id, &resp, label);
        if resp.clicked() {
            let _ = crate::menus::invoke(app, &ctx, id, serde_json::json!({}));
        }
    }
    let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + TAB_H + 8.0), rect.right_bottom());
    match app.ui.ayp_side {
        AypSideTab::Props if app.session.state.selection.is_empty() => {
            let r = Rect::from_min_size(body.min + vec2(6.0, 4.0), vec2(body.width() - 12.0, 34.0));
            let l = ui.put(r, egui::Label::new(egui::RichText::new("Chọn một clip để xem thông số").color(t.text_dim)));
            app.auto.add_response("ayp.hint.selectClip", &l, "Chọn một clip để xem thông số");
        }
        AypSideTab::Props => crate::panels::show(app, ui, crate::dock::PanelKind::Properties, body),
        AypSideTab::Ai => {
            let r = Rect::from_min_size(body.min + vec2(6.0, 4.0), vec2(body.width() - 12.0, 60.0));
            ui.put(r, egui::Label::new(egui::RichText::new("Khung chat AI sẽ có ở bản kế tiếp. Trong lúc chờ: bấm Tìm lệnh…").color(t.text_dim)));
            let b = Rect::from_min_size(egui::pos2(body.left() + 6.0, r.bottom() + 6.0), vec2(110.0, 28.0));
            let resp = ui.put(b, egui::Button::new("Tìm lệnh…"));
            app.auto.add_response("ayp.side.openPalette", &resp, "Tìm lệnh…");
            if resp.clicked() {
                // Task 8 adds the palette; until then the command is unknown and the error is dropped.
                let before = app.ui.status.clone();
                if crate::menus::invoke(app, &ctx, "ayp.palette.open", serde_json::json!({})).is_err() {
                    app.ui.status = before;
                }
            }
        }
    }
}
