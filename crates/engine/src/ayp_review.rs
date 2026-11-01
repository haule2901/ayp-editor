//! AYP: review loop with the local frame.io-like page (`tools/ayp-review`, port 8765).
//! `review.open` exports the active sequence to the render drive and opens the page on that clip;
//! `review.import` turns the clip's `<clip>.mp4.feedback.json` notes into sequence markers.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use filmcraft_project::{Label, Marker, MarkerId, MarkerKind};
use filmcraft_time::FrameRate;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, bad, has_seq, str_p};
use crate::{EngineError, Result, Session};

const ROOT: &str = "/Volumes/Extreme SSD/Render";
const PORT: u16 = 8765;
const SCRIPT: &str = include_str!("../../../tools/ayp-review/review.py");
const PAGE: &str = include_str!("../../../tools/ayp-review/duyet-clip.html");
/// Markers named with this prefix came from the review page; an import replaces them.
const PREFIX: &str = "Góp ý";

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "review.open",
            label: "Duyệt góp ý (AYP)…",
            menu: &["Sequence"],
            shortcut: None,
            params: r#"{} (exports the active sequence to /Volumes/Extreme SSD/Render/<project>/<sequence>.mp4, then opens http://127.0.0.1:8765 on it)"#,
            enabled: has_seq,
            run: open,
            journal: false,
        },
        CommandSpec {
            id: "review.import",
            label: "Góp ý thành marker (AYP)",
            menu: &["Sequence"],
            shortcut: None,
            params: r#"{"path":str?} (the reviewed clip; default: where review.open exports the active sequence)"#,
            enabled: has_seq,
            run: import,
            journal: true,
        },
    ]
}

/// Mirrors `export_tools::file_safe`: the folder and file names the export writes.
fn safe(n: &str) -> String {
    let s: String = n.chars().map(|c| if c.is_alphanumeric() || " -_().".contains(c) { c } else { '_' }).collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Export".into() } else { s }
}

fn clip_path(s: &Session) -> Result<PathBuf> {
    let seq = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let name = s.project.item(seq).map(|i| i.name.as_str()).unwrap_or("Sequence");
    // a sequence made from a clip is named after the file ("x.mp4"): no "x.mp4.mp4"
    let name = [".mp4", ".mov", ".MP4", ".MOV"].iter().find_map(|e| name.strip_suffix(e)).unwrap_or(name);
    Ok(Path::new(ROOT).join(safe(&s.project.name)).join(format!("{}.mp4", safe(name))))
}

/// `?clip=` value: everything but unreserved characters and `/` percent-encoded.
fn encode(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Starts review.py (written to `<data dir>/review`, refreshed every time) unless the port answers.
fn ensure_server() -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        return Ok(());
    }
    let dir = crate::autosave::default_data_dir().ok_or_else(|| bad("review.open", "no data directory"))?.join("review");
    let io = |e: std::io::Error| bad("review.open", format!("review page: {e}"));
    std::fs::create_dir_all(&dir).map_err(io)?;
    std::fs::write(dir.join("review.py"), SCRIPT).map_err(io)?;
    std::fs::write(dir.join("duyet-clip.html"), PAGE).map_err(io)?;
    std::process::Command::new("/usr/bin/python3")
        .args(["review.py", "--no-open", "--root", ROOT])
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(io)?;
    Ok(())
}

fn open(s: &mut Session, _p: &Value) -> Result<Value> {
    if !Path::new(ROOT).is_dir() {
        return Err(bad("review.open", format!("không thấy {ROOT} (chưa cắm ổ Extreme SSD?)")));
    }
    let path = clip_path(s)?.to_string_lossy().into_owned();
    ensure_server()?;
    let out = crate::export_tools::export_media(
        s,
        &json!({"path": path, "format": "h264", "bitrateKbps": 8000, "range": "entire", "burnCaptions": true}),
    )?;
    let url = format!("http://127.0.0.1:{PORT}/?clip={}", encode(&path));
    let job = out["job"].as_u64().and_then(|id| s.jobs.iter().find(|j| j.id == id)).map(|j| j.result.clone());
    if let Some(result) = job {
        let url = url.clone();
        // Open the page once the export has finished (a failed export leaves it closed; Jobs shows why).
        let _ = std::thread::Builder::new().name("ayp-review-open".into()).spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                match &*result.lock().unwrap_or_else(|e| e.into_inner()) {
                    Some(Ok(_)) => break,
                    Some(Err(_)) => return,
                    None => {}
                }
            }
            let _ = std::process::Command::new("open").arg(&url).spawn();
        });
    }
    Ok(json!({"job": out["job"], "path": path, "url": url}))
}

