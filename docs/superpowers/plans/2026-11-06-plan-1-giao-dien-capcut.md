# AYPEditor — Kế hoạch 1: giao diện kiểu CapCut + 41 chức năng có sẵn

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** AYPEditor.app mở ra với giao diện kiểu CapCut: thẻ ở bên trái, player ở giữa, tab Thông số/AI ở
bên phải, thanh công cụ cùng timeline ở dưới, và nút "Tìm lệnh…". 41 chức năng ✅ của spec nằm đúng thẻ và
đều chạy được bằng nút lẫn qua cổng control.

**Architecture:** Giao diện mới là **một workspace "AYP"** trong hệ dock có sẵn. Workspace này gồm 3
PanelKind mới (`AypLibrary`, `AypSide`, `AypToolbar`) và 2 panel cũ (`Program`, `Timeline`). Mọi nút đều
gọi `crate::menus::invoke(app, ctx, id, params)`, giống các panel cũ. Code mới nằm trong
`crates/ui-egui/src/ayp/`. File dùng chung chỉ bị sửa ở các điểm nối được nêu tên trong từng task. Chữ trong
các panel AYP viết bằng tiếng Việt qua bảng `ayp/vi.tsv`, **không dùng `tl!`/`tlf!`**, vì test quét mã
nguồn sẽ đòi bản dịch es/zh/ja cho mọi chuỗi `tl!`. Workspace AYP chỉ thành mặc định khi app chạy với biến
môi trường `AYP_EDITOR=1` (bundle AYPEditor tự đặt biến này). Vì vậy FilmCraft gốc và khoảng 40 file test
hiện có vẫn thấy workspace "Editing" như trước.

**Tech Stack:** Rust, egui/eframe 0.36 (app hiện thực `logic()`/`ui()`, không có `update`), egui_kittest,
`cargo test -p filmcraft-ui-egui`, `cargo test -p filmcraft-engine`.

**Spec:** `docs/superpowers/specs/2026-11-06-ayp-editor-design.md` (đọc mục 2, 3, 4, 5.1, 9.1, 10)

## Global Constraints

