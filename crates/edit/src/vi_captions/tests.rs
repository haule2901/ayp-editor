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
    assert_eq!(texts(&words("tôi muốn nói về thất nghiệp hôm nay", 0.0), &rules(5, &["thất nghiệp"])), ["Tôi muốn nói về", "thất nghiệp hôm nay"]);
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
