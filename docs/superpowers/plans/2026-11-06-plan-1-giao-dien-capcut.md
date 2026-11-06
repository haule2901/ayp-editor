# AYPEditor — Kế hoạch 1: giao diện kiểu CapCut + 41 chức năng có sẵn

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** AYPEditor.app mở ra với giao diện kiểu CapCut (thẻ trái, player giữa, tab Thông số/AI bên phải,
thanh công cụ + timeline dưới, ô ⌘K). 41 chức năng ✅ của spec nằm đúng thẻ và đều chạy được bằng nút
lẫn qua control 9879.

**Architecture:** Giao diện mới là **một workspace "AYP"** trong hệ dock có sẵn, gồm 3 PanelKind mới
(`AypLibrary`, `AypSide`, `AypToolbar`) và 2 panel cũ (`Program`, `Timeline`). Mọi nút gọi
`crate::menus::invoke(app, ctx, id, params)`, giống các panel cũ. Toàn bộ code mới nằm trong module
`crates/ui-egui/src/ayp/`. File dùng chung chỉ bị sửa ở các điểm nối được nêu tên trong từng task.
Chữ của giao diện mới viết thẳng bằng tiếng Việt qua bảng `ayp/vi.tsv` (không qua `tl!`), nên các
test i18n hiện có không phải thêm bản dịch cho chúng.

**Tech Stack:** Rust (edition của workspace), egui/eframe 0.36, egui_kittest cho test giao diện,
`cargo test -p filmcraft-ui-egui`, `cargo test -p filmcraft-engine`.

**Spec:** `docs/superpowers/specs/2026-11-06-ayp-editor-design.md` (đọc mục 2, 3, 4, 5.1, 9.1 và 10)

## Global Constraints

- Repo: `/Users/haule/App Creator/AYP-Editor`, nhánh `main`. Commit nhỏ sau mỗi task. **Không push**
  cho tới khi Task 10 xong (người điều phối sẽ push).
- Tên app **AYPEditor**, bundle id `vn.ayp.editor`, cài ở `~/Applications/AYPEditor.app`. **Không bao
  giờ** sửa, xoá hay đè `~/Applications/FilmCraft AYP.app` hoặc repo `App Creator/filmcraft`.
- AYPEditor dùng chung thư mục dữ liệu với FilmCraft AYP và mặc định bật control trên cổng **9879**.
- Ổ máy chỉ còn khoảng 7 GB. Build với `CARGO_INCREMENTAL=0` và
  `CARGO_TARGET_DIR="/Users/haule/App Creator/filmcraft/target"` (dùng lại cache build sẵn).
  Nếu `df -h /` báo dưới 2 GB thì dừng lại và báo người điều phối, không tự xoá gì.
- Mọi thay đổi project phải đi qua lệnh (`menus::invoke` / `Session::execute`). Không sửa `Project` trực tiếp từ UI.
- Không chép hình ảnh, biểu tượng hay tài nguyên của CapCut. Biểu tượng dùng `crate::icons` có sẵn
  hoặc ký tự Unicode.
- Chữ trên giao diện bằng tiếng Việt. Tên gọi theo spec mục 3 (Phương tiện, Âm thanh, Văn bản, Phụ đề,
  Hiệu ứng, Chuyển tiếp, Bộ lọc, Tách, Dừng hình, Hút vào đoạn chính…).
- Font chữ giao diện phải hiện đúng tiếng Việt có dấu (kiểm bằng ảnh chụp `ui.screenshot`).
- Test hiện có của crate phải vẫn xanh: `cargo test -p filmcraft-ui-egui` và
  `cargo test -p filmcraft-engine` (chạy cả hai trước khi commit task có sửa file dùng chung).

## Review Focus

1. **Không có clip nào được chọn** khi bấm một nút cần clip: lệnh trả lỗi `enabled()`. Phải hiện lý do
   bằng tiếng Việt ở thanh trạng thái, không panic, không im lặng. Test ở Task 5.
