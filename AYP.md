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

## Cần cài thêm (không có trong repo)

1. **Model Whisper large-v3-turbo**: app tự tải từ Hugging Face trong lần đầu tạo phụ đề
   (*Text ▸ Transcribe*, chọn model *Whisper large-v3-turbo*).
2. **VieNeu-TTS** cho 25 giọng tiếng Việt. Cài một lần, dung lượng khoảng 1 GB:
   ```bash
   D="$HOME/Library/Application Support/FilmCraft/vieneu"
   mkdir -p "$D" && cd "$D"
   git clone --depth 1 https://github.com/pnnbao97/VieNeu-TTS
   cd VieNeu-TTS && uv sync
   ```
   Lần đầu đọc giọng, FilmCraft tự bật server `127.0.0.1:8891` và tải model ONNX, mất
   khoảng 70 giây. Các lần sau chỉ mất khoảng 20 giây.
3. **Thư mục xuất cho Duyệt góp ý**: đường dẫn đang viết cứng là `/Volumes/Extreme SSD/Render`
   (ổ SSD rời). Nếu không có ổ tên *Extreme SSD* thì tạo thư mục này bằng tay:
   ```bash
   sudo mkdir -p "/Volumes/Extreme SSD/Render" && sudo chown "$USER" "/Volumes/Extreme SSD" "/Volumes/Extreme SSD/Render"
   ```
   macOS có thể xoá thư mục này sau khi khởi động lại; khi đó tạo lại. Trang duyệt cần
   `/usr/bin/python3`, máy Mac đã có sẵn.
4. *(Tuỳ chọn)* **Luật phụ đề riêng** trong `~/.capcut-sub/` (`config.txt`: `min_words`, `max_words`,
   `max_line_chars`, cùng các danh sách từ). Không có thư mục này thì app dùng luật mặc định.
5. *(Tuỳ chọn)* **AYP Edit Hub**: app thanh menu riêng, không nằm trong repo này. App này mở MCP
   cổng 9878 để Claude điều khiển FilmCraft. Nếu không có, vẫn điều khiển trực tiếp được bằng
   `filmcraft --control <cổng>` hoặc `cargo run --release -p filmcraft-cli -- mcp`.

## Cập nhật bản mới

```bash
git pull
CARGO_INCREMENTAL=0 cargo build --release -p filmcraft --features whisper,vieneu-voices
cp target/release/filmcraft "$HOME/Applications/FilmCraft AYP.app/Contents/MacOS/FilmCraft"
codesign --force --deep --sign - "$HOME/Applications/FilmCraft AYP.app"
```
