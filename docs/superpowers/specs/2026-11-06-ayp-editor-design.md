# AYP Editor — giao diện kiểu CapCut + khung chat AI cho FilmCraft AYP

> Tài liệu thiết kế · 2026-11-06 · trạng thái: chờ duyệt
> Phiên brainstorm: các bản phác ở `App Creator/AYP-Editor/.superpowers/brainstorm/`

## 1. Vì sao làm

FilmCraft AYP đã làm được gần hết những gì cần cho việc dựng clip của AYP: 709 lệnh, 304 hiệu ứng
và chuyển cảnh, phụ đề tiếng Việt, 25 giọng VieNeu, Duyệt góp ý, làm mịn da. Lõi không có vấn đề.
**Vấn đề nằm ở giao diện**: giao diện bắt chước Premiere (nhiều panel, lệnh nằm sâu trong menu,
bố cục không tự lưu) nên người dùng *"không biết ở đâu để tìm"* chức năng.

AYP Editor **là FilmCraft AYP với giao diện thân thiện như CapCut, cộng thêm 24 chức năng đã chọn**.
Không có gì khác. App phục vụ hai đối tượng ngang nhau ngay từ đầu:

- **Người**: bấm, kéo thả, chỉnh thông số trên một màn hình kiểu CapCut.
- **AI (Claude)**: điều khiển qua MCP, hoặc qua khung chat nằm ngay trong app.

Người và AI thao tác trên **cùng một timeline và cùng một bộ lệnh**. AI vừa làm gì thì người thấy
ngay và sửa tay được. Người vừa bấm gì thì AI đọc được để làm tiếp.

## 2. Mục tiêu, ngoài phạm vi, điều cấm

**Mục tiêu**
- Mọi chức năng đều tìm được trong vài giây: theo thẻ có biểu tượng, bằng ô ⌘K, hoặc hỏi AI.
- Thừa hưởng **toàn bộ** FilmCraft AYP. Không lệnh nào, file project nào, script nào hay MCP nào bị hỏng.
- Có đủ 41 chức năng ✅ và 24 chức năng đã chọn ở mục 5.

**Ngoài phạm vi bản đầu** (user không chọn): kho footage online trong app, phụ đề song ngữ,
xoá câu nói hỏng hoặc nói lại, cắt video dài thành clip ngắn, đề xuất nhiều phương án dựng, nhép môi.
Cũng không làm: nhiều người dùng, đồng bộ đám mây, App Store, Windows.

**Điều cấm**: nếu một trong các điều sau xảy ra thì coi như thất bại, dù app vẫn chạy.
- Một lệnh hoặc tính năng đang chạy được trong FilmCraft AYP bị hỏng hoặc bị giấu mất.
- Script hoặc tool hiện có (`ayp` MCP, `tools/ayp-*`, glass_lib, các `build.py`, skill `ayp-phu-de`)
  phải sửa mới chạy được.
- AI làm thay đổi project mà người không thấy và không hoàn tác được.
- Chép hình ảnh, biểu tượng, mã hoặc tài nguyên của CapCut. Chỉ học bố cục và cách sắp xếp.
  Repo fork là public.
- Một chức năng ✅ không chạy đúng mà vẫn bị báo là xong.

## 3. Ràng buộc

- Làm trong repo riêng `App Creator/AYP-Editor` (tách từ fork FilmCraft AYP, nhánh `main`, public tại
  github.com/haule2901/ayp-editor; xem mục 9.1). Rust, egui 0.36 + wgpu. Giữ đúng quy tắc tầng trong
  `docs/architecture.md`: mọi thay đổi project đi qua `Session::execute(id, params)`, và
  "everything is a command".
- Máy: M4 16 GB, macOS 27 beta, **ổ chỉ còn khoảng 7–8 GB trống**. Build theo `AYP.md`, dùng
  `CARGO_INCREMENTAL=0`, dọn `target` khi đầy.
