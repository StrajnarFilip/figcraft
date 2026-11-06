#!/usr/bin/env python3
"""Capture actual FigCraft windows on the private Xvfb display, using synthetic files."""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time

class App:
    def __init__(self, binary, out):
        self.out = Path(out).resolve()
        self.out.parent.mkdir(parents=True, exist_ok=True)
        self.config = tempfile.mkdtemp(prefix="artcraft-review-config-")
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        sock.close()
        env = dict(os.environ, DISPLAY=":87", WINIT_UNIX_BACKEND="x11", WAYLAND_DISPLAY="",
                   XDG_CONFIG_HOME=self.config, XDG_DATA_HOME=self.config + "/data",
                   HTTP_PROXY="http://127.0.0.1:9", HTTPS_PROXY="http://127.0.0.1:9",
                   http_proxy="http://127.0.0.1:9", https_proxy="http://127.0.0.1:9",
                   ALL_PROXY="http://127.0.0.1:9", NO_PROXY="localhost,127.0.0.1")
        self.log = open(str(self.out) + ".log", "w")
        self.proc = subprocess.Popen([str(binary), "--control", str(port)], env=env,
                                     stdout=self.log, stderr=self.log)
        self.conn = None
        for _ in range(100):
            if self.proc.poll() is not None:
                raise RuntimeError(f"App exited; see {self.out}.log")
            try:
                self.conn = socket.create_connection(("127.0.0.1", port), timeout=30)
                break
            except OSError:
                time.sleep(.2)
        if self.conn is None:
            raise RuntimeError("Control channel unavailable")
        self.file = self.conn.makefile("rw")
        self.rid = 0
        self.calls = []
        time.sleep(1)

    def call(self, method, params=None):
        self.rid += 1
        request = {"id": self.rid, "method": method, "params": params or {}}
        self.file.write(json.dumps(request) + "\n")
        self.file.flush()
        reply = json.loads(self.file.readline())
        self.calls.append({"request": request, "reply": reply})
        if reply.get("ok") is False or reply.get("error"):
            raise RuntimeError(str(reply))
        return reply.get("result", reply)

    def cmd(self, command, **params):
        return self.call("engine.execute", {"command": command, "params": params})

    def node(self, command, **params):
        result = self.cmd(command, **params)
        return result["id"]

    def input(self, *events):
        return self.call("ui.input", {"events": list(events)})

    def shot(self):
        time.sleep(.4)
        self.call("ui.screenshot", {"path": str(self.out)})

    def close(self):
        Path(str(self.out) + ".json").write_text(json.dumps(self.calls, indent=2))
        self.proc.terminate()
        try:
            self.proc.wait(5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        self.log.close()
        shutil.rmtree(self.config, ignore_errors=True)

def canvas(app):
    result = app.call("ui.inspect")
    print(json.dumps(result)[:4500], flush=True)
    return result

def run(binary, case, out):
    app = App(binary, out)
    try:
        app.cmd("file.new", name="Synthetic PR evidence")
        app.call("ui.set", {"home": False, "theme": "light"})
        if case == "fonts":
            for y, family, style, text in [
                (0, "SF Pro", "Bold", "SF Pro Bold: Hamburgefonts 012345"),
                (70, "Menlo", "Regular", "Menlo: iii WWW 0123456789"),
                (140, "Google Sans", "Bold", "Google Sans Bold: Missing fonts"),
            ]:
                app.node("text.create", text=text, x=0, y=y,
                         props={"fontFamily": family, "fontStyle": style, "fontSize": 27})
            app.cmd("select.none")
            app.cmd("view.set", zoom=1, cx=290, cy=100)
            app.call("ui.menu.invoke", {"id": "text.missingFonts"})
        elif case == "annotation":
            card = app.node("shape.rectangle", x=0, y=0, width=260, height=160,
                            props={"name": "Card to annotate", "fills": ["#DDEAFF"]})
            app.cmd("select.set", ids=[card])
            app.cmd("view.set", zoom=1, cx=130, cy=80)
            app.call("ui.set", {"rightTab": "design"})
            app.input({"key": "T", "shift": True})
            app.input({"text": "Use 16 px padding"})
        elif case in ["prototype", "canvas"]:
            origin = app.node("frame.create", x=0, y=0, width=200, height=150, name="START")
            target = app.node("frame.create", x=1100, y=0, width=200, height=150, name="OFF-SCREEN TARGET")
            if case == "prototype":
                app.cmd("select.set", ids=[origin])
                app.cmd("view.set", zoom=1, cx=100, cy=75)
                app.call("ui.set", {"rightTab": "prototype"})
            else:
                movable = app.node("shape.rectangle", x=40, y=220, width=100, height=80,
                                   props={"name": "Drag me", "fills": ["#1877F2"]})
                app.cmd("select.set", ids=[movable])
                app.cmd("view.set", zoom=1, cx=200, cy=150)
            time.sleep(.5)
            result = app.call("ui.inspect")
            Path(str(app.out) + ".inspect.json").write_text(json.dumps(result, indent=2))
            x0, y0, x1, y1 = result["canvas"]["rect"]
            center = [(x0 + x1) / 2, (y0 + y1) / 2]
            start = [center[0] + 111, center[1]] if case == "prototype" else [center[0] - 110, center[1] + 110]
            edge = [x1 - 4, start[1]]
            app.input({"move": start})
            app.input({"down": start})
            for k in range(1, 9):
                app.input({"move": [start[0] + (edge[0] - start[0]) * k / 8, start[1]]})
            deadline = time.monotonic() + 3.5
            view = app.cmd("view.get")
            while time.monotonic() < deadline and view["cx"] < 850:
                time.sleep(.08)
                view = app.cmd("view.get")
            if case == "prototype" and view["cx"] >= 850:
                # The off-screen frame is now reachable. Drop on its actual page-space centre.
                destination = [center[0] + 1200 - view["cx"], center[1] + 75 - view["cy"]]
                app.input({"move": destination})
                app.input({"up": destination})
            else:
                finish = [x1 - 90, start[1]]
                app.input({"move": finish})
                app.input({"up": finish})
            app.cmd("view.get")
            app.call("node.get", {"id": origin})
            app.call("ui.inspect")
        app.call("ui.inspect")
        app.shot()
    finally:
        app.close()

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary")
    parser.add_argument("case")
    parser.add_argument("out")
    args = parser.parse_args()
    run(args.binary, args.case, args.out)
