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

#[test]
fn run_clicks_an_item_like_the_button_and_rejects_unknown() {
    let (mut app, ctx) = app();
    let before = app.session.history.undo.len();
    let v = call(&mut app, &ctx, "ui.ayp.run", json!({"key": "markers.add"}));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(app.session.history.undo.len(), before + 1, "one click, one undo step");
    for bad in [json!({"key": "nope"}), json!({})] {
        let v = call(&mut app, &ctx, "ui.ayp.run", bad);
        assert_eq!(v["ok"], false, "{v}");
    }
}

fn chat_app(replies: Vec<serde_json::Value>) -> (FilmcraftApp, egui::Context) {
    use filmcraft_ui_egui::{HttpPost, HttpResponse};
    use std::sync::{Arc, Mutex};
    let (mut app, ctx) = app();
    let q = Arc::new(Mutex::new(std::collections::VecDeque::from(replies)));
    let post: HttpPost = Arc::new(move |_| {
        let content = q.lock().unwrap().pop_front().unwrap_or(json!([{"type": "text", "text": "hết"}]));
        let stop = if content.to_string().contains("tool_use") { "tool_use" } else { "end_turn" };
        let body = json!({"id": "m", "type": "message", "role": "assistant", "content": content, "stop_reason": stop});
        Ok(HttpResponse { status: 200, retry_after: None, body: body.to_string() })
    });
    app.hooks.http_post = Some(post);
    app.hooks.secret_get = Some(Box::new(|k| (k == "anthropic").then(|| "sk-ant-SECRET-test-key".to_string())));
    (app, ctx)
}

fn settle(app: &mut FilmcraftApp, ctx: &egui::Context) {
    for _ in 0..3000 {
        filmcraft_ui_egui::ayp::chat::poll(app, ctx);
        if !app.ayp_chat.busy() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("chat never settled");
}

#[test]
fn chat_methods_send_stop_and_report_state() {
    let (mut app, ctx) = chat_app(vec![json!([{"type": "text", "text": "Xin chào anh"}])]);
    let s = call(&mut app, &ctx, "ui.ayp.chat.state", json!({}))["result"].clone();
    assert_eq!((s["busy"].as_bool(), s["model"].as_str()), (Some(false), Some("claude-sonnet-5-5")));
    assert_eq!(s["cards"], json!([]));

    let v = call(&mut app, &ctx, "ui.ayp.chat.send", json!({"text": "Chào em"}));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(call(&mut app, &ctx, "ui.ayp.chat.state", json!({}))["result"]["busy"], true);
    // a second message while the turn runs is refused
    let v = call(&mut app, &ctx, "ui.ayp.chat.send", json!({"text": "nữa"}));
    assert_eq!(v["ok"], false, "{v}");
    settle(&mut app, &ctx);
    let s = call(&mut app, &ctx, "ui.ayp.chat.state", json!({}))["result"].clone();
    assert_eq!(s["busy"], false);
    assert_eq!(s["cards"][0], json!({"User": "Chào em"}), "{s}");
    assert_eq!(s["cards"][1], json!({"Text": "Xin chào anh"}), "{s}");

    for bad in [json!({}), json!({"text": "  "})] {
        assert_eq!(call(&mut app, &ctx, "ui.ayp.chat.send", bad)["ok"], false);
    }
    assert_eq!(call(&mut app, &ctx, "ui.ayp.chat.stop", json!({}))["ok"], true, "stop when idle is harmless");
}

#[test]
fn chat_state_never_contains_the_key() {
    let (mut app, ctx) = chat_app(vec![json!([{"type": "text", "text": "ok"}])]);
    call(&mut app, &ctx, "ui.ayp.chat.send", json!({"text": "Chào"}));
    settle(&mut app, &ctx);
    app.ayp_chat.key_field = "sk-ant-SECRET-typed".into();
    let all = [call(&mut app, &ctx, "ui.ayp.chat.state", json!({})).to_string(), call(&mut app, &ctx, "ui.ayp.state", json!({})).to_string()];
    for a in all {
        assert!(!a.contains("SECRET"), "{a}");
    }
}

#[test]
fn plan_choice_and_undo_methods_need_something_to_answer() {
    let plan = json!([{"type": "tool_use", "id": "p1", "name": "propose_plan", "input": {"title": "T", "steps": ["a"]}}]);
    let add = json!([{"type": "tool_use", "id": "t1", "name": "run_command", "input": {"id": "markers.add", "params": {}}}]);
    let (mut app, ctx) = chat_app(vec![plan, add, json!([{"type": "text", "text": "Xong"}])]);
    // nothing waits yet
    for (m, p) in [("ui.ayp.chat.plan", json!({"approve": true})), ("ui.ayp.chat.choose", json!({"option": 0})), ("ui.ayp.chat.undoTurn", json!({"card": 0}))] {
        assert_eq!(call(&mut app, &ctx, m, p)["ok"], false, "{m}");
    }
    // missing / wrong-typed params
    for (m, p) in [("ui.ayp.chat.plan", json!({})), ("ui.ayp.chat.choose", json!({})), ("ui.ayp.chat.undoTurn", json!({"card": "x"}))] {
        assert_eq!(call(&mut app, &ctx, m, p)["ok"], false, "{m}");
    }
    call(&mut app, &ctx, "ui.ayp.chat.send", json!({"text": "Thêm marker"}));
    settle(&mut app, &ctx);
    let before = app.session.history.undo.len();
    let v = call(&mut app, &ctx, "ui.ayp.chat.plan", json!({"approve": true}));
    assert_eq!(v["ok"], true, "{v}");
    settle(&mut app, &ctx);
    assert_eq!(app.session.history.undo.len(), before + 1, "the turn is one undo step");
    let s = call(&mut app, &ctx, "ui.ayp.chat.state", json!({}))["result"].clone();
    let i = s["cards"].as_array().unwrap().iter().position(|c| c.get("Result").is_some()).expect("a Result card");
    let v = call(&mut app, &ctx, "ui.ayp.chat.undoTurn", json!({"card": i}));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(app.session.history.undo.len(), before);
}
