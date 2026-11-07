# Plan 2 — Khung chat AI trong AYPEditor · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The AI tab of the AYP right column becomes a working Claude chat that edits the open project
through the same commands as the buttons, with Plan / Choice / Doing / Result cards, "Chỉ chỗ", and
"Hoàn tác cả lượt" (one undo step per AI turn).

**Architecture:** ui-egui stays free of networking: the desktop app hands it two host hooks, an HTTPS
POST (ureq + rustls, as `crates/speech` downloads models) and a Keychain secret store
(`crates/platform`). A worker thread only POSTs JSON and returns JSON; the conversation state machine
lives on the UI thread, runs Claude's tool calls as ordinary commands through `menus::invoke`, and
wraps each turn in `Session::group_begin/group_end`. Long work (Chép lời) becomes an engine job the
chat can wait on without freezing the window.

**Tech Stack:** Rust, egui 0.36, ureq 3 (rustls, platform verifier), `security-framework` (macOS
Keychain), Claude Messages API over raw HTTP.

**Spec:** `docs/superpowers/specs/2026-11-06-ayp-editor-design.md` (§5.2 "Chat AI" rows, §6, §9.3, §9.5).
Plan 1 (`2026-11-06-plan-1-giao-dien-capcut.md`) is done and merged; its ledger parked two items this
plan closes (group_end dirtying the project, GroupMark `#[must_use]`).

## Global Constraints

- AGENTS.md rules: no `unwrap/expect/panic!/unreachable!/todo!/unimplemented!` or `unsafe` outside
  tests; `unsafe` only in `crates/platform`; background work under `catch_unwind`; mutex
  `lock().unwrap_or_else(PoisonError::into_inner)`; numbers from the network are hostile (`get()`,
  capped sizes).
- Layering: `cargo xtask layers` must pass; ui-egui gets **no** ureq / keychain dependency (it also
  builds for the web app) — networking and Keychain come in through `HostHooks`.
- Every interactive widget registers an automation id (`app.auto.add*`); new UI state readable over
  control.
- New UI text is Vietnamese, written directly (AYP code does not use `tl!`; plan 1 ruling). Do not add
  `tl!` literals (they would need 4+ translation rows each).
- Spec §6: model default **`claude-sonnet-5-5`**, changeable in settings; key in macOS Keychain; chat
  uses the commands in `command.list` as its tools; engine never calls Claude.
- API wire format (verified against the claude-api skill, 2026-11-07): `POST
  https://api.anthropic.com/v1/messages`, headers `x-api-key`, `anthropic-version: 2026-10-21`,
  `content-type: application/json`, `anthropic-beta: server-side-fallback-2026-07-01`; body has
  `"fallbacks": "default"`, `"max_tokens": 16000`, `"output_config": {"effort": "medium"}`, top-level
  `"cache_control": {"type": "ephemeral"}`, **no** `thinking`, **no** `temperature`, **no**
  `tool_choice` (forced choice is a 400 on this model). History is **append-only**: the whole
  assistant `content` array (text, tool_use, thinking, fallback blocks) is pushed back unchanged; no
  earlier message is ever edited.
- Keychain item: service `vn.ayp.editor`, account `anthropic`. Fallback when absent: env
  `ANTHROPIC_API_KEY`. The key never appears in logs, status lines, control output or the journal.
- One AI turn = one undo entry labelled `AI: <first 40 chars of the user's message>`.
- Build: `PATH=/opt/homebrew/opt/rustup/bin:$PATH CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$CARGO_TARGET_DIR"` — set `CARGO_TARGET_DIR` to a target folder on a drive with room (never `../filmcraft/target`).
  Run tests per crate (`cargo test -p filmcraft-ui-egui`, `-p filmcraft-engine`, `-p filmcraft`), not
  the whole workspace, unless the task says so. Never edit files while a cargo test run is in progress.
- Known red baselines, not yours to fix: `vfx_tests::effects_list_reports_folders_and_badges`,
  `effect::vfx::tests::premiere_26_video_effects_catalogue`, flaky
  `crash::tests::hook_records_and_logs_worker_panics`.

## Review Focus

1. **No key / wrong key (401) / no network** → one Vietnamese error card saying what to do ("Nhập
   khoá Anthropic ở ⚙ Cài đặt AI"), the turn ends, the app never hangs or panics. *(Task 6 test
   `missing_key_and_401_end_the_turn_with_a_clear_card`.)*
