//! Caption layout and burn-in.
//!
//! [`overlay`] lays out the caption showing on a track at a time with the track's
//! [`CaptionStyle`] (size relative to a 1080-line frame, colour, background box, outline,
//! alignment, anchor and margin; WebVTT `line:N%` and `align:` cue settings override the style)
//! and rasterises it into a small premultiplied linear-light RGBA [`Overlay`] positioned in the
//! frame. Renderers composite overlays over the finished picture.

use filmcraft_color::srgb_to_linear;
use filmcraft_project::{Caption, CaptionAlign, CaptionAnchor, CaptionHighlight, CaptionStyle, CaptionTrack, Sequence};
use filmcraft_time::Tick;
use std::ops::Range;

use filmcraft_text::{ParagraphStyle, StyleRun, TextStyle, layout_rich, render};

/// A rendered caption: premultiplied linear-light RGBA f32 pixels at `(x, y)` in the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Overlay {
    /// Composite over a premultiplied RGBA f32 canvas of `cw`×`ch` pixels.
    pub fn composite_onto(&self, canvas: &mut [f32], cw: usize, ch: usize) {
        for y in 0..self.h {
            let cy = self.y + y as i32;
            if cy < 0 || cy >= ch as i32 {
                continue;
            }
            for x in 0..self.w {
                let cx = self.x + x as i32;
                if cx < 0 || cx >= cw as i32 {
                    continue;
                }
                let s = &self.px[(y * self.w + x) * 4..(y * self.w + x) * 4 + 4];
                if s[3] <= 0.0 {
                    continue;
                }
                let i = (cy as usize * cw + cx as usize) * 4;
                let k = 1.0 - s[3];
                for c in 0..4 {
                    canvas[i + c] = s[c] + canvas[i + c] * k;
                }
            }
        }
    }
}

/// The caption face: the track's font family (Inter when empty or not installed) at `px`, in the
/// track's weight (AYP; SemiBold when empty) and letter spacing.
fn caption_style(style: &CaptionStyle, px: f32) -> TextStyle {
    let font = style.font.trim();
    let family = if font.is_empty() { filmcraft_text::fonts::DEFAULT_FAMILY } else { font };
    let weight = if style.font_style.trim().is_empty() { "SemiBold" } else { style.font_style.trim() };
    let tracking = if style.tracking.is_finite() { style.tracking.clamp(-200.0, 1000.0) } else { 0.0 };
    TextStyle { family: family.into(), style: weight.into(), size: px, tracking, ..Default::default() }
}

fn lin(c: [u8; 4]) -> [f32; 4] {
    let a = c[3] as f32 / 255.0;
    [srgb_to_linear(c[0] as f32 / 255.0) * a, srgb_to_linear(c[1] as f32 / 255.0) * a, srgb_to_linear(c[2] as f32 / 255.0) * a, a]
}

/// Word-wrap one line to `max_w` pixels: byte ranges of `line`, split at spaces.
fn wrap(line: &str, ts: &TextStyle, max_w: f32) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let (mut start, mut end) = (0usize, 0usize);
    let mut pos = 0usize;
    for word in line.split(' ') {
        let (ws, we) = (pos, pos + word.len());
        pos = we + 1;
        if end == start && start == ws {
            end = we;
            continue;
        }
        let cand = line.get(start..we).unwrap_or("");
        if filmcraft_text::measure(cand, ts) > max_w {
            out.push(start..end);
            start = ws;
        }
        end = we;
    }
    out.push(start..end.min(line.len()));
    out
}