- Repo `/Users/haule/App Creator/AYP-Editor`, nhánh **`feat/plan-1`**. Commit nhỏ sau mỗi task, kết thúc
  commit message bằng `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Không push (người điều phối push).
- **Build:** `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="/Volumes/Extreme SSD/aypeditor-target"`. **Không bao
  giờ** dùng `../filmcraft/target`, vì sẽ đè bản build của FilmCraft AYP. Nếu `df -h /` báo dưới 2 GB thì dừng
  và báo, không tự xoá gì.
- Tên app **AYPEditor**, bundle id `vn.ayp.editor`, cài ở `~/Applications/AYPEditor.app`. **Không bao giờ** sửa,
  xoá hay thoát `~/Applications/FilmCraft AYP.app`, cũng không đụng tới repo `App Creator/filmcraft`.
- Thư mục dữ liệu là `~/Library/Application Support/FilmCraft`, đường cố định
  (`engine/src/autosave.rs::default_data_dir`), nên tự dùng chung. Cổng control đọc từ `FILMCRAFT_CONTROL_PORT`,
  còn cờ `--control <port>` được ưu tiên hơn.
- Chạy thử ban đêm: nếu `lsof -nP -iTCP:9879 -sTCP:LISTEN` báo đang bận thì chạy
  `~/Applications/AYPEditor.app/Contents/MacOS/<exe> --control 9899` và test trên 9899. Mọi lệnh file luôn truyền
  `paths`/`path` (thiếu thì app mở hộp thoại hệ thống, treo luồng UI). Không dùng file trên `/Volumes` hay
  `~/Documents` trực tiếp, vì bundle mới sẽ bị macOS hỏi quyền mà không ai bấm: chép file vào `$TMPDIR` trước.
- Mọi thay đổi project đi qua lệnh (`menus::invoke` / `Session::execute`).
- Không chép hình ảnh, biểu tượng hay tài nguyên của CapCut. Biểu tượng dùng `crate::icons` (ưu tiên) hoặc ký tự
  có trong font sẵn có; kiểm bằng ảnh chụp.
- Chữ trong panel AYP bằng tiếng Việt (Phương tiện, Âm thanh, Văn bản, Phụ đề, Hiệu ứng, Chuyển tiếp, Bộ lọc,
  Tách, Dừng hình, Hút vào đoạn chính…). Menu bar và các panel cũ vẫn bằng tiếng Anh ở kế hoạch này.
- Được phép sửa test hiện có **chỉ** ở các chỗ được nêu tên trong task: danh sách workspace trong
  `tests/workspaces_ui.rs`, độ dài `PanelKind::ALL`, và các catalog es/zh-cn/ja/pt-br cho nhãn mới.
- Trước khi commit task nào có sửa file dùng chung: chạy `cargo test -p filmcraft-ui-egui` và
  `cargo test -p filmcraft-engine`, cả hai phải xanh.

## Review Focus

1. **Bấm nút cần clip khi chưa chọn clip.** Lệnh có thể qua `enabled()` nhưng lỗi khi chạy (ví dụ
   `effects.apply`: "select clips first"). `invoke` đã ghi lỗi tiếng Anh lên `ui.status`. Panel AYP phải hiện
   "Chưa làm được: <lý do>", với lý do thường gặp dịch qua `vi()`. Không panic. Test ở Task 5.
2. **Người dùng cũ.** `workspaces.json` dùng chung đã có `current: "Editing"`. AYPEditor mở lần đầu vẫn vào
   AYP, chỉ chuyển **một lần** (ghi dấu ở file riêng `aypeditor.json` trong thư mục dữ liệu). Từ lần sau, app
   mở lại workspace dùng lần trước. Test ở Task 4.
3. **Gõ không dấu khi tìm lệnh** ("xoa khoang lang", "phu de") vẫn ra lệnh đúng. Test ở Task 8.
4. **Không có sequence nào đang mở / project có nhiều track.** Panel AYP không panic, hiện hướng dẫn. Test ở Task 5, 6.
5. **Cổng control đang bận.** AYPEditor vẫn mở, hiện toast "Cổng điều khiển 9879 đang bận — AI/MCP chưa nối
   được." Thông báo trên status tự xoá sau 8 s nên phải dùng `app.toast`. Test ở Task 1.

---

### Task 1: Đóng gói AYPEditor.app + báo cổng bận

**Files:**
- Create: `scripts/build-aypeditor.sh`
- Modify: `apps/filmcraft/src/control_server.rs` (`start` trả `Result`), `apps/filmcraft/src/main.rs`
  (lỗi bind cổng → toast trong app, không chết)
- Test: unit test trong `control_server.rs`

**Interfaces:**
- Produces: `scripts/build-aypeditor.sh [--install]`. Script build release
  `cargo build --release -p filmcraft --features whisper,vieneu-voices` với env ở Global Constraints, rồi ghép
  `$CARGO_TARGET_DIR/AYPEditor.app` theo đúng cách AYP.md ghép FilmCraft AYP. Trong đó: `CFBundleIdentifier`
  = `vn.ayp.editor`; `CFBundleName` và `CFBundleDisplayName` = `AYPEditor`;
  `plutil -insert LSEnvironment -json '{"FILMCRAFT_CONTROL_PORT":"9879","AYP_EDITOR":"1"}'`;
  `codesign --force --deep -s -`. Với `--install` thì chép sang `~/Applications/AYPEditor.app` (xoá bản
  AYPEditor cũ nếu có, không đụng app khác). Không đổi `with_title("FilmCraft")` và `NATIVE_APP_MENU` (có test ghim).
- Produces: `control_server::start(port) -> Result<Receiver<ControlRequest>, String>`. Lỗi có chữ "đang bận" khi cổng đã có người dùng.

- [ ] **Step 1: Test hỏng** `busy_port_is_an_error` trong `control_server.rs`: bind một `TcpListener` lên
  `127.0.0.1:0`, gọi `start(port)` trên cổng đó. Assert `Err(e)` và `e.contains("đang bận")`.
- [ ] **Step 2:** `cargo test -p filmcraft busy_port`. Expected: FAIL.
- [ ] **Step 3:** Sửa `start` và chỗ gọi trong `main.rs`: có lỗi thì app vẫn chạy, không kênh control, và hiện
  toast "Cổng điều khiển {port} đang bận — AI/MCP chưa nối được." (tìm API toast có sẵn, ví dụ `app.toast`).
- [ ] **Step 4:** Test PASS. Viết script. Chạy `scripts/build-aypeditor.sh --install` (build release lần đầu lâu,
  chạy nền nếu cần).
- [ ] **Step 5: Chạy thử.** Theo luật cổng ở Global Constraints: chạy app (9879 nếu trống, không thì `--control 9899`),
  gửi `{"id":1,"method":"engine.commands","params":{}}`. Expected: `ok:true`, ≥ 700 lệnh, có
  `transcript.createCaptionsVi`. Kiểm `plutil -p ~/Applications/AYPEditor.app/Contents/Info.plist` có đủ bundle
  id, tên và LSEnvironment. Thoát bản thử bằng method `app.quit`.
- [ ] **Step 6:** Commit `feat(aypeditor): bundle AYPEditor.app; busy control port no longer silent`.

### Task 2: Bảng chữ tiếng Việt cho panel AYP

**Files:**
- Create: `crates/ui-egui/src/ayp/mod.rs`, `ayp/vi.rs`, `ayp/vi.tsv`
- Modify: `crates/ui-egui/src/lib.rs` (`pub mod ayp;`), `crates/ui-egui/src/i18n/mod.rs` (`mod catalog;` → `pub(crate) mod catalog;`)

**Interfaces:**
- Produces: `pub fn vi(english: &str) -> &str`. Đọc từ `vi.tsv` (định dạng như `es.tsv`, parse bằng
  `i18n::catalog::Catalog::parse` trong `OnceLock`); không có thì trả lại đầu vào.
  `pub fn has(english: &str) -> bool`.
  `pub fn synonyms(english: &str) -> Vec<&'static str>`: các dòng context `syn`, cột 3 là các từ cách nhau bằng
  `|`. Giữ `String` trong `OnceLock` rồi trả lát cắt.
  `pub fn reason(engine_error: &str) -> String`: dịch lỗi engine thường gặp ("select clips first",
  "no sequence is open", "no clips selected"…) qua các dòng context `err`, không có thì giữ nguyên.

