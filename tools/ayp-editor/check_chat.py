#!/usr/bin/env python3
"""Real-app acceptance of AYPEditor's AI chat (plan 2, Task 9).

    AYP_EDITOR=1 ~/Applications/AYPEditor.app/Contents/MacOS/FilmCraft --control 9899 &
    python3 tools/ayp-editor/check_chat.py --port 9899 --media <clip with speech>

Reuses the control client and the safety guards of check_features.py: refuses any app on --port that is not
AYPEditor, and any AYPEditor with unsaved work. It opens a COPY of the clip in a scratch project, runs the
checks, then leaves the app on an empty project (quit it with app.quit). Rows with a live key (d, e) need an
Anthropic key in the Keychain (service vn.ayp.editor) or ANTHROPIC_API_KEY; without one they are "chờ khoá".
Stdlib only.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from check_features import App, Fail, table


def has_key():
    if os.environ.get("ANTHROPIC_API_KEY"):
        return True
    r = subprocess.run(["security", "find-generic-password", "-s", "vn.ayp.editor"], capture_output=True)
    return r.returncode == 0


def shoot(app, work, shots, noframe):
    """A screenshot needs a visible window; when the display sleeps it is refused or waits: the machine's doing, not a freeze."""
    try:
        r = app.call("ui.screenshot", {"path": os.path.join(work, "bg.png")}, timeout=70)
        (shots if r.get("ok") else noframe).append(r.get("error") or "ok")
    except OSError as e:
        noframe.append(f"{type(e).__name__}")


def probe_jobs(app):
    r = app.call("engine.execute", {"id": "jobs.list", "params": {}}, timeout=5)
    return r.get("result") or []


def wake():
    """A hidden or sleeping display draws no frames and the app only works on frames: wake it and bring the app forward."""
    subprocess.run(["caffeinate", "-u", "-t", "2"])
    subprocess.run(["open", "-a", os.path.expanduser("~/Applications/AYPEditor.app")])


def cards(app):
    return app.call("ui.ayp.chat.state")["result"]["cards"]


def kind(card):
    return next(iter(card)) if isinstance(card, dict) and card else str(card)