fn hex_colour(s: &str) -> Option<[u8; 4]> {
    let h = s.trim().strip_prefix('#')?;
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    match h.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

/// AYP: split a caption line into its drawn text (markup removed, entities decoded) and the byte
/// ranges coloured by `<c #rrggbb>…</c>`. A span left open continues on the next line (`open`);
/// `<c>` without a valid colour colours nothing. Other markup is dropped as before.
pub fn colour_spans(line: &str, open: &mut Option<[u8; 4]>) -> (String, Vec<(std::ops::Range<usize>, [u8; 4])>) {
    let mut segs: Vec<(String, Option<[u8; 4]>)> = vec![(String::new(), *open)];
    let mut rest = line;
    while let Some(i) = rest.find('<') {
        let (text, tail) = rest.split_at(i);
        if let Some(last) = segs.last_mut() {
            last.0.push_str(text);
        }
        let Some(j) = tail.find('>') else {
            if let Some(last) = segs.last_mut() {
                last.0.push_str(tail);
            }
            rest = "";
            break;
        };
        let tag = tail.get(1..j).unwrap_or("").trim();
        if tag.eq_ignore_ascii_case("/c") {
            *open = None;
            segs.push((String::new(), None));
        } else if tag == "c" || tag.starts_with("c ") || tag.starts_with("c#") {
            *open = hex_colour(tag.get(1..).unwrap_or(""));
            segs.push((String::new(), *open));
        } else {
            // any other tag (<i>, <b>…) is left to plain_text below
            if let Some(last) = segs.last_mut() {
                last.0.push_str(tail.get(..=j).unwrap_or(""));
            }
        }
        rest = tail.get(j + 1..).unwrap_or("");
    }
    if let Some(last) = segs.last_mut() {
        last.0.push_str(rest);
    }
    let mut plain = String::new();
    let mut spans = Vec::new();
    for (text, colour) in segs {
        let t = filmcraft_project::caption::plain_text(&text);
        let a = plain.len();
        plain.push_str(&t);
        if let (Some(c), true) = (colour, plain.len() > a) {
            spans.push((a..plain.len(), c));
        }
    }
    (plain, spans)
}

/// Placement overrides from WebVTT cue settings.
fn vtt_overrides(settings: &str) -> (Option<f32>, Option<CaptionAlign>) {
    let mut line = None;
    let mut align = None;
    for kv in settings.split_whitespace() {
        let Some((k, v)) = kv.split_once(':') else { continue };
        match k {
            "line" => {
                let v = v.split(',').next().unwrap_or("");
                if let Some(p) = v.strip_suffix('%').and_then(|p| p.parse::<f32>().ok()) {
                    line = Some((p / 100.0).clamp(0.0, 1.0));
                }
            }
            "align" => {
                align = match v {
                    "start" | "left" => Some(CaptionAlign::Left),
                    "end" | "right" => Some(CaptionAlign::Right),
                    "center" | "middle" => Some(CaptionAlign::Center),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    (line, align)
}

/// Lay out and rasterise one caption for a `w`×`h` frame, as it shows at its start.
pub fn render_caption(c: &Caption, style: &CaptionStyle, w: usize, h: usize) -> Option<Overlay> {
    render_caption_at(c, style, c.start, w, h)
}

/// AYP: the highlighted words of `c` at timeline time `t`: byte ranges in
/// `plain_text(&c.text)` and whether each is the word being said (the last one started).
fn highlighted(c: &Caption, style: &CaptionStyle, t: Tick) -> Vec<(Range<usize>, bool)> {
    if style.highlight == CaptionHighlight::None {
        return Vec::new();
    }
    let r = t - c.start;
    let active = c.words.iter().rposition(|w| w.start <= r);
    c.words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let is_active = active == Some(i);
            let on = match style.highlight {
                CaptionHighlight::Karaoke => w.start <= r,
                CaptionHighlight::ActiveWord => is_active,
                CaptionHighlight::None => false,
            };
            on.then(|| (w.range.clone(), is_active))
        })
        .collect()
}

/// AYP: one styled run of a drawn line: its bytes, its colour (None = the track colour) and
/// whether it is drawn at the highlight scale.
type Run = (Range<usize>, Option<[u8; 4]>, bool);

/// AYP: the runs of line bytes `a..b` (line-local), from its `<c>` colour spans (line-local) and
/// the highlighted words (caption-global, the line starting at byte `off`).
fn line_runs(a: usize, b: usize, off: usize, spans: &[(Range<usize>, [u8; 4])], hl: &[(Range<usize>, bool)], hl_color: [u8; 4], scaled: bool) -> Vec<Run> {
    let len = b.saturating_sub(a);
    let mut col: Vec<Option<[u8; 4]>> = vec![None; len];
    let mut big = vec![false; len];
    for (r, c) in spans {
        for k in r.start.max(a)..r.end.min(b) {
            if let Some(x) = col.get_mut(k - a) {
                *x = Some(*c);
            }
        }
    }
    for (g, active) in hl {
        let (x, y) = (g.start.saturating_sub(off).max(a), g.end.saturating_sub(off).min(b));
        for k in x..y {
            if let Some(v) = col.get_mut(k - a) {
                *v = Some(hl_color);
            }
            if let Some(v) = big.get_mut(k - a) {
                *v = *active && scaled;
            }
        }
    }
    let mut runs: Vec<Run> = Vec::new();
    for k in 0..len {
        let (c, g) = (col.get(k).copied().flatten(), big.get(k).copied().unwrap_or(false));
        if c.is_none() && !g {
            continue;
        }
        match runs.last_mut() {
            Some((r, rc, rg)) if r.end == k && *rc == c && *rg == g => r.end = k + 1,
            _ => runs.push((k..k + 1, c, g)),
        }
    }
    runs
}

/// Lay out and rasterise one caption for a `w`×`h` frame at timeline time `t` (AYP: `t` picks
/// the highlighted words).
pub fn render_caption_at(c: &Caption, style: &CaptionStyle, t: Tick, w: usize, h: usize) -> Option<Overlay> {
    if w == 0 || h == 0 {
        return None;
    }
    let scale = h as f32 / 1080.0;
    let px = (style.size * scale).max(4.0);
    let max_w = w as f32 * 0.9;
    let ts = caption_style(style, px);
    let hl_scale = if style.highlight_scale.is_finite() { style.highlight_scale.clamp(0.5, 3.0) } else { 1.0 };
    let big_ts = TextStyle { size: px * hl_scale, ..ts.clone() };
    let hl = highlighted(c, style, t);
    // (speaker names are metadata; like Premiere, they are not burned in)
    // AYP: each line keeps its `<c #rrggbb>` colour spans and highlighted words through wrapping.
    let mut lines: Vec<String> = Vec::new();
    let mut line_runs_all: Vec<Vec<Run>> = Vec::new();
    let mut open = None;
    let mut off = 0usize;
    for raw in c.text.lines() {
        let (plain, spans) = colour_spans(raw, &mut open);
        let lead = plain.len() - plain.trim_start().len();
        let trimmed = plain.trim();
        for r in wrap(trimmed, &ts, max_w) {
            let (a, b) = (r.start + lead, r.end + lead);
            let Some(text) = plain.get(a..b) else { continue };
            if text.trim().is_empty() {
                continue;
            }
            let runs = line_runs(a, b, off, &spans, &hl, style.highlight_color, hl_scale != 1.0);
            lines.push(text.to_string());
            line_runs_all.push(runs.into_iter().map(|(r, c, g)| ((r.start - a)..(r.end - a), c, g)).collect());
        }
        off += plain.len() + 1;
    }
    if lines.is_empty() {
        return None;
    }
    let layouts: Vec<_> = lines
        .iter()
        .zip(&line_runs_all)
        .map(|(line, runs)| {
            let sr: Vec<StyleRun> = runs.iter().map(|(r, _, g)| StyleRun { range: r.clone(), style: if *g { big_ts.clone() } else { ts.clone() } }).collect();
            layout_rich(line, &ts, &sr, &ParagraphStyle::default())
        })
        .collect();
    let vm = filmcraft_text::fonts::face(filmcraft_text::resolve(&ts.family, &ts.style).face).metrics(px);
    let (asc, desc) = (vm.ascent, vm.descent);
    let lh = px * style.line_spacing.max(0.8);
    let pad_v = if style.background_padding.is_finite() { (style.background_padding.clamp(0.0, 200.0) * scale).round() } else { 0.0 };
    let pad_x = (px * 0.3).round() + pad_v;
    let block_h = lh * lines.len() as f32;
    let (line_override, align_override) = vtt_overrides(&c.settings);
    let align = align_override.unwrap_or(style.align);
    let margin = style.margin.clamp(0.0, 0.45) * h as f32;
    let top = match (line_override, style.anchor) {
        (Some(f), _) => (f * h as f32).min(h as f32 - block_h),
        (None, CaptionAnchor::Top) => margin,
        (None, CaptionAnchor::Middle) => (h as f32 - block_h) / 2.0,
        (None, CaptionAnchor::Bottom) => h as f32 - margin - block_h,
    }
    .max(0.0)
    .round();
    let side = w as f32 * 0.05;
    let widths: Vec<f32> = layouts.iter().map(|l| l.lines.iter().map(|x| x.width).fold(0.0, f32::max)).collect();
    let xs: Vec<f32> = widths
        .iter()
        .map(|&lw| match align {
            CaptionAlign::Left => side + pad_x,
            CaptionAlign::Right => w as f32 - side - pad_x - lw,
            CaptionAlign::Center => (w as f32 - lw) / 2.0,
        })
        .collect();
    let outline = (style.outline * scale).max(0.0);
    // AYP: drop shadow offset and softness; the overlay grows to hold it
    let finite = |v: f32, hi: f32| if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 };
    let (s_dist, s_soft) = (finite(style.shadow_distance, 200.0) * scale, finite(style.shadow_smoothing, 100.0) * scale);
    let s_angle = if style.shadow_angle.is_finite() { style.shadow_angle.to_radians() } else { 0.0 };
    let (s_dx, s_dy) = (s_angle.cos() * s_dist, -s_angle.sin() * s_dist);
    let shadow_pad = if style.shadow { (s_dist + s_soft).ceil() + 1.0 } else { 0.0 };
    let grow = outline.ceil() + 1.0 + shadow_pad;
    // a bigger highlighted word may rise above its line box
    let vgrow = grow + ((hl_scale - 1.0).max(0.0) * asc).ceil();
    let bx0 = xs.iter().zip(&widths).map(|(x, _)| x - pad_x - grow).fold(f32::MAX, f32::min).floor();
    let bx1 = xs.iter().zip(&widths).map(|(x, lw)| x + lw + pad_x + grow).fold(f32::MIN, f32::max).ceil();
    let by0 = (top - vgrow - pad_v).floor();
    let by1 = (top + block_h + vgrow + pad_v).ceil();
    let (ox, oy) = (bx0 as i32, by0 as i32);
    let ow = (bx1 - bx0).max(1.0) as usize;
    let oh = (by1 - by0).max(1.0) as usize;
    let mut cover = vec![0.0f32; ow * oh];
    let mut coloured: Vec<([u8; 4], Vec<f32>)> = Vec::new();
    let mut bg = vec![0.0f32; ow * oh];
    for (i, l) in layouts.iter().enumerate() {
        let lt = top + lh * i as f32;
        if style.background && style.background_color[3] > 0 && !style.background_block {
            let r = [xs[i] - pad_x, lt - pad_v, xs[i] + widths[i] + pad_x, lt + lh + pad_v];
            fill_round_rect(&mut bg, (ox, oy, ow, oh), r, style.background_round_radius);
        }
        let baseline = lt + (lh - (asc + desc)) / 2.0 + asc;
        let at = render::at(xs[i], baseline.round());
        let runs = line_runs_all.get(i).map(Vec::as_slice).unwrap_or(&[]);
        // the track colour: the base run and runs that are only scaled
        let mut m = filmcraft_text::Mask { w: ow, h: oh, a: std::mem::take(&mut cover) };
        if runs.is_empty() {
            render::draw(l, &at, &mut m, (ox, oy));
        } else {
            render::draw_run(l, &at, &mut m, (ox, oy), Some(0));
            for (k, _) in runs.iter().enumerate().filter(|(_, r)| r.1.is_none()) {
                render::draw_run(l, &at, &mut m, (ox, oy), u16::try_from(k + 1).ok());
            }
        }
        cover = m.a;
        for (k, (_, col, _)) in runs.iter().enumerate() {
            let Some(col) = col else { continue };
            let idx = match coloured.iter().position(|(c, _)| c == col) {
                Some(p) => p,
                None => {
                    coloured.push((*col, vec![0.0; ow * oh]));
                    coloured.len() - 1
                }
            };
            let Some((_, mask)) = coloured.get_mut(idx) else { continue };
            let mut m = filmcraft_text::Mask { w: ow, h: oh, a: std::mem::take(mask) };
            render::draw_run(l, &at, &mut m, (ox, oy), u16::try_from(k + 1).ok());
            *mask = m.a;
        }
    }
    // AYP: one box around the whole caption
    if style.background && style.background_color[3] > 0 && style.background_block {
        let x0 = xs.iter().fold(f32::MAX, |a, x| a.min(*x)) - pad_x;
        let x1 = xs.iter().zip(&widths).fold(f32::MIN, |a, (x, lw)| a.max(x + lw)) + pad_x;
        fill_round_rect(&mut bg, (ox, oy, ow, oh), [x0, top - pad_v, x1, top + block_h + pad_v], style.background_round_radius);
    }
    // the outline goes around every glyph, coloured or not
    let union: Vec<f32> =
        if coloured.is_empty() { Vec::new() } else { (0..ow * oh).map(|j| coloured.iter().fold(cover[j], |a, (_, m)| a.max(m[j]))).collect() };
    let stroke = if outline > 0.0 { dilate(if union.is_empty() { &cover } else { &union }, ow, oh, outline) } else { Vec::new() };
    // AYP: the shadow is everything drawn (box, outline, text), moved and softened
    let shadow = if style.shadow && style.shadow_color[3] > 0 {
        let a: Vec<f32> = (0..ow * oh)
            .map(|j| {
                let mut v = cover[j].max(bg[j]).max(union.get(j).copied().unwrap_or(0.0));
                if let Some(x) = stroke.get(j) {
                    v = v.max(*x);
                }
                v
            })
            .collect();
        let m = filmcraft_text::mask::offset(&filmcraft_text::Mask { w: ow, h: oh, a }, s_dx.round() as i32, s_dy.round() as i32);
        filmcraft_text::mask::blur(&m, s_soft / 3.0).a
    } else {
        Vec::new()
    };
    let (tc, bc, oc) = (lin(style.color), lin(style.background_color), lin(style.outline_color));
    let sc = lin(style.shadow_color);
    let mut out = vec![0.0f32; ow * oh * 4];
    for j in 0..ow * oh {
        let mut p = [0.0f32; 4];
        let over = |p: &mut [f32; 4], c: [f32; 4], a: f32| {
            if a > 0.0 {
                for k in 0..4 {
                    p[k] = c[k] * a + p[k] * (1.0 - c[3] * a);
                }
            }
        };
        if let Some(a) = shadow.get(j) {
            over(&mut p, sc, *a);
        }
        over(&mut p, bc, bg[j]);
        if !stroke.is_empty() {
            over(&mut p, oc, stroke[j]);
        }
        over(&mut p, tc, cover[j]);
        for (col, m) in &coloured {
            over(&mut p, lin(*col), m[j]);
        }
        out[j * 4..j * 4 + 4].copy_from_slice(&p);
    }
    Some(Overlay { x: ox, y: oy, w: ow, h: oh, px: out })
}

/// AYP: add the antialiased coverage of rectangle `r` (`[x0, y0, x1, y1]` in frame pixels) with
/// corners rounded by `round` (0 square … 1 pill) into `mask`, an `ow`×`oh` area at `(ox, oy)`.
fn fill_round_rect(mask: &mut [f32], (ox, oy, ow, oh): (i32, i32, usize, usize), r: [f32; 4], round: f32) {
    let (hx, hy) = ((r[2] - r[0]) / 2.0, (r[3] - r[1]) / 2.0);
    if !(hx > 0.0 && hy > 0.0) {
        return;
    }
    let (cx, cy) = (r[0] + hx, r[1] + hy);
    let rad = if round.is_finite() { round.clamp(0.0, 1.0) } else { 0.0 } * hx.min(hy);
    for y in (r[1].floor() as i32).max(oy)..(r[3].ceil() as i32).min(oy + oh as i32) {
        for x in (r[0].floor() as i32).max(ox)..(r[2].ceil() as i32).min(ox + ow as i32) {
            // signed distance from the pixel centre to the rounded rectangle
            let (qx, qy) = ((x as f32 + 0.5 - cx).abs() - (hx - rad), (y as f32 + 0.5 - cy).abs() - (hy - rad));
            let d = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - rad;
            let a = (0.5 - d).clamp(0.0, 1.0);
            if let Some(v) = mask.get_mut((y - oy) as usize * ow + (x - ox) as usize) {
                *v = v.max(a);
            }
        }
    }
}

/// Max-filter a coverage mask with a disc of radius `r`.
fn dilate(m: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    let ri = r.ceil() as i32;
    let mut offs = Vec::new();
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d <= r + 0.5 {
                offs.push((dx, dy, (r + 0.5 - d).clamp(0.0, 1.0)));
            }
        }
    }
    let mut out = vec![0.0f32; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut v = 0.0f32;
            for &(dx, dy, wgt) in &offs {
                let (sx, sy) = (x + dx, y + dy);
                if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                    v = v.max(m[sy as usize * w + sx as usize] * wgt);
                }
            }
            out[y as usize * w + x as usize] = v;
        }
    }
    out
}