- [ ] **Step 1: Test hỏng** trong `vi.rs`:
```rust
#[test] fn known_labels_are_vietnamese() {
    assert_eq!(vi("Add Edit"), "Tách");
    assert_eq!(vi("Add Frame Hold"), "Dừng hình");
    assert_eq!(vi("no such label"), "no such label");
}
#[test] fn synonyms_are_listed() { assert!(synonyms("Remove Pauses").contains(&"khoảng lặng")); }
#[test] fn errors_are_explained() { assert!(reason("select clips first").contains("chọn")); }
#[test] fn every_menu_command_has_a_row() {
    // mọi lệnh engine có trong menu đều có dòng trong vi.tsv (dòng giữ nguyên chữ được phép: nhãn đã là tiếng Việt, AAF, EDL…)
    let missing: Vec<_> = filmcraft_engine::command_specs().iter()
        .filter(|c| !c.menu.is_empty() && !has(c.label)).map(|c| c.label).collect();
    assert!(missing.is_empty(), "{missing:#?}");
}
```
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui ayp::vi`. Expected: FAIL.
- [ ] **Step 3:** Viết `vi.rs` và `vi.tsv`: mọi nhãn lệnh có menu (lấy danh sách từ test hỏng), dòng `syn` cho
  mọi lệnh trong bảng thẻ Task 3, dòng `err` cho các lỗi `enabled()`/run thường gặp. Header file ghi: viết từ
  nghĩa thông thường của nhãn tiếng Anh, không chép bản dịch của phần mềm khác.
- [ ] **Step 4:** Test PASS. Chạy `cargo test -p filmcraft-ui-egui i18n`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): Vietnamese catalog for the AYP panels`.

### Task 3: Bảng thẻ — 41 chức năng nằm ở thẻ nào

**Files:**
- Create: `crates/ui-egui/src/ayp/tabs.rs`

**Interfaces:**
- Produces:
```rust
pub struct Item { pub cmd: &'static str, pub params: &'static str /* JSON */, pub label: &'static str /* tiếng Việt */, pub key: &'static str /* duy nhất trong thẻ: thường = cmd, effects.apply → "effects.apply.<effect>" */ }
pub struct Tab { pub id: &'static str, pub icon: crate::icons::Icon /* hoặc ký tự nếu icons không có */, pub label: &'static str, pub items: &'static [Item], pub embed: Option<crate::dock::PanelKind> }
pub const TABS: &[Tab];
pub fn find_cmd(cmd: &str) -> Option<(&'static Tab, &'static Item)>;
```
  `embed`: media → `Project`, audio → `EssentialSound`, text → `EssentialGraphics`, captions → `Text`,
  voice → `TextToSpeech`, effects → `Effects`, transitions → `Effects`, color → `LumetriColor`, cut → `Text`,
  motion → None.

