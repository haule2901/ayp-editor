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

/// One command of a click, through [`super::run`].
fn step(app: &mut FilmcraftApp, ctx: &egui::Context, cmd: &str, params: &str) -> Result<serde_json::Value, String> {
    let r = super::run(app, ctx, cmd, params);
    // Chuẩn hoá âm lượng on a clip nobody has tagged yet: treat it as speech (Dialogue), as a talking clip is.
    if cmd == "essentialSound.autoMatch"
        && r.as_ref().is_err_and(|e| e.contains("no clips with an audio type"))
        && super::run(app, ctx, "essentialSound.setType", r#"{"type":"dialogue"}"#).is_ok()
    {
        return super::run(app, ctx, cmd, params);
    }
    r
}

/// The first empty, unlocked audio track of the open sequence; a new one at the bottom when none is empty.
fn free_audio_track(app: &mut FilmcraftApp) -> Result<u64, String> {
    let empty =
        |app: &FilmcraftApp| app.session.active_sequence().and_then(|q| q.audio_tracks.iter().find(|t| t.items.is_empty() && !t.locked).map(|t| t.id.0));
    if app.session.active_sequence().is_none() {
        return Err("no sequence is open".into());
    }
    if let Some(t) = empty(app) {
        return Ok(t);
    }
    app.session.execute("sequence.addTracks", serde_json::json!({"video": 0, "audio": 1, "audioAfter": "last"})).map_err(|e| e.to_string())?;
    empty(app).ok_or_else(|| "could not add an audio track".into())
}

/// Ghi âm lời: the Timeline's own record toggle (it rolls playback and the countdown), on an empty audio
/// track, so the take never overwrites the clip's sound on A1.
fn voiceover(app: &mut FilmcraftApp, start: bool) -> Result<(), String> {
    let recording = app.session.voiceover.recording();
    if start == recording {
        return Err(if start { "a voice-over is already recording" } else { "no voice-over is recording" }.into());
    }
    let track = if start { Some(free_audio_track(app)?) } else { None };
    crate::panels::voiceover::toggle(app, track)?;
    if start {
        app.ui.status = "Đang ghi âm lời… bấm Dừng ghi âm để dừng".into();
    }
    Ok(())
}

pub(crate) fn click_item(app: &mut FilmcraftApp, ctx: &egui::Context, item: &'static Item) {
    if item.cmd == "transcript.generate" {
        // Chép lời runs as a background job; the status line follows it (run_pending, every frame).
        if app.ayp_transcribing.is_some() {
            app.ui.status = "Đang chép lời rồi".into();
            return;
        }
        let mut params: serde_json::Value = serde_json::from_str(item.params).unwrap_or_else(|_| serde_json::json!({}));
        params["background"] = true.into();
        if let Ok(r) = step(app, ctx, item.cmd, &params.to_string()) {
            app.ayp_transcribing = r.get("job").and_then(serde_json::Value::as_u64);
            app.ui.status = "Đang chép lời… 0%".into();
        }
        return;
    }
    run_item(app, ctx, item);
}

/// Every frame: show how far the Chép lời job is, and say when it is over.
pub(crate) fn run_pending(app: &mut FilmcraftApp, _ctx: &egui::Context) {
    let Some(id) = app.ayp_transcribing else { return };
    let job = app.session.jobs.iter().find(|j| j.id == id);
    let running = !app.session.transcript_jobs.is_empty() && job.is_some_and(|j| !j.progress.finished.load(std::sync::atomic::Ordering::Relaxed));
    if running {
        let pct = job.map(|j| (j.progress.fraction() * 100.0) as u32).unwrap_or(0).min(100);
        app.ui.status = format!("Đang chép lời… {pct}%");
        return;
    }
    app.ayp_transcribing = None;
    let err = job.and_then(|j| j.result.lock().unwrap_or_else(|e| e.into_inner()).clone()).and_then(Result::err);
    app.ui.status = match err {
        None => "Đã chép lời xong".into(),
        Some(e) if e == "stopped" => "Đã dừng chép lời".into(),
        Some(e) => format!("Chưa làm được: {}", vi::reason(&e)),
    };
}

fn run_item(app: &mut FilmcraftApp, ctx: &egui::Context, item: &Item) {
    if item.kind == Kind::Run && matches!(item.cmd, "audio.voiceover.start" | "audio.voiceover.stop") {
        app.ui.status.clear();
        if let Err(e) = voiceover(app, item.cmd == "audio.voiceover.start") {
            app.ui.status = format!("Chưa làm được: {}", vi::reason(&e));
        }
        return;
    }
    match item.kind {
        Kind::Panel => {
            // from Tìm lệnh… the tab may not be on show: open it, with its panel
            if let Some(tab) = TABS.iter().find(|t| t.items.iter().any(|i| i.key == item.key)) {
                let _ = super::open_tab(app, tab.id);
            }
            app.ui.status = "Chọn trong bảng bên dưới".into();
        }
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
            // one click, one undo step: a failed `then` takes the whole click back
            let first = super::one_step(app, item.label, |app| {
                let mut first = None;
                for (cmd, params) in std::iter::once((item.cmd, params)).chain(item.then.iter().copied()) {
                    let v = step(app, ctx, cmd, params)?;
                    first.get_or_insert(v);
                }
                Ok(first)
            });
            match first {
                // the failing step has said why
                Err(_) => return,
                // Xoá khoảng lặng / từ đệm that found nothing would otherwise look like a dead button.
                Ok(Some(v)) if v.get("removed").and_then(serde_json::Value::as_u64) == Some(0) => {
                    app.ui.status = "Không tìm thấy chỗ nào để xoá".into();
                }
                Ok(_) => {}
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
    ui.put(
        Rect::from_min_size(head.min, vec2(head.width() - find_w - 4.0, head.height())),
        egui::Label::new(egui::RichText::new(title).strong().color(t.text)).truncate(),
    );
    let find = ui
        .put(Rect::from_min_size(egui::pos2(head.right() - find_w, head.top()), vec2(find_w, head.height())), egui::Button::new("Tìm lệnh…"))
        .on_hover_text("Tìm và chạy mọi lệnh (⌘⇧P)");
    app.auto.add_response("ayp.find", &find, "Tìm lệnh…");
    if find.clicked() {
        let _ = super::run(app, &ctx, "ayp.palette.open", "{}");
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

    // One-click buttons: up to 45% of the column when a panel follows (60% for a long list), then they scroll.
    let left = (rect.bottom() - y).max(0.0);
    let want = tab.items.len() as f32 * (ROW_H + 4.0) + 4.0;
    let cap = if tab.items.len() > 8 { 0.6 } else { 0.45 };
    let list_h = if tab.embed.is_some() { want.min(left * cap) } else { left };
    let mut list = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(egui::pos2(rect.left() + 6.0, y), vec2(w - 12.0, list_h))));
    egui::ScrollArea::vertical().id_salt("ayp.items").auto_shrink([false, false]).show(&mut list, |ui| {
        for item in tab.items {
            // A command that cannot run right now is dimmed but stays clickable, so the click explains why.
            let ink = if item.kind == Kind::Ai || app.session.is_enabled(item.cmd) { t.text } else { t.text_dim };
            let button = egui::Button::new(egui::RichText::new(item.label).color(ink)).min_size(vec2(ui.available_width(), ROW_H));
            let resp = ui.add_enabled(item.kind != Kind::Ai, button);
            let resp = match item.kind {
                Kind::Ai => resp.on_disabled_hover_text("Sẽ có ở bản sau (AI)"),
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn demo() -> (FilmcraftApp, egui::Context) {
        let mut s = filmcraft_engine::Session::default();
        s.execute("file.openDemoProject", serde_json::json!({})).unwrap();
        (FilmcraftApp::new(s), egui::Context::default())
    }

    /// The demo project with a transcriber that answers at once (the test build has no Whisper).
    pub(crate) fn demo_transcribing() -> (FilmcraftApp, egui::Context) {
        let (mut app, ctx) = demo();
        app.session.transcriber = Some(std::sync::Arc::new(filmcraft_speech::FixedTranscriber { transcript: Default::default(), id: "fixed".into() }));
        (app, ctx)
    }

    fn item(key: &str) -> &'static Item {
        TABS.iter().flat_map(|t| t.items).find(|i| i.key == key).unwrap()
    }

    /// Chép lời is a background job: the click returns at once, the status follows the job, and the
    /// transcript lands in one undo step; a second click meanwhile says it is already running.
    #[test]
    fn transcribe_button_does_not_block() {
        let (mut app, ctx) = demo_transcribing();
        let before = app.session.history.undo.len();
        click_item(&mut app, &ctx, item("captions.transcribe"));
        assert!(app.ui.status.starts_with("Đang chép lời… "), "{}", app.ui.status);
        assert!(app.ayp_transcribing.is_some());
        click_item(&mut app, &ctx, item("transcript.generate"));
        assert_eq!(app.ui.status, "Đang chép lời rồi");
        let t0 = std::time::Instant::now();
        while app.ayp_transcribing.is_some() {
            app.session.poll_persistence();
            run_pending(&mut app, &ctx);
            assert!(t0.elapsed().as_secs() < 60, "{}", app.ui.status);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.ui.status, "Đã chép lời xong");
        assert_eq!(app.session.history.undo.len(), before + 1);
    }
}
