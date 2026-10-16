//! AYP: Vietnamese voices from VieNeu-TTS v3 Turbo (Apache-2.0, `github.com/pnnbao97/VieNeu-TTS`).
//!
//! VieNeu runs as its own local process: the project's OpenAI-compatible server
//! (`apps/openai_speech.py`, ONNX Runtime on the CPU) installed with `uv` in
//! `<data dir>/vieneu/VieNeu-TTS` and listening on `127.0.0.1:`[`PORT`]. A voice starts the
//! server when it is not running (loading takes ~20 s) and leaves it running for the next
//! narration. Each speech segment of the script (pause markers kept) is sent in pieces of at most
//! 1 500 characters cut at sentence ends (`POST /v1/audio/speech`, 24 kHz 16-bit PCM); one
//! narration at a time (the CPU server runs a single stream). VieNeu has no speed control: pace is a
//! resample with the pitch change compensated by the pitch shifter of `filmcraft-audio-dsp`.

use std::path::{Path, PathBuf};

#[cfg(feature = "vieneu")]
use crate::{Audio, MAX_SECONDS, Params, SAMPLE_RATE, TtsError, Voice, check_text};
use crate::{Gender, VoiceInfo};

/// Port of the local VieNeu server (8000 is the project's default; FilmCraft keeps its own).
pub const PORT: u16 = 8891;
const LICENSE: &str = "Apache-2.0 (VieNeu-TTS v3 Turbo, pnnbao97)";

/// A VieNeu preset voice.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub info: VoiceInfo,
    /// The name the server knows the voice by.
    pub preset: &'static str,
}

macro_rules! v {
    ($id:literal, $preset:literal, $g:ident, $desc:literal) => {
        Preset {
            info: VoiceInfo {
                id: $id,
                name: $preset,
                language: "vi-VN",
                gender: Gender::$g,
                engine: "vieneu",
                description: $desc,
                license: LICENSE,
                installed: false,
            },
            preset: $preset,
        }
    };
}

/// The 25 presets of VieNeu-TTS v3 Turbo (`src/vieneu/assets/voices_v3_turbo.json`).
pub static VOICES: &[Preset] = &[
    v!("vieneu-adam-bua", "Adam bựa", Male, "Nam · Bắc · Phong cách tự nhiên"),
    v!("vieneu-truc-ly", "Trúc Ly", Female, "Nữ · Bắc · Phong cách tự nhiên"),
    v!("vieneu-thien-minh", "Thiện Minh", Male, "Nam · Bắc · Phong cách kể chuyện"),
    v!("vieneu-mai-anh", "Mai Anh", Female, "Nữ · Bắc · Phong cách tin tức"),
    v!("vieneu-hai-dang", "Hải Đăng", Male, "Nam · Bắc · Phong cách tự nhiên"),
    v!("vieneu-thuy-dung", "Thùy Dung", Female, "Nữ · Nam · Phong cách tin tức"),
    v!("vieneu-thien-tam-duc", "Thiền Tâm Đức", Male, "Nam · Bắc · Phong cách kể chuyện"),
    v!("vieneu-ngoc-huyen", "Ngọc Huyền", Female, "Nữ · Bắc · Giọng đọc tự nhiên"),
    v!("vieneu-quang-son", "Quang Sơn", Male, "Nam · Trung · Phong cách tự nhiên"),
    v!("vieneu-ngoc-tran", "Ngọc Trân", Female, "Nữ · Trung · Phong cách tự nhiên"),
    v!("vieneu-minh-duc", "Minh Đức", Male, "Nam · Bắc · Phong cách tin tức"),
    v!("vieneu-pham-tuyen", "Phạm Tuyên", Male, "Nam · Bắc · Phong cách tự nhiên"),
    v!("vieneu-thai-son", "Thái Sơn", Male, "Nam · Nam · Phong cách kể chuyện"),
    v!("vieneu-xuan-vinh", "Xuân Vĩnh", Male, "Nam · Bắc · Phong cách tự nhiên"),
    v!("vieneu-thanh-binh", "Thanh Bình", Male, "Nam · Bắc · Phong cách kể chuyện"),
    v!("vieneu-ngoc-linh", "Ngọc Linh", Female, "Nữ · Bắc · Phong cách kể chuyện"),
    v!("vieneu-doan-trang", "Đoan Trang", Female, "Nữ · Bắc · Phong cách tự nhiên"),
    v!("vieneu-thuc-doan", "Thục Đoan", Female, "Nữ · Nam · Phong cách kể chuyện"),
    v!("vieneu-minh-triet", "Minh Triết", Male, "Nam · Nam · Phong cách tin tức"),
    v!("vieneu-my-duyen", "Mỹ Duyên", Female, "Nữ · Nam · Phong cách đọc truyện"),
    v!("vieneu-quynh-anh", "Quỳnh Anh", Female, "Nữ · Bắc · Phong cách đọc truyện"),
    v!("vieneu-duc-tri", "Đức Trí", Male, "Nam · Nam · Phong cách đọc truyện"),
    v!("vieneu-kim-thanh", "Kim Thanh", Female, "Nữ · Nam · Phong cách đọc truyện"),
    v!("vieneu-adam", "Adam", Male, "Nam · Nam · Giọng đọc tự nhiên"),
    v!("vieneu-quoc-tuan", "Quốc Tuấn", Male, "Nam · Bắc · Phong cách tự nhiên"),
];

