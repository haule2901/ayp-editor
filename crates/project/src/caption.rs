//! Caption tracks.
//!
//! Premiere keeps captions in their own track area above the video tracks. A [`CaptionTrack`] has a
//! format (Subtitle, CEA-608, CEA-708, Teletext), a track style (font, size, colour, background box,
//! position) and a list of [`Caption`] blocks with exact in/out times in sequence ticks. Captions on
//! one track never overlap (like track items); [`CaptionTrack::check`] verifies it.
//!
//! Caption text is kept as written (lines separated by `\n`, inline `<i>`/`<b>`/`<u>` tags and
//! WebVTT cue settings preserved) so files round-trip; renderers strip markup for display.

use filmcraft_time::{Tick, TimeRange};
use serde::{Deserialize, Serialize};

use crate::{ClipId, TrackId};

/// Caption stream format of a track (Premiere's "Caption Format" choices).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CaptionFormat {
    /// Open/sidecar subtitles (SRT, WebVTT).
    #[default]
    Subtitle,
    /// CEA-608 (line 21) closed captions.
    Cea608,
    /// CEA-708 (DTVCC) closed captions.
    Cea708,
    /// EBU Teletext subtitles.
    Teletext,
}

impl CaptionFormat {
    pub const ALL: [CaptionFormat; 4] = [CaptionFormat::Subtitle, CaptionFormat::Cea608, CaptionFormat::Cea708, CaptionFormat::Teletext];
    pub fn label(self) -> &'static str {
        match self {
            CaptionFormat::Subtitle => "Subtitle",
            CaptionFormat::Cea608 => "CEA-608",
            CaptionFormat::Cea708 => "CEA-708",
            CaptionFormat::Teletext => "Teletext",
        }
    }
    pub fn from_name(s: &str) -> Option<CaptionFormat> {
        let n = s.to_ascii_lowercase().replace(['-', ' ', '_'], "");
        Self::ALL.iter().copied().find(|f| f.label().to_ascii_lowercase().replace(['-', ' '], "") == n)
    }
    /// Characters per line limit used when wrapping (608 is fixed at 32 columns).
    pub fn max_columns(self) -> Option<usize> {
        match self {
            CaptionFormat::Cea608 => Some(32),
            CaptionFormat::Teletext => Some(40),
            CaptionFormat::Cea708 => Some(42),
            CaptionFormat::Subtitle => None,
        }
    }
}

/// Horizontal text alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CaptionAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// Vertical placement of the caption block in the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CaptionAnchor {
    Top,
    Middle,
    #[default]
    Bottom,
}

/// AYP: how the words being spoken stand out (needs word timings, see [`Caption::words`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CaptionHighlight {
    #[default]
    None,
    /// Every word heard so far takes the highlight colour (CapCut karaoke, `ktv_color`).
    Karaoke,
    /// Only the word being said (the last one started) takes the highlight colour.
    ActiveWord,
}

impl CaptionHighlight {
    pub const ALL: [CaptionHighlight; 3] = [CaptionHighlight::None, CaptionHighlight::Karaoke, CaptionHighlight::ActiveWord];
    pub fn label(self) -> &'static str {
        match self {
            CaptionHighlight::None => "None",
            CaptionHighlight::Karaoke => "Karaoke",
            CaptionHighlight::ActiveWord => "Active word",
        }
    }
}

