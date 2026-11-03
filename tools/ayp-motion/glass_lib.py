"""AYP: bộ dựng chung cho typo/motion kính mờ (EffectCraft, cổng điều khiển 9880):
kính mờ có texture (tint trắng + hạt + viền sáng mảnh), chữ Be Vietnam Pro trắng glow vừa, bay vào có
motion blur + vượt nhẹ rồi lắng, ra khác kiểu vào. Dùng: import glass_lib as G (xem docs/ayp-styles.md).

Quy ước: mọi thời gian là giây trong comp; comp 1080×1920 25 fps, đặt giữa khung FilmCraft.
Layer mới nằm TRÊN cùng → dựng từ dưới lên."""
import math, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ec import ex, call  # noqa: E402,F401

F, W, Y, GREEN = "Be Vietnam Pro", "#ffffff", "#ffd200", "#34c759"
FPS = 25


def ok(r):
    if not r.get("ok"):
        raise SystemExit(f"EffectCraft: {r.get('error')}")
    return r["result"]


# ---------- keyframe ----------
def anim(layer, path, pts, style="out", infl=80):
    """pts [(t, v)]. style: out = lao nhanh rồi phanh (vào); in = khởi chậm rồi vút (ra); io = êm hai đầu."""
    pts = [(round(t, 3), v) for t, v in pts]
    for t, v in pts:
        ok(ex("prop.addKey", {"layer": layer, "path": path, "time": t, "timeBase": "comp", "value": v}))
    last = len(pts) - 1

    def ease(t, side, infl_):
        ex("keys.setEase", {"layer": layer, "path": path, "time": t, "side": side, "speed": 0, "influence": infl_})

    for i, (t, _) in enumerate(pts):
        if style == "io":
            if i: ease(t, "in", 70)
            if i < last: ease(t, "out", 70)
        elif style == "out":
            if i: ease(t, "in", infl)
            if 0 < i < last: ease(t, "out", 45)
        elif style == "in":
            if i < last: ease(t, "out", infl)
            if 0 < i < last: ease(t, "in", 45)


def effect(layer, name, **params):
    path = ok(ex("effect.apply", {"effect": name, "layers": [layer]}))["paths"][0]
    for k, v in params.items():
        ok(ex("prop.set", {"layer": layer, "path": f"{path}/{k}", "value": v}))
    return path


def mblur(*layers):
    ok(ex("layer.setSwitch", {"layers": list(layers), "switch": "motionBlur", "value": True}))


# ---------- dựng lớp ----------
def comp(name, dur, w=1080, h=1920):
    cid = ok(ex("comp.new", {"name": name, "width": w, "height": h, "frameRate": FPS, "duration": round(dur, 2),
                             "open": True, "shutterAngle": 200, "motionBlurSamples": 16}))["comp"]
    ok(ex("comp.setSwitch", {"switch": "motionBlur", "value": True}))
    return cid


def open_comp(cid):
    ok(ex("comp.open", {"comp": cid}))


def shape(name, kind, size, pos, fill=None, fill_op=100, stroke=None, sw=0, stroke_op=100, radius=None):
    p = {"kind": kind, "name": name, "size": size, "position": pos}
    if fill: p.update(fill=fill, fillOpacity=fill_op)
    else: p["fillType"] = "none"
    if stroke: p.update(stroke=stroke, strokeWidth=sw, strokeOpacity=stroke_op)
    else: p["strokeType"] = "none"
    ok(ex("layer.newShape", p))
    if radius is not None and kind == "rounded":
        ok(ex("prop.set", {"layer": name, "path": "contents/group/contents/rect/roundness", "value": radius}))
    return name