2. **Người dùng cũ đã có `workspaces.json` với `current: "Editing"`**: AYPEditor lần đầu mở vẫn phải vào
   workspace AYP, và chỉ chuyển một lần. Sau đó lần mở nào cũng mở lại workspace dùng lần trước. Test ở Task 4.
3. **Gõ không dấu trong ⌘K** ("xoa khoang lang", "phu de"): vẫn tìm ra lệnh đúng. Test ở Task 8.
4. **Project cũ có nhiều track hoặc không có sequence nào đang mở**: các panel mới không panic và hiện
   hướng dẫn mở hoặc tạo sequence. Test ở Task 5 và Task 6.
5. **Cổng 9879 đang bận** (FilmCraft AYP đang mở): AYPEditor vẫn mở được và báo rõ "cổng điều khiển
   9879 đang bận". Kiểm thủ công ở Task 1.

---

### Task 1: Đóng gói AYPEditor.app

**Files:**
- Create: `scripts/build-aypeditor.sh`
- Modify: điểm đặt tên app / thư mục dữ liệu / cổng control ở `apps/filmcraft/src/main.rs` và
  `apps/filmcraft/src/args.rs` (đọc `AYP.md` và `packaging/macos/Info.plist.in` để biết bản FilmCraft AYP
  hiện được ghép bundle thế nào)
- Create: `packaging/macos/AYPEditor.Info.plist` (hoặc sinh từ `Info.plist.in`)

**Interfaces:**
- Produces: lệnh `scripts/build-aypeditor.sh [--install]` build release
  (`cargo build --release -p filmcraft --features whisper,vieneu-voices`) rồi ghép
  `target/AYPEditor.app`. Với `--install` thì chép sang `~/Applications/AYPEditor.app`. Các task sau
  dùng app này để chạy thử.

- [ ] **Step 1:** Đọc `AYP.md`, `packaging/macos/*`, `apps/filmcraft/src/{main.rs,args.rs}` để biết: bundle
  được ghép thế nào, thư mục dữ liệu lấy từ đâu (phải giữ **cùng thư mục dữ liệu** như FilmCraft AYP), cổng
  control bật bằng cờ hay biến môi trường nào.
- [ ] **Step 2:** Viết `scripts/build-aypeditor.sh`: build release với env ở Global Constraints, ghép bundle
  với `CFBundleName`/`CFBundleDisplayName` = `AYPEditor`, `CFBundleIdentifier` = `vn.ayp.editor`, icon giữ
  icon hiện có. Khi chạy từ Finder, app bật control trên cổng 9879 (qua `LSEnvironment` trong Info.plist
  hoặc mặc định trong `args.rs` khi bundle id là `vn.ayp.editor`; chọn cách nào ít sửa code nhất). Tên cửa sổ
  và menu app là "AYPEditor".
- [ ] **Step 3:** Nếu cổng 9879 đang bận thì app vẫn mở và hiện thông báo
  "Cổng điều khiển 9879 đang bận — AI/MCP chưa nối được." ở thanh trạng thái (không thoát).
- [ ] **Step 4: Chạy thử.** Thoát FilmCraft AYP nếu đang mở (`osascript -e 'quit app "FilmCraft AYP"'`), chạy
  `scripts/build-aypeditor.sh --install`, rồi `open -a ~/Applications/AYPEditor.app`. Kiểm bằng:
  `python3 -c` gửi `{"id":1,"method":"engine.commands","params":{}}` tới 127.0.0.1:9879.
  Expected: `ok: true` và có ≥ 700 lệnh, trong đó có `transcript.createCaptionsVi`.
- [ ] **Step 5:** Commit `feat(aypeditor): build and bundle AYPEditor.app (vn.ayp.editor, control 9879)`.

### Task 2: Bảng chữ tiếng Việt cho giao diện mới

