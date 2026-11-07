//! Text-based editing commands (`transcript.*`): the Text panel ▸ Transcript tab.
//!
//! Transcripts belong to media items (`Project::transcripts`, media time); the sequence transcript
//! is derived from them ([`filmcraft_edit::transcript::sequence_words`]). Words of the sequence
//! transcript are addressed by index (`from`, `to`, inclusive), as `transcript.inspect` lists them.
//!
//! Speech recognition goes through a [`Transcriber`]: [`Session::transcriber`] when a host or a
//! test installed one, else the Whisper model named by `model` from `<data dir>/models` (needs the
//! engine feature `whisper`; without it `transcript.generate` fails with a clear error, and agents
//! can still bring their own transcript with `transcript.set`).

use std::sync::Arc;

use serde_json::{Value, json};

use filmcraft_edit as edit;
use filmcraft_edit::transcript::{self as tx, CaptionRules, SeqWord};
use filmcraft_project::{CaptionFormat, CaptionStyle, CaptionTrack, ItemId, ItemKind, TrackId, Transcript};
use filmcraft_speech::{Options, SpeechError, Transcriber};
use filmcraft_time::{TICKS_PER_SECOND, Tick, TimeRange};

use crate::commands::{CommandSpec, always, bad, bool_p, f64_p, has_seq, str_p, u64_p};
use crate::{EngineError, Result, Session};

type Run = fn(&mut Session, &Value) -> Result<Value>;
type Enabled = fn(&Session) -> std::result::Result<(), String>;

fn spec(id: &'static str, label: &'static str, menu: &'static [&'static str], params: &'static str, enabled: Enabled, run: Run, journal: bool) -> CommandSpec {
    CommandSpec { id, label, menu, shortcut: None, params, enabled, run, journal }
}

/// Where downloaded speech models live (`<data dir>/models`).
pub fn models_dir() -> Option<std::path::PathBuf> {
    crate::autosave::default_data_dir().map(|d| d.join("models"))
}

/// Whether this build can transcribe with Whisper (feature `whisper`).
pub fn speech_available() -> bool {
    filmcraft_speech::available()
}

/// The words of the active sequence's transcript.
pub fn sequence_words(s: &Session) -> Vec<SeqWord> {
    match s.active_sequence() {
        Some(q) => tx::sequence_words(q, &s.project.transcripts),
        None => Vec::new(),
    }
}

fn has_transcript(s: &Session) -> std::result::Result<(), String> {
    has_seq(s)?;
    if sequence_words(s).is_empty() { Err("the sequence has no transcript (Transcribe first)".into()) } else { Ok(()) }
}

fn has_transcripts(s: &Session) -> std::result::Result<(), String> {
    if s.project.transcripts.is_empty() { Err("there are no transcripts".into()) } else { Ok(()) }
}

/// Why speech-to-text can't run in this build (no installed transcriber, built without `whisper`).
pub(crate) const NO_SPEECH: &str =
    "speech-to-text is not available in this build (built without the `whisper` feature); import a transcript with transcript.set instead";

/// `transcript.generate` can run: a host installed a transcriber or the build has speech-to-text
/// (#97: it reported enabled and then always failed).
pub(crate) fn can_transcribe(s: &Session) -> std::result::Result<(), String> {
    if s.transcriber.is_some() || speech_available() { Ok(()) } else { Err(NO_SPEECH.into()) }
}

/// `transcript.downloadModel` can run: built with `speech-download` (#98).
fn can_download(_: &Session) -> std::result::Result<(), String> {
    if cfg!(feature = "speech-download") {
        Ok(())
    } else {
        Err("model downloads are not available in this build (built without the `speech-download` feature)".into())
    }
}

/// The media item behind a project item (subclips resolve to their parent).
fn media_item(s: &Session, item: ItemId) -> Option<ItemId> {
    s.project.resolve_media(item).map(|(root, _, _)| root)
}

fn ids_p(p: &Value, k: &str) -> Option<Vec<ItemId>> {
    p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(ItemId).collect())
}

