//! AYP: Vietnamese caption layout and ASR clean-up. Expected values come from the Swift port
//! (`ayp-cut` CaptionBuilder / CaptionPolish tests) and, for polish, from the Python engine of
//! CapCut Sub Fixer (`tests/fixtures/polish-golden.json`).

use super::*;
use filmcraft_project::{ClipId, ItemId};
use serde_json::Value;

const R: FrameRate = FrameRate { num: 30, den: 1 };

fn s(x: f64) -> Tick {
    Tick((x * TICKS_PER_SECOND as f64).round() as i64)
}

/// Words 0.3 s apart, each 0.25 s long, from `from` seconds.
fn words(text: &str, from: f64) -> Vec<SeqWord> {
    text.split(' ')
        .enumerate()
        .map(|(i, w)| SeqWord {
            text: w.to_string(),
            start: s(from + i as f64 * 0.3),
            end: s(from + i as f64 * 0.3 + 0.25),
            clip: ClipId(1),
            item: ItemId(1),
            index: i,
            track: 0,
            speaker: None,
            confidence: 1.0,
        })
        .collect()
}

fn rules(max_words: usize, compounds: &[&str]) -> ViRules {
    let mut r = ViRules::standard();
    r.max_words = max_words;
    r.polish = false;
    r.line_chars = 0; // these tests are about grouping words into captions
    r.highlight = None;
    r.compounds = compounds.iter().map(|c| c.to_string()).collect();
    r
}

fn texts(words: &[SeqWord], r: &ViRules) -> Vec<String> {
    vi_caption_blocks(words, r, &CaptionRules::default(), R).into_iter().map(|b| b.text).collect()
}

#[test]
fn compound_list_loads() {
    assert!(ViRules::standard().compounds.len() > 400);
}

#[test]
fn splits_by_max_words_without_breaking_compounds() {
    // balanced, and "về" stays with what it introduces
    assert_eq!(texts(&words("tôi muốn nói về thất nghiệp hôm nay", 0.0), &rules(5, &["thất nghiệp"])), ["Tôi muốn nói", "về thất nghiệp hôm nay"]);
}

#[test]
fn long_pause_starts_new_capitalized_caption() {
    let mut w = words("xin chào", 0.0);
    w.extend(words("hôm nay trời đẹp", 3.0));
    let b = vi_caption_blocks(&w, &rules(8, &[]), &CaptionRules::default(), R);
    assert_eq!(b.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["Xin chào", "Hôm nay trời đẹp"]);
    assert!(b[0].end <= b[1].start);
    assert_eq!(b[1].start, s(3.0));
}

#[test]
fn fillers_are_removed_but_final_particle_kept() {
    assert_eq!(texts(&words("ừm mình à nghĩ là vậy à", 0.0), &rules(8, &[])), ["Mình nghĩ là vậy à"]);
}

#[test]
fn sentence_end_breaks_caption() {
    assert_eq!(texts(&words("chào bạn. hôm nay mình kể chuyện", 0.0), &rules(8, &[])), ["Chào bạn.", "Hôm nay mình kể chuyện"]);
}

#[test]
fn sentence_ending_word_is_not_orphaned() {
    assert_eq!(texts(&words("mà vẫn chưa được.", 0.0), &rules(3, &[])), ["Mà vẫn chưa được."]);
}

#[test]
fn hallucinations_are_dropped_but_real_outro_kept() {
    let outro = words("cảm ơn các bạn đã xem video hãy subscribe cho kênh nhé", 0.0);
    assert_eq!(remove_hallucinations(&outro).len(), outro.len());
    let mut junk = words("chào bạn", 0.0);
    junk.extend(words("ghiền mì gõ để không bỏ lỡ", 5.0));
    junk.extend(words("tiếp tục", 9.0));
    let kept: Vec<String> = remove_hallucinations(&junk).iter().map(|w| w.text.clone()).collect();
    assert_eq!(kept, ["chào", "bạn", "tiếp", "tục"]);
}

