//! Headless UI test of the caption track font picker in the Text panel's Captions tab: picking a
//! family sets the track style's font, and macOS-private (`.`-prefixed) faces are not offered.
//! AYP: the preset picker applies a preset and saves the track style under a typed name.

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn demo() -> Self {
        Self::with(|_| {})
    }

    fn with(setup: impl FnOnce(&mut Session)) -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        setup(&mut session);
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let harness = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(4);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
                return v["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn ids(&mut self, prefix: &str) -> Vec<String> {
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().unwrap().iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }

    fn font(&mut self, track: u64) -> String {
        let r = self.ok("engine.execute", json!({"command": "captions.list", "params": {}}));
        let t = r["tracks"].as_array().unwrap().iter().find(|t| t["id"] == json!(track)).expect("track").clone();
        t["style"]["font"].as_str().unwrap().to_string()
    }
}

#[test]
fn caption_font_picker_sets_the_track_font() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Captions and Graphics"}));
    let track = d.ok("engine.execute", json!({"command": "captions.newTrack", "params": {"format": "Subtitle"}}))["track"].as_u64().unwrap();
    d.ok("engine.execute", json!({"command": "captions.add", "params": {"track": track, "text": "Hello", "seconds": 1.0}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "text.tab.Captions"}));
    d.frames(3);
    assert_eq!(d.font(track), "Inter");

    d.ok("ui.click", json!({"id": "text.captions.style.font"}));
    d.frames(3);
    let options = d.ids("text.captions.style.font.option.");
    assert!(!options.is_empty(), "the font list is open");
    assert!(!options.iter().any(|o| o.starts_with("text.captions.style.font.option..")), "private faces offered: {options:?}");
    // with system fonts the list is long: scroll until the bundled serif is on screen
    let serif = "text.captions.style.font.option.Noto Serif";
    for _ in 0..200 {
        let shown = d.ids("text.captions.style.font.option.");
        if shown.iter().any(|o| o == serif) {
            break;
        }
        // wheel over a row in the middle (the edge rows are clipped by the popup)
        let Some(mid) = shown.get(shown.len() / 2) else { break };
        d.ok("ui.scroll", json!({"id": mid, "dy": -120.0}));
        d.frames(2);
    }
    d.ok("ui.click", json!({"id": serif}));
    d.frames(3);
    assert_eq!(d.font(track), "Noto Serif");
}

/// AYP: Preset ▸ "Karaoke vàng" applies the preset; a typed name + Save style saves the track
/// style, which then shows in the list (with a delete button).
#[test]
fn caption_preset_picker_applies_and_saves() {
    let dir = std::env::temp_dir().join(format!("fc-cap-presets-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let prefs = dir.join("prefs.json");
    let mut d = Driver::with(move |s| s.prefs_path = Some(prefs));
    d.ok("ui.set", json!({"workspace": "Captions and Graphics"}));
    let track = d.ok("engine.execute", json!({"command": "captions.newTrack", "params": {"format": "Subtitle"}}))["track"].as_u64().unwrap();
    d.ok("engine.execute", json!({"command": "captions.add", "params": {"track": track, "text": "Hello", "seconds": 1.0}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "text.tab.Captions"}));
    d.frames(3);
    d.ok("ui.click", json!({"id": "text.captions.preset"}));
    d.frames(3);
    d.ok("ui.click", json!({"id": "text.captions.preset.Karaoke vàng"}));
    d.frames(3);
    let style = |d: &mut Driver| d.ok("engine.execute", json!({"command": "captions.list", "params": {}}))["tracks"][0]["style"].clone();
    assert_eq!(style(&mut d)["highlight"], "karaoke");
    d.ok("ui.click", json!({"id": "text.captions.preset.name"}));
    d.frames(2);
    d.ok("ui.type", json!({"text": "Của tôi"}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "text.captions.preset.save"}));
    d.frames(4);
    let names = d.ok("engine.execute", json!({"command": "captions.presets", "params": {}}));
    assert!(names["presets"].as_array().unwrap().iter().any(|p| p["name"] == "Của tôi" && p["builtin"] == false), "{names}");
    assert!(d.ids("text.captions.preset.Của tôi.delete").len() == 1, "saved preset listed with a delete button");
}

/// AYP: Captions ▸ More holds the CapCut-like settings: the shadow checkbox and the highlight
/// mode set the track style.
#[test]
fn caption_more_styles_set_shadow_and_highlight() {
    let mut d = Driver::demo();
    d.ok("ui.set", json!({"workspace": "Captions and Graphics"}));
    let track = d.ok("engine.execute", json!({"command": "captions.newTrack", "params": {"format": "Subtitle"}}))["track"].as_u64().unwrap();
    d.ok("engine.execute", json!({"command": "captions.add", "params": {"track": track, "text": "Hello", "seconds": 1.0}}));
    d.frames(2);
    d.ok("ui.click", json!({"id": "text.tab.Captions"}));
    d.frames(3);
    d.ok("ui.click", json!({"id": "text.captions.style.more"}));
    d.frames(3);
    d.ok("ui.click", json!({"id": "text.captions.style.shadow"}));
    d.frames(3);
    let style = |d: &mut Driver| d.ok("engine.execute", json!({"command": "captions.list", "params": {}}))["tracks"][0]["style"].clone();
    assert_eq!(style(&mut d)["shadow"], true);
    for id in ["text.captions.viWords", "text.captions.viRecreate", "text.captions.style.backgroundRoundRadius", "text.captions.style.tracking"] {
        assert_eq!(d.ids(id).len(), 1, "{id} shown");
    }
    d.ok("ui.click", json!({"id": "text.captions.style.highlight.activeWord"}));
    d.frames(3);
    assert_eq!(style(&mut d)["highlight"], "activeWord");
}
