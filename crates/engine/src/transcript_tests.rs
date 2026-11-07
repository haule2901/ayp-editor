use std::sync::Arc;

use serde_json::json;

use crate::Session;
use filmcraft_project::{ItemId, Transcript, Word};
use filmcraft_speech::FixedTranscriber;
use filmcraft_time::Tick;

/// The demo project with a fake transcriber whose transcript fits the first A1 clip's media:
/// "Hello um world." (Speaker 1), a 1.2 s pause, "Second speaker here." (Speaker 2).
fn session() -> (Session, ItemId, Tick) {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).unwrap();
    let q = s.active_sequence().unwrap();
    let a = &q.audio_tracks[0].items[0];
    let (item, sin) = (a.item, a.source_in);
    let sec = |x: f64| sin + Tick::from_seconds_f64(x);
    let mut t = Transcript { language: "en".into(), ..Default::default() };
    for (text, a, b, sp) in
        [("Hello", 0.2, 0.5, 0), ("um", 0.6, 0.9, 0), ("world.", 1.0, 1.4, 0), ("Second", 2.6, 3.0, 1), ("speaker", 3.0, 3.5, 1), ("here.", 3.5, 4.0, 1)]
    {
        let mut w = Word::new(text, sec(a), sec(b));
        w.speaker = Some(sp);
        t.words.push(w);
    }
    t.normalize();
    s.transcriber = Some(Arc::new(FixedTranscriber { transcript: t, id: "fixed".into() }));
    let start = a_start(&s);
    (s, item, start)
}

fn a_start(s: &Session) -> Tick {
    s.active_sequence().unwrap().audio_tracks[0].items[0].start
}

fn words(s: &mut Session) -> Vec<String> {
    let r = s.execute("transcript.inspect", json!({})).unwrap();
    r["words"].as_array().unwrap().iter().map(|w| w["text"].as_str().unwrap().to_string()).collect()
}

#[test]
fn generate_inspect_search_and_rename() {
    let (mut s, item, start) = session();
    assert!(s.execute("transcript.select", json!({"from": 0})).is_err(), "disabled before transcribing");
    let r = s.execute("transcript.generate", json!({"items": [item.0]})).unwrap();
    assert_eq!(r["items"][0]["words"], 6, "{r}");
    assert_eq!(r["items"][0]["source"], "fixed");
    assert_eq!(s.project.transcripts[&item].words.len(), 6);

    let r = s.execute("transcript.inspect", json!({})).unwrap();
    assert_eq!(words(&mut s), ["Hello", "um", "world.", "Second", "speaker", "here."]);
    assert_eq!(r["words"][0]["start"].as_i64().unwrap(), (start + Tick::from_seconds_f64(0.2)).0, "mapped to sequence time");
    assert_eq!(r["paragraphs"].as_array().unwrap().len(), 2, "speaker change splits paragraphs: {}", r["paragraphs"]);
    assert_eq!(r["speakers"], json!(["Speaker 1", "Speaker 2"]));

    let r = s.execute("transcript.search", json!({"query": "second spea"})).unwrap();
    assert_eq!(r["matches"], json!([{"from": 3, "to": 4, "start": r["matches"][0]["start"], "end": r["matches"][0]["end"]}]));

    s.execute("transcript.renameSpeaker", json!({"speaker": "Speaker 2", "name": "Ann"})).unwrap();
    let r = s.execute("transcript.inspect", json!({})).unwrap();
    assert_eq!(r["words"][3]["speaker"], "Ann");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.project.transcripts[&item].speakers[1].name, "Speaker 2");
    s.execute("transcript.renameSpeaker", json!({"speaker": 0, "item": item.0, "name": "Bo"})).unwrap();
    assert_eq!(s.project.transcripts[&item].speakers[0].name, "Bo");
    assert!(s.execute("transcript.renameSpeaker", json!({"speaker": "Nobody", "name": "X"})).is_err());

    // transcribing is one undo step
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert!(s.project.transcripts.is_empty());
}