pub fn find(id: &str) -> Option<&'static Preset> {
    VOICES.iter().find(|v| v.info.id == id)
}

/// Where VieNeu is installed: `<data dir>/vieneu/VieNeu-TTS`, next to `<data dir>/models`.
pub fn install_dir(models_dir: &Path) -> PathBuf {
    models_dir.parent().unwrap_or(models_dir).join("vieneu").join("VieNeu-TTS")
}

/// VieNeu and its environment are installed.
pub fn installed(models_dir: &Path) -> bool {
    let d = install_dir(models_dir);
    d.join(".venv").is_dir() && d.join("apps").join("openai_speech.py").is_file()
}

/// Little-endian 16-bit PCM to floats in [-1, 1); a trailing odd byte is ignored.
pub fn pcm16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.as_chunks::<2>().0.iter().map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0).collect()
}

/// Resample to `len / pace` samples (linear interpolation): faster speech, higher pitch.
pub fn stretch(x: &[f32], pace: f64) -> Vec<f32> {
    if x.is_empty() || !pace.is_finite() || pace <= 0.0 {
        return x.to_vec();
    }
    let n = ((x.len() as f64) / pace).round() as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * pace;
            let k = p.floor() as usize;
            let f = (p - p.floor()) as f32;
            let a = x.get(k).copied().unwrap_or(0.0);
            let b = x.get(k + 1).copied().unwrap_or(a);
            a + (b - a) * f
        })
        .collect()
}

/// Split a speech segment into pieces of at most `max_chars` characters for the server (it refuses
/// inputs over 20 000 characters, and one long request would run into the timeout): at sentence
/// ends, and a sentence that is still too long at spaces. Nothing is dropped.
pub fn server_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let max = max_chars.max(16);
    let mut sentences: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if matches!(c, '.' | '!' | '?' | '…' | '\n') {
            sentences.push(std::mem::take(&mut cur));
        }
    }
    if !cur.trim().is_empty() {
        sentences.push(cur);
    }
    // sentences longer than the limit: cut at spaces
    let mut pieces: Vec<String> = Vec::new();
    for snt in sentences {
        if snt.chars().count() <= max {
            pieces.push(snt);
            continue;
        }
        let mut part = String::new();
        for w in snt.split_inclusive(' ') {
            if !part.is_empty() && part.chars().count() + w.chars().count() > max {
                pieces.push(std::mem::take(&mut part));
            }
            part.push_str(w);
        }
        if !part.is_empty() {
            pieces.push(part);
        }
    }
    // pack consecutive pieces up to the limit
    let mut out: Vec<String> = Vec::new();
    for p in pieces {
        match out.last_mut() {
            Some(last) if last.chars().count() + p.chars().count() <= max => last.push_str(&p),
            _ => out.push(p),
        }
    }
    out.into_iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The `POST /v1/audio/speech` body: 24 kHz 16-bit PCM of `text` in voice `preset`.
