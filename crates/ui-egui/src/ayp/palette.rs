//! "Tìm lệnh…": one box that finds and runs every command, with or without Vietnamese accents.
use super::{tabs::TABS, vi};
use crate::FilmcraftApp;
use egui::{Align2, FontId, Key, Modifiers, Sense, vec2};

pub struct Hit {
    pub id: &'static str,
    pub label_vi: String,
    pub label_en: &'static str,
    pub enabled: Result<(), String>,
    pub score: i32,
}

/// Lower-case and strip Vietnamese marks: "Xoá khoảng lặng Đ" -> "xoa khoang lang d".
/// Hand-written table (no crate); decomposed input (base letter + combining mark) folds too.
pub fn fold(s: &str) -> String {
    const GROUPS: [(&str, char); 7] = [
        ("àáảãạăằắẳẵặâầấẩẫậ", 'a'),
        ("èéẻẽẹêềếểễệ", 'e'),
        ("ìíỉĩị", 'i'),
        ("òóỏõọôồốổỗộơờớởỡợ", 'o'),
        ("ùúủũụưừứửữự", 'u'),
        ("ỳýỷỹỵ", 'y'),
        ("đ", 'd'),
    ];
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

pub fn search(app: &FilmcraftApp, query: &str, limit: usize) -> Vec<Hit> {
    let q = words(query);
    let toks: Vec<&str> = q.split_whitespace().collect();
    let in_tab = |id: &str| TABS.iter().any(|t| t.items.iter().any(|i| i.cmd == id));
    let engine = filmcraft_engine::command_specs().iter().map(|c| (c.id, c.label, (c.enabled)(&app.session)));
    let ui = crate::menus::UI_COMMANDS.iter().map(|c| (c.id, c.label, Ok(())));
    let mut hits: Vec<Hit> = engine
        .chain(ui)
        .filter_map(|(id, en, enabled)| {
            let label_vi = vi::vi(en).to_string();
            let vi_w = words(&label_vi);
            let syn: Vec<String> = vi::synonyms(en).into_iter().map(words).collect();
            let (en_w, id_w) = (words(en), words(id));
            let mut score = 0;
            for t in &toks {
                let best = [
                    hit(t, &vi_w, (100, 55)),
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
            let label = vi_w.trim();
            let phrase = toks.join(" ");
            if !toks.is_empty() {
                if label == phrase {
                    score += 140;
                } else if label.starts_with(&phrase) {
                    score += 40;
                } else if label.contains(&phrase) {
                    score += 30;
                }
            }
            score += if enabled.is_ok() { 15 } else { 0 } + if in_tab(id) { 70 } else { 0 };
            score -= (label.len() as i32) / 4; // shorter label = more likely what was meant
            Some(Hit { id, label_vi, label_en: en, enabled, score })
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
    if h.enabled.is_err() {
        return; // the row already shows why; stay open so the user can pick another
    }
    app.ui.ayp_palette = None;
    if let Err(e) = crate::menus::invoke(app, ctx, h.id, serde_json::json!({})) {
        app.ui.status = format!("Chưa làm được: {}", vi::reason(&e));
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
    if enter && let Some(h) = hits.get(sel) {
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
                p.with_clip_rect(left).text(rect.left_top() + vec2(10.0, 27.0), Align2::LEFT_TOP, format!("{} · {}", h.label_en, h.id), FontId::proportional(11.0), t.text_faint);
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
    fn unrelated_words_find_nothing() {
        let app = FilmcraftApp::new(filmcraft_engine::Session::default());
        assert!(search(&app, "zzqx", 5).is_empty());
    }
}
