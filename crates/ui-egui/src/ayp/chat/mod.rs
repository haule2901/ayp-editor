//! The in-app Claude chat: the Messages API client (`api`), the tools (`tools`), the system prompt
//! (`prompt`) and, here, the conversation and its turns.
//!
//! A turn starts with a request (the user's message, or the answer to a plan or a choice) and lasts
//! until Claude ends it, it pauses on a plan or a choice, it fails, or the user stops it. Its edits are
//! one undo step (`Session::group_begin` / `group_end`). History is append-only: assistant content is
//! pushed back exactly as received and no earlier message is ever changed.
pub mod api;
pub mod cards;
pub mod prompt;
pub mod tools;
pub mod view;

use crate::{FilmcraftApp, HttpPost, HttpRequest};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use tools::Outcome;

pub(crate) const BUSY: &str = filmcraft_engine::EDIT_LOCKED;
const NO_NET: &str = "App này chưa nối mạng cho AI";
const STOPPED: &str = "Người dùng đã dừng";
const NOT_CHOSEN: &str = "Người dùng không chọn";
const APPROVED: &str = "Người dùng duyệt";
const DROPPED: &str = "Người dùng bỏ kế hoạch";
const CUT: &str = "Câu trả lời bị cắt";
const REFUSED: &str = "AI từ chối yêu cầu này";
const REPLACED: &str = "Đã mở project khác — AI dừng lượt này";
const AFTER: &str = "Đã có thao tác sau lượt này — dùng Hoàn tác (⌘Z) từng bước";
const CUT_SHORT: &str = "Cuộc trò chuyện quá dài hoặc bị ngắt — bấm Trò chuyện mới";
const NEW_PROJECT: &str = "Đã mở project khác — bắt đầu cuộc trò chuyện mới";
const SKILL_MAX_BYTES: u64 = 200_000;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Decision {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Card {
    User(String),
    Text(String),
    Plan {
        title: String,
        steps: Vec<String>,
        state: Decision,
    },
    Choice {
        question: String,
        options: Vec<(String, String)>,
        picked: Option<usize>,
    },
    /// Each `run_command` of the turn: its Vietnamese label, and ok? (`None` while it runs).
    Doing {
        steps: Vec<(String, Option<bool>)>,
    },
    /// A finished turn that added an undo step: its label, the command ids it ran, and the top of
    /// the undo history right after it (Hoàn tác cả lượt only works while nothing came after).
    Result {
        label: String,
        commands: Vec<String>,
        #[serde(skip)]
        mark: filmcraft_engine::UndoMark,
        undone: bool,
    },
    Error(String),
}

/// One `tool_use` block of the last assistant message.
struct Call {
    id: String,
    name: String,
    input: Value,
}

enum Phase {
    Idle,
    /// A request is out.
    Asking(Receiver<Result<Value, api::ApiError>>),
    /// `Chat::queue` holds the tool calls still to run (one per poll).
    Tools,
    /// The `wait_job` call `tool` waits for job `job`.
    Job {
        tool: String,
        job: u64,
    },
    /// The turn ended on a plan or a choice: `tool` is answered by `decide_plan` / `choose`.
    Plan {
        tool: String,
        card: usize,
    },
    Choice {
        tool: String,
        card: usize,
    },
}

struct Turn {
    mark: filmcraft_engine::GroupMark,
    /// The top of the undo history when the turn began: a Result card only when the turn moved it.
    before: filmcraft_engine::UndoMark,
    label: String,
    /// The turn answers an approved plan: no edit is held back for want of one.
    plan_ok: bool,
    /// Successful project-changing commands, and their distinct ids.
    edits: usize,
    commands: Vec<String>,
    doing: Option<usize>,
}

/// What starts a turn.
#[derive(Clone, Copy)]
enum Start<'a> {
    Text(&'a str),
    Plan(bool),
    Choice(usize),
}

pub struct Chat {
    pub cards: Vec<Card>,
    pub model: String,
    /// The message being typed.
    pub input: String,
    /// An IME is composing in the box: Enter belongs to it.
    pub composing: bool,
    /// ⚙ Cài đặt AI is open; its key field (never shown back), the model field and the last note.
    pub settings_open: bool,
    pub key_field: String,
    pub model_field: String,
    pub note: String,
    messages: Vec<Value>,
    phase: Phase,
    turn: Option<Turn>,
    queue: VecDeque<Call>,
    /// `tool_result` blocks waiting for the next user message, in `tool_use` order.
    answers: Vec<Value>,
    /// `tool_use` ids left unanswered by a stop or an error: the next user message says so.
    unanswered: Vec<String>,
    cancel: Arc<AtomicBool>,
    in_tool: bool,
    /// The undo label of the last turn started from a message (plan and choice answers reuse it).
    label: String,
    /// Built on first use (it reads the ayp-phu-de skill once).
    system: Option<String>,
    /// Where the key comes from when the keychain has none (tests swap it).
    env: fn(&str) -> Option<String>,
    /// `Session::projects_opened` the conversation belongs to.
    opened: u64,
}

impl Default for Chat {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            model: api::DEFAULT_MODEL.to_string(),
            input: String::new(),
            composing: false,
            settings_open: false,
            key_field: String::new(),
            model_field: String::new(),
            note: String::new(),
            messages: Vec::new(),
            phase: Phase::Idle,
            turn: None,
            queue: VecDeque::new(),
            answers: Vec::new(),
            unanswered: Vec::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            in_tool: false,
            label: String::new(),
            system: None,
            env: |k| std::env::var(k).ok(),
            opened: 0,
        }
    }
}