/// Items to transcribe: `items` / `item`, else the Project panel selection, else the media of the
/// active sequence's enabled audio clips. Subclips resolve to their media; duplicates are removed.
fn targets(s: &Session, p: &Value) -> Vec<ItemId> {
    let mut raw = ids_p(p, "items").or_else(|| u64_p(p, "item").map(|i| vec![ItemId(i)])).unwrap_or_default();
    if raw.is_empty() {
        raw = s.state.project_selection.clone();
    }
    if raw.is_empty()
        && let Some(q) = s.active_sequence()
    {
        raw = q.audio_tracks.iter().flat_map(|t| t.items.iter()).filter(|it| it.enabled).map(|it| it.item).collect();
    }
    let mut out = Vec::new();
    for i in raw {
        if let Some(m) = media_item(s, i)
            && !out.contains(&m)
        {
            out.push(m);
        }
    }
    out
}

/// Mono 16 kHz audio of a media item (None: no audio).
fn item_audio(s: &Session, item: ItemId) -> Option<Vec<f32>> {
    let dur = match &s.project.item(item)?.kind {
        ItemKind::Media(m) => m.duration(),
        _ => return None,
    };
    let src = s.source(item)?;
    if !src.info().has_audio() {
        return None;
    }
    audio_of(&src, dur)
}

/// Mono 16 kHz audio of the first `dur` of a source.
fn audio_of(src: &filmcraft_media::SharedSource, dur: Tick) -> Option<Vec<f32>> {
    let sr = filmcraft_speech::SAMPLE_RATE;
    let len = dur.to_units_floor(sr as i64).max(0) as usize;
    let buf = src.audio(0, len, sr).ok()?;
    Some(filmcraft_speech::downmix(&buf.channels))
}

type Done = Vec<(ItemId, Transcript)>;

