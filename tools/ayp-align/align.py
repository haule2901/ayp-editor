#!/usr/bin/env python3
"""AYP: khớp lại mốc từng chữ của transcript bằng wav2vec2 tiếng Việt (CTC forced alignment).

Whisper đoán đúng chữ nhưng mốc thời gian từng chữ hay lệch; wav2vec2 nghe từng âm nên đặt
đầu/cuối chữ sát hơn, giúp cắt và chạy phụ đề chính xác. Chữ giữ nguyên, chỉ thời gian đổi.

  ~/.venvs/align/bin/python tools/ayp-align/align.py <audio|video> <transcript.json> -o aligned.json

transcript.json nhận 3 dạng:
  - JSON của whisper-cli (-oj): {"transcription": [{"offsets": {"from": ms, "to": ms}, "text": ...}]}
  - danh sách chữ theo giây: [{"text", "start", "end"}]
  - transcript FilmCraft: {"words": [{"text", "start", "end"}]} với start/end là tick
Ra mặc định transcript FilmCraft (tick) để nạp bằng `transcript.set {item, transcript}`;
`--seconds` ra danh sách chữ theo giây.

Cần: ffmpeg, venv ~/.venvs/align (scripts/install-ayp.sh), model wav2vec2-base-vietnamese-250h
(CC BY-NC 4.0 — chỉ dùng phi thương mại) ở <thư mục làm việc>/tools/models/wav2vec2-vi-250h.
"""
import argparse, json, subprocess, sys, unicodedata
from pathlib import Path

TPS = 254_016_000_000  # tick FilmCraft mỗi giây
SR = 16_000
PAD = 1.0  # giây đệm hai đầu mỗi cửa sổ, đủ cho mốc whisper lệch
MAX_WINDOW = 20.0  # cửa sổ dài nhất khi đầu vào là danh sách chữ
WORKDIR_FILE = Path.home() / "Library/Application Support/FilmCraft/ayp-workdir.txt"


def default_model():
    w = WORKDIR_FILE.read_text("utf-8").strip() if WORKDIR_FILE.exists() else ""
    return Path(w).expanduser() / "tools/models/wav2vec2-vi-250h" if w else None


def load_words(path):
    """→ (words [{text, start, end} giây], windows [(start, end, [chỉ số chữ])], fmt)."""
    d = json.loads(Path(path).read_text("utf-8"))
    if isinstance(d, dict) and "transcription" in d:  # whisper-cli: chia chữ đều trong câu, align sửa lại
        words, windows = [], []
        for seg in d["transcription"]:
            a, b = seg["offsets"]["from"] / 1000, seg["offsets"]["to"] / 1000
            toks = seg["text"].split()
            if not toks:
                continue
            step = (b - a) / len(toks)
            idx = list(range(len(words), len(words) + len(toks)))
            words += [{"text": t, "start": a + k * step, "end": a + (k + 1) * step} for k, t in enumerate(toks)]
            windows.append((a, b, idx))
        return words, windows, "whisper-cli"
    if isinstance(d, dict) and "words" in d:
        words = [dict(w, start=w["start"] / TPS, end=w["end"] / TPS) for w in d["words"]]
        return words, group(words), "filmcraft"
    words = [dict(w) for w in d]
    return words, group(words), "seconds"


def group(words):
    """Cửa sổ ≤ MAX_WINDOW giây, cắt ở khoảng lặng dài nhất."""
    out, cur = [], []
    for i, w in enumerate(words):
        if cur and w["end"] - words[cur[0]]["start"] > MAX_WINDOW:
            gaps = [words[cur[k + 1]]["start"] - words[cur[k]]["end"] for k in range(len(cur) - 1)]
            cut = gaps.index(max(gaps)) + 1 if gaps else len(cur)
            out.append(cur[:cut])
            cur = cur[cut:]
        cur.append(i)
    if cur:
        out.append(cur)
    return [(words[c[0]]["start"], words[c[-1]]["end"], c) for c in out]