- [ ] **Step 1: Test hỏng** trong `tabs.rs`:
```rust
#[test] fn tab_ids_are_the_ten_tabs() {
    let ids: Vec<_> = TABS.iter().map(|t| t.id).collect();
    assert_eq!(ids, ["media","audio","text","captions","cut","voice","effects","transitions","color","motion"]);
}
#[test] fn every_item_is_a_real_command_with_json_params() {
    for t in TABS { for i in t.items {
        assert!(filmcraft_engine::find_command(i.cmd).is_some() || crate::menus::UI_COMMANDS.iter().any(|u| u.id == i.cmd), "{}", i.cmd);
        assert!(serde_json::from_str::<serde_json::Value>(i.params).is_ok(), "{}", i.cmd);
    }}
}
#[test] fn keys_are_unique() { let mut k: Vec<_> = TABS.iter().flat_map(|t| t.items.iter().map(|i| i.key)).collect(); let n = k.len(); k.sort(); k.dedup(); assert_eq!(k.len(), n); }
#[test] fn effect_items_name_real_effects() { /* mọi effects.apply có "effect" nằm trong danh sách filmcraft_project effect ids (dùng registry engine dùng cho effects.list) */ }
#[test] fn the_41_features_are_placed() {
    for c in ["file.import","audio.voiceover.start","clip.extractAudio","essentialSound.autoMatch","essentialSound.generateDucking",
              "graphics.newText","transcript.createCaptionsVi","transcript.captionsFromChunks","captions.applyPreset",
              "transcript.removePauses","transcript.removeFillers","transcript.lift","tts.create",
              "sequence.applyVideoTransition","lumetri.applyPreset","lut.import","lumetri.applyMatch","clip.speedDuration","clip.frameHold",
              "masks.add","masks.track","clip.nest","clip.group","multicam.cut","markers.add","review.open","review.import",
              "export.quick","export.queue.add"] {
        assert!(find_cmd(c).is_some(), "{c} không nằm ở thẻ nào");
    }
    for e in ["skin_smooth","gaussian_blur","warp_stabilizer","auto_reframe","ultra_key","horizontal_flip","crop","denoise","speech_enhance"] {
        assert!(TABS.iter().flat_map(|t| t.items).any(|i| i.key == format!("effects.apply.{e}")), "{e}");
    }
    assert!(TABS.iter().map(|t| t.items.len()).sum::<usize>() >= 41);
}
```
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui ayp::tabs`. Expected: FAIL.
- [ ] **Step 3:** Viết `TABS` phủ đủ 41 mục ✅ ở spec 5.1, mỗi mục là một dòng ghi rõ `cmd` + `params`. Gồm cả:
  rõ dần/mờ dần âm thanh (`effects.setParam` volume keyframe, hoặc audio transition qua lệnh có sẵn),
  multicam, marker, góp ý. Vị trí / tỷ lệ / xoay / độ mờ / keyframe **không** là nút trong thẻ: chúng nằm ở tab
  Thông số (Task 6) và vẫn được tính vào 41 chức năng. Mặc định chữ AYP: `graphics.newText`
  `{"text":"Chữ mới","font":"Be Vietnam Pro","fontStyle":"SemiBold"}` (đọc params thật của `graphics.newText`
  để đặt đúng tên trường; tracking −60 nếu lệnh nhận). Chuyển cảnh mặc định
  `{"effect":"cross_dissolve","frames":12}`. Id hiệu ứng đúng: `speech_enhance` (không phải `enhance_speech`).
- [ ] **Step 4:** Test PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): tab table mapping the 41 built-in features`.

### Task 4: Workspace "AYP", mặc định cho AYPEditor, Chế độ nâng cao, tự lưu bố cục

**Files:**
- Modify: `crates/ui-egui/src/dock.rs`: thêm `AypLibrary`, `AypSide`, `AypToolbar` vào `PanelKind`; `ALL`
  `[PanelKind; 31]`; `title()` = "AYP Library" / "AYP Side" / "AYP Toolbar"; `compact()` true cho cả ba;
  `WORKSPACES` thêm "AYP" (độ dài 10); nhánh `"AYP"` trong `workspace()`; hàm thuần
  `pub fn initial_workspace(prefs: &WorkspacePrefs, ayp_default: bool, already_introduced: bool) -> Option<String>`
- Modify: `crates/ui-egui/src/lib.rs`: field `pub ayp_default: bool` trên `FilmcraftApp` (mặc định false). Áp dụng
  trong `apply_prefs` ở chỗ khôi phục workspace lần đầu (`workspace_restored`, khoảng `lib.rs:755`). Dấu "đã
  giới thiệu" là file `aypeditor.json` (`{"introduced":true}`) trong `default_data_dir()`.
- Modify: `apps/filmcraft/src/main.rs`: `app.ayp_default = std::env::var("AYP_EDITOR").is_ok_and(|v| v == "1")`
- Modify: `crates/ui-egui/src/menus.rs`: UI command `uic!("window.ayp.advanced", "Advanced Mode", ["Window"], None)`
  → workspace "Editing"; `uic!("window.ayp.simple", "AYP Interface", ["Window"], None)` → workspace "AYP".
  **Không** gán Alt+Shift+7 (đã thuộc Learning).
- Modify: `crates/ui-egui/src/panels/mod.rs`: dispatch 3 panel mới sang `crate::ayp::{library,side,toolbar}::show`
  (tạm vẽ một nhãn; Task 5–7 làm tiếp)
- Modify: `crates/ui-egui/tests/workspaces_ui.rs:163-178`: thêm "AYP" sau "Audio" trong danh sách
- Modify: `crates/ui-egui/src/i18n/{es,zh-cn,ja,pt-br}.tsv`: các chuỗi test báo thiếu ("AYP", "AYP Library",
  "AYP Side", "AYP Toolbar", "Advanced Mode", "AYP Interface")
- Test: `crates/ui-egui/tests/ayp_workspace_ui.rs`, unit test `initial_workspace` trong `dock.rs`