def arc(name, c, r, a0, a1, color=W, sw=4):
    """Cung tròn (độ, y hướng xuống) bằng 1 đoạn bezier (≤ 100°)."""
    t0, t1 = math.radians(a0), math.radians(a1)
    k = 4 / 3 * math.tan((t1 - t0) / 4) * r
    p0 = [c[0] + r * math.cos(t0), c[1] + r * math.sin(t0)]
    p1 = [c[0] + r * math.cos(t1), c[1] + r * math.sin(t1)]
    ok(ex("shape.newPath", {"name": name, "vertices": [p0, p1], "closed": False, "space": "comp", "fill": False,
                            "inTangents": [[0, 0], [k * math.sin(t1), -k * math.cos(t1)]],
                            "outTangents": [[-k * math.sin(t0), k * math.cos(t0)], [0, 0]],
                            "stroke": color, "strokeWidth": sw}))
    return name


def text(name, s, pos, size, style="Black", fill=W, tracking=None, justify="center", opacity=100):
    ok(ex("layer.newText", {"text": s, "name": name, "position": pos, "font": F, "style": style, "size": size,
                            "fill": fill, "justify": justify}))
    if tracking is not None:
        ok(ex("layer.setText", {"layer": name, "tracking": tracking}))
    if opacity != 100:
        ok(ex("prop.set", {"layer": name, "path": "transform/opacity", "value": opacity}))
    return name


def glow(layer, radius=22, intensity=0.55, threshold=45):
    """Glow vừa phải (không neon): cùng thông số cho chữ và nét."""
    return effect(layer, "Glow", threshold=threshold, radius=radius, intensity=intensity)


def ramp(name, start, end, w=1080, h=1920):
    """Solid gradient trắng→đen (toạ độ chuẩn hoá 0..1) dùng làm luma matte."""
    ok(ex("layer.newSolid", {"name": name, "color": "#000000", "width": w, "height": h}))
    effect(name, "Gradient Ramp", start=start, end=end, startColor=[1, 1, 1, 1], endColor=[0, 0, 0, 1])
    return name


def luma_matte(layer, matte):
    ok(ex("layer.setTrackMatte", {"layer": layer, "matte": matte, "kind": "luma"}))


def place(item, name, pos=None, index=None):
    """Đưa comp con vào comp đang mở, đổi tên layer cho dễ gọi."""
    p = {"item": item, "time": 0, "index": 1}
    if pos: p["position"] = pos
    ok(ex("layer.addItem", p))
    lid = [l["id"] for l in ok(ex("comp.info", {}))["layers"] if l["index"] == 1][0]
    ok(ex("layer.rename", {"layer": lid, "name": name}))
    return lid


# ---------- kính mờ ----------
def glass_panel(cid_name, size, radius=56, w=1080, h=1920, tint=13, grain=11):
    """Comp riêng chứa tấm kính tĩnh, tâm khung. Video bên dưới đã mờ + tối (FilmCraft làm), tấm này thêm:
    bóng mềm, tint trắng, hạt mịn, ánh sáng chéo, viền mảnh + mép trên sáng."""
    cid = comp(cid_name, 30, w, h)
    cx, cy = w / 2, h / 2
    shape("shadow", "rounded", size, [cx, cy + 26], fill="#000000", radius=radius)
    effect("shadow", "Gaussian Blur", blurriness=70, repeatEdge=False)
    ok(ex("prop.set", {"layer": "shadow", "path": "transform/opacity", "value": 32}))
    shape("tint", "rounded", size, [cx, cy], fill=W, fill_op=tint, radius=radius)
    shape("grain", "rounded", size, [cx, cy], fill="#808080", radius=radius)
    effect("grain", "Noise", amount=100, color=False)
    ok(ex("prop.set", {"layer": "grain", "path": "transform/opacity", "value": grain}))
    # ánh sáng chéo góc trên trái
    shape("sheen", "rounded", size, [cx, cy], fill=W, radius=radius)
    ramp("sheen_m", [0.15, 0.18], [0.62, 0.55])
    luma_matte("sheen", "sheen_m")
    ok(ex("prop.set", {"layer": "sheen", "path": "transform/opacity", "value": 9}))
    shape("edge", "rounded", size, [cx, cy], stroke=W, sw=2, stroke_op=30, radius=radius)
    shape("edge_hi", "rounded", size, [cx, cy], stroke=W, sw=2.5, stroke_op=95, radius=radius)
    top = (cy - size[1] / 2) / h
    ramp("edge_hi_m", [0.5, top - 0.01], [0.5, top + 0.16])
    luma_matte("edge_hi", "edge_hi_m")
    return cid


