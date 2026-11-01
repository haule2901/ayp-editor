#!/usr/bin/env python3
"""Trang duyệt clip local kiểu frame.io: xem clip render, ghim góp ý theo đoạn, lưu <clip>.feedback.json cạnh clip.

  python3 review.py [--root DIR] [--port 8765]        chạy trang tại http://127.0.0.1:8765
  python3 review.py dump <clip.mp4> [--all]           in góp ý (mặc định chỉ góp ý đang mở) ra JSON cho Claude
  python3 review.py done <clip.mp4> <id> ["ghi chú"]  đánh dấu đã sửa + ghi chú Claude hiện trên trang
"""
import argparse, json, mimetypes, os, re, sys, threading, uuid, webbrowser
from datetime import datetime, timezone
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlparse, parse_qs

PAGE = Path(__file__).resolve().parent / "duyet-clip.html"
FPS = 25
LOCK = threading.Lock()
ROOT = Path("/Volumes/Extreme SSD/Render")


def fb_path(clip: Path) -> Path:
    return clip.with_name(clip.name + ".feedback.json")


def load(clip: Path) -> dict:
    p = fb_path(clip)
    return json.loads(p.read_text("utf-8")) if p.exists() else {"clip": str(clip), "fps": FPS, "items": []}


def save(clip: Path, data: dict) -> None:
    p = fb_path(clip)
    tmp = p.with_name(p.name + ".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2), "utf-8")
    os.replace(tmp, p)  # ghi nguyên tử, không để file nửa vời


def tc(frame: int) -> str:
    s = frame // FPS
    return f"{s // 60:02d}:{s % 60:02d}:{frame % FPS:02d}"


def clean(item: dict) -> dict:
    """Chuẩn hoá góp ý từ trang: chỉ giữ trường đã biết, tính lại timecode từ số khung."""
    f = int(item["frame"])
    fe = item.get("frameEnd")
    fe = int(fe) if fe is not None and int(fe) > f else None
    return {
        "frame": f, "t": round(f / FPS, 3), "tc": tc(f),
        "frameEnd": fe, "tEnd": round(fe / FPS, 3) if fe is not None else None, "tcEnd": tc(fe) if fe is not None else None,
        "x": item.get("x"), "y": item.get("y"),
        "draw": item.get("draw") or [],
        "text": str(item.get("text", "")).strip()[:4000],
    }


def clips(root: Path) -> list:
    out = []
    for p in root.rglob("*.mp4"):
        if p.name.startswith("._") or "review-proxy" in p.parts:
            continue
        items = load(p)["items"]
        out.append({
            "path": str(p), "name": p.stem, "folder": str(p.parent.relative_to(root)),
            "mtime": p.stat().st_mtime, "open": sum(i["status"] != "done" for i in items), "total": len(items),
        })
    return sorted(out, key=lambda c: -c["mtime"])


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def clip(self, q) -> Path:
        p = Path(q["path"][0]).resolve()
        if ROOT.resolve() not in p.parents or p.suffix.lower() not in (".mp4", ".mov") or not p.exists():
            raise PermissionError(str(p))
        return p

    def send_json(self, obj, code=200):
        body = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def body(self) -> dict:
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n) or b"{}")

    def run(self, fn):
        try:
            fn(parse_qs(urlparse(self.path).query))
        except PermissionError:
            self.send_json({"error": "Đường dẫn nằm ngoài thư mục render"}, 403)
        except (KeyError, ValueError, StopIteration) as e:
            self.send_json({"error": f"Thiếu hoặc sai dữ liệu: {e!r}"}, 400)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        route = urlparse(self.path).path
        if route == "/":
            data = PAGE.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        elif route == "/api/clips":
            self.run(lambda q: self.send_json({"root": str(ROOT), "clips": clips(ROOT)}))
        elif route == "/api/feedback":
            self.run(lambda q: self.send_json(load(self.clip(q))))
        elif route == "/video":
            self.run(self.video)
        else:
            self.send_error(404)

    def video(self, q):  # phát file gốc, hỗ trợ Range để tua
        p = self.clip(q)
        size = p.stat().st_size
        start, end = 0, size - 1
        m = re.match(r"bytes=(\d*)-(\d*)", self.headers.get("Range", ""))
        if m and m[1]:
            start, end = int(m[1]), int(m[2]) if m[2] else end
        elif m and m[2]:
            start = size - int(m[2])
        end = min(end, size - 1)
        self.send_response(206 if m else 200)
        self.send_header("Content-Type", mimetypes.guess_type(p.name)[0] or "video/mp4")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(end - start + 1))
        if m:
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.end_headers()
        with p.open("rb") as f:
            f.seek(start)
            left = end - start + 1
            while left > 0 and (chunk := f.read(min(1 << 20, left))):
                self.wfile.write(chunk)
                left -= len(chunk)

    def do_POST(self):  # thêm góp ý
        def go(q):
            p, item = self.clip(q), clean(self.body())
            if not item["text"] and not item["draw"]:
                raise ValueError("góp ý trống")
            item.update(id=uuid.uuid4().hex[:8], status="open", reply="", created=datetime.now(timezone.utc).isoformat(timespec="seconds"))
            with LOCK:
                data = load(p)
                data["items"].append(item)
                save(p, data)
            self.send_json(item, 201)
        self.run(go)

    def do_PATCH(self):  # đổi trạng thái / nội dung
        def go(q):
            p, iid, patch = self.clip(q), q["id"][0], self.body()
            with LOCK:
                data = load(p)
                item = next(i for i in data["items"] if i["id"] == iid)
                if patch.get("status") in ("open", "done"):
                    item["status"] = patch["status"]
                if "text" in patch:
                    item["text"] = str(patch["text"]).strip()[:4000]
                save(p, data)
            self.send_json(item)
        self.run(go)

    def do_DELETE(self):
        def go(q):
            p, iid = self.clip(q), q["id"][0]
            with LOCK:
                data = load(p)
                data["items"] = [i for i in data["items"] if i["id"] != iid]
                save(p, data)
            self.send_json({"ok": True})
        self.run(go)


