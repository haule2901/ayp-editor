//! "Tìm lệnh…": one box that finds and runs every command, with or without Vietnamese accents.
use super::{tabs::TABS, vi};
use crate::FilmcraftApp;
use egui::{Align2, FontId, Key, Modifiers, Sense, vec2};

/// What running a hit does: the same as clicking it where it lives.
#[derive(Clone, Copy)]
pub enum Run {
    /// A plain command, no parameters.
    Command(&'static str),
    /// An item of an AYP tab (library.rs's click path).
    Item(&'static super::tabs::Item),
    /// A toolbar button: command and JSON params.
    Tool(&'static str, &'static str),
}

pub struct Hit {
    /// Command id, or `effects.apply.<effect>` for a tab item without its own command.
    pub id: &'static str,
    pub label_vi: String,
    pub label_en: &'static str,
    pub enabled: Result<(), String>,
    pub score: i32,
    pub run: Run,
}

/// Lower-case and strip Vietnamese marks: "Xoá khoảng lặng Đ" -> "xoa khoang lang d".
/// Hand-written table (no crate); decomposed input (base letter + combining mark) folds too.
pub fn fold(s: &str) -> String {
    const GROUPS: [(&str, char); 7] =
        [("àáảãạăằắẳẵặâầấẩẫậ", 'a'), ("èéẻẽẹêềếểễệ", 'e'), ("ìíỉĩị", 'i'), ("òóỏõọôồốổỗộơờớởỡợ", 'o'), ("ùúủũụưừứửữự", 'u'), ("ỳýỷỹỵ", 'y'), ("đ", 'd')];
    s.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .map(|c| GROUPS.iter().find(|(g, _)| g.contains(c)).map_or(c, |(_, b)| *b))
        .collect()
}

/// Folded text with every non-alphanumeric run turned into one space, so " tok" means "word starts with tok".
fn words(s: &str) -> String {
    let mut out = String::from(" ");
    for c in fold(s).chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out
}

/// (word-prefix points, substring points) for a token found in one field.
fn hit(tok: &str, field: &str, (pre, sub): (i32, i32)) -> i32 {
    if field.contains(&format!(" {tok}")) {
        pre
    } else if field.contains(tok) {
        sub
    } else {
        0
    }
}

struct Cand {
    id: &'static str,
    run: Run,
    /// Labels the user may type against: the shown one first. Empty = a UI command outside the menus.
    labels: Vec<String>,
    en: &'static str,
    enabled: Result<(), String>,
    listed: bool,
}

fn state_of(app: &FilmcraftApp, cmd: &str) -> Result<(), String> {
    match filmcraft_engine::find_command(cmd) {
        Some(c) => (c.enabled)(&app.session),
        None => Ok(()),
    }
}

fn label_of(cmd: &str) -> &'static str {
    filmcraft_engine::find_command(cmd).map(|c| c.label).or_else(|| crate::menus::UI_COMMANDS.iter().find(|u| u.id == cmd).map(|u| u.label)).unwrap_or("")
}

fn candidates(app: &FilmcraftApp) -> Vec<Cand> {
    use super::tabs::Kind;
    use super::toolbar::{BUTTONS, EXPORT};
    let mut out: Vec<Cand> = Vec::new();
    // ids of tab items left out because another tab has the same command and params
    let mut twins: Vec<&str> = Vec::new();
    for item in TABS.iter().flat_map(|t| t.items) {
        if out.iter().any(|c| matches!(c.run, Run::Item(i) if i.cmd == item.cmd && i.params == item.params)) {
            twins.push(item.key);
            continue;
        }
        let enabled = match item.kind {
            // it asks the AI chat, which says by itself when there is no key
            Kind::Ai => Ok(()),
            // it only opens its tab's panel, which works whatever the command's state
            Kind::Panel => Ok(()),
            Kind::Run => state_of(app, item.cmd),
        };
        out.push(Cand { id: item.key, run: Run::Item(item), labels: vec![item.label.into()], en: label_of(item.cmd), enabled, listed: true });
    }
    let tools: &'static [(&str, &str, &str, crate::icons::Icon)] = &BUTTONS;
    for b in tools.iter().chain(std::iter::once(&EXPORT)) {
        // a tab item for the same command takes the toolbar's name as one more thing to match
        if let Some(c) = out.iter_mut().find(|c| c.id == b.1) {
            c.labels.push(b.0.into());
        } else {
            out.push(Cand { id: b.1, run: Run::Tool(b.1, b.2), labels: vec![b.0.into()], en: label_of(b.1), enabled: state_of(app, b.1), listed: true });
        }
    }
    let covered: Vec<&str> = out.iter().map(|c| c.id).chain(twins).collect();
    let engine: Vec<_> =
        filmcraft_engine::command_specs().iter().filter(|c| !covered.contains(&c.id)).map(|c| (c.id, c.label, (c.enabled)(&app.session), true)).collect();
    let ui: Vec<_> = crate::menus::UI_COMMANDS.iter().filter(|c| !covered.contains(&c.id)).map(|c| (c.id, c.label, Ok(()), !c.menu.is_empty())).collect();
    for (id, en, enabled, in_menu) in engine.into_iter().chain(ui) {
        let label = if vi::has(en) { vi::vi(en).to_string() } else { en.to_string() };
        out.push(Cand { id, run: Run::Command(id), labels: if in_menu { vec![label] } else { vec![] }, en, enabled, listed: false });
    }
    out
}

pub fn search(app: &FilmcraftApp, query: &str, limit: usize) -> Vec<Hit> {
    let q = words(query);
    let toks: Vec<&str> = q.split_whitespace().collect();
    let phrase = toks.join(" ");
    let mut hits: Vec<Hit> = candidates(app)
        .into_iter()
        .filter_map(|c| {
            let hidden = c.labels.is_empty();
            let labels: Vec<String> = c.labels.iter().map(|l| words(l)).collect();
            let syn: Vec<String> = vi::synonyms(c.en).into_iter().map(words).collect();
            let (en_w, id_w) = (words(c.en), words(c.id));
            if hidden && !toks.iter().all(|t| id_w.contains(t)) {
                return None;
            }
            let mut score = 0;
            for t in &toks {
                let best = [
                    labels.iter().map(|l| hit(t, l, (100, 55))).max().unwrap_or(0),
                    syn.iter().map(|s| hit(t, s, (80, 45))).max().unwrap_or(0),
                    hit(t, &en_w, (50, 25)),
                    hit(t, &id_w, (20, 10)),
                ]
                .into_iter()
                .max()
                .unwrap_or(0);
                if best == 0 {
                    return None;
                }
                score += best;
            }
            if !toks.is_empty() {
                // the phrase against the best of the Vietnamese labels and the synonyms
                let bonus = |f: &String, (exact, prefix, inside): (i32, i32, i32)| {
                    let f = f.trim();
                    if f == phrase {
                        exact
                    } else if f.starts_with(&phrase) {
                        prefix
                    } else if f.contains(&phrase) {
                        inside
                    } else {
                        0
                    }
                };
                score += labels.iter().map(|l| bonus(l, (140, 40, 30))).chain(syn.iter().map(|s| bonus(s, (120, 35, 25)))).max().unwrap_or(0);
            }
            score += if c.enabled.is_ok() { 15 } else { 0 } + if c.listed { 70 } else { 0 };
            score -= labels.first().map_or(0, |l| l.trim().len() as i32 / 4); // shorter = more likely meant
            if !c.listed && !vi::has(c.en) {
                score -= 40; // no Vietnamese name yet
            }
            let label_vi = c.labels.first().cloned().unwrap_or_else(|| c.en.to_string());
            Some(Hit { id: c.id, label_vi, label_en: c.en, enabled: c.enabled, score, run: c.run })
        })
        .collect();
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| fold(&a.label_vi).cmp(&fold(&b.label_vi))));
    hits.truncate(limit);
    hits
}

