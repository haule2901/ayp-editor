use super::*;
use crate::ayp::library::tests::demo;
use crate::{HttpPost, HttpResponse};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<Value>>>;

/// A scripted Claude: every request body is logged (with the key it carried as `_key`) and answered
/// with the next reply; past the script it answers 400.
fn script(app: &mut FilmcraftApp, replies: Vec<HttpResponse>) -> Log {
    let log: Log = Default::default();
    let queue = Mutex::new(VecDeque::from(replies));
    let l = log.clone();
    let post: HttpPost = Arc::new(move |req| {
        let mut body: Value = serde_json::from_str(&req.body).unwrap();
        body["_key"] = json!(req.headers.iter().find(|(k, _)| k == "x-api-key").map(|(_, v)| v.clone()));
        l.lock().unwrap().push(body);
        Ok(queue.lock().unwrap().pop_front().unwrap_or_else(|| status(400)))
    });
    app.hooks.http_post = Some(post);
    app.hooks.secret_get = Some(Box::new(|k| (k == "anthropic").then(|| "test-key".to_string())));
    app.ayp_chat.env = |_| None;
    log
}

fn status(code: u16) -> HttpResponse {
    HttpResponse { status: code, retry_after: None, body: r#"{"type":"error","error":{"message":"no more replies"}}"#.into() }
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

fn run_cmd(id: &str, cmd: &str, params: Value) -> Value {
    tool_use(id, "run_command", json!({"id": cmd, "params": params}))
}

fn add_track(id: &str) -> Value {
    run_cmd(id, "sequence.addTracks", json!({"video": 1, "audio": 0}))
}

/// Poll (1 ms apart, at most ~2 s) until `done`.
fn until(app: &mut FilmcraftApp, ctx: &egui::Context, done: impl Fn(&FilmcraftApp) -> bool) {
    for _ in 0..2000 {
        if done(app) {
            return;
        }
        poll(app, ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("never settled; cards: {:?}", app.ayp_chat.cards);
}

fn settle(app: &mut FilmcraftApp, ctx: &egui::Context) {
    until(app, ctx, |a| !a.ayp_chat.busy());
}

fn messages(body: &Value) -> &Vec<Value> {
    body["messages"].as_array().unwrap()
}

fn last_user_content(body: &Value) -> Value {
    let last = messages(body).last().unwrap();
    assert_eq!(last["role"], "user");
    last["content"].clone()
}

fn result_card(app: &FilmcraftApp) -> Option<usize> {
    app.ayp_chat.cards.iter().rposition(|c| matches!(c, Card::Result { .. }))
}

#[test]
fn a_turn_runs_tools_and_is_one_undo_step() {
    let (mut app, ctx) = demo();
    let log = script(
        &mut app,
        vec![
            reply("tool_use", json!([text("Mình thêm hai track."), add_track("t1"), add_track("t2"), run_cmd("t3", "no.such.command", json!({}))])),
            reply("end_turn", json!([text("Xong rồi.")])),
        ],
    );
    let before = app.session.history.undo.len();
    let ask = "Thêm hai track video cho mình nhé, cảm ơn bạn rất nhiều";
    send(&mut app, ask).unwrap();
    assert!(app.ayp_chat.busy());
    assert_eq!(send(&mut app, "nữa"), Err("AI đang làm — bấm Dừng trước".to_string()));
    settle(&mut app, &ctx);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0]["_key"], "test-key");
    assert_eq!(log[0]["model"], app.ayp_chat.model.as_str());
    assert_eq!(log[0]["tools"], tools::schemas());
    assert_eq!(last_user_content(&log[0]), json!([text(ask)]));
    // all three results go back in one user message, in order; the failure is marked as an error
    let results = last_user_content(&log[1]);
    let ids: Vec<&str> = results.as_array().unwrap().iter().map(|r| r["tool_use_id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["t1", "t2", "t3"]);
    assert!(results[0].get("is_error").is_none() && results[0]["content"].is_string());
    assert_eq!(results[2]["is_error"], true);

    // the whole turn is one undo step
    let label = format!("AI: {}", ask.chars().take(40).collect::<String>());
    assert_eq!(app.session.history.undo.len(), before + 1);
    assert_eq!(app.session.history.undo.last().unwrap().0, label);

    let vi_add = crate::ayp::vi::vi(filmcraft_engine::find_command("sequence.addTracks").unwrap().label).to_string();
    let cards = &app.ayp_chat.cards;
    assert_eq!(cards[0], Card::User(ask.into()));
    assert!(cards.contains(&Card::Text("Mình thêm hai track.".into())) && cards.contains(&Card::Text("Xong rồi.".into())));
    let doing = cards.iter().find_map(|c| if let Card::Doing { steps } = c { Some(steps.clone()) } else { None }).unwrap();
    assert_eq!(doing, vec![(vi_add.clone(), Some(true)), (vi_add, Some(true)), ("no.such.command".into(), Some(false))]);
    assert_eq!(cards.iter().filter(|c| matches!(c, Card::Doing { .. })).count(), 1, "one Doing card per turn");
    assert_eq!(cards[result_card(&app).unwrap()], Card::Result { label, commands: vec!["sequence.addTracks".into()], undo_len: before + 1, undone: false });
}

#[test]
fn plan_waits_for_approval_then_continues() {
    let (mut app, ctx) = demo();
    let plan = json!({"title": "Thêm 4 track", "steps": ["Thêm track", "Kiểm tra"]});
    let log = script(
        &mut app,
        vec![
            reply("tool_use", json!([text("Kế hoạch đây."), tool_use("p1", "propose_plan", plan)])),
            reply("tool_use", json!([add_track("a1"), add_track("a2"), add_track("a3"), add_track("a4")])),
            reply("end_turn", json!([text("Đã làm xong.")])),
        ],
    );
    let before = app.session.history.undo.len();
    assert!(decide_plan(&mut app, true).is_err(), "no plan yet");
    send(&mut app, "Thêm bốn track video").unwrap();
    settle(&mut app, &ctx);
    assert_eq!(log.lock().unwrap().len(), 1);
    let plan_card = app.ayp_chat.cards.iter().position(|c| matches!(c, Card::Plan { .. })).unwrap();
    assert_eq!(
        app.ayp_chat.cards[plan_card],
        Card::Plan { title: "Thêm 4 track".into(), steps: vec!["Thêm track".into(), "Kiểm tra".into()], state: Decision::Pending }
    );
    // nothing is asked until the user decides
    for _ in 0..20 {
        poll(&mut app, &ctx);
    }
    assert_eq!(log.lock().unwrap().len(), 1);

    decide_plan(&mut app, true).unwrap();
    settle(&mut app, &ctx);
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 3);
    assert_eq!(last_user_content(&log[1]), json!([{"type": "tool_result", "tool_use_id": "p1", "content": "Người dùng duyệt"}]));
    assert!(matches!(&app.ayp_chat.cards[plan_card], Card::Plan { state: Decision::Approved, .. }));
    // the approved plan lifts the guard: the 4th edit is not held back
    let doing = app.ayp_chat.cards.iter().rev().find_map(|c| if let Card::Doing { steps } = c { Some(steps.clone()) } else { None }).unwrap();
    assert_eq!(doing.iter().map(|s| s.1).collect::<Vec<_>>(), [Some(true); 4]);
    assert_eq!(app.session.history.undo.len(), before + 1);
    assert_eq!(app.session.history.undo.last().unwrap().0, "AI: Thêm bốn track video");
    assert!(decide_plan(&mut app, true).is_err(), "already decided");
}

#[test]
fn choice_answer_goes_back_as_tool_result() {
    let (mut app, ctx) = demo();
    let options = json!([{"label": "Karaoke", "detail": "chữ sáng dần"}, {"label": "Tĩnh", "detail": "hiện cả câu"}]);
    let log = script(
        &mut app,
        vec![
            reply("tool_use", json!([tool_use("c1", "offer_choices", json!({"question": "Kiểu phụ đề?", "options": options}))])),
            reply("end_turn", json!([text("Được, kiểu Tĩnh.")])),
        ],
    );
    send(&mut app, "Làm phụ đề").unwrap();
    settle(&mut app, &ctx);
    let card = app.ayp_chat.cards.iter().position(|c| matches!(c, Card::Choice { .. })).unwrap();
    assert_eq!(
        app.ayp_chat.cards[card],
        Card::Choice {
            question: "Kiểu phụ đề?".into(),
            options: vec![("Karaoke".into(), "chữ sáng dần".into()), ("Tĩnh".into(), "hiện cả câu".into())],
            picked: None
        }
    );
    assert!(choose(&mut app, 5).is_err());
    assert!(decide_plan(&mut app, true).is_err(), "a choice is not a plan");
    choose(&mut app, 1).unwrap();
    settle(&mut app, &ctx);
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(last_user_content(&log[1]), json!([{"type": "tool_result", "tool_use_id": "c1", "content": "Tĩnh"}]));
    assert!(matches!(app.ayp_chat.cards[card], Card::Choice { picked: Some(1), .. }));
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Text("Được, kiểu Tĩnh.".into())));
    assert!(choose(&mut app, 0).is_err(), "already chosen");
}

#[test]
fn missing_key_and_401_end_the_turn_with_a_clear_card() {
    let (mut app, ctx) = demo();
    let undo = app.session.history.undo.len();
    let dirty = app.session.is_dirty();
    assert!(send(&mut app, "   ").is_err(), "nothing to send");

    // the web app: no network hook
    app.ayp_chat.env = |_| Some("env-key".into());
    send(&mut app, "xin chào").unwrap();
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Error("App này chưa nối mạng cho AI".into())));
    assert!(!app.ayp_chat.busy());

    // network, but no key in the keychain or the environment
    let log = script(&mut app, vec![status(401), reply("end_turn", json!([text("chào")]))]);
    app.hooks.secret_get = Some(Box::new(|_| None));
    send(&mut app, "xin chào").unwrap();
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Error(api::ApiError::NoKey.vi())));
    assert!(!app.ayp_chat.busy());
    assert!(log.lock().unwrap().is_empty(), "no request without a key");

    // a key Claude does not accept
    app.hooks.secret_get = Some(Box::new(|_| Some("wrong-key".into())));
    send(&mut app, "xin chào").unwrap();
    settle(&mut app, &ctx);
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Error(api::ApiError::Auth.vi())));
    assert_eq!(log.lock().unwrap().len(), 1);
    assert_eq!((app.session.history.undo.len(), app.session.is_dirty()), (undo, dirty), "a failed turn changes nothing");

    // the environment key is the fallback
    app.hooks.secret_get = None;
    app.ayp_chat.env = |k| (k == "ANTHROPIC_API_KEY").then(|| "env-key".to_string());
    send(&mut app, "lại nào").unwrap();
    settle(&mut app, &ctx);
    assert_eq!(log.lock().unwrap()[1]["_key"], "env-key");
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Text("chào".into())));
}