def main():
    global ROOT
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", default=str(ROOT))
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--no-open", action="store_true")
    sub = ap.add_subparsers(dest="cmd")
    d = sub.add_parser("dump"); d.add_argument("clip"); d.add_argument("--all", action="store_true")
    n = sub.add_parser("done"); n.add_argument("clip"); n.add_argument("id"); n.add_argument("note", nargs="?", default="")
    a = ap.parse_args()

    if a.cmd == "dump":
        clip = Path(a.clip).resolve()
        items = sorted(load(clip)["items"], key=lambda i: i["frame"])
        if not a.all:
            items = [i for i in items if i["status"] != "done"]
        print(json.dumps({"clip": str(clip), "fps": FPS, "items": items}, ensure_ascii=False, indent=2))
    elif a.cmd == "done":
        clip = Path(a.clip).resolve()
        data = load(clip)
        item = next((i for i in data["items"] if i["id"] == a.id), None)
        if not item:
            sys.exit(f"Không có góp ý id={a.id}")
        item["status"], item["reply"] = "done", a.note
        save(clip, data)
        print(f"Đã đánh dấu {a.id} [{item['tc']}] là đã sửa")
    else:
        ROOT = Path(a.root)
        srv = ThreadingHTTPServer(("127.0.0.1", a.port), H)
        url = f"http://127.0.0.1:{a.port}/"
        print(f"Trang duyệt clip: {url}  (thư mục: {ROOT})", flush=True)
        if not a.no_open:
            threading.Timer(0.5, webbrowser.open, [url]).start()
        srv.serve_forever()


if __name__ == "__main__":
    main()