const ROWS: usize = 8;
const ROW_H: f32 = 46.0;
const WIDTH: f32 = 580.0;

fn run(app: &mut FilmcraftApp, ctx: &egui::Context, h: &Hit) {
    if let Err(why) = &h.enabled {
        // stay open so the user can pick another; say why on the status line too
        app.ui.status = format!("Chưa làm được: {}", vi::reason(why));
        return;
    }
    app.ui.ayp_palette = None;
    match h.run {
        Run::Item(item) => super::library::click_item(app, ctx, item),
        Run::Command(id) => {
            let _ = super::run(app, ctx, id, "{}");
        }
        Run::Tool(id, params) => {
            let _ = super::run(app, ctx, id, params);
        }
    }
}

pub fn show(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let Some(mut query) = app.ui.ayp_palette.clone() else { return };
    let t = app.tokens;
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        app.ui.ayp_palette = None;
        return;
    }
    // Selected row, reset whenever the text changes.
    let sel_id = egui::Id::new("ayp.palette.sel");
    let (last_q, mut sel): (String, usize) = ctx.data(|d| d.get_temp(sel_id)).unwrap_or_default();
    if last_q != query {
        sel = 0;
    }
    let down = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));
    let up = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp));
    let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let hits = search(app, &query, ROWS);
    if down {
        sel += 1;
    }
    if up {
        sel = sel.saturating_sub(1);
    }
    sel = sel.min(hits.len().saturating_sub(1));
    if enter
        && !query.trim().is_empty()
        && let Some(h) = hits.get(sel)
    {
        run(app, ctx, h);
        return;
    }
    let mut clicked = None;
    let area = egui::Area::new(egui::Id::new("ayp.palette")).order(egui::Order::Foreground).anchor(Align2::CENTER_TOP, vec2(0.0, 90.0)).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(t.panel_bg).stroke(egui::Stroke::new(1.0, t.field_border)).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(WIDTH);
            let input = egui::TextEdit::singleline(&mut query)
                .hint_text("Gõ việc muốn làm: tách, phụ đề, xuất, tốc độ… (không dấu cũng được)")
                .font(FontId::proportional(18.0))
                .margin(vec2(10.0, 8.0))
                .desired_width(f32::INFINITY)
                .show(ui);
            input.response.request_focus(); // typing starts at once and the editor's single-key shortcuts stay quiet
            app.auto.add_response("ayp.palette.input", &input.response, "Tìm lệnh…");
            ui.add_space(6.0);
            if hits.is_empty() {
                ui.label(egui::RichText::new("Không thấy lệnh nào. Thử gõ ngắn hơn, ví dụ: tách, phụ đề, xuất.").color(t.text_dim));
            }
            for (i, h) in hits.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, ROW_H), Sense::click());
                if i == sel || resp.hovered() {
                    ui.painter().rect_filled(rect, 6.0, if i == sel { t.row_selected } else { t.hover });
                }
                let ok = h.enabled.is_ok();
                let ink = if ok { t.text } else { t.text_dim };
                let p = ui.painter();
                p.text(rect.left_top() + vec2(10.0, 5.0), Align2::LEFT_TOP, &h.label_vi, FontId::proportional(16.0), ink);
                // second line: English name and id on the left, the reason it can't run now on the right
                let why = h.enabled.as_ref().err().map(|w| p.layout_no_wrap(vi::reason(w), FontId::proportional(12.0), t.text_dim));
                let why_w = why.as_ref().map_or(0.0, |g| g.size().x + 16.0);
                let left = rect.with_max_x(rect.right() - why_w);
                p.with_clip_rect(left).text(
                    rect.left_top() + vec2(10.0, 27.0),
                    Align2::LEFT_TOP,
                    format!("{} · {}", h.label_en, h.id),
                    FontId::proportional(11.0),
                    t.text_faint,
                );
                if let Some(g) = why {
                    p.galley(egui::pos2(rect.right() - g.size().x - 10.0, rect.top() + 26.0), g, t.text_dim);
                }
                app.auto.add_response(&format!("ayp.palette.hit.{}", h.id), &resp, &h.label_vi);
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new("↑↓ chọn · Enter chạy · Esc đóng").size(11.0).color(t.text_faint));
        });
    });
    app.auto.add("ayp.palette", area.response.rect, "Tìm lệnh…");
    ctx.data_mut(|d| d.insert_temp(sel_id, (query.clone(), sel)));
    if let Some(h) = clicked.and_then(|i| hits.get(i)) {
        run(app, ctx, h);
        return;
    }
    // A press outside the box closes it.
    if let Some(pos) = ctx.input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten())
        && !area.response.rect.contains(pos)
    {
        app.ui.ayp_palette = None;
        return;
    }
    app.ui.ayp_palette = Some(query);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fold_strips_vietnamese_marks() {
        assert_eq!(fold("Xoá khoảng lặng Đ"), "xoa khoang lang d");
    }
    #[test]
    fn fold_covers_every_vowel_and_tone() {
        let all = "àáảãạ ăằắẳẵặ âầấẩẫậ èéẻẽẹ êềếểễệ ìíỉĩị òóỏõọ ôồốổỗộ ơờớởỡợ ùúủũụ ưừứửữự ỳýỷỹỵ đ";
        let plain = "aaaaa aaaaaa aaaaaa eeeee eeeeee iiiii ooooo oooooo oooooo uuuuu uuuuuu yyyyy d";
        assert_eq!(fold(all), plain);
        assert_eq!(fold(&all.to_uppercase()), plain);
        // decomposed (NFD) input folds too
        assert_eq!(fold("Xoa\u{0301} kho\u{0309}ang"), "xoa khoang");
    }
    #[test]
    fn finds_captions_with_or_without_marks() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        for q in ["phụ đề tiếng việt", "phu de tieng viet"] {
            assert_eq!(search(&app, q, 5)[0].id, "transcript.createCaptionsVi");
        }
    }
    #[test]
    fn finds_pauses_export_and_speed() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        assert_eq!(search(&app, "xoa khoang lang", 5)[0].id, "transcript.removePauses");
        assert!(search(&app, "xuat", 5).iter().any(|h| h.id == "export.quick"));
        assert!(search(&app, "toc do", 5).iter().any(|h| h.id == "clip.speedDuration"));
    }
    #[test]
    fn every_command_is_searchable() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        assert!(search(&app, "", usize::MAX).len() >= 700);
    }
    #[test]
    fn common_queries_find_the_ayp_surface() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        let ids = |q: &str| search(&app, q, 6).into_iter().map(|h| h.id).collect::<Vec<_>>();
        for (q, want, within) in [
            ("phu de", "transcript.createCaptionsVi", 1),
            ("cat", "sequence.addEdit", 1),
            ("xoa", "edit.rippleDelete", 2),
            ("xuat", "file.exportMedia", 1),
            ("lam mo", "effects.apply.gaussian_blur", 1),
            ("giong doc", "tts.create", 2),
            ("giam on", "effects.apply.denoise", 1),
            ("chong rung", "effects.apply.warp_stabilizer", 1),
            ("xoa phong xanh", "effects.apply.ultra_key", 1),
            ("lam min da", "effects.apply.skin_smooth", 1),
        ] {
            let got = ids(q);
            assert!(got.iter().take(within).any(|i| *i == want), "{q}: {got:?}");
        }
    }
    #[test]
    fn a_command_is_not_listed_twice() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        let all = search(&app, "", usize::MAX);
        let mut ids: Vec<_> = all.iter().map(|h| h.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
    #[test]
    fn transcribing_from_the_finder_starts_a_job() {
        let (mut app, ctx) = super::super::library::tests::demo_transcribing();
        let before = app.session.history.undo.len();
        app.ui.ayp_palette = Some("chep loi".into());
        let hits = search(&app, "chep loi", 5);
        let h = hits.iter().find(|h| matches!(h.run, Run::Item(i) if i.cmd == "transcript.generate")).expect("Chép lời is found");
        run(&mut app, &ctx, h);
        assert!(app.ui.ayp_palette.is_none());
        assert!(app.ui.status.starts_with("Đang chép lời"), "{}", app.ui.status);
        assert_eq!(app.session.history.undo.len(), before, "not applied in the click frame");
        assert!(app.ayp_transcribing.is_some());
    }
    #[test]
    fn the_same_command_and_params_are_listed_once() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        let n = search(&app, "chep loi", 10).iter().filter(|h| h.label_vi == "Chép lời (tiếng Việt)").count();
        assert_eq!(n, 1);
    }
    #[test]
    fn ai_hits_can_be_run() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        let h = search(&app, "chia cum phu de", 3).into_iter().find(|h| h.id == "transcript.captionsFromChunks").expect("listed");
        assert_eq!(h.enabled, Ok(()));
    }
    #[test]
    fn unrelated_words_find_nothing() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        assert!(search(&app, "zzqx", 5).is_empty());
    }
}
