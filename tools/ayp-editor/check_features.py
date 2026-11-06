#!/usr/bin/env python3
"""Real-clip check of AYPEditor's built-in features (plan 1, Task 10).

Drives a running AYPEditor over its control port, the way the user (clicks) and the AI
(engine.execute) do, and writes a Markdown table of what really happened.

    AYP_EDITOR=1 ~/Applications/AYPEditor.app/Contents/MacOS/FilmCraft --control 9899 &
    python3 tools/ayp-editor/check_features.py --port 9899 --media $TMPDIR/aype/0716-60s.mp4 \
        --media2 $TMPDIR/aype/0716.mp4 --work $TMPDIR/aype --out docs/aypeditor-check.md

Paths:
- "click": ui.click on the AYP button (`ayp.item.<key>` / `ayp.tool.<cmd>`), as a person does; the app runs the
  item's command and its `then` steps.
- "dialog": the button opens a dialog with {} — the underlying command runs with explicit params instead
  (a native file dialog would hang an unattended run).
- "panel": the button points at the embedded classic panel; the command runs with params a person picks there.
- "ai": the button is for the AI; the command runs with params the AI would send.
Results: PASS, PASS(dialog|panel|ai), FAIL (with the error), SKIP, MANUAL. Stdlib only.
"""
import argparse
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request

TICKS = 254_016_000_000  # filmcraft_time::Tick per second


class Fail(Exception):
    pass


class App:
    def __init__(self, port):
        self.port, self.n, self.sock, self.buf = port, 0, None, b""

    def _connect(self):
        self.sock = socket.create_connection(("127.0.0.1", self.port), timeout=130)
        self.buf = b""

    def call(self, method, params=None, timeout=130):
        """One JSON-lines request. A call the server cuts off (60 s) keeps the channel busy: wait it out."""
        for attempt in range(2):
            try:
                if self.sock is None:
                    self._connect()
                self.sock.settimeout(timeout)
                self.n += 1
                self.sock.sendall((json.dumps({"id": self.n, "method": method, "params": params or {}}) + "\n").encode())
                while b"\n" not in self.buf:
                    chunk = self.sock.recv(1 << 20)
                    if not chunk:
                        raise ConnectionError("closed")
                    self.buf += chunk
                line, self.buf = self.buf.split(b"\n", 1)
                return json.loads(line)
            except (OSError, ConnectionError, ValueError):
                self.sock = None
                if attempt:
                    raise
                time.sleep(1)

    def ex(self, cmd, params=None, timeout=130):
        r = self.call("engine.execute", {"id": cmd, "params": params or {}}, timeout)
        if "timed out" in str(r.get("error", "")) or "timeout" in str(r.get("error", "")).lower():
            self.wait_idle()
        return r

    def res(self, cmd, params=None):
        r = self.ex(cmd, params)
        if not r.get("ok"):
            raise Fail(f"{cmd}: {r.get('error')}")
        return r.get("result")

    def wait_idle(self, limit=900):
        t0 = time.time()
        while time.time() - t0 < limit:
            try:
                if self.call("ui.ayp.state", timeout=10).get("ok"):
                    return
            except OSError:
                pass
            time.sleep(2)

    # ---- state
    def seq(self):
        return self.res("sequence.inspect")

    def hist(self):
        return len(self.res("history.list")["undo"])

    def ui(self):
        return self.call("ui.inspect")["result"]["ui"]

    def status(self):
        return self.ui().get("status", "")

    def elements(self, prefix):
        r = self.call("ui.elements", {"prefix": prefix})
        return r.get("result") or []

    def jobs(self):
        return self.res("jobs.list")


def panels_of(dock, out):
    if isinstance(dock, dict):
        for k, v in dock.items():
            if k == "panels" and isinstance(v, list):
                out.update(map(str, v))
            else:
                panels_of(v, out)
    elif isinstance(dock, list):
        for v in dock:
            panels_of(v, out)
    return out


def clip_of(seq, clip):
    for kind in ("video", "audio"):
        for t in seq[kind]:
            for it in t["items"]:
                if it["clip"] == clip:
                    return it
    return None


def effects_of(seq, clip):
    it = clip_of(seq, clip)
    return [e["effect"] for e in it["effects"]] if it else []


def probe(path):
    if not path or not os.path.exists(path):
        return 0.0
    try:
        out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path], capture_output=True, text=True, timeout=60)
        return float(out.stdout.strip() or 0)
    except (OSError, ValueError, subprocess.SubprocessError):
        return 0.0


def free_gb():
    st = os.statvfs("/")
    return st.f_bavail * st.f_frsize / 1e9