#[test]
fn stop_drops_a_late_reply() {
    let (mut app, ctx) = demo();
    let log = script(&mut app, vec![]);
    // the first request hangs until the test lets it answer
    let (go, wait) = std::sync::mpsc::channel::<()>();
    let wait = Mutex::new(wait);
    let calls = Arc::new(Mutex::new(0u32));
    let inner = app.hooks.http_post.take().unwrap();
    app.hooks.http_post = Some(Arc::new(move |req| {
        let n = {
            let mut c = calls.lock().unwrap();
            *c += 1;
            *c
        };
        let _ = inner(req); // logged
        if n == 1 {
            let _ = wait.lock().unwrap().recv();
            return Ok(reply("end_turn", json!([text("TRỄ")])));
        }
        Ok(reply("end_turn", json!([text("Mới")])))
    }));
    send(&mut app, "một").unwrap();
    until(&mut app, &ctx, |_| !log.lock().unwrap().is_empty());
    stop(&mut app);
    assert!(!app.ayp_chat.busy());
    go.send(()).unwrap();
    for _ in 0..50 {
        poll(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(!app.ayp_chat.cards.contains(&Card::Text("TRỄ".into())), "{:?}", app.ayp_chat.cards);
    send(&mut app, "hai").unwrap();
    settle(&mut app, &ctx);
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Text("Mới".into())));
    assert!(!app.ayp_chat.cards.contains(&Card::Text("TRỄ".into())));
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert!(!log[1]["messages"].to_string().contains("TRỄ"), "the late reply never enters the history");
}

