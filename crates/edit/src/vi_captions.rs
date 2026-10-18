//! AYP: Vietnamese captions from the sequence transcript.
//!
//! Ported from AYP Cut (`CaptionBuilder`, `CaptionPolish`), whose polish is in turn a port of
//! CapCut Sub Fixer's `polish`:
//!
//! - lines of at most `max_words` words that never split a two-syllable compound
//!   (`vi_compounds.txt`), never leave a sentence-ending word alone on the next line, and break at
//!   pauses (`max_gap`) and at sentence ends;
//! - filler words (`ừm`, and `à`/`ờ`… unless they end the phrase) and Whisper's usual
//!   hallucinations are dropped; the first word of a sentence is capitalised;
//! - ASR clean-up: digit runs mixed with number words merged (`một chín 38` → `1938`),
//!   Sino-Vietnamese numbers restored (`7 nghiệp` → `thất nghiệp`), stray capitals inside a
//!   sentence lowered (proper nouns kept, including the second syllable of `Hà Nội`), capitals
//!   after a pause or sentence end.
//!
//! The word lists can be tuned per user with CapCut Sub Fixer's files in `~/.capcut-sub/`
//! (`tu-ghep.txt`, `tu-dem.txt`, `tu-thuong.txt`: one word per line, `-word` removes it;
//! `config.txt`: `sentence_gap_seconds = 0.6`, and AYP's `min_words = 6`, `max_words = 10`,
//! `max_line_chars = 21`). Unreadable files and out-of-range values are ignored.

use std::collections::HashSet;
use std::path::Path;

use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick};

use crate::transcript::{CaptionBlock, CaptionRules, SeqWord, place_blocks, snap_span};

const COMPOUNDS: &str = include_str!("vi_compounds.txt");
/// AYP: two-syllable words from Wiktionary (CC BY-SA 4.0, see the file's header). Only used to
/// keep compounds whole — not to decide which capitals are stray (names would be lowered).
const COMPOUNDS_WIKTIONARY: &str = include_str!("vi_compounds_wiktionary.txt");

/// Prepositions, conjunctions, classifiers… (CapCut Sub Fixer's NO_END_WORDS): ordinary words.
const NO_END_WORDS: &[&str] = &[
    "của", "và", "là", "cho", "với", "trong", "ngoài", "trên", "dưới", "khi", "mà", "nhưng", "thì", "để", "từ", "về", "theo", "như", "vì", "nếu", "hay",
    "hoặc", "các", "những", "một", "cái", "con", "chiếc", "bị", "đã", "đang", "sẽ", "vẫn", "rất", "hơi", "khá", "ở", "tại", "bằng", "sau", "trước", "giữa",
    "cùng", "do", "bởi", "nên", "mới", "chỉ", "không", "chưa", "mỗi", "vào", "sang", "qua",
];
/// Common words a stray capital may be lowered on.
const POLISH_COMMON: &[&str] = &[
    "ra", "ai", "đi", "đến", "làm", "có", "người", "em", "anh", "chị", "mình", "tôi", "bạn", "nó", "họ", "gì", "đó", "này", "thôi", "nha", "nhé", "ạ", "à",
    "ơi", "ừ", "dạ", "vâng", "rồi", "xong", "luôn", "lại", "nữa", "cũng", "đều", "được", "cả", "thêm", "bớt", "ít", "nhiều", "lớn", "nhỏ", "cao", "thấp", "xa",
    "gần", "vô", "dô", "lên", "xuống", "ha", "hen", "nghen", "đúng", "sai", "hông", "hong", "chớ", "chứ", "vậy", "vầy", "kiểu", "thiệt", "thật", "quá", "lắm",
    "ghê", "dữ", "trời", "đất", "ông", "bà", "cô", "chú", "bác", "cậu", "dì", "cháu", "chuyện", "việc", "cách", "lúc", "hồi", "khúc", "đoạn", "phần", "cây",
    "quyển", "cuốn", "tờ", "tấm", "bức", "cần", "muốn", "thích", "biết", "hiểu", "nói", "nghe", "thấy", "nhìn", "coi", "xem", "ăn", "uống", "ngủ", "sao",
    "tại", "check", "ok", "oke", "okay",
];
const FILLER_ALWAYS: &[&str] = &["ừm", "ùm", "ờm", "umm", "ưm", "hmm", "hm", "ừmm"];
const FILLER_NON_FINAL: &[&str] = &["à", "ờ", "ơ", "ừ", "ừa"];
/// Phrases Whisper makes up over silence. Real outros ("cảm ơn các bạn đã xem") are not here.
const HALLUCINATIONS: &[&str] = &["ghiền mì gõ", "để không bỏ lỡ những video hấp dẫn"];
/// Number words merged with digits ("một chín 38" → 1938); not "năm"/"mười"/"tư"/"lăm" (ambiguous).
const WORD_DIGIT: &[(&str, &str)] =
    &[("không", "0"), ("một", "1"), ("hai", "2"), ("ba", "3"), ("bốn", "4"), ("sáu", "6"), ("bảy", "7"), ("tám", "8"), ("chín", "9")];