**Files:**
- Create: `crates/ui-egui/src/ayp/mod.rs`, `crates/ui-egui/src/ayp/vi.rs`, `crates/ui-egui/src/ayp/vi.tsv`
- Modify: `crates/ui-egui/src/lib.rs` (thêm `pub mod ayp;`), `crates/ui-egui/src/i18n/mod.rs`
  (`mod catalog;` → `pub(crate) mod catalog;`)

**Interfaces:**
- Produces: `pub fn vi(english: &str) -> &str` trong `ayp::vi`. Trả bản tiếng Việt từ `vi.tsv` (định dạng
  giống `es.tsv`, đọc bằng `i18n::catalog::Catalog::parse`, cột context để trống); không có thì trả lại
  `english`. `pub fn synonyms(english: &str) -> &'static [&'static str]`: từ đồng nghĩa tiếng Việt dùng cho
  ⌘K, đọc từ dòng có context `syn` (cột dịch chứa các từ cách nhau bằng `|`).

- [ ] **Step 1: Viết test hỏng** trong `ayp/vi.rs`:
```rust
#[test] fn known_labels_are_vietnamese() {
    assert_eq!(vi("Split"), "Tách");
    assert_eq!(vi("Freeze Frame"), "Dừng hình");
    assert_eq!(vi("no such label"), "no such label");
}
#[test] fn synonyms_are_listed() { assert!(synonyms("Remove Pauses").contains(&"khoảng lặng")); }
#[test] fn every_command_label_has_vietnamese() {
    // mọi lệnh engine có trong menu đều có bản tiếng Việt (để ⌘K tìm bằng tiếng Việt)
    let missing: Vec<_> = filmcraft_engine::command_specs().iter()
        .filter(|c| !c.menu.is_empty() && vi(c.label) == c.label).map(|c| c.label).collect();
    assert!(missing.is_empty(), "{missing:#?}");
}
```
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui ayp::vi`. Expected: FAIL (module chưa có).
- [ ] **Step 3:** Viết `vi.rs` và `vi.tsv`. Dịch **mọi nhãn lệnh có menu** (khoảng 400 nhãn; lấy danh sách
  từ test hỏng) theo từ vựng ở Global Constraints. Thêm dòng `syn` cho ít nhất các lệnh trong bảng thẻ ở
  Task 3. File có header ghi rõ: viết từ nghĩa thông thường của nhãn tiếng Anh, không chép bản dịch của
  phần mềm khác.
- [ ] **Step 4:** Chạy lại test. Expected: PASS. Chạy cả `cargo test -p filmcraft-ui-egui i18n`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): Vietnamese catalog for the AYP interface`.

### Task 3: Bảng thẻ — chức năng nào nằm ở thẻ nào

**Files:**
- Create: `crates/ui-egui/src/ayp/tabs.rs`

**Interfaces:**
- Produces:
```rust
pub struct Item { pub cmd: &'static str, pub params: &'static str /* JSON, "{}" khi không có */, pub label: &'static str /* tiếng Việt */ }
pub struct Tab { pub id: &'static str, pub icon: &'static str /* ký tự Unicode */, pub label: &'static str, pub items: &'static [Item], pub embed: Option<crate::dock::PanelKind> }
pub const TABS: &[Tab];
pub fn find_cmd(cmd: &str) -> Option<(&'static Tab, &'static Item)>;
```
  `embed` là panel cũ được vẽ bên dưới danh sách nút của thẻ (ví dụ Phương tiện → `Project`,
  Hiệu ứng → `Effects`, Văn bản → `Text`, Giọng đọc → `TextToSpeech`, Âm thanh → `EssentialSound`,
  Bộ lọc → `LumetriColor`).