#[test]
fn undo_turn_only_when_nothing_came_after() {
    let (mut app, ctx) = demo();
    script(&mut app, vec![reply("tool_use", json!([add_track("t1")])), reply("end_turn", json!([text("Xong.")]))]);
    let project = app.session.project.clone();
    send(&mut app, "Thêm một track").unwrap();
    settle(&mut app, &ctx);
    let card = result_card(&app).unwrap();
    let after = "Đã có thao tác sau lượt này — dùng Hoàn tác (⌘Z) từng bước".to_string();

    crate::menus::invoke(&mut app, &ctx, "sequence.addTracks", json!({"video": 1, "audio": 0})).unwrap();
    assert_eq!(undo_turn(&mut app, &ctx, card), Err(after));
    crate::menus::invoke(&mut app, &ctx, "edit.undo", json!({})).unwrap();
    undo_turn(&mut app, &ctx, card).unwrap();
    assert!(Arc::ptr_eq(&app.session.project, &project), "the whole turn is taken back");
    assert!(matches!(app.ayp_chat.cards[card], Card::Result { undone: true, .. }));
    assert!(undo_turn(&mut app, &ctx, card).is_err(), "only once");
    assert!(undo_turn(&mut app, &ctx, 0).is_err(), "not a result card");
    assert!(undo_turn(&mut app, &ctx, 999).is_err());
}