2. **429 / 529 / 5xx / timeout** → at most 3 retries honouring `retry-after` (capped 30 s), then an
   error card; **Dừng** works while waiting and drops the late reply. *(Task 4 `retry_policy`, Task 6
   `stop_drops_a_late_reply`.)*
3. **A tool call that fails** (unknown id, bad params, disabled command) → `tool_result` with
   `is_error: true` and the Vietnamese reason; the turn continues; a turn whose edits all failed
   leaves the project clean (not "Edited"). *(Task 1 `failed_group_leaves_project_clean`, Task 5
   `bad_tool_calls_come_back_as_errors`.)*
4. **Huge tool output** (`sequence.inspect` on a long project) → tool_result text capped at 20 000
   characters with `…(cắt bớt N ký tự)`; never an unbounded request. *(Task 5
   `tool_output_is_capped`.)*
5. **Manual edits during an AI turn** (click, shortcut, MCP `engine.execute`) → refused with "AI đang
   làm — bấm Dừng trước" while the turn runs; reads still work. *(Task 7
   `edits_are_refused_while_ai_works`.)*

---

### Task 1: Undo groups as commands; a failed group leaves the project clean

**Files:** Modify `crates/engine/src/lib.rs` (group_begin/group_end ~L896-940, Session fields),
`crates/engine/src/commands.rs` (register); Test `crates/engine/src/tests.rs`.

**Interfaces:**
- Produces: `#[must_use] pub struct GroupMark`; `group_end(mark, label, ok)` — when `ok == false`, or
  `ok == true` with no new history entry, it restores project/state/history and calls `bump()`
  **only if** the project `Arc` or the undo length changed since the mark.
- Produces commands (spec §9.3, optional for outside tools): `history.beginGroup {}` → `{"ok":true}`
  (Err "a group is already open" if one is), `history.endGroup {"label": str, "ok": bool?=true}` (Err
  "no group is open"). The open mark lives in `Session.open_group: Option<GroupMark>`; both have
  `journal: false`, menu `[]`.

- [ ] Step 1: tests `failed_group_leaves_project_clean` (group_begin → a command that errs →
  group_end(ok=false) → `s.revision()` / dirty flag unchanged) and `history_group_commands`
  (beginGroup, two `sequence.addTracks`, endGroup{label:"AI: thử"} → one undo entry "AI: thử";
  endGroup again → Err; beginGroup twice → Err).
- [ ] Step 2: run `cargo test -p filmcraft-engine failed_group history_group` → FAIL.
- [ ] Step 3: implement.
- [ ] Step 4: same command → PASS; `cargo test -p filmcraft-engine` no new failures.
- [ ] Step 5: commit `fix(engine): undo groups as commands; a failed group leaves the project clean`.

### Task 2: Chép lời runs as a background job

**Files:** Modify `crates/engine/src/transcript.rs` (`generate`, ~L148), `crates/engine/src/lib.rs`
(`poll_persistence` calls `transcript::poll`), `crates/ui-egui/src/ayp/library.rs` (transcribe item,
`transcribe_minutes`, freeze warning); Test `crates/engine/src/transcript_tests.rs`,
`crates/ui-egui/src/ayp/library.rs` tests.

**Interfaces:**
- Produces: `transcript.generate` accepts `"background": bool?` (default false = today's synchronous
  behaviour, so build.py / Edit Hub are unchanged). With `true` it returns `{"job": id}` at once; the
  job appears in `jobs.list` with progress from the transcriber callback and is cancellable with
  `jobs.cancel`; when finished, `transcript::poll(s)` applies all transcripts as **one** undo step
  ("Transcribe"). Follow `scene_detect.rs` (`PendingScene`, `guard_job`, thread name
  `filmcraft-transcribe`). Cancelled → nothing changes.
- Produces: the AYP "Chép lời (tiếng Việt)" button runs it with `background: true`; the status line
  shows `Đang chép lời… N%` while the job runs and `Đã chép lời xong` at the end; the "app sẽ đứng"
  warning and `ayp_transcribed` double-click guard go (a second click while a job runs says
  `Đang chép lời rồi`).

- [ ] Step 1: tests `background_transcribe_applies_once_when_done` (FixedTranscriber; execute with
  background:true → `{"job"}`; transcript absent until `poll` after finish; then present; one undo
  entry), `background_transcribe_cancel_changes_nothing`, ui `transcribe_button_does_not_block`.