/// Track-level caption style (Premiere's caption track style / Essential Graphics text settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptionStyle {
    /// Font family name (bundled, craft-fonts or system); unknown or empty names fall back to Inter.
    pub font: String,
    /// AYP: font weight / style name ("Regular", "SemiBold", "Bold", "Black"…); empty = SemiBold.
    pub font_style: String,
    /// Font size in pixels for a 1080-line frame (scaled with the frame height).
    pub size: f32,
    /// AYP: letter spacing in 1/1000 em (CapCut `letter_spacing`).
    pub tracking: f32,
    /// Text colour, sRGB + alpha.
    pub color: [u8; 4],
    /// Draw a background box behind each line.
    pub background: bool,
    /// Background box colour, sRGB + alpha.
    pub background_color: [u8; 4],
    /// AYP: box corner roundness, 0 (square) to 1 (pill) (CapCut `background_round_radius`).
    pub background_round_radius: f32,
    /// AYP: extra room between text and box edge, pixels at 1080 lines.
    pub background_padding: f32,
    /// AYP: one box around the whole caption instead of one per line.
    pub background_block: bool,
    pub align: CaptionAlign,
    pub anchor: CaptionAnchor,
    /// Distance from the anchored frame edge, as a fraction of the frame height.
    pub margin: f32,
    /// Line height as a multiple of the font size.
    pub line_spacing: f32,
    /// Text outline width in pixels at 1080 lines (0 = none).
    pub outline: f32,
    pub outline_color: [u8; 4],
    /// AYP: highlight of the words being spoken.
    pub highlight: CaptionHighlight,
    /// AYP: colour of highlighted words (CapCut `ktv_color`), sRGB + alpha.
    pub highlight_color: [u8; 4],
    /// AYP: size of the word being said relative to the others (1 = same size).
    pub highlight_scale: f32,
    /// AYP: drop shadow behind text, outline and box (CapCut `has_shadow`).
    pub shadow: bool,
    /// AYP: shadow colour, sRGB + alpha (CapCut `shadow_color` + `shadow_alpha`).
    pub shadow_color: [u8; 4],
    /// AYP: direction the shadow falls, degrees counter-clockwise from the right (-45 = down
    /// right, CapCut `shadow_angle`).
    pub shadow_angle: f32,
    /// AYP: shadow offset in pixels at 1080 lines (CapCut `shadow_distance`).
    pub shadow_distance: f32,
    /// AYP: shadow softness (blur) in pixels at 1080 lines (CapCut `shadow_smoothing`).
    pub shadow_smoothing: f32,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font: "Inter".into(),
            font_style: "SemiBold".into(),
            size: 54.0,
            tracking: 0.0,
            color: [255, 255, 255, 255],
            background: true,
            background_color: [0, 0, 0, 191],
            background_round_radius: 0.0,
            background_padding: 0.0,
            background_block: false,
            align: CaptionAlign::Center,
            anchor: CaptionAnchor::Bottom,
            margin: 0.08,
            line_spacing: 1.25,
            outline: 0.0,
            outline_color: [0, 0, 0, 255],
            highlight: CaptionHighlight::None,
            highlight_color: [255, 210, 0, 255],
            highlight_scale: 1.0,
            shadow: false,
            shadow_color: [0, 0, 0, 160],
            shadow_angle: -45.0,
            shadow_distance: 5.0,
            shadow_smoothing: 6.0,
        }
    }
}

/// One caption block on a caption track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub id: ClipId,
    /// Timeline start (sequence ticks).
    pub start: Tick,
    pub duration: Tick,
    /// Caption text; lines separated by `\n`.
    pub text: String,
    /// Speaker name (WebVTT `<v Name>` voice span).
    #[serde(default)]
    pub speaker: Option<String>,
    /// WebVTT cue identifier, kept for round trips.
    #[serde(default)]
    pub cue_id: Option<String>,
    /// WebVTT cue settings (`line:90% align:start`…), kept verbatim.
    #[serde(default)]
    pub settings: String,
    /// AYP: when each drawn word is heard (karaoke / active-word highlight). Empty for captions
    /// typed by hand or imported from files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<CapWord>,
}

/// AYP: one spoken word of a caption (CapCut's `words`): its bytes in the drawn text
/// (`plain_text(&caption.text)`, lines joined by `\n`) and when it is heard, relative to the
/// caption start so the timings move with the caption.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapWord {
    pub range: std::ops::Range<usize>,
    pub start: Tick,
    pub end: Tick,
}