/// The overlay for a caption track at timeline time `t` (None when nothing shows).
pub fn track_overlay(track: &CaptionTrack, t: Tick, w: usize, h: usize) -> Option<Overlay> {
    if !track.enabled {
        return None;
    }
    let c = track.caption_at(t)?;
    render_caption_at(c, &track.style, t, w, h)
}

/// Overlays of every visible caption track of a sequence at `t`, bottom track first.
pub fn sequence_overlays(seq: &Sequence, t: Tick, w: usize, h: usize) -> Vec<Overlay> {
    seq.caption_tracks.iter().rev().filter_map(|tr| track_overlay(tr, t, w, h)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_project::{CaptionFormat, CaptionHighlight, ClipId, TrackId};

    fn cap(text: &str) -> Caption {
        Caption {
            id: ClipId(1),
            start: Tick(0),
            duration: Tick(100),
            text: text.into(),
            speaker: None,
            cue_id: None,
            settings: String::new(),
            words: Vec::new(),
        }
    }

    #[test]
    fn bottom_centre_with_box() {
        let st = CaptionStyle::default();
        let o = render_caption(&cap("Hello world"), &st, 1920, 1080).unwrap();
        // centred horizontally, in the lower part of the frame
        let cx = o.x as f32 + o.w as f32 / 2.0;
        assert!((cx - 960.0).abs() < 4.0, "centre {cx}");
        assert!(o.y > 800 && (o.y as usize + o.h) < 1080, "y {} h {}", o.y, o.h);
        // white text pixels exist and the box is dark translucent
        let mut white = 0;
        let mut boxy = 0;
        for p in o.px.chunks(4) {
            if p[3] > 0.99 && p[0] > 0.9 {
                white += 1;
            } else if p[3] > 0.7 && p[0] < 0.01 {
                boxy += 1;
            }
        }
        assert!(white > 500, "{white}");
        assert!(boxy > 1000, "{boxy}");
    }

    #[test]
    fn top_anchor_and_vtt_line() {
        let st = CaptionStyle { anchor: CaptionAnchor::Top, ..Default::default() };
        let o = render_caption(&cap("Top"), &st, 1280, 720).unwrap();
        assert!(o.y < 100);
        let mut c = cap("Line");
        c.settings = "line:50% align:start".into();
        let o = render_caption(&c, &CaptionStyle::default(), 1280, 720).unwrap();
        assert!((o.y - 360).abs() < 4, "{}", o.y);
        assert!(o.x < 120, "{}", o.x);
    }

    #[test]
    fn wraps_and_composites() {
        let long = "word ".repeat(60);
        let o = render_caption(&cap(&long), &CaptionStyle::default(), 640, 360).unwrap();
        assert!(o.w <= 640 && o.h > 40, "{}x{}", o.w, o.h);
        let mut canvas = vec![0.0f32; 640 * 360 * 4];
        o.composite_onto(&mut canvas, 640, 360);
        assert!(canvas.chunks(4).any(|p| p[0] > 0.9));
    }

    #[test]
    fn outline_draws_around_text() {
        let st = CaptionStyle { background: false, outline: 4.0, outline_color: [255, 0, 0, 255], ..Default::default() };
        let o = render_caption(&cap("O"), &st, 1920, 1080).unwrap();
        assert!(o.px.chunks(4).any(|p| p[0] > 0.9 && p[1] < 0.05 && p[3] > 0.9), "red outline pixels");
    }

    #[test]
    fn track_font_is_used() {
        // the style's font used to be ignored (always Inter)
        let inter = render_caption(&cap("Hello world"), &CaptionStyle::default(), 1920, 1080).unwrap();
        let serif = CaptionStyle { font: "Noto Serif".into(), ..Default::default() };
        let serif = render_caption(&cap("Hello world"), &serif, 1920, 1080).unwrap();
        assert_ne!((inter.w, &inter.px), (serif.w, &serif.px), "Noto Serif renders like Inter");
        let mono = CaptionStyle { font: "jetbrains mono".into(), ..Default::default() };
        let mono = render_caption(&cap("Hello world"), &mono, 1920, 1080).unwrap();
        assert_ne!((inter.w, &inter.px), (mono.w, &mono.px), "JetBrains Mono renders like Inter");
    }

    #[test]
    fn unknown_or_empty_font_falls_back_to_inter() {
        let inter = render_caption(&cap("Hello"), &CaptionStyle::default(), 1280, 720).unwrap();
        for font in ["", "  ", "No Such Font 123"] {
            let st = CaptionStyle { font: font.into(), ..Default::default() };
            assert_eq!(render_caption(&cap("Hello"), &st, 1280, 720), Some(inter.clone()), "font {font:?}");
        }
    }

    /// AYP: `<c #rrggbb>…</c>` colours words (keywords) inside a caption.
    #[test]
    fn colour_spans_parse_and_carry_over_lines() {
        let mut open = None;
        let (plain, spans) = colour_spans("Mình nợ <c #ffd200>10 triệu</c> rồi", &mut open);
        assert_eq!(plain, "Mình nợ 10 triệu rồi");
        assert_eq!(spans, vec![(plain.find("10").unwrap()..plain.find(" rồi").unwrap(), [0xff, 0xd2, 0x00, 0xff])]);
        assert!(open.is_none());
        let (p1, s1) = colour_spans("a <c #ff0000>b", &mut open);
        let (p2, s2) = colour_spans("c</c> d <i>e</i>", &mut open);
        assert_eq!((p1.as_str(), s1), ("a b", vec![(2..3, [255, 0, 0, 255])]));
        assert_eq!((p2.as_str(), s2), ("c d e", vec![(0..1, [255, 0, 0, 255])]));
        // a malformed colour falls back to the track colour (no span), never a panic
        let (p3, s3) = colour_spans("<c #zz>x</c> <c>y</c>", &mut None);
        assert_eq!((p3.as_str(), s3.len()), ("x y", 0));
    }

    #[test]
    fn a_whole_coloured_line_keeps_its_colour() {
        let st = CaptionStyle { background: false, size: 120.0, ..Default::default() };
        let o = render_caption(&cap("WWWW\n<c #ff0000>RRRR RRRR</c>"), &st, 1280, 720).unwrap();
        let red = o.px.chunks(4).filter(|p| p[3] > 0.9 && p[0] > 0.5 && p[1] < 0.1).count();
        assert!(red > 200, "red {red}");
    }

    #[test]
    fn coloured_words_are_drawn_in_their_colour() {
        let st = CaptionStyle { background: false, size: 120.0, ..Default::default() };
        let o = render_caption(&cap("<c #ff0000>RRRR</c> WWWW"), &st, 1280, 720).unwrap();
        let (mut red, mut white) = (0, 0);
        for p in o.px.chunks(4) {
            if p[3] > 0.9 {
                if p[0] > 0.5 && p[1] < 0.1 {
                    red += 1
                }
                if p[0] > 0.5 && p[1] > 0.5 && p[2] > 0.5 {
                    white += 1
                }
            }
        }
        assert!(red > 200 && white > 200, "red {red} white {white}");
    }

    /// AYP: "AAAA" heard 0..50, "BBBB" heard 50..100 (relative to the caption start 0).
    fn timed_cap() -> Caption {
        let mut c = cap("AAAA BBBB");
        c.words = filmcraft_project::align_words("AAAA BBBB", &[("AAAA", Tick(0), Tick(50)), ("BBBB", Tick(50), Tick(100))]);
        c
    }

    /// Red pixels in the left and right halves of an overlay.
    fn red_halves(o: &Overlay) -> (usize, usize) {
        let (mut l, mut r) = (0, 0);
        for (i, p) in o.px.chunks(4).enumerate() {
            if p[3] > 0.9 && p[0] > 0.5 && p[1] < 0.1 {
                if i % o.w < o.w / 2 { l += 1 } else { r += 1 }
            }
        }
        (l, r)
    }

    #[test]
    fn karaoke_colours_every_word_heard_so_far() {
        let st = CaptionStyle { background: false, size: 120.0, highlight: CaptionHighlight::Karaoke, highlight_color: [255, 0, 0, 255], ..Default::default() };
        let (l, r) = red_halves(&render_caption_at(&timed_cap(), &st, Tick(10), 1280, 720).unwrap());
        assert!(l > 200 && r == 0, "{l} {r}");
        let (l, r) = red_halves(&render_caption_at(&timed_cap(), &st, Tick(60), 1280, 720).unwrap());
        assert!(l > 200 && r > 200, "{l} {r}");
    }

    #[test]
    fn active_word_colours_only_the_word_being_said() {
        let st =
            CaptionStyle { background: false, size: 120.0, highlight: CaptionHighlight::ActiveWord, highlight_color: [255, 0, 0, 255], ..Default::default() };
        let (l, r) = red_halves(&render_caption_at(&timed_cap(), &st, Tick(60), 1280, 720).unwrap());
        assert!(l == 0 && r > 200, "{l} {r}");
        // no highlight: never red; untimed captions draw as before
        let none = CaptionStyle { highlight: CaptionHighlight::None, ..st.clone() };
        assert_eq!(red_halves(&render_caption_at(&timed_cap(), &none, Tick(60), 1280, 720).unwrap()), (0, 0));
        assert_eq!(render_caption_at(&cap("AAAA BBBB"), &st, Tick(60), 1280, 720), render_caption(&cap("AAAA BBBB"), &none, 1280, 720));
        // a bigger active word makes the line wider
        let big = CaptionStyle { highlight_scale: 1.5, ..st.clone() };
        let (a, b) = (render_caption_at(&timed_cap(), &st, Tick(60), 1280, 720).unwrap(), render_caption_at(&timed_cap(), &big, Tick(60), 1280, 720).unwrap());
        assert!(b.w > a.w + 20, "{} {}", a.w, b.w);
    }

    /// Centroid of the pixels matching `f`.
    fn centroid(o: &Overlay, f: impl Fn(&[f32]) -> bool) -> (f32, f32, usize) {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0usize);
        for (i, p) in o.px.chunks(4).enumerate() {
            if f(p) {
                sx += (i % o.w) as f32;
                sy += (i / o.w) as f32;
                n += 1;
            }
        }
        (sx / n.max(1) as f32, sy / n.max(1) as f32, n)
    }

    /// AYP: a drop shadow (CapCut shadow_*) sits down and to the right of the text at -45°.
    #[test]
    fn drop_shadow_falls_down_right() {
        let plain = CaptionStyle { background: false, size: 120.0, ..Default::default() };
        let st =
            CaptionStyle { shadow: true, shadow_color: [255, 0, 0, 255], shadow_angle: -45.0, shadow_distance: 12.0, shadow_smoothing: 0.0, ..plain.clone() };
        let red = |p: &[f32]| p[3] > 0.9 && p[0] > 0.5 && p[1] < 0.1;
        let white = |p: &[f32]| p[3] > 0.9 && p[0] > 0.9 && p[1] > 0.9;
        assert_eq!(centroid(&render_caption(&cap("Shadow"), &plain, 1920, 1080).unwrap(), red).2, 0);
        let o = render_caption(&cap("Shadow"), &st, 1920, 1080).unwrap();
        let (rx, ry, rn) = centroid(&o, red);
        let (wx, wy, _) = centroid(&o, white);
        assert!(rn > 200 && rx > wx + 3.0 && ry > wy + 3.0, "red {rx},{ry} ({rn}) white {wx},{wy}");
        // a soft shadow spreads (partly transparent pixels) and stays inside the overlay
        let soft = CaptionStyle { shadow_smoothing: 10.0, ..st };
        let o = render_caption(&cap("Shadow"), &soft, 1920, 1080).unwrap();
        assert!(o.px.chunks(4).any(|p| p[3] > 0.05 && p[3] < 0.5 && p[0] > p[1]));
        let edge = |x: usize, y: usize| o.px[(y * o.w + x) * 4 + 3];
        assert!((0..o.h).all(|y| edge(o.w - 1, y) < 0.05), "shadow cut at the right edge");
    }

    /// AYP: rounded box corners (CapCut background_round_radius), inner padding and one box
    /// around the whole block instead of one per line.
    #[test]
    fn rounded_padded_block_box() {
        let square = CaptionStyle { background_color: [0, 0, 0, 255], ..Default::default() };
        let round = CaptionStyle { background_round_radius: 1.0, ..square.clone() };
        let (a, b) = (render_caption(&cap("Box"), &square, 1920, 1080).unwrap(), render_caption(&cap("Box"), &round, 1920, 1080).unwrap());
        assert_eq!((a.x, a.y, a.w, a.h), (b.x, b.y, b.w, b.h));
        // the box's top-left corner
        let (mut cx, mut cy) = (usize::MAX, usize::MAX);
        for (i, p) in a.px.chunks(4).enumerate() {
            if p[3] > 0.5 {
                cx = cx.min(i % a.w);
                cy = cy.min(i / a.w);
            }
        }
        let alpha = |o: &Overlay| o.px[((cy + 1) * o.w + cx + 1) * 4 + 3];
        assert!(alpha(&a) > 0.9 && alpha(&b) < 0.1, "square {} round {}", alpha(&a), alpha(&b));
        let padded = CaptionStyle { background_padding: 20.0, ..square.clone() };
        let c = render_caption(&cap("Box"), &padded, 1920, 1080).unwrap();
        assert!(c.h >= a.h + 38 && c.w >= a.w + 38, "{}x{} vs {}x{}", c.w, c.h, a.w, a.h);
        let boxed = |o: &Overlay| o.px.chunks(4).filter(|p| p[3] > 0.9 && p[0] < 0.01).count();
        let text = "WWWWWWWWWW\nW";
        let lines = render_caption(&cap(text), &square, 1920, 1080).unwrap();
        let block = render_caption(&cap(text), &CaptionStyle { background_block: true, ..square }, 1920, 1080).unwrap();
        assert!(boxed(&block) > boxed(&lines) * 13 / 10, "{} {}", boxed(&block), boxed(&lines));
    }

    /// AYP: font weight (CapCut font style) and letter spacing.
    #[test]
    fn font_weight_and_tracking() {
        let semi = render_caption(&cap("Hello"), &CaptionStyle::default(), 1280, 720).unwrap();
        let bold = render_caption(&cap("Hello"), &CaptionStyle { font_style: "Bold".into(), ..Default::default() }, 1280, 720).unwrap();
        let ink = |o: &Overlay| o.px.chunks(4).map(|p| p[0]).sum::<f32>();
        assert!(ink(&bold) > ink(&semi) * 1.03, "{} {}", ink(&bold), ink(&semi));
        // an empty weight is the default SemiBold; an unknown one never fails
        assert_eq!(render_caption(&cap("Hello"), &CaptionStyle { font_style: " ".into(), ..Default::default() }, 1280, 720).unwrap(), semi);
        assert!(render_caption(&cap("Hello"), &CaptionStyle { font_style: "Ultra 900".into(), ..Default::default() }, 1280, 720).is_some());
        let wide = render_caption(&cap("Hello"), &CaptionStyle { tracking: 200.0, ..Default::default() }, 1280, 720).unwrap();
        assert!(wide.w > semi.w + 20, "{} {}", wide.w, semi.w);
    }

    #[test]
    fn hidden_track_draws_nothing() {
        let mut t = CaptionTrack::new(TrackId(1), "S".into(), CaptionFormat::Subtitle);
        t.captions.push(cap("x"));
        assert!(track_overlay(&t, Tick(5), 100, 100).is_some());
        t.enabled = false;
        assert!(track_overlay(&t, Tick(5), 100, 100).is_none());
    }
}
