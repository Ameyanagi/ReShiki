"""Exercise real ReShiki keyboard actions and independent Wayland transfers."""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

from gi.repository import GLib

NATIVE = "application/x-reshiki-drawing+json"
TEXT = "text/plain;charset=utf-8"
MIMES = [NATIVE, "image/png", "image/svg+xml", TEXT]
ROOT = Path(__file__).resolve().parents[3]


def wait_for(check, description, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"Timed out waiting for {description}")


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def graph(document):
    atoms = {atom["id"]: atom["element"] for atom in document["atoms"]}
    return {
        "atoms": sorted(atoms.values()),
        "bonds": sorted(
            (sorted([atoms[bond["a"]], atoms[bond["b"]]]), bond["order"])
            for bond in document["bonds"]
        ),
    }


class Session:
    def __init__(self, args):
        self.args = args
        self.out = args.output.resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.processes = []
        self.protocol_logs = {}
        self.logs = []
        self.env = os.environ.copy()
        runtime = self.out / "runtime"
        runtime.mkdir(mode=0o700)
        self.env.update(
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(self.out / "config"),
            XDG_CACHE_HOME=str(self.out / "cache"),
            XDG_DATA_HOME=str(self.out / "data"),
            LIBGL_ALWAYS_SOFTWARE="1",
        )
        self.env.pop("WAYLAND_DISPLAY", None)
        self.env.pop("RESHIKI_INCHI_HELPER", None)
        self.command_id = 0
        self.receipts = []
        self.gates = []
        self.binary_hash = hashlib.sha256(args.binary.read_bytes()).hexdigest()

    def start(self, name, command, env=None):
        log = (self.out / f"{name}.log").open("w")
        self.logs.append(log)
        process = subprocess.Popen(command, env=env or self.env, stdout=log, stderr=log)
        self.processes.append(process)
        self.protocol_logs[process.pid] = self.out / f"{name}.log"
        return process

    def run(self, command, env=None):
        result = subprocess.run(command, env=env or self.env, check=True, capture_output=True)
        with (self.out / "commands.log").open("ab") as log:
            log.write((repr(command) + "\n").encode() + result.stdout + result.stderr)
        return result.stdout.decode().strip()

    def probe(self, command):
        result = subprocess.run(command, env=self.env, capture_output=True)
        return result.stdout.decode().strip() if result.returncode == 0 else ""

    def compositor(self):
        if self.args.compositor == "gnome":
            display = self.out / "x-display"
            with display.open("w") as descriptor:
                log = (self.out / "xvfb.log").open("w")
                self.logs.append(log)
                process = subprocess.Popen(
                    [
                        "Xvfb",
                        "-displayfd",
                        str(descriptor.fileno()),
                        "-screen",
                        "0",
                        "1600x1000x24",
                        "-nolisten",
                        "tcp",
                    ],
                    pass_fds=[descriptor.fileno()],
                    stdout=log,
                    stderr=log,
                )
                self.processes.append(process)
            self.env["DISPLAY"] = ":" + wait_for(
                lambda: display.read_text().strip(), "Xvfb display"
            )
            self.env["MUTTER_DEBUG_DUMMY_MODE_SPECS"] = "1600x1000"
            self.shell = self.start(
                "compositor",
                [
                    "gnome-shell",
                    "--nested",
                    "--wayland",
                    "--no-x11",
                    "--unsafe-mode",
                    "--wayland-display=wayland-reshiki",
                ],
            )

            def stage_window():
                windows = self.probe(
                    ["xdotool", "search", "--pid", str(self.shell.pid)]
                ).splitlines()
                for window in windows:
                    geometry = dict(
                        line.split("=", 1)
                        for line in self.probe(
                            ["xdotool", "getwindowgeometry", "--shell", window]
                        ).splitlines()
                    )
                    if (
                        int(geometry.get("WIDTH", 0)) >= 1000
                        and int(geometry.get("HEIGHT", 0)) >= 680
                    ):
                        return window
                return None

            self.outer_window = wait_for(
                stage_window,
                "nested compositor window",
            )
            self.env["WAYLAND_DISPLAY"] = "wayland-reshiki"
        else:
            config = self.out / "sway.conf"
            config.write_text(
                "xwayland disable\noutput HEADLESS-1 mode 1600x1000\nseat seat0 fallback true\n"
            )
            self.env.update(
                WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman"
            )
            self.env.pop("DISPLAY", None)
            self.shell = self.start("compositor", ["sway", "-c", str(config), "-d"])
            socket = wait_for(
                lambda: next((self.out / "runtime").glob("sway-ipc.*.sock"), None), "Sway IPC"
            )
            self.env["SWAYSOCK"] = str(socket)
            self.env["WAYLAND_DISPLAY"] = wait_for(
                lambda: next(
                    (
                        path.name
                        for path in (self.out / "runtime").glob("wayland-*")
                        if not path.name.endswith(".lock")
                    ),
                    None,
                ),
                "Sway Wayland socket",
            )
        wait_for(
            lambda: (self.out / "runtime" / self.env["WAYLAND_DISPLAY"]).exists(),
            "Wayland compositor",
        )
        self.client_env = self.env.copy()
        self.client_env.pop("DISPLAY", None)
        self.client_env.update(
            GDK_BACKEND="wayland", WAYLAND_DEBUG="1", RESHIKI_DATA_DIR=str(self.out / "app-data")
        )
        if self.args.compositor == "sway":
            # Keep the headless seat's keyboard capability alive between keys.
            # A short-lived virtual keyboard otherwise disappears before Iced
            # can bind it, or before asynchronous clipboard preparation finishes.
            self.start("keyboard", ["wtype", "-s", "600000"])

    def focus(self, process):
        if self.args.compositor == "sway":
            wait_for(
                lambda: '"success": true' in self.probe(["swaymsg", f"[pid={process.pid}] focus"]),
                "Sway window focus",
            )
        else:
            expression = f"(() => {{ Main.overview.hide(); if (Main.overview.visible) return false; const a = global.get_window_actors().find(a => a.meta_window.get_pid() === {process.pid}); if (!a || !a.is_mapped() || !a.is_visible() || a.meta_window.is_hidden()) return false; const r = a.meta_window.get_frame_rect(); if (r.width <= 0 || r.height <= 0) return false; a.meta_window.activate(global.get_current_time()); return global.display.focus_window === a.meta_window; }})()"

            def focused():
                output = self.probe(
                    [
                        "gdbus",
                        "call",
                        "--session",
                        "--dest",
                        "org.gnome.Shell",
                        "--object-path",
                        "/org/gnome/Shell",
                        "--method",
                        "org.gnome.Shell.Eval",
                        expression,
                    ]
                )
                if not output:
                    return False
                success, value = GLib.Variant.parse(
                    GLib.VariantType.new("(bs)"), output, None, None
                ).unpack()
                return success and json.loads(value) is True

            wait_for(
                focused,
                "GNOME window focus",
            )
            self.run(["xdotool", "windowfocus", "--sync", self.outer_window])

        def keyboard_focused():
            events = re.findall(
                r"wl_keyboard@\d+\.(enter|leave)\(",
                self.protocol_logs[process.pid].read_text(),
            )
            return events and events[-1] == "enter"

        wait_for(keyboard_focused, "actual Wayland keyboard focus")

    def key(self, key):
        if self.args.compositor == "gnome":
            self.run(["xdotool", "key", "--clearmodifiers", key])
        else:
            parts = key.split("+")
            modifiers = parts[:-1]
            self.run(
                [
                    "wtype",
                    "-s",
                    "100",
                    *[arg for mod in modifiers for arg in ("-M", mod)],
                    "-k",
                    parts[-1],
                    *[arg for mod in reversed(modifiers) for arg in ("-m", mod)],
                    "-s",
                    "100",
                ]
            )

    def state(self, event=None, after=-1):
        def ready():
            paths = (
                sorted(self.app_dir.glob("state-*.json"), reverse=True)
                if event
                else [self.app_dir / "state.json"]
            )
            for path in paths:
                state = read_json(path)
                if state.get("sequence", -1) > after and (
                    event is None or state.get("event") == event
                ):
                    return state
            return None

        return wait_for(ready, f"application {event or 'state'}")

    def app_key(self, key, event):
        before = self.state()["sequence"]
        self.focus(self.app)
        self.key(key)
        state = self.state(event, before)
        assert not state["error"], state
        return state

    def receiver(self, name):
        self.receiver_dir = self.out / name
        self.receiver_dir.mkdir()
        self.consumer = self.start(
            name,
            [
                "/usr/bin/python3",
                str(ROOT / "native/linux/tests/gtk_receiver.py"),
                str(self.receiver_dir),
            ],
            self.client_env,
        )
        wait_for(lambda: (self.receiver_dir / "ready.json").exists(), "GTK receiver")
        self.focus(self.consumer)

    def clipboard(self, operation="read", items=None):
        self.focus(self.consumer)
        self.command_id += 1
        command = {"id": self.command_id, "operation": operation, "mimes": MIMES, "items": items}
        temporary = self.receiver_dir / "command.tmp"
        temporary.write_text(json.dumps(command))
        temporary.replace(self.receiver_dir / "command.json")
        self.key("F12")
        result = wait_for(
            lambda: (
                value
                if (value := read_json(self.receiver_dir / "result.json")).get("id")
                == self.command_id
                else None
            ),
            "independent clipboard transfer",
        )
        assert result["active"], result
        self.receipts.append(result)
        return result

    def save(self, name):
        self.app_key("ctrl+s", "saved")
        shutil.copyfile(self.fixture, self.out / name)
        return json.loads(self.fixture.read_text())

    def begin_gated_cut(self, phase):
        for suffix in ("entered", "release"):
            (self.app_dir / f"gate-{phase}.{suffix}").unlink(missing_ok=True)
        (self.app_dir / f"gate-{phase}.arm").touch()
        before = self.state()["sequence"]
        self.focus(self.app)
        self.key("ctrl+x")
        gate = self.app_dir / f"gate-{phase}.entered"
        marker = wait_for(
            lambda: (
                lines
                if gate.exists() and len(lines := gate.read_text().splitlines()) >= 2
                else None
            ),
            f"real transport {phase} waitpoint",
            timeout=5,
        )
        state = self.state()
        assert state["busy"] and state["atoms"] == 3, state
        if phase == "receipt":
            assert marker[1].startswith("application/x-reshiki-clipboard-receipt-"), marker
        self.gates.append(
            {
                "phase": phase,
                "matched_marker": marker[1] if len(marker) > 1 else None,
                "pending_state": state,
            }
        )
        return before

    def release_gate(self, phase):
        (self.app_dir / f"gate-{phase}.release").touch()

    def cancelled_cut(self, replace=None):
        before = self.begin_gated_cut("receipt")
        (self.app_dir / "cancel-write").touch()
        state = self.state("clipboard_written", before)
        assert state["error"] and "cancelled by QA" in state["status"], state
        assert state["atoms"] == 3
        if replace:
            self.clipboard("publish", replace)
            self.receiver("replacement-receiver")
            # An independent process proves B is published by the compositor;
            # this does not read the GTK publisher's cached local content.
            replaced = self.clipboard()
            assert (
                replaced[NATIVE]["sha256"]
                == hashlib.sha256(Path(replace[NATIVE]).read_bytes()).hexdigest()
            )
        self.release_gate("receipt")
        return self.clipboard()

    def exercise(self):
        self.compositor()
        self.app_dir = self.out / "app"
        self.app_dir.mkdir()
        self.fixture = self.out / "drawing.rsk"
        shutil.copyfile(ROOT / "tests/fixtures/ui-ethanol.reshiki", self.fixture)
        env = self.client_env | {"RESHIKI_WAYLAND_QA_DIR": str(self.app_dir)}
        self.app = self.start(
            "app", [str(self.args.binary.resolve()), "--open", str(self.fixture)], env
        )
        assert self.state("opened")["atoms"] == 3
        self.app_key("ctrl+a", "selected")
        self.app_key("ctrl+c", "clipboard_written")
        self.receiver("receiver")
        copied = self.clipboard()
        original = json.loads(Path(copied[NATIVE]["file"]).read_text())
        assert graph(original)["atoms"] == ["C", "C", "O"]
        assert len(original["bonds"]) == 2
        assert copied["image/png"]["dimensions"][0] > 0
        assert "error" not in copied["image/svg+xml"], copied
        assert Path(copied[TEXT]["file"]).read_text() == "CCO"

        sentinel = self.out / "sentinel.txt"
        sentinel.write_text("foreign clipboard owner\n")
        self.clipboard("publish", {"text/plain;charset=utf-8": str(sentinel)})
        self.app_key("ctrl+c", "clipboard_written")
        replaced = self.clipboard()
        assert replaced[NATIVE]["sha256"] == copied[NATIVE]["sha256"]

        foreign = self.out / "foreign-native.json"
        foreign_document = json.loads(Path(copied[NATIVE]["file"]).read_text())
        foreign_document["atoms"][2]["charge"] = 1
        foreign.write_text(json.dumps(foreign_document))
        foreign_items = {NATIVE: str(foreign), "image/png": copied["image/png"]["file"]}
        self.clipboard("publish", foreign_items)
        before = self.begin_gated_cut("request")
        self.clipboard("focus")  # Real receiver F12 arrives after keyboard focus.
        self.release_gate("request")
        failed = self.state("clipboard_written", before)
        assert failed["error"] and failed["atoms"] == 3, failed
        assert "focus" in failed["status"], failed
        assert graph(self.save("failed-cut.rsk")) == graph(original)
        preserved = self.clipboard()
        assert preserved[NATIVE]["sha256"] == hashlib.sha256(foreign.read_bytes()).hexdigest()

        retained = self.cancelled_cut()
        assert retained[NATIVE]["sha256"] == copied[NATIVE]["sha256"]
        assert retained["image/png"]["sha256"] == copied["image/png"]["sha256"]
        preserved = self.cancelled_cut(foreign_items)
        assert preserved[NATIVE]["sha256"] == hashlib.sha256(foreign.read_bytes()).hexdigest()
        assert preserved["image/png"]["sha256"] == copied["image/png"]["sha256"]

        before = self.begin_gated_cut("receipt")
        assert graph(self.save("before-receipt.rsk")) == graph(original)
        assert self.state()["busy"]  # No early publication acknowledgement.
        self.release_gate("receipt")
        written = self.state("clipboard_written", before)
        assert not written["error"] and written["atoms"] == 0, written
        assert not self.save("cut.rsk")["atoms"]
        cut = self.clipboard()
        assert cut[NATIVE]["sha256"] == copied[NATIVE]["sha256"]
        assert self.app_key("ctrl+z", "undo")["atoms"] == 3
        assert graph(self.save("undo.rsk")) == graph(original)

        self.clipboard(
            "publish", {NATIVE: copied[NATIVE]["file"], "image/png": copied["image/png"]["file"]}
        )
        assert self.app_key("ctrl+v", "clipboard_read")["atoms"] == 6
        pasted = self.save("pasted.rsk")
        assert len(pasted["bonds"]) == 4
        assert graph(pasted)["atoms"] == ["C", "C", "C", "C", "O", "O"]
        self.app_key("ctrl+shift+c", "clipboard_written")
        picture = self.clipboard()
        native_picture = json.loads(Path(picture[NATIVE]["file"]).read_text())
        assert not native_picture["atoms"] and native_picture["graphics"]
        assert picture["image/png"]["dimensions"][0] > 0

        self.app_key("ctrl+a", "selected")
        self.app_key("ctrl+c", "clipboard_written")
        final_copy = self.clipboard()
        for mime in MIMES:
            assert "error" not in final_copy[mime], final_copy
        self.save("before-exit.rsk")
        if self.args.compositor == "sway":
            self.run(["swaymsg", f"[pid={self.app.pid}] kill"])
        else:
            expression = f"global.get_window_actors().find(a => a.meta_window.get_pid() === {self.app.pid}).meta_window.delete(global.get_current_time())"
            self.run(
                [
                    "gdbus",
                    "call",
                    "--session",
                    "--dest",
                    "org.gnome.Shell",
                    "--object-path",
                    "/org/gnome/Shell",
                    "--method",
                    "org.gnome.Shell.Eval",
                    expression,
                ]
            )
        self.app.wait(timeout=30)
        assert self.app.returncode == 0
        self.consumer.terminate()
        self.consumer.wait(timeout=5)
        self.receiver("fresh-receiver")
        after_exit = self.clipboard()
        assert "error" in after_exit[NATIVE]
        assert not any(
            mime in (NATIVE, "dev.reshiki.drawing")
            or mime.startswith("application/x-reshiki-clipboard-receipt-")
            for mime in after_exit["formats"]
        )
        if self.args.compositor == "sway":
            assert not after_exit["formats"], after_exit
        else:
            preserved = [mime for mime in MIMES[1:] if "error" not in after_exit[mime]]
            assert preserved, after_exit
            for mime in preserved:
                assert after_exit[mime]["sha256"] == final_copy[mime]["sha256"]

        protocol = (self.out / "app.log").read_text()
        assert "wl_keyboard" in protocol and "set_selection" in protocol
        assert "application/x-reshiki" in protocol
        assert "wl_data_device" in protocol and "selection(" in protocol
        serials = list(
            map(int, re.findall(r"set_selection\(wl_data_source@\d+, (\d+)\)", protocol))
        )
        assert serials and all(serial > 0 for serial in serials), serials
        report = {
            "compositor": self.args.compositor,
            "source_head": os.environ.get("RESHIKI_QA_SOURCE_HEAD", "unrecorded"),
            "binary_sha256": self.binary_hash,
            "source_sha256": {
                name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
                for name in (
                    "Cargo.toml",
                    "Cargo.lock",
                    "src/app.rs",
                    "src/app/wayland_qa.rs",
                    "native/linux/Cargo.toml",
                    "native/linux/src/gui.rs",
                    "native/linux/src/wayland_qa.rs",
                    "native/linux/tests/gtk_receiver.py",
                    "native/linux/tests/wayland_session.py",
                    "vendor/smithay-clipboard/Cargo.toml",
                    "vendor/smithay-clipboard/src/qa.rs",
                    "vendor/smithay-clipboard/src/state/binary.rs",
                    "tests/fixtures/ui-ethanol.reshiki",
                )
            },
            "packages": self.run(
                [
                    "dpkg-query",
                    "-W",
                    "gnome-shell",
                    "mutter-common",
                    "libmutter-14-0",
                    "sway",
                    "libwlroots12t64",
                    "libgtk-4-1",
                    "libwayland-client0",
                    "mesa-vulkan-drivers",
                ]
            ),
            "receipts": self.receipts,
            "gates": self.gates,
            "publication_serials": serials,
            "checks": [
                "real_keyboard_copy",
                "foreign_owner_replacement",
                "focus_loss_cut_preserved",
                "cancelled_source_retained",
                "cancelled_cleanup_preserves_foreign_owner",
                "cut_waits_for_real_receipt",
                "cut_saved_graph",
                "undo_saved_graph",
                "native_paste_saved_graph",
                "copy_image_png",
                "fresh_receiver_after_exit",
            ],
        }
        (self.out / "manifest.json").write_text(json.dumps(report, indent=2) + "\n")
        print(
            f"{self.args.compositor}: {len(report['checks'])} real compositor checks passed",
            flush=True,
        )

    def close(self):
        for process in reversed(self.processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for log in self.logs:
            log.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("compositor", choices=["gnome", "sway"])
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    session = Session(parser.parse_args())
    try:
        session.exercise()
    finally:
        session.close()


if __name__ == "__main__":
    main()