# ---------- chuyển động ----------
def _lerp(a, b, k):
    return [x + (y - x) * k for x, y in zip(a, b)] if isinstance(a, list) else a + (b - a) * k


def fly_in(layer, t, dur, pos=None, scale=None, rot=None, over=0.07, fade=0.12, blur=None):
    """Bay vào: (từ, tới) cho từng thuộc tính; vượt `over` rồi lắng. blur=(path_hiệu_ứng, từ) để nhoè → nét."""
    tl, ts = t + dur * 0.68, t + dur
    for path, ft in (("transform/position", pos), ("transform/scale", scale), ("transform/rotation", rot)):
        if ft:
            a, b = ft
            o = [y + (y - x) * over for x, y in zip(a, b)] if isinstance(a, list) else b + (b - a) * over
            anim(layer, path, [(t, a), (tl, o), (ts, b)], "out")
    anim(layer, "transform/opacity", [(t, 0), (t + fade, 100)], "io")
    if blur:
        anim(layer, f"{blur[0]}/blurriness", [(t, blur[1]), (tl, 0)], "out")


def fly_out(layer, t, dur, pos=None, scale=None, rot=None, blur=None, top=100):
    """Ra: khởi chậm rồi vút (khác kiểu vào), mờ dần ở 60 % cuối."""
    for path, ft in (("transform/position", pos), ("transform/scale", scale), ("transform/rotation", rot)):
        if ft:
            anim(layer, path, [(t, ft[0]), (t + dur, ft[1])], "in")
    anim(layer, "transform/opacity", [(t + dur * 0.35, top), (t + dur, 0)], "in")
    if blur:
        anim(layer, f"{blur[0]}/blurriness", [(t, 0), (t + dur, blur[1])], "in")


def float_idle(layer, t0, t1, amp=7, rot=1.2):
    """Lơ lửng nhẹ giữa hai mốc (cộng vào keyframe sẵn có), có vào/ra êm."""
    env = f"Math.min(1,Math.max(0,(time-{t0})/0.5))*Math.min(1,Math.max(0,({t1}-time)/0.3))"
    ok(ex("prop.setExpression", {"layer": layer, "path": "transform/position",
                                 "expression": f"var e={env}; value+[0, Math.sin((time-{t0})*2.4)*{amp}*e]"}))
    if rot:
        ok(ex("prop.setExpression", {"layer": layer, "path": "transform/rotation",
                                     "expression": f"var e={env}; value+Math.sin((time-{t0})*1.7)*{rot}*e"}))


def check(cid, times, out_dir, tag, max_side=720):
    os.makedirs(out_dir, exist_ok=True)
    paths = []
    for t in times:
        p = f"{out_dir}/{tag}_{t:.2f}.png"
        ok(call("render.frame", {"comp": cid, "time": round(t, 3), "path": p, "max_side": max_side}))
        paths.append(p)
    return paths


def preview(main_cid, bg_png, dur, name="pv"):
    """Comp xem thử: ảnh khung thật (đã mờ + tối sẵn như FilmCraft) + comp chính — render.frame không có alpha."""
    r = ok(ex("file.import", {"paths": [bg_png], "sequence": False}))
    it = (r.get("items") or r.get("imported") or [r])[0]
    it = it.get("id", it) if isinstance(it, dict) else it
    cid = comp(name, dur)
    place(it, "bg")
    place(main_cid, "motion")
    return cid