pub fn request_body(text: &str, preset: &str) -> String {
    format!(r#"{{"model":"vieneu-v3-turbo","input":{},"voice":{},"response_format":"pcm","sample_rate":24000}}"#, json_str(text), json_str(preset))
}

#[cfg(feature = "vieneu")]
fn agent(timeout_s: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(2)))
        .timeout_global(Some(std::time::Duration::from_secs(timeout_s)))
        .http_status_as_error(false)
        .build()
        .into()
}

#[cfg(feature = "vieneu")]
fn healthy(port: u16) -> bool {
    agent(3).get(&format!("http://127.0.0.1:{port}/health")).call().is_ok_and(|r| r.status().is_success())
}

/// One segment through the server on `port`.
#[cfg(feature = "vieneu")]
pub fn speak(port: u16, text: &str, preset: &str) -> Result<Vec<f32>, TtsError> {
    let url = format!("http://127.0.0.1:{port}/v1/audio/speech");
    let resp = agent(600)
        .post(&url)
        .header("Content-Type", "application/json")
        .send(request_body(text, preset))
        .map_err(|e| TtsError::Model(format!("VieNeu: {e}")))?;
    let status = resp.status();
    let mut body = resp.into_body();
    let bytes = body.with_config().limit(400 * 1024 * 1024).read_to_vec().map_err(|e| TtsError::Model(format!("VieNeu: {e}")))?;
    if !status.is_success() {
        let msg = String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>();
        return Err(TtsError::Model(format!("VieNeu {status}: {msg}")));
    }
    Ok(pcm16_to_f32(&bytes))
}

#[cfg(feature = "vieneu")]
fn uv() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    [PathBuf::from("/opt/homebrew/bin/uv"), PathBuf::from("/usr/local/bin/uv"), home.join(".local/bin/uv"), home.join(".cargo/bin/uv")]
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("uv"))
}

/// The server process this FilmCraft started (if any). The lock also serialises narrations: the
/// CPU server runs one stream and answers a second one with 429.
#[cfg(feature = "vieneu")]
fn server() -> &'static std::sync::Mutex<Option<std::process::Child>> {
    static S: std::sync::OnceLock<std::sync::Mutex<Option<std::process::Child>>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

/// Wait until the server on `port` answers, failing at once (with the end of `log`) if `child`
/// exits, or after `timeout`.
#[cfg(feature = "vieneu")]
pub fn wait_ready(child: &mut std::process::Child, port: u16, timeout: std::time::Duration, log: &Path) -> Result<(), TtsError> {
    let tail = || {
        let t = std::fs::read_to_string(log).unwrap_or_default();
        let lines: Vec<&str> = t.lines().collect();
        lines.get(lines.len().saturating_sub(5)..).unwrap_or_default().join(" | ")
    };
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(TtsError::Model(format!("VieNeu dừng ngay khi khởi động ({status}): {}", tail())));
        }
        if healthy(port) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Err(TtsError::Model(format!("VieNeu không khởi động sau {} s — xem {}", timeout.as_secs(), log.display())))
}

/// Start the server from `dir` unless it already answers or is already starting; wait for it.
#[cfg(feature = "vieneu")]
fn ensure_server(dir: &Path, running: &mut Option<std::process::Child>) -> Result<(), TtsError> {
    if healthy(PORT) {
        return Ok(());
    }
    let log_path = dir.join("filmcraft-server.log");
    let starting = running.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    if !starting {
        let log = std::fs::File::create(&log_path).map_err(|e| TtsError::Model(format!("VieNeu log: {e}")))?;
        let err = log.try_clone().map_err(|e| TtsError::Model(format!("VieNeu log: {e}")))?;
        let child = std::process::Command::new(uv())
            .args(["run", "--no-sync", "python", "-m", "apps.openai_speech"])
            .current_dir(dir)
            .env("HOST", "127.0.0.1")
            .env("PORT", PORT.to_string())
            .env("VIENEU_BACKEND", "onnx")
            .stdin(std::process::Stdio::null())
            .stdout(log)
            .stderr(err)
            .spawn()
            .map_err(|e| TtsError::Model(format!("không chạy được VieNeu (uv): {e}")))?;
        *running = Some(child);
    }
    match running.as_mut() {
        Some(c) => wait_ready(c, PORT, std::time::Duration::from_secs(180), &log_path),
        None => Err(TtsError::Model("VieNeu chưa khởi động".into())),
    }
}