**Interfaces:**
- Bố cục "AYP":
  `vsplit(Ratio(0.6), hsplit(FixedA(300.0), tabs(&[AypLibrary],0), hsplit(FixedB(360.0), tabs(&[Program],0), tabs(&[AypSide],0))), vsplit(FixedA(34.0), tabs(&[AypToolbar],0), tabs(&[Timeline],0)))`
- `initial_workspace`: `ayp_default && !already_introduced` → `Some("AYP")`; còn lại → `None` (giữ cách cũ).
- **Tự lưu bố cục AYP:** khi `app.ayp_default` và workspace là "AYP", mỗi lần dock đổi (kéo cỡ, đóng/mở tab)
  thì lưu bằng đường "Save Changes to this Workspace" có sẵn (`panels/workspaces.rs`). Không ghi file mỗi frame:
  chỉ khi layout khác bản đã lưu.

- [ ] **Step 1: Test hỏng.** Unit: `initial_workspace` cho 4 tổ hợp (ayp_default × introduced). UI
  `ayp_workspace_ui.rs` (harness như `tests/chrome_ui.rs`):
  - `ayp_default_opens_ayp`: `app.ayp_default = true`, dữ liệu trong thư mục tạm (`FILMCRAFT_DATA_DIR`), thì
    workspace "AYP", dock chứa đủ 5 panel, và `aypeditor.json` được ghi.
  - `second_launch_keeps_the_last_workspace`: đã có `aypeditor.json`, `current:"Editing"`, thì giữ "Editing".
  - `plain_filmcraft_still_opens_editing`: `ayp_default = false` thì "Editing".
  - `advanced_and_simple_switch`: `window.ayp.advanced` → "Editing"; `window.ayp.simple` → "AYP".
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui --test ayp_workspace_ui` và `cargo test -p filmcraft-ui-egui dock::`. Expected: FAIL.
- [ ] **Step 3:** Làm các thay đổi trên.
- [ ] **Step 4:** Test mới PASS. Chạy toàn crate ui-egui và engine. Sửa **chỉ** các chỗ được phép ở Global Constraints. Expected: tất cả PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): AYP workspace, default in AYPEditor, advanced-mode switch, layout autosave`.

### Task 5: Panel thẻ bên trái (`AypLibrary`)

**Files:**
- Create: `crates/ui-egui/src/ayp/library.rs`
- Modify: `crates/ui-egui/src/state.rs` (`UiState.ayp_tab: String`, `#[serde(default = "default_ayp_tab")]` = `"media"`)
- Test: `crates/ui-egui/tests/ayp_library_ui.rs`

**Interfaces:**
- Consumes: `ayp::tabs::TABS`, `ayp::vi::{vi, reason}`, `menus::invoke`, `panels::show`.
- Produces: `pub fn show(app:&mut FilmcraftApp, ui:&mut egui::Ui, rect:egui::Rect)`. Automation id: thẻ
  `ayp.tab.<id>`, nút `ayp.item.<key>`, gợi ý `ayp.hint.noSequence`, nút tìm `ayp.find`.
- Hàng trên cùng của panel: tên project · tỉ lệ khung (ví dụ "0716 · 9:16") và nút **"Tìm lệnh…"** (`ayp.find`,
  mở palette ở Task 8; tạm thời gọi UI command `ayp.palette.open`, khai ở Task 8, nên trước Task 8 nút này có thể
  chưa có gì xảy ra).

- [ ] **Step 1: Test hỏng** `ayp_library_ui.rs`. Dùng mẫu Driver trong `tests/menus_ui.rs` hoặc gọi
  `control::handle` như `tests/ui_control_input.rs:218`. Chọn clip bằng
  `sequence.inspect` → `["video"][0]["items"][0]["clip"]` → `timeline.select {"clips":[id]}`; bỏ chọn bằng `edit.deselectAll`.
  - `tabs_are_listed_and_switch`: đủ 10 phần tử `ayp.tab.*`. Click `ayp.tab.captions` thì `ui.ayp_tab == "captions"`
    và có `ayp.item.transcript.createCaptionsVi`.
  - `clicking_an_item_runs_its_command`: demo project, chọn clip, mở thẻ effects, click
    `ayp.item.effects.apply.gaussian_blur`. Assert `session.history.undo.len()` tăng đúng 1.
  - `a_failing_command_says_why_in_vietnamese`: bỏ chọn, click `ayp.item.effects.apply.gaussian_blur`. Assert
    `ui.status` bắt đầu bằng "Chưa làm được:", không panic, history không đổi.
  - `no_open_sequence_shows_a_hint`: `Session::default()`. Có `ayp.hint.noSequence`, không panic.
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui --test ayp_library_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết `library.rs`:
  - Lưới thẻ (icon + nhãn, 2 hàng).
  - Danh sách nút của thẻ đang mở: nút rộng, chữ tiếng Việt, tooltip là id lệnh. Lệnh có `enabled()` lỗi thì mờ đi.
  - Bấm nút → `invoke`. Lỗi → `ui.status = format!("Chưa làm được: {}", vi::reason(&e))`.
  - Phần còn lại vẽ panel `embed`.