- [ ] Step 2: run → FAIL.  Step 3: implement.  Step 4: run → PASS (+ crate suites).
- [ ] Step 5: commit `feat(engine): transcript.generate in the background (Chép lời no longer freezes)`.

### Task 3: Host hooks for HTTPS and the Keychain

**Files:** Create `crates/platform/src/keychain.rs`; Modify `crates/platform/src/lib.rs`,
`crates/platform/Cargo.toml` (`security-framework` on macOS), `crates/ui-egui/src/lib.rs`
(`HostHooks`), `apps/filmcraft/src/main.rs`, `apps/filmcraft/Cargo.toml` (ureq + rustls exactly as
`crates/speech/Cargo.toml` L28-30); Test in `crates/platform/src/keychain.rs`, `apps/filmcraft`.

**Interfaces:**
- Produces in platform: `pub fn secret_get(service: &str, account: &str) -> Option<String>`,
  `pub fn secret_set(service: &str, account: &str, value: &str) -> Result<(), String>` (empty value =
  delete). Non-macOS: get → None, set → Err("Keychain chỉ có trên macOS").
- Produces in ui-egui:
  ```rust
  pub struct HttpRequest { pub url: String, pub headers: Vec<(String, String)>, pub body: String }
  pub struct HttpResponse { pub status: u16, pub retry_after: Option<f64>, pub body: String }
  pub type HttpPost = std::sync::Arc<dyn Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync>;
  // HostHooks:
  pub http_post: Option<HttpPost>,
  pub secret_get: Option<Box<dyn Fn(&str) -> Option<String>>>,          // account → value
  pub secret_set: Option<Box<dyn FnMut(&str, &str) -> Result<(), String>>>,
  ```
- Desktop wiring: `http_post` = ureq agent (rustls + platform verifier as `speech/src/models.rs:322`),
  global timeout 600 s, non-2xx returned as `HttpResponse` (not Err; `http_status_as_error(false)`),
  body capped at 8 MB; `secret_*` = platform with service `vn.ayp.editor`.

- [ ] Step 1: tests `keychain_round_trip` (macOS only, `#[cfg(target_os="macos")]`, service
  `vn.ayp.editor.test`, set → get → set "" → None) and an app-crate test that `http_post` maps a
  non-2xx to `Ok(HttpResponse{status})` using a local `std::net::TcpListener` serving a canned 429 with
  `retry-after: 2`.
- [ ] Step 2: FAIL.  Step 3: implement.  Step 4: PASS; `cargo xtask layers` passes.
- [ ] Step 5: commit `feat(aypeditor): HTTPS and Keychain host hooks for the AI chat`.

### Task 4: Claude API client (pure functions + worker)

**Files:** Create `crates/ui-egui/src/ayp/chat/mod.rs` (module root, declares submodules),
`crates/ui-egui/src/ayp/chat/api.rs`; Modify `crates/ui-egui/src/ayp/mod.rs` (`pub mod chat;`).

**Interfaces:**
- Produces in `api.rs`:
  - `pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";`
  - `pub fn request_body(model: &str, system: &str, tools: &serde_json::Value, messages: &[serde_json::Value]) -> serde_json::Value`
    — exactly the Global Constraints body; `system` as one text block.
  - `pub fn headers(key: &str) -> Vec<(String, String)>`.
  - `pub enum ApiError { NoKey, Auth, Http { status: u16, message: String }, Network(String), Bad(String) }`
    with `pub fn vi(&self) -> String` (Vietnamese card text; `NoKey`/`Auth` say "Nhập khoá Anthropic ở ⚙ Cài đặt AI").
  - `pub fn parse(resp: &HttpResponse) -> Result<serde_json::Value, ApiError>` — 2xx → the message
    JSON (must have `content` array and `stop_reason`); 401/403 → `Auth`; other → `Http` with the
    API's `error.message`.
  - `pub fn retry_delay(status: u16, retry_after: Option<f64>, attempt: u32) -> Option<std::time::Duration>`
    — retry 408/429/500/502/503/504/529 up to attempt 3: `retry_after` capped at 30 s, else 1 s·2^attempt.
  - `pub fn spawn(post: HttpPost, req: HttpRequest, cancel: Arc<AtomicBool>) -> std::sync::mpsc::Receiver<Result<serde_json::Value, ApiError>>`
    — thread `ayp-chat`, under `catch_unwind`, retries per `retry_delay`, sleeps in 100 ms slices
    checking `cancel`.