/// A VieNeu voice ready to speak (feature `vieneu`).
#[cfg(feature = "vieneu")]
pub struct VieneuVoice {
    info: VoiceInfo,
    preset: &'static str,
    dir: PathBuf,
}

/// The VieNeu voice `id`, if VieNeu is installed next to `models_dir`.
#[cfg(feature = "vieneu")]
pub fn load(id: &str, models_dir: &Path) -> Result<VieneuVoice, TtsError> {
    let v = find(id).ok_or_else(|| TtsError::UnknownVoice(id.to_string()))?;
    if !installed(models_dir) {
        return Err(TtsError::Model(format!("VieNeu-TTS chưa cài — cài vào {}", install_dir(models_dir).display())));
    }
    Ok(VieneuVoice { info: VoiceInfo { installed: true, ..v.info }, preset: v.preset, dir: install_dir(models_dir) })
}

#[cfg(feature = "vieneu")]
impl Voice for VieneuVoice {
    fn info(&self) -> VoiceInfo {
        self.info
    }

    fn synthesize(&self, text: &str, params: &Params) -> Result<Audio, TtsError> {
        check_text(text)?;
        params.check()?;
        let mut running = server().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_server(&self.dir, &mut running)?;
        let rate = SAMPLE_RATE;
        let max = (MAX_SECONDS * f64::from(rate)) as usize;
        let mut out: Vec<f32> = Vec::new();
        let mut spoke = false;
        for seg in crate::script::parse(text) {
            match seg {
                crate::script::Segment::Pause { millis } => {
                    out.resize(out.len().saturating_add((u64::from(millis) * u64::from(rate) / 1000) as usize), 0.0);
                }
                crate::script::Segment::Speech(t) => {
                    if !t.chars().any(char::is_alphanumeric) {
                        continue;
                    }
                    for chunk in server_chunks(&t, 1500) {
                        out.extend(speak(PORT, &chunk, self.preset)?);
                        if out.len() > max {
                            return Err(TtsError::AudioTooLong);
                        }
                    }
                    spoke = true;
                }
            }
            if out.len() > max {
                return Err(TtsError::AudioTooLong);
            }
        }
        if !spoke {
            return Err(TtsError::Empty);
        }
        let mut shift = params.semitones;
        if (params.pace - 1.0).abs() > 1e-6 {
            out = stretch(&out, params.pace);
            shift -= 12.0 * params.pace.log2();
        }
        if shift.abs() > 1e-6 {
            out = pitch_shift(out, shift, rate);
        }
        let peak = out.iter().fold(0f32, |m, x| m.max(x.abs()));
        if peak > 0.98 {
            let g = 0.98 / peak;
            out.iter_mut().for_each(|x| *x *= g);
        }
        Ok(Audio { samples: out, sample_rate: rate })
    }
}

/// Shift pitch by `semitones` keeping the length (latency compensated), as the neural voices do.
#[cfg(feature = "vieneu")]
fn pitch_shift(mut x: Vec<f32>, semitones: f64, rate: u32) -> Vec<f32> {
    use filmcraft_audio_dsp::AudioEffect;
    let mut ps = filmcraft_audio_dsp::effects::PitchShifter::new(rate as f32, 1);
    ps.set_param("semitones", semitones as f32);
    ps.reset();
    let lat = ps.latency();
    let n = x.len();
    x.resize(n + lat, 0.0);
    for block in x.chunks_mut(4096) {
        ps.process(&mut [block]);
    }
    x.drain(..lat.min(x.len()));
    x.truncate(n);
    x
}

#[cfg(test)]
mod tests;
