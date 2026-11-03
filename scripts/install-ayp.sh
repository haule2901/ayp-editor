#!/usr/bin/env bash
# AYP: cài một lần những thứ FilmCraft AYP cần mà không nằm trong repo (chạy lại được, phần đã có thì bỏ qua):
#   1. thư mục làm việc (người dùng chọn) — clip duyệt góp ý ra <thư mục>/Render, model ở <thư mục>/tools/models
#   2. VieNeu-TTS — 25 giọng tiếng Việt (Apache-2.0), ~1 GB
#   3. wav2vec2 tiếng Việt + venv ~/.venvs/align — khớp mốc từng chữ cho transcript (tools/ayp-align), ~1,2 GB
# Dùng: bash scripts/install-ayp.sh
set -euo pipefail

DATA="${FILMCRAFT_DATA_DIR:-$HOME/Library/Application Support/FilmCraft}"
WORKDIR_FILE="$DATA/ayp-workdir.txt"
VIENEU="$DATA/vieneu/VieNeu-TTS"
ALIGN_VENV="$HOME/.venvs/align"
W2V_REPO="nguyenvulebinh/wav2vec2-base-vietnamese-250h"

no() { local a; read -r -p "$1 [Y/n] " a; [[ "$a" =~ ^[nNkK] ]]; }   # mặc định: có
yes() { local a; read -r -p "$1 [y/N] " a; [[ "$a" =~ ^[yYcC] ]]; }  # mặc định: không
need() { command -v "$1" >/dev/null || { echo "Thiếu $1 — cài bằng: brew install $2"; exit 1; }; }

need git git
need uv uv
need ffmpeg ffmpeg
mkdir -p "$DATA"

# ---- 1. thư mục làm việc: luôn hỏi, không tự chọn ổ nào ---------------------------------------
WORK=""
if [[ -s "$WORKDIR_FILE" ]]; then
  WORK="$(head -n1 "$WORKDIR_FILE")"
  no "Thư mục làm việc AYP đang là \"$WORK\". Giữ nguyên?" && WORK=""
fi
while [[ -z "$WORK" ]]; do
  echo "Thư mục làm việc AYP là nơi để clip xuất (Render/), model và tư liệu dựng."
  read -r -p "Đã có thư mục làm việc chưa? Nhập đường dẫn (có sẵn, hoặc muốn tạo mới): " WORK
  WORK="${WORK/#\~/$HOME}"
  if [[ -n "$WORK" && ! -d "$WORK" ]]; then
    if yes "Chưa có \"$WORK\". Tạo mới?"; then mkdir -p "$WORK"; else WORK=""; fi
  fi
done
WORK="$(cd "$WORK" && pwd)"
mkdir -p "$WORK/Render" "$WORK/tools/models"
printf '%s\n' "$WORK" >"$WORKDIR_FILE"
echo "✓ Thư mục làm việc: $WORK (ghi ở $WORKDIR_FILE)"

# ---- 2. VieNeu-TTS ------------------------------------------------------------------------------
if [[ -d "$VIENEU/.venv" && -f "$VIENEU/apps/openai_speech.py" ]]; then
  echo "✓ VieNeu-TTS đã cài ($VIENEU)"
elif ! no "Cài VieNeu-TTS (25 giọng tiếng Việt, Apache-2.0, ~1 GB)?"; then
  mkdir -p "$(dirname "$VIENEU")"
  [[ -d "$VIENEU/.git" ]] || git clone --depth 1 https://github.com/pnnbao97/VieNeu-TTS "$VIENEU"
  (cd "$VIENEU" && uv sync)
  echo "✓ VieNeu-TTS — lần đầu đọc giọng FilmCraft tự tải model ONNX (~70 s)"
fi

# ---- 3. wav2vec2 tiếng Việt ---------------------------------------------------------------------
MODEL="$WORK/tools/models/wav2vec2-vi-250h"
if [[ -x "$ALIGN_VENV/bin/python" && -f "$MODEL/pytorch_model.bin" ]]; then
  echo "✓ wav2vec2 đã cài (venv $ALIGN_VENV, model $MODEL)"
else
  echo "Model $W2V_REPO dùng giấy phép CC BY-NC 4.0: CHỈ được dùng phi thương mại."
  if yes "Đồng ý điều khoản đó và cài wav2vec2 (~1,2 GB)?"; then
    [[ -x "$ALIGN_VENV/bin/python" ]] || uv venv --python 3.12 "$ALIGN_VENV"
    uv pip install --python "$ALIGN_VENV/bin/python" torch==2.15.0 torchaudio==2.11.1 transformers==5.22.1 soundfile numpy
    "$ALIGN_VENV/bin/python" -c "import sys; from huggingface_hub import snapshot_download
snapshot_download(sys.argv[1], local_dir=sys.argv[2], allow_patterns=['*.json', '*.txt', 'pytorch_model.bin'])" "$W2V_REPO" "$MODEL"
    echo "✓ wav2vec2 → $MODEL"
  fi
fi

echo
echo "Xong. Phụ đề, giọng đọc, duyệt góp ý dùng ngay trong FilmCraft AYP. Khớp mốc chữ:"
echo "  $ALIGN_VENV/bin/python tools/ayp-align/align.py <audio> <whisper.json> -o aligned.json"