/// A running background `transcript.generate`; applied by [`poll`] when it finishes.
pub struct PendingTranscribe {
    pub job: u64,
    pub results: Arc<std::sync::Mutex<Option<Done>>>,
    /// Each item's media when the job started: a result lands only on that same media (the project
    /// may have been replaced meanwhile, and item ids repeat across projects).
    pub media: Vec<(ItemId, filmcraft_project::MediaRef)>,
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `transcript.generate {background: true}`: decode and transcribe in a named thread (`jobs.list`
/// shows the progress, `jobs.cancel` stops it), apply everything as one undo step in [`poll`].
fn generate_background(s: &mut Session, items: Vec<ItemId>, t: Arc<dyn Transcriber>, opts: Options) -> Result<Value> {
    let mut work = Vec::new();
    let mut media = Vec::new();
    let mut skipped = Vec::new();
    for item in items {
        let clip = match s.project.item(item).map(|i| &i.kind) {
            Some(ItemKind::Media(m)) => Some((m.duration(), m.media.clone())),
            _ => None,
        };
        match (clip, s.source(item)) {
            (Some((d, m)), Some(src)) if src.info().has_audio() => {
                work.push((item, src, d));
                media.push((item, m));
            }
            _ => skipped.push(item.0),
        }
    }
    if work.is_empty() {
        return Err(EngineError::Other("none of the clips has audio to transcribe".into()));
    }
    const PER_ITEM: u64 = 1000;
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let label = format!("Transcribe ({} clip{})", work.len(), if work.len() == 1 { "" } else { "s" });
    let job = crate::Job { id, label, progress: Default::default(), result: Default::default() };
    job.progress.total.store(work.len() as u64 * PER_ITEM, std::sync::atomic::Ordering::Relaxed);
    let results: Arc<std::sync::Mutex<Option<Done>>> = Arc::default();
    let (prog, res, out) = (job.progress.clone(), job.result.clone(), results.clone());
    let run = move || {
        use std::sync::atomic::Ordering;
        let t0 = web_time::Instant::now();
        let mut done: Done = Vec::new();
        let mut err = None;
        for (k, (item, src, dur)) in work.iter().enumerate() {
            let Some(audio) = audio_of(src, *dur) else { continue };
            let base = k as u64 * PER_ITEM;
            let mut cb = |f: f32, _: &str| {
                prog.done.store(base + (f.clamp(0.0, 1.0) * PER_ITEM as f32) as u64, Ordering::Relaxed);
                !prog.cancel.load(Ordering::Relaxed)
            };
            match t.transcribe(&audio, &opts, &mut cb) {
                Ok(mut tr) => {
                    tr.normalize();
                    done.push((*item, tr));
                    prog.done.store(base + PER_ITEM, Ordering::Relaxed);
                }
                Err(SpeechError::Cancelled) => {
                    err = Some("stopped".to_string());
                    break;
                }
                Err(e) => {
                    err = Some(e.to_string());
                    break;
                }
            }
        }
        if err.is_none() && done.is_empty() {
            err = Some("none of the clips has audio to transcribe".into());
        }
        let secs = t0.elapsed().as_secs_f64();
        *lock(&prog.status) = match &err {
            Some(e) if e == "stopped" => "Stopped: nothing was changed".into(),
            Some(e) => e.clone(),
            None => format!("Transcribed {} clip(s) ({secs:.1}s)", done.len()),
        };
        let r = match err {
            Some(e) => Err(e),
            None => {
                let frames = done.len() as u64;
                *lock(&out) = Some(done);
                Ok(filmcraft_export::Report { path: String::new(), frames, seconds: secs, bytes: 0, render_fps: 0.0, extra_files: Vec::new() })
            }
        };
        *lock(&res) = Some(r);
        prog.finished.store(true, Ordering::Relaxed);
    };
    let run = crate::export_tools::guard_job(job.progress.clone(), job.result.clone(), run);
    s.jobs.push(job);
    s.transcript_jobs.push(PendingTranscribe { job: id, results, media });
    if cfg!(target_arch = "wasm32") {
        run();
        poll(s);
    } else {
        std::thread::Builder::new().name("filmcraft-transcribe".into()).spawn(run).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    Ok(json!({"job": id, "skipped": skipped}))
}

/// Apply finished background transcriptions (one undo step each); a cancelled or failed job changes
/// nothing. Called once per UI frame from [`Session::poll_persistence`].
pub fn poll(s: &mut Session) {
    use std::sync::atomic::Ordering;
    let mut i = 0;
    while i < s.transcript_jobs.len() {
        let job = s.jobs.iter().find(|j| j.id == s.transcript_jobs[i].job);
        if !job.is_none_or(|j| j.progress.finished.load(Ordering::Relaxed)) {
            i += 1;
            continue;
        }
        let cancelled = job.is_some_and(|j| j.progress.cancel.load(Ordering::Relaxed));
        let pj = s.transcript_jobs.remove(i);
        let Some(done) = lock(&pj.results).take().filter(|_| !cancelled) else { continue };
        // only onto the media it was made from: never a same-numbered item of another project
        let same = |id: &ItemId| {
            let started = pj.media.iter().find(|(i, _)| i == id).map(|(_, m)| m);
            matches!((s.project.item(*id).map(|i| &i.kind), started), (Some(ItemKind::Media(m)), Some(was)) if m.media == *was)
        };
        let done: Done = done.into_iter().filter(|(id, _)| same(id)).collect();
        if done.is_empty() {
            continue;
        }
        let r = s.edit("Transcribe", move |pr, _| {
            for (i, t) in done {
                pr.transcripts.insert(i, Arc::new(t));
            }
            Ok(())
        });
        if let Err(e) = r {
            s.error_toast("transcript.generate", format!("Transcribe: {e}"));
        }
    }
}

fn speech_err(e: SpeechError) -> EngineError {
    EngineError::Other(e.to_string())
}

/// The transcriber to use: the installed one, else the named catalogue model.
fn transcriber(s: &Session, p: &Value) -> Result<Arc<dyn Transcriber>> {
    if let Some(t) = &s.transcriber {
        return Ok(t.clone());
    }
    // Settings ▸ Media Analysis & Transcription ▸ Speech model
    let model = str_p(p, "model").unwrap_or(&s.prefs.media_analysis.whisper_model);
    if filmcraft_speech::models::find(model).is_none() {
        return Err(speech_err(SpeechError::UnknownModel(model.into())));
    }
    if !filmcraft_speech::available() {
        return Err(EngineError::Other(NO_SPEECH.into()));
    }
    let dir = models_dir().ok_or_else(|| EngineError::Other("no data directory for speech models".into()))?;
    filmcraft_speech::load(&dir, model).map_err(speech_err)
}

fn generate(s: &mut Session, p: &Value) -> Result<Value> {
    let items = targets(s, p);
    if items.is_empty() {
        return Err(bad("transcript.generate", "nothing to transcribe (pass `items`, select clips, or open a sequence with audio)"));
    }
    let t = transcriber(s, p)?;
    // Settings ▸ Media Analysis & Transcription: language (or auto-detect) and speaker labelling
    let ma = &s.prefs.media_analysis;
    let default_language = if ma.language_auto_detect { None } else { Some(ma.default_language.clone()) };
    let opts = Options {
        language: match str_p(p, "language") {
            Some(l) => Some(l).filter(|l| !l.is_empty() && *l != "auto").map(str::to_string),
            None => default_language,
        },
        diarize: bool_p(p, "diarize").unwrap_or(ma.speaker_labeling != "off"),
        max_speakers: u64_p(p, "maxSpeakers").map(|n| n.clamp(1, 32) as usize).unwrap_or(Options::default().max_speakers),
    };
    if bool_p(p, "background").unwrap_or(false) {
        return generate_background(s, items, t, opts);
    }
    let mut done: Vec<(ItemId, Transcript)> = Vec::new();
    let mut skipped = Vec::new();
    for item in items {
        let Some(audio) = item_audio(s, item) else {
            skipped.push(item.0);
            continue;
        };
        let mut tr = t.transcribe(&audio, &opts, &mut |_, _| true).map_err(speech_err)?;
        tr.normalize();
        done.push((item, tr));
    }
    if done.is_empty() {
        return Err(EngineError::Other("none of the clips has audio to transcribe".into()));
    }
    let report: Vec<Value> = done
        .iter()
        .map(|(i, t)| json!({"item": i.0, "words": t.words.len(), "speakers": t.speakers.len(), "language": t.language, "source": t.source}))
        .collect();
    s.edit("Transcribe", move |pr, _| {
        for (i, t) in done {
            pr.transcripts.insert(i, Arc::new(t));
        }
        Ok(())
    })?;
    Ok(json!({"items": report, "skipped": skipped}))
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let item = u64_p(p, "item").map(ItemId).ok_or_else(|| bad("transcript.set", "`item` is required"))?;
    let item = media_item(s, item).ok_or_else(|| bad("transcript.set", "no such media item"))?;
    let v = p.get("transcript").cloned().ok_or_else(|| bad("transcript.set", "`transcript` is required"))?;
    let mut t: Transcript = serde_json::from_value(v).map_err(|e| bad("transcript.set", e.to_string()))?;
    if t.source.is_empty() {
        t.source = "imported".into();
    }
    t.normalize();
    t.check().map_err(|e| bad("transcript.set", e))?;
    let n = t.words.len();
    s.edit("Set Transcript", move |pr, _| {
        pr.transcripts.insert(item, Arc::new(t));
        Ok(())
    })?;
    Ok(json!({"item": item.0, "words": n}))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let items: Vec<ItemId> = match ids_p(p, "items").or_else(|| u64_p(p, "item").map(|i| vec![ItemId(i)])) {
        Some(v) => v.into_iter().filter_map(|i| media_item(s, i)).collect(),
        None => s.project.transcripts.keys().copied().collect(),
    };
    let n = items.iter().filter(|i| s.project.transcripts.contains_key(i)).count();
    if n == 0 {
        return Err(EngineError::Other("no transcript to delete".into()));
    }
    s.edit("Delete Transcript", move |pr, _| {
        for i in items {
            pr.transcripts.remove(&i);
        }
        Ok(())
    })?;
    Ok(json!({"deleted": n}))
}

fn word_json(i: usize, w: &SeqWord) -> Value {
    json!({"i": i, "text": w.text, "start": w.start.0, "end": w.end.0, "speaker": w.speaker, "clip": w.clip.0, "item": w.item.0, "confidence": w.confidence})
}

fn inspect(s: &mut Session, p: &Value) -> Result<Value> {
    let words = sequence_words(s);
    let gap = Tick::from_seconds_f64(f64_p(p, "paragraphGapSeconds").unwrap_or(1.5));
    let paras: Vec<Value> = tx::paragraphs(&words, gap)
        .into_iter()
        .map(|r| {
            let text = words[r.clone()].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            json!({"from": r.start, "to": r.end - 1, "speaker": words[r.start].speaker, "start": words[r.start].start.0, "end": words[r.end - 1].end.0, "text": text})
        })
        .collect();
    let speakers: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for w in &words {
            if let Some(n) = &w.speaker
                && !v.contains(n)
            {
                v.push(n.clone());
            }
        }
        v
    };
    let current = tx::word_at(&words, s.playhead());
    Ok(json!({
        "words": words.iter().enumerate().map(|(i, w)| word_json(i, w)).collect::<Vec<_>>(),
        "paragraphs": paras,
        "speakers": speakers,
        "current": current,
        "items": s.project.transcripts.iter().map(|(i, t)| json!({"item": i.0, "words": t.words.len(), "language": t.language, "source": t.source, "speakers": t.speakers.iter().map(|k| &k.name).collect::<Vec<_>>()})).collect::<Vec<_>>(),
    }))
}

fn search(s: &mut Session, p: &Value) -> Result<Value> {
    let q = str_p(p, "query").ok_or_else(|| bad("transcript.search", "`query` is required"))?;
    let words = sequence_words(s);
    let hits: Vec<Value> = tx::search(&words, q)
        .into_iter()
        .map(|r| json!({"from": r.start, "to": r.end - 1, "start": words[r.start].start.0, "end": words[r.end - 1].end.0}))
        .collect();
    Ok(json!({"matches": hits}))
}

/// Timeline range of the words `from..=to` (frame-snapped outward).
fn range_p(s: &Session, p: &Value, cmd: &str) -> Result<TimeRange> {
    let words = sequence_words(s);
    let from = u64_p(p, "from").ok_or_else(|| bad(cmd, "`from` (word index) is required"))? as usize;
    let to = u64_p(p, "to").map(|n| n as usize).unwrap_or(from);
    tx::word_range(&words, from, to, s.sequence_rate()).ok_or_else(|| bad(cmd, format!("word index out of range (the transcript has {} words)", words.len())))
}

fn range_json(r: TimeRange) -> Value {
    json!({"start": r.start.0, "end": r.end().0})
}

fn select(s: &mut Session, p: &Value) -> Result<Value> {
    let r = range_p(s, p, "transcript.select")?;
    let fd = s.sequence_rate().frame_duration();
    s.edit_sequence("Mark Transcript Selection", |q, _, _| {
        q.mark_in = Some(r.start);
        q.mark_out = Some(r.end() - fd);
        Ok(())
    })?;
    s.set_playhead(r.start);
    Ok(range_json(r))
}

fn extract_or_lift(s: &mut Session, p: &Value, extract: bool) -> Result<Value> {
    let cmd = if extract { "transcript.extract" } else { "transcript.lift" };
    let r = range_p(s, p, cmd)?;
    let tg = s.targeting().targeted;
    s.edit_sequence(if extract { "Extract Text" } else { "Lift Text" }, |q, ctx, _| {
        if extract {
            edit::extract(q, &tg, r, ctx);
        } else {
            edit::lift(q, &tg, r, ctx);
        }
        q.mark_in = None;
        q.mark_out = None;
        Ok(())
    })?;
    s.set_playhead(r.start);
    Ok(range_json(r))
}

fn rename_speaker(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_p(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad("transcript.renameSpeaker", "`name` is required"))?.to_string();
    let item = u64_p(p, "item").map(ItemId).and_then(|i| media_item(s, i));
    // `speaker`: the current name (every transcript), or an index (needs `item`)
    let (old_name, index) = match p.get("speaker") {
        Some(Value::String(n)) => (Some(n.clone()), None),
        Some(v) if v.is_u64() => (None, v.as_u64().map(|n| n as usize)),
        _ => return Err(bad("transcript.renameSpeaker", "`speaker` (name, or index with `item`) is required")),
    };
    if index.is_some() && item.is_none() {
        return Err(bad("transcript.renameSpeaker", "a speaker index needs `item`"));
    }
    let mut n = 0;
    let mut next = s.project.transcripts.clone();
    for (i, t) in next.iter_mut() {
        if item.is_some_and(|x| x != *i) {
            continue;
        }
        let tt = Arc::make_mut(t);
        for (k, sp) in tt.speakers.iter_mut().enumerate() {
            if old_name.as_ref().is_some_and(|o| *o == sp.name) || index == Some(k) {
                sp.name = name.clone();
                n += 1;
            }
        }
    }
    if n == 0 {
        return Err(EngineError::Other("no such speaker".into()));
    }
    s.edit("Rename Speaker", move |pr, _| {
        pr.transcripts = next;
        Ok(())
    })?;
    Ok(json!({"renamed": n}))
}

fn remove_ranges(s: &mut Session, label: &str, ranges: Vec<TimeRange>) -> Result<Value> {
    let n = ranges.len();
    if n == 0 {
        return Ok(json!({"removed": 0, "ticks": 0}));
    }
    let total = s.edit_sequence(label, |q, ctx, _| Ok(tx::ripple_delete_ranges(q, ranges, ctx)))?;
    Ok(json!({"removed": n, "ticks": total.0, "seconds": total.0 as f64 / TICKS_PER_SECOND as f64}))
}

fn remove_pauses(s: &mut Session, p: &Value) -> Result<Value> {
    let words = sequence_words(s);
    let min = Tick::from_seconds_f64(f64_p(p, "minSeconds").unwrap_or(1.0));
    let keep = Tick::from_seconds_f64(f64_p(p, "keepSeconds").unwrap_or(0.15));
    let ranges = tx::find_pauses(&words, min, keep, s.sequence_rate());
    remove_ranges(s, "Remove Pauses", ranges)
}

fn remove_fillers(s: &mut Session, p: &Value) -> Result<Value> {
    let words = sequence_words(s);
    let fillers: Vec<String> = match p.get("fillers").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        None => tx::DEFAULT_FILLERS.iter().map(|f| f.to_string()).collect(),
    };
    let hits = tx::find_fillers(&words, &fillers);
    let ranges = tx::filler_ranges(&words, &hits, s.sequence_rate());
    remove_ranges(s, "Remove Filler Words", ranges)
}

fn create_captions(s: &mut Session, p: &Value) -> Result<Value> {
    let words = sequence_words(s);
    let d = CaptionRules::default();
    let rules = CaptionRules {
        max_chars: u64_p(p, "maxChars").map(|n| n as usize).unwrap_or(d.max_chars),
        lines: u64_p(p, "lines").map(|n| n as usize).unwrap_or(d.lines),
        min_duration: f64_p(p, "minSeconds").map(Tick::from_seconds_f64).unwrap_or(d.min_duration),
        max_duration: f64_p(p, "maxSeconds").map(Tick::from_seconds_f64).unwrap_or(d.max_duration),
        gap_frames: p.get("gapFrames").and_then(Value::as_i64).unwrap_or(d.gap_frames),
        break_pause: d.break_pause,
    };
    let blocks = tx::caption_blocks(&words, &rules, s.sequence_rate());
    let format = str_p(p, "format").and_then(CaptionFormat::from_name).unwrap_or_default();
    let name = str_p(p, "name").unwrap_or("Transcript").to_string();
    let n = blocks.len();
    let tid = s.edit_sequence("Create Captions", |q, ctx, st| {
        let tid = TrackId(ctx.alloc());
        let mut t = CaptionTrack::new(tid, name, format);
        t.captions = tx::blocks_to_captions(&blocks, &words, ctx);
        q.caption_tracks.insert(0, t);
        st.caption_selection.clear();
        Ok(tid)
    })?;
    Ok(json!({"track": tid.0, "captions": n}))
}

/// AYP: the caption track "Phụ đề tiếng Việt (AYP)…" creates and replaces.
pub const AYP_TRACK: &str = "Phụ đề AYP";

/// AYP: Vietnamese captions from the transcript ([`filmcraft_edit::vi_captions`]): word-count lines
/// that keep compounds whole, filler and ASR clean-up tuned by `~/.capcut-sub/` (`userRules:
/// false` ignores it), on a track named [`AYP_TRACK`] that replaces the previous one, styled
/// white with a black outline, raised above phone UI on a vertical frame.
fn create_captions_vi(s: &mut Session, p: &Value) -> Result<Value> {
    use filmcraft_edit::vi_captions::vi_caption_blocks;
    let words = sequence_words(s);
    let mut rules = vi_rules(p);
    if let Some(n) = u64_p(p, "maxWords") {
        rules.max_words = usize::try_from(n.clamp(1, 30)).unwrap_or(10);
    }
    if let Some(b) = bool_p(p, "polish") {
        rules.polish = b;
    }
    if let Some(list) = p.get("replace").and_then(Value::as_array) {
        rules.replacements = list
            .iter()
            .filter_map(|x| {
                let pair = x.as_array()?;
                Some((pair.first()?.as_str()?.trim().to_string(), pair.get(1)?.as_str()?.trim().to_string()))
            })
            .filter(|(a, b)| !a.is_empty() && !b.is_empty())
            .take(500)
            .collect();
    }
    let blocks = vi_caption_blocks(&words, &rules, &CaptionRules::default(), s.sequence_rate());
    if blocks.is_empty() {
        return Err(bad("transcript.createCaptionsVi", "Không có câu nào để làm phụ đề"));
    }
    let n = blocks.len();
    let tid = put_ayp_track(s, &blocks, &words)?;
    Ok(json!({"track": tid.0, "captions": n}))
}

/// The user's caption rules (`~/.capcut-sub`, unless `userRules: false`) with `keywords`.
fn vi_rules(p: &Value) -> filmcraft_edit::vi_captions::ViRules {
    use filmcraft_edit::vi_captions::ViRules;
    let mut rules = match (bool_p(p, "userRules").unwrap_or(true), std::env::var_os("HOME")) {
        (true, Some(home)) => ViRules::load_user(&std::path::Path::new(&home).join(".capcut-sub")),
        _ => ViRules::standard(),
    };
    if let Some(k) = p.get("keywords").and_then(Value::as_array) {
        rules.keywords = k.iter().filter_map(Value::as_str).map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).take(200).collect();
    }
    rules
}

