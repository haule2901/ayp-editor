//! AYP: VieNeu-TTS voices. Pure parts here; the server round trip is `#[ignore]` (needs the
//! local install, see `ayp-cut/docs/plans/2026-10-16-filmcraft-vieneu-tts.md`).

use super::*;

#[test]
fn catalogue_is_vietnamese_unique_and_findable() {
    assert_eq!(VOICES.len(), 25);
    for (i, v) in VOICES.iter().enumerate() {
        assert!(v.info.id.starts_with("vieneu-") && v.info.id.is_ascii(), "{}", v.info.id);
        assert_eq!(v.info.language, "vi-VN");
        assert_eq!(v.info.engine, "vieneu");
        assert!(v.info.license.contains("Apache-2.0"));
        assert!(VOICES.iter().skip(i + 1).all(|w| w.info.id != v.info.id));
        assert_eq!(find(v.info.id).map(|p| p.preset), Some(v.preset));
    }
    assert_eq!(find("vieneu-mai-anh").map(|p| p.preset), Some("Mai Anh"));
    assert!(find("basic-female").is_none());
}

#[test]
fn pcm16_little_endian_to_float() {
    assert_eq!(pcm16_to_f32(&[0x00, 0x00, 0xff, 0x7f, 0x00, 0x80, 0x01]), vec![0.0, 32767.0 / 32768.0, -1.0]);
}

#[test]
fn stretch_changes_length_by_pace() {
    let x: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.01).sin()).collect();
    assert_eq!(stretch(&x, 1.0).len(), 24_000);
    assert_eq!(stretch(&x, 2.0).len(), 12_000);
    assert_eq!(stretch(&x, 0.5).len(), 48_000);
    assert!(stretch(&[], 1.5).is_empty());
}

#[test]
fn request_asks_for_24k_pcm_of_the_preset() {
    let v: serde_json::Value = serde_json::from_str(&request_body("Xin chào \"bạn\"", "Mai Anh")).unwrap();
    assert_eq!(v["input"], "Xin chào \"bạn\"");
    assert_eq!(v["voice"], "Mai Anh");
    assert_eq!(v["response_format"], "pcm");
    assert_eq!(v["sample_rate"], 24_000);
}

#[test]
fn install_lives_next_to_the_models() {
    let d = std::path::Path::new("/x/FilmCraft/models");
    assert_eq!(install_dir(d), std::path::Path::new("/x/FilmCraft/vieneu/VieNeu-TTS"));
    assert!(!installed(std::path::Path::new("/khong/co/models")));
}

#[cfg(feature = "vieneu")]
#[test]
fn a_dead_server_is_a_clear_error_not_a_hang() {
    let start = std::time::Instant::now();
    let err = speak(1, "Xin chào", "Mai Anh").unwrap_err();
    assert!(matches!(err, TtsError::Model(_)), "{err:?}");
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
}

/// Needs the local install and a running (or startable) server.
#[cfg(feature = "vieneu")]
#[test]
#[ignore]
fn speaks_vietnamese_through_the_local_server() {
    let models = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/FilmCraft/models");
    let v = load("vieneu-mai-anh", &models).unwrap();
    let a = v.synthesize("Xin chào, đây là giọng đọc tiếng Việt. [pause 500ms] Hẹn gặp lại.", &Params::default()).unwrap();
    assert_eq!(a.sample_rate, SAMPLE_RATE);
    assert!(a.seconds() > 2.0, "{}", a.seconds());
}

#[test]
fn long_scripts_are_split_at_sentences_under_the_server_limit() {
    let sentence = "Hôm nay mình kể chuyện tiết kiệm tiền khi mới đi làm. ";
    let text = sentence.repeat(200); // ~11 000 chars, no pause markers
    let parts = server_chunks(&text, 1500);
    assert!(parts.len() >= 7);
    assert!(parts.iter().all(|p| p.chars().count() <= 1500 && p.trim_end().ends_with('.')));
    assert_eq!(parts.iter().map(|p| p.split_whitespace().count()).sum::<usize>(), text.split_whitespace().count(), "nothing lost");
    // one sentence longer than the limit is cut at spaces, never mid-word
    let long = "chữ ".repeat(1000);
    assert!(server_chunks(&long, 1500).iter().all(|p| p.chars().count() <= 1500 && !p.starts_with("ữ")));
    assert_eq!(server_chunks("Xin chào.", 1500), vec!["Xin chào.".to_string()]);
}

#[cfg(feature = "vieneu")]
#[test]
fn a_server_that_dies_at_start_fails_fast() {
    let dir = std::env::temp_dir().join(format!("ayp-vieneu-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let log = dir.join("log.txt");
    std::fs::write(&log, "Traceback: boom\n").unwrap();
    let mut child = std::process::Command::new("/usr/bin/false").spawn().unwrap();
    let start = std::time::Instant::now();
    let err = wait_ready(&mut child, 1, std::time::Duration::from_secs(60), &log).unwrap_err();
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "did not wait the whole timeout");
    assert!(format!("{err}").contains("boom"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
