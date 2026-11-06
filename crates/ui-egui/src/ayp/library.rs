//! The AYP left column: project header, a grid of ten tabs, the open tab's one-click buttons and
//! its embedded classic panel.
use super::tabs::{Item, Kind, TABS};
use super::vi;
use crate::FilmcraftApp;
use egui::{Rect, Sense, vec2};

const COLS: usize = 5;
const CELL_H: f32 = 52.0;
const ROW_H: f32 = 30.0;

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn run(app: &mut FilmcraftApp, ctx: &egui::Context, cmd: &str, params: &str) -> Result<(), String> {
    let p = serde_json::from_str(params).map_err(|e| e.to_string())?;
    crate::menus::invoke(app, ctx, cmd, p).map(|_| ())
}

pub(crate) fn click_item(app: &mut FilmcraftApp, ctx: &egui::Context, item: &Item) {
    match item.kind {
        Kind::Panel => app.ui.status = "Chọn trong bảng bên dưới".into(),
        Kind::Ai => {}
        Kind::Run => {
            // Lift / Extract act on the words picked in the embedded Text panel, as its own buttons do.
            let picked;
            let params = if matches!(item.cmd, "transcript.lift" | "transcript.extract") {
                let Some((a, b)) = app.ui.transcript_sel else {
                    app.ui.status = "Chưa làm được: chọn chữ trong bản chép lời bên dưới trước".into();
                    return;
                };
                picked = serde_json::json!({"from": a.min(b), "to": a.max(b)}).to_string();
                picked.as_str()
            } else {
                item.params
            };
            for (cmd, params) in std::iter::once((item.cmd, params)).chain(item.then.iter().copied()) {
                if let Err(e) = run(app, ctx, cmd, params) {
                    app.ui.status = format!("Chưa làm được: {}", vi::reason(&e));
                    return;
                }
            }
            if params != item.params {
                app.ui.transcript_sel = None;
            }
        }
    }
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = app.tokens;
    let ctx = ui.ctx().clone();
    let w = rect.width();
    let mut y = rect.top() + 4.0;

    // Header: project · aspect ratio, and the finder.
    let head = Rect::from_min_size(egui::pos2(rect.left() + 6.0, y), vec2(w - 12.0, 26.0));
    let title = match app.session.active_sequence().map(|s| (s.settings.width, s.settings.height)) {
        Some((a, b)) => {
            let g = gcd(a, b).max(1);
            format!("{} · {}:{}", app.session.project.name, a / g, b / g)
        }
        None => app.session.project.name.clone(),
    };
    let find_w = 92.0;
    ui.put(Rect::from_min_size(head.min, vec2(head.width() - find_w - 4.0, head.height())), egui::Label::new(egui::RichText::new(title).strong().color(t.text)).truncate());
    let find = ui.put(Rect::from_min_size(egui::pos2(head.right() - find_w, head.top()), vec2(find_w, head.height())), egui::Button::new("Tìm lệnh…"));
    app.auto.add_response("ayp.find", &find, "Tìm lệnh…");
    if find.clicked() {
        // Task 8 adds the palette; before that the command is unknown and the error is dropped.
        let before = app.ui.status.clone();
        if crate::menus::invoke(app, &ctx, "ayp.palette.open", serde_json::json!({})).is_err() {
            app.ui.status = before;
        }
    }
    y = head.bottom() + 4.0;

    if app.session.active_sequence().is_none() {
        let r = Rect::from_min_size(egui::pos2(rect.left() + 6.0, y), vec2(w - 12.0, 34.0));
        let l = ui.put(r, egui::Label::new(egui::RichText::new("Chưa mở sequence nào. Nhập tệp ở thẻ Phương tiện để bắt đầu.").color(t.text_dim)));
        app.auto.add_response("ayp.hint.noSequence", &l, "Chưa mở sequence nào");
        y = r.bottom() + 4.0;
    }

    // Tab grid: icon over label, two rows.
    let cell_w = (w - 12.0) / COLS as f32;
    for (i, tab) in TABS.iter().enumerate() {
        let (row, col) = (i / COLS, i % COLS);
        let r = Rect::from_min_size(egui::pos2(rect.left() + 6.0 + col as f32 * cell_w, y + row as f32 * CELL_H), vec2(cell_w, CELL_H)).shrink(2.0);
        let resp = ui.interact(r, egui::Id::new(("ayp.tab", tab.id)), Sense::click()).on_hover_text(tab.label);
        app.auto.add_response(&format!("ayp.tab.{}", tab.id), &resp, tab.label);
        let active = app.ui.ayp_tab == tab.id;
        if active {
            ui.painter().rect_filled(r, 6.0, t.accent);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 6.0, t.hover);
        }
        let (icon, text) = if active { (egui::Color32::WHITE, egui::Color32::WHITE) } else { (t.icon, t.text) };
        crate::icons::paint(ui.painter(), Rect::from_center_size(r.center_top() + vec2(0.0, 16.0), vec2(18.0, 18.0)), tab.icon, icon);
        ui.painter().text(r.center_bottom() - vec2(0.0, 10.0), egui::Align2::CENTER_CENTER, tab.label, egui::FontId::proportional(10.5), text);
        if resp.clicked() {
            app.ui.ayp_tab = tab.id.into();
        }
    }
    y += CELL_H * TABS.len().div_ceil(COLS) as f32 + 6.0;

    let tab = TABS.iter().find(|t| t.id == app.ui.ayp_tab).unwrap_or(&TABS[0]);

    // One-click buttons: up to 45% of the column when a panel follows, then they scroll.
    let left = (rect.bottom() - y).max(0.0);
    let want = tab.items.len() as f32 * (ROW_H + 4.0) + 4.0;
    let list_h = if tab.embed.is_some() { want.min(left * 0.45) } else { left };
    let mut list = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(egui::pos2(rect.left() + 6.0, y), vec2(w - 12.0, list_h))));
    egui::ScrollArea::vertical().id_salt("ayp.items").auto_shrink([false, false]).show(&mut list, |ui| {
        for item in tab.items {
            // A command that cannot run right now is dimmed but stays clickable, so the click explains why.
            let ink = if item.kind == Kind::Ai || app.session.is_enabled(item.cmd) { t.text } else { t.text_dim };
            let button = egui::Button::new(egui::RichText::new(item.label).color(ink)).min_size(vec2(ui.available_width(), ROW_H));
            let resp = ui.add_enabled(item.kind != Kind::Ai, button);
            let resp = match item.kind {
                Kind::Ai => resp.on_disabled_hover_text("AI dùng lệnh này — hỏi trong tab AI hoặc Tìm lệnh…"),
                _ => resp.on_hover_text(item.cmd),
            };
            // Scrolled-out buttons are not registered: a click on their rect would land on the toolbar below.
            if ui.clip_rect().contains(resp.rect.center()) {
                app.auto.add_response(&format!("ayp.item.{}", item.key), &resp, item.label);
            }
            if resp.clicked() {
                click_item(app, &ctx, item);
            }
        }
    });
    y += list_h + 4.0;

    if let Some(kind) = tab.embed
        && rect.bottom() - y > 40.0
    {
        crate::panels::show(app, ui, kind, Rect::from_min_max(egui::pos2(rect.left(), y), rect.right_bottom()));
    }
}
