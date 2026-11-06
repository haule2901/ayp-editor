//! AYPEditor's "AYP" workspace: opened by default on the first launch only, the switch to the
//! classic layout and back, and the autosave of the AYP layout.

use std::path::Path;
use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use filmcraft_ui_egui::dock::PanelKind;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn launch(data: &Path, ayp_default: bool) -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        session.prefs_path = Some(data.join("preferences.json"));
        let (tx, rx) = channel();
        let mut app = FilmcraftApp::new(session).with_control(rx);
        app.ayp_default = ayp_default;
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

    fn menu(&mut self, id: &str) {
        let (req, reply) = ControlRequest::new("ui.menu.invoke", json!({"id": id, "params": {}}));
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                assert_eq!(v["ok"], json!(true), "{id}: {v}");
                return;
            }
        }
        panic!("no reply to {id}");
    }

    fn workspace(&self) -> String {
        self.harness.state().ui.workspace.clone()
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("aypws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ayp_default_opens_ayp() {
    let data = tmp("default");
    let d = Driver::launch(&data, true);
    assert_eq!(d.workspace(), "AYP");
    for p in [PanelKind::AypLibrary, PanelKind::Program, PanelKind::AypSide, PanelKind::AypToolbar, PanelKind::Timeline] {
        assert!(d.harness.state().ui.dock.contains(p), "{p:?}");
    }
    let marker: Value = serde_json::from_slice(&std::fs::read(data.join("aypeditor.json")).unwrap()).unwrap();
    assert_eq!(marker["introduced"], json!(true));
    assert!(d.harness.state().ui_error.is_none(), "{:?}", d.harness.state().ui_error);
}

fn current(data: &Path) -> Option<String> {
    let v: Value = serde_json::from_slice(&std::fs::read(data.join("workspaces.json")).ok()?).ok()?;
    v["current"].as_str().map(str::to_string)
}

fn marker(data: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(data.join("aypeditor.json")).unwrap()).unwrap()
}

/// FilmCraft AYP shares `workspaces.json`: its `current` must not decide where AYPEditor opens.
#[test]
fn ayp_editor_ignores_the_shared_current_of_filmcraft_ayp() {
    let data = tmp("shared");
    std::fs::write(data.join("aypeditor.json"), r#"{"introduced":true,"workspace":"AYP"}"#).unwrap();
    std::fs::write(data.join("workspaces.json"), r#"{"current":"Editing","saved":[]}"#).unwrap();
    let d = Driver::launch(&data, true);
    assert_eq!(d.workspace(), "AYP");
}

/// An old marker (no workspace yet) opens AYP, not FilmCraft AYP's last workspace.
#[test]
fn an_old_marker_opens_ayp() {
    let data = tmp("oldmarker");
    std::fs::write(data.join("aypeditor.json"), r#"{"introduced":true}"#).unwrap();
    std::fs::write(data.join("workspaces.json"), r#"{"current":"Editing","saved":[]}"#).unwrap();
    let d = Driver::launch(&data, true);
    assert_eq!(d.workspace(), "AYP");
}

/// AYPEditor keeps its own last workspace in `aypeditor.json` and never writes the shared `current`.
#[test]
fn ayp_editor_reopens_its_own_last_workspace() {
    let data = tmp("own");
    std::fs::write(data.join("aypeditor.json"), r#"{"introduced":true,"workspace":"AYP"}"#).unwrap();
    std::fs::write(data.join("workspaces.json"), r#"{"current":"Assembly","saved":[]}"#).unwrap();
    let mut d = Driver::launch(&data, true);
    assert_eq!(d.workspace(), "AYP");
    d.menu("window.ayp.advanced");
    assert_eq!(d.workspace(), "Editing");
    assert_eq!(marker(&data)["workspace"], json!("Editing"));
    assert_eq!(marker(&data)["introduced"], json!(true));
    assert_eq!(current(&data).as_deref(), Some("Assembly"), "the shared current is FilmCraft AYP's");
    drop(d);
    let d = Driver::launch(&data, true);
    assert_eq!(d.workspace(), "Editing");
}

/// Plain FilmCraft still reopens and writes the shared `current`.
#[test]
fn plain_filmcraft_uses_the_shared_current() {
    let data = tmp("plaincur");
    std::fs::write(data.join("workspaces.json"), r#"{"current":"Color","saved":[]}"#).unwrap();
    let mut d = Driver::launch(&data, false);
    assert_eq!(d.workspace(), "Color");
    d.menu("window.workspace.editing");
    assert_eq!(current(&data).as_deref(), Some("Editing"));
    assert!(!data.join("aypeditor.json").exists());
}

#[test]
fn advanced_and_simple_switch() {
    let data = tmp("switch");
    let mut d = Driver::launch(&data, true);
    d.menu("window.ayp.advanced");
    assert_eq!(d.workspace(), "Editing");
    d.menu("window.ayp.simple");
    assert_eq!(d.workspace(), "AYP");
}

#[test]
fn ayp_layout_changes_are_saved_once_after_settling() {
    let data = tmp("autosave");
    let mut d = Driver::launch(&data, true);
    let ws = data.join("workspaces.json");
    let ayp = |ws: &Path| -> Option<String> {
        let v: Value = serde_json::from_slice(&std::fs::read(ws).ok()?).ok()?;
        v["saved"].as_array()?.iter().find(|s| s["name"] == "AYP").map(|s| s["layout"].to_string())
    };
    d.frames(5);
    assert_eq!(ayp(&ws), None, "no change, no layout write");
    // several changes in a row: nothing is written while the layout keeps moving
    d.harness.state_mut().ui.dock.close(PanelKind::AypSide);
    d.frames(2);
    d.harness.state_mut().ui.dock.close(PanelKind::AypLibrary);
    d.frames(2);
    assert_eq!(ayp(&ws), None, "still unsettled");
    // settled: one write holding the final layout
    for _ in 0..200 {
        if ayp(&ws).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        d.frames(1);
    }
    let saved = ayp(&ws).expect("AYP layout saved");
    assert!(!saved.contains("AypSide") && !saved.contains("AypLibrary"), "{saved}");
}