/// AYP: captions from chunks the editor (a person, or Claude) grouped by meaning:
/// `chunks: [{from, to?, text}]` with word indices from `transcript.inspect`. The tool times and
/// checks them; any broken layout rule fails the whole call with every problem listed.
fn captions_from_chunks(s: &mut Session, p: &Value) -> Result<Value> {
    use filmcraft_edit::vi_captions::{Chunk, chunk_blocks};
    const CMD: &str = "transcript.captionsFromChunks";
    let list = p.get("chunks").and_then(Value::as_array).ok_or_else(|| bad(CMD, "chunks: [{from, to?, text}] is required"))?;
    if list.is_empty() || list.len() > 5000 {
        return Err(bad(CMD, "chunks: 1–5000 chunks"));
    }
    let mut chunks = Vec::with_capacity(list.len());
    for (i, c) in list.iter().enumerate() {
        let idx = |k: &str| c.get(k).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
        let (Some(from), Some(text)) = (idx("from"), c.get("text").and_then(Value::as_str)) else {
            return Err(bad(CMD, format!("chunk #{i}: needs from (word index) and text")));
        };
        chunks.push(Chunk { from, to: idx("to"), text: text.to_string() });
    }
    let words = sequence_words(s);
    let rules = vi_rules(p);
    let (blocks, dropped) = chunk_blocks(&words, &chunks, &rules, &CaptionRules::default(), s.sequence_rate()).map_err(|e| bad(CMD, e.join("\n")))?;
    let n = blocks.len();
    let tid = put_ayp_track(s, &blocks, &words)?;
    Ok(json!({"track": tid.0, "captions": n, "dropped": dropped.iter().map(|(i, w)| json!({"chunk": i, "words": w})).collect::<Vec<_>>()}))
}