class Check:
    def __init__(self, app, a):
        self.app, self.a, self.rows = app, a, []
        self.base = 0
        self.pollution = []

    # ---- bookkeeping
    def row(self, feature, where, path, result, note=""):
        self.rows.append((feature, where, path, result, note.replace("|", "\\|").replace("\n", " ")))
        print(f"{result:13} {feature} — {note}", flush=True)

    def setup(self):
        app, a = self.app, self.a
        app.call("ui.click", {"id": "recovery.later"})  # a leftover recovery prompt: Not Now (never Discard)
        app.res("file.newProject", {"name": "aype-check"})
        self.item = app.res("file.import", {"paths": [a.media]})["items"][0]
        self.seq_id = app.res("file.newSequence", {"name": "Kiểm tra 9:16", "width": 1080, "height": 1920, "fromItem": self.item})["sequence"]
        app.res("file.saveAs", {"path": os.path.join(a.work, "aype-check.fcproj")})
        q = app.seq()
        self.v = q["video"][0]["items"][0]["clip"]
        self.au = q["audio"][0]["items"][0]["clip"]
        r = app.ex("transcript.generate", {"items": [self.item], "model": a.model, "language": "vi"}, timeout=900)
        if not r.get("ok") and "timeout" in str(r.get("error")):
            # cut off at 60 s by the control server; the command kept running and has finished by now
            r = {"ok": True, "result": app.res("transcript.inspect")["items"]}
        if not r.get("ok"):
            raise Fail(f"transcript.generate: {r.get('error')}")
        self.words = app.res("transcript.inspect")["words"]
        if not self.words:
            raise Fail("transcript.generate produced no words")
        app.res("file.save")
        app.call("ui.ayp.open", {"tab": "media", "side": "props"})
        self.base = app.hist()
        self.dock = panels_of(app.ui()["dock"], set())
        return r.get("result")

    def reset(self):
        app = self.app
        u = app.ui()
        d = u.get("clip_dialog")
        if d:
            app.call("ui.click", {"id": {"clip.nest": "nest", "clip.speedDuration": "speedDuration"}.get(d.get("command"), "x") + ".cancel"})
        if str(u.get("mode")).lower() != "edit":
            app.ex("mode.edit")
        if app.call("ui.ayp.state")["result"].get("sequence") != self.seq_id:
            app.ex("sequence.open", {"item": self.seq_id})
        for _ in range(40):
            if app.hist() <= self.base:
                break
            app.ex("edit.undo")
        q = app.seq()
        if q.get("in") is not None or q.get("out") is not None:
            app.ex("markers.clearInOut")
        app.ex("timeline.select", {"clips": [self.v, self.au]})
        app.ex("playhead.set", {"seconds": 20})

    def watch_layout(self, what):
        now = panels_of(self.app.ui()["dock"], set())
        extra = now - self.dock
        if extra:
            self.pollution.append((what, sorted(extra)))
            self.app.call("ui.set", {"workspace": "AYP"})

    # ---- clicking the real buttons
    def click_item(self, tab, key):
        app = self.app
        app.call("ui.ayp.open", {"tab": tab})
        time.sleep(0.2)
        eid = f"ayp.item.{key}"
        items = app.elements("ayp.item.")
        if items:  # scroll the list back to the top, then down until the button shows
            r = items[0]["rect"]
            pt = {"x": r[0] + r[2] / 2, "y": r[1] + r[3] / 2}
            app.call("ui.scroll", dict(pt, dy=2000))
            for _ in range(15):
                if any(e["id"] == eid for e in app.elements(eid)):
                    break
                app.call("ui.scroll", dict(pt, dy=-90))
                time.sleep(0.1)
        if not any(e["id"] == eid for e in app.elements(eid)):
            raise Fail(f"button {eid} is not on screen")
        r = app.call("ui.click", {"id": eid})
        if not r.get("ok"):
            raise Fail(f"ui.click {eid}: {r.get('error')}")
        time.sleep(0.3)
        return app.status()

    def click_tool(self, cmd):
        r = self.app.call("ui.click", {"id": f"ayp.tool.{cmd}"})
        if not r.get("ok"):
            raise Fail(f"ui.click ayp.tool.{cmd}: {r.get('error')}")
        time.sleep(0.3)
        return self.app.status()

    def wait_jobs(self, since, limit=600):
        """Jobs started after `since` (ids); waits until none is running. Returns them."""
        t0 = time.time()
        while True:
            js = [j for j in self.app.jobs() if j.get("id") not in since]
            busy = [j for j in js if not j.get("finished")]
            if not busy or time.time() - t0 > limit:
                return js
            time.sleep(2)

    def job_ids(self):
        return {j.get("id") for j in self.app.jobs()}

    def run(self, feature, where, path, fn):
        self.reset()
        try:
            res, note = fn()
        except Fail as e:
            res, note = "FAIL", str(e)
        except Exception as e:  # noqa: BLE001 - one broken check must not stop the run
            res, note = "FAIL", f"check crashed: {type(e).__name__}: {e}"
        self.row(feature, where, path, res, note)
        self.watch_layout(path)

    # ---- expectations
    def grew(self, before, status, extra=""):
        n = self.app.hist() - before
        if n > 0:
            return "PASS", f"+{n} history step(s){'; ' + extra if extra else ''}"
        raise Fail(f"no history step; status: {status!r}")

    def click_edit(self, tab, key, extra=None):
        before = self.app.hist()
        st = self.click_item(tab, key)
        if self.app.hist() > before:  # the click clears the status line, so a reason here is this click's
            return self.grew(before, st, extra() if extra else "")
        raise Fail(st or "no history step and no reason shown")

    def chunks(self, n_words=30):
        """What the AI sends: 4–8 words a caption, two balanced lines of at most 21 characters."""
        w, out, i = self.words, [], 0
        while i < min(n_words, len(w)):
            best = None
            for n in range(8, 3, -1):
                ws = [x["text"] for x in w[i : i + n]]
                if len(ws) < n:
                    continue
                if len(" ".join(ws)) <= 21:
                    best = (n, " ".join(ws))
                    break
                for k in range(2, n - 1):
                    a, b = " ".join(ws[:k]), " ".join(ws[k:])
                    if len(a) <= 21 and len(b) <= 21:
                        best = (n, f"{a} / {b}")
                        break
                if best:
                    break
            if not best:
                break
            n, text = best
            out.append({"from": w[i]["i"], "to": w[i + n - 1]["i"], "text": text})
            i += n
        return out

    def export_check(self, cmd, params, seconds):
        if free_gb() < 2:
            raise Fail(f"only {free_gb():.1f} GB free on / — export skipped")
        app = self.app
        app.res("markers.markIn", {"seconds": 2})
        app.res("markers.markOut", {"seconds": 2 + seconds})
        since = self.job_ids()
        r = app.res(cmd, params)
        js = self.wait_jobs(since)
        done = [(j.get("result") or {}).get("path") for j in js]
        path = next((x for x in done if x), None) or (r or {}).get("path") or params.get("path")
        d = probe(path)
        bad = [j for j in js if (j.get("result") or {}).get("error")]
        if d <= 0:
            raise Fail(f"no playable output at {path}; jobs: {json.dumps(bad or js, ensure_ascii=False)[:300]}")
        size = os.path.getsize(path) / 1e6
        os.remove(path)
        return d, size

    # =========================================================== the features
    def all(self):
        a, app = self.a, self.app
        W = a.work

        # ---------------- Phương tiện
        def imp():
            r = app.res("file.import", {"paths": [a.media2]})
            if not r.get("items"):
                raise Fail(f"nothing imported: {r}")
            return "PASS(dialog)", f"imported item {r['items'][0]}; the button itself opens the system file dialog"
        self.run("Nhập tệp (file.import)", "Phương tiện", "engine.execute file.import {paths}", imp)
        self.row("Ghi âm lời thoại — bắt đầu (audio.voiceover.start)", "Phương tiện", "not run (needs the microphone)", "MANUAL",
                 "defaults with {}: track = record-armed audio track, else the first targeted one, else A1; starts at the playhead "
                 "(or In→Out punch when both are marked); preroll from Settings ▸ Voice-over. The take is placed with overwrite, so on "
                 "a clip whose own sound is on A1 it replaces that sound for the recorded span")
        self.row("Ghi âm lời thoại — dừng (audio.voiceover.stop)", "Phương tiện", "not run", "MANUAL", "needs a recording in progress")

        def export_media():
            st = self.click_item("media", "file.exportMedia")
            mode = str(app.ui().get("mode"))
            app.ex("mode.edit")
            if "export" not in mode.lower():
                raise Fail(f"the button did not open Export mode (mode {mode}); status {st!r}")
            d, mb = self.export_check("file.exportMedia", {"path": os.path.join(W, "x-media.mp4"), "format": "h264", "bitrateKbps": 8000, "range": "inOut"}, 3)
            return "PASS", f"button opens Export mode; file.exportMedia {{path, 8 Mbps, In/Out 3 s}} wrote {d:.2f} s ({mb:.1f} MB, ffprobe)"
        self.run("Xuất video (file.exportMedia)", "Phương tiện", "click ayp.item.file.exportMedia + engine.execute with path", export_media)

        def queue():
            n0 = len(app.res("export.queue.list")["items"])
            st = self.click_item("media", "export.queue.add")
            items = app.res("export.queue.list")["items"]
            if len(items) <= n0:
                raise Fail(f"queue did not grow; status {st!r}")
            where = items[-1].get("path") or items[-1].get("output")
            for it in items[n0:]:
                app.ex("export.queue.remove", {"id": it.get("id")})
            out = os.path.join(W, "x-queue.mp4")
            d, mb = self.export_check("export.queue.add", {"path": out, "format": "h264", "bitrateKbps": 8000, "range": "inOut", "start": True}, 2)
            return "PASS", f"button queued an item (default output {where}, removed again); queue with path + start wrote {d:.2f} s ({mb:.1f} MB)"
        self.run("Hàng đợi xuất (export.queue.add)", "Phương tiện", "click ayp.item.export.queue.add + queue start", queue)

        # ---------------- Âm thanh
        def extract():
            r = self.click_edit("audio", "clip.extractAudio")
            return r[0], r[1] + f"; status {app.status()!r}"
        self.run("Tách âm thanh (clip.extractAudio)", "Âm thanh", "click ayp.item.clip.extractAudio", extract)
        for eff, name in (("denoise", "Giảm ồn (DeNoise)"), ("speech_enhance", "Làm rõ giọng (Enhance Speech)")):
            def fx(eff=eff):
                r = self.click_edit("audio", f"effects.apply.{eff}")
                if eff not in effects_of(app.seq(), self.au):
                    raise Fail(f"{eff} is not on the audio clip; effects {effects_of(app.seq(), self.au)}")
                return r[0], r[1] + f"; {eff} on the A1 clip"
            self.run(name, "Âm thanh", f"click ayp.item.effects.apply.{eff}", fx)

        def automatch():
            return self.click_edit("audio", "essentialSound.autoMatch", lambda: json.dumps(app.res("essentialSound.inspect", {"clips": [self.au]}), ensure_ascii=False)[:160])
        self.run("Chuẩn hoá âm lượng (essentialSound.autoMatch)", "Âm thanh", "click ayp.item.essentialSound.autoMatch", automatch)

        def ducking():
            before = app.hist()
            st = self.click_item("audio", "essentialSound.generateDucking")
            if app.hist() > before:
                return "PASS", f"+{app.hist() - before} step(s) on a bare click"
            # the real use: narration (dialogue) over the clip's sound (music)
            app.res("essentialSound.setType", {"clips": [self.au], "type": "music"})
            tts = app.res("tts.create", {"text": "Đây là lời dẫn thử để nhạc nền tự nhỏ lại.", "voice": "vieneu-ngoc-huyen", "dir": W})
            app.res("essentialSound.setType", {"clips": [tts["clip"]], "type": "dialogue"})
            app.ex("timeline.select", {"clips": [self.au]})
            before = app.hist()
            st2 = self.click_item("audio", "essentialSound.generateDucking")
            if app.hist() > before:
                return "PASS", f"bare click: {st!r}; after tagging the clip Music and a TTS narration Dialogue the click ducked (+{app.hist() - before})"
            raise Fail(f"bare click: {st!r}; with Music + Dialogue: {st2!r}")
        self.run("Nhạc nền tự nhỏ khi có lời (essentialSound.generateDucking)", "Âm thanh", "click ayp.item.essentialSound.generateDucking", ducking)
        for key, name in (("audio.fadeIn", "Rõ dần"), ("audio.fadeOut", "Mờ dần")):
            def fade(key=key):
                r = self.click_edit("audio", key)
                tr = [t for t in app.seq()["audio"][0]["transitions"]]
                if not tr:
                    raise Fail("no audio transition on A1")
                return r[0], r[1] + f"; A1 transitions {len(tr)}"
            self.run(f"{name} (sequence.applyAudioTransition)", "Âm thanh", f"click ayp.item.{key}", fade)

        def vol_keys():
            t0, t1 = 0, 2 * TICKS
            app.res("effects.setParam", {"clip": self.au, "effect": "volume", "param": "level", "value": -40, "time": t0, "keyframe": True})
            app.res("effects.setParam", {"clip": self.au, "effect": "volume", "param": "level", "value": 0, "time": t1, "keyframe": True})
            it = clip_of(app.seq(), self.au)
            k = next(e for e in it["effects"] if e["effect"] == "volume")["params"]["level"]["keyframes"]
            if k < 2:
                raise Fail(f"volume keyframes {k}")
            return "PASS(panel)", f"{k} volume keyframes (−40 dB → 0 dB over 2 s), set in Thông số"
        self.run("Rõ dần bằng keyframe volume", "Thông số", "engine.execute effects.setParam volume keyframe", vol_keys)

        # ---------------- Văn bản
        def new_text():
            before = app.hist()
            st = self.click_item("text", "graphics.newText")
            if app.hist() - before < 2:
                raise Fail(f"expected newText + graphics.set; got {app.hist() - before}; status {st!r}")
            sel = app.call("ui.ayp.state")["result"]["selection"]
            dump = json.dumps(clip_of(app.seq(), sel[0]) if sel else {}, ensure_ascii=False)
            ok = "Be Vietnam Pro" in dump and "-60" in dump and "SemiBold" in dump
            if not ok:
                raise Fail(f"text clip {sel} lacks Be Vietnam Pro SemiBold / tracking −60")
            return "PASS", f"+{app.hist() - before} steps (newText + graphics.set); clip {sel[0]}: Be Vietnam Pro SemiBold, tracking −60"
        self.run("Thêm chữ (graphics.newText + kiểu AYP)", "Văn bản", "click ayp.item.graphics.newText", new_text)

        def set_text():
            c = app.res("graphics.newText", {"text": "Thử kiểu chữ"})["clip"]
            app.ex("timeline.select", {"clips": [c]})
            return self.click_edit("text", "graphics.set")
        self.run("Font/cỡ/màu/giãn chữ (graphics.set)", "Văn bản", "click ayp.item.graphics.set on a text clip", set_text)

        # ---------------- Phụ đề
        def caps_vi():
            r = self.click_edit("captions", "transcript.createCaptionsVi")
            n = sum(len(t["captions"]) for t in app.res("captions.list")["tracks"])
            if not n:
                raise Fail("no captions")
            return r[0], r[1] + f"; {n} captions"
        self.run("Phụ đề tiếng Việt (transcript.createCaptionsVi)", "Phụ đề", "click ayp.item.transcript.createCaptionsVi", caps_vi)

        def chunks():
            ch = self.chunks()
            r = app.ex("transcript.captionsFromChunks", {"chunks": ch})
            note = ""
            if not r.get("ok"):
                bad = {int(x.split()[0][1:]) for x in str(r.get("error")).split("#")[1:] if x.split() and x.split()[0][1:].isdigit()}
                note = f"first try refused {sorted(bad)} ({str(r.get('error'))[:160]}); "
                ch = [c for k, c in enumerate(ch) if k not in bad]
                r = app.ex("transcript.captionsFromChunks", {"chunks": ch})
            if not r.get("ok"):
                raise Fail(f"refused {len(ch)} chunks: {r.get('error')}"[:400])
            return "PASS(ai)", note + f"{len(ch)} chunks → {json.dumps(r['result'], ensure_ascii=False)[:200]}"
        self.run("Chia cụm phụ đề theo nghĩa (transcript.captionsFromChunks)", "Phụ đề", "engine.execute captionsFromChunks {chunks}", chunks)

        def preset():
            app.res("transcript.createCaptionsVi")
            st = self.click_item("captions", "captions.applyPreset")
            name = app.res("captions.presets")["presets"][0]["name"]
            before = app.hist()
            app.res("captions.applyPreset", {"name": name})
            self.grew(before, st)
            return "PASS(panel)", f"preset {name!r} applied to the AYP captions; the button shows {st!r}"
        self.run("Kiểu phụ đề lưu sẵn (captions.applyPreset)", "Phụ đề", "engine.execute captions.applyPreset {name}", preset)

        # ---------------- Cắt bằng chữ
        for key, name, params in (("transcript.removePauses", "Xoá khoảng lặng", {"minSeconds": 0.3}),
                                  ("transcript.removeFillers", "Xoá từ đệm", {"fillers": ["thì", "á"]})):
            def tx(key=key, params=params):
                d0 = app.seq()["durationFrames"]
                before = app.hist()
                st = self.click_item("cut", key)
                d1 = app.seq()["durationFrames"]
                if app.hist() > before:
                    return "PASS", f"+{app.hist() - before} step; sequence {d0} → {d1} frames"
                if st != "Không tìm thấy chỗ nào để xoá":
                    raise Fail(st or "nothing happened, no reason shown")
                # the button ran and found nothing on this clip; show the command does cut when there is something
                r = app.res(key, params)
                return "PASS", (f"the click ran and said {st!r} (no pause ≥ 1 s / no ờ-ừm filler in these 60 s); "
                                f"{key} {json.dumps(params, ensure_ascii=False)} → {json.dumps(r, ensure_ascii=False)}")
            self.run(f"{name} ({key})", "Cắt bằng chữ", f"click ayp.item.{key}", tx)
        for key, name in (("transcript.lift", "Xoá chữ chọn, để khoảng trống"), ("transcript.extract", "Xoá chữ chọn, kéo khít")):
            def pick(key=key):
                app.call("ui.ayp.open", {"tab": "cut"})
                time.sleep(0.2)
                app.call("ui.click", {"id": "text.tab.Transcript"})  # the embedded Text panel may be on Captions
                time.sleep(0.2)
                a5, a7 = "text.transcript.word.5", "text.transcript.word.7"
                if not app.elements(a5):
                    raise Fail("the embedded Text panel shows no transcript words")
                app.call("ui.click", {"id": a5})
                app.call("ui.click", {"id": a7, "modifiers": {"shift": True}})
                sel = app.ui().get("transcript_sel")
                d0 = app.seq()["durationFrames"]
                r = self.click_edit("cut", key)
                return r[0], r[1] + f"; picked words {sel} in the Text panel; sequence {d0} → {app.seq()['durationFrames']} frames"
            self.run(f"{name} ({key})", "Cắt bằng chữ", f"click words in Text panel + ayp.item.{key}", pick)

        def marker():
            n0 = len(app.seq()["markers"])
            r = self.click_edit("cut", "markers.add")
            return r[0], r[1] + f"; markers {n0} → {len(app.seq()['markers'])}"
        self.run("Thêm marker (markers.add)", "Cắt bằng chữ", "click ayp.item.markers.add", marker)

        def multicam(cam):
            def f():
                before = app.hist()
                st = self.click_item("cut", f"multicam.cutToCamera{cam}")
                if app.hist() > before:
                    return "PASS", "bare click worked"
                # the real use: a multicam clip made from two angles
                item2 = app.res("file.import", {"paths": [a.media2]})["items"]
                item2 = item2[0] if item2 else None
                if item2 is None:
                    raise Fail(f"bare click: {st!r}; could not import the second angle")
                mc = app.res("clip.createMulticam", {"items": [self.item, item2], "method": "in", "name": "aype-multicam"})
                mid = mc.get("item") or mc.get("sequence") or (mc.get("items") or [None])[0]
                ms = app.res("file.newSequence", {"name": "aype-mc", "fromItem": mid})["sequence"]
                app.ex("edit.selectAll")
                app.ex("playhead.set", {"seconds": 5})
                before = app.hist()
                st2 = self.click_item("cut", f"multicam.cutToCamera{cam}")
                ok = app.hist() > before
                info = app.ex("multicam.inspect")
                app.ex("sequence.open", {"item": self.seq_id})
                app.ex("sequence.close", {"item": ms})
                if ok:
                    return "PASS", f"bare click on a plain sequence: {st!r}; on a multicam sequence (2 angles) the click cut (+1)"
                raise Fail(f"plain: {st!r}; multicam: {st2!r}; {json.dumps(info, ensure_ascii=False)[:200]}")
            return f
        for cam in (1, 2):
            self.run(f"Multicam: cắt sang camera {cam}", "Cắt bằng chữ", f"click ayp.item.multicam.cutToCamera{cam}", multicam(cam))

        # ---------------- Giọng đọc
        def tts():
            st = self.click_item("voice", "tts.create")
            vs = [v["id"] for v in app.res("tts.voices")["voices"] if v.get("language") == "vi-VN" and v.get("installed")]
            r = app.res("tts.create", {"text": "Xin chào, đây là giọng đọc thử của AYP.", "voice": "vieneu-ngoc-huyen", "dir": W})
            d = probe(r.get("path"))
            if d <= 0:
                raise Fail(f"no audio at {r.get('path')}")
            return "PASS(panel)", f"{len(vs)} VieNeu voices installed; vieneu-ngoc-huyen → {d:.2f} s WAV on {r.get('track')}; button shows {st!r}"
        self.run("Giọng đọc VieNeu (tts.create)", "Giọng đọc", "engine.execute tts.create {text, voice}", tts)

        # ---------------- Hiệu ứng
        for eff, name in (("skin_smooth", "Làm mịn da"), ("gaussian_blur", "Làm mờ"), ("warp_stabilizer", "Chống rung (Warp Stabilizer)"),
                          ("auto_reframe", "Tự khung hình 9:16 (Auto Reframe)"), ("ultra_key", "Xoá phông xanh (Ultra Key)"),
                          ("horizontal_flip", "Lật ngang"), ("crop", "Cắt khung")):
            def fx(eff=eff):
                since = self.job_ids()
                r = self.click_edit("effects", f"effects.apply.{eff}")
                if eff not in effects_of(app.seq(), self.v):
                    raise Fail(f"{eff} not on the clip: {effects_of(app.seq(), self.v)}")
                js = self.wait_jobs(since, 300)
                jn = "; jobs: " + ", ".join(f"{j.get('name', j.get('label'))} {j.get('status')}" for j in js) if js else ""
                return r[0], r[1] + jn
            self.run(f"{name} ({eff})", "Hiệu ứng", f"click ayp.item.effects.apply.{eff}", fx)

        def speed():
            st = self.click_item("effects", "clip.speedDuration")
            d = app.ui().get("clip_dialog") or {}
            if d.get("command") != "clip.speedDuration":
                raise Fail(f"no Speed/Duration dialog; status {st!r}")
            app.call("ui.set", {"clipDialog": {"speed": 200}})
            before = app.hist()
            app.call("ui.click", {"id": "speedDuration.ok"})
            time.sleep(0.3)
            it = clip_of(app.seq(), self.v)
            if app.hist() <= before:
                raise Fail(f"OK did not apply; status {app.status()!r}")
            return "PASS", f"dialog → speed 200 % → OK: clip speed {it['speed']}, {it['durationFrames']} frames"
        self.run("Tốc độ / đảo ngược (clip.speedDuration)", "Hiệu ứng", "click ayp.item.clip.speedDuration + dialog OK", speed)
        self.run("Dừng hình (clip.frameHold)", "Hiệu ứng", "click ayp.item.clip.frameHold", lambda: self.click_edit("effects", "clip.frameHold"))

        def mask():
            r = self.click_edit("effects", "masks.add")
            m = app.res("masks.list", {"clip": self.v})
            return r[0], r[1] + f"; bare click with only the clip selected → {json.dumps(m, ensure_ascii=False)[:120]}"
        self.run("Tạo mặt nạ (masks.add ellipse)", "Hiệu ứng", "click ayp.item.masks.add", mask)

        def track():
            app.res("masks.add", {"clip": self.v, "shape": "ellipse"})
            app.ex("playhead.set", {"seconds": 1})
            since = self.job_ids()
            before = app.hist()
            st = self.click_item("effects", "masks.track")
            js = self.wait_jobs(since, 300)
            if st.startswith("Chưa làm được"):
                raise Fail(st)
            n = app.hist() - before
            if n <= 0 and not js:
                raise Fail(f"nothing tracked; status {st!r}")
            return "PASS", f"+{n} step(s); jobs {[(j.get('name', j.get('label')), j.get('status')) for j in js]}"
        self.run("Mặt nạ bám theo vật (masks.track)", "Hiệu ứng", "click ayp.item.masks.track", track)

        def nest():
            st = self.click_item("effects", "clip.nest")
            d = app.ui().get("clip_dialog") or {}
            if d.get("command") != "clip.nest":
                raise Fail(f"no Nest dialog; status {st!r}")
            before = app.hist()
            app.call("ui.click", {"id": "nest.ok"})
            time.sleep(0.3)
            return self.grew(before, app.status(), "dialog → OK")
        self.run("Gộp thành nest (clip.nest)", "Hiệu ứng", "click ayp.item.clip.nest + dialog OK", nest)

        def group():
            c = app.res("graphics.newText", {"text": "Nhóm", "seconds": 3})["clip"]
            app.ex("timeline.select", {"clips": [self.v, self.au, c]})
            return self.click_edit("effects", "clip.group")
        self.run("Nhóm clip (clip.group)", "Hiệu ứng", "click ayp.item.clip.group", group)

        # ---------------- Chuyển tiếp
        def trans():
            r = self.click_edit("transitions", "sequence.applyVideoTransition")
            return r[0], r[1] + f"; V1 transitions {len(app.seq()['video'][0]['transitions'])}"
        self.run("Chuyển cảnh mờ (sequence.applyVideoTransition)", "Chuyển tiếp", "click ayp.item.sequence.applyVideoTransition", trans)

        # ---------------- Màu
        def lp():
            st = self.click_item("color", "lumetri.applyPreset")
            name = app.res("lumetri.presets")["presets"][0]["name"]
            before = app.hist()
            app.res("lumetri.applyPreset", {"name": name})
            self.grew(before, st)
            return "PASS(panel)", f"{name!r} → effects {effects_of(app.seq(), self.v)}; button shows {st!r}"
        self.run("Bộ lọc màu (lumetri.applyPreset)", "Màu", "engine.execute lumetri.applyPreset {name}", lp)

        def lut():
            cube = os.path.join(W, "aype-identity.cube")
            with open(cube, "w") as f:
                f.write('TITLE "aype identity"\nLUT_3D_SIZE 2\n' + "".join(f"{r} {g} {b}\n" for b in (0, 1) for g in (0, 1) for r in (0, 1)))
            r = app.res("lut.import", {"path": cube, "name": "aype-check identity"})
            lid = (r or {}).get("id")
            look = app.ex("lumetri.setLook", {"clip": self.v, "lut": f"lib:{lid}"}) if lid else {"ok": False}
            if lid:
                app.ex("lut.remove", {"id": lid})  # keep the user's LUT library as it was
            return "PASS(panel)", f"imported {r}; applied as Look: {look.get('ok')}; removed from the library again"
        self.run("Nhập LUT (lut.import)", "Màu", "engine.execute lut.import {path}", lut)

        def match():
            bare = app.ex("lumetri.applyMatch", {"clip": self.v, "referenceTime": 5 * TICKS})
            app.res("sequence.addEdit", {"time": 10 * TICKS})  # a second shot: the reference must come from another clip
            q = app.seq()
            later = next(it["clip"] for it in q["video"][0]["items"] if it["start"] > 0)
            r = app.ex("lumetri.applyMatch", {"clip": later, "referenceTime": 5 * TICKS})
            if not r.get("ok"):
                raise Fail(r.get("error"))
            return "PASS(panel)", (f"same-clip reference refused ({bare.get('error', 'ok')}); after a cut at 10 s, the shot at 20 s matched to "
                                   f"the frame at 5 s: {json.dumps(r['result'], ensure_ascii=False)[:140]}")
        self.run("Khớp màu (lumetri.applyMatch)", "Màu", "engine.execute lumetri.applyMatch {referenceTime}", match)

        def lumetri_adjust():
            app.res("lumetri.applyPreset", {"name": app.res("lumetri.presets")["presets"][0]["name"]})
            eff = [e for e in clip_of(app.seq(), self.v)["effects"] if "lumetri" in e["effect"]]
            if not eff:
                raise Fail("no Lumetri effect")
            before = app.hist()
            app.res("effects.setParam", {"clip": self.v, "effect": "lumetri", "param": "exposure", "value": 0.5})
            app.res("effects.setParam", {"clip": self.v, "effect": "lumetri", "param": "contrast", "value": 20})
            self.grew(before, "")
            p = next(e for e in clip_of(app.seq(), self.v)["effects"] if e["effect"] == "lumetri")["params"]
            return "PASS(panel)", f"exposure {p['exposure']['value']}, contrast {p['contrast']['value']} (the Lumetri panel's own command)"
        self.run("Chỉnh màu Lumetri (effects.setParam)", "Màu", "engine.execute effects.setParam on Lumetri", lumetri_adjust)

        # ---------------- Motion AYP
        def mblur():
            r = self.click_edit("motion", "motion.blur")
            if "gaussian_blur" not in effects_of(app.seq(), self.v):
                raise Fail("no gaussian_blur")
            return r
        self.run("Làm mờ cả clip (Motion AYP)", "Motion AYP", "click ayp.item.motion.blur", mblur)
        self.review()

        # ---------------- Thông số (right column)
        def props():
            app.call("ui.ayp.open", {"side": "props"})
            time.sleep(0.3)
            side = next((e["rect"] for e in app.elements("panel.AypSide") if e["id"] == "panel.AypSide"), None)
            inside = [e["id"] for e in app.call("ui.elements", {"prefix": ""})["result"]
                      if side and e["id"].startswith(("properties.", "props.", "ec.", "effectControls."))
                      and side[0] <= e["rect"][0] <= side[0] + side[2]]
            hint = app.elements("ayp.hint.selectClip")
            if hint or not inside:
                raise Fail(f"Thông số shows the hint or nothing for a selected clip ({len(inside)} controls)")
            before = app.hist()
            for param, value in (("position", [400, 900]), ("scale", 120), ("rotation", 15)):
                app.res("effects.setParam", {"clip": self.v, "effect": "motion", "param": param, "value": value})
            app.res("effects.setParam", {"clip": self.v, "effect": "opacity", "param": "opacity", "value": 80})
            app.res("effects.setParam", {"clip": self.v, "effect": "opacity", "param": "blend", "value": "Multiply"})
            m = {e["effect"]: e["params"] for e in clip_of(app.seq(), self.v)["effects"]}
            vals = {k: m["motion"][k]["value"] for k in ("position", "scale", "rotation")}
            vals.update(opacity=m["opacity"]["opacity"]["value"], blend=m["opacity"]["blend"]["value"])
            return "PASS", f"{len(inside)} controls in Thông số; +{app.hist() - before} steps; {vals}"
        self.run("Vị trí/tỷ lệ/xoay/độ mờ/hoà trộn", "Thông số", "Thông số panel + engine.execute effects.setParam", props)

        def keyframes():
            for t, v in ((0, 100), (2 * TICKS, 130)):
                app.res("effects.setParam", {"clip": self.v, "effect": "motion", "param": "scale", "value": v, "time": t, "keyframe": True})
            k = next(e for e in clip_of(app.seq(), self.v)["effects"] if e["effect"] == "motion")["params"]["scale"]["keyframes"]
            if k < 2:
                raise Fail(f"{k} keyframes")
            return "PASS", f"{k} scale keyframes ◆"
        self.run("Keyframe ◆", "Thông số", "engine.execute effects.setParam keyframe", keyframes)

        # ---------------- Timeline toolbar
        def undo_redo():
            app.res("markers.add", {"name": "undo-test"})
            h = app.hist()
            self.click_tool("edit.undo")
            h1 = app.hist()
            self.click_tool("edit.redo")
            h2 = app.hist()
            if not (h1 == h - 1 and h2 == h):
                raise Fail(f"history {h} → undo {h1} → redo {h2}")
            return "PASS", f"history {h} → Hoàn tác {h1} → Làm lại {h2} (limit 200 set by file.newProject)"
        self.run("Hoàn tác / Làm lại", "Thanh công cụ", "click ayp.tool.edit.undo / edit.redo", undo_redo)

        def split():
            before = app.hist()
            st = self.click_tool("sequence.addEdit")
            n = len(app.seq()["video"][0]["items"])
            if n < 2:
                raise Fail(f"V1 still has {n} clip(s); status {st!r}")
            return self.grew(before, st, f"V1 now {n} clips")
        self.run("Tách (sequence.addEdit)", "Thanh công cụ", "click ayp.tool.sequence.addEdit", split)

        def ripple():
            app.res("sequence.addEdit", {})
            q = app.seq()
            second = [it["clip"] for t in q["video"][:1] + q["audio"][:1] for it in t["items"] if it["start"] > 0]
            app.res("timeline.select", {"clips": second})
            d0 = q["durationFrames"]
            before = app.hist()
            st = self.click_tool("edit.rippleDelete")
            d1 = app.seq()["durationFrames"]
            if d1 >= d0:
                raise Fail(f"duration {d0} → {d1}; status {st!r}")
            return self.grew(before, st, f"{d0} → {d1} frames")
        self.run("Xoá kéo khít (edit.rippleDelete)", "Thanh công cụ", "click ayp.tool.edit.rippleDelete", ripple)

        def hold_tool():
            before = app.hist()
            st = self.click_tool("clip.frameHold")
            return self.grew(before, st)
        self.run("Dừng hình (thanh công cụ)", "Thanh công cụ", "click ayp.tool.clip.frameHold", hold_tool)

        def link():
            app.res("clip.link", {"clips": [self.v, self.au]})  # toggles: unlink first
            linked0 = clip_of(app.seq(), self.v).get("link")
            app.res("timeline.select", {"clips": [self.v, self.au]})
            before = app.hist()
            st = self.click_tool("clip.link")
            linked1 = clip_of(app.seq(), self.v).get("link")
            if linked1 == linked0:
                raise Fail(f"link {linked0} → {linked1}; status {st!r}")
            return self.grew(before, st, f"link {linked0} → {linked1}")
        self.run("Liên kết (clip.link)", "Thanh công cụ", "click ayp.tool.clip.link", link)

        def snap():
            s0 = app.res("state.inspect").get("snapping")
            self.click_tool("sequence.snap")
            s1 = app.res("state.inspect").get("snapping")
            self.click_tool("sequence.snap")
            s2 = app.res("state.inspect").get("snapping")
            if s1 == s0 or s2 != s0:
                raise Fail(f"snapping {s0} → {s1} → {s2}")
            return "PASS", f"Hút {s0} → {s1} → {s2}"
        self.run("Tự hút (sequence.snap)", "Thanh công cụ", "click ayp.tool.sequence.snap ×2", snap)

        def tmarker():
            n0 = len(app.seq()["markers"])
            before = app.hist()
            st = self.click_tool("markers.add")
            return self.grew(before, st, f"markers {n0} → {len(app.seq()['markers'])}")
        self.run("Marker (thanh công cụ)", "Thanh công cụ", "click ayp.tool.markers.add", tmarker)

        def zoom():
            p0 = app.ui()["timeline"].get("target_pps")
            self.click_tool("view.zoomIn")
            p1 = app.ui()["timeline"].get("target_pps")
            self.click_tool("view.zoomOut")
            p2 = app.ui()["timeline"].get("target_pps")
            if not (p1 and p0 and p1 > p0 and p2 < p1):
                raise Fail(f"pps {p0} → {p1} → {p2}")
            return "PASS", f"pixels/s {p0:.1f} → Phóng to {p1:.1f} → Thu nhỏ {p2:.1f}"
        self.run("Phóng to / Thu nhỏ timeline", "Thanh công cụ", "click ayp.tool.view.zoomIn / zoomOut", zoom)

        def xuat():
            st = self.click_tool("file.exportMedia")
            mode = str(app.ui().get("mode"))
            app.ex("mode.edit")
            if "export" not in mode.lower():
                raise Fail(f"mode {mode}; status {st!r}")
            return "PASS", "opens Export mode"
        self.run("Xuất (thanh công cụ)", "Thanh công cụ", "click ayp.tool.file.exportMedia", xuat)

        def dup():
            app.res("timeline.select", {"clips": [self.v, self.au]})
            app.res("edit.copy")
            app.res("playhead.set", {"seconds": 60})
            before = app.hist()
            app.res("edit.paste")
            n = len(app.seq()["video"][0]["items"])
            if n < 2:
                raise Fail(f"V1 has {n} clip(s) after paste")
            return self.grew(before, "", f"V1 now {n} clips")
        self.run("Nhân đôi clip (edit.copy + edit.paste)", "Tìm lệnh…", "engine.execute edit.copy / edit.paste", dup)

        def quick():
            d, mb = self.export_check("export.quick", {"path": os.path.join(W, "x-quick.mp4"), "format": "h264", "bitrateKbps": 8000, "range": "inOut"}, 3)
            return "PASS", f"In/Out 3 s at 8 Mbps → {d:.2f} s, {mb:.1f} MB (ffprobe)"
        self.run("Xuất nhanh MP4 (export.quick)", "Tìm lệnh…", "engine.execute export.quick {path, 8 Mbps, inOut}", quick)

        def palette():
            app.call("ui.click", {"id": "ayp.find"})
            time.sleep(0.3)
            p = app.call("ui.ayp.state")["result"].get("palette")
            app.call("ui.key", {"key": "Escape"})
            if p is None:
                raise Fail("Tìm lệnh… did not open")
            return "PASS", "Tìm lệnh… opens over every command"
        self.run("Tìm lệnh… (⌘K)", "Đầu cột trái", "click ayp.find", palette)

        def ai_sees():
            cmds = app.call("engine.commands")["result"]
            ids = {c["id"] for c in cmds}
            tabs = app.call("ui.ayp.tabs")["result"]
            missing = [i["cmd"] for t in tabs for i in t["items"] if i["cmd"] not in ids]
            if missing:
                raise Fail(f"tab commands missing from engine.commands: {missing}")
            return "PASS", f"{len(cmds)} commands; every AYP button's command is in engine.commands (same session the user edits)"
        self.run("AI nhìn thấy thao tác tay (command.list)", "AI / MCP", "engine.commands + ui.ayp.tabs", ai_sees)

    def review(self):
        """review.open exports the active sequence into <AYP work folder>/Render and serves it on 8765."""
        app, a = self.app, self.a
        if free_gb() < 2:
            self.row("Duyệt góp ý (review.open)", "Motion AYP", "click ayp.item.review.open", "SKIP", "under 2 GB free")
            return
        server_before = subprocess.run(["lsof", "-nP", "-iTCP:8765", "-sTCP:LISTEN", "-t"], capture_output=True, text=True).stdout.split()
        self.reset()
        # a 2-second sequence keeps the review export small
        rs = app.res("file.newSequence", {"name": "aype-review-check", "width": 1080, "height": 1920, "fps": 30})["sequence"]
        app.res("graphics.newText", {"text": "Duyệt thử", "seconds": 2, "time": 0})
        out = {}
        try:
            since = self.job_ids()
            st = self.click_item("motion", "review.open")
            js = self.wait_jobs(since, 300)
            path = next(((j.get("result") or {}).get("path") for j in js if (j.get("result") or {}).get("path")), None)
            out["path"] = path
            time.sleep(2)
            app.call("ui.focus", {})  # the review page opens in the browser over the editor; take the window back
            d = probe(path)
            code = 0
            for _ in range(20):
                try:
                    code = urllib.request.urlopen("http://127.0.0.1:8765/", timeout=3).status
                    break
                except OSError:
                    time.sleep(0.5)
            if st.startswith("Chưa làm được") or d <= 0 or code != 200:
                self.row("Duyệt góp ý (review.open)", "Motion AYP", "click ayp.item.review.open", "FAIL", f"status {st!r}; clip {path} {d:.2f} s; page {code}")
            else:
                self.row("Duyệt góp ý (review.open)", "Motion AYP", "click ayp.item.review.open", "PASS", f"exported {d:.2f} s to {path} (8 Mbps); http://127.0.0.1:8765/ → {code}")
            # review.import: one open note in the feedback file the page writes beside the clip
            if path:
                fb = path + ".feedback.json"
                with open(fb, "w") as f:
                    json.dump({"fps": 30, "items": [{"id": "t1", "frame": 15, "frameEnd": 45, "tc": "00:00:00:15", "text": "Chữ to hơn chút", "status": "open", "reply": ""}]}, f, ensure_ascii=False)
                n0 = len(app.seq()["markers"])
                st = self.click_item("motion", "review.import")
                names = [m.get("name") for m in app.seq()["markers"]]
                ok = any(str(n).startswith("Góp ý") for n in names)
                dump = subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), "..", "ayp-review", "review.py"), "dump", path],
                                      capture_output=True, text=True, timeout=30)
                dumped = dump.returncode == 0 and "Chữ to hơn chút" in dump.stdout
                self.row("Góp ý thành marker (review.import)", "Motion AYP", "click ayp.item.review.import", "PASS" if ok else "FAIL",
                         f"markers {n0} → {len(names)} {names[-1:] if ok else ''}; status {st!r}; review.py dump reads the note: {dumped}")
        finally:
            app.ex("sequence.open", {"item": self.seq_id})
            app.ex("sequence.close", {"item": rs})
            if out.get("path") and os.path.exists(out["path"]):
                folder = os.path.dirname(out["path"])
                for f in (out["path"], out["path"] + ".feedback.json"):
                    if os.path.exists(f):
                        os.remove(f)
                if not os.listdir(folder):
                    os.rmdir(folder)
            app.call("ui.focus", {})
            if not server_before:  # stop the review page only if this run started it
                for pid in subprocess.run(["lsof", "-nP", "-iTCP:8765", "-sTCP:LISTEN", "-t"], capture_output=True, text=True).stdout.split():
                    subprocess.run(["kill", pid])


