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
//! `config.txt`: `sentence_gap_seconds = 0.6`). Unreadable files are ignored.

use std::collections::HashSet;
use std::path::Path;

use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick};

use crate::transcript::{CaptionBlock, CaptionRules, SeqWord, place_blocks, snap_span};

const COMPOUNDS: &str = include_str!("vi_compounds.txt");

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
        let compounds: HashSet<String> = COMPOUNDS.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        let mut common = set(NO_END_WORDS);
        common.extend(set(POLISH_COMMON));
        add_syllables(&mut common, &compounds);
        let gap = Tick(TICKS_PER_SECOND / 10 * 6);
        Self {
            max_words: 8,
            max_gap: gap,
            remove_fillers: true,
            polish: true,
            sentence_gap: gap,
            compounds,
            common,
            filler_always: set(FILLER_ALWAYS),
            filler_non_final: set(FILLER_NON_FINAL),
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
                if k.trim() == "sentence_gap_seconds"
                    && let Ok(v) = v.trim().parse::<f64>()
                    && v.is_finite()
                    && (0.0..=60.0).contains(&v)
                {
                    r.sentence_gap = Tick::from_seconds_f64(v);
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

/// Lay the sequence transcript out as Vietnamese captions (see the module docs). Times are
/// snapped to frames and placed with `base` (gap frames, minimum duration).
pub fn vi_caption_blocks(words: &[SeqWord], r: &ViRules, base: &CaptionRules, rate: FrameRate) -> Vec<CaptionBlock> {
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
    // 2) lines
    let compound = |a: &SeqWord, b: &SeqWord| r.compounds.contains(&format!("{} {}", core(&a.text), core(&b.text)));
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for (n, &i) in kept.iter().enumerate() {
        let Some(w) = words.get(i) else { continue };
        if let Some(last) = current.last().and_then(|&l| words.get(l)) {
            let pair_is_compound = compound(last, w);
            // A sentence-ending word is not left alone on the next line.
            let closes_sentence = ends_sentence(&w.text) && current.len() == max_words;
            let full = current.len() >= max_words && !pair_is_compound && !closes_sentence;
            // Nearly full and this word opens a compound with the next one: break before it.
            let opens_compound = kept.get(n + 1).and_then(|&j| words.get(j)).is_some_and(|nx| compound(w, nx));
            let nearly_full = current.len() + 1 == max_words && opens_compound && current.len() >= 2;
            // a caption never spans a cut: after edits two clips' words sit back to back. A cut
            // that only drops a few words of the same media (a stutter) is still one sentence.
            let stutter = w.item == last.item && w.index > last.index && w.index - last.index <= 5;
            let cut = w.clip != last.clip && !stutter;
            if cut || w.start - last.end >= r.max_gap || ends_sentence(&last.text) || full || current.len() > max_words + 1 || nearly_full {
                groups.push(std::mem::take(&mut current));
            }
        }
        current.push(i);
    }
    if !current.is_empty() {
        groups.push(current);
    }
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
    let out = spans
        .into_iter()
        .map(|(start, end, text, range, speaker)| {
            let (start, end) = snap_span(start, end, rate);
            CaptionBlock { start, end, text, speaker, words: range }
        })
        .collect();
    place_blocks(out, base, rate)
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