- [ ] **Step 1: Test hỏng** trong `tabs.rs`:
```rust
#[test] fn tab_ids_are_the_ten_tabs() {
    let ids: Vec<_> = TABS.iter().map(|t| t.id).collect();
    assert_eq!(ids, ["media","audio","text","captions","cut","voice","effects","transitions","color","motion"]);
}
#[test] fn every_item_is_a_real_command() {
    for t in TABS { for i in t.items {
        assert!(filmcraft_engine::find_command(i.cmd).is_some() || crate::menus::UI_COMMANDS.iter().any(|u| u.id == i.cmd), "{}", i.cmd);
        assert!(serde_json::from_str::<serde_json::Value>(i.params).is_ok(), "{}", i.cmd);
    }}
}
#[test] fn the_must_have_features_are_placed() {
    for c in ["file.import","audio.voiceover.start","clip.extractAudio","essentialSound.autoMatch","essentialSound.generateDucking",
              "graphics.newText","transcript.createCaptionsVi","transcript.captionsFromChunks","captions.applyPreset",
              "transcript.removePauses","transcript.removeFillers","transcript.lift","tts.create","effects.apply",
              "sequence.applyVideoTransition","lumetri.applyPreset","lumetri.applyMatch","clip.speedDuration","clip.frameHold",
              "masks.add","masks.track","clip.nest","clip.group","review.open","review.import","export.quick"] {
        assert!(find_cmd(c).is_some(), "{c} không nằm ở thẻ nào");
    }
}
```
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui ayp::tabs`. Expected: FAIL.
- [ ] **Step 3:** Viết `TABS` gồm 10 thẻ (id như test). Phân bổ đủ 41 mục ✅ ở spec 5.1. Mục là hiệu ứng
  cụ thể thì dùng `effects.apply` với params, ví dụ `{"effect":"skin_smooth"}`, `{"effect":"gaussian_blur"}`,
  `{"effect":"warp_stabilizer"}`, `{"effect":"auto_reframe"}`, `{"effect":"ultra_key"}`, `{"effect":"horizontal_flip"}`,
  `{"effect":"crop"}`, `{"effect":"denoise"}`, `{"effect":"enhance_speech"}`. Lấy id đúng từ
  `effects.list` (`engine.execute`) và sửa lại nếu id khác. Mục chuyển cảnh mặc định là
  `{"effect":"cross_dissolve","frames":12}`.
- [ ] **Step 4:** Chạy test. Expected: PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): tab table mapping the 41 built-in features`.

### Task 4: Workspace "AYP", panel mới, mặc định và Chế độ nâng cao

**Files:**
- Modify: `crates/ui-egui/src/dock.rs` (thêm `AypLibrary`, `AypSide`, `AypToolbar` vào `PanelKind`, `ALL`,
  `title()` = "AYP Library"/"AYP Side"/"AYP Toolbar", `compact()` trả true cho cả ba; thêm "AYP" vào
  `WORKSPACES`; nhánh `"AYP"` trong `workspace()`)
- Modify: `crates/ui-egui/src/dock.rs` `WorkspacePrefs` (thêm field `#[serde(default)] pub ayp_intro: bool`)
- Modify: `crates/ui-egui/src/state.rs` (mặc định `workspace: "AYP"`, `dock: workspace("AYP")`)
- Modify: `crates/ui-egui/src/menus.rs` (`uic!("window.workspace.ayp", "AYP", ["Window","Workspaces"], Some("Alt+Shift+7"))`)
- Modify: `crates/ui-egui/src/panels/mod.rs` (dispatch 3 panel mới sang `crate::ayp::{library,side,toolbar}::show`; mỗi file tạm vẽ một nhãn, Task 5–7 làm tiếp)
- Modify: `crates/ui-egui/src/i18n/{es,zh-cn,ja,pt-br}.tsv` (thêm đúng những chuỗi mà test i18n báo thiếu)
- Test: `crates/ui-egui/tests/ayp_workspace_ui.rs`

