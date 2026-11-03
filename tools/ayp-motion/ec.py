"""EffectCraft control channel (127.0.0.1:9880): ex(command, params), call(method, params)."""
import json, socket
_f = None; _id = 0
def call(method, params=None):
    global _f, _id
    if _f is None:
        s = socket.create_connection(("127.0.0.1", 9880)); s.settimeout(300); _f = s.makefile("rwb")
    _id += 1
    _f.write((json.dumps({"id": _id, "method": method, "params": params or {}}) + "\n").encode()); _f.flush()
    return json.loads(_f.readline())
def ex(command, params=None):
    return call("engine.execute", {"command": command, "params": params or {}})
