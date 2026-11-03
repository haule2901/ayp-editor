# FilmCraft AYP

Bản fork của [FilmCraft](https://github.com/storytold/filmcraft) (MIT OR Apache-2.0) đã chỉnh
cho cách dựng clip của AYP. Mọi thứ khác giống FilmCraft gốc, xem [README.md](README.md).

## Phần custom AYP

- **Phụ đề tiếng Việt**: *Sequence ▸ Transcript ▸ Phụ đề tiếng Việt (AYP)…* (`transcript.createCaptionsVi`).
  Dùng Whisper large-v3-turbo, ngắt câu theo nghĩa (6–10 chữ, không tách từ ghép, không để chữ trơ
  một mình trên dòng). Biên tập viên (người hoặc Claude qua MCP) có thể tự chia cụm rồi gửi
  `transcript.captionsFromChunks`. Luật đầy đủ nằm trong [CLAUDE.md](CLAUDE.md). Khi xuất video, phụ đề mặc định được ghi vào hình.
- **25 giọng tiếng Việt VieNeu-TTS**: panel *Window ▸ Text to Speech*, chọn ngôn ngữ `vi-VN`
  (panel có sẵn trong workspace Editing).
- **Duyệt góp ý**: *Sequence ▸ Duyệt góp ý (AYP)…* xuất sequence đang mở rồi mở trang duyệt
  `http://127.0.0.1:8765`, nơi người xem ghim góp ý theo đoạn. *Sequence ▸ Góp ý thành marker (AYP)*
  biến các góp ý đó thành marker. Mã trang nằm ở `tools/ayp-review/`.
- **Làm mịn da** (`skin_smooth`): *Video Effects ▸ Blur & Sharpen*.
- **Khớp mốc từng chữ bằng wav2vec2** (`tools/ayp-align`): whisper nhận chữ, wav2vec2 đặt lại mốc
  đầu/cuối từng chữ, giúp cắt và chạy phụ đề chính xác hơn.
- **Sổ style edit** ([docs/ayp-styles.md](docs/ayp-styles.md)): chọn style theo loại clip, luật phụ đề,
  cắt clip, bố cục dọc từ Zoom, motion/typo, đóng gói, quy trình clip học viên. Mã dựng motion kính mờ
  ở `tools/ayp-motion/`.

## Build trên macOS (Apple Silicon)

Cần khoảng **10 GB ổ trống** cho thư mục `target/`.

```bash
xcode-select --install          # công cụ dòng lệnh của Xcode (bỏ qua nếu đã cài)
brew install rustup uv
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # thêm dòng này vào ~/.zshrc
rustup default stable           # cần Rust ≥ 1.95

git clone https://github.com/haule2901/filmcraft-ayp.git
cd filmcraft-ayp
CARGO_INCREMENTAL=0 cargo build --release -p filmcraft --features whisper,vieneu-voices
```

Muốn chạy thử ngay thì dùng `./target/release/filmcraft`. Còn nếu muốn đóng gói thành app
`FilmCraft AYP.app` trong `~/Applications`:

```bash
APP="$HOME/Applications/FilmCraft AYP.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/filmcraft "$APP/Contents/MacOS/FilmCraft"
cp assets/app-icon/filmcraft.icns "$APP/Contents/Resources/FilmCraft.icns"
sed -e "s/@VERSION@/0.8.0/g" -e "s/@SHORT_VERSION@/0.8.0/g" -e "s/@BUILD_SHA@/$(git rev-parse --short HEAD)/g" \
  packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string vn.ayp.filmcraft "$APP/Contents/Info.plist"
plutil -replace CFBundleDisplayName -string "FilmCraft AYP" "$APP/Contents/Info.plist"
plutil -replace CFBundleName -string "FilmCraft AYP" "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
codesign --force --deep --sign - "$APP"
```

Đã cài FilmCraft gốc thì vẫn giữ được, hai app nằm cạnh nhau vì khác bundle id.

## Cài phần bổ sung: chạy một lệnh

```bash
brew install uv ffmpeg git
bash scripts/install-ayp.sh
```

Script hỏi từng bước, chạy lại được (phần đã cài thì bỏ qua):

1. **Thư mục làm việc**: script hỏi bạn đã có thư mục làm việc chưa, muốn để ở đâu (ổ trong hay ổ
   rời đều được), rồi ghi đường dẫn vào `~/Library/Application Support/FilmCraft/ayp-workdir.txt`.
   Trong đó có:
   - `Render/`: nơi *Duyệt góp ý* xuất clip và trang duyệt đọc góp ý;
   - `tools/models/`: chứa model wav2vec2;
   - `Editor Material/`: SFX, nhạc, tư liệu (bạn tự chép vào).
2. **VieNeu-TTS** (Apache-2.0, ~1 GB) cho 25 giọng tiếng Việt, cài vào
   `~/Library/Application Support/FilmCraft/vieneu/VieNeu-TTS`. Lần đầu đọc giọng, FilmCraft tự bật
   server `127.0.0.1:8891` và tải model ONNX (~70 s); các lần sau chỉ ~20 s.
3. **wav2vec2 tiếng Việt** (~1,2 GB): venv `~/.venvs/align` (torch, transformers) cùng model
   [`nguyenvulebinh/wav2vec2-base-vietnamese-250h`](https://huggingface.co/nguyenvulebinh/wav2vec2-base-vietnamese-250h),
   tải từ Hugging Face về `<thư mục làm việc>/tools/models/wav2vec2-vi-250h`.
   ⚠️ **Giấy phép CC BY-NC 4.0: model chỉ được dùng phi thương mại.** Script sẽ hỏi bạn đồng ý trước
   khi tải. Model không nằm trong repo.

Không nằm trong script:

- **Model Whisper large-v3-turbo**: app tự tải trong lần đầu tạo phụ đề (*Text ▸ Transcribe*, chọn
  *Whisper large-v3-turbo*).
- *(Tuỳ chọn)* **Luật phụ đề riêng** trong `~/.capcut-sub/` (`config.txt`: `min_words`, `max_words`,
  `max_line_chars`, cùng các danh sách từ). Không có thì app dùng luật mặc định.
- *(Tuỳ chọn)* **AYP Edit Hub**: app thanh menu riêng, không nằm trong repo. App này mở MCP cổng 9878
  để Claude điều khiển FilmCraft. Không có thì vẫn điều khiển được bằng `filmcraft --control <cổng>`
  hoặc `cargo run --release -p filmcraft-cli -- mcp`.
- **SFX và nhạc**: không có trong repo (gói của bên thứ ba, không được phân phối lại). Xin chủ repo
  gửi riêng rồi chép vào `<thư mục làm việc>/Editor Material/`.

## Khớp mốc chữ bằng wav2vec2

```bash
~/.venvs/align/bin/python tools/ayp-align/align.py clip.mp4 whisper.json -o aligned.json
```

- `whisper.json` có thể là JSON của `whisper-cli -oj`, danh sách chữ `[{text, start, end}]` theo giây,
  hoặc transcript FilmCraft.
- Kết quả mặc định là transcript FilmCraft: nạp lại bằng lệnh `transcript.set {item, transcript}` rồi
  tạo phụ đề như thường. Thêm `--seconds` nếu muốn danh sách chữ theo giây.
- Đo trên 60 s một buổi Zoom: 195/195 chữ khớp, mất ~8 s trên M-series (MPS).
- Kiểm phần không cần model: `python3 tools/ayp-align/test_align.py`.

## Cập nhật bản mới

```bash
git pull
CARGO_INCREMENTAL=0 cargo build --release -p filmcraft --features whisper,vieneu-voices
cp target/release/filmcraft "$HOME/Applications/FilmCraft AYP.app/Contents/MacOS/FilmCraft"
codesign --force --deep --sign - "$HOME/Applications/FilmCraft AYP.app"
```