**Interfaces:**
- Produces: `PanelKind::{AypLibrary, AypSide, AypToolbar}`. Bố cục workspace "AYP":
  `vsplit(Ratio(0.6), hsplit(FixedA(300.0), tabs(&[AypLibrary],0), hsplit(FixedB(360.0), tabs(&[Program],0), tabs(&[AypSide],0))), vsplit(FixedA(34.0), tabs(&[AypToolbar],0), tabs(&[Timeline],0)))`.
  Lần đầu AYPEditor nạp `WorkspacePrefs` có `ayp_intro == false` thì đặt `current = "AYP"`,
  `ayp_intro = true`, rồi lưu. Thanh menu có mục **"Chế độ nâng cao"** (`window.workspace.editing`) và
  **"Giao diện AYP"** (`window.workspace.ayp`) ở chỗ dễ thấy (menu Window, cạnh mục Workspaces).

- [ ] **Step 1: Test hỏng** `tests/ayp_workspace_ui.rs` (mẫu harness như `tests/chrome_ui.rs`):
  - `fresh_app_opens_the_ayp_workspace`: app mới với demo project. Assert `app.ui.workspace == "AYP"` và
    dock chứa đủ `AypLibrary, Program, AypSide, AypToolbar, Timeline`.
  - `advanced_mode_switches_to_editing_and_back`: `menus::invoke(..,"window.workspace.editing",{})` thì
    workspace là "Editing"; `window.workspace.ayp` thì về "AYP".
  - `old_prefs_with_editing_are_moved_once`: `WorkspacePrefs{current:"Editing", ayp_intro:false}` được áp
    dụng → "AYP" và `ayp_intro == true`. Áp tiếp `WorkspacePrefs{current:"Editing", ayp_intro:true}` thì
    giữ "Editing".
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui --test ayp_workspace_ui`. Expected: FAIL.
- [ ] **Step 3:** Làm các thay đổi ở mục Files. Chỗ áp `ayp_intro` là nơi app nạp `WorkspacePrefs` lúc khởi
  động (tìm `WorkspacePrefs::load`).
- [ ] **Step 4:** Chạy test mới, rồi `cargo test -p filmcraft-ui-egui` (toàn crate). Với mỗi chuỗi test i18n báo
  thiếu ("AYP", "AYP Library"…), thêm dòng vào các catalog được nêu. Ở `es`/`pt-br`/`zh-cn`/`ja`,
  "AYP" giữ nguyên là "AYP". Expected: tất cả PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): AYP workspace as the default, advanced mode switch`.

### Task 5: Panel thẻ bên trái (`AypLibrary`)

**Files:**
- Create: `crates/ui-egui/src/ayp/library.rs`
- Test: `crates/ui-egui/tests/ayp_library_ui.rs`

**Interfaces:**
- Consumes: `ayp::tabs::TABS`, `ayp::vi::vi`, `menus::invoke`, `panels::show` (vẽ panel `embed`).
- Produces: `pub fn show(app:&mut FilmcraftApp, ui:&mut egui::Ui, rect:egui::Rect)`. Trạng thái thẻ đang mở
  lưu ở `app.ui.ayp_tab: String` (thêm vào `UiState`, `#[serde(default)]`, mặc định `"media"`). Automation
  id: thẻ `ayp.tab.<id>`, nút `ayp.item.<cmd>` (cùng cmd xuất hiện nhiều lần thì thêm `#<n>`).

