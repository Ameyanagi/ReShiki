"""Create native installers from the already verified application directory."""

import hashlib
import json
import os
import platform
import shutil
import sys
import tempfile
import time
from pathlib import Path

from build_release import ROOT, run, verify_binary, verify_inchi_helper
from check_runtime_dependencies import verify_payload, verify_runtime
from inchi_source import manifest


class InstallerCheckDirectory(tempfile.TemporaryDirectory):
    def cleanup(self):
        # Inno's clone deletes the original EXE after that process returns:
        # https://jrsoftware.org/ishelp/topic_uninstexitcodes.htm
        deadline = time.monotonic() + 30
        delay = 0.05
        while True:
            try:
                super().cleanup()
                return
            except OSError as error:
                if getattr(error, "winerror", None) not in {5, 32, 33}:
                    raise
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    error.add_note("Installer check cleanup remained locked after 30 seconds.")
                    raise
                time.sleep(min(delay, remaining))
                delay = min(delay * 2, 0.25)


def checksum(output):
    with output.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    Path(str(output) + ".sha256").write_text(
        f"{digest}  {output.name}\n", encoding="ascii", newline="\n"
    )


def inno_compiler():
    candidates = [os.environ.get("RESHIKI_ISCC"), shutil.which("ISCC")]
    for variable in ("ProgramFiles(x86)", "ProgramFiles"):
        if root := os.environ.get(variable):
            candidates.append(str(Path(root) / "Inno Setup 6/ISCC.exe"))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    raise ValueError("Install Inno Setup 6.7.3, or set RESHIKI_ISCC to ISCC.exe")


def windows_installer(folder, output_dir):
    metadata = json.loads((folder / "build.json").read_text(encoding="utf-8"))
    architecture = metadata["architecture"]
    if metadata["platform"] != "windows" or architecture not in {"x64", "arm64"}:
        raise ValueError("A Windows x64 or ARM64 build is required")
    verify_binary(folder / "reshiki.exe", "windows", architecture)
    name = folder.name + "-setup"
    output_dir.mkdir(parents=True, exist_ok=True)
    run(
        [
            inno_compiler(),
            f"/DSourceDir={folder.resolve()}",
            f"/DAppVersion={metadata['version']}",
            f"/DAppArchitecture={architecture}",
            f"/DOutputDir={output_dir.resolve()}",
            f"/DOutputName={name}",
            ROOT / "packaging/windows/reshiki.iss",
        ]
    )
    output = output_dir / f"{name}.exe"
    verify_windows_installer(output, folder)
    checksum(output)
    return output


