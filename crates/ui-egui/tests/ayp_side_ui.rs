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
    let n = d.h.state().auto.query("").len();
    let mut d2 = Driver::launch(true);
    d2.exec("edit.deselectAll", json!({}));
    d2.frames(2);
    assert!(d.has("ayp.side.props") && d.has("ayp.side.ai"));
    assert!(n > d2.h.state().auto.query("").len() + 1, "properties controls should appear: {} vs {}", n, d2.h.state().auto.query("").len());
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

#[test]
fn switching_to_ai() {
    let mut d = Driver::launch(true);
    assert_eq!(d.h.state().ui.ayp_side, AypSideTab::Props);
    d.click("ayp.side.ai");
    assert_eq!(d.h.state().ui.ayp_side, AypSideTab::Ai);
    assert!(d.has("ayp.side.openPalette"));
    d.click("ayp.side.openPalette"); // palette arrives in Task 8: the error is swallowed
    assert!(d.h.state().ui.status.is_empty(), "{}", d.h.state().ui.status);
    d.click("ayp.side.props");
    assert_eq!(d.h.state().ui.ayp_side, AypSideTab::Props);
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