- [ ] **Step 1: Test hỏng** `tests/ayp_library_ui.rs`:
  - `tabs_are_listed_and_switch`: có đủ 10 phần tử `ayp.tab.*`. Click `ayp.tab.captions` thì
    `app.ui.ayp_tab == "captions"` và có phần tử `ayp.item.transcript.createCaptionsVi`.
  - `clicking_an_item_runs_its_command`: demo project, chọn clip video đầu tiên của sequence (qua lệnh chọn
    clip có sẵn trong `command.list`), mở thẻ `effects`, click `ayp.item.effects.apply` (mục Gaussian Blur).
    Assert `app.session.history.undo.len()` tăng đúng 1.
  - `a_disabled_command_says_why_in_vietnamese`: bỏ chọn mọi clip, click một mục cần clip. Assert
    `app.ui.status` khác rỗng, không panic.
  - `no_open_sequence_shows_a_hint`: `Session::default()` không có sequence. Panel vẽ được, không panic, và có
    phần tử `ayp.hint.noSequence`.
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui --test ayp_library_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết `library.rs`. Trên cùng là lưới thẻ: biểu tượng + nhãn, 2 hàng, rộng theo panel.
  Dưới là danh sách nút của thẻ đang mở (nút rộng, chữ tiếng Việt, tooltip là id lệnh). Nút nào có
  `enabled()` lỗi thì vẫn hiện, chỉ mờ đi; bấm vào thì đưa lý do lên `app.ui.status` theo dạng
  "Chưa làm được: <lý do>". Dưới cùng vẽ panel `embed` trong phần còn lại. Lệnh có "…" (mở hộp thoại) chạy
  qua `menus::invoke` như menu.
- [ ] **Step 4:** Chạy test. Expected: PASS. Chạy cả crate.
- [ ] **Step 5:** Commit `feat(aypeditor): left tab library with one-click features`.

### Task 6: Panel bên phải (`AypSide`): Thông số | AI

**Files:**
- Create: `crates/ui-egui/src/ayp/side.rs`
- Test: `crates/ui-egui/tests/ayp_side_ui.rs`

**Interfaces:**
- Produces: `pub fn show(app, ui, rect)`. `app.ui.ayp_side: AypSideTab` (`enum AypSideTab { Props, Ai }`,
  serde, mặc định `Props`). Automation id `ayp.side.props`, `ayp.side.ai`. Tab Thông số vẽ đúng như
  `PanelKind::Properties` trong `panels::show` (gọi lại `panels::show(app, ui, PanelKind::Properties, inner)`).
  Tab AI ở kế hoạch này chỉ có dòng "Khung chat AI sẽ có ở bản kế tiếp. Trong lúc chờ: bấm ⌘K để tìm mọi lệnh."
  cùng nút `ayp.side.openPalette` mở ⌘K (Task 8). Kế hoạch 2 sẽ thay phần này.

- [ ] **Step 1: Test hỏng:** `props_tab_shows_the_selected_clip`: chọn một clip thì có phần tử automation
  của Properties panel (tìm id có sẵn bằng `h.state().auto.query("")` trong demo). `switching_to_ai`: click
  `ayp.side.ai` thì `app.ui.ayp_side == Ai`. `no_selection_hint`: không chọn gì thì có phần tử
  `ayp.hint.selectClip` ("Chọn một clip để xem thông số").
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui --test ayp_side_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết `side.rs`.
- [ ] **Step 4:** Chạy test, rồi cả crate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): right side panel with properties and AI tab`.

### Task 7: Thanh công cụ timeline (`AypToolbar`)

**Files:**
- Create: `crates/ui-egui/src/ayp/toolbar.rs`
- Test: `crates/ui-egui/tests/ayp_toolbar_ui.rs`

**Interfaces:**
- Produces: `pub fn show(app, ui, rect)` với các nút sau, theo đúng thứ tự (id automation
  `ayp.tool.<cmd>`):

| Nhãn | Lệnh | Params |
|---|---|---|
| ⟲ Hoàn tác | `edit.undo` | `{}` |
| ⟳ Làm lại | `edit.redo` | `{}` |
| ✂ Tách | `sequence.addEdit` | `{}` (tại playhead) |
| ⌫ Xoá | `edit.rippleDelete` | `{}` |
| ⏸ Dừng hình | `clip.frameHold` | `{}` |
| ⧉ Nhân đôi | `edit.duplicate` | `{}` |
| 🧲 Hút | `sequence.snap` | `{}` (nút bật/tắt, sáng khi đang bật) |
| ⚑ Marker | `markers.add` | `{"target":"program"}` |
| ⤓ Xuất | `export.quick` | `{}` (bên phải thanh, màu nhấn) |

- [ ] **Step 1: Test hỏng:** `buttons_are_in_order` (thứ tự id như bảng). `split_adds_an_edit`: demo project,
  playhead giữa clip, click `ayp.tool.sequence.addEdit` thì số clip trên V1 tăng 1.
  `undo_reverts_it`: click `ayp.tool.edit.undo` thì số clip về như cũ.
  `snap_toggles`: click hai lần thì trạng thái snap đổi rồi trở lại.
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui --test ayp_toolbar_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết `toolbar.rs`.
- [ ] **Step 4:** Chạy test, rồi cả crate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): timeline toolbar`.