const SINO_NUMBERS: &[&str] = &["nhất", "nhị", "tam", "tứ", "ngũ", "lục", "thất", "bát", "cửu", "thập"];
const OPEN: &[char] = &['"', '\'', '“', '”', '‘', '’', '(', '['];
const CLOSE: &[char] = &['"', '\'', '“', '”', '‘', '’', ')', ']', ',', '.', '!', '?', '…', ':', ';'];
const SENTENCE_END: &[char] = &['.', '!', '?', '…'];

/// Layout and clean-up rules.
#[derive(Clone, Debug)]
pub struct ViRules {
    /// Words per caption line.
    pub max_words: usize,
    /// A pause at least this long starts a new caption (and sentence).
    pub max_gap: Tick,
    pub remove_fillers: bool,
    /// Run the ASR clean-up on the caption text.
    pub polish: bool,
    /// For the clean-up: a pause at least this long between captions starts a sentence.
    pub sentence_gap: Tick,
    /// Two-syllable compounds, lower case ("thất nghiệp").
    pub compounds: HashSet<String>,
    /// Words a stray capital may be lowered on.
    pub common: HashSet<String>,
    pub filler_always: HashSet<String>,
    /// Dropped unless they end the phrase ("vậy à" is a real question).
    pub filler_non_final: HashSet<String>,
    /// Break a caption longer than this many characters into two balanced lines (0 = never).
    pub line_chars: usize,
    /// Most characters a line may have (what fits the frame width: about 21 at size 48 on a
    /// vertical frame); longer phrases get more captions.
    pub max_line_chars: usize,
    /// Colour (`#rrggbb`) of numbers with their unit and of [`ViRules::keywords`]; None = none.
    pub highlight: Option<String>,
    /// Words or phrases to colour (lower case, matched whole).
    pub keywords: Vec<String>,
    /// Fewest words a caption should have: shorter pieces of one sentence are joined.
    pub min_words: usize,
    /// ASR fixes `(heard, meant)`, whole words, case-insensitive, applied before lines are broken.
    pub replacements: Vec<(String, String)>,
}

fn set(words: &[&str]) -> HashSet<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

fn add_syllables(common: &mut HashSet<String>, compounds: &HashSet<String>) {
    for c in compounds {
        common.extend(c.split(' ').filter(|w| !w.is_empty()).map(str::to_string));
    }
}

impl ViRules {
    pub fn standard() -> Self {
        let list = |t: &'static str| t.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty() && !l.starts_with('#'));
        let mut compounds: HashSet<String> = list(COMPOUNDS).collect();
        let mut common = set(NO_END_WORDS);
        common.extend(set(POLISH_COMMON));
        add_syllables(&mut common, &compounds);
        compounds.extend(list(COMPOUNDS_WIKTIONARY));
        let gap = Tick(TICKS_PER_SECOND / 10 * 6);
        Self {
            max_words: 10,
            max_gap: gap,
            remove_fillers: true,
            polish: true,
            sentence_gap: gap,
            compounds,
            common,
            filler_always: set(FILLER_ALWAYS),
            filler_non_final: set(FILLER_NON_FINAL),
            line_chars: 21,
            max_line_chars: 21,
            highlight: Some("#ffd200".into()),
            keywords: Vec::new(),
            min_words: 6,
            replacements: Vec::new(),
        }
    }

    /// [`ViRules::standard`] tuned by the CapCut Sub Fixer files in `dir` (see the module docs).
    /// Missing or unreadable files change nothing. Removals are applied last, so `-word` in
    /// `tu-thuong.txt` always protects a capital.
    pub fn load_user(dir: &Path) -> Self {
        let mut r = Self::standard();
        let words = |name: &str| -> (HashSet<String>, HashSet<String>) {
            let mut add = HashSet::new();
            let mut remove = HashSet::new();
            let Ok(text) = std::fs::read_to_string(dir.join(name)) else { return (add, remove) };
            for line in text.lines().take(100_000) {
                let w = line.split('#').next().unwrap_or("").trim().to_lowercase();
                if let Some(rest) = w.strip_prefix('-') {
                    remove.insert(rest.trim().to_string());
                } else if !w.is_empty() {
                    add.insert(w);
                }
            }
            (add, remove)
        };
        let (add, remove) = words("tu-ghep.txt");
        add_syllables(&mut r.common, &add);
        r.compounds.extend(add);
        r.compounds.retain(|c| !remove.contains(c));
        let (add, remove) = words("tu-dem.txt");
        r.filler_non_final.extend(add);
        r.filler_non_final.retain(|c| !remove.contains(c));
        r.filler_always.retain(|c| !remove.contains(c));
        let (add, remove) = words("tu-thuong.txt");
        r.common.extend(add);
        r.common.retain(|c| !remove.contains(c));
        if let Ok(cfg) = std::fs::read_to_string(dir.join("config.txt")) {
            for line in cfg.lines() {
                let line = line.split('#').next().unwrap_or("");
                let Some((k, v)) = line.split_once('=') else { continue };
                let (k, v) = (k.trim(), v.trim());
                if k == "sentence_gap_seconds"
                    && let Ok(v) = v.parse::<f64>()
                    && v.is_finite()
                    && (0.0..=60.0).contains(&v)
                {
                    r.sentence_gap = Tick::from_seconds_f64(v);
                }
                // AYP: the caption rules' numbers (see CLAUDE.md ▸ AYP caption rules)
                let n = v.parse::<usize>().ok();
                match (k, n) {
                    ("min_words", Some(n)) if (1..=30).contains(&n) => r.min_words = n,
                    ("max_words", Some(n)) if (1..=30).contains(&n) => r.max_words = n,
                    ("max_line_chars", Some(n)) if (8..=80).contains(&n) => {
                        r.max_line_chars = n;
                        r.line_chars = n;
                    }
                    _ => {}
                }
            }
        }
        r
    }
}

fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation() || "…“”‘’«»–—".contains(c)
}

/// A word without surrounding punctuation, lower case.
fn core(word: &str) -> String {
    word.trim_matches(|c: char| c.is_whitespace() || is_punct(c)).to_lowercase()
}

fn ends_sentence(word: &str) -> bool {
    word.chars().last().is_some_and(|c| ".?!…".contains(c))
}

fn hallucination_mask(words: &[SeqWord]) -> Vec<bool> {
    let mut keep = vec![true; words.len()];
    let cores: Vec<String> = words.iter().map(|w| core(&w.text)).collect();
    let pause = Tick(TICKS_PER_SECOND / 10 * 4);
    for phrase in HALLUCINATIONS {
        let parts: Vec<&str> = phrase.split(' ').collect();
        if parts.is_empty() || parts.len() > cores.len() {
            continue;
        }
        for i in 0..=cores.len() - parts.len() {
            let hit = cores.get(i..i + parts.len()).is_some_and(|c| c.iter().zip(&parts).all(|(a, b)| a == b));
            if !hit {
                continue;
            }
            for k in keep.iter_mut().skip(i).take(parts.len()) {
                *k = false;
            }
            // The made-up phrase usually drags a tail along ("để không bỏ lỡ…"): drop the words
            // spoken right after it.
            let mut j = i + parts.len() - 1;
            while let (Some(a), Some(b)) = (words.get(j), words.get(j + 1)) {
                if b.start - a.end >= pause {
                    break;
                }
                j += 1;
                if let Some(k) = keep.get_mut(j) {
                    *k = false;
                }
            }
        }
    }
    keep
}

