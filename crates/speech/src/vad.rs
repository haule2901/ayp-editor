//! Energy-based tightening of word bounds.
//!
//! Attention alignment places every word boundary on the next word's start, so silence between
//! words is absorbed into the word before it (and leading silence into the first word). Pause
//! detection and text-based editing need the silences, so each word's start and end are moved
//! inward past 10 ms frames whose level is below a threshold set between the clip's noise floor
//! (10th percentile of the levels of frames above −100 dBFS) and its speech level (95th percentile): 30 % of the way up,
//! and at least 6 dB above the floor. A word keeps at least 60 ms.

use filmcraft_project::Word;

use crate::{SAMPLE_RATE, TICKS_PER_SAMPLE};

const FRAME: usize = (SAMPLE_RATE / 100) as usize;
const MIN_WORD_FRAMES: usize = 6;

/// Frame levels in dBFS (10 ms RMS).
pub fn frame_db(audio: &[f32]) -> Vec<f32> {
    audio
        .chunks(FRAME)
        .map(|c| {
            let e = c.iter().map(|v| v * v).sum::<f32>() / c.len().max(1) as f32;
            10.0 * (e + 1e-12).log10()
        })
        .collect()
}

/// The speech/silence threshold in dBFS.
pub fn threshold(db: &[f32]) -> f32 {
    if db.is_empty() {
        return -60.0;
    }
    // digital silence (< −100 dBFS) would drag the floor far below the recording's own noise
    let mut s: Vec<f32> = db.iter().copied().filter(|v| *v > -100.0).collect();
    if s.is_empty() {
        return -60.0;
    }
    s.sort_by(f32::total_cmp);
    let q = |p: f32| s[((s.len() - 1) as f32 * p) as usize];
    let (floor, speech) = (q(0.10), q(0.95));
    (floor + 0.3 * (speech - floor)).max(floor + 6.0)
}

/// Number of 10 ms frames in `[from, to)` louder than `thr` (frames past the end are ignored).
pub fn speech_frames(db: &[f32], thr: f32, from: usize, to: usize) -> usize {
    db.get(from.min(db.len())..to.min(db.len())).map_or(0, |f| f.iter().filter(|v| **v >= thr).count())
}

/// AYP: where the next Whisper window starts when this one ended with a lone timestamp at
/// `last_ts` (frames into the window). Whisper takes that as "no more speech here" and skips the
/// rest of the window, but large-v3-turbo also stops early mid-speech, losing up to 27 s. When at
/// least 0.3 s of speech (`speech(from, to)`, frames into the window) follows the stop, resume at
/// the stop instead. Stops in the first 0.5 s keep the normal advance, so the loop always moves.
pub fn resume_after_early_end(advance: usize, last_ts: Option<usize>, seg_frames: usize, speech: impl Fn(usize, usize) -> usize) -> usize {
    match last_ts {
        Some(ts) if ts >= 50 && ts < advance && speech(ts, seg_frames) >= 30 => ts,
        _ => advance,
    }
}

/// Move word bounds inward past silent frames (see the module docs).
pub fn tighten_words(audio: &[f32], words: &mut [Word]) {
    let db = frame_db(audio);
    if db.is_empty() {
        return;
    }
    let th = threshold(&db);
    let frame_ticks = TICKS_PER_SAMPLE * FRAME as i64;
    for w in words {
        let a = (w.start.0 / frame_ticks).max(0) as usize;
        let b = (((w.end.0 + frame_ticks - 1) / frame_ticks).max(0) as usize).min(db.len());
        if b <= a + MIN_WORD_FRAMES {
            continue;
        }
        let mut s = a;
        while s + MIN_WORD_FRAMES < b && db[s] < th {
            s += 1;
        }
        let mut e = b;
        while e > s + MIN_WORD_FRAMES && db[e - 1] < th {
            e -= 1;
        }
        // nothing voiced at all: leave the word alone
        if (s..e).all(|f| db[f] < th) {
            continue;
        }
        if s > a {
            w.start = filmcraft_time::Tick(s as i64 * frame_ticks);
        }
        if e < b {
            w.end = filmcraft_time::Tick(e as i64 * frame_ticks).max(w.start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seconds_tick;

    #[test]
    fn silence_is_trimmed_from_words() {
        // 0.0–0.5 silence, 0.5–1.0 tone, 1.0–2.0 silence, 2.0–2.5 tone
        let mut audio: Vec<f32> = (0..40_000).map(|i| if i % 2 == 0 { 0.0001 } else { 0.0 }).collect();
        for i in (8_000..16_000).chain(32_000..40_000) {
            audio[i] = 0.3 * (i as f32 * 0.2).sin();
        }
        let mut w = vec![Word::new("a", seconds_tick(0.0), seconds_tick(2.0)), Word::new("b", seconds_tick(2.0), seconds_tick(2.5))];
        tighten_words(&audio, &mut w);
        assert_eq!(w[0].start, seconds_tick(0.5));
        assert_eq!(w[0].end, seconds_tick(1.0));
        assert_eq!((w[1].start, w[1].end), (seconds_tick(2.0), seconds_tick(2.5)));
    }

    /// AYP: Whisper (large-v3-turbo especially) sometimes ends a 30 s window early with a lone
    /// timestamp while speech goes on; skipping the whole window then loses up to 27 s of words.
    #[test]
    fn early_end_resumes_where_speech_continues() {
        // 10 ms frames: silence, then speech from 3 s to 30 s
        let mut db = vec![-70.0f32; 3000];
        db[300..3000].iter_mut().for_each(|v| *v = -20.0);
        let thr = threshold(&db);
        // the model stopped at 2.9 s (frame 290) of a full window: resume there
        assert_eq!(resume_after_early_end(3000, Some(290), 3000, |a, b| speech_frames(&db, thr, a, b)), 290);
        // nothing but silence after the last timestamp: keep skipping the window
        let quiet = vec![-70.0f32; 3000];
        assert_eq!(resume_after_early_end(3000, Some(290), 3000, |a, b| speech_frames(&quiet, thr, a, b)), 3000);
        // a stop in the first half second never stalls the loop
        assert_eq!(resume_after_early_end(3000, Some(20), 3000, |a, b| speech_frames(&db, thr, a, b)), 3000);
        // out-of-range frames are ignored, not a panic
        assert_eq!(speech_frames(&db, thr, 2990, 99_999), 10);
    }
}