#[test]
fn history_is_append_only() {
    let (mut app, ctx) = demo();
    let first = json!([
        {"type": "thinking", "thinking": "nghĩ", "signature": "sig"},
        text("Để mình xem."),
        run_cmd("r1", "sequence.inspect", json!({}))
    ]);
    let odd = json!([{"type": "fallback", "model": "x", "note": "kept as is"}, text("Ừ.")]);
    let log = script(
        &mut app,
        vec![
            reply("tool_use", first.clone()),
            reply("tool_use", json!([tool_use("p1", "propose_plan", json!({"title": "T", "steps": ["a"]}))])),
            reply("end_turn", odd.clone()),
            status(400),
            reply("max_tokens", json!([text("Câu dài…"), tool_use("x1", "get_state", json!({}))])),
            reply("end_turn", json!([text("Hết.")])),
        ],
    );
    send(&mut app, "một").unwrap();
    settle(&mut app, &ctx);
    // a new message instead of a decision: the plan is dropped and its tool_use still answered
    send(&mut app, "Không, làm khác").unwrap();
    settle(&mut app, &ctx);
    assert!(app.ayp_chat.cards.iter().any(|c| matches!(c, Card::Plan { state: Decision::Rejected, .. })));
    send(&mut app, "ba").unwrap();
    settle(&mut app, &ctx);
    assert!(matches!(app.ayp_chat.cards.last(), Some(Card::Error(_))), "the 400 ends the turn");
    send(&mut app, "bốn").unwrap();
    settle(&mut app, &ctx);
    assert!(app.ayp_chat.cards.ends_with(&[Card::Text("Câu dài…".into()), Card::Error("Câu trả lời bị cắt".into())]));
    send(&mut app, "năm").unwrap();
    settle(&mut app, &ctx);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 6);
    for w in log.windows(2) {
        let (a, b) = (messages(&w[0]), messages(&w[1]));
        assert!(b.len() > a.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.to_string(), y.to_string(), "an earlier message changed");
        }
    }
    let all = messages(&log[5]);
    // assistant content goes back exactly as received, every block type kept
    assert_eq!(all[1], json!({"role": "assistant", "content": first}));
    assert!(all.contains(&json!({"role": "assistant", "content": odd})));
    let superseded = last_user_content(&log[2]);
    assert_eq!(superseded, json!([{"type": "tool_result", "tool_use_id": "p1", "content": "Người dùng đã dừng"}, text("Không, làm khác")]));
    // the tool_use cut off by max_tokens is answered in the next message
    assert_eq!(last_user_content(&log[5]), json!([{"type": "tool_result", "tool_use_id": "x1", "content": "Người dùng đã dừng"}, text("năm")]));
}

