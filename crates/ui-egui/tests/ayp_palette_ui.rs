//! "Tìm lệnh…": opens from the command, the Task 5 button and the shortcut; types, runs, closes.

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
    fn launch() -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        // One data dir per driver: the AYP-first-launch marker lives beside the preferences.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("aypfind-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
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

    fn ok(&mut self, method: &str, params: Value) {
        let v = self.call(method, params);
        assert_eq!(v["ok"], json!(true), "{method}: {v}");
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

#[test]
fn the_command_opens_the_finder_and_esc_closes_it() {
    let mut d = Driver::launch();
    assert!(!d.has("ayp.palette"));
    d.exec("ayp.palette.open", json!({}));
    d.frames(3);
    assert!(d.has("ayp.palette"));
    d.ok("ui.key", json!({"key": "Escape"}));
    assert!(!d.has("ayp.palette"));
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

#[test]
fn the_find_button_and_the_shortcut_open_it() {
    let mut d = Driver::launch();
    d.ok("ui.click", json!({"id": "ayp.find"}));
    assert!(d.has("ayp.palette"));
    d.ok("ui.key", json!({"key": "Escape"}));
    assert!(!d.has("ayp.palette"));
    d.ok("ui.key", json!({"key": "Cmd+Shift+P"}));
    assert!(d.has("ayp.palette"), "Cmd+Shift+P opens the finder");
}

#[test]
fn typing_without_marks_then_enter_runs_the_top_command() {
    let mut d = Driver::launch();
    let items = d.v1_items();
    let first = &items[0];
    let mid = first["start"].as_i64().unwrap() + first["duration"].as_i64().unwrap() / 2;
    d.exec("playhead.set", json!({"time": mid}));
    let before = d.v1_items().len();
    d.exec("ayp.palette.open", json!({}));
    d.frames(3);
    d.ok("ui.type", json!({"text": "tach"}));
    assert!(d.has("ayp.palette.hit.sequence.addEdit"), "Add Edit is listed for `tach`");
    d.ok("ui.key", json!({"key": "Enter"}));
    assert!(!d.has("ayp.palette"), "the finder closes after running");
    assert_eq!(d.v1_items().len(), before + 1, "status: {}", d.h.state().ui.status);
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

#[test]
fn a_command_that_works_clears_an_old_reason() {
    let mut d = Driver::launch();
    let first = d.v1_items()[0].clone();
    d.exec("playhead.set", json!({"time": first["start"].as_i64().unwrap() + first["duration"].as_i64().unwrap() / 2}));
    d.h.state_mut().ui.status = "Chưa làm được: lần trước".into();
    d.exec("ayp.palette.open", json!({}));
    d.frames(3);
    d.ok("ui.type", json!({"text": "tach"}));
    d.ok("ui.key", json!({"key": "Enter"}));
    assert_eq!(d.h.state().ui.status, "");
}

#[test]
fn typing_does_not_trigger_editor_shortcuts() {
    let mut d = Driver::launch();
    d.exec("edit.deselectAll", json!({}));
    let tool = d.h.state().ui.tool;
    d.exec("ayp.palette.open", json!({}));
    d.frames(3);
    // `c` is the Razor tool's key and `v` the Selection tool's
    d.ok("ui.type", json!({"text": "cv"}));
    assert_eq!(d.h.state().ui.tool, tool);
    assert_eq!(d.h.state().ui.ayp_palette.as_deref(), Some("cv"));
}

#[test]
fn enter_on_nothing_does_nothing_and_a_disabled_hit_says_why() {
    let mut d = Driver::launch();
    d.exec("ayp.palette.open", json!({}));
    d.frames(3);
    d.ok("ui.key", json!({"key": "Enter"}));
    assert!(d.has("ayp.palette"), "Enter with no text keeps the finder open");
    d.ok("ui.type", json!({"text": "phu de tieng viet"}));
    d.ok("ui.key", json!({"key": "Enter"}));
    assert!(d.has("ayp.palette"));
    assert!(d.h.state().ui.status.starts_with("Chưa làm được:"), "{}", d.h.state().ui.status);
}

/// `AYP_PALETTE_PNG=/path.png cargo test --test ayp_palette_ui snapshot -- --ignored` writes a picture of the open finder.
#[test]
#[ignore]
fn snapshot() {
    let Ok(path) = std::env::var("AYP_PALETTE_PNG") else { return };
    let mut d = Driver::launch();
    let items = d.v1_items();
    let first = &items[0];
    d.exec("playhead.set", json!({"time": first["start"].as_i64().unwrap() + first["duration"].as_i64().unwrap() / 2}));
    d.exec("ayp.palette.open", json!({}));
    d.frames(3);
    d.ok("ui.type", json!({"text": "phu de"}));
    d.frames(3);
    d.h.render().expect("render").save(path).unwrap();
}