- Giao diện tiếng Việt. Hiện i18n chưa có `vi`, cần thêm `crates/ui-egui/src/i18n/vi.tsv`. Tên gọi
  theo cách gọi quen thuộc của CapCut tiếng Việt (Phương tiện, Âm thanh, Văn bản, Hiệu ứng, Chuyển tiếp,
  Phụ đề, Bộ lọc, Điều chỉnh, Tốc độ, Hiệu ứng động, Mặt nạ, Tách, Dừng hình, Hút vào đoạn chính…).
  Chỉ lấy tên gọi, không chép file.
- Mặc định chữ theo sổ style AYP: Be Vietnam Pro, giãn chữ −60, màu nhấn theo
  `docs/ayp-styles.md` (cam #F26722 cho tài chính, xanh #1A5FD6 / đỏ #E62117 cho nội dung sự nghiệp).
- Cổng hiện có giữ nguyên: control `9879` (JSON-lines), MCP `ayp` 9878 qua AYP Edit Hub,
  EffectCraft 9880, VieNeu 8891, trang Duyệt góp ý 8765.

## 4. Giao diện (đã chốt: bản phác C)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ☰  Tên project · 9:16     [⌘K Tìm lệnh…]                  [Xuất]         │
├───────────────┬──────────────────────────────┬───────────────────────────┤
│ Thẻ biểu tượng│                              │ [AI] [Thông số]           │
│ 🎞 ♪ T ▭ ✦ ⇄  │          Video               │  khung chat AI            │
│ 🎙 ✧ ✂ …      │       (player giữa)          │  hoặc thông số của        │
│ thư viện của  │                              │  clip đang chọn           │
│ thẻ đang mở   │                              │                           │
├───────────────┴──────────────────────────────┴───────────────────────────┤
│ ✂ Tách  ⌫ Xoá  ⟲  ◆  🧲 Hút  ⧉ Liên kết            zoom ───●───          │
│ [chữ / phụ đề]                                                            │
│ [track chính ████ ███ █████]  ← nam châm                                  │
│ [âm thanh ██████████████]                                                 │
└──────────────────────────────────────────────────────────────────────────┘
```

- **Bên trái**: hàng thẻ có biểu tượng và tên tiếng Việt. Bấm thẻ nào thì thư viện của thẻ đó mở ra
  để kéo vào timeline. Gồm: Phương tiện, Âm thanh, Văn bản, Phụ đề, Hiệu ứng, Chuyển tiếp,
  Bộ lọc/Màu, Giọng đọc, Motion AYP, Cắt bằng chữ.
- **Ở giữa**: player.
- **Bên phải**: hai tab, **AI** và **Thông số**. Tab Thông số chỉ hiện thông số của loại clip đang chọn
  (video / chữ / âm thanh / hiệu ứng / nhiều clip). Bên trong chia nhóm con như CapCut: Cơ bản,
  Hiệu ứng động, Tốc độ, Mặt nạ, Nền, Màu…
- **Ở dưới**: timeline đơn giản, có một track chính kèm nam châm, track phủ ở trên, âm thanh ở dưới,
  và thanh công cụ nhỏ.
- **⌘K**: ô tìm lệnh, tìm theo tên tiếng Việt và từ đồng nghĩa trong **toàn bộ** danh sách lệnh
  (`command.list`), kể cả lệnh không có nút. Đây là lối đi để "thừa hưởng tất cả": lệnh nào chưa có
  nút thì vẫn gọi được ở đây.
- **Giao diện cũ** của FilmCraft (các workspace Editing, Color…) vẫn mở được qua một mục menu
  *Chế độ nâng cao*. *(Quyết định của Claude: phần này đã có sẵn nên không tốn công.)*
- Bố cục mặc định là giao diện mới, và app mở lại đúng bố cục lần trước.

## 5. Chức năng bản đầu

Chú thích: ✅ lõi đã làm được, chỉ cần đặt nút · 🟡 có một phần · ❌ phải làm mới. Lệnh ghi theo id trong
`command.list`. Toàn bộ ✅ là **bắt buộc**, và mỗi chức năng phải chạy thử được trên clip thật.

### 5.1 Có sẵn (✅, 41 mục)
- **Phương tiện**: nhập file/thư mục (`file.import`), ghi âm lời thoại (`audio.voiceover.*`).
- **Âm thanh**: tách âm thanh (`clip.extractAudio`), giảm ồn và làm rõ giọng (DeNoise, Enhance Speech),
  chuẩn hoá âm lượng (`essentialSound.autoMatch`), nhạc nền tự nhỏ lại khi có lời
  (`essentialSound.generateDucking`), rõ dần/mờ dần (keyframe `volume`).
- **Văn bản**: thêm chữ, font/cỡ/màu/giãn chữ/viền/bóng (`graphics.newText`, `graphics.set`).
- **Phụ đề**: phụ đề tiếng Việt (`transcript.createCaptionsVi`), chia cụm theo nghĩa
  (`transcript.captionsFromChunks`), kiểu phụ đề lưu sẵn (`captions.applyPreset`).
- **Cắt bằng chữ**: xoá chữ trong bản chép lời để cắt video (`transcript.select/lift/extract`),
  xoá khoảng lặng (`transcript.removePauses`), xoá từ đệm (`transcript.removeFillers`).
- **Giọng đọc**: 25 giọng VieNeu (`tts.*`).
- **Hiệu ứng / Màu**: khoảng 250 hiệu ứng hình (`effects.apply`), khoảng 150 chuyển cảnh
  (`sequence.applyVideoTransition`), bộ lọc/LUT (`lumetri.applyPreset`, `lut.*`), chỉnh màu Lumetri,
  khớp màu (`lumetri.applyMatch`), làm mịn da (`skin_smooth`), phông xanh (Ultra Key).
- **Motion AYP**: làm mờ cả video khi có minh hoạ (Gaussian Blur + keyframe).
- **Thông số clip**: vị trí/tỷ lệ/xoay/độ mờ/hoà trộn, keyframe ◆, cắt khung, lật,
  tốc độ/đảo ngược (`clip.speedDuration`), dừng hình (`clip.frameHold`), chống rung (Warp Stabilizer),
  Auto Reframe 9:16, mặt nạ bám theo vật (`masks.*`).
- **Timeline**: tách (`timeline.razor`), xoá kéo khít (`edit.rippleDelete`), tự hút (`sequence.snap`),
  nhân đôi/nhóm/nest, hoàn tác 200 bước, multicam, marker, góp ý thành marker (`review.import`).
- **Xuất**: xuất nhanh MP4 với preset bitrate vừa phải (`export.quick`), hàng đợi xuất,
  Duyệt góp ý (`review.open`).
- **AI nhìn thấy thao tác tay**: chung timeline, cùng `command.list`.

### 5.2 Đã chọn thêm (🟡/❌, 24 mục)
| Nhóm | Chức năng | Hiện có |
|---|---|---|
| Âm thanh | Đổi giọng (trầm, robot, chipmunk…) bằng các preset | 🟡 chỉ có Pitch Shifter |
| Âm thanh | Tự đánh dấu nhịp nhạc (beat) thành marker để cắt theo nhạc | ❌ |
| Âm thanh | Thư viện nhạc + SFX (pop, whoosh…) từ thư mục của user, có nghe thử, bấm là thêm | ❌ |
| Văn bản | Kiểu chữ AYP bấm là có: typo cam, khung tên, kính mờ (glass_lib) | 🟡 |
| Văn bản | Kho hoạt ảnh chữ vào / ra / lặp (mờ→nét, gõ chữ, nảy…) | 🟡 keyframe tay |
| Văn bản | Chữ bám theo người / vật | 🟡 có `masks.track` |
| Phụ đề | Karaoke tô sáng từng chữ đang đọc (dùng mốc chữ wav2vec2 nếu có) | 🟡 đang làm qua script |
| Phụ đề | Sửa hàng loạt, tìm và thay chữ trong phụ đề | 🟡 |
| AI dựng | Tìm đoạn hay / câu đắt, kết quả là marker hoặc vùng chọn | ❌ (Claude) |
| AI dựng | Cuối clip trong một bước: khớp nhạc nền, SFX chuyển cảnh, mờ dần | 🟡 có lệnh lẻ |
| Giọng | Giọng anh Trí (VieNeu LoRA) | 🟡 chỉ chạy GPU Colab |
| Màu | Xoá phông bằng AI (không cần phông xanh) | ❌ |
| Màu | Chỉnh ánh sáng bằng AI (relight) | ❌ |
| Motion | Motion typo kiểu Apple (EffectCraft) ngay trong app | 🟡 app riêng, cổng 9880 |
| Motion | Che mặt học viên bằng avatar bám mặt | 🟡 script Apple Vision |
| Clip | Đường cong tốc độ có sẵn (nhanh–chậm–nhanh, …) | 🟡 Time Remapping |
| Clip | Nền mờ / nền màu khi video không phủ khung | 🟡 làm tay |
| Clip | Kho hoạt ảnh vào / ra cho clip (zoom in, trượt…) | 🟡 |
| Timeline | Track chính có nam châm: xoá hay kéo là tự khít, không để khoảng trống | 🟡 |
| Timeline | Clip phụ (chữ, sticker, âm thanh) đi theo clip chính khi kéo | 🟡 `clip.link` |
| Chat AI | AI đưa **kế hoạch** trước, người duyệt rồi mới làm | ❌ |
| Chat AI | AI đưa vài **lựa chọn dạng thẻ** để bấm | ❌ |
| Chat AI | Nút **"chỉ chỗ"**: mở đúng thẻ / thông số / lệnh mà AI vừa dùng | ❌ |
| Chat AI | **Hoàn tác cả lượt** AI vừa làm bằng một nút | 🟡 history có, cần gom theo lượt |

Thứ tự làm *(quyết định của Claude)*: giao diện khung → các chức năng ✅ → chat AI → 🟡 → ❌.
**Ba phần nặng nhất làm sau cùng**: xoá phông AI, chỉnh sáng AI, giọng anh Trí.

## 6. AI và MCP

- **Một bộ lệnh cho tất cả.** Mỗi chức năng mới ở mục 5.2 đều phải là một **lệnh mới** trong engine
  (có id, nhãn, tham số, `enabled()` kèm lý do), như các lệnh hiện có. Nút bấm, ⌘K, chat AI và MCP
  đều gọi lệnh đó. Không làm chức năng nào "chỉ có trên UI".
  **Ngoại lệ:** việc cần *phán đoán*, như "tìm đoạn hay", là **hành vi của AI**. Claude (trong chat
  hoặc qua MCP) đọc transcript rồi ghi kết quả bằng lệnh có sẵn (marker, vùng chọn). Engine không tự
  gọi Claude, tránh việc Claude Code gọi qua MCP lại kích một lần gọi Claude thứ hai. Phần làm theo
  quy tắc, như "cuối clip trong một bước" với nhạc và SFX cho trước, vẫn là lệnh engine.
  *(Quyết định của Claude.)*
- **MCP bên ngoài** (Claude Code, Claude app): giữ nguyên đường hiện có (MCP `ayp` → control 9879).
  Lệnh mới tự xuất hiện trong `command.list`. Thêm lệnh đọc trạng thái giao diện mới (thẻ nào đang mở,
  clip nào đang chọn, tab phải đang ở đâu) để AI ngoài biết người đang làm gì.
- **Chat AI trong app** (tab AI bên phải): gọi Claude qua API Anthropic. Khoá lưu trong macOS Keychain,
  model mặc định `claude-sonnet-5-5`, đổi được trong Cài đặt. Đây là cách app AYP Swift đang làm
  (`ayp-cut/Sources/AYPAI/LLMClient.swift`). *(Quyết định của Claude.)* Chat dùng chính các lệnh trong
  `command.list` làm tool, kèm luật biên tập AYP (`CLAUDE.md`, `docs/ayp-styles.md`, skill phụ đề)
  trong lời nhắn hệ thống.
- **Các loại thẻ trong chat** (học từ AI agent có sẵn trong CapCut 9.5, chỉ học ý tưởng):
  thẻ văn bản, **Kế hoạch** (danh sách bước kèm nút Làm / Sửa / Bỏ), **Lựa chọn** (2–4 thẻ để bấm),
  **Đang làm** (từng bước, có thể dừng), **Kết quả** (tóm tắt việc đã đổi, nút **Chỉ chỗ**, nút
  **Hoàn tác cả lượt**).
- **Mặc định an toàn**: việc nào đổi nhiều thứ (cắt hàng loạt, xoá, thay phụ đề) thì AI phải đưa thẻ
  Kế hoạch trước. Mỗi lượt AI làm là **một mục trong lịch sử hoàn tác**.
- **Lệnh chạy lâu** (chép lời, xuất video, giọng đọc) chạy thành job và báo tiến độ. Không chặn kênh
  điều khiển (đã đo: lệnh chạy lâu chặn kênh tới khi xong, và lời gọi bị cắt ở 60 giây).

## 7. Đã biết từ nghiên cứu (để không đo lại)

- Control 9879: `engine.commands` (709 lệnh, 401 có trong menu), `engine.execute {"id","params"}`,
  `ui.menu.list`, `ui.inspect`, `ui.screenshot`, `ui.click`, `ui.panel.show`. `effects.list` trả
  304 hiệu ứng/chuyển cảnh, chia theo folder.
- Bẫy FilmCraft đã gặp: `effects.reset` motion đặt anchor về tâm sequence; `file.exportFrame` không vẽ
  chuyển cảnh và keyframe opacity; xoá hết keyframe opacity thì giá trị tĩnh = key cuối;
  `markers.rippleSequenceMarkers` là công tắc; xuất mặc định rất nặng nên phải đặt bitrate;
  `replaceFromBin` lỗi khi trùng tên; bố cục panel không tự lưu.
- CapCut 9.5 (tham khảo bố cục): Thư viện trái (MaterialPanel), Thông số phải theo loại clip
  (FunctionPanel / DraftInspector*), Timeline có roughcut và text_rough (cắt bằng chữ), Agent chat
  với các thẻ Plan / Choice / Thinking / RoughCut / RemoveFillerWord, 6 skill agent.

## 8. Để người làm tự quyết

- Viết giao diện mới thành crate hoặc module mới (ví dụ `ui-egui/src/simple/`) hay tái dùng panel cũ.
  Ưu tiên tái dùng widget, timeline và player đang có.
- Cách lưu bố cục, cách đặt biểu tượng (vẽ mới, không lấy của CapCut).
- Cách làm beat detection, xoá phông (gợi ý: Apple Vision person segmentation chạy trên máy, qua
  helper Swift giống script che mặt), relight, và cách gọi giọng anh Trí trên Colab. Nếu Colab không
  chạy thì phải báo rõ, không treo app.
- Từ đồng nghĩa tiếng Việt cho ⌘K.

## 9. Câu hỏi của người đọc thử và câu trả lời

Các câu dưới đây là *quyết định của Claude*, trừ chỗ ghi là user đã nói.

1. **Một app hay hai app?** *User chốt 2026-11-06:* app riêng tên **AYPEditor**, cài ở
   `~/Applications/AYPEditor.app` (bundle id `vn.ayp.editor`), nằm cạnh `FilmCraft AYP.app` và không đè
   lên app đó. Repo cũng tách riêng: **`App Creator/AYP-Editor`**, public tại
   github.com/haule2901/ayp-editor. Repo giữ nguyên lịch sử của fork, remote `ayp` = filmcraft-ayp,
   remote `upstream` = storytold/filmcraft. Để thừa hưởng mọi thứ, AYPEditor **dùng chung thư mục dữ
   liệu** với FilmCraft AYP (model Whisper, VieNeu, preset, `ayp-workdir.txt`) và mặc định mở **cổng
   control 9879**, nên MCP `ayp`, các `build.py` và `tools/ayp-*` chạy được với AYPEditor mà không phải
   sửa. Không chạy hai app cùng lúc; nếu cổng đang bận thì báo rõ. Workspace mặc định của AYPEditor là
   giao diện mới. *Chế độ nâng cao* chuyển sang các workspace cũ.
2. **EffectCraft và kính mờ.** Điều khiển app EffectCraft đang có (cổng 9880) rồi đặt bản render vào
   timeline, đúng như đường `motion_to_filmcraft` mà AYP Edit Hub đang dùng. App tự mở EffectCraft khi
   cần; nếu không mở được thì báo rõ. Không nhúng engine EffectCraft vào FilmCraft ở bản đầu.
   Các kiểu chữ AYP *không* cần kính mờ (typo cam, khung tên) thì dựng bằng graphics có sẵn của FilmCraft.
3. **"Một lượt AI" để hoàn tác.** Một lượt là mọi lệnh chat AI chạy giữa lúc người gửi tin và lúc AI trả
   lời xong. Engine có thêm lệnh tuỳ chọn `history.beginGroup` / `history.endGroup` (dựa trên
   `Session::edit_group`). Chat trong app luôn dùng hai lệnh này. Tool bên ngoài *có thể* dùng; không
   dùng thì vẫn chạy như cũ, nên không vi phạm điều cấm. **Trong lúc AI đang làm, timeline chỉ xem được**
   và có nút **Dừng**, nên không có thao tác tay nào lẫn vào lượt của AI.
4. **Track chính có nam châm.** Track chính là V1. Nam châm là một công tắc trong preferences (mặc định
   bật ở giao diện mới), không đổi định dạng file project. Project cũ có nhiều track thì timeline đơn
   giản vẫn hiện **đủ mọi track**, chỉ gọn hơn. Không giấu track nào.
5. **Lệnh chạy lâu.** Giữ nguyên các id hiện có và cách chúng trả kết quả đồng bộ, để `build.py` và
   Edit Hub không phải sửa. Giao diện mới và chat chạy các việc này qua job có sẵn (`jobs.*`,
   `long_job.rs`). Edit Hub (ngoài fork) không thuộc phạm vi. Giới hạn 60 giây là timeout phía client
   của Edit Hub, không phải của FilmCraft.
6. **AI có gọi Claude từ engine không?** Không, xem ngoại lệ ở mục 6.
7. **Ổ đĩa.** `target/` của fork đã build sẵn (6,8 GB, ngay trong repo), nên build tiếp chỉ là build
   tăng dần. Nếu ổ máy đầy, đặt `CARGO_TARGET_DIR` sang `/Volumes/Extreme SSD` (còn khoảng 111 GB).
8. **Gộp code với upstream.** Fork vẫn kéo code từ `origin` (storytold/filmcraft). Vì vậy giao diện mới
   nằm ở **module riêng** (ví dụ `crates/ui-egui/src/simple/`), lệnh mới nằm ở **file riêng** trong
   engine (giống `ayp_review.rs`), và sửa file dùng chung ít nhất có thể.

## 10. Thế nào là xong

- Mỗi chức năng ✅ và mỗi chức năng ở mục 5.2 đều được **chạy thật** trên clip (gợi ý:
  `/Volumes/Extreme SSD/0716.mp4` có lời tiếng Việt), bằng cả nút bấm lẫn lệnh qua control 9879,
  và kiểm kết quả (xuất đoạn ngắn rồi đo bằng ffmpeg khi cần). Chức năng nào chưa đạt thì ghi rõ.
- Mở được một project FilmCraft AYP cũ và xuất ra đúng như trước.
- Các tool hiện có vẫn chạy mà không phải sửa (`ayp` MCP, `tools/ayp-*`, skill `ayp-phu-de`).
- Một người lần đầu mở app tìm được "thêm phụ đề tiếng Việt", "xoá khoảng lặng", "đổi tốc độ" và
  "xuất video" mà không cần hỏi.
