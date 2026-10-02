#!/usr/bin/env python3
"""Test save-new-document -> keep open -> edit -> save-back -> save/reopen.

Uses the installed extension, real renderer, and a compiled test editor
surrogate. This does not test desktop activation or human editor interaction.
Run only against an isolated, empty LibreOffice profile.
"""

import argparse
import json
import platform
import time
from pathlib import Path

import roundtrip as rt
import uno

e = rt.extension


def verify(document, kind, expected):
    objects = rt.objects_with_frames(document, kind)
    assert len(objects) == len(expected), "Incorrect object count"
    for (obj, frame), (native, png, extent) in zip(objects, expected):
        component = obj.getComponent()
        assert component.getTransferData(e.flavor(e.NATIVE_MIME)).value == native
        assert component.getTransferData(e.flavor("image/png")).value == png
        actual = obj.getVisualAreaSize(1)
        assert (actual.Width, actual.Height) == extent
        frame_size = (
            (frame.Width, frame.Height)
            if kind == "swriter"
            else (frame.Size.Width, frame.Size.Height)
        )
        assert all(abs(a - b) <= 3 for a, b in zip(frame_size, extent)), frame_size
    return [obj.getEntryName() for obj, _ in objects]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uno-url", required=True)
    parser.add_argument("--reshiki", type=Path, required=True)
    parser.add_argument("--editor-surrogate", type=Path, required=True)
    parser.add_argument("--drawing", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    program = args.reshiki.resolve()
    surrogate = args.editor_surrogate.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    raw = args.drawing.read_bytes()
    value = e.worker(str(program), "--libreoffice-preview", raw)
    original = e.packet(value)
    edited = json.loads(raw)
    edited["annotations"][0]["text"] = "confirmed save after host Save As with a larger preview"
    edited = json.dumps(edited).encode("utf-8")
    expected = e.packet(e.worker(str(program), "--libreoffice-preview", edited))
    assert expected[2][0] > original[2][0], "Fixture must increase the physical width"
    (surrogate.parent / "renderer-path.txt").write_text(str(program), encoding="utf-8")
    (surrogate.parent / "edited.rsk").write_bytes(edited)
    ctx = uno.getComponentContext()
    ctx = e.service(ctx, "com.sun.star.bridge.UnoUrlResolver").resolve(args.uno_url)
    manager = ctx.getValueByName("/singletons/com.sun.star.deployment.ExtensionManager")
    package = manager.getDeployedExtension("user", "dev.reshiki.libreoffice", "", None)
    url = package.getURL()
    if url.startswith("vnd.sun.star.expand:"):
        expander = ctx.getValueByName("/singletons/com.sun.star.util.theMacroExpander")
        url = expander.expandMacros(url.removeprefix("vnd.sun.star.expand:"))
    assert (
        Path(uno.fileUrlToSystemPath(url)) / "reshiki.py"
    ).read_bytes() == rt.SOURCE.read_bytes()
    desktop = e.service(ctx, "com.sun.star.frame.Desktop")
    assert not desktop.getComponents().hasElements(), "Use an empty isolated test profile"
    settings = e.settings_path(ctx)
    previous_settings = settings.read_bytes() if settings.exists() else None
    report = {
        "platform": platform.platform(),
        "extension_sha256": rt.digest(rt.SOURCE.read_bytes()),
        "renderer_executable_sha256": rt.digest(program.read_bytes()),
        "editor_surrogate_sha256": rt.digest(surrogate.read_bytes()),
        "sequence": "save new document, keep open, edit, accept, save, close, reopen",
        "test_editor_surrogate": True,
        "surrogate_retries_windows_atomic_replace": platform.system() == "Windows",
        "desktop_clipboard_or_activation_tested": False,
        "cases": [],
    }
    try:
        settings.write_text(json.dumps({"executable": str(surrogate)}), encoding="utf-8")
        for kind, suffix, filter_name in (
            ("swriter", "odt", "writer8"),
            ("scalc", "ods", "calc8"),
            ("simpress", "odp", "impress8"),
        ):
            document = desktop.loadComponentFromURL(
                "private:factory/" + kind, "_blank", 0, (e.prop("Hidden", True),)
            )
            e.insert(ctx, document, value)
            e.insert(ctx, document, value)
            destination = args.output.resolve() / ("saveback." + suffix)
            document.storeAsURL(
                uno.systemPathToFileUrl(str(destination)),
                (e.prop("FilterName", filter_name), e.prop("Overwrite", True)),
            )
            identities = verify(document, kind, (original, original))
            (args.output / ("before." + suffix)).write_bytes(destination.read_bytes())
            # Deliberately do not close/reopen here: that hid the old HandsOff bug.
            receipt_path = surrogate.parent / "accepted-receipt.json"
            receipt_path.unlink(missing_ok=True)
            first = rt.drawings(document, kind)[0]
            first.doVerb(0)
            deadline = time.monotonic() + 40
            while first.getCurrentState() == 4 and time.monotonic() < deadline:
                time.sleep(0.1)
            assert first.getCurrentState() == 1, (kind, "edit session did not finish")
            assert verify(document, kind, (expected, original)) == identities
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            assert receipt["accepted_sha256"] == rt.digest(edited)
            session_path = Path((surrogate.parent / "session-path.txt").read_text())
            assert not session_path.exists(), "Accepted session was not cleaned up"
            document.store()
            document.close(True)
            document = desktop.loadComponentFromURL(
                uno.systemPathToFileUrl(str(destination)),
                "_blank",
                0,
                (e.prop("Hidden", True),),
            )
            assert verify(document, kind, (expected, original)) == identities
            document.close(True)
            report["cases"].append(
                {
                    "format": suffix,
                    "passed": True,
                    "objects": 2,
                    "acknowledged_sha256": receipt["accepted_sha256"],
                    "unchanged_second_object": True,
                    "original_extent": original[2],
                    "updated_extent": expected[2],
                    "host_frame_size_checked": True,
                    "session_cleaned": True,
                }
            )
            print(kind, "PASS", flush=True)
    finally:
        if previous_settings is None:
            settings.unlink(missing_ok=True)
        else:
            settings.write_bytes(previous_settings)
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