- [ ] **Step 4:** Test PASS, cả crate PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): left tab library with one-click features`.

### Task 6: Panel bên phải (`AypSide`): Thông số | AI

**Files:**
- Create: `crates/ui-egui/src/ayp/side.rs`
- Modify: `crates/ui-egui/src/state.rs` (`UiState.ayp_side: AypSideTab`, enum derive
  `Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize`, mặc định `Props`)
- Modify: `crates/ui-egui/src/menus.rs` (UI command `ayp.side.props` "Properties Tab" và `ayp.side.ai` "AI Tab",
  không menu, không phím tắt; route vào `ui.ayp_side`)
- Test: `crates/ui-egui/tests/ayp_side_ui.rs`

**Interfaces:**
- Produces: `pub fn show(app, ui, rect)`. Hai nút tab `ayp.side.props` / `ayp.side.ai`.
  - Tab Thông số gọi `panels::show(app, ui, PanelKind::Properties, inner)`. Panel này tự chọn graphics hay clip
    theo đối tượng đang chọn, nên Thông số tự đổi theo loại clip. Chia nhóm con kiểu CapCut **để sang kế hoạch 3**.
  - Chưa chọn gì thì hiện `ayp.hint.selectClip` ("Chọn một clip để xem thông số").
  - Tab AI ở kế hoạch này chỉ có dòng "Khung chat AI sẽ có ở bản kế tiếp. Trong lúc chờ: bấm Tìm lệnh…" cùng nút
    `ayp.side.openPalette`.

- [ ] **Step 1: Test hỏng:**
  - `props_tab_shows_the_selected_clip`: chọn clip thì có phần tử automation của Properties panel (tìm id bằng
    `h.state().auto.query("")` trong demo).
  - `switching_to_ai`: click `ayp.side.ai` thì `ui.ayp_side == Ai`.
  - `no_selection_hint`: không chọn gì thì có `ayp.hint.selectClip`.
  - `no_sequence_does_not_panic`.
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui --test ayp_side_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết `side.rs` và các UI command.
- [ ] **Step 4:** Test PASS, cả crate PASS (catalog es/zh-cn/ja/pt-br cho nhãn UI command mới nếu test đòi).
- [ ] **Step 5:** Commit `feat(aypeditor): right side panel with properties and AI tab`.

### Task 7: Thanh công cụ timeline (`AypToolbar`)

**Files:**
- Create: `crates/ui-egui/src/ayp/toolbar.rs`
- Test: `crates/ui-egui/tests/ayp_toolbar_ui.rs`

**Interfaces:**
- Produces: `pub fn show(app, ui, rect)` với các nút theo đúng thứ tự (id automation `ayp.tool.<cmd>`):

| Nhãn | Lệnh | Params |
|---|---|---|
| Hoàn tác | `edit.undo` | `{}` |
| Làm lại | `edit.redo` | `{}` |
| Tách | `sequence.addEdit` | `{}` (tại playhead) |
| Xoá | `edit.rippleDelete` | `{}` |
| Dừng hình | `clip.frameHold` | `{}` |
| Liên kết | `clip.link` | `{}` |
| Hút | `sequence.snap` | `{}` (bật/tắt, sáng khi `session.state.snapping`) |
| Marker | `markers.add` | `{"target":"program"}` |
| Thu nhỏ | `view.zoomOut` | `{}` |
| Phóng to | `view.zoomIn` | `{}` |
| **Xuất** | `export.quick` | `{}` (sát phải, màu nhấn) |

  Biểu tượng ưu tiên `crate::icons`. Nút keyframe ◆ **để sang kế hoạch 3**.

- [ ] **Step 1: Test hỏng:**
  - `buttons_are_in_order`.
  - `split_adds_an_edit`: demo, `playhead.set` vào giữa clip đầu V1, click `ayp.tool.sequence.addEdit`, thì số
    item trên V1 (`sequence.inspect`) tăng 1.
  - `undo_reverts_it`.
  - `snap_toggles`: hai lần click thì `session.state.snapping` đổi rồi trở lại.
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui --test ayp_toolbar_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết `toolbar.rs`. Bấm nút nào lỗi thì ghi status như Task 5.
- [ ] **Step 4:** Test PASS, cả crate PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): timeline toolbar`.

### Task 8: "Tìm lệnh…" — tìm và chạy mọi lệnh, không dấu cũng tìm được