def tokens(text, vocab):
    """Chữ → id ký tự của model (thường, NFC; ký tự không có thì tách dấu; dấu câu bỏ)."""
    ids = []
    for ch in unicodedata.normalize("NFC", text.lower()):
        parts = [ch] if ch in vocab else [c for c in unicodedata.normalize("NFD", ch) if c in vocab]
        ids += [vocab[c] for c in parts if c not in ("|", "<pad>", "<unk>", "<s>", "</s>")]
    return ids


def read_audio(path):
    import numpy as np
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"],
                         check=True, capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.float32)


def align(audio, words, windows, model_dir):
    import torch, torchaudio.functional as F
    from transformers import Wav2Vec2ForCTC
    vocab = json.loads((Path(model_dir) / "vocab.json").read_text("utf-8"))
    dev = "mps" if torch.backends.mps.is_available() else "cpu"
    model = Wav2Vec2ForCTC.from_pretrained(model_dir).to(dev).eval()
    done = failed = 0
    floor = 0.0  # cuối chữ đã khớp trước đó: cửa sổ sau không được lấn vào tiếng của câu trước
    for n_win, (a, b, idx) in enumerate(windows):
        nxt = windows[n_win + 1][1] if n_win + 1 < len(windows) else b + PAD
        s0 = int(max(floor, a - PAD) * SR)
        chunk = audio[s0:int(min(b + PAD, nxt) * SR)]
        toks = [tokens(words[i]["text"], vocab) for i in idx]
        keep = [k for k, t in enumerate(toks) if t]
        flat = [t for k in keep for t in toks[k]]
        if not flat or len(chunk) < SR // 10:
            failed += len(idx)
            continue
        x = torch.from_numpy((chunk - chunk.mean()) / (chunk.std() + 1e-7)).unsqueeze(0).to(dev)
        with torch.inference_mode():
            emis = torch.log_softmax(model(x).logits, dim=-1).float().cpu()
        try:
            ali, score = F.forced_align(emis, torch.tensor([flat], dtype=torch.int32), blank=vocab["<pad>"])
        except RuntimeError:  # cửa sổ ngắn hơn số ký tự: giữ mốc cũ
            failed += len(idx)
            continue
        spans = F.merge_tokens(ali[0], score[0].exp(), blank=vocab["<pad>"])
        sec = len(chunk) / SR / emis.shape[1]  # giây mỗi khung CTC (~0,02)
        t0 = s0 / SR
        n = 0
        for k in keep:
            sp = spans[n:n + len(toks[k])]
            n += len(toks[k])
            w = words[idx[k]]
            w["start"], w["end"] = round(t0 + sp[0].start * sec, 3), round(t0 + sp[-1].end * sec, 3)
            w["confidence"] = round(sum(s.score for s in sp) / len(sp), 3)
            floor = w["end"]
            done += 1
        failed += len(idx) - len(keep)
    return done, failed


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("audio")
    ap.add_argument("transcript")
    ap.add_argument("-o", "--out", required=True)
    ap.add_argument("--model", default=default_model())
    ap.add_argument("--seconds", action="store_true", help="ra danh sách chữ theo giây thay vì transcript FilmCraft")
    a = ap.parse_args()
    if not a.model or not (Path(a.model) / "vocab.json").exists():
        sys.exit(f"Không thấy model wav2vec2 ({a.model}) — chạy scripts/install-ayp.sh hoặc truyền --model")
    words, windows, fmt = load_words(a.transcript)
    done, failed = align(read_audio(a.audio), words, windows, a.model)
    if a.seconds:
        out = words
    else:
        out = {"language": "vi", "source": "wav2vec2-vi-250h",
               "words": [{"text": w["text"], "start": round(w["start"] * TPS), "end": round(w["end"] * TPS),
                          "confidence": w.get("confidence", 1.0)} for w in words]}
    Path(a.out).write_text(json.dumps(out, ensure_ascii=False, indent=1), "utf-8")
    print(f"Đã khớp {done} chữ, giữ mốc cũ {failed} chữ (đầu vào: {fmt}) → {a.out}")


if __name__ == "__main__":
    main()