#[test]
fn polish_matches_capcut_sub_fixer() {
    let g: Value = serde_json::from_str(include_str!("../../tests/fixtures/polish-golden.json")).unwrap();
    let r = ViRules::standard();
    for c in g["texts"].as_array().unwrap() {
        let text = c["text"].as_str().unwrap();
        let got = polish_text(text, c["prev"].as_str(), &HashSet::new(), c["newSentence"].as_bool().unwrap(), &r);
        assert_eq!(got, c["expected"].as_str().unwrap(), "{text}");
    }
}

#[test]
fn polish_captions_use_pauses_for_sentences() {
    let g: Value = serde_json::from_str(include_str!("../../tests/fixtures/polish-golden.json")).unwrap();
    let cues: Vec<(Tick, Tick, String)> = g["cues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (s(c["start"].as_f64().unwrap() / 1e6), s(c["end"].as_f64().unwrap() / 1e6), c["text"].as_str().unwrap().to_string()))
        .collect();
    let expected: Vec<&str> = g["cues"].as_array().unwrap().iter().map(|c| c["expected"].as_str().unwrap()).collect();
    assert_eq!(polish_all(&cues, &ViRules::standard()), expected);
}

#[test]
fn hostile_words_never_panic() {
    let mut w = words("😀 x 123 4567 . , ... “” Việt Nam", 0.0);
    w.push(SeqWord { text: String::new(), ..w[0].clone() });
    let r = ViRules::standard();
    let _ = vi_caption_blocks(&w, &r, &CaptionRules::default(), R);
    let many: Vec<SeqWord> = (0..10_000).map(|i| SeqWord { start: s(i as f64 * 0.3), end: s(i as f64 * 0.3 + 0.2), index: i, ..w[1].clone() }).collect();
    assert!(!vi_caption_blocks(&many, &r, &CaptionRules::default(), R).is_empty());
    assert!(vi_caption_blocks(&[], &r, &CaptionRules::default(), R).is_empty());
}

#[test]
fn user_files_tune_rules_and_bad_files_are_ignored() {
    let dir = std::env::temp_dir().join(format!("ayp-capcut-sub-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("tu-thuong.txt"), "-người   # giữ hoa\n").unwrap();
    std::fs::write(dir.join("tu-dem.txt"), "thì\n").unwrap();
    std::fs::write(dir.join("config.txt"), "sentence_gap_seconds = 2\n").unwrap();
    std::fs::write(dir.join("tu-ghep.txt"), [0xffu8, 0xfe, 0x00]).unwrap(); // not UTF-8 → ignored
    let r = ViRules::load_user(&dir);
    assert_eq!(polish_text("rồi Người ta nói", Some("x"), &HashSet::new(), false, &r), "rồi Người ta nói");
    assert_eq!(polish_text("thì là vậy", Some("x"), &HashSet::new(), false, &r), "là vậy");
    assert_eq!(r.sentence_gap, s(2.0));
    assert_eq!(ViRules::load_user(Path::new("/khong/co")).sentence_gap, ViRules::standard().sentence_gap);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AYP: after cuts the words of two clips sit back to back on the timeline; a caption never
/// spans a cut ("…hết tháng" | "sau đó…" were joined into one line).
#[test]
fn captions_break_at_clip_boundaries() {
    let mut w = words("để sống hết tháng", 0.0);
    let mut next = words("sau đó mình bán tài sản", 1.2);
    for x in &mut next {
        x.clip = ClipId(2);
    }
    w.extend(next);
    assert_eq!(texts(&w, &rules(8, &[])), ["Để sống hết tháng", "sau đó mình bán tài sản"]);
}

/// AYP: cutting out a stutter ("trả trả nợ") makes a new clip mid-sentence; that is not a real
/// cut — the caption continues when only a few words of the same media were skipped.
#[test]
fn captions_flow_over_stutter_cuts() {
    let mut w = words("để mình trả", 0.0);
    let mut next = words("nợ xong sớm", 0.9);
    for (i, x) in next.iter_mut().enumerate() {
        x.clip = ClipId(2);
        x.index = 4 + i; // one word ("trả") skipped in the media transcript
    }
    w.extend(next);
    assert_eq!(texts(&w, &rules(8, &[])), ["Để mình trả nợ xong sớm"]);
}

/// AYP: two balanced lines broken where they read well (never after "của/và/là…", never
/// inside a compound), and important words coloured.
#[test]
fn two_lines_break_where_they_read_well() {
    let r = ViRules::standard();
    let t = break_lines("Từ cái người mà hồi xưa mình làm freelance", &r);
    let (a, b) = t.split_once('\n').unwrap();
    assert!(!a.ends_with(" mà") && !a.ends_with(" của") && !a.ends_with(" là"), "{t}");
    assert!((a.chars().count() as i64 - b.chars().count() as i64).abs() <= 14, "{t}");
    // never inside a compound
    let mut r2 = ViRules::standard();
    r2.compounds.insert("thất nghiệp".into());
    let t2 = break_lines("tỷ lệ người bị thất nghiệp tăng cao", &r2);
    assert!(!t2.contains("thất\nnghiệp"), "{t2}");
    // short text stays on one line
    assert_eq!(break_lines("Xin chào bạn", &r), "Xin chào bạn");
}

#[test]
fn numbers_and_keywords_are_coloured() {
    let mut r = ViRules::standard();
    r.keywords = vec!["lãi".into(), "im".into(), "tiết kiệm".into()];
    assert_eq!(highlight("trả 4 triệu thì 2 triệu là lãi", &r), "trả <c #ffd200>4 triệu</c> thì <c #ffd200>2 triệu</c> là <c #ffd200>lãi</c>");
    assert_eq!(highlight("hên là có IM, tiết kiệm được 50%", &r), "hên là có <c #ffd200>IM</c>, <c #ffd200>tiết kiệm</c> được <c #ffd200>50%</c>");
    // not inside another word ("lãi" in "lãinh" is not a keyword hit)
    assert_eq!(highlight("lãinh đạo", &r), "lãinh đạo");
    r.highlight = None;
    assert_eq!(highlight("trả 4 triệu", &r), "trả 4 triệu");
}

/// AYP: a long phrase is split into balanced captions — no one-word leftovers, a number stays
/// with its unit, and a pause is the preferred place to break.
#[test]
fn long_phrases_split_evenly_without_orphans() {
    let r = rules(8, &[]);
    let t = texts(&words("ngoài ra còn có máy phim thì máy phim là 8-10 triệu rồi mua phim mua đồ tráng", 0.0), &r);
    assert!(t.iter().all(|c| c.split(' ').count() >= 4), "{t:?}");
    assert!(t.iter().any(|c| c.contains("8-10 triệu")), "{t:?}");
    // 9 words with max 8: one caption, not 8 + 1
    assert_eq!(texts(&words("mình đã trả gần hết nợ cho bên đó", 0.0), &r).len(), 1);
    // a short pause inside the phrase is where it breaks
    let mut w = words("mình phát hiện ra bốn triệu tiền trả", 0.0);
    w.extend(words("thì có hai triệu là tiền lãi", 2.65));
    let t = texts(&w, &r);
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[0].ends_with("tiền trả"), "{t:?}");
}

#[test]
fn line_breaks_keep_numbers_with_units() {
    let r = ViRules::standard();
    let t = break_lines("máy phim là 8-10 triệu rồi mua phim,", &r);
    assert!(!t.contains("8-10\ntriệu"), "{t}");
}

#[test]
fn keywords_and_number_words_are_never_split() {
    let mut r = ViRules::standard();
    r.keywords = vec!["đám cưới".into(), "con dao hai lưỡi".into()];
    let t = break_lines("và cũng có kế hoạch cho một cái đám cưới", &r);
    assert!(!t.contains("đám\ncưới"), "{t}");
    let t = break_lines("thẻ tín dụng là cái như con dao hai lưỡi", &r);
    assert!(!t.contains("dao\nhai") && !t.contains("con\ndao") && !t.contains("hai\nlưỡi"), "{t}");
    let t = break_lines("mình bắt đầu có vài triệu để sống", &r);
    assert!(!t.contains("vài\ntriệu"), "{t}");
}

/// AYP: lines must fit the frame (about 21 characters at size 48 on a vertical frame).
#[test]
fn captions_fit_the_line_width() {
    let mut r = rules(8, &[]);
    r.line_chars = ViRules::standard().line_chars;
    let t = texts(&words("Từ cái người mà hồi xưa mình làm freelance", 0.0), &r);
    for c in &t {
        assert!(c.lines().all(|l| l.chars().count() <= r.max_line_chars), "{t:?}");
    }
}

/// AYP: captions keep when each drawn word is heard (relative to the caption start), through
/// filler removal, line breaks and keyword colouring.
#[test]
fn captions_keep_word_timings() {
    let w = words("ừ mình nợ 10 triệu rồi", 1.0);
    let mut r = rules(8, &[]);
    r.highlight = Some("#ffd200".into());
    let blocks = vi_caption_blocks(&w, &r, &CaptionRules::default(), R);
    let mut next = 1;
    let none = |_| None;
    let mut ctx = crate::EditCtx { next_id: &mut next, media_duration: &none, media_start: &|_| Tick::ZERO, min_duration: R.frame_duration() };
    let caps = crate::transcript::blocks_to_captions(&blocks, &w, &mut ctx);
    assert_eq!(caps.len(), 1);
    let c = &caps[0];
    assert!(c.text.contains("<c #ffd200>"), "{}", c.text);
    let plain = filmcraft_project::plain_text(&c.text);
    let got: Vec<(&str, Tick)> = c.words.iter().map(|x| (&plain[x.range.clone()], c.start + x.start)).collect();
    assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), ["Mình", "nợ", "10", "triệu", "rồi"]);
    assert_eq!(got[3].1, w[4].start);
    assert!(c.words.iter().all(|x| x.start >= Tick::ZERO && x.end <= c.duration));
}

// ---- AYP: phụ đề "chuẩn Apple": mỗi dòng có nghĩa, cân, không chữ trơ ----

/// "thì nếu mà mình / đợi": a caption that fits one line stays one line, and a line never
/// holds a single word.
#[test]
fn no_lonely_word_and_no_needless_break() {
    let r = ViRules::standard();
    assert_eq!(break_lines("thì nếu mà mình đợi", &r), "thì nếu mà mình đợi");
    for t in ["Các bạn có nhớ cái anh có nói", "khi nào sẽ layoff tiếp anh ơi vậy", "anh không dám trả lời vì anh không biết"] {
        let b = break_lines(t, &r);
        assert!(b.lines().all(|l| l.split(' ').count() >= 2), "{b}");
        assert!(b.lines().all(|l| l.chars().count() <= r.max_line_chars), "{b}");
    }
}

/// Lines break where a clause starts ("nếu", "thì", "mà", "vì", "để", "nhưng"…).
#[test]
fn lines_break_before_a_clause() {
    let r = ViRules::standard();
    assert_eq!(break_lines("phải trả lời sao nếu mình không biết", &r), "phải trả lời sao\nnếu mình không biết");
}

/// 6–10 words a caption: short pieces of one sentence are joined, long ones split evenly.
#[test]
fn captions_hold_six_to_ten_words() {
    let mut r = ViRules::standard();
    r.polish = false;
    r.highlight = None;
    r.line_chars = 0;
    r.max_line_chars = 0;
    // two short pieces of one sentence with a 0.8 s breath between them
    let mut w = words("thì nếu mà mình đợi", 0.0);
    w.extend(words("những cái thói quen cũ", 2.3));
    assert_eq!(texts(&w, &r), ["Thì nếu mà mình đợi những cái thói quen cũ"]);
    // a long phrase: every caption 6..=10 words
    let long = words("cho nên là trả lời một câu là có là không thì anh không dám trả lời làm sao mà dám chắc được là sẽ không sao", 0.0);
    let t = texts(&long, &r);
    assert!(t.iter().all(|c| (6..=10).contains(&c.split(' ').count())), "{t:?}");
}

/// "Khi nào sẽ lay / off tiếp anh?": an English phrase is one unit.
#[test]
fn english_phrases_stay_whole() {
    let mut r = ViRules::standard();
    r.polish = false;
    r.highlight = None;
    r.max_words = 4;
    let t = texts(&words("khi nào sẽ lay off tiếp anh nhỉ", 0.0), &r);
    assert!(t.iter().all(|c| !c.ends_with("lay") && !c.contains("lay\n")), "{t:?}");
}

/// ASR fixes are applied before lines are broken, so a fix never overflows a line.
#[test]
fn replacements_come_before_line_breaks() {
    let mut r = ViRules::standard();
    r.polish = false;
    r.highlight = None;
    r.replacements = vec![("lay up".into(), "layoff".into()), ("hiệu sức công".into(), "hiệu suất công việc".into())];
    let t = texts(&words("để mình tăng hiệu sức công của mình", 0.0), &r);
    assert!(t.iter().all(|c| c.lines().all(|l| l.chars().count() <= r.max_line_chars && l.split(' ').count() >= 2)), "{t:?}");
    assert!(t.join(" ").replace('\n', " ").contains("hiệu suất công việc"), "{t:?}");
    let t = texts(&words("còn có lay up nữa không", 0.0), &r);
    assert!(t.concat().contains("layoff"), "{t:?}");
}

/// The Wiktionary list knows everyday compounds ("tồn tại", "làm việc", "tính năng").
#[test]
fn everyday_compounds_are_known() {
    let r = ViRules::standard();
    for c in ["tồn tại", "làm việc", "tính năng", "hiệu suất", "thị trường"] {
        assert!(r.compounds.contains(c), "{c}");
    }
    // "hoàn thành công việc" = hoàn thành | công việc: the overlap "thành công" may break
    let t = break_lines("để mình hoàn thành công việc thì anh nghĩ", &r);
    assert!(!t.contains("hoàn\nthành") && !t.contains("công\nviệc"), "{t}");
}

/// A one- or two-word sentence ("Why?", "Đúng không?") joins the caption before it.
#[test]
fn tiny_sentences_join_the_previous_caption() {
    let mut r = ViRules::standard();
    r.polish = false;
    r.highlight = None;
    let mut w = words("mình làm cái này để làm cái gì.", 0.0);
    w.extend(words("Why?", 2.4));
    assert_eq!(texts(&w, &r).len(), 1, "{:?}", texts(&w, &r));
}

/// Captions are cut so that each one breaks into lines cleanly (no wide line, no lonely word).
#[test]
fn captions_are_cut_where_their_lines_read_well() {
    let mut r = ViRules::standard();
    r.polish = false;
    r.keywords = vec!["không chịu thay đổi".into()];
    let t = texts(&words("hai là cái nhóm làm việc bình thường nhưng không chịu thay đổi thì nhóm làm việc bình thường không chịu thay đổi", 0.0), &r);
    for c in &t {
        let plain: String = c.replace("<c #ffd200>", "").replace("</c>", "");
        assert!(plain.lines().all(|l| l.chars().count() <= r.max_line_chars && l.split(' ').count() >= 2), "{t:?}");
    }
}