/// AYP: time the words of `plain` from spoken words `(text, start, end)`. Words match in order
/// ignoring case and punctuation (a few unmatched spoken words, like removed fillers, are
/// skipped); a drawn word with no match ("10" for "mười") shares the time between its neighbours.
pub fn align_words(plain: &str, src: &[(&str, Tick, Tick)]) -> Vec<CapWord> {
    const LOOKAHEAD: usize = 6;
    let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let (Some(t0), Some(t1)) = (src.iter().map(|w| w.1).min(), src.iter().map(|w| w.2).max()) else { return Vec::new() };
    let mut toks: Vec<std::ops::Range<usize>> = Vec::new();
    let mut open = None;
    for (i, ch) in plain.char_indices() {
        match (ch.is_whitespace(), open) {
            (true, Some(a)) => {
                toks.push(a..i);
                open = None;
            }
            (false, None) => open = Some(i),
            _ => {}
        }
    }
    if let Some(a) = open {
        toks.push(a..plain.len());
    }
    let mut hit: Vec<Option<(Tick, Tick)>> = vec![None; toks.len()];
    let mut j = 0;
    for (r, h) in toks.iter().zip(hit.iter_mut()) {
        let k = key(plain.get(r.clone()).unwrap_or(""));
        if k.is_empty() {
            continue;
        }
        let rest = src.get(j..).unwrap_or_default();
        if let Some(off) = rest.iter().take(LOOKAHEAD).position(|w| key(w.0) == k) {
            if let Some(w) = rest.get(off) {
                *h = Some((w.1, w.2));
            }
            j += off + 1;
        }
    }
    let mut out: Vec<CapWord> = Vec::with_capacity(toks.len());
    let mut k = 0;
    while k < toks.len() {
        if let Some(Some((start, end))) = hit.get(k) {
            out.push(CapWord { range: toks[k].clone(), start: *start, end: *end });
            k += 1;
            continue;
        }
        let run_end = (k..toks.len()).find(|&m| hit.get(m).is_some_and(Option::is_some)).unwrap_or(toks.len());
        let lo = out.last().map_or(t0, |w| w.end);
        let hi = hit.get(run_end).copied().flatten().map_or(t1, |h| h.0).max(lo);
        let (n, span) = ((run_end - k) as i128, i128::from(hi.0) - i128::from(lo.0));
        let at = |m: i128| lo + Tick(i64::try_from(span * m / n.max(1)).unwrap_or(0));
        for (m, r) in toks.get(k..run_end).unwrap_or_default().iter().enumerate() {
            out.push(CapWord { range: r.clone(), start: at(m as i128), end: at(m as i128 + 1) });
        }
        k = run_end;
    }
    out
}

impl Caption {
    /// AYP: after the text changed from `old_text`, carry the word timings over to the new text.
    pub fn retime_words(&mut self, old_text: &str) {
        if self.words.is_empty() {
            return;
        }
        let old = plain_text(old_text);
        let src: Vec<(&str, Tick, Tick)> = self.words.iter().filter_map(|w| Some((old.get(w.range.clone())?, w.start, w.end))).collect();
        self.words = align_words(&plain_text(&self.text), &src);
    }
    /// AYP: the caption start moved later by `by`: word times (relative to the start) move earlier.
    pub fn shift_words(&mut self, by: Tick) {
        for w in &mut self.words {
            w.start -= by;
            w.end -= by;
        }
    }
    pub fn end(&self) -> Tick {
        self.start + self.duration
    }
    pub fn range(&self) -> TimeRange {
        TimeRange::new(self.start, self.duration)
    }
    /// The caption's text with markup removed and entities decoded (what is drawn).
    pub fn plain_lines(&self) -> Vec<String> {
        plain_text(&self.text).lines().map(str::to_string).collect()
    }
}

/// A caption track (shown in the caption area above the video tracks).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptionTrack {
    pub id: TrackId,
    pub name: String,
    pub format: CaptionFormat,
    /// BCP-47 language tag (`en`, `fr-CA`…).
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub style: CaptionStyle,
    /// Track output (eye): shown in the Program monitor and burned in on export.
    pub enabled: bool,
    pub locked: bool,
    /// Follows inserts / extracts on other tracks.
    #[serde(default = "yes")]
    pub sync_lock: bool,
    /// Captions sorted by start, non-overlapping.
    pub captions: Vec<Caption>,
    /// WebVTT header blocks (STYLE / REGION / NOTE) kept for round trips.
    #[serde(default)]
    pub vtt_blocks: Vec<String>,
}