impl Chat {
    /// A turn is running: waiting for Claude, running its tools, or waiting for a job.
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Asking(_) | Phase::Tools | Phase::Job { .. })
    }

    /// The chat itself is running a command right now (the edit guard lets it through).
    pub fn running_tool(&self) -> bool {
        self.in_tool
    }

    /// The plan or choice card the conversation waits on (its buttons stay, whatever follows it).
    pub fn waiting_card(&self) -> Option<usize> {
        match self.phase {
            Phase::Plan { card, .. } | Phase::Choice { card, .. } => Some(card),
            _ => None,
        }
    }

    /// Forget the conversation (model, settings and the typed message stay).
    fn reset(&mut self) {
        self.cards.clear();
        self.messages.clear();
        self.phase = Phase::Idle;
        self.turn = None;
        self.queue.clear();
        self.answers.clear();
        self.unanswered.clear();
        self.label.clear();
    }
}

/// The one place the engine edit lock follows the chat: on while a turn runs, lifted only while the
/// chat runs its own tool. Every entry point ends here, so no path leaves it on after a turn.
fn sync_lock(app: &mut FilmcraftApp) {
    let on = app.ayp_chat.busy() && !app.ayp_chat.in_tool;
    app.session.set_edit_lock(on);
}

/// Another project was opened: the conversation (its ids, its turn) belongs to the old one.
fn follow_project(app: &mut FilmcraftApp) {
    let n = app.session.projects_opened;
    if n == app.ayp_chat.opened {
        return;
    }
    app.ayp_chat.opened = n;
    if app.ayp_chat.busy() {
        stop_turn(app);
    }
    let chat = &mut app.ayp_chat;
    let had = !chat.messages.is_empty() || !chat.cards.is_empty();
    chat.reset();
    if had {
        chat.cards.push(Card::Text(NEW_PROJECT.into()));
    }
}

/// Trò chuyện mới: start over (not while a turn runs).
pub fn new_conversation(app: &mut FilmcraftApp) -> Result<(), String> {
    if app.ayp_chat.busy() {
        return Err(BUSY.into());
    }
    app.ayp_chat.reset();
    sync_lock(app);
    Ok(())
}

/// `~/.claude/skills/ayp-phu-de/SKILL.md`, when there is one of a sane size.
fn read_skill() -> Option<String> {
    let path = std::path::Path::new(&std::env::var("HOME").ok()?).join(".claude/skills/ayp-phu-de/SKILL.md");
    if std::fs::metadata(&path).ok()?.len() > SKILL_MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(&path).ok()
}

fn tool_result(id: &str, content: String, is_error: bool) -> Value {
    let mut r = json!({"type": "tool_result", "tool_use_id": id, "content": content});
    if is_error {
        r["is_error"] = json!(true);
    }
    r
}