/// Words without Whisper's usual made-up phrases.
pub fn remove_hallucinations(words: &[SeqWord]) -> Vec<SeqWord> {
    words.iter().zip(hallucination_mask(words)).filter(|(_, k)| *k).map(|(w, _)| w.clone()).collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Spoken numbers that take a unit ("vài triệu", "hai năm").
const NUMBER_WORDS: &[&str] = &["vài", "mấy", "một", "hai", "ba", "bốn", "sáu", "bảy", "tám", "chín", "mười", "trăm"];

fn is_number_unit(a: &str, b: &str) -> bool {
    let ca = core(a);
    (ca.chars().any(|c| c.is_ascii_digit()) || NUMBER_WORDS.contains(&ca.as_str())) && UNITS.contains(&core(b).as_str())
}

/// English phrases speakers mix in: never split ("lay off", "follow up"…).
const EN_PHRASES: &[&str] = &[
    "lay off",
    "lay up",
    "follow up",
    "next step",
    "wake up",
    "feedback loop",
    "business focus",
    "line manager",
    "public speaking",
    "career navigation",
    "check gpt",
];
/// Words that open a clause: the best place for a line to start.
const CLAUSE_START: &[&str] = &["nếu", "thì", "mà", "vì", "để", "nhưng", "nên", "khi", "còn", "rồi", "tại", "bởi", "hay", "hoặc", "và", "cho"];

/// `a b` must stay together: a compound, a number and its unit, an English phrase, or inside a
/// keyword phrase.
fn unbreakable(a: &str, b: &str, r: &ViRules) -> bool {
    let pair = format!("{} {}", core(a), core(b));
    EN_PHRASES.contains(&pair.as_str())
        || r.compounds.contains(&pair)
        || is_number_unit(a, b)
        || r.keywords.iter().any(|k| {
            let ws: Vec<&str> = k.split_whitespace().collect();
            ws.windows(2).any(|w| w.first().zip(w.get(1)).is_some_and(|(x, y)| format!("{x} {y}") == pair))
        })
}

/// How bad a break between `a` and `b` is (negative = a good place): never inside a compound or
/// between a number and its unit, not after a word that leads into the next, best at
/// punctuation, a pause or a cut.
fn break_cost(a: &SeqWord, b: &SeqWord, r: &ViRules) -> i64 {
    let ca = core(&a.text);
    let mut c = 0;
    if unbreakable(&a.text, &b.text, r) {
        c += 200;
    }
    if NO_END_WORDS.contains(&ca.as_str()) {
        c += 40;
    }
    if a.text.ends_with([',', ';', ':']) {
        c -= 30;
    }
    if b.start - a.end >= Tick(TICKS_PER_SECOND / 4) {
        c -= 30;
    }
    if a.clip != b.clip {
        c -= 20;
    }
    c
}

/// Split one phrase into captions of at most `max_words` words (one more allowed so a single
/// word is never left over): the fewest captions, as even as possible, broken at the cheapest
/// places ([`break_cost`]). Dynamic programming over break positions.
fn split_phrase(p: &[usize], words: &[SeqWord], r: &ViRules, max_words: usize) -> Vec<Vec<usize>> {
    let n = p.len();
    let chars = |g: &[usize]| g.iter().filter_map(|&i| words.get(i)).map(|w| w.text.chars().count() + 1).sum::<usize>().saturating_sub(1);
    // two lines of at most max_line_chars, minus slack for a balanced break
    let cap = if r.max_line_chars == 0 { usize::MAX } else { (2 * r.max_line_chars).saturating_sub(6).max(8) };
    let total = chars(p);
    let texts = |g: &[usize]| g.iter().filter_map(|&i| words.get(i)).map(|w| w.text.as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>();
    if n <= max_words + 1 && total <= cap && layout_cost(&texts(p), r) < 250 {
        return vec![p.to_vec()];
    }
    if n < 2 {
        return vec![p.to_vec()];
    }
    // ponytail: a phrase with no pause for minutes (hostile input) is cut into blocks of
    // 4×max words first, so the table stays small; real speech pauses long before that
    if n > 6 * max_words {
        return p.chunks(4 * max_words).flat_map(|c| split_phrase(c, words, r, max_words)).collect();
    }
    let k = n.div_ceil(max_words).max(total.div_ceil(cap)).min(n / 2).max(1);
    let ideal = n as f64 / k as f64;
    let (lo, hi) = (2usize, max_words + 1);
    // best[g][i]: cost of the first i words in g captions; from[g][i]: where caption g starts
    let inf = i64::MAX / 4;
    let mut best = vec![vec![inf; n + 1]; k + 1];
    let mut from = vec![vec![0usize; n + 1]; k + 1];
    if let Some(row) = best.get_mut(0)
        && let Some(c) = row.get_mut(0)
    {
        *c = 0;
    }
    for g in 1..=k {
        for i in 1..=n {
            for size in lo..=hi.min(i) {
                let j = i - size;
                let Some(&prev) = best.get(g - 1).and_then(|row| row.get(j)) else { continue };
                if prev >= inf {
                    continue;
                }
                let dev = size as f64 - ideal;
                let mut c = prev + (dev * dev * 6.0) as i64;
                let grp = p.get(j..i).unwrap_or_default();
                if chars(grp) > cap {
                    c += 1000;
                }
                c += layout_cost(&texts(grp), r);
                if i < n {
                    let (Some(a), Some(b)) = (p.get(i - 1).and_then(|&x| words.get(x)), p.get(i).and_then(|&x| words.get(x))) else { continue };
                    c += break_cost(a, b, r);
                }
                if let (Some(cell), Some(f)) = (best.get_mut(g).and_then(|row| row.get_mut(i)), from.get_mut(g).and_then(|row| row.get_mut(i)))
                    && c < *cell
                {
                    *cell = c;
                    *f = j;
                }
            }
        }
    }
    if best.get(k).and_then(|row| row.get(n)).is_none_or(|&c| c >= inf) {
        return p.chunks(max_words).map(<[usize]>::to_vec).collect();
    }
    let mut out = Vec::with_capacity(k);
    let (mut g, mut i) = (k, n);
    while g > 0 && i > 0 {
        let j = from.get(g).and_then(|row| row.get(i)).copied().unwrap_or(0);
        out.push(p.get(j..i).unwrap_or_default().to_vec());
        i = j;
        g -= 1;
    }
    out.reverse();
    out
}

/// Lay the sequence transcript out as Vietnamese captions (see the module docs). Times are
/// snapped to frames and placed with `base` (gap frames, minimum duration).
pub fn vi_caption_blocks(words: &[SeqWord], r: &ViRules, base: &CaptionRules, rate: FrameRate) -> Vec<CaptionBlock> {
    // ASR fixes first, so every length decision below sees the final text. Same number of words
    // (a fixed phrase lives on its first word, the others become empty and are skipped), so block
    // word ranges still index the caller's words.
    let fixed = replace_in_words(words, r);
    let words = fixed.as_slice();
    let max_words = r.max_words.max(1);
    let mask = hallucination_mask(words);
    let spoken: Vec<usize> = (0..words.len()).filter(|&i| mask.get(i).copied().unwrap_or(false)).collect();
    // 1) fillers
    let mut kept: Vec<usize> = Vec::with_capacity(spoken.len());
    for (n, &i) in spoken.iter().enumerate() {
        let Some(w) = words.get(i) else { continue };
        let c = core(&w.text);
        if c.is_empty() && w.text.trim().is_empty() {
            continue;
        }
        if r.remove_fillers {
            if r.filler_always.contains(&c) {
                continue;
            }
            let next = spoken.get(n + 1).and_then(|&j| words.get(j));
            let is_final = next.is_none_or(|nx| nx.start - w.end >= r.max_gap) || ends_sentence(&w.text);
            if r.filler_non_final.contains(&c) && !is_final {
                continue;
            }
        }
        kept.push(i);
    }
    // 2) lines: phrases end at a pause, a sentence end or a cut; each phrase is then split into
    // balanced captions at the best places (see `split_phrase`)
    let mut phrases: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for &i in &kept {
        let Some(w) = words.get(i) else { continue };
        if let Some(last) = current.last().and_then(|&l| words.get(l)) {
            // a caption never spans a cut: after edits two clips' words sit back to back. A cut
            // that only drops a few words of the same media (a stutter) is still one sentence.
            let stutter = w.item == last.item && w.index > last.index && w.index - last.index <= 5;
            let cut = w.clip != last.clip && !stutter;
            if cut || w.start - last.end >= r.max_gap || ends_sentence(&last.text) {
                phrases.push(std::mem::take(&mut current));
            }
        }
        current.push(i);
    }
    if !current.is_empty() {
        phrases.push(current);
    }
    // short pieces of one sentence (a breath under 1.5 s, no cut) are joined up to max_words
    let mut joined: Vec<Vec<usize>> = Vec::with_capacity(phrases.len());
    for p in phrases {
        let merge = match (joined.last(), p.first().and_then(|&i| words.get(i))) {
            (Some(prev), Some(first)) => prev.last().and_then(|&l| words.get(l)).is_some_and(|last| {
                let stutter = first.item == last.item && first.index > last.index && first.index - last.index <= 5;
                let same = first.clip == last.clip || stutter;
                // a one- or two-word sentence ("Why?", "Đúng không?") never stands alone
                let tiny = p.len() <= 2;
                same && (tiny || !ends_sentence(&last.text))
                    && first.start - last.end < Tick(TICKS_PER_SECOND * 3 / 2)
                    && (prev.len() < r.min_words || p.len() < r.min_words)
                    && prev.len() + p.len() <= max_words + usize::from(tiny)
            }),
            _ => false,
        };
        match joined.last_mut() {
            Some(prev) if merge => prev.extend(p),
            _ => joined.push(p),
        }
    }
    let groups: Vec<Vec<usize>> = joined.iter().flat_map(|p| split_phrase(p, words, r, max_words)).collect();
    // 3) text (capitalised at sentence starts), times; a short gap is bridged so text doesn't blink
    let bridge = Tick(TICKS_PER_SECOND / 10 * 3);
    let mut spans: Vec<(Tick, Tick, String, std::ops::Range<usize>, Option<String>)> = Vec::with_capacity(groups.len());
    for (gi, g) in groups.iter().enumerate() {
        let (Some(&a), Some(&b)) = (g.first(), g.last()) else { continue };
        let (Some(first), Some(last)) = (words.get(a), words.get(b)) else { continue };
        let prev_last = gi.checked_sub(1).and_then(|p| groups.get(p)).and_then(|p| p.last()).and_then(|&l| words.get(l));
        let new_sentence = prev_last.is_none_or(|p| ends_sentence(&p.text) || first.start - p.end >= r.max_gap);
        let mut text = g.iter().filter_map(|&i| words.get(i)).map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        if new_sentence {
            text = capitalize(&text);
        }
        let mut end = last.end;
        if let Some(next_start) = groups.get(gi + 1).and_then(|n| n.first()).and_then(|&i| words.get(i)).map(|w| w.start)
            && next_start - end < bridge
        {
            end = next_start;
        }
        spans.push((first.start, end, text, a..b + 1, first.speaker.clone()));
    }
    if r.polish {
        let cues: Vec<(Tick, Tick, String)> = spans.iter().map(|s| (s.0, s.1, s.2.clone())).collect();
        for (s, t) in spans.iter_mut().zip(polish_all(&cues, r)) {
            s.2 = t;
        }
    }
    for s in &mut spans {
        s.2 = highlight(&break_lines(&s.2, r), r);
    }
    let out = spans
        .into_iter()
        .map(|(start, end, text, range, speaker)| {
            let (start, end) = snap_span(start, end, rate);
            CaptionBlock { start, end, text, speaker, words: range }
        })
        .collect();
    place_blocks(out, base, rate)
}

/// Units that belong to the number before them ("4 triệu", "2 năm").
const UNITS: &[&str] = &["triệu", "ngàn", "nghìn", "tỷ", "tỉ", "đồng", "k", "năm", "tháng", "tuần", "ngày", "tuổi", "lần", "giờ", "phút"];

/// Break `text` into two lines when it is longer than `line_chars`: the split that balances the
/// lines best, never after a word that leads into the next one ("của", "và", "là"…) and never
/// inside a compound; after a comma or full stop is preferred.
pub fn break_lines(text: &str, r: &ViRules) -> String {
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    if r.line_chars == 0 || text.chars().count() <= r.line_chars || words.len() < 2 || text.contains('\n') {
        return text.to_string();
    }
    match best_break(&words, r) {
        Some((_, k)) => format!("{}\n{}", words.get(..k).unwrap_or_default().join(" "), words.get(k..).unwrap_or_default().join(" ")),
        None => text.to_string(),
    }
}

/// The best place to break `words` into two lines and its score (lower is better; ≥ 250 means a
/// lonely word, a line too wide or a split compound could not be avoided).
fn best_break(words: &[&str], r: &ViRules) -> Option<(i64, usize)> {
    let len = |ws: &[&str]| ws.iter().map(|w| w.chars().count()).sum::<usize>() + ws.len().saturating_sub(1);
    let mut best: Option<(i64, usize)> = None;
    for k in 1..words.len() {
        let (a, b) = (words.get(..k).unwrap_or_default(), words.get(k..).unwrap_or_default());
        let (Some(last), Some(first)) = (a.last(), b.first()) else { continue };
        let mut score = (len(a) as i64 - len(b) as i64).abs();
        if NO_END_WORDS.contains(&core(last).as_str()) {
            score += 30;
        }
        if unbreakable(last, first, r) {
            score += 400; // worse than a line the renderer has to wrap
        }
        if last.ends_with([',', '.', '!', '?', '…', ':', ';']) {
            score -= 8;
        }
        if a.len() == 1 || b.len() == 1 {
            score += 250; // a line never holds a single word
        }
        if CLAUSE_START.contains(&core(first).as_str()) {
            score -= 12; // a line starts where a clause starts
        }
        if r.max_line_chars > 0 && (len(a) > r.max_line_chars || len(b) > r.max_line_chars) {
            score += 300; // the renderer would wrap it again
        }
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, k));
        }
    }
    best
}