#[test]
fn wait_job_resumes_when_the_job_finishes() {
    let (mut app, ctx) = demo();
    let job = |id| filmcraft_engine::Job { id, label: "Chép lời".into(), progress: Default::default(), result: Default::default() };
    let (j1, j2) = (job(4242), job(4243));
    let done = j1.result.clone();
    app.session.jobs.extend([j1, j2]);
    let log = script(
        &mut app,
        vec![
            reply("tool_use", json!([tool_use("w0", "wait_job", json!({"job": 999_999})), tool_use("w1", "wait_job", json!({"job": 4242}))])),
            reply("end_turn", json!([text("Xong.")])),
            reply("tool_use", json!([tool_use("w2", "wait_job", json!({"job": 4243}))])),
            reply("end_turn", json!([text("Thôi.")])),
        ],
    );
    send(&mut app, "Chép lời rồi báo mình").unwrap();
    until(&mut app, &ctx, |a| a.ayp_chat.cards.iter().any(|c| matches!(c, Card::User(_))) && log.lock().unwrap().len() == 1);
    for _ in 0..50 {
        poll(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(app.ayp_chat.busy() && !app.ayp_chat.running_tool(), "waiting on the job, no tool running");
    assert_eq!(log.lock().unwrap().len(), 1);

    *done.lock().unwrap() = Some(Err("hỏng tệp".into()));
    settle(&mut app, &ctx);
    {
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 2);
        let results = last_user_content(&log[1]);
        assert_eq!(results[0]["tool_use_id"], "w0");
        assert_eq!(results[0]["is_error"], true, "an unknown job is an error, not a wait forever");
        assert_eq!(results[1]["tool_use_id"], "w1");
        let job: Value = serde_json::from_str(results[1]["content"].as_str().unwrap()).unwrap();
        assert_eq!((job["id"].as_u64(), job["finished"].as_bool()), (Some(4242), Some(true)));
        assert_eq!(job["result"], json!({"error": "hỏng tệp"}));
    }

    // stopped while waiting: the wait is answered with the next message
    send(&mut app, "Chờ việc kia").unwrap();
    until(&mut app, &ctx, |_| log.lock().unwrap().len() == 3);
    for _ in 0..20 {
        poll(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(app.ayp_chat.busy());
    stop(&mut app);
    assert!(!app.ayp_chat.busy());
    send(&mut app, "thôi").unwrap();
    settle(&mut app, &ctx);
    let log = log.lock().unwrap();
    assert_eq!(last_user_content(&log[3]), json!([{"type": "tool_result", "tool_use_id": "w2", "content": "Người dùng đã dừng"}, text("thôi")]));
}

#[test]
fn opening_another_project_ends_the_turn() {
    let (mut app, ctx) = demo();
    let log = script(
        &mut app,
        vec![
            reply("tool_use", json!([add_track("t1"), run_cmd("t2", "file.openDemoProject", json!({})), add_track("t3")])),
            reply("end_turn", json!([text("Ok.")])),
        ],
    );
    send(&mut app, "Mở lại demo").unwrap();
    settle(&mut app, &ctx);
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Error("Đã mở project khác — AI dừng lượt này".into())));
    assert_eq!(log.lock().unwrap().len(), 1, "the turn stops: no more requests");
    // the new project keeps its own fresh history: the old turn's mark never touches it
    assert!(app.session.history.undo.is_empty());
    assert_eq!(app.session.history.limit, 200);
    assert!(result_card(&app).is_none());

    send(&mut app, "tiếp").unwrap();
    settle(&mut app, &ctx);
    let c = last_user_content(&log.lock().unwrap()[1]);
    let ids: Vec<&str> = c.as_array().unwrap().iter().filter_map(|r| r["tool_use_id"].as_str()).collect();
    assert_eq!(ids, ["t1", "t2", "t3"]);
    assert_eq!(c[2]["content"], "Người dùng đã dừng");
    assert_eq!(c[3], text("tiếp"));
}

#[test]
fn the_model_is_remembered_in_aypeditor_json() {
    let dir = std::env::temp_dir().join(format!("ayp-chat-model-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("aypeditor.json"), r#"{"introduced":true,"workspace":"AYP"}"#).unwrap();
    let (mut app, _ctx) = demo();
    app.session.prefs_path = Some(dir.join("prefs.json"));
    set_model(&mut app, "  claude-opus-5-5 ");
    assert_eq!(app.ayp_chat.model, "claude-opus-5-5");
    let v: Value = serde_json::from_slice(&std::fs::read(dir.join("aypeditor.json")).unwrap()).unwrap();
    assert_eq!(v, json!({"introduced": true, "workspace": "AYP", "aiModel": "claude-opus-5-5"}), "other keys kept");
    set_model(&mut app, "");
    assert_eq!(app.ayp_chat.model, api::DEFAULT_MODEL);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_reply_does_not_break_the_conversation() {
    let (mut app, ctx) = demo();
    let log = script(&mut app, vec![reply("end_turn", json!([])), reply("end_turn", json!([text("Đây.")]))]);
    send(&mut app, "một").unwrap();
    settle(&mut app, &ctx);
    send(&mut app, "hai").unwrap();
    settle(&mut app, &ctx);
    assert_eq!(app.ayp_chat.cards.last(), Some(&Card::Text("Đây.".into())));
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 2);
    // the API refuses an assistant message with empty content anywhere but last
    assert!(!messages(&log[1]).iter().any(|m| m["role"] == "assistant" && m["content"] == json!([])), "{}", log[1]["messages"]);
}

#[test]
fn the_ai_cannot_group_or_undo_inside_its_turn() {
    let (mut app, ctx) = demo();
    let ids = ["history.beginGroup", "history.endGroup", "edit.undo", "edit.redo"];
    let calls: Vec<Value> = ids.iter().enumerate().map(|(i, id)| run_cmd(&format!("g{i}"), id, json!({}))).collect();
    let log = script(&mut app, vec![reply("tool_use", json!(calls)), reply("end_turn", json!([text("Ok.")]))]);
    send(&mut app, "Gom nhóm giúp mình").unwrap();
    settle(&mut app, &ctx);
    let results = last_user_content(&log.lock().unwrap()[1]);
    let msg = "Mỗi lượt AI đã là một bước hoàn tác — không dùng lệnh này trong chat";
    for (i, r) in results.as_array().unwrap().iter().enumerate() {
        assert_eq!((r["is_error"].as_bool(), r["content"].as_str()), (Some(true), Some(msg)), "{}", ids[i]);
    }
    assert_ne!(app.session.history.limit, usize::MAX, "no group outlives the turn");
    assert!(app.session.execute("history.endGroup", json!({"label": "x"})).is_err(), "no group was left open");
}