### Task 8: Ô ⌘K tìm và chạy mọi lệnh

**Files:**
- Create: `crates/ui-egui/src/ayp/palette.rs`
- Modify: `crates/ui-egui/src/lib.rs` (trong `eframe::App::update`, mở palette khi bấm ⌘K / Ctrl+K và gọi
  `ayp::palette::show(app, ctx)` mỗi frame)
- Test: unit test trong `palette.rs` + `crates/ui-egui/tests/ayp_palette_ui.rs`

**Interfaces:**
- Produces:
```rust
pub struct Hit { pub id: &'static str, pub label_vi: String, pub label_en: &'static str, pub enabled: Result<(), String>, pub score: i32 }
pub fn fold(s: &str) -> String;           // bỏ dấu tiếng Việt + chữ thường: "Xoá khoảng lặng" -> "xoa khoang lang", "đ" -> "d"
pub fn search(app: &FilmcraftApp, query: &str, limit: usize) -> Vec<Hit>;
pub fn show(app: &mut FilmcraftApp, ctx: &egui::Context);
```
  Nguồn: `filmcraft_engine::command_specs()` cộng `menus::UI_COMMANDS`. Khớp theo `fold` của: nhãn tiếng
  Việt (`vi`), từ đồng nghĩa (`synonyms`), nhãn tiếng Anh, id. Điểm cao hơn khi khớp đầu từ, khớp nhãn tiếng
  Việt, lệnh đang dùng được, lệnh có trong `ayp::tabs`. App giữ `app.ui.ayp_palette: Option<String>` (None = đóng).
  Enter chạy lệnh đầu qua `menus::invoke`; lệnh bị tắt thì hiện lý do và không chạy. Esc đóng.
  Automation id: `ayp.palette`, `ayp.palette.hit.<id>`.

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
- [ ] **Step 2: Test hỏng (UI)** `ayp_palette_ui.rs`: gửi phím Cmd+K thì có phần tử `ayp.palette`. Gõ "tach"
  rồi Enter (với playhead giữa clip) thì số clip tăng 1. Gõ chữ rồi Esc thì palette đóng.
- [ ] **Step 3:** Chạy `cargo test -p filmcraft-ui-egui palette` và `--test ayp_palette_ui`. Expected: FAIL.
- [ ] **Step 4:** Viết `palette.rs` và chỗ nối trong `lib.rs`. `fold` dùng một bảng ký tự tiếng Việt viết tay
  (workspace chưa có crate `unicode-normalization`, đừng thêm).
- [ ] **Step 5:** Chạy test, rồi cả crate. Expected: PASS.
- [ ] **Step 6:** Commit `feat(aypeditor): ⌘K palette over every command, accent-insensitive`.

### Task 9: Control cho AI: đọc trạng thái giao diện mới

**Files:**
- Modify: `crates/ui-egui/src/control.rs` (thêm 3 method)
- Modify: `docs/control-protocol.md` (ghi các method mới)
- Test: `crates/ui-egui/tests/ayp_control_ui.rs`