def verify_windows_installer(installer, source):
    """Install, upgrade in place, run chemistry, and uninstall on a disposable CI runner."""
    if sys.platform != "win32":
        raise ValueError("Windows installer verification requires Windows")
    import winreg

    ole_key = r"Software\Classes\CLSID\{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}\LocalServer32"
    with InstallerCheckDirectory(prefix="ReShiki installer check ") as temporary:
        root = Path(temporary)
        destination = root / "Installed ReShiki"
        user_data = root / "User data"
        user_data.mkdir()
        sentinel = user_data / "keep.reshiki"
        sentinel.write_text("User drawing", encoding="utf-8")
        # Old user-owned caches and drawings must survive app-owned worker retirement.
        user_cache = root / "User cache/chemistry"
        user_cache.mkdir(parents=True)
        cache_sentinel = user_cache / "keep.txt"
        cache_sentinel.write_text("User cache", encoding="utf-8")
        flags = [
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/SP-",
            "/NOICONS",
            "/TASKS=",
            f"/DIR={destination}",
        ]
        try:
            verify_payload(source)
            for installation in range(2):
                if installation == 1:
                    legacy = destination / "chemistry/engine"
                    legacy.mkdir(parents=True)
                    (legacy / "worker.py").write_text("Legacy worker", encoding="utf-8")
                    (legacy.parent / "uv.lock").write_text("Legacy lock", encoding="utf-8")
                run([installer, *flags], timeout=180)
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, ole_key) as key:
                    command, _ = winreg.QueryValueEx(key, "")
                if command != f'"{destination / "reshiki.exe"}" --ole-server':
                    raise ValueError("Installed Office editor registration is incorrect")
                verify_payload(destination)
                # Every shipped byte must survive setup and upgrade.
                for original in source.rglob("*"):
                    if original.is_file():
                        installed = destination / original.relative_to(source)
                        if (
                            not installed.is_file()
                            or installed.read_bytes() != original.read_bytes()
                        ):
                            raise ValueError(f"Installed file mismatch: {original.name}")
            metadata = json.loads((source / "build.json").read_text(encoding="utf-8"))
            helper = destination / "reshiki-inchi-helper.exe"
            verify_binary(helper, "windows", metadata["architecture"])
            verify_inchi_helper(helper, manifest()["inchi_version"])
            verify_runtime(destination / "reshiki.exe", destination, user_data=user_data)
        finally:
            uninstaller = destination / "unins000.exe"
            if uninstaller.is_file():
                run([uninstaller, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"], timeout=180)
        if (destination / "reshiki.exe").exists():
            raise ValueError("Uninstaller left the application executable behind")
        if sentinel.read_text(encoding="utf-8") != "User drawing":
            raise ValueError("Uninstaller modified user data")
        if cache_sentinel.read_text(encoding="utf-8") != "User cache":
            raise ValueError("Installer or uninstaller modified the user chemistry cache")
    print("Windows installation, upgrade, chemistry, and uninstallation verified.")


def mac_disk_image(folder, output_dir, signed=False):
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / f"{folder.name}.dmg"
    with tempfile.TemporaryDirectory(prefix="ReShiki disk image ") as temporary:
        staging = Path(temporary) / "contents"
        staging.mkdir()
        run(["ditto", folder / "ReShiki.app", staging / "ReShiki.app"])
        (staging / "Applications").symlink_to("/Applications")
        (staging / "Install ReShiki.txt").write_text(
            "Drag ReShiki.app to Applications.\n\n"
            "Drawing and chemistry tools are included and work offline.\n"
            "https://reshiki.com/guide/install/\n",
            encoding="utf-8",
        )
        run(
            [
                "hdiutil",
                "create",
                "-volname",
                "ReShiki",
                "-srcfolder",
                staging,
                "-format",
                "UDZO",
                "-ov",
                output,
            ]
        )
    if signed:
        from sign_macos import sign_disk_image

        sign_disk_image(output)
    verify_mac_disk_image(output, signed)
    checksum(output)
    return output


def verify_mac_disk_image(output, signed):
    with tempfile.TemporaryDirectory(prefix="ReShiki mount check ") as temporary:
        root = Path(temporary)
        mount = root / "mounted"
        run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount, output])
        try:
            if (
                not (mount / "Applications").is_symlink()
                or os.readlink(mount / "Applications") != "/Applications"
            ):
                raise ValueError("Disk image has no Applications shortcut")
            installed = root / "Installed/ReShiki.app"
            run(["ditto", mount / "ReShiki.app", installed])
        finally:
            run(["hdiutil", "detach", mount])
        verify_binary(installed / "Contents/MacOS/reshiki", "macos", "arm64")
        helper = installed / "Contents/MacOS/reshiki-inchi-helper"
        verify_binary(helper, "macos", "arm64")
        verify_inchi_helper(helper, manifest()["inchi_version"])
        verify_runtime(installed / "Contents/MacOS/reshiki", installed)
        run(["codesign", "--verify", "--deep", "--strict", installed])
        if signed:
            from sign_macos import verify_app

            verify_app(installed)
    print("macOS disk image and drag-to-install copy verified.")


def build_installer(folder, output_dir, signed=False):
    if platform.system() == "Darwin":
        return mac_disk_image(folder, output_dir, signed)
    if platform.system() == "Windows":
        return windows_installer(folder, output_dir)
    return None