/// Every project replacement (open, new, close, demo) installs a fresh history, which drops the
/// unlimited history `group_begin` set: an open turn's mark then belongs to the old project.
fn replaced(app: &FilmcraftApp) -> bool {
    app.ayp_chat.turn.is_some() && app.session.history.limit != usize::MAX
}

/// The network hook and the key (keychain, then `ANTHROPIC_API_KEY`), read afresh for each request.
fn connect(app: &FilmcraftApp) -> Result<(HttpPost, String), String> {
    let post = app.hooks.http_post.clone().ok_or(NO_NET)?;
    let key = app.hooks.secret_get.as_ref().and_then(|get| get("anthropic")).or_else(|| (app.ayp_chat.env)("ANTHROPIC_API_KEY"));
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).ok_or_else(|| api::ApiError::NoKey.vi())?;
    Ok((post, key))
}

/// Push a user message and send the conversation.
fn ask(app: &mut FilmcraftApp, post: HttpPost, key: &str, content: Vec<Value>) {
    let chat = &mut app.ayp_chat;
    chat.messages.push(json!({"role": "user", "content": content}));
    let system = chat.system.get_or_insert_with(|| prompt::system_prompt(read_skill().as_deref()));
    let body = api::request_body(&chat.model, system, &tools::schemas(), &chat.messages);
    let req = HttpRequest { url: api::URL.to_string(), headers: api::headers(key), body: body.to_string() };
    chat.cancel = Arc::new(AtomicBool::new(false));
    chat.phase = Phase::Asking(api::spawn(post, req, chat.cancel.clone()));
}

/// Start a turn. Answers the paused tool_use (if any), every unanswered one, then adds the text.
fn begin(app: &mut FilmcraftApp, start: Start) -> Result<(), String> {
    follow_project(app);
    let chat = &mut app.ayp_chat;
    if chat.busy() {
        return Err(BUSY.into());
    }
    let paused = match (&chat.phase, &start) {
        (Phase::Plan { tool, card }, Start::Plan(ok)) => Some((tool.clone(), *card, if *ok { APPROVED } else { DROPPED }.to_string())),
        (Phase::Choice { tool, card }, Start::Choice(n)) => {
            let label = match chat.cards.get(*card) {
                Some(Card::Choice { options, .. }) => options.get(*n).map(|o| o.0.clone()),
                _ => None,
            };
            Some((tool.clone(), *card, label.ok_or("Không có lựa chọn này")?))
        }
        (Phase::Plan { tool, card }, Start::Text(_)) => Some((tool.clone(), *card, STOPPED.to_string())),
        (Phase::Choice { tool, card }, Start::Text(_)) => Some((tool.clone(), *card, NOT_CHOSEN.to_string())),
        (_, Start::Text(_)) => None,
        (_, Start::Plan(_)) => return Err("Không có kế hoạch nào đang chờ".into()),
        (_, Start::Choice(_)) => return Err("Không có câu hỏi nào đang chờ".into()),
    };
    if let Start::Text(t) = start {
        chat.cards.push(Card::User(t.to_string()));
    }
    let (post, key) = match connect(app) {
        Ok(c) => c,
        // nothing is consumed: the user can fix the key and send (or decide) again
        Err(e) => {
            app.ayp_chat.cards.push(Card::Error(e));
            return Ok(());
        }
    };
    let chat = &mut app.ayp_chat;
    let mut content = std::mem::take(&mut chat.answers);
    if let Some((tool, card, answer)) = paused {
        match (chat.cards.get_mut(card), &start) {
            (Some(Card::Plan { state, .. }), Start::Plan(true)) => *state = Decision::Approved,
            (Some(Card::Plan { state, .. }), _) => *state = Decision::Rejected,
            (Some(Card::Choice { picked, .. }), Start::Choice(n)) => *picked = Some(*n),
            _ => {}
        }
        content.push(tool_result(&tool, answer, false));
    }
    content.extend(chat.unanswered.drain(..).map(|id| tool_result(&id, STOPPED.into(), false)));
    if let Start::Text(t) = start {
        chat.label = format!("AI: {}", t.chars().take(40).collect::<String>());
        content.push(json!({"type": "text", "text": t}));
    }
    let turn = Turn {
        before: app.session.undo_mark(),
        mark: app.session.group_begin(),
        label: app.ayp_chat.label.clone(),
        plan_ok: matches!(start, Start::Plan(true)),
        edits: 0,
        commands: Vec::new(),
        doing: None,
    };
    app.ayp_chat.turn = Some(turn);
    ask(app, post, &key, content);
    Ok(())
}

