#!/usr/bin/env python3
"""Check a real installed extension in an isolated headless LibreOffice profile.

Run with matching Python UNO bindings. This tests native ODF persistence and
transfer-data copy-back, not desktop clipboard selection or editor activation.
"""

import argparse
import hashlib
import importlib.util
import json
import platform
from pathlib import Path

import uno

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def objects_with_frames(document, kind):
    if kind == "swriter":
        values = document.EmbeddedObjects
        return [
            (values.getByIndex(i).getExtendedControlOverEmbeddedObject(), values.getByIndex(i))
            for i in range(values.getCount())
        ]
    page = (
        document.Sheets.getByIndex(0).DrawPage
        if kind == "scalc"
        else document.DrawPages.getByIndex(0)
    )
    result = []
    for i in range(page.getCount()):
        shape = page.getByIndex(i)
        if hasattr(shape, "EmbeddedObject") and shape.EmbeddedObject:
            result.append((shape.EmbeddedObject, shape))
    return result


def drawings(document, kind):
    return [obj for obj, _ in objects_with_frames(document, kind)]


def verify(document, kind, native, png, extent):
    objects = objects_with_frames(document, kind)
    if len(objects) != 2 or len({obj.getEntryName() for obj, _ in objects}) != 2:
        raise AssertionError("Two inserted objects must retain distinct identities.")
    for obj, frame in objects:
        component = obj.getComponent()
        if component.getTransferData(extension.flavor(extension.NATIVE_MIME)).value != native:
            raise AssertionError("Native data changed during persistence or copy-back.")
        if component.getTransferData(extension.flavor("image/png")).value != png:
            raise AssertionError("Preview bytes changed during persistence.")
        actual = obj.getVisualAreaSize(1)
        if (actual.Width, actual.Height) != tuple(extent):
            raise AssertionError("Intrinsic physical drawing extent changed during persistence.")
        frame_size = (
            (frame.Width, frame.Height)
            if kind == "swriter"
            else (frame.Size.Width, frame.Size.Height)
        )
        if any(abs(actual - expected) > 3 for actual, expected in zip(frame_size, extent)):
            raise AssertionError("Host frame does not preserve the drawing's physical size.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uno-url", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--packet", type=Path, help="Pre-rendered worker packet; no renderer verification"
    )
    parser.add_argument("--reshiki", type=Path)
    parser.add_argument("--drawing", type=Path)
    parser.add_argument(
        "--incoming", type=Path, help="Also verify and resave roundtrip-2.* files from another OS"
    )
    args = parser.parse_args()
    if args.packet:
        value = json.loads(args.packet.read_text(encoding="utf-8"))
    elif args.reshiki and args.drawing:
        value = extension.worker(
            str(args.reshiki.resolve()), "--libreoffice-preview", args.drawing.read_bytes()
        )
    else:
        parser.error("Provide --packet or both --reshiki and --drawing")
    native, png, extent = extension.packet(value)
    ctx = uno.getComponentContext()
    resolver = extension.service(ctx, "com.sun.star.bridge.UnoUrlResolver")
    ctx = resolver.resolve(args.uno_url)
    manager = ctx.getValueByName("/singletons/com.sun.star.deployment.ExtensionManager")
    package = manager.getDeployedExtension("user", "dev.reshiki.libreoffice", "", None)
    package_url = package.getURL()
    if package_url.startswith("vnd.sun.star.expand:"):
        expander = ctx.getValueByName("/singletons/com.sun.star.util.theMacroExpander")
        package_url = expander.expandMacros(package_url.removeprefix("vnd.sun.star.expand:"))
    installed = Path(uno.fileUrlToSystemPath(package_url)) / "reshiki.py"
    if installed.read_bytes() != SOURCE.read_bytes():
        raise AssertionError(
            "Installed extension differs from the source being tested. Reinstall and restart LibreOffice."
        )
    desktop = extension.service(ctx, "com.sun.star.frame.Desktop")
    args.output.mkdir(parents=True, exist_ok=True)
    report = {
        "platform": platform.platform(),
        "extension_sha256": digest(SOURCE.read_bytes()),
        "native_sha256": digest(native),
        "preview_sha256": digest(png),
        "extent": extent,
        "renderer_executable_sha256": digest(args.reshiki.read_bytes()) if args.reshiki else None,
        "desktop_clipboard_or_activation_tested": False,
        "host_frame_size_checked": True,
        "cases": [],
        "imported_cases": [],
    }
    for kind, suffix, filter_name in (
        ("swriter", "odt", "writer8"),
        ("scalc", "ods", "calc8"),
        ("simpress", "odp", "impress8"),
    ):
        document = desktop.loadComponentFromURL(
            "private:factory/" + kind, "_blank", 0, (extension.prop("Hidden", True),)
        )
        try:
            extension.insert(ctx, document, value)
            extension.insert(ctx, document, value)
            for pass_number in (1, 2):
                verify(document, kind, native, png, extent)
                path = args.output / ("roundtrip-" + str(pass_number) + "." + suffix)
                document.storeAsURL(
                    uno.systemPathToFileUrl(str(path.resolve())),
                    (extension.prop("FilterName", filter_name), extension.prop("Overwrite", True)),
                )
                document.close(True)
                document = desktop.loadComponentFromURL(
                    uno.systemPathToFileUrl(str(path.resolve())),
                    "_blank",
                    0,
                    (extension.prop("Hidden", True),),
                )
            verify(document, kind, native, png, extent)
            report["cases"].append(
                {"format": suffix, "objects": 2, "save_reopen_cycles": 2, "passed": True}
            )
        finally:
            document.close(True)
        if args.incoming:
            incoming = args.incoming / ("roundtrip-2." + suffix)
            document = desktop.loadComponentFromURL(
                uno.systemPathToFileUrl(str(incoming.resolve())),
                "_blank",
                0,
                (extension.prop("Hidden", True),),
            )
            try:
                verify(document, kind, native, png, extent)
                destination = args.output / ("imported." + suffix)
                document.storeAsURL(
                    uno.systemPathToFileUrl(str(destination.resolve())),
                    (extension.prop("FilterName", filter_name), extension.prop("Overwrite", True)),
                )
                document.close(True)
                document = desktop.loadComponentFromURL(
                    uno.systemPathToFileUrl(str(destination.resolve())),
                    "_blank",
                    0,
                    (extension.prop("Hidden", True),),
                )
                verify(document, kind, native, png, extent)
                report["imported_cases"].append({"format": suffix, "objects": 2, "passed": True})
            finally:
                document.close(True)
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