#[test]
fn select_extract_and_lift_by_words() {
    let (mut s, item, _) = session();
    s.execute("transcript.generate", json!({"items": [item.0]})).unwrap();
    let rate = s.sequence_rate();
    let before = s.active_sequence().unwrap().duration();

    let r = s.execute("transcript.select", json!({"from": 3, "to": 5})).unwrap();
    let (a, b) = (Tick(r["start"].as_i64().unwrap()), Tick(r["end"].as_i64().unwrap()));
    let q = s.active_sequence().unwrap();
    assert_eq!(q.mark_in, Some(a));
    assert_eq!(q.mark_out, Some(b - rate.frame_duration()), "Out is the last frame inside");
    assert_eq!(rate.snap(a), a, "frame aligned");
    assert_eq!(s.playhead(), a);

    // Extract "um": the sequence gets shorter by the word's frames and the word is gone
    let r = s.execute("transcript.extract", json!({"from": 1})).unwrap();
    let cut = Tick(r["end"].as_i64().unwrap()) - Tick(r["start"].as_i64().unwrap());
    assert!(cut > Tick::ZERO);
    assert_eq!(s.active_sequence().unwrap().duration(), before - cut);
    assert!(!words(&mut s).contains(&"um".to_string()), "{:?}", words(&mut s));
    s.active_sequence().unwrap().check().unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.active_sequence().unwrap().duration(), before);

    // Lift leaves a gap: same duration, word gone
    s.execute("transcript.lift", json!({"from": 1, "to": 1})).unwrap();
    assert_eq!(s.active_sequence().unwrap().duration(), before);
    assert_eq!(words(&mut s), ["Hello", "world.", "Second", "speaker", "here."]);
    assert!(s.execute("transcript.extract", json!({"from": 99})).is_err());
}

#[test]
fn remove_fillers_pauses_and_create_captions() {
    let (mut s, item, _) = session();
    s.execute("transcript.generate", json!({"items": [item.0]})).unwrap();
    let before = s.active_sequence().unwrap().duration();
    let r = s.execute("transcript.removeFillers", json!({})).unwrap();
    assert_eq!(r["removed"], 1, "{r}");
    assert!(!words(&mut s).contains(&"um".to_string()));
    let r = s.execute("transcript.removePauses", json!({"minSeconds": 1.0, "keepSeconds": 0.1})).unwrap();
    assert_eq!(r["removed"], 1, "{r}");
    let after = s.active_sequence().unwrap().duration();
    assert!(after < before - Tick::from_seconds_f64(1.0), "the pause and the filler are gone");
    assert_eq!(words(&mut s).len(), 5);
    s.active_sequence().unwrap().check().unwrap();

    let r = s.execute("transcript.createCaptions", json!({"maxChars": 32})).unwrap();
    assert_eq!(r["captions"], 2, "one caption per speaker: {r}");
    let q = s.active_sequence().unwrap();
    let tr = &q.caption_tracks[0];
    assert_eq!(tr.captions[0].text, "Hello world.");
    assert_eq!(tr.captions[1].speaker.as_deref(), Some("Speaker 2"));
    q.check().unwrap();
}

#[test]
fn set_delete_and_models() {
    let (mut s, item, _) = session();
    let t = json!({"language": "en", "words": [
        {"text": "b", "start": 2000, "end": 3000, "speaker": 1},
        {"text": "a", "start": 0, "end": 1000},
    ]});
    let r = s.execute("transcript.set", json!({"item": item.0, "transcript": t})).unwrap();
    assert_eq!(r["words"], 2);
    let tr = &s.project.transcripts[&item];
    assert_eq!(tr.words[0].text, "a", "normalized: sorted");
    assert_eq!(tr.speakers.len(), 2, "missing speaker labels added");
    assert_eq!(tr.source, "imported");
    assert!(s.execute("transcript.set", json!({"item": 999_999, "transcript": {}})).is_err());
    assert!(s.execute("transcript.set", json!({"item": item.0, "transcript": {"words": 3}})).is_err());

    // survives save/load
    let bytes = filmcraft_format::encode(&s.project, true);
    let back = filmcraft_format::decode(&bytes).unwrap();
    assert_eq!(back.project.transcripts[&item].words.len(), 2);

    s.execute("transcript.delete", json!({"items": [item.0]})).unwrap();
    assert!(s.project.transcripts.is_empty());
    assert!(s.execute("transcript.delete", json!({})).is_err());

    let m = s.execute("transcript.models", json!({})).unwrap();
    assert_eq!(m["available"], filmcraft_speech::available());
    assert!(m["models"].as_array().unwrap().iter().any(|x| x["id"] == "whisper-base"));
}

#[test]
fn generate_without_a_transcriber() {
    let (mut s, item, _) = session();
    s.transcriber = None;
    let e = s.execute("transcript.generate", json!({"items": [item.0], "model": "nope"})).unwrap_err().to_string();
    // a build without speech-to-text says that first: the command is disabled (#97)
    let why = if filmcraft_speech::available() { "unknown speech model" } else { "not available in this build" };
    assert!(e.contains(why), "{e}");
    if !filmcraft_speech::available() {
        assert!(!s.is_enabled("sequence.transcribe"), "Transcribe Sequence follows transcript.generate");
        let e = s.execute("transcript.generate", json!({"items": [item.0]})).unwrap_err().to_string();
        assert!(e.contains("whisper") && e.contains("not available"), "{e}");
        #[cfg(not(feature = "speech-download"))]
        assert!(s.execute("transcript.downloadModel", json!({})).unwrap_err().to_string().contains("not available"));
    }
    // defaults to the media of the open sequence's audio clips
    s.transcriber = Some(Arc::new(FixedTranscriber { transcript: Transcript::default(), id: "empty".into() }));
    assert!(s.is_enabled("sequence.transcribe"), "an installed recogniser enables Transcribe Sequence");
    let r = s.execute("transcript.generate", json!({})).unwrap();
    assert!(r["items"].as_array().unwrap().len() >= 2, "{r}");
}