fn yes() -> bool {
    true
}

impl CaptionTrack {
    pub fn new(id: TrackId, name: String, format: CaptionFormat) -> Self {
        Self {
            id,
            name,
            format,
            language: "en".into(),
            style: CaptionStyle::default(),
            enabled: true,
            locked: false,
            sync_lock: true,
            captions: Vec::new(),
            vtt_blocks: Vec::new(),
        }
    }
    pub fn end(&self) -> Tick {
        self.captions.iter().map(Caption::end).max().unwrap_or(Tick::ZERO)
    }
    /// Caption covering `t`.
    pub fn caption_at(&self, t: Tick) -> Option<&Caption> {
        let idx = self.captions.partition_point(|c| c.start <= t);
        idx.checked_sub(1).map(|i| &self.captions[i]).filter(|c| t < c.end())
    }
    pub fn caption(&self, id: ClipId) -> Option<&Caption> {
        self.captions.iter().find(|c| c.id == id)
    }
    pub fn caption_mut(&mut self, id: ClipId) -> Option<&mut Caption> {
        self.captions.iter_mut().find(|c| c.id == id)
    }
    pub fn sort(&mut self) {
        self.captions.sort_by_key(|c| c.start);
    }
    /// Invariant check: captions sorted, positive duration, no overlaps.
    pub fn check(&self) -> Result<(), String> {
        self.check_bounds()?;
        for c in &self.captions {
            if c.duration.0 <= 0 {
                return Err(format!("{}: caption {:?} has non-positive duration", self.name, c.id));
            }
        }
        for w in self.captions.windows(2) {
            if w[0].end() > w[1].start {
                return Err(format!("{}: captions {:?} and {:?} overlap", self.name, w[0].id, w[1].id));
            }
        }
        Ok(())
    }
    /// Only the time bounds that keep caption arithmetic from overflowing (see `Track::check_bounds`).
    pub fn check_bounds(&self) -> Result<(), String> {
        for c in &self.captions {
            if !crate::bounded_time_range(c.start, c.duration) {
                return Err(format!("{}: caption {:?} is outside supported time bounds", self.name, c.id));
            }
        }
        Ok(())
    }
}