- [ ] Step 1: tests `request_body_matches_the_wire_contract` (asserts every Global-Constraints field,
  absence of `thinking`/`temperature`/`tool_choice`), `parse_maps_statuses`, `retry_policy`
  (429 with retry_after 2 → 2 s; 529 attempt 0 → 1 s; attempt 3 → None; 400 → None; retry_after 999 → 30 s),
  `spawn_retries_then_succeeds` (fake HttpPost: 529, 529, 200).
- [ ] Step 2: FAIL.  Step 3: implement.  Step 4: PASS.
- [ ] Step 5: commit `feat(aypeditor): Claude Messages API client for the chat`.

### Task 5: Tools and system prompt

**Files:** Create `crates/ui-egui/src/ayp/chat/tools.rs`, `crates/ui-egui/src/ayp/chat/prompt.rs`.

**Interfaces:**
- Produces in `tools.rs`:
  - `pub fn schemas() -> serde_json::Value` — six tools, fixed order, each `strict: true` with
    `additionalProperties: false`:
    `find_commands {query: string}` · `run_command {id: string, params: object}` · `get_state {}` ·
    `wait_job {job: integer}` · `propose_plan {title: string, steps: [string] (1..12)}` ·
    `offer_choices {question: string, options: [{label: string, detail: string}] (2..4)}`.
    Descriptions in English, say *when* to use each (plan before deleting/cutting many clips,
    replacing captions, or more than 3 edits; choices when the user must pick a style).
  - `pub enum Outcome { Done(serde_json::Value), Error(String), Plan { title: String, steps: Vec<String> }, Choice { question: String, options: Vec<(String, String)> }, WaitJob(u64) }`
  - `pub fn run(app: &mut FilmcraftApp, ctx: &egui::Context, name: &str, input: &serde_json::Value, edits_without_plan: usize) -> Outcome`
    — `find_commands` → `palette::search(app, query, 20)` as `[{id, label, labelVi, params, enabled, reason?}]`
    (params from `command_specs()`); `run_command` → `menus::invoke` (Err → `Error(vi::reason(e))`);
    refuses with Error("Việc này đổi nhiều thứ: gọi propose_plan trước") when the command is
    journaled and `edits_without_plan >= 3`; `get_state` → the `ui.ayp.state` JSON plus
    `"history": last 5 undo labels`; the rest map to their Outcome.
  - `pub fn cap(text: String) -> String` — 20 000 chars max, suffix `…(cắt bớt N ký tự)`.
- Produces in `prompt.rs`: `pub fn system_prompt(extra_skill: Option<&str>) -> String` — Vietnamese
  role + rules (always answer in Vietnamese, short; find_commands before guessing ids; plan first for
  big changes; captions follow the AYP rules; never invent file paths) + `include_str!` of
  `docs/ayp-styles.md` and the "## AYP caption rules" section of `CLAUDE.md` (sliced at build time
  between that heading and the next `## `) + `extra_skill` (the caller reads
  `~/.claude/skills/ayp-phu-de/SKILL.md` when it exists). Deterministic: same input → same bytes
  (prompt caching).

- [ ] Step 1: tests `schemas_are_strict_and_ordered`, `find_commands_finds_vietnamese`
  (query "giảm ồn" returns an audio denoise command id), `bad_tool_calls_come_back_as_errors`
  (unknown id, bad params JSON shape), `big_changes_need_a_plan` (4th journaled run → Error),
  `tool_output_is_capped`, `prompt_has_the_caption_rules_and_is_stable`.
- [ ] Step 2: FAIL.  Step 3: implement.  Step 4: PASS.
- [ ] Step 5: commit `feat(aypeditor): chat tools over the command list, AYP system prompt`.

### Task 6: Conversation state machine and turns

**Files:** Modify `crates/ui-egui/src/ayp/chat/mod.rs`, `crates/ui-egui/src/lib.rs` (field
`pub ayp_chat: crate::ayp::chat::Chat`, call `ayp::chat::poll(self, ctx)` once per frame next to
`ayp::library::run_pending`).

