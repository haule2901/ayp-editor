//! The AYP left panel: ten tabs, one-click buttons, Vietnamese failure reasons.

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
        let dir = std::env::temp_dir().join(format!("ayplib-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
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

    fn undo_len(&self) -> usize {
        self.h.state().session.history.undo.len()
    }

    fn select_first_clip(&mut self) {
        let v = self.exec("sequence.inspect", json!({}));
        let id = v["result"]["video"][0]["items"][0]["clip"].clone();
        self.exec("timeline.select", json!({"clips": [id]}));
        self.frames(2);
    }
}

#[test]
fn tabs_are_listed_and_switch() {
    let mut d = Driver::launch(true);
    assert_eq!(d.h.state().auto.query("ayp.tab.").len(), 10);
    d.click("ayp.tab.captions");
    assert_eq!(d.h.state().ui.ayp_tab, "captions");
    assert!(d.has("ayp.item.transcript.createCaptionsVi"));
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

#[test]
fn clicking_an_item_runs_its_command() {
    let mut d = Driver::launch(true);
    d.select_first_clip();
    d.click("ayp.tab.effects");
    let before = d.undo_len();
    d.click("ayp.item.effects.apply.gaussian_blur");
    assert_eq!(d.undo_len(), before + 1, "status: {}", d.h.state().ui.status);
}

#[test]
fn a_failing_command_says_why_in_vietnamese() {
    let mut d = Driver::launch(true);
    d.exec("edit.deselectAll", json!({}));
    d.click("ayp.tab.effects");
    let before = d.undo_len();
    d.click("ayp.item.effects.apply.gaussian_blur");
    assert!(d.h.state().ui.status.starts_with("Chưa làm được:"), "{}", d.h.state().ui.status);
    assert_eq!(d.undo_len(), before);
}

#[test]
fn panel_items_only_point_at_the_panel_and_ai_items_do_nothing() {
    let mut d = Driver::launch(true);
    d.click("ayp.tab.captions");
    let before = d.undo_len();
    d.click("ayp.item.captions.applyPreset");
    assert!(d.h.state().ui.status.contains("bên dưới"), "{}", d.h.state().ui.status);
    d.h.state_mut().ui.status.clear();
    d.click("ayp.item.transcript.captionsFromChunks");
    assert!(d.h.state().ui.status.is_empty(), "{}", d.h.state().ui.status);
    assert_eq!(d.undo_len(), before);
}

#[test]
fn no_open_sequence_shows_a_hint() {
    let d = Driver::launch(false);
    assert!(d.has("ayp.hint.noSequence"));
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

/// `AYP_LIB_PNG=/path.png cargo test --test ayp_library_ui snapshot -- --ignored` writes a picture of the window.
#[test]
#[ignore]
fn snapshot() {
    let Ok(path) = std::env::var("AYP_LIB_PNG") else { return };
    let mut d = Driver::launch(true);
    d.select_first_clip();
    d.click("ayp.tab.effects");
    d.frames(3);
    d.h.render().expect("render").save(path).unwrap();
}