/// End the turn: its edits become one undo step, and a Result card when it changed the project.
fn end_turn(app: &mut FilmcraftApp) {
    if replaced(app) {
        // the mark would restore or merge into the new project's history: drop it, never apply it
        app.ayp_chat.turn = None;
        app.ayp_chat.cards.push(Card::Error(REPLACED.into()));
        return;
    }
    let Some(t) = app.ayp_chat.turn.take() else { return };
    app.session.group_end(t.mark, &t.label, true);
    let mark = app.session.undo_mark();
    if mark != t.before {
        app.ayp_chat.cards.push(Card::Result { label: t.label, commands: t.commands, mark, undone: false });
    }
}

/// Stop a running turn: no more requests or tools; what was not answered is answered next time.
fn stop_turn(app: &mut FilmcraftApp) {
    let chat = &mut app.ayp_chat;
    // the worker only stops on cancel: set it before the receiver is dropped
    chat.cancel.store(true, Ordering::SeqCst);
    if let Phase::Job { tool, job } = std::mem::replace(&mut chat.phase, Phase::Idle) {
        chat.unanswered.push(tool);
        // the job the AI waited on stops with it (as jobs.cancel does)
        if let Some(j) = app.session.jobs.iter().find(|j| j.id == job) {
            j.progress.cancel.store(true, Ordering::Relaxed);
        }
    }
    let chat = &mut app.ayp_chat;
    let left: Vec<String> = chat.queue.drain(..).map(|c| c.id).collect();
    chat.unanswered.extend(left);
    end_turn(app);
}

fn fail(app: &mut FilmcraftApp, why: String) {
    app.ayp_chat.cards.push(Card::Error(why));
    stop_turn(app);
}

fn on_reply(app: &mut FilmcraftApp, msg: &Value) {
    let chat = &mut app.ayp_chat;
    let content = msg.get("content").cloned().unwrap_or_else(|| json!([]));
    // an empty assistant message would make every later request a 400: it is not kept at all
    if content.as_array().is_some_and(|c| !c.is_empty()) {
        chat.messages.push(json!({"role": "assistant", "content": content}));
    }
    let mut calls = VecDeque::new();
    for b in content.as_array().into_iter().flatten() {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = b.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
                    chat.cards.push(Card::Text(t.to_string()));
                }
            }
            Some("tool_use") => {
                // a tool_use without an id cannot be answered, so it is not run either
                if let Some(id) = b.get("id").and_then(Value::as_str) {
                    let name = b.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                    calls.push_back(Call { id: id.to_string(), name, input: b.get("input").cloned().unwrap_or_else(|| json!({})) });
                }
            }
            _ => {}
        }
    }
    let stop = msg.get("stop_reason").and_then(Value::as_str).unwrap_or_default();
    if stop == "tool_use" && !calls.is_empty() {
        chat.queue = calls;
        chat.phase = Phase::Tools;
        return;
    }
    chat.unanswered.extend(calls.into_iter().map(|c| c.id));
    match stop {
        "end_turn" | "stop_sequence" => {}
        "max_tokens" => chat.cards.push(Card::Error(CUT.into())),
        "refusal" => chat.cards.push(Card::Error(REFUSED.into())),
        // model_context_window_exceeded, pause_turn, none…: the conversation cannot go on as it is
        _ => chat.cards.push(Card::Error(CUT_SHORT.into())),
    }
    stop_turn(app);
}

/// The Vietnamese label of a command id (the id itself when it has none).
pub(crate) fn command_label(id: &str) -> String {
    let en = filmcraft_engine::find_command(id).map(|c| c.label).or_else(|| crate::menus::UI_COMMANDS.iter().find(|c| c.id == id).map(|c| c.label));
    en.map_or_else(|| id.to_string(), |l| crate::ayp::vi::vi(l).to_string())
}

