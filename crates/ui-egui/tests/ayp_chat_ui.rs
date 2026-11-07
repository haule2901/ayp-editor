//! The AYP AI chat panel: sending, cards, plan buttons, Chỉ chỗ, the read-only guard.

use std::collections::VecDeque;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::control::ControlRequest;
use filmcraft_ui_egui::{FilmcraftApp, HttpPost, HttpResponse};
use serde_json::{Value, json};

type Log = Arc<Mutex<Vec<Value>>>;
/// Replies the fake Claude gives, in order. `None` = wait for `Driver::release` first.
type Script = Arc<Mutex<VecDeque<(Option<std::sync::mpsc::Receiver<()>>, HttpResponse)>>>;

struct Driver {
    h: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    log: Log,
    script: Script,
}

fn reply(stop: &str, content: Value) -> HttpResponse {
    let body = json!({"id": "msg", "type": "message", "role": "assistant", "content": content, "stop_reason": stop});
    HttpResponse { status: 200, retry_after: None, body: body.to_string() }
}

fn text(t: &str) -> Value {
    json!({"type": "text", "text": t})
}

fn tool_use(id: &str, name: &str, input: Value) -> Value {
    json!({"type": "tool_use", "id": id, "name": name, "input": input})
}

fn run_cmd(id: &str, cmd: &str) -> Value {
    tool_use(id, "run_command", json!({"id": cmd, "params": {}}))
}

fn add_track(id: &str) -> Value {
    tool_use(id, "run_command", json!({"id": "sequence.addTracks", "params": {"video": 1, "audio": 0}}))
}