/// Replace the AYP caption track with `blocks`, keeping its style (a preset the user applied).
fn put_ayp_track(s: &mut Session, blocks: &[tx::CaptionBlock], words: &[SeqWord]) -> Result<TrackId> {
    let portrait = s.active_sequence().is_some_and(|q| q.settings.height > q.settings.width);
    s.edit_sequence("Phụ đề tiếng Việt (AYP)", |q, ctx, st| {
        // re-creating keeps the style of the track it replaces (a preset the user applied)
        let kept = q.caption_tracks.iter().find(|t| t.name == AYP_TRACK).map(|t| t.style.clone());
        q.caption_tracks.retain(|t| t.name != AYP_TRACK);
        let tid = TrackId(ctx.alloc());
        let mut t = CaptionTrack::new(tid, AYP_TRACK.to_string(), CaptionFormat::default());
        t.language = "vi".into();
        // Be Vietnam Pro (falls back to Inter when not installed), the block about two thirds down
        t.style = CaptionStyle {
            font: "Be Vietnam Pro".into(),
            size: 48.0,
            background: false,
            outline: 3.0,
            margin: if portrait { 0.28 } else { 0.08 },
            ..CaptionStyle::default()
        };
        if let Some(k) = kept {
            t.style = k;
        }
        t.captions = tx::blocks_to_captions(blocks, words, ctx);
        q.caption_tracks.insert(0, t);
        st.caption_selection.clear();
        Ok(tid)
    })
}