/// Add a step to the turn's Doing card; returns (card, step).
fn doing_step(chat: &mut Chat, label: String) -> Option<(usize, usize)> {
    let turn = chat.turn.as_mut()?;
    let card = match turn.doing {
        Some(c) => c,
        None => {
            chat.cards.push(Card::Doing { steps: Vec::new() });
            *turn.doing.insert(chat.cards.len() - 1)
        }
    };
    let Some(Card::Doing { steps }) = chat.cards.get_mut(card) else { return None };
    steps.push((label, None));
    Some((card, steps.len() - 1))
}

/// Run the next tool call, or send the results once every call has run.
fn run_next(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let Some(call) = app.ayp_chat.queue.pop_front() else {
        let answers = std::mem::take(&mut app.ayp_chat.answers);
        match connect(app) {
            Ok((post, key)) if !answers.is_empty() => ask(app, post, &key, answers),
            r => {
                app.ayp_chat.answers = answers; // answered with the next message
                if let Err(e) = r {
                    app.ayp_chat.cards.push(Card::Error(e));
                }
                stop_turn(app);
            }
        }
        return;
    };
    let command = (call.name == "run_command").then(|| call.input.get("id").and_then(Value::as_str).unwrap_or_default().to_string());
    let step = command.as_deref().and_then(|id| doing_step(&mut app.ayp_chat, command_label(id)));
    let edits = app.ayp_chat.turn.as_ref().map_or(0, |t| if t.plan_ok { 0 } else { t.edits });
    app.ayp_chat.in_tool = true;
    sync_lock(app);
    let run = std::panic::AssertUnwindSafe(|| tools::run(app, ctx, &call.name, &call.input, edits));
    let out = std::panic::catch_unwind(run).unwrap_or_else(|_| Outcome::Error("lỗi nội bộ khi chạy công cụ".into()));
    app.ayp_chat.in_tool = false;
    sync_lock(app);
    let chat = &mut app.ayp_chat;
    if let Some((card, i)) = step
        && let Some(Card::Doing { steps }) = chat.cards.get_mut(card)
        && let Some(s) = steps.get_mut(i)
    {
        s.1 = Some(matches!(out, Outcome::Done(_)));
    }
    match out {
        Outcome::Done(v) => {
            if let (Some(id), Some(t)) = (&command, chat.turn.as_mut())
                && filmcraft_engine::find_command(id).is_some_and(|c| c.journal)
            {
                t.edits += 1;
                if !t.commands.contains(id) {
                    t.commands.push(id.clone());
                }
            }
            chat.answers.push(tool_result(&call.id, tools::cap(v.to_string()), false));
        }
        Outcome::Error(e) => chat.answers.push(tool_result(&call.id, tools::cap(e), true)),
        Outcome::WaitJob(job) => chat.phase = Phase::Job { tool: call.id, job },
        Outcome::Plan { title, steps } => {
            chat.cards.push(Card::Plan { title, steps, state: Decision::Pending });
            let card = chat.cards.len() - 1;
            return pause(app, Phase::Plan { tool: call.id, card });
        }
        Outcome::Choice { question, options } => {
            chat.cards.push(Card::Choice { question, options, picked: None });
            let card = chat.cards.len() - 1;
            return pause(app, Phase::Choice { tool: call.id, card });
        }
    }
    if replaced(app) {
        stop_turn(app);
    }
}

/// End the turn on a plan or a choice; tool calls after it are answered with the next message.
fn pause(app: &mut FilmcraftApp, phase: Phase) {
    let chat = &mut app.ayp_chat;
    let left: Vec<String> = chat.queue.drain(..).map(|c| c.id).collect();
    chat.unanswered.extend(left);
    chat.phase = phase;
    end_turn(app);
}