impl Driver {
    fn launch(replies: Vec<HttpResponse>) -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("aypchat-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        session.prefs_path = Some(dir.join("preferences.json"));
        let (tx, rx) = channel();
        let mut app = FilmcraftApp::new(session).with_control(rx);
        app.ayp_default = true;
        let log: Log = Default::default();
        let script: Script = Arc::new(Mutex::new(replies.into_iter().map(|r| (None, r)).collect()));
        let (l, s) = (log.clone(), script.clone());
        let post: HttpPost = Arc::new(move |req| {
            l.lock().unwrap().push(serde_json::from_str(&req.body).unwrap());
            let next = s.lock().unwrap().pop_front();
            match next {
                Some((gate, r)) => {
                    if let Some(gate) = gate {
                        let _ = gate.recv();
                    }
                    Ok(r)
                }
                None => Ok(HttpResponse { status: 400, retry_after: None, body: r#"{"type":"error","error":{"message":"no more replies"}}"#.into() }),
            }
        });
        app.hooks.http_post = Some(post);
        app.hooks.secret_get = Some(Box::new(|k| (k == "anthropic").then(|| "test-key".to_string())));
        let h = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
        let mut d = Driver { h, tx, log, script };
        d.frames(4);
        d.click("ayp.side.ai");
        d
    }

    /// The next reply waits until the returned sender fires (or is dropped).
    fn hold_next(&mut self) -> Sender<()> {
        let (tx, rx) = channel();
        if let Some(front) = self.script.lock().unwrap().front_mut() {
            front.0 = Some(rx);
        }
        tx
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

    fn click(&mut self, id: &str) {
        let v = self.call("ui.click", json!({"id": id}));
        assert_eq!(v["ok"], json!(true), "{id}: {v}");
        self.frames(3);
    }

    fn key(&mut self, key: &str) {
        let v = self.call("ui.key", json!({"key": key}));
        assert_eq!(v["ok"], json!(true), "{key}: {v}");
        self.frames(3);
    }

    fn type_into(&mut self, id: &str, text: &str) {
        self.click(id);
        let v = self.call("ui.type", json!({"text": text}));
        assert_eq!(v["ok"], json!(true), "{v}");
        self.frames(2);
    }

    fn has(&self, id: &str) -> bool {
        self.h.state().auto.query(id).iter().any(|e| e.id == id)
    }

    fn ids(&self, prefix: &str) -> Vec<String> {
        self.h.state().auto.query(prefix).iter().map(|e| e.id.clone()).collect()
    }

    fn label(&self, id: &str) -> Option<String> {
        self.h.state().auto.query(id).iter().find(|e| e.id == id).map(|e| e.label.clone())
    }

    /// Frames (1 ms apart, at most ~3 s) until the chat is not busy.
    fn settle(&mut self) {
        for _ in 0..3000 {
            if !self.h.state().ayp_chat.busy() {
                self.frames(3);
                return;
            }
            self.frames(1);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("never settled: {:?}", self.h.state().ayp_chat.cards);
    }

    fn requests(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    fn tracks(&mut self) -> usize {
        let v = self.call("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
        v["result"]["video"].as_array().map_or(0, Vec::len)
    }
}

#[test]
fn chat_panel_sends_and_shows_cards() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Xin chào anh")]))]);
    for id in ["ayp.chat.input", "ayp.chat.send", "ayp.chat.settings"] {
        assert!(d.has(id), "{id} is on the AI tab");
    }
    assert!(!d.has("ayp.chat.stop"), "Dừng only shows while the AI works");
    d.type_into("ayp.chat.input", "Chào em");
    d.click("ayp.chat.send");
    d.settle();
    assert_eq!(d.requests(), 1);
    assert_eq!(d.label("ayp.chat.card.0").as_deref(), Some("Chào em"));
    assert_eq!(d.label("ayp.chat.card.1").as_deref(), Some("Xin chào anh"));
    assert!(d.h.state().ayp_chat.input.is_empty(), "the box is emptied once sent");
    assert!(d.h.state().ui_error.is_none(), "{:?}", d.h.state().ui_error);
}

#[test]
fn plan_buttons_drive_the_turn() {
    let plan = |id: &str| tool_use(id, "propose_plan", json!({"title": "Thêm track", "steps": ["Thêm một track video"]}));
    let mut d = Driver::launch(vec![
        reply("tool_use", json!([plan("p1")])),
        reply("end_turn", json!([text("Mình bỏ kế hoạch rồi")])),
        reply("tool_use", json!([plan("p2")])),
        reply("tool_use", json!([add_track("t1")])),
        reply("end_turn", json!([text("Xong")])),
    ]);
    let tracks = d.tracks();
    d.type_into("ayp.chat.input", "Thêm một track video");
    d.click("ayp.chat.send");
    d.settle();
    for id in ["ayp.chat.plan.approve", "ayp.chat.plan.edit", "ayp.chat.plan.reject"] {
        assert!(d.has(id), "{id}");
    }
    // Sửa: the words to change go in the box, the plan stays waiting
    d.click("ayp.chat.plan.edit");
    assert_eq!(d.h.state().ayp_chat.input, "Sửa kế hoạch: ");
    assert_eq!(d.requests(), 1);
    // Bỏ: Claude is told, the plan card is closed
    d.click("ayp.chat.plan.reject");
    d.settle();
    assert_eq!(d.requests(), 2);
    assert!(!d.has("ayp.chat.plan.approve"), "a decided plan has no buttons");
    // a new plan, then Làm: the turn runs and one button takes it all back
    d.type_into("ayp.chat.input", "Làm lại đi");
    d.click("ayp.chat.send");
    d.settle();
    d.click("ayp.chat.plan.approve");
    d.settle();
    assert_eq!(d.requests(), 5);
    assert_eq!(d.tracks(), tracks + 1);
    let undo = d.ids("ayp.chat.undoTurn.");
    assert_eq!(undo.len(), 1, "{:?}", d.h.state().ayp_chat.cards);
    d.click(&undo[0]);
    assert_eq!(d.tracks(), tracks, "Hoàn tác cả lượt");
}

#[test]
fn place_opens_the_tab_and_flashes_the_item() {
    let mut d = Driver::launch(vec![reply("tool_use", json!([run_cmd("t1", "markers.add"), add_track("t2")])), reply("end_turn", json!([text("Xong")]))]);
    assert_eq!(d.h.state().ui.ayp_tab, "media");
    d.type_into("ayp.chat.input", "Thêm marker và một track");
    d.click("ayp.chat.send");
    d.settle();
    let places = d.ids("ayp.chat.place.");
    assert_eq!(places.len(), 2, "{places:?}");
    // markers.add has a button in Cắt: its tab opens and the button is outlined
    d.click(&places[0]);
    assert_eq!(d.h.state().ui.ayp_tab, "cut");
    let state = d.call("ui.ayp.state", json!({}));
    assert_eq!(state["result"]["flash"], "markers.add", "{state}");
    assert!(d.has("ayp.item.markers.add"));
    // 1.5 s later the outline is gone
    std::thread::sleep(std::time::Duration::from_millis(1600));
    d.frames(2);
    let state = d.call("ui.ayp.state", json!({}));
    assert!(state["result"]["flash"].is_null(), "{state}");
    // adding a track has no button: Tìm lệnh opens with its name typed in
    d.click(&places[1]);
    assert_eq!(d.h.state().ui.ayp_palette.as_deref(), Some("Thêm track…"));
    assert!(d.has("ayp.palette"));
}

#[test]
fn edits_are_refused_while_ai_works() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Xong")]))]);
    let gate = d.hold_next();
    d.type_into("ayp.chat.input", "Làm gì đó");
    d.click("ayp.chat.send");
    assert!(d.h.state().ayp_chat.busy());
    assert!(d.has("ayp.chat.stop"));
    // an edit over control is refused, a read is not
    let v = d.call("engine.execute", json!({"command": "sequence.addTracks", "params": {"video": 1, "audio": 0}}));
    assert_eq!(v["ok"], json!(false), "{v}");
    assert!(v["error"].as_str().unwrap_or_default().contains("AI đang làm"), "{v}");
    let v = d.call("engine.execute", json!({"command": "sequence.inspect", "params": {}}));
    assert_eq!(v["ok"], json!(true), "{v}");
    // the pointer cannot reach the rest of the window either
    d.click("ayp.tab.cut");
    assert_eq!(d.h.state().ui.ayp_tab, "media", "a click outside the AI column lands on the cover");
    gate.send(()).unwrap();
    d.settle();
    let v = d.call("engine.execute", json!({"command": "sequence.addTracks", "params": {"video": 1, "audio": 0}}));
    assert_eq!(v["ok"], json!(true), "edits are back once the AI is done: {v}");

    // Dừng ends the turn while a reply is still on its way
    d.script.lock().unwrap().push_back((None, reply("end_turn", json!([text("muộn")]))));
    let _gate = d.hold_next();
    d.type_into("ayp.chat.input", "Lại nữa");
    d.click("ayp.chat.send");
    assert!(d.h.state().ayp_chat.busy());
    d.click("ayp.chat.stop");
    assert!(!d.h.state().ayp_chat.busy());
    assert!(!d.has("ayp.chat.stop"));
}

/// I1: while the AI works only the chat is free: not the Thông số tab either. Away from the chat
/// (another right tab, opened over MCP) the whole window is covered and a bar keeps Dừng in reach.
#[test]
fn only_the_chat_is_free_while_ai_works() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Xong")])), reply("end_turn", json!([text("Xong")]))]);
    let gate = d.hold_next();
    d.type_into("ayp.chat.input", "Làm gì đó");
    d.click("ayp.chat.send");
    assert!(d.h.state().ayp_chat.busy());
    d.click("ayp.side.props");
    assert_eq!(d.h.state().ui.ayp_side, filmcraft_ui_egui::state::AypSideTab::Ai, "the Thông số tab is under the cover");
    assert!(!d.has("ayp.chat.stopBar"), "the chat is on screen: no bar");
    gate.send(()).unwrap();
    d.settle();

    // a turn started over MCP while Thông số is open
    d.click("ayp.side.props");
    let _gate = d.hold_next();
    let v = d.call("ui.ayp.chat.send", json!({"text": "Làm tiếp"}));
    assert_eq!(v["ok"], json!(true), "{v}");
    d.frames(3);
    assert!(d.h.state().ayp_chat.busy());
    d.click("ayp.tab.cut");
    assert_eq!(d.h.state().ui.ayp_tab, "media", "everything is covered");
    d.click("ayp.chat.stopBar");
    assert!(!d.h.state().ayp_chat.busy(), "Dừng on the bar stops the turn");
    assert!(!d.h.state().session.edit_locked());
}

/// I4: Làm / Sửa / Bỏ stay on the plan when cards follow it: edits before the plan in the same reply,
/// or an error card from pressing Làm offline.
#[test]
fn plan_buttons_survive_cards_after_the_plan() {
    let plan = tool_use("p1", "propose_plan", json!({"title": "Thêm track", "steps": ["Thêm một track video"]}));
    let mut d = Driver::launch(vec![reply("tool_use", json!([add_track("t1"), plan]))]);
    d.type_into("ayp.chat.input", "Thêm track rồi lập kế hoạch");
    d.click("ayp.chat.send");
    d.settle();
    assert!(d.ids("ayp.chat.undoTurn.").len() == 1, "a Result card follows the plan: {:?}", d.h.state().ayp_chat.cards);
    for id in ["ayp.chat.plan.approve", "ayp.chat.plan.edit", "ayp.chat.plan.reject"] {
        assert!(d.has(id), "{id}");
    }
    d.h.state_mut().hooks.http_post = None;
    d.click("ayp.chat.plan.approve");
    assert!(matches!(d.h.state().ayp_chat.cards.last(), Some(filmcraft_ui_egui::ayp::chat::Card::Error(_))));
    assert!(d.has("ayp.chat.plan.approve"), "offline: the plan still waits, its buttons stay");
}

/// I5: Trò chuyện mới, as a button and over control.
#[test]
fn new_conversation_button_and_method() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Một")])), reply("end_turn", json!([text("Hai")]))]);
    d.type_into("ayp.chat.input", "Một");
    d.click("ayp.chat.send");
    d.settle();
    assert!(d.has("ayp.chat.new"));
    d.click("ayp.chat.new");
    assert!(d.h.state().ayp_chat.cards.is_empty());
    d.type_into("ayp.chat.input", "Hai");
    d.click("ayp.chat.send");
    d.settle();
    let v = d.call("ui.ayp.chat.new", json!({}));
    assert_eq!(v["ok"], json!(true), "{v}");
    assert!(d.h.state().ayp_chat.cards.is_empty());
    assert_eq!(d.log.lock().unwrap()[1]["messages"].as_array().unwrap().len(), 1, "the second request forgot the first");
}

/// M7: an IME commit and Enter in the same frame send the committed text and empty the box.
#[test]
fn ime_commit_and_enter_in_one_frame_send_the_text() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Ừ")]))]);
    d.type_into("ayp.chat.input", "xin ");
    d.h.input_mut().events.push(egui::Event::Ime(egui::ImeEvent::Preedit { text: "chào".into(), active_range_chars: None }));
    d.frames(2);
    d.h.input_mut().events.push(egui::Event::Ime(egui::ImeEvent::Commit("chào".into())));
    d.h.input_mut().events.push(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
    d.frames(2);
    d.settle();
    let sent = d.h.state().ayp_chat.cards.first().cloned();
    assert!(matches!(&sent, Some(filmcraft_ui_egui::ayp::chat::Card::User(t)) if t.ends_with("chào")), "{sent:?}");
    assert!(d.h.state().ayp_chat.input.is_empty(), "the box is emptied");
}

#[test]
fn ai_items_open_the_chat() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Anh gửi bản chép lời nhé")]))]);
    d.click("ayp.side.props");
    d.click("ayp.tab.captions");
    d.click("ayp.item.transcript.captionsFromChunks");
    assert_eq!(d.h.state().ui.ayp_side, filmcraft_ui_egui::state::AypSideTab::Ai);
    d.settle();
    assert_eq!(d.requests(), 1);
    assert_eq!(d.label("ayp.chat.card.0").as_deref(), Some("Chia cụm phụ đề theo nghĩa (AI)"));
}

#[test]
fn enter_during_ime_does_not_send() {
    let mut d = Driver::launch(vec![reply("end_turn", json!([text("Ừ")]))]);
    d.click("ayp.chat.input");
    // a Vietnamese IME is composing "xin": Enter belongs to the IME
    d.h.input_mut().events.push(egui::Event::Ime(egui::ImeEvent::Preedit { text: "xin".into(), active_range_chars: None }));
    d.frames(2);
    d.key("Enter");
    assert_eq!(d.requests(), 0, "composing: Enter does not send");
    assert!(d.h.state().ayp_chat.cards.is_empty());
    // committed: now Enter sends
    d.h.input_mut().events.push(egui::Event::Ime(egui::ImeEvent::Commit("xin".into())));
    d.frames(2);
    d.key("Enter");
    d.settle();
    assert_eq!(d.requests(), 1);
    assert!(matches!(d.h.state().ayp_chat.cards.first(), Some(filmcraft_ui_egui::ayp::chat::Card::User(_))));
}

#[test]
fn the_key_goes_to_the_keychain_and_nowhere_else() {
    let mut d = Driver::launch(vec![]);
    let saved: Arc<Mutex<Vec<(String, String)>>> = Default::default();
    let s = saved.clone();
    d.h.state_mut().hooks.secret_set = Some(Box::new(move |a, v| {
        s.lock().unwrap().push((a.to_string(), v.to_string()));
        Ok(())
    }));
    assert!(!d.has("ayp.chat.key"), "settings start closed");
    d.click("ayp.chat.settings");
    for id in ["ayp.chat.key", "ayp.chat.model", "ayp.chat.saveKey"] {
        assert!(d.has(id), "{id}");
    }
    d.type_into("ayp.chat.key", "sk-ant-secret-123");
    d.click("ayp.chat.saveKey");
    assert_eq!(*saved.lock().unwrap(), vec![("anthropic".to_string(), "sk-ant-secret-123".to_string())]);
    let state = d.h.state();
    assert!(state.ayp_chat.key_field.is_empty(), "the field is cleared");
    let everywhere = format!("{:?}{}{:?}", state.auto.previous, state.ui.status, state.ayp_chat.cards);
    assert!(!everywhere.contains("sk-ant-secret-123"), "the key shows nowhere");
    assert!(everywhere.contains("Đã lưu khoá"));
}

/// The model is remembered in aypeditor.json only by AYPEditor; plain FilmCraft never creates the file.
#[test]
fn only_aypeditor_remembers_the_model() {
    for ayp in [false, true] {
        let dir = std::env::temp_dir().join(format!("aypmodel-{}-{ayp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = Session::default();
        session.prefs_path = Some(dir.join("preferences.json"));
        let mut app = FilmcraftApp::new(session);
        app.ayp_default = ayp;
        filmcraft_ui_egui::ayp::chat::set_model(&mut app, "claude-test-1");
        assert_eq!(app.ayp_chat.model, "claude-test-1");
        assert_eq!(dir.join("aypeditor.json").exists(), ayp);
    }
}

/// `AYP_CHAT_PNG=/path.png cargo test --test ayp_chat_ui snapshot -- --ignored` writes a picture of the window.
#[test]
#[ignore]
fn snapshot() {
    let Ok(path) = std::env::var("AYP_CHAT_PNG") else { return };
    let plan = tool_use("p1", "propose_plan", json!({"title": "Thêm marker", "steps": ["Thêm marker", "Thêm một track"]}));
    let mut d = Driver::launch(vec![
        reply("tool_use", json!([text("Mình thêm marker rồi một track."), run_cmd("t1", "markers.add"), add_track("t2")])),
        reply("end_turn", json!([text("Xong rồi anh.")])),
        reply("tool_use", json!([plan])),
    ]);
    d.type_into("ayp.chat.input", "Thêm marker và một track");
    d.click("ayp.chat.send");
    d.settle();
    d.type_into("ayp.chat.input", "Lập kế hoạch");
    d.click("ayp.chat.send");
    d.settle();
    d.frames(3);
    d.h.render().expect("render").save(path).unwrap();
}