/// How badly a caption made of `words` lays out: 0 when it fits one line, else the score of its
/// best line break (see [`best_break`]).
fn layout_cost(words: &[&str], r: &ViRules) -> i64 {
    let chars = words.iter().map(|w| w.chars().count()).sum::<usize>() + words.len().saturating_sub(1);
    if r.line_chars == 0 || chars <= r.line_chars || words.len() < 2 {
        return 0;
    }
    best_break(words, r).map_or(0, |(s, _)| s.max(0))
}

/// The rule's ASR fixes `(heard, meant)`, matched on whole words (punctuation and case ignored;
/// the punctuation after the last word is kept), applied to the transcript words (see
/// [`vi_caption_blocks`]).
fn replace_in_words(words: &[SeqWord], r: &ViRules) -> Vec<SeqWord> {
    let mut out = words.to_vec();
    for (from, to) in &r.replacements {
        let pat: Vec<String> = from.split_whitespace().map(core).collect();
        if pat.is_empty() || pat.iter().all(String::is_empty) || to.trim().is_empty() {
            continue;
        }
        let mut i = 0;
        while i + pat.len() <= out.len() {
            let hit = out.get(i..i + pat.len()).is_some_and(|w| w.iter().map(|t| core(&t.text)).eq(pat.iter().cloned()));
            if !hit {
                i += 1;
                continue;
            }
            let suf = out.get(i + pat.len() - 1).map(|t| split(&t.text).suf).unwrap_or_default();
            let upper = out.get(i).and_then(|t| t.text.chars().next()).is_some_and(char::is_uppercase);
            let end = out.get(i + pat.len() - 1).map(|w| w.end);
            if let Some(first) = out.get_mut(i) {
                let t = to.trim();
                first.text = format!("{}{suf}", if upper { capitalize(t) } else { t.to_string() });
                if let Some(e) = end {
                    first.end = e;
                }
            }
            for w in out.iter_mut().skip(i + 1).take(pat.len() - 1) {
                w.text = String::new();
            }
            i += pat.len();
        }
    }
    out
}