/// The job's `jobs.list` entry once it has finished; an error when there is no such job.
fn job_done(app: &mut FilmcraftApp, job: u64) -> Option<Result<Value, String>> {
    let list = app.session.execute("jobs.list", json!({})).unwrap_or_default();
    let Some(entry) = list.as_array().into_iter().flatten().find(|j| j.get("id").and_then(Value::as_u64) == Some(job)) else {
        return Some(Err(format!("không có việc nền #{job}")));
    };
    entry.get("finished").and_then(Value::as_bool).unwrap_or(false).then(|| Ok(entry.clone()))
}

/// Once a frame: take Claude's reply, run the next tool, or check on the job being waited for.
pub fn poll(app: &mut FilmcraftApp, ctx: &egui::Context) {
    follow_project(app);
    step(app, ctx);
    sync_lock(app);
}

fn step(app: &mut FilmcraftApp, ctx: &egui::Context) {
    if !app.ayp_chat.busy() {
        return;
    }
    if replaced(app) {
        return stop_turn(app);
    }
    let reply = match &app.ayp_chat.phase {
        Phase::Asking(rx) => Some(rx.try_recv()),
        _ => None,
    };
    match reply {
        Some(Err(TryRecvError::Empty)) => {}
        Some(Err(TryRecvError::Disconnected)) => fail(app, api::ApiError::Bad("luồng gọi Claude dừng giữa chừng".into()).vi()),
        Some(Ok(Err(e))) => fail(app, e.vi()),
        Some(Ok(Ok(msg))) => on_reply(app, &msg),
        None => {
            if let Phase::Job { tool, job } = &app.ayp_chat.phase {
                let (tool, job) = (tool.clone(), *job);
                if let Some(r) = job_done(app, job) {
                    let block = match r {
                        Ok(entry) => tool_result(&tool, tools::cap(entry.to_string()), false),
                        Err(e) => tool_result(&tool, e, true),
                    };
                    app.ayp_chat.answers.push(block);
                    app.ayp_chat.phase = Phase::Tools;
                }
            } else {
                run_next(app, ctx);
            }
        }
    }
    if app.ayp_chat.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// Send the user's message as a new turn. Err when a turn is running or the text is empty.
pub fn send(app: &mut FilmcraftApp, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Chưa nhập gì".into());
    }
    let r = begin(app, Start::Text(text));
    sync_lock(app);
    r
}

/// Stop the running turn; a reply that arrives later is dropped.
pub fn stop(app: &mut FilmcraftApp) {
    if app.ayp_chat.busy() {
        stop_turn(app);
    }
    sync_lock(app);
}

/// Answer the plan the conversation waits on; either way Claude gets the answer in a new turn.
pub fn decide_plan(app: &mut FilmcraftApp, approve: bool) -> Result<(), String> {
    let r = begin(app, Start::Plan(approve));
    sync_lock(app);
    r
}

/// Pick option `option` of the choice the conversation waits on.
pub fn choose(app: &mut FilmcraftApp, option: usize) -> Result<(), String> {
    let r = begin(app, Start::Choice(option));
    sync_lock(app);
    r
}

/// Hoàn tác cả lượt: undo the turn of Result card `card`, only while it is still the last step.
pub fn undo_turn(app: &mut FilmcraftApp, ctx: &egui::Context, card: usize) -> Result<(), String> {
    if app.ayp_chat.busy() {
        return Err(BUSY.into());
    }
    let Some(Card::Result { label, mark, undone, .. }) = app.ayp_chat.cards.get(card) else {
        return Err("Không có lượt này".into());
    };
    if *undone {
        return Err("Lượt này đã hoàn tác rồi".into());
    }
    if app.session.undo_mark() != *mark || app.session.history.undo.last().map(|u| u.0.as_str()) != Some(label.as_str()) {
        return Err(AFTER.into());
    }
    crate::menus::invoke(app, ctx, "edit.undo", json!({}))?;
    if let Some(Card::Result { undone, .. }) = app.ayp_chat.cards.get_mut(card) {
        *undone = true;
    }
    Ok(())
}

/// Use `model` from now on (empty: the default) and remember it in `aypeditor.json`.
pub fn set_model(app: &mut FilmcraftApp, model: &str) {
    let m = model.trim();
    app.ayp_chat.model = if m.is_empty() { api::DEFAULT_MODEL } else { m }.to_string();
    app.remember_ai_model();
}

#[cfg(test)]
mod tests;