**Interfaces:**
- Produces:
  ```rust
  pub enum Card { User(String), Text(String), Plan { title: String, steps: Vec<String>, state: Decision },
      Choice { question: String, options: Vec<(String, String)>, picked: Option<usize> },
      Doing { steps: Vec<(String, Option<bool>)> },            // label, ok?
      Result { label: String, commands: Vec<String>, undo_len: usize, undone: bool },
      Error(String) }
  pub enum Decision { Pending, Approved, Rejected }
  pub struct Chat { pub cards: Vec<Card>, pub model: String, /* private: messages, phase, turn, cancel */ }
  impl Chat { pub fn busy(&self) -> bool; pub fn running_tool(&self) -> bool; }
  pub fn send(app: &mut FilmcraftApp, text: &str) -> Result<(), String>;   // Err when busy / empty
  pub fn poll(app: &mut FilmcraftApp, ctx: &egui::Context);
  pub fn stop(app: &mut FilmcraftApp);
  pub fn decide_plan(app: &mut FilmcraftApp, approve: bool) -> Result<(), String>;
  pub fn choose(app: &mut FilmcraftApp, option: usize) -> Result<(), String>;
  pub fn undo_turn(app: &mut FilmcraftApp, ctx: &egui::Context, card: usize) -> Result<(), String>;
  ```
