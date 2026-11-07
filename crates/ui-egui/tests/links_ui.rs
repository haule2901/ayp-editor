//! Headless UI test: AYPEditor has no community links. No Discord button in the header, no
//! Discord/website/GitHub/issue items in the Help menu, none in About, no Community section on
//! the Home (Import) screen.

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
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
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

    fn call(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                return v;
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params.clone());
        assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
        v["result"].clone()
    }

    fn ids(&mut self, prefix: &str) -> Vec<String> {
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().unwrap().iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }
}

#[test]
fn no_community_links_anywhere() {
    let mut d = Driver::demo();
    let gone = ["help.discord", "help.website", "help.appPage", "help.github", "help.reportIssue"];
    let menu = d.ok("ui.menu.list", json!({})).to_string();
    for id in gone {
        assert!(!menu.contains(id), "Help menu still lists {id}");
        let r = d.call("engine.execute", json!({"command": id, "params": {}}));
        assert_ne!(r["ok"], json!(true), "{id} still runs: {r}");
    }
    assert!(!menu.to_lowercase().contains("discord"), "no Discord in any menu");
    assert!(d.ids("header.discord").is_empty(), "header Discord button is gone");
    // About keeps version and licence, without link buttons.
    d.ok("engine.execute", json!({"command": "app.about", "params": {}}));
    d.frames(3);
    let about = d.ids("about.");
    assert!(about.iter().any(|a| a.starts_with("about.tab.")), "About is open: {about:?}");
    for id in ["about.discord", "about.website", "about.appPage", "about.github", "about.reportIssue"] {
        assert!(!about.iter().any(|a| a == id), "{id} still in About");
    }
    d.ok("ui.key", json!({"key": "Escape"}));
    d.frames(3);
    // Home screen: no Community section.
    d.ok("engine.execute", json!({"command": "mode.import", "params": {}}));
    d.frames(3);
    assert!(d.ids("import.link.").is_empty(), "Home has no community links");
    assert!(!d.ids("import.").is_empty(), "Home still shows its import cards");
}
