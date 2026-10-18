//! AYP: caption style presets (CapCut "text templates" for a caption track). Built-in presets are
//! authored in code ([`builtin`]); presets the user saves persist in
//! `<data dir>/caption-presets.json` as `[{"name": str, "style": CaptionStyle}…]` (a preset with
//! a built-in's name shadows it). Applying keeps the track's margin, which depends on the frame
//! shape (raised above phone UI on vertical video).
//!
//! Commands: `captions.presets`, `captions.applyPreset` (undoable), `captions.savePreset`,
//! `captions.deletePreset`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use filmcraft_project::{CaptionAnchor, CaptionHighlight, CaptionStyle};

use crate::captions::{has_caption_track, track_param};
use crate::commands::{CommandSpec, always, bad, str_p};
use crate::{EngineError, Result, Session};

const FILE: &str = "caption-presets.json";
/// Longest preset name (characters) and most user presets kept.
const MAX_NAME: usize = 64;
const MAX_PRESETS: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptionPreset {
    pub name: String,
    #[serde(default)]
    pub style: CaptionStyle,
}

const YELLOW: [u8; 4] = [255, 210, 0, 255];

/// The presets that ship with the app.
pub fn builtin() -> Vec<CaptionPreset> {
    let base = CaptionStyle { font: "Be Vietnam Pro".into(), font_style: "Bold".into(), size: 48.0, background: false, ..CaptionStyle::default() };
    let p = |name: &str, style: CaptionStyle| CaptionPreset { name: name.into(), style };
    vec![
        p("Vàng viền đen AYP", CaptionStyle { color: YELLOW, outline: 3.0, ..base.clone() }),
        p("Karaoke vàng", CaptionStyle { outline: 3.0, highlight: CaptionHighlight::Karaoke, highlight_color: YELLOW, ..base.clone() }),
        p(
            "Hộp trắng bo góc",
            CaptionStyle {
                color: [0, 0, 0, 255],
                background: true,
                background_color: [255, 255, 255, 255],
                background_round_radius: 0.4,
                background_padding: 6.0,
                ..base.clone()
            },
        ),
        p(
            "Một từ to giữa khung",
            CaptionStyle {
                font_style: "Black".into(),
                size: 84.0,
                anchor: CaptionAnchor::Middle,
                outline: 4.0,
                highlight: CaptionHighlight::ActiveWord,
                highlight_color: YELLOW,
                highlight_scale: 1.15,
                shadow: true,
                ..base
            },
        ),
    ]
}

fn file(s: &Session) -> Option<std::path::PathBuf> {
    s.prefs_path.as_ref().and_then(|p| p.parent()).map(|d| d.join(FILE))
}

/// The user's saved presets (a missing or unreadable file is an empty list).
fn user(s: &Session) -> Vec<CaptionPreset> {
    let Some(bytes) = file(s).and_then(|f| std::fs::read(f).ok()) else { return Vec::new() };
    match serde_json::from_slice::<Vec<CaptionPreset>>(&bytes) {
        Ok(mut v) => {
            v.truncate(MAX_PRESETS);
            v
        }
        Err(e) => {
            log::warn!("{FILE}: {e}");
            Vec::new()
        }
    }
}

fn write_user(s: &Session, v: &[CaptionPreset]) -> Result<()> {
    let f = file(s).ok_or_else(|| EngineError::Other("no data directory to save caption presets in".into()))?;
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| EngineError::Other(format!("{}: {e}", d.display())))?;
    }
    let bytes = serde_json::to_vec_pretty(v).map_err(|e| EngineError::Other(e.to_string()))?;
    filmcraft_format::atomic_write(&f, &bytes).map_err(|e| EngineError::Other(format!("{}: {e}", f.display())))
}

/// Built-in and user presets (`true` = built in), user presets shadowing built-ins of the same name.
pub fn all(s: &Session) -> Vec<(CaptionPreset, bool)> {
    let user = user(s);
    let mut v: Vec<(CaptionPreset, bool)> = builtin().into_iter().filter(|b| !user.iter().any(|u| u.name == b.name)).map(|b| (b, true)).collect();
    v.extend(user.into_iter().map(|u| (u, false)));
    v
}

fn name_p(p: &Value, cmd: &str) -> Result<String> {
    let n: String = str_p(p, "name").unwrap_or("").trim().chars().take(MAX_NAME).collect();
    if n.is_empty() { Err(bad(cmd, "need a preset `name`")) } else { Ok(n) }
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({"presets": all(s).iter().map(|(p, b)| json!({"name": p.name, "builtin": b})).collect::<Vec<_>>()}))
}

fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    let name = name_p(p, "captions.applyPreset")?;
    let preset = all(s).into_iter().find(|(x, _)| x.name == name).ok_or_else(|| bad("captions.applyPreset", format!("no preset `{name}`")))?.0;
    let tid = track_param(s, p).ok_or_else(|| bad("captions.applyPreset", "no such caption track"))?;
    s.edit("Apply Caption Preset", |pr, st| {
        let seq = st.active_sequence.ok_or(EngineError::NoSequence)?;
        let t = pr.sequence_mut(seq).and_then(|q| q.caption_track_mut(tid)).ok_or(EngineError::NoSequence)?;
        t.style = CaptionStyle { margin: t.style.margin, ..preset.style };
        Ok(())
    })?;
    Ok(json!({"applied": name}))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let name = name_p(p, "captions.savePreset")?;
    let tid = track_param(s, p).ok_or_else(|| bad("captions.savePreset", "no such caption track"))?;
    let style = s.active_sequence().and_then(|q| q.caption_track(tid)).map(|t| t.style.clone()).ok_or(EngineError::NoSequence)?;
    let mut v = user(s);
    v.retain(|x| x.name != name);
    if v.len() >= MAX_PRESETS {
        return Err(bad("captions.savePreset", format!("at most {MAX_PRESETS} presets; delete one first")));
    }
    v.push(CaptionPreset { name: name.clone(), style });
    write_user(s, &v)?;
    Ok(json!({"saved": name}))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let name = name_p(p, "captions.deletePreset")?;
    let mut v = user(s);
    let n = v.len();
    v.retain(|x| x.name != name);
    if v.len() == n {
        return Err(bad("captions.deletePreset", format!("no saved preset `{name}` (built-in presets cannot be deleted)")));
    }
    write_user(s, &v)?;
    Ok(json!({"deleted": name}))
}

pub fn commands() -> Vec<CommandSpec> {
    let spec = |id, label, params, enabled, run, journal| CommandSpec { id, label, menu: &[], shortcut: None, params, enabled, run, journal };
    vec![
        spec("captions.presets", "List Caption Presets", "{}", always, list, false),
        spec("captions.applyPreset", "Apply Caption Preset", r#"{"name":str,"track":id|"C1"?}"#, has_caption_track, apply, true),
        spec("captions.savePreset", "Save Caption Style as Preset", r#"{"name":str,"track":id|"C1"?}"#, has_caption_track, save, true),
        spec("captions.deletePreset", "Delete Caption Preset", r#"{"name":str}"#, always, delete, true),
    ]
}
