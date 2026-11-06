//! The AYP timeline toolbar.

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
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
        let dir = std::env::temp_dir().join(format!("ayptool-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
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

    fn v1_items(&mut self) -> Vec<Value> {
        let v = self.exec("sequence.inspect", json!({}));
        v["result"]["video"][0]["items"].as_array().unwrap().clone()
    }
}

const ORDER: [&str; 11] = [
    "edit.undo",
    "edit.redo",
    "sequence.addEdit",
    "edit.rippleDelete",
    "clip.frameHold",
    "clip.link",
    "sequence.snap",
    "markers.add",
    "view.zoomOut",
    "view.zoomIn",
    "file.exportMedia",
];

#[test]
fn buttons_are_in_order() {
    let d = Driver::launch(true);
    let mut found: Vec<(f32, &str)> = Vec::new();
    for c in ORDER {
        let id = format!("ayp.tool.{c}");
        let e = d.h.state().auto.query(&id).into_iter().find(|e| e.id == id).unwrap_or_else(|| panic!("missing {id}"));
        found.push((e.rect[0], c));
    }
    assert!(found.windows(2).all(|w| w[0].0 < w[1].0), "{found:?}");
}

fn mid_clip(d: &mut Driver) -> i64 {
    let it = d.v1_items()[0].clone();
    let (s, e) = (it["start"].as_i64().unwrap(), it["end"].as_i64().unwrap());
    (s + e) / 2
}

#[test]
fn split_adds_an_edit() {
    let mut d = Driver::launch(true);
    let n = d.v1_items().len();
    let t = mid_clip(&mut d);
    d.exec("playhead.set", json!({"time": t}));
    d.click("ayp.tool.sequence.addEdit");
    assert_eq!(d.v1_items().len(), n + 1, "{}", d.h.state().ui.status);
}

#[test]
fn undo_reverts_it() {
    let mut d = Driver::launch(true);
    let n = d.v1_items().len();
    let t = mid_clip(&mut d);
    d.exec("playhead.set", json!({"time": t}));
    d.click("ayp.tool.sequence.addEdit");
    d.click("ayp.tool.edit.undo");
    assert_eq!(d.v1_items().len(), n);
}

#[test]
fn snap_toggles() {
    let mut d = Driver::launch(true);
    let a = d.h.state().session.state.snapping;
    d.click("ayp.tool.sequence.snap");
    assert_eq!(d.h.state().session.state.snapping, !a);
    d.click("ayp.tool.sequence.snap");
    assert_eq!(d.h.state().session.state.snapping, a);
}

#[test]
fn failing_button_says_why_and_no_sequence_is_safe() {
    let mut d = Driver::launch(true);
    d.exec("edit.deselectAll", json!({}));
    d.click("ayp.tool.clip.link");
    assert!(d.h.state().ui.status.starts_with("Chưa làm được"), "{}", d.h.state().ui.status);
    let mut e = Driver::launch(false);
    e.frames(3);
    assert!(e.has("ayp.tool.edit.undo"));
    assert!(e.h.state().ui_error.is_none());
}

/// `AYP_TOOLBAR_PNG=/path.png cargo test --test ayp_toolbar_ui snapshot -- --ignored`
#[test]
#[ignore]
fn snapshot() {
    let Ok(path) = std::env::var("AYP_TOOLBAR_PNG") else { return };
    let mut d = Driver::launch(true);
    d.frames(3);
    d.h.render().expect("render").save(path).unwrap();
}