def table(rows):
    lines = ["| # | Chức năng | Ở đâu | Đường thử | Kết quả | Ghi chú |", "|---|---|---|---|---|---|"]
    for i, (f, w, p, r, n) in enumerate(rows, 1):
        lines.append(f"| {i} | {f} | {w} | `{p}` | **{r}** | {n} |")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--port", type=int, default=9879)
    ap.add_argument("--media", required=True, help="clip with Vietnamese speech, copied to a temp folder first")
    ap.add_argument("--media2", required=True, help="a second clip (import and multicam angle)")
    ap.add_argument("--work", required=True, help="temp folder for the project and the exports")
    ap.add_argument("--model", default="whisper-large-v3-turbo")
    ap.add_argument("--out", default="docs/aypeditor-check.md")
    a = ap.parse_args()
    app = App(a.port)
    c = Check(app, a)
    t0 = time.time()
    tr = c.setup()
    print(f"setup: transcript {tr} in {time.time() - t0:.0f} s", flush=True)
    c.all()
    c.reset()
    counts = {}
    for r in c.rows:
        k = r[3].split("(")[0] if r[3].startswith("PASS(") else r[3]
        k = "PASS(…)" if r[3].startswith("PASS(") else k
        counts[k] = counts.get(k, 0) + 1
    summary = ", ".join(f"{k} {v}" for k, v in sorted(counts.items()))
    poll = "\n".join(f"- `{w}` added {p} to the AYP dock" for w, p in c.pollution) or "- none: no command added a classic panel tab to the AYP dock"
    md = f"""# AYPEditor: chạy thật các chức năng có sẵn trên clip thật

Generated by `tools/ayp-editor/check_features.py` on {time.strftime('%Y-%m-%d %H:%M')} against a running AYPEditor
(port {a.port}). Clip: `{os.path.basename(a.media)}` (first 60 s of `0716.mp4`, Vietnamese speech, 1080×1080) in a
1080×1920 sequence; transcript by `{a.model}` (vi): {json.dumps(tr, ensure_ascii=False)}.

Paths: `click` = ui.click on the AYP button, the way a person clicks it; `engine.execute` = the AI's path, used
when the button opens a dialog (PASS(dialog)), points at the embedded classic panel (PASS(panel)) or is for the
AI (PASS(ai)). Each row starts from the same state (history undone back, clip selected, playhead at 20 s).

**Result: {summary}** ({len(c.rows)} rows).

{table(c.rows)}

## Classic panels added to the AYP layout

{poll}
"""
    md = md.replace(a.work.rstrip("/") + "/", "<work>/").replace(a.work, "<work>")  # no machine-specific temp paths
    # the hand-written part (old project, AYP tools, notes) below the marker survives a re-run
    marker = "<!-- written by hand below -->"
    tail = ""
    if os.path.exists(a.out):
        old = open(a.out, encoding="utf-8").read()
        if marker in old:
            tail = old[old.index(marker):]
    with open(a.out, "w", encoding="utf-8") as f:
        f.write(md + ("\n" + tail if tail else ""))
    print(f"\n{summary}\nwrote {a.out}")


if __name__ == "__main__":
    main()