def wait(cond, limit, step=0.5):
    t0 = time.time()
    while time.time() - t0 < limit:
        v = cond()
        if v:
            return v
        time.sleep(step)
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--port", type=int, default=9899)
    ap.add_argument("--media", required=True)
    ap.add_argument("--prompt", default="cắt bớt khoảng lặng ở 20 giây đầu", help="the live-turn request (d)")
    ap.add_argument("--seconds", type=int, default=60, help="length of the clip copy")
    ap.add_argument("--shot", help="save the AI-tab screenshot here")
    ap.add_argument("--skip-b", action="store_true", help="skip the background transcription (it still runs once so d has a transcript)")
    ap.add_argument("--out", help="write the rows as a Markdown table here")
    a = ap.parse_args()
    app = App(a.port)
    try:
        tabs, state, proj = app.call("ui.ayp.tabs"), app.call("ui.ayp.state"), app.call("engine.execute", {"id": "project.inspect"})
    except OSError as e:
        sys.exit(f"nothing answers on 127.0.0.1:{a.port} ({e}); start AYPEditor with --control {a.port}")
    if not (tabs.get("ok") and "workspace" in (state.get("result") or {}) and app.call("ui.ayp.chat.state").get("ok")):
        sys.exit(f"the app on port {a.port} is not AYPEditor with the AI chat; refusing to touch it")
    pr = proj.get("result") or {}
    if pr.get("dirty") and (pr.get("root") or {}).get("children"):  # an empty project (a run's own leftover) loses nothing
        sys.exit(f"AYPEditor on port {a.port} has unsaved changes; save or close them first")

    wake()
    work = tempfile.mkdtemp(prefix="aype-chat-")
    clip = os.path.join(work, "clip.mp4")
    if subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", a.media, "-t", str(a.seconds), "-c", "copy", clip]).returncode:
        shutil.copy(a.media, clip)
    rows = []

    def row(name, result, note):
        rows.append((name, result, note.replace("|", "\\|").replace("\n", " ")))
        print(f"{result:6} {name} — {note}", flush=True)

    def run(name, fn):
        try:
            row(name, *fn())
        except Fail as e:
            row(name, "FAIL", str(e))
        except Exception as e:  # a crashed check must not hide the others
            row(name, "FAIL", f"{type(e).__name__}: {e}")

    app.call("ui.click", {"id": "recovery.later"})  # leftover recovery prompt: Not Now, never Discard
    app.res("file.newProject", {"name": "aype-chat-check"})
    item = app.res("file.import", {"paths": [clip]})["items"][0]
    seq_id = app.res("file.newSequence", {"name": "Kiểm tra chat", "width": 1080, "height": 1920, "fromItem": item})["sequence"]
    app.res("file.saveAs", {"path": os.path.join(work, "chat-check.fcproj")})
    q = app.seq()
    v, au = q["video"][0]["items"][0]["clip"], q["audio"][0]["items"][0]["clip"]
    base = app.hist()
    app.ex("timeline.select", {"clips": [v, au]})
    app.ex("playhead.set", {"seconds": 5})
    app.call("ui.ayp.open", {"tab": "cut", "side": "ai"})

    # (a) three AYP buttons by key
    def keys():
        out = []
        for key in ("markers.add", "audio.fadeIn", "graphics.newText"):
            h0 = app.hist()
            r = app.call("ui.ayp.run", {"key": key})
            if not r.get("ok"):
                raise Fail(f"{key}: {r.get('error')}")
            out.append(f"{key} +{app.hist() - h0} step(s), status {r['result'].get('status')!r}")
        if app.call("ui.ayp.run", {"key": "no.such.key"}).get("ok"):
            raise Fail("an unknown key was accepted")
        if app.hist() <= base:
            raise Fail("no key changed the project")
        return "PASS", "; ".join(out) + "; unknown key refused"
    run("a. ui.ayp.run for three AYP keys", keys)
    for _ in range(40):
        if app.hist() <= base:
            break
        app.ex("edit.undo")

    # (b) background transcription, the UI keeps answering
    def bg():
        app.ex("timeline.select", {"clips": [v, au]})
        r = app.ex("transcript.generate", {"background": True, "language": "vi", "model": "whisper-large-v3-turbo"})
        if not r.get("ok"):
            raise Fail(f"transcript.generate background: {r.get('error')}")
        job = (r.get("result") or {}).get("job")
        # a second connection probes the UI thread while the first one may sit in a screenshot
        probe, lat, n, t0, shots, noframe = App(a.port), [], 0, time.time(), [], []
        th = None
        while time.time() - t0 < 600:
            if n % 20 == 0:
                wake()
            t = time.time()
            rr = probe.call("ui.ayp.state", timeout=5)
            lat.append(time.time() - t)
            n += 1
            if not rr.get("ok"):
                raise Fail(f"ui.ayp.state failed during the job: {rr.get('error')}")
            if n % 3 == 0 and not (th and th.is_alive()):
                th = threading.Thread(target=shoot, args=(app, work, shots, noframe), daemon=True)
                th.start()
            js = [j for j in probe_jobs(probe) if job is None or j.get("id") == job]
            if js and all(j.get("finished") for j in js):
                if th:
                    th.join(80)  # the first connection is free again
                words = wait(lambda: app.ex("playhead.set", {"seconds": 5}) and len(app.res("transcript.inspect")["words"]), 30, 1) or 0  # applied on a UI frame; a hidden window draws few, so nudge it
                st = js[0].get("state") or js[0].get("status") or ""
                note = f"job {job} ran {time.time() - t0:.0f} s; {n} state replies on a 2nd connection + {len(shots)} screenshots ({len(noframe)} not taken: {noframe[:1]}), slowest {max(lat):.2f} s; {words} words {st}"
                if max(lat) > 2.0:
                    raise Fail(note + " (a reply took over 2 s: the UI froze)")
                if not words:
                    return "SKIP", note + "; Whisper gave no words in AYPEditor, nothing to fake"
                return "PASS", note
            time.sleep(0.4)
        raise Fail("the job did not finish in 10 min")
    run("b. chép lời in the background, UI stays responsive", bg)

    # (c) no key: the key error card
    def nokey():
        if has_key():
            return "SKIP", "a key exists on this machine; (c) needs none"
        h0 = app.hist()
        r = app.call("ui.ayp.chat.send", {"text": "cắt bớt khoảng lặng ở 20 giây đầu"})
        if not r.get("ok"):
            raise Fail(f"send refused: {r.get('error')}")
        if not wait(lambda: not app.call("ui.ayp.chat.state")["result"]["busy"], 15):
            raise Fail("the turn never ended")
        cs = cards(app)
        errs = [c["Error"] for c in cs if kind(c) == "Error"]
        if not errs or "khoá" not in errs[-1]:
            raise Fail(f"no key-error card: {json.dumps(cs, ensure_ascii=False)[:300]}")
        if a.shot:
            app.call("ui.ayp.open", {"tab": "cut", "side": "ai"})
            time.sleep(0.5)
            app.call("ui.screenshot", {"path": a.shot})
        if app.hist() != h0:
            raise Fail("the failed turn changed the project")
        return "PASS", f"cards {[kind(c) for c in cs]}; Error card: {errs[-1]!r}; project untouched"
    run("c. no key: chat shows the key error card", nokey)

    # (d) live turn, (e) manual edit refused during it
    if not has_key():
        row("d. live turn: Plan → approve → Result → Hoàn tác cả lượt", "SKIP", "chờ khoá (no Anthropic key on this machine; none requested)")
        row("e. manual edit during a turn is refused", "SKIP", "chờ khoá (needs a running turn; the no-key turn ends at once)")
    else:
        before = {}

        def live():
            app.res("file.save")
            before["seq"] = app.seq()
            n0 = len(cards(app))  # the chat keeps old cards: only the ones after this send are this turn's
            app.call("ui.ayp.chat.send", {"text": a.prompt})
            refused, plan_seen = None, False
            t0 = time.time()
            while time.time() - t0 < 180:
                wake()
                s = app.call("ui.ayp.chat.state")["result"]
                if s["busy"] and refused is None:
                    e = app.ex("markers.add")
                    refused = "" if e.get("ok") else e.get("error")  # accepted ("") is a FAIL; never undo here, it would undo the AI's step
                ks = [kind(c) for c in s["cards"][n0 + 1:]]
                last = s["cards"][-1] if s["cards"] else {}
                if ks and ks[-1] == "Choice" and (last.get("Choice") or {}).get("picked") is None:
                    app.call("ui.ayp.chat.choose", {"option": 0})  # the first option is the closest to what was asked
                if ks and ks[-1] == "Plan" and (last.get("Plan") or {}).get("state") == "Pending":  # waiting on us, so not "busy"
                    plan_seen = True
                    app.call("ui.ayp.chat.plan", {"approve": True})
                if not s["busy"] and ks and ks[-1] in ("Result", "Error", "Text"):
                    break
                time.sleep(1)
            cs = cards(app)[n0:]
            ks = [kind(c) for c in cs]
            if "Result" not in ks:
                raise Fail(f"no Result card: {json.dumps(cs, ensure_ascii=False)[:600]}")
            r = app.call("ui.ayp.chat.undoTurn", {"card": n0 + max(i for i, k in enumerate(ks) if k == "Result")})
            if not r.get("ok"):
                lab = [str(x)[:40] for x in app.res("history.list")["undo"][-4:]]
                raise Fail(f"cards {ks}; undoTurn: {r.get('error')}; last undo steps {lab}")
            same = app.seq() == before["seq"]
            if not same:
                raise Fail(f"cards {ks}; sequence differs after Hoàn tác cả lượt")
            before["refused"] = refused
            txt = [(kind(c), str(next(iter(c.values())))[:160]) for c in cs]
            # the model decides whether a request needs a plan: a one-step edit may go without one
            return "PASS", f"{'Plan approved' if plan_seen else 'no Plan card (the model went straight to the edit)'}; cards {txt}; sequence.inspect equal after Hoàn tác cả lượt"
        run("d. live turn: Plan → approve → Result → Hoàn tác cả lượt", live)
        row("   requests", "INFO", "1 live turn (the plan and result take a few API requests inside it)")
        r = before.get("refused")
        row("e. manual edit during a turn is refused", "PASS" if r else ("SKIP" if r is None else "FAIL"),
            f"markers.add while busy → {r!r}" if r else "turn never seen busy" if r is None else "the edit was accepted while busy")

    app.ex("file.newProject", {"name": "Untitled"})
    shutil.rmtree(work, ignore_errors=True)
    counts = {}
    for _, r, _ in rows:
        counts[r] = counts.get(r, 0) + 1
    print("\n" + " · ".join(f"{k} {v}" for k, v in counts.items()))
    md = "| # | Kiểm tra | Kết quả | Ghi chú |\n|---|---|---|---|\n" + "\n".join(
        f"| {i} | {n} | **{r}** | {t} |" for i, (n, r, t) in enumerate(rows, 1))
    print(md)
    if a.out:
        open(a.out, "w", encoding="utf-8").write(md + "\n")
    sys.exit(1 if counts.get("FAIL") else 0)


if __name__ == "__main__":
    main()