/// Colour numbers with their unit ("4 triệu", "50%") and the rule's keywords with
/// `<c #rrggbb>…</c>` (punctuation stays outside the colour). Each line on its own.
pub fn highlight(text: &str, r: &ViRules) -> String {
    let Some(colour) = r.highlight.as_deref() else { return text.to_string() };
    let phrases: Vec<Vec<String>> =
        r.keywords.iter().map(|k| k.split_whitespace().map(|w| w.to_lowercase()).collect::<Vec<_>>()).filter(|p| !p.is_empty()).collect();
    text.split('\n')
        .map(|line| {
            let toks: Vec<Token> = line.split(' ').filter(|w| !w.is_empty()).map(split).collect();
            let low: Vec<String> = toks.iter().map(|t| t.core.to_lowercase()).collect();
            let mut mark = vec![false; toks.len()];
            for (i, t) in toks.iter().enumerate() {
                if t.core.chars().any(|c| c.is_ascii_digit()) {
                    mark[i] = true;
                    if t.suf.is_empty() && low.get(i + 1).is_some_and(|u| UNITS.contains(&u.as_str())) {
                        mark[i + 1] = true;
                    }
                }
            }
            for p in &phrases {
                for i in 0..toks.len() {
                    if low.get(i..i + p.len()).is_some_and(|w| w == p.as_slice()) {
                        mark.iter_mut().skip(i).take(p.len()).for_each(|m| *m = true);
                    }
                }
            }
            let mut out: Vec<String> = Vec::with_capacity(toks.len());
            let mut i = 0;
            while i < toks.len() {
                let Some(t) = toks.get(i) else { break };
                if !mark.get(i).copied().unwrap_or(false) || t.core.is_empty() {
                    out.push(t.joined());
                    i += 1;
                    continue;
                }
                // a run of marked words with no punctuation between them shares one colour
                let mut j = i;
                while mark.get(j + 1).copied().unwrap_or(false)
                    && toks.get(j).is_some_and(|x| x.suf.is_empty())
                    && toks.get(j + 1).is_some_and(|x| x.pre.is_empty())
                {
                    j += 1;
                }
                let words: Vec<&str> = (i..=j).filter_map(|k| toks.get(k)).map(|x| x.core.as_str()).collect();
                let suf = toks.get(j).map(|x| x.suf.as_str()).unwrap_or("");
                out.push(format!("{}<c {colour}>{}</c>{suf}", t.pre, words.join(" ")));
                i = j + 1;
            }
            out.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Debug, Default)]
struct Token {
    pre: String,
    core: String,
    suf: String,
}

impl Token {
    fn tail(&self) -> String {
        if self.suf.is_empty() { self.core.chars().last().map(String::from).unwrap_or_default() } else { self.suf.clone() }
    }
    fn joined(&self) -> String {
        format!("{}{}{}", self.pre, self.core, self.suf)
    }
}

fn split(t: &str) -> Token {
    let chars: Vec<char> = t.chars().collect();
    let p = chars.iter().take_while(|c| OPEN.contains(c)).count();
    let rest = chars.get(p..).unwrap_or_default();
    let s = rest.iter().rev().take_while(|c| CLOSE.contains(c)).count();
    let core_end = rest.len().saturating_sub(s);
    Token {
        pre: chars.get(..p).unwrap_or_default().iter().collect(),
        core: rest.get(..core_end).unwrap_or_default().iter().collect(),
        suf: rest.get(core_end..).unwrap_or_default().iter().collect(),
    }
}

fn ends_sentence_mark(s: &str) -> bool {
    s.contains(SENTENCE_END)
}

fn ascii_digits(s: &str, max: usize) -> bool {
    !s.is_empty() && s.chars().count() <= max && s.chars().all(|c| c.is_ascii_digit())
}

fn word_digit(w: &str) -> Option<&'static str> {
    WORD_DIGIT.iter().find(|(k, _)| *k == w).map(|(_, d)| *d)
}