**Files:**
- Create: `crates/ui-egui/src/ayp/palette.rs`
- Modify: `crates/ui-egui/src/menus.rs`: UI command `uic!("ayp.palette.open", "Find Command…", ["Window"], Some("Cmd+Shift+P"))`
  (kiểm phím không trùng bằng lệnh `shortcuts.conflicts` hoặc test bàn phím có sẵn; trùng thì chọn phím trống
  khác và ghi vào report). **Không** đổi ⌘K: đó là phím Add Edit, có test của engine ghim.
- Modify: `crates/ui-egui/src/lib.rs`: vẽ palette trong `FilmcraftApp::frame`, ngay sau `panels::dialogs::show(self,&ctx)` (khoảng `lib.rs:1794`)
- Modify: catalog es/zh-cn/ja/pt-br cho "Find Command…"
- Test: unit test trong `palette.rs` + `crates/ui-egui/tests/ayp_palette_ui.rs`

**Interfaces:**
- Produces:
```rust
pub struct Hit { pub id: &'static str, pub label_vi: String, pub label_en: &'static str, pub enabled: Result<(), String>, pub score: i32 }
pub fn fold(s: &str) -> String;           // bỏ dấu tiếng Việt + chữ thường: "Xoá khoảng lặng Đ" -> "xoa khoang lang d"
pub fn search(app: &FilmcraftApp, query: &str, limit: usize) -> Vec<Hit>;
pub fn show(app: &mut FilmcraftApp, ctx: &egui::Context);
```
  - **Nguồn:** `filmcraft_engine::command_specs()` cộng `menus::UI_COMMANDS`.
  - **Khớp** theo `fold` của: nhãn tiếng Việt, từ đồng nghĩa, nhãn tiếng Anh, id.
  - **Điểm cao hơn** khi: khớp đầu từ, khớp tiếng Việt, lệnh đang dùng được, lệnh có trong `ayp::tabs`.
  - **Trạng thái:** `app.ui.ayp_palette: Option<String>` (None = đóng; `#[serde(skip)]`).
  - **Phím:** Enter chạy lệnh đầu qua `invoke`. Lệnh bị tắt thì hiện lý do (`vi::reason`) và không chạy. Esc đóng.
  - **Automation id:** `ayp.palette`, `ayp.palette.hit.<id>`.
  - **Bảng chữ:** `fold` dùng bảng ký tự tiếng Việt viết tay (không thêm crate).

- [ ] **Step 1: Test hỏng (unit):**
```rust
#[test] fn fold_strips_vietnamese_marks() { assert_eq!(fold("Xoá khoảng lặng Đ"), "xoa khoang lang d"); }
#[test] fn finds_captions_with_or_without_marks() {
    let app = FilmcraftApp::new(filmcraft_engine::Session::default());
    for q in ["phụ đề tiếng việt", "phu de tieng viet"] { assert_eq!(search(&app, q, 5)[0].id, "transcript.createCaptionsVi"); }
}
#[test] fn finds_pauses_export_and_speed() {
    let app = FilmcraftApp::new(filmcraft_engine::Session::default());
    assert_eq!(search(&app, "xoa khoang lang", 5)[0].id, "transcript.removePauses");
    assert!(search(&app, "xuat", 5).iter().any(|h| h.id == "export.quick"));
    assert!(search(&app, "toc do", 5).iter().any(|h| h.id == "clip.speedDuration"));
}
#[test] fn every_command_is_searchable() {
    let app = FilmcraftApp::new(filmcraft_engine::Session::default());
    assert!(search(&app, "", usize::MAX).len() >= 700);
}
```
- [ ] **Step 2: Test hỏng (UI)** `ayp_palette_ui.rs`:
  - `invoke("ayp.palette.open")` thì có phần tử `ayp.palette`.
  - Gõ "tach" rồi Enter (playhead giữa clip) thì số item V1 tăng 1.
  - Esc thì đóng.
  - Click nút `ayp.find` (Task 5) thì cũng mở.
- [ ] **Step 3:** `cargo test -p filmcraft-ui-egui palette` và `--test ayp_palette_ui`. Expected: FAIL.
- [ ] **Step 4:** Viết `palette.rs` và các chỗ nối.
- [ ] **Step 5:** Test PASS, cả crate PASS.
- [ ] **Step 6:** Commit `feat(aypeditor): Find Command palette over every command, accent-insensitive`.

### Task 9: Control cho AI: đọc và điều khiển giao diện AYP

**Files:**
- Modify: `crates/ui-egui/src/control.rs` (thêm 3 method)
- Modify: `docs/control-protocol.md`
- Test: `crates/ui-egui/tests/ayp_control_ui.rs`