/// AYP: "Phụ đề tiếng Việt (AYP)…" — Vietnamese captions with the AYP track style.
#[test]
fn vietnamese_captions_replace_their_track_and_undo() {
    let (mut s, item, _) = session();
    let q = s.active_sequence().unwrap();
    let sin = q.audio_tracks[0].items[0].source_in;
    assert!(s.execute("transcript.createCaptionsVi", json!({})).is_err(), "disabled without a transcript");

    let sec = |x: f64| (sin + Tick::from_seconds_f64(x)).0;
    let words: Vec<_> = ["ừm", "hôm", "nay", "Người", "ta", "nói", "về", "7", "nghiệp."]
        .iter()
        .enumerate()
        .map(|(i, w)| json!({"text": w, "start": sec(0.2 + i as f64 * 0.3), "end": sec(0.45 + i as f64 * 0.3)}))
        .collect();
    s.execute("transcript.set", json!({"item": item.0, "transcript": {"language": "vi", "speakers": [], "words": words}})).unwrap();
    s.execute("sequence.settings", json!({"width": 1080, "height": 1920})).unwrap();

    let r = s.execute("transcript.createCaptionsVi", json!({"userRules": false, "keywords": ["thất nghiệp"]})).unwrap();
    assert_eq!(r["captions"], 1, "{r}");
    let q = s.active_sequence().unwrap();
    let ayp: Vec<_> = q.caption_tracks.iter().filter(|t| t.name == "Phụ đề AYP").collect();
    assert_eq!(ayp.len(), 1);
    let c = &ayp[0].captions[0];
    assert_eq!(c.plain_lines().join(" "), "Hôm nay người ta nói về thất nghiệp.", "fillers, capitals and numbers fixed");
    assert_eq!(c.text.lines().count(), 2, "two balanced lines: {:?}", c.text);
    assert!(c.text.contains("<c #ffd200>thất nghiệp</c>"), "keyword coloured: {:?}", c.text);
    let st = &ayp[0].style;
    assert_eq!((st.font.as_str(), st.size, st.outline, st.background), ("Be Vietnam Pro", 48.0, 3.0, false));
    assert!((st.margin - 0.28).abs() < 1e-6, "the block sits about two thirds down a vertical frame");
    q.check().unwrap();

    // AYP: re-creating keeps the track's style (a preset the user applied)
    s.execute("captions.setStyle", json!({"track": "C1", "highlight": "karaoke"})).unwrap();
    // ASR fixes from the caller ("replace": [[heard, meant]])
    s.execute("transcript.createCaptionsVi", json!({"userRules": false, "replace": [["người ta", "mọi người"]]})).unwrap();
    let q = s.active_sequence().unwrap();
    assert!(q.caption_tracks[0].captions.iter().any(|c| c.plain_lines().join(" ").contains("mọi người")), "replace applied");

    s.execute("transcript.createCaptionsVi", json!({"maxWords": 4, "userRules": false})).unwrap();
    let q = s.active_sequence().unwrap();
    assert_eq!(q.caption_tracks.iter().filter(|t| t.name == "Phụ đề AYP").count(), 1, "replaced, not stacked");
    assert!(q.caption_tracks[0].captions.len() >= 2);
    assert_eq!(q.caption_tracks[0].style.highlight, filmcraft_project::CaptionHighlight::Karaoke);
    // one word per caption (TikTok style)
    s.execute("transcript.createCaptionsVi", json!({"maxWords": 1, "userRules": false})).unwrap();
    let caps = &s.active_sequence().unwrap().caption_tracks[0].captions;
    assert!(caps.len() >= 4 && caps.iter().all(|c| c.words.len() <= 2), "compounds stay whole: {:?}", caps.iter().map(|c| &c.text).collect::<Vec<_>>());
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();

    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert!(s.active_sequence().unwrap().caption_tracks.iter().all(|t| t.name != "Phụ đề AYP"));
    s.execute("edit.redo", json!({})).unwrap();
    assert_eq!(s.active_sequence().unwrap().caption_tracks.iter().filter(|t| t.name == "Phụ đề AYP").count(), 1);
}