/// One review note as a marker: open notes red, fixed ones green; a range note spans In→Out.
fn marker_of(item: &Value, rate: FrameRate, id: MarkerId) -> Option<Marker> {
    let frame = item.get("frame")?.as_i64()?.max(0);
    let end = item.get("frameEnd").and_then(Value::as_i64).filter(|e| *e > frame);
    let text = item.get("text").and_then(Value::as_str).unwrap_or("").trim();
    let tc = item.get("tc").and_then(Value::as_str).unwrap_or("");
    let done = item.get("status").and_then(Value::as_str) == Some("done");
    let reply = item.get("reply").and_then(Value::as_str).unwrap_or("").trim();
    let first: String = text.lines().next().unwrap_or("").chars().take(60).collect();
    let mut comment = text.to_string();
    if !reply.is_empty() {
        comment.push_str(&format!("\n— Claude: {reply}"));
    }
    if let Some(iid) = item.get("id").and_then(Value::as_str) {
        comment.push_str(&format!("\n[{tc} · id {iid}]"));
    }
    Some(Marker {
        id,
        start: rate.tick_of(frame),
        duration: end.map(|e| rate.tick_of(e.saturating_sub(frame))).unwrap_or_default(),
        name: format!("{PREFIX}: {}", if first.is_empty() { "(hình vẽ)" } else { &first }),
        comment,
        kind: MarkerKind::Comment,
        color: if done { Label::Green } else { Label::Rose },
    })
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let clip = match str_p(p, "path") {
        Some(x) => PathBuf::from(crate::export_tools::expand_home(x)),
        None => clip_path(s)?,
    };
    let fb = clip.with_file_name(format!("{}.feedback.json", clip.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    let text = std::fs::read_to_string(&fb).map_err(|e| bad("review.import", format!("{}: {e}", fb.display())))?;
    let data: Value = serde_json::from_str(&text).map_err(|e| bad("review.import", format!("{}: {e}", fb.display())))?;
    let fps = data.get("fps").and_then(Value::as_i64).filter(|f| (1..=240).contains(f)).unwrap_or(25);
    let rate = FrameRate::new(fps, 1);
    let items = data.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    let (added, open) = s.edit_sequence("Import Review Notes", |q, ctx, _| {
        q.markers.retain(|m| !m.name.starts_with(PREFIX));
        let mut open = 0;
        let mut added = 0;
        for item in items.iter().take(10_000) {
            if let Some(m) = marker_of(item, rate, MarkerId(ctx.alloc())) {
                open += usize::from(m.color == Label::Rose);
                added += 1;
                q.markers.push(m);
            }
        }
        q.markers.sort_by_key(|m| m.start);
        Ok((added, open))
    })?;
    Ok(json!({"path": clip.to_string_lossy(), "markers": added, "open": open, "done": added - open}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_become_markers() {
        let rate = FrameRate::new(25, 1);
        let note = json!({"id": "a1", "frame": 50, "frameEnd": 75, "tc": "00:02:00", "text": "Cắt bớt\nchỗ này", "status": "open", "reply": ""});
        let m = marker_of(&note, rate, MarkerId(1)).expect("marker");
        assert_eq!((m.start, m.duration, m.color), (rate.tick_of(50), rate.tick_of(25), Label::Rose));
        assert_eq!(m.name, "Góp ý: Cắt bớt");
        let done = marker_of(&json!({"frame": 10, "frameEnd": 5, "status": "done", "reply": "ok", "draw": [1]}), rate, MarkerId(2)).expect("marker");
        assert_eq!((done.duration, done.color, done.name.as_str()), (Default::default(), Label::Green, "Góp ý: (hình vẽ)"));
        assert!(done.comment.contains("Claude: ok"));
        assert!(marker_of(&json!({"text": "no frame"}), rate, MarkerId(3)).is_none());
        assert_eq!(encode("/Volumes/Extreme SSD/a – b.mp4"), "/Volumes/Extreme%20SSD/a%20%E2%80%93%20b.mp4");
    }
}
