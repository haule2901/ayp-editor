//! Control methods that let an AI read and drive the AYP interface.

use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::{ControlRequest, Outcome, handle};
use serde_json::{Value, json};

fn app() -> (FilmcraftApp, egui::Context) {
    let mut session = Session::default();
    session.execute("file.openDemoProject", json!({})).expect("demo project");
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("aypctl-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    session.prefs_path = Some(dir.join("preferences.json"));
    let mut app = FilmcraftApp::new(session);
    app.ayp_default = true;
    (app, egui::Context::default())
}

fn call(app: &mut FilmcraftApp, ctx: &egui::Context, method: &str, params: Value) -> Value {
    let (req, _) = ControlRequest::new(method, params);
    let Outcome::Done(v) = handle(app, ctx, &req) else { panic!("{method} must answer at once") };
    v
}

#[test]
fn state_and_tabs_have_the_documented_shape() {
    let (mut app, ctx) = app();
    let s = call(&mut app, &ctx, "ui.ayp.state", json!({}));
    assert_eq!(s["ok"], true, "{s}");
    let r = &s["result"];
    for k in ["workspace", "tab", "side", "palette", "selection", "playhead", "sequence"] {
        assert!(r.get(k).is_some(), "missing {k}: {r}");
    }
    assert_eq!(r["side"], "props");
    assert!(r["palette"].is_null());
    assert!(r["selection"].is_array());

    let t = call(&mut app, &ctx, "ui.ayp.tabs", json!({}));
    let tabs = t["result"].as_array().expect("array");
    assert_eq!(tabs.len(), 10);
    let first = &tabs[0]["items"][0];
    for k in ["key", "cmd", "params", "label", "kind"] {
        assert!(first.get(k).is_some(), "missing {k}: {first}");
    }
    assert!(first["params"].is_object());
    assert!(tabs.iter().flat_map(|t| t["items"].as_array().unwrap()).any(|i| i["kind"] == "ai"));
}

#[test]
fn open_switches_tab_and_side_and_rejects_unknown() {
    let (mut app, ctx) = app();
    let o = call(&mut app, &ctx, "ui.ayp.open", json!({"tab": "captions", "side": "ai"}));
    assert_eq!(o["ok"], true, "{o}");
    let r = call(&mut app, &ctx, "ui.ayp.state", json!({}))["result"].clone();
    assert_eq!((r["tab"].as_str(), r["side"].as_str(), r["workspace"].as_str()), (Some("captions"), Some("ai"), Some("AYP")));

    // from another workspace it switches back to AYP
    let o = call(&mut app, &ctx, "ui.set", json!({"workspace": "Editing"}));
    assert_eq!(o["ok"], true, "{o}");
    call(&mut app, &ctx, "ui.ayp.open", json!({"tab": "media"}));
    let r = call(&mut app, &ctx, "ui.ayp.state", json!({}))["result"].clone();
    assert_eq!((r["tab"].as_str(), r["workspace"].as_str()), (Some("media"), Some("AYP")));

    for bad in [json!({"tab": "nope"}), json!({"side": "nope"})] {
        let v = call(&mut app, &ctx, "ui.ayp.open", bad);
        assert_eq!(v["ok"], false, "{v}");
    }
    let r2 = call(&mut app, &ctx, "ui.ayp.state", json!({}))["result"].clone();
    assert_eq!(r2["tab"], "media");
}