/// AYP: the editor (a person, or Claude) groups captions by meaning; the tool times and checks them.
#[test]
fn editor_chunks_become_the_ayp_track() {
    let (mut s, item, _) = session();
    let sin = s.active_sequence().unwrap().audio_tracks[0].items[0].source_in;
    let sec = |x: f64| (sin + Tick::from_seconds_f64(x)).0;
    let words: Vec<_> = ["hôm", "nay", "mình", "nói", "về", "thất", "nghiệp", "nhé", "các", "bạn"]
        .iter()
        .enumerate()
        .map(|(i, w)| json!({"text": w, "start": sec(0.2 + i as f64 * 0.3), "end": sec(0.45 + i as f64 * 0.3)}))
        .collect();
    s.execute("transcript.set", json!({"item": item.0, "transcript": {"language": "vi", "speakers": [], "words": words}})).unwrap();
    let chunks = json!([{"from": 0, "text": "Hôm nay mình nói về thất / nghiệp nhé các bạn."}]);
    // a broken rule is refused with its reason, and nothing changes
    let e = s.execute("transcript.captionsFromChunks", json!({"chunks": chunks, "userRules": false})).unwrap_err().to_string();
    assert!(e.contains("tách đôi \"thất nghiệp\"") && e.contains("ký tự"), "{e}");
    assert!(s.active_sequence().unwrap().caption_tracks.is_empty());

    let chunks = json!([{"from": 0, "text": "Hôm nay mình nói về"}, {"from": 5, "text": "thất nghiệp nhé / các bạn."}]);
    let r = s.execute("transcript.captionsFromChunks", json!({"chunks": chunks, "userRules": false, "keywords": ["thất nghiệp"]})).unwrap();
    assert_eq!((r["captions"].as_u64(), r["dropped"].as_array().map(Vec::len)), (Some(2), Some(0)), "{r}");
    let q = s.active_sequence().unwrap();
    let t = &q.caption_tracks[0];
    assert_eq!(t.name, "Phụ đề AYP");
    assert_eq!(t.captions[1].text, "<c #ffd200>thất nghiệp</c> nhé\ncác bạn.");
    assert!(t.captions[0].start + t.captions[0].duration <= t.captions[1].start);
    q.check().unwrap();
}

fn wait_job(s: &mut Session) {
    let t0 = std::time::Instant::now();
    while !s.transcript_jobs.is_empty() {
        s.poll_persistence();
        assert!(t0.elapsed().as_secs() < 60, "job never finished");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// `background: true` returns a job at once; the transcript lands in one undo step once it finished.
#[test]
fn background_transcribe_applies_once_when_done() {
    let (mut s, item, _) = session();
    let before = s.history.undo.len();
    let r = s.execute("transcript.generate", json!({"items": [item.0], "background": true})).unwrap();
    let job = r["job"].as_u64().expect("a job id");
    assert!(r.get("items").is_none(), "{r}");
    wait_job(&mut s);
    let j = s.execute("jobs.list", json!({})).unwrap();
    let j = j.as_array().unwrap().iter().find(|x| x["id"] == job).unwrap().clone();
    assert_eq!(j["finished"], true, "{j}");
    assert_eq!(j["done"], j["total"], "{j}");
    assert_eq!(s.project.transcripts[&item].words.len(), 6);
    assert_eq!(s.history.undo.len(), before + 1, "one undo step");
    s.execute("edit.undo", json!({})).unwrap();
    assert!(s.project.transcripts.is_empty());
}

/// Waits until it is cancelled.
struct Stubborn;
impl filmcraft_speech::Transcriber for Stubborn {
    fn id(&self) -> String {
        "stubborn".into()
    }
    fn transcribe(
        &self,
        _: &[f32],
        _: &filmcraft_speech::Options,
        progress: filmcraft_speech::ProgressFn,
    ) -> Result<Transcript, filmcraft_speech::SpeechError> {
        while progress(0.1, "waiting") {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err(filmcraft_speech::SpeechError::Cancelled)
    }
}

#[test]
fn background_transcribe_cancel_changes_nothing() {
    let (mut s, item, _) = session();
    s.transcriber = Some(Arc::new(Stubborn));
    let (rev, before) = (s.revision, s.history.undo.len());
    let r = s.execute("transcript.generate", json!({"items": [item.0], "background": true})).unwrap();
    s.execute("jobs.cancel", json!({"job": r["job"]})).unwrap();
    wait_job(&mut s);
    assert!(s.project.transcripts.is_empty());
    assert_eq!((s.revision, s.history.undo.len()), (rev, before));
}