fn models(_: &mut Session, _: &Value) -> Result<Value> {
    let dir = models_dir();
    Ok(json!({
        "available": filmcraft_speech::available(),
        "default": filmcraft_speech::models::DEFAULT_MODEL,
        "dir": dir.as_ref().map(|d| d.to_string_lossy().to_string()),
        "models": filmcraft_speech::models::catalogue().iter().map(|m| json!({
            "id": m.id, "name": m.name, "multilingual": m.multilingual, "description": m.description,
            "license": m.license, "source": m.source, "size": m.size(),
            "installed": dir.as_ref().is_some_and(|d| filmcraft_speech::models::installed(d, m)),
        })).collect::<Vec<_>>(),
    }))
}

/// Download a catalogue model into `<data dir>/models` (feature `speech-download`). Hosts show
/// the size, source and licence (`transcript.models`) and ask before running this.
fn download_model(_: &mut Session, p: &Value) -> Result<Value> {
    let id = str_p(p, "model").unwrap_or(filmcraft_speech::models::DEFAULT_MODEL);
    let m = filmcraft_speech::models::find(id).ok_or_else(|| speech_err(SpeechError::UnknownModel(id.into())))?;
    let dir = models_dir().ok_or_else(|| EngineError::Other("no data directory for speech models".into()))?;
    #[cfg(feature = "speech-download")]
    {
        filmcraft_speech::models::download(&dir, m, &mut |_, _, _| true).map_err(speech_err)?;
        Ok(json!({"model": m.id, "dir": filmcraft_speech::models::model_dir(&dir, m).to_string_lossy()}))
    }
    #[cfg(not(feature = "speech-download"))]
    {
        let _ = (m, dir);
        Err(EngineError::Other("model downloads are not available in this build (built without the `speech-download` feature)".into()))
    }
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "transcript.generate",
            "Transcribe…",
            &["Sequence", "Transcript"],
            r#"{"items":[id]?,"model":"whisper-base"?,"language":"en|auto"?,"diarize":bool?,"maxSpeakers":n?,"background":bool=false}"#,
            can_transcribe,
            generate,
            true,
        ),
        spec(
            "transcript.set",
            "Import Transcript",
            &[],
            r#"{"item":id,"transcript":{"language":str,"speakers":[{"name":str}],"words":[{"text":str,"start":tick,"end":tick,"speaker":n?}]}}"#,
            always,
            set,
            true,
        ),
        spec("transcript.delete", "Delete Transcript", &["Sequence", "Transcript"], r#"{"items":[id]?}"#, has_transcripts, delete, true),
        spec("transcript.inspect", "Inspect Transcript", &[], r#"{"paragraphGapSeconds":f?}"#, always, inspect, false),
        spec("transcript.search", "Search Transcript", &[], r#"{"query":str}"#, always, search, false),
        spec("transcript.models", "List Speech Models", &[], "{}", always, models, false),
        spec("transcript.downloadModel", "Download Speech Model", &[], r#"{"model":"whisper-base"?}"#, can_download, download_model, true),
        spec("transcript.select", "Mark Selected Text", &[], r#"{"from":word,"to":word?}"#, has_transcript, select, true),
        spec("transcript.extract", "Extract Selected Text", &[], r#"{"from":word,"to":word?}"#, has_transcript, |s, p| extract_or_lift(s, p, true), true),
        spec("transcript.lift", "Lift Selected Text", &[], r#"{"from":word,"to":word?}"#, has_transcript, |s, p| extract_or_lift(s, p, false), true),
        spec(
            "transcript.renameSpeaker",
            "Rename Speaker…",
            &[],
            r#"{"speaker":"Speaker 1"|index,"name":str,"item":id?}"#,
            has_transcripts,
            rename_speaker,
            true,
        ),
        spec(
            "transcript.removePauses",
            "Remove Pauses",
            &["Sequence", "Transcript"],
            r#"{"minSeconds":f?,"keepSeconds":f?}"#,
            has_transcript,
            remove_pauses,
            true,
        ),
        spec("transcript.removeFillers", "Remove Filler Words", &["Sequence", "Transcript"], r#"{"fillers":[str]?}"#, has_transcript, remove_fillers, true),
        spec(
            "transcript.createCaptions",
            "Create Captions from Transcript…",
            &["Sequence", "Transcript"],
            r#"{"maxChars":n?,"lines":1|2?,"minSeconds":f?,"maxSeconds":f?,"gapFrames":n?,"format":str?,"name":str?}"#,
            has_transcript,
            create_captions,
            true,
        ),
        spec(
            "transcript.createCaptionsVi",
            "Phụ đề tiếng Việt (AYP)…",
            &["Sequence", "Transcript"],
            r#"{"maxWords":1..30?,"polish":bool?,"userRules":bool?,"keywords":[str]?,"replace":[[str,str]]?}"#,
            has_transcript,
            create_captions_vi,
            true,
        ),
        spec(
            "transcript.captionsFromChunks",
            "Phụ đề theo cụm của biên tập viên (AYP)",
            &["Sequence", "Transcript"],
            r#"{"chunks":[{"from":n,"to":n?,"text":str}],"keywords":[str]?,"userRules":bool?}"#,
            has_transcript,
            captions_from_chunks,
            true,
        ),
    ]
}
