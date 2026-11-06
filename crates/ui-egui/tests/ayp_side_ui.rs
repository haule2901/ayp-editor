//! The AYP right panel: Thông số | AI.

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::state::AypSideTab;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    h: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn launch(demo: bool) -> Self {
        let mut session = Session::default();
        if demo {
            session.execute("file.openDemoProject", json!({})).expect("demo project");
        }
        // One data dir per driver: the AYP-first-launch marker lives beside the preferences.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("aypside-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        session.prefs_path = Some(dir.join("preferences.json"));
        let (tx, rx) = channel();
        let mut app = FilmcraftApp::new(session).with_control(rx);
        app.ayp_default = true;
        let h = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
        let mut d = Driver { h, tx };
        d.frames(4);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.h.ctx.clone();
            let mut raw = std::mem::take(self.h.input_mut());
            eframe::App::raw_input_hook(self.h.state_mut(), &ctx, &mut raw);
            *self.h.input_mut() = raw;
            self.h.step();
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params);
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                return v;
            }
        }
        panic!("no reply to {method}");
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        let v = self.call("engine.execute", json!({"command": command, "params": params}));
        assert_eq!(v["ok"], json!(true), "{command}: {v}");
        v
    }

    fn click(&mut self, id: &str) {
        let v = self.call("ui.click", json!({"id": id}));
        assert_eq!(v["ok"], json!(true), "{id}: {v}");
        self.frames(3);
    }

    fn has(&self, id: &str) -> bool {
        self.h.state().auto.query(id).iter().any(|e| e.id == id)
    }

    fn select_first_clip(&mut self) {
        let v = self.exec("sequence.inspect", json!({}));
        let id = v["result"]["video"][0]["items"][0]["clip"].clone();
        self.exec("timeline.select", json!({"clips": [id]}));
        self.frames(2);
    }
}

#[test]
fn props_tab_shows_the_selected_clip() {
    let mut d = Driver::launch(true);
    d.select_first_clip();
    assert!(!d.has("ayp.hint.selectClip"));
    assert!(d.has("ayp.side.props") && d.has("ayp.side.ai"));
    // the classic Properties panel's own controls: the clip's Motion ▸ Position keyframe button
    assert!(d.has("properties.motion.position.addKeyframe"));
    d.exec("edit.deselectAll", json!({}));
    d.frames(2);
    assert!(!d.has("properties.motion.position.addKeyframe"));
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

#[test]
fn switching_to_ai() {
    let mut d = Driver::launch(true);
    assert_eq!(d.h.state().ui.ayp_side, AypSideTab::Props);
    d.click("ayp.side.ai");
    assert_eq!(d.h.state().ui.ayp_side, AypSideTab::Ai);
    assert!(d.has("ayp.side.openPalette"));
    d.click("ayp.side.openPalette");
    assert!(d.has("ayp.palette"), "Tìm lệnh… opened: {}", d.h.state().ui.status);
    assert_eq!(d.h.state().ui.ayp_palette.as_deref(), Some(""));
    d.call("ui.key", json!({"key": "Escape"}));
    d.frames(2);
    d.click("ayp.side.props");
    assert_eq!(d.h.state().ui.ayp_side, AypSideTab::Props);
}

/// The control port was taken by another app at launch: the AI tab says so for as long as the app runs.
#[test]
fn ai_tab_says_when_the_control_port_is_busy() {
    let mut d = Driver::launch(true);
    d.click("ayp.side.ai");
    assert!(!d.has("ayp.side.portBusy"));
    d.h.state_mut().control_busy_port = Some(9879);
    d.frames(2);
    let e = d.h.state().auto.query("ayp.side.portBusy");
    assert_eq!(e.first().map(|e| e.label.as_str()), Some("Cổng 9879 đang bận — AI/MCP đang nối với app khác"));
}

#[test]
fn no_selection_hint() {
    let mut d = Driver::launch(true);
    d.exec("edit.deselectAll", json!({}));
    d.frames(2);
    assert!(d.has("ayp.hint.selectClip"));
}

#[test]
fn no_sequence_does_not_panic() {
    let mut d = Driver::launch(false);
    d.frames(3);
    assert!(d.has("ayp.side.props"));
    d.click("ayp.side.ai");
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

/// `AYP_SIDE_PNG=/path.png cargo test --test ayp_side_ui snapshot -- --ignored` writes a picture of the window.
#[test]
#[ignore]
fn snapshot() {
    let Ok(path) = std::env::var("AYP_SIDE_PNG") else { return };
    let mut d = Driver::launch(true);
    d.select_first_clip();
    d.frames(3);
    d.h.render().expect("render").save(path).unwrap();
}
