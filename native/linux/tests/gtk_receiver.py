"""Independent, focused GTK Wayland clipboard consumer/producer for private QA."""

import argparse
import hashlib
import json
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gdk, Gio, GLib, Gtk  # noqa: E402


def atomic_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


class Receiver:
    def __init__(self, directory):
        self.directory = directory
        self.seen = -1
        self.window = Gtk.Window(title="ReShiki Wayland receiver")
        self.window.set_default_size(420, 180)
        self.window.set_child(Gtk.Label(label="Independent Wayland clipboard receiver"))
        keyboard = Gtk.EventControllerKey.new()
        keyboard.connect("key-pressed", self.key_pressed)
        self.window.add_controller(keyboard)
        self.window.connect("map", self.mapped)
        self.window.present()
        self.clipboard = self.window.get_clipboard()
        self.loop = GLib.MainLoop()

    def mapped(self, window):
        clock = window.get_frame_clock()

        def painted(frame_clock):
            frame_clock.disconnect(handler)
            atomic_json(
                self.directory / "ready.json",
                {"backend": Gdk.Display.get_default().get_name()},
            )

        handler = clock.connect("after-paint", painted)

    def key_pressed(self, _controller, keyval, _keycode, _state):
        if keyval == Gdk.KEY_F12:
            self.poll()
            return True
        return False

    def poll(self):
        path = self.directory / "command.json"
        if not path.exists():
            return True
        command = json.loads(path.read_text())
        if command["id"] <= self.seen:
            return True
        self.seen = command["id"]
        result = {
            "id": self.seen,
            "formats": list(self.clipboard.get_formats().get_mime_types()),
            "active": self.window.is_active(),
        }
        if command["operation"] == "publish":
            providers = [
                Gdk.ContentProvider.new_for_bytes(mime, GLib.Bytes.new(Path(filename).read_bytes()))
                for mime, filename in command["items"].items()
            ]
            marker = command["marker"]
            providers.append(
                Gdk.ContentProvider.new_for_bytes(marker, GLib.Bytes.new(marker.encode()))
            )
            result["marker"] = marker
            result["queued"] = self.clipboard.set_content(Gdk.ContentProvider.new_union(providers))
            # This orders queued requests while the publisher remains focused.
            # Only the incoming selection marker and an independent read prove
            # acceptance; GTK local content and this sync are not receipts.
            self.window.get_display().sync()
            atomic_json(self.directory / "result.json", result)
        elif command["operation"] == "focus":
            atomic_json(self.directory / "result.json", result)
        else:
            self.read_next(command["mimes"], result)
        return True

    def read_next(self, mimes, result):
        if not mimes:
            atomic_json(self.directory / "result.json", result)
            return
        mime, *remaining = mimes

        def complete(clipboard, response):
            try:
                stream, actual = clipboard.read_finish(response)
            except GLib.Error as error:
                result[mime] = {"error": str(error)}
                self.read_next(remaining, result)
                return
            chunks = []

            def chunk_ready(source, response):
                try:
                    data = source.read_bytes_finish(response).get_data()
                    if data:
                        chunks.append(data)
                        if sum(map(len, chunks)) > 64 * 1024 * 1024:
                            raise ValueError("Receiver payload exceeds the clipboard budget")
                        source.read_bytes_async(65536, GLib.PRIORITY_DEFAULT, None, chunk_ready)
                        return
                    payload = b"".join(chunks)
                    name = f"received-{result['id']}-{mime.replace('/', '_')}.bin"
                    (self.directory / name).write_bytes(payload)
                    item = {
                        "mime": actual,
                        "file": str(self.directory / name),
                        "bytes": len(payload),
                        "sha256": hashlib.sha256(payload).hexdigest(),
                    }
                    if mime == "image/png":
                        texture = Gdk.Texture.new_from_bytes(GLib.Bytes.new(payload))
                        item["dimensions"] = [texture.get_width(), texture.get_height()]
                    result[mime] = item
                except (GLib.Error, ValueError) as error:
                    result[mime] = {"error": str(error)}
                source.close(None)
                self.read_next(remaining, result)

            stream.read_bytes_async(65536, GLib.PRIORITY_DEFAULT, None, chunk_ready)

        self.clipboard.read_async([mime], GLib.PRIORITY_DEFAULT, Gio.Cancellable(), complete)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    Receiver(args.directory).loop.run()


if __name__ == "__main__":
    main()
