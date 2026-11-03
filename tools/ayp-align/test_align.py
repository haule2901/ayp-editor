"""python3 tools/ayp-align/test_align.py — kiểm phần không cần model (tách ký tự, chia cửa sổ, đọc đầu vào)."""
import json, sys, tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import align as A

vocab = {c: i for i, c in enumerate("abcdhilnorồđ|")}
vocab["<pad>"] = 99
# chữ thường NFC, dấu câu bỏ; ký tự không có trong vocab thì tách dấu ("ó" → "o" + dấu sắc không có → "o")
assert A.tokens("Rồi,", vocab) == [vocab["r"], vocab["ồ"], vocab["i"]]
assert A.tokens("đó!", vocab) == [vocab["đ"], vocab["o"]]

# cửa sổ ≤ 20 s, cắt ở khoảng lặng dài nhất, không mất chữ nào
words = [{"text": "a", "start": t, "end": t + 0.3} for t in (0, 1, 2, 9, 10, 18, 19, 25)]
wins = A.group(words)
assert all(b - a <= A.MAX_WINDOW for a, b, _ in wins), wins
assert [i for _, _, idx in wins for i in idx] == list(range(len(words)))

# whisper-cli: mỗi câu là một cửa sổ, chữ chia đều trong câu
with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
    json.dump({"transcription": [{"offsets": {"from": 1000, "to": 3000}, "text": " rồi ok"}]}, f)
w, wins, fmt = A.load_words(f.name)
assert fmt == "whisper-cli" and [x["text"] for x in w] == ["rồi", "ok"] and wins == [(1.0, 3.0, [0, 1])]
assert (w[0]["start"], w[1]["end"]) == (1.0, 3.0)
print("ok")