fn starts_upper(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

/// Clean up one caption (see the module docs). `new_sentence`: it opens a spoken sentence (after a
/// pause). Returns the text unchanged when nothing needed fixing, or when everything was a filler.
pub fn polish_text(text: &str, prev: Option<&str>, vocab: &HashSet<String>, new_sentence: bool, r: &ViRules) -> String {
    let mut changed = false;
    let mut out_lines: Vec<String> = Vec::new();
    let prev_tail = prev.and_then(|p| p.split(' ').next_back()).map(|t| split(t).tail()).unwrap_or_default();
    let lines: Vec<&str> = text.split('\n').collect();
    let is_proper = |t: Option<&Token>| {
        t.is_some_and(|t| {
            let low = t.core.to_lowercase();
            starts_upper(&t.core) && !r.common.contains(&low) && !vocab.contains(&low)
        })
    };
    for (li, line) in lines.iter().enumerate() {
        let mut parts: Vec<Token> = line.split(' ').filter(|t| !t.is_empty()).map(split).collect();

        // 1) digit runs mixed with number words: "một chín 38" → "1938"
        let mut merged: Vec<Token> = Vec::with_capacity(parts.len());
        let mut i = 0;
        while i < parts.len() {
            let (mut j, mut chunks, mut nwords) = (i, 0, 0);
            while let Some(p) = parts.get(j) {
                if ascii_digits(&p.core, 4) {
                    chunks += 1;
                } else if word_digit(&p.core.to_lowercase()).is_some() {
                    nwords += 1;
                } else {
                    break;
                }
                j += 1;
                if !p.suf.is_empty() {
                    break;
                }
            }
            if j - i >= 2 && chunks >= 1 && nwords >= 1 {
                let digits: String = parts
                    .get(i..j)
                    .unwrap_or_default()
                    .iter()
                    .map(|p| if ascii_digits(&p.core, 4) { p.core.clone() } else { word_digit(&p.core.to_lowercase()).unwrap_or("").to_string() })
                    .collect();
                if (2..=4).contains(&digits.chars().count()) {
                    let pre = parts.get(i).map(|p| p.pre.clone()).unwrap_or_default();
                    let suf = parts.get(j - 1).map(|p| p.suf.clone()).unwrap_or_default();
                    merged.push(Token { pre, core: digits, suf });
                    changed = true;
                    i = j;
                    continue;
                }
            }
            if let Some(p) = parts.get(i) {
                merged.push(p.clone());
            }
            i += 1;
        }
        parts = merged;

        // 2) a digit inside words: "7 nghiệp" → "thất nghiệp"
        for k in 0..parts.len().saturating_sub(1) {
            let Some(next) = parts.get(k + 1).map(|p| p.core.to_lowercase()) else { continue };
            let Some(p) = parts.get_mut(k) else { continue };
            if !p.suf.is_empty() || !(p.core == "10" || ascii_digits(&p.core, 1)) {
                continue;
            }
            let cands: Vec<&str> = SINO_NUMBERS.iter().copied().filter(|w| r.compounds.contains(&format!("{w} {next}"))).collect();
            if let [only] = cands.as_slice() {
                p.core = (*only).to_string();
                changed = true;
            }
        }

        // 3) fillers, except at the end of the caption
        let count = parts.len();
        let last_line = li + 1 == lines.len();
        let before = parts.len();
        parts = parts
            .into_iter()
            .enumerate()
            .filter(|(k, p)| {
                let low = p.core.to_lowercase();
                let is_final = last_line && k + 1 == count;
                !(r.filler_always.contains(&low) || (r.filler_non_final.contains(&low) && !is_final))
            })
            .map(|(_, p)| p)
            .collect();
        changed |= parts.len() != before;

        // 4) stray capitals inside a sentence
        let alpha: Vec<&String> = parts.iter().map(|p| &p.core).filter(|c| c.chars().any(char::is_alphabetic)).collect();
        let line_all_caps = !alpha.is_empty() && alpha.iter().all(|c| **c == c.to_uppercase());
        for k in 0..parts.len() {
            let Some(core) = parts.get(k).map(|p| p.core.clone()) else { continue };
            if !starts_upper(&core) {
                continue;
            }
            let tail = if k == 0 { if li == 0 { prev_tail.clone() } else { String::new() } } else { parts.get(k - 1).map(Token::tail).unwrap_or_default() };
            let low = core.to_lowercase();
            let rest: String = core.chars().skip(1).collect();
            let keep = (core.chars().count() > 1 && core == core.to_uppercase() && line_all_caps)
                || (core != core.to_uppercase() && rest != rest.to_lowercase())
                || core.chars().any(|c| c.is_numeric())
                || (!r.common.contains(&low) && !vocab.contains(&low))
                || (k == 0 && li == 0 && (new_sentence || prev.is_none()))
                || ends_sentence_mark(&tail)
                // AYP: a capital next to a proper noun is part of the name ("Hà Nội", "Việt Nam").
                || is_proper(k.checked_sub(1).and_then(|j| parts.get(j)))
                || is_proper(parts.get(k + 1));
            if !keep && let Some(p) = parts.get_mut(k) {
                p.core = low;
                changed = true;
            }
        }

        // 5) capital at sentence starts: after a pause, and after sentence-ending punctuation
        for k in 0..parts.len() {
            let starts = (li == 0 && k == 0 && new_sentence) || (k > 0 && parts.get(k - 1).is_some_and(|p| ends_sentence_mark(&p.tail())));
            let Some(p) = parts.get_mut(k) else { continue };
            if starts && p.core.chars().next().is_some_and(|c| c.is_alphabetic() && c.is_lowercase()) {
                p.core = capitalize(&p.core);
                changed = true;
            }
        }

        if !parts.is_empty() {
            out_lines.push(parts.iter().map(Token::joined).collect::<Vec<_>>().join(" "));
        }
    }
    let new = out_lines.join("\n");
    if changed && !new.is_empty() { new } else { text.to_string() }
}

/// [`polish_text`] over captions in time order: a caption opens a sentence after a pause of
/// `sentence_gap` or when the previous one ends a sentence. Words seen in lower case in these
/// captions count as common words.
pub fn polish_all(cues: &[(Tick, Tick, String)], r: &ViRules) -> Vec<String> {
    let mut vocab = HashSet::new();
    for (_, _, t) in cues {
        for w in t.split_whitespace() {
            let c = split(w).core;
            if c.chars().next().is_some_and(char::is_lowercase) {
                vocab.insert(c.to_lowercase());
            }
        }
    }
    let mut prev_text: Option<String> = None;
    let mut prev_end: Option<Tick> = None;
    let mut out = Vec::with_capacity(cues.len());
    for (start, end, text) in cues {
        let mut ns = prev_end.is_none_or(|e| *start - e >= r.sentence_gap);
        if !ns && let Some(last) = prev_text.as_deref().map(str::trim).and_then(|p| p.chars().last()) {
            ns = SENTENCE_END.contains(&last);
        }
        let new = polish_text(text, prev_text.as_deref(), &vocab, ns, r);
        prev_text = Some(new.clone());
        prev_end = Some(*end);
        out.push(new);
    }
    out
}

#[cfg(test)]
mod tests;