**Interfaces:**
- Produces:
  - `ui.ayp.state` → `{"workspace":str,"tab":str,"side":"props"|"ai","palette":str|null,"selection":[clipId],"playhead":ticks,"sequence":id|null}`
  - `ui.ayp.tabs` → `[{"id","label","items":[{"cmd","params","label"}]}]` (dựng từ `ayp::tabs::TABS`)
  - `ui.ayp.open` `{"tab":str?,"side":"props"|"ai"?}` → mở đúng thẻ/tab (để nút "chỉ chỗ" của kế hoạch 2
    và AI bên ngoài dùng). Thẻ không có thì trả lỗi.

- [ ] **Step 1: Test hỏng:** dựng `ControlRequest` cho từng method và gọi `control::handle` (xem cách
  test hiện có trong crate gọi control, ví dụ `grep -rn "control::handle" crates/ui-egui/tests`). Assert
  hình dạng JSON như trên. `ui.ayp.open {"tab":"captions"}` thì sau đó `ui.ayp.state.tab == "captions"`.
  `{"tab":"nope"}` thì trả lỗi.
- [ ] **Step 2:** Chạy `cargo test -p filmcraft-ui-egui --test ayp_control_ui`. Expected: FAIL.
- [ ] **Step 3:** Viết 3 method.
- [ ] **Step 4:** Chạy test, rồi cả crate. Expected: PASS.
- [ ] **Step 5:** Commit `feat(aypeditor): control methods for the AYP interface state`.

### Task 10: Chạy thật 41 chức năng trên clip thật + đóng gói lại

**Files:**
- Create: `tools/ayp-editor/check_features.py`
- Create: `docs/aypeditor-check.md` (kết quả, sinh bởi script)

**Interfaces:**
- Consumes: AYPEditor.app (Task 1), `ui.ayp.tabs` (Task 9), `engine.execute`.

- [ ] **Step 1:** Build lại và cài: `scripts/build-aypeditor.sh --install`, rồi mở app.
- [ ] **Step 2:** Viết `check_features.py` (Python 3, chỉ stdlib), nói chuyện với cổng 9879 bằng JSON-lines
  `{"id","method","params"}`:
  1. Tạo project tạm trong `$TMPDIR`. Import `/Volumes/Extreme SSD/0716.mp4` (nếu ổ không có thì dùng
     `/Users/haule/Documents/render/0721.mp4` và đánh dấu các mục cần lời nói là SKIP). Tạo sequence 9:16
     từ clip đó (`file.newSequence` với `fromItem`).
  2. Với **mỗi item** của `ui.ayp.tabs`: chuẩn bị điều kiện (chọn clip, đặt playhead ở giữa, đã có transcript
     với lệnh transcript.*), rồi `engine.execute`. Lệnh chạy lâu (`transcript.generate`, `export.quick`,
     `tts.*`) thì chạy job và poll `jobs.list` (xem bẫy trong spec mục 7). Lệnh mở hộp thoại ("…") thì dùng
     params đủ để khỏi cần hộp thoại; nếu không được thì ghi MANUAL.
  3. Kiểm kết quả từng mục: project đổi (`history`), hoặc có file ra (xuất: đặt bitrate ~8 Mbps, rồi đo bằng
     `ffprobe` thời lượng > 0). Ghi PASS / FAIL (kèm lỗi) / SKIP / MANUAL.
  4. Ghi bảng Markdown vào `docs/aypeditor-check.md`.
- [ ] **Step 3:** Chạy script. Mục nào FAIL vì lỗi ở code AYPEditor thì sửa và chạy lại. Mục nào FAIL vì lệnh
  gốc của FilmCraft thì ghi rõ trong bảng, **không giấu**.
- [ ] **Step 4:** Chụp `ui.screenshot` của workspace AYP, lưu vào `docs/images/aypeditor-ayp-workspace.png`. Mở
  ảnh xem: chữ tiếng Việt có dấu hiện đúng, bố cục đúng spec mục 4.
- [ ] **Step 5:** Commit `test(aypeditor): real-clip check of the 41 built-in features`.