/// Strip `<…>` tags and decode the common HTML/WebVTT entities.
pub fn plain_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                // a tag runs to the next '>' (if none, keep the '<' literally)
                let rest: String = chars.clone().collect();
                if let Some(end) = rest.find('>') {
                    for _ in 0..rest[..=end].chars().count() {
                        chars.next();
                    }
                } else {
                    out.push('<');
                }
            }
            '{' if chars.peek() == Some(&'\\') => {
                // SSA-style override blocks some SRT files carry: {\an8}
                let rest: String = chars.clone().collect();
                if let Some(end) = rest.find('}') {
                    for _ in 0..rest[..=end].chars().count() {
                        chars.next();
                    }
                } else {
                    out.push('{');
                }
            }
            '&' => {
                let rest: String = chars.clone().take(8).collect();
                let ents = [
                    ("amp;", '&'),
                    ("lt;", '<'),
                    ("gt;", '>'),
                    ("nbsp;", '\u{a0}'),
                    ("quot;", '"'),
                    ("apos;", '\''),
                    ("lrm;", '\u{200e}'),
                    ("rlm;", '\u{200f}'),
                ];
                if let Some((name, ch)) = ents.iter().find(|(n, _)| rest.starts_with(n)) {
                    for _ in 0..name.len() {
                        chars.next();
                    }
                    out.push(*ch);
                } else {
                    out.push('&');
                }
            }
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain() {
        assert_eq!(plain_text("<i>Hello</i> &amp; <b>bye</b>"), "Hello & bye");
        assert_eq!(plain_text("{\\an8}Top"), "Top");
        assert_eq!(plain_text("a < b"), "a < b");
        assert_eq!(plain_text("<v Bob>Hi</v>"), "Hi");
    }

    #[test]
    fn caption_at() {
        let mut t = CaptionTrack::new(TrackId(1), "Subtitle".into(), CaptionFormat::Subtitle);
        for (i, s) in [0i64, 100, 300].iter().enumerate() {
            t.captions.push(Caption {
                id: ClipId(i as u64),
                start: Tick(*s),
                duration: Tick(50),
                text: format!("c{i}"),
                speaker: None,
                cue_id: None,
                settings: String::new(),
                words: Vec::new(),
            });
        }
        assert_eq!(t.caption_at(Tick(120)).unwrap().text, "c1");
        assert!(t.caption_at(Tick(160)).is_none());
        assert!(t.check().is_ok());
        assert_eq!(CaptionFormat::from_name("cea608"), Some(CaptionFormat::Cea608));
        assert_eq!(CaptionFormat::from_name("CEA-708"), Some(CaptionFormat::Cea708));
    }

    /// AYP: word timings line up with the drawn words even after clean-up changed the text.
    #[test]
    fn align_words_matches_and_interpolates() {
        let plain = "Mình nợ 10 triệu,\nrồi.";
        let src = [
            ("mình", Tick(0), Tick(10)),
            ("ừ", Tick(10), Tick(20)),
            ("nợ", Tick(20), Tick(30)),
            ("mười", Tick(30), Tick(40)),
            ("triệu", Tick(50), Tick(60)),
            ("rồi", Tick(60), Tick(70)),
        ];
        let w = align_words(plain, &src);
        let words: Vec<&str> = w.iter().map(|x| &plain[x.range.clone()]).collect();
        assert_eq!(words, ["Mình", "nợ", "10", "triệu,", "rồi."]);
        assert_eq!((w[0].start, w[0].end), (Tick(0), Tick(10)));
        assert_eq!((w[1].start, w[1].end), (Tick(20), Tick(30)));
        // "10" was "mười": it gets the time between its neighbours
        assert_eq!((w[2].start, w[2].end), (Tick(30), Tick(50)));
        assert_eq!((w[3].start, w[4].end), (Tick(50), Tick(70)));
        assert!(align_words("x", &[]).is_empty());
        // hostile times never overflow
        let wild = align_words("a b c", &[("z", Tick(i64::MIN), Tick(i64::MAX))]);
        assert_eq!(wild.len(), 3);
    }

    #[test]
    fn old_captions_load_without_words_and_text_edits_keep_timings() {
        let c: Caption = serde_json::from_str(r#"{"id":1,"start":0,"duration":100,"text":"a b"}"#).unwrap();
        assert!(c.words.is_empty());
        let mut c = Caption { words: align_words("Hello world", &[("hello", Tick(0), Tick(5)), ("world", Tick(5), Tick(9))]), ..c };
        c.text = "Hello <c #ffd200>big</c> world".into();
        c.retime_words("Hello world");
        let plain = plain_text(&c.text);
        let got: Vec<(&str, i64, i64)> = c.words.iter().map(|w| (&plain[w.range.clone()], w.start.0, w.end.0)).collect();
        assert_eq!(got, [("Hello", 0, 5), ("big", 5, 5), ("world", 5, 9)]);
        c.shift_words(Tick(5));
        assert_eq!(c.words[2].start, Tick(0));
    }

    #[test]
    fn corrupt_caption_times_do_not_overflow_the_overlap_check() {
        let mut track = CaptionTrack::new(TrackId(1), "Subtitles".into(), CaptionFormat::Subtitle);
        for (id, start) in [(1, i64::MAX), (2, 10)] {
            track.captions.push(Caption {
                id: ClipId(id),
                start: Tick(start),
                duration: Tick(50),
                text: String::new(),
                speaker: None,
                cue_id: None,
                settings: String::new(),
                words: Vec::new(),
            });
        }
        let result = std::panic::catch_unwind(|| track.check());
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }
}