- Behaviour (spec §6, §9.3):
  - `send`: key from `hooks.secret_get("anthropic")`, else env; none → `Card::Error(ApiError::NoKey.vi())`, no request.
    Starts a turn: `group_begin()`, label `AI: <first 40 chars>`. Any `tool_use` left unanswered from a
    stopped/paused turn gets a `tool_result` ("Người dùng đã dừng / không chọn") in the same new user message.
  - Each reply: push the whole assistant message; show text blocks as `Card::Text`; `stop_reason`
    `tool_use` → run tools in order via `tools::run`, collecting **all** results into one user message;
    `Plan`/`Choice` outcomes pause (no request) until `decide_plan`/`choose`, whose answer becomes that
    tool_use's result ("Người dùng duyệt" / "Người dùng bỏ kế hoạch" / the picked label); `WaitJob`
    pauses until the job is finished in `jobs.list`, then its JSON is the result.
    `end_turn` → turn ends; `max_tokens` → Text + Error("Câu trả lời bị cắt"); `refusal` → Error("AI từ chối yêu cầu này").
  - While a turn runs, one `Card::Doing` per turn lists each `run_command` as it happens (the
    command's Vietnamese label, `vi::vi(label)`), `None` while running, then ✓ / ✗.
  - A turn ends (end_turn, pause for Plan/Choice, error, stop): `group_end(mark, label, true)`; if
    any journaled command succeeded, push `Card::Result` with the distinct commands and
    `undo_len = history.undo.len()`.
  - `stop`: sets cancel, drops the pending receiver, ends the turn.
  - `undo_turn`: allowed only when `history.undo.len() == undo_len` and the last label equals the
    card's label → `menus::invoke("edit.undo")`, `undone = true`; else Err("Đã có thao tác sau lượt
    này — dùng Hoàn tác (⌘Z) từng bước").
  - Model: `Chat.model` loaded from / saved to `aypeditor.json` key `"aiModel"` (default
    `api::DEFAULT_MODEL`).

- [ ] Step 1: tests with a scripted fake `http_post` (queue of canned replies; tests step `poll`
  until idle): `a_turn_runs_tools_and_is_one_undo_step`, `plan_waits_for_approval_then_continues`,
  `choice_answer_goes_back_as_tool_result`, `missing_key_and_401_end_the_turn_with_a_clear_card`,
  `stop_drops_a_late_reply`, `undo_turn_only_when_nothing_came_after`,
  `history_is_append_only` (every request's `messages` starts with the previous request's
  `messages` byte-for-byte), `wait_job_resumes_when_the_job_finishes`.
- [ ] Step 2: FAIL.  Step 3: implement.  Step 4: PASS.
- [ ] Step 5: commit `feat(aypeditor): AI chat turns, plans, choices, one undo step per turn`.

### Task 7: Chat panel, Chỉ chỗ, read-only while AI works

**Files:** Create `crates/ui-egui/src/ayp/chat/view.rs`; Modify `crates/ui-egui/src/ayp/side.rs`
(AI tab body → `chat::view::show`), `crates/ui-egui/src/ayp/library.rs` (flash highlight; `Kind::Ai`
items), `crates/ui-egui/src/menus.rs` (`invoke` guard), `crates/ui-egui/src/lib.rs` (overlay);
Test `crates/ui-egui/tests/ayp_chat_ui.rs`.

**Interfaces:**
- Consumes Task 6 API. Produces automation ids: `ayp.chat.input`, `ayp.chat.send`, `ayp.chat.stop`,
  `ayp.chat.card.<i>`, `ayp.chat.plan.approve|edit|reject`, `ayp.chat.choice.<n>`,
  `ayp.chat.place.<i>.<n>`, `ayp.chat.undoTurn.<i>`, `ayp.chat.settings`, `ayp.chat.key`,
  `ayp.chat.model`, `ayp.chat.saveKey`.
- View: cards scroll (stick to bottom), multiline input (Enter sends, Shift+Enter newline, Enter
  while an IME composition is active does **not** send), Dừng while busy, port-busy line kept from
  plan 1, ⚙ Cài đặt AI (password field → `secret_set("anthropic", key)`, shows only "Đã lưu khoá"; model
  text field). Plan card buttons Làm / Sửa (puts "Sửa kế hoạch: " in the input) / Bỏ; Choice buttons;
  Doing shows ✓ / ✗ per step; Result shows each command's Vietnamese label with a **Chỉ chỗ** button
  and **Hoàn tác cả lượt**.
- Chỉ chỗ: `tabs::find_cmd(id)` → `open_tab(tab)` + `app.ui.ayp_flash = Some((key, Instant))` drawn as
  an accent outline on that item for 1.5 s; no AYP item → open Tìm lệnh with the command's label typed in.
- `Kind::Ai` items: open the AI tab and `send` the item's label as the request.
- Guard: in `menus::invoke`, when `app.ayp_chat.busy() && !app.ayp_chat.running_tool()` and the target
  is a journaled engine command → Err("AI đang làm — bấm Dừng trước"). While busy, four interactable
  areas cover everything outside the AYP right column (pointer cannot reach timeline / player).

- [ ] Step 1: harness tests `chat_panel_sends_and_shows_cards`, `plan_buttons_drive_the_turn`,
  `place_opens_the_tab_and_flashes_the_item`, `edits_are_refused_while_ai_works` (engine.execute
  over control → Err; `sequence.inspect` → Ok), `ai_items_open_the_chat`, `enter_during_ime_does_not_send`.
- [ ] Step 2: FAIL.  Step 3: implement.  Step 4: PASS; screenshot snapshot looked at.
- [ ] Step 5: commit `feat(aypeditor): AI chat panel with Chỉ chỗ and turn undo`.

### Task 8: Control channel and docs

**Files:** Modify `crates/ui-egui/src/control.rs`, `docs/control-protocol.md`; Test
`crates/ui-egui/tests/ayp_control_ui.rs`.

**Interfaces:**
- `ui.ayp.run {"key": str}` — runs an AYP item exactly as a click (`library::click_item` path); Err
  for an unknown key.
- `ui.ayp.chat.state` → `{"busy", "model", "cards": [serde of Card]}` (no key, ever);
  `ui.ayp.chat.send {"text"}`, `ui.ayp.chat.stop`, `ui.ayp.chat.plan {"approve": bool}`,
  `ui.ayp.chat.choose {"option": n}`, `ui.ayp.chat.undoTurn {"card": n}`.
- Docs: one table row per method in `docs/control-protocol.md`, plus the AI chat section (key setup,
  model, what a turn is).

- [ ] Step 1: tests for each method (fake http_post), including `chat_state_never_contains_the_key`.
- [ ] Step 2: FAIL.  Step 3: implement.  Step 4: PASS.
- [ ] Step 5: commit `feat(aypeditor): control methods for the AYP buttons and the AI chat`.

### Task 9: Acceptance on the real app

**Files:** Create `tools/ayp-editor/check_chat.py`; Modify `docs/aypeditor-check.md`.

- [ ] Step 1: `check_chat.py --port 9899` (refuses any app that isn't AYPEditor, like
  `check_features.py`): opens a copy of the 0716 test project, then via control —
  (a) `ui.ayp.run` for three AYP keys; (b) chép lời in the background while `ui.screenshot` keeps
  answering (no freeze); (c) with no key: chat shows the key error card; (d) **only if** a key is
  available (Keychain or env): "cắt bớt khoảng lặng ở 20 giây đầu" → a Plan card → approve → Result
  card → Hoàn tác cả lượt restores the original sequence (`sequence.inspect` equal before/after);
  (e) manual edit during the turn refused. Prints a PASS/FAIL table.
- [ ] Step 2: build + install (`scripts/build-aypeditor.sh --install`), run it, record results in
  `docs/aypeditor-check.md` (no private names or paths). Live-key rows are marked "chờ khoá" when no
  key exists.
- [ ] Step 3: commit `test(aypeditor): real-app acceptance for the AI chat`.