**Interfaces:**
- Produces:
  - `ui.ayp.state` →
    `{"workspace":str,"tab":str,"side":"props"|"ai","palette":str|null,"selection":[clipId],"playhead":ticks,"sequence":id|null}`
  - `ui.ayp.tabs` → `[{"id","label","items":[{"key","cmd","params","label"}]}]` (dựng từ `TABS`)
  - `ui.ayp.open {"tab":str?,"side":"props"|"ai"?}`: đổi sang workspace AYP nếu đang ở chỗ khác, mở đúng thẻ/tab.
    Thẻ không có thì trả lỗi.

- [ ] **Step 1: Test hỏng.** Gọi `control::handle(app, ctx, &ControlRequest::new(method, params))` như
  `tests/ui_control_input.rs:218`:
  - Hình dạng JSON của `ui.ayp.state` và `ui.ayp.tabs` (10 thẻ).
  - `ui.ayp.open {"tab":"captions"}` thì `ui.ayp.state.tab == "captions"`.
  - `{"tab":"nope"}` thì trả lỗi.
- [ ] **Step 2:** `cargo test -p filmcraft-ui-egui --test ayp_control_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết 3 method và ghi tài liệu.
- [ ] **Step 4:** Test PASS, cả crate PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): control methods for the AYP interface`.

### Task 10: Chạy thật 41 chức năng trên clip thật, kiểm project cũ, kiểm công cụ AYP

**Files:**
- Create: `tools/ayp-editor/check_features.py`
- Create: `docs/aypeditor-check.md` (kết quả do script sinh, kèm phần ghi tay ở cuối)
- Create: `docs/images/aypeditor-ayp-workspace.png`

**Interfaces:**
- Consumes: AYPEditor.app (Task 1, build lại), `ui.ayp.tabs` (Task 9), `engine.execute {"command"|"id","params"}`.

- [ ] **Step 1:** `scripts/build-aypeditor.sh --install`. Chạy app theo luật cổng ở Global Constraints.
- [ ] **Step 2: Chuẩn bị dữ liệu.** Chép `/Volumes/Extreme SSD/0716.mp4` sang `$TMPDIR/aype/0716.mp4` bằng shell. Nếu
  không có thì dùng `/Users/haule/Documents/render/0721.mp4` và đánh SKIP các mục cần lời nói.
- [ ] **Step 3: Viết `check_features.py`** (Python 3, chỉ stdlib, JSON-lines `{"id","method","params"}`, cổng
  là tham số `--port`):
  1. `file.newProject`; `file.import {"paths":[...]}`; `file.newSequence` 1080×1920 `fromItem`; `file.saveAs`
     với `path` trong `$TMPDIR`.
  2. Với **mỗi item** của `ui.ayp.tabs`:
     - Chuẩn bị điều kiện: chọn clip, đặt playhead giữa clip, đã có transcript cho các lệnh `transcript.*`.
     - Chạy `engine.execute`.
     - Lệnh lâu (`transcript.generate`, `export.*`, `tts.*`): chạy job và poll `jobs.list`.
     - Lệnh cần hộp thoại hoặc quyền hệ thống (`audio.voiceover.*` cần micro): ghi MANUAL.
  3. Kiểm kết quả:
     - Lệnh sửa project: `history` có thêm bước.
     - Lệnh xuất: đặt bitrate ~8 Mbps, `ffprobe` thời lượng > 0.
     - `review.open`: trang 8765 trả 200.
     - Ghi PASS / FAIL (kèm lỗi) / SKIP / MANUAL.
  4. Ghi bảng Markdown vào `docs/aypeditor-check.md`.
- [ ] **Step 4:** Chạy script. Mục FAIL do code AYPEditor thì sửa rồi chạy lại. Mục FAIL do lệnh gốc FilmCraft thì
  ghi rõ trong bảng, **không giấu**.
- [ ] **Step 5: Project cũ.** Tìm một file `.filmcraft` có sẵn (`mdfind -name .filmcraft | head`). Chép vào
  `$TMPDIR`, mở bằng `file.open {"path":...}`, `sequence.inspect`. Xuất 3 giây đầu (`markers.markIn/markOut` +
  `export.quick`, rồi `markers.clearInOut`). Ghi kết quả vào `docs/aypeditor-check.md`.
- [ ] **Step 6: Công cụ AYP.** Chạy `python3 tools/ayp-review/review.py --help` (hoặc lệnh dump tương đương) và một
  lệnh đọc của `tools/ayp-motion/glass_lib.py` (nếu có `--help`/self-test). Gửi `engine.commands` qua cổng
  đang dùng. Ghi kết quả.
- [ ] **Step 7:** `ui.screenshot` của workspace AYP vào `docs/images/aypeditor-ayp-workspace.png`. Mở ảnh xem: chữ
  tiếng Việt có dấu hiện đúng, bố cục đúng spec mục 4, biểu tượng không bị ô vuông. Có lỗi hiển thị thì sửa.
- [ ] **Step 8:** Thoát bản thử (`app.quit`). Commit `test(aypeditor): real-clip check of the 41 built-in features`.
