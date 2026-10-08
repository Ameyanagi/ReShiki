"""Create native installers from the already verified application directory."""

import json
import os
import platform
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

from agent_api_client import StdioClient, result
from build_inchi_helper import INCHI_VERSION
from build_release import ROOT, checksum, numeric_version, run, verify_binary, verify_inchi_worker
from check_runtime_dependencies import (
    AGENT_CALL,
    verify_agent_api,
    verify_macos_workers,
    verify_payload,
    verify_runtime,
)

# An upgrade over a running MCP server that takes longer than this has hung.
UPGRADE_LIMIT = 300
UNINSTALL_KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.reshiki.editor_is1"


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


def inno_compiler():
    candidates = [os.environ.get("RESHIKI_ISCC"), shutil.which("ISCC")]
    for variable in ("ProgramFiles(x86)", "ProgramFiles"):
        if root := os.environ.get(variable):
            candidates.append(str(Path(root) / "Inno Setup 6/ISCC.exe"))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    raise ValueError("Install Inno Setup 6.7.3, or set RESHIKI_ISCC to ISCC.exe")


@dataclass
class _VersionInfoNode:
    key: str
    kind: int
    value: bytes
    children: list["_VersionInfoNode"]


def _read_version_info(data, start=0, limit=None):
    limit = len(data) if limit is None else limit
    if start + 6 > limit:
        raise ValueError("Truncated Windows version resource")
    length, value_length, kind = struct.unpack_from("<HHH", data, start)
    end = start + length
    if length < 6 or end > limit or kind not in {0, 1}:
        raise ValueError("Invalid Windows version resource node")
    key_end = start + 6
    while key_end + 2 <= end and data[key_end : key_end + 2] != b"\0\0":
        key_end += 2
    if key_end + 2 > end:
        raise ValueError("Unterminated Windows version resource key")
    key = data[start + 6 : key_end].decode("utf-16le")
    value_start = (key_end + 2 + 3) & ~3
    value_end = value_start + value_length * (2 if kind == 1 else 1)
    if value_end > end:
        raise ValueError("Invalid Windows version resource value length")
    children = []
    child_start = (value_end + 3) & ~3
    while child_start < end:
        child, child_end = _read_version_info(data, child_start, end)
        children.append(child)
        child_start = (child_end + 3) & ~3
    return _VersionInfoNode(key, kind, bytes(data[value_start:value_end]), children), end


def _write_version_info(node):
    data = bytearray(6) + (node.key + "\0").encode("utf-16le")
    data += bytes(-len(data) % 4)
    data += node.value
    for child in node.children:
        data += bytes(-len(data) % 4)
        data += _write_version_info(child)
    value_length = len(node.value) // 2 if node.kind == 1 else len(node.value)
    if len(data) > 65535 or value_length > 65535:
        raise ValueError("Windows version resource node exceeds its 16-bit length field")
    struct.pack_into("<HHH", data, 0, len(data), value_length, node.kind)
    return bytes(data)


def _windows_version_resource_locations(data):
    """Locate RT_VERSION data without loading the executable or moving its overlay."""

    def bounded(offset, size):
        if offset < 0 or size < 0 or offset + size > len(data):
            raise ValueError("Truncated Windows installer PE resource")
        return offset

    bounded(0, 64)
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    bounded(pe_offset, 24)
    if data[:2] != b"MZ" or data[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise ValueError("Windows installer must be a PE executable")
    sections_count = struct.unpack_from("<H", data, pe_offset + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe_offset + 20)[0]
    optional = bounded(pe_offset + 24, optional_size)
    magic = struct.unpack_from("<H", data, bounded(optional, 2))[0]
    directory_start = {0x10B: 96, 0x20B: 112}.get(magic)
    if directory_start is None or optional_size < directory_start + 40:
        raise ValueError("Unsupported Windows installer PE optional header")
    directory_count = struct.unpack_from("<I", data, optional + directory_start - 4)[0]
    if directory_count < 5:
        raise ValueError("Windows installer lacks PE resource/security directories")
    security = struct.unpack_from("<II", data, optional + directory_start + 32)
    if security != (0, 0):
        raise ValueError("Only an unsigned Windows installer can have its metadata normalized")
    resource_rva, resource_size = struct.unpack_from("<II", data, optional + directory_start + 16)
    sections = bounded(optional + optional_size, sections_count * 40)

    def file_offset(rva, size):
        for index in range(sections_count):
            section = sections + index * 40
            virtual_address, raw_size, raw_offset = struct.unpack_from("<III", data, section + 12)
            relative = rva - virtual_address
            if relative >= 0 and relative + size <= raw_size:
                return bounded(raw_offset + relative, size)
        raise ValueError("Windows version resource lies outside PE section data")

    resource_start = file_offset(resource_rva, resource_size)

    def resource_offset(offset, size):
        if offset < 0 or offset + size > resource_size:
            raise ValueError("Windows resource directory exceeds its allocation")
        return resource_start + offset

    def entries(offset):
        directory = resource_offset(offset, 16)
        named, numeric = struct.unpack_from("<HH", data, directory + 12)
        table = resource_offset(offset + 16, (named + numeric) * 8)
        return [
            struct.unpack_from("<II", data, table + index * 8) for index in range(named + numeric)
        ]

    locations = []
    for resource_type, names_offset in entries(0):
        if resource_type != 16:
            continue
        if not names_offset & 0x80000000:
            raise ValueError("Invalid RT_VERSION resource directory")
        for _name, languages_offset in entries(names_offset & 0x7FFFFFFF):
            if not languages_offset & 0x80000000:
                raise ValueError("Invalid Windows version language directory")
            for _language, value_offset in entries(languages_offset & 0x7FFFFFFF):
                if value_offset & 0x80000000:
                    raise ValueError("Invalid Windows version resource data entry")
                entry = resource_offset(value_offset, 16)
                value_rva, size = struct.unpack_from("<II", data, entry)
                location = file_offset(value_rva, size)
                if location < resource_start or location + size > resource_start + resource_size:
                    raise ValueError("Windows version data exceeds its resource allocation")
                locations.append((location, size))
    if not locations:
        raise ValueError("Windows installer has no version resource")
    ordered = sorted(locations)
    if any(left + size > right for (left, size), (right, _) in zip(ordered, ordered[1:])):
        raise ValueError("Windows version resource allocations overlap")
    return locations, optional + 64


def _pe_checksum(data, checksum_offset):
    even_size = len(data) & ~1
    if sys.byteorder == "little":
        total = sum(memoryview(data)[:even_size].cast("H"))
    else:
        total = sum(value[0] for value in struct.iter_unpack("<H", data[:even_size]))
    total -= sum(struct.unpack_from("<HH", data, checksum_offset))
    if len(data) % 2:
        total += data[-1]
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return total + len(data)


def normalize_windows_installer_metadata(installer, version):
    """Set exact identity strings inside Inno's existing unsigned resource allocation.

    Inno 6.7.3 overwrites space-filled strings without terminating shorter values,
    and its FileVersion placeholder truncates nightly identities to 20 characters.
    Rebuilding only RT_VERSION within its allocation fixes both while preserving
    the loader's RCDATA offset/CRC table and every byte of its appended payload.
    """
    data = bytearray(installer.read_bytes())
    locations, checksum_offset = _windows_version_resource_locations(data)
    major, minor, patch = map(int, numeric_version(version).split("."))
    numeric = (major << 16 | minor, patch << 16) * 2
    replacements = {"ProductName": "ReShiki", "ProductVersion": version, "FileVersion": version}
    for offset, size in locations:
        root, _end = _read_version_info(data[offset : offset + size])
        if root.key != "VS_VERSION_INFO" or root.kind != 0 or len(root.value) != 52:
            raise ValueError("Windows installer has an invalid fixed version resource")
        if struct.unpack_from("<I", root.value)[0] != 0xFEEF04BD:
            raise ValueError("Windows installer has an invalid fixed version signature")
        if struct.unpack_from("<IIII", root.value, 8) != numeric:
            raise ValueError("Windows installer numeric versions do not match the package")
        string_info = [child for child in root.children if child.key == "StringFileInfo"]
        if len(string_info) != 1 or not string_info[0].children:
            raise ValueError("Windows installer lacks version string tables")
        for table in string_info[0].children:
            for key, value in replacements.items():
                fields = [child for child in table.children if child.key == key]
                if len(fields) != 1 or fields[0].kind != 1:
                    raise ValueError(f"Windows installer lacks one text version field: {key}")
                fields[0].value = (value + "\0").encode("utf-16le")
        resource = _write_version_info(root)
        if len(resource) > size:
            raise ValueError("Exact Windows installer identity exceeds its resource allocation")
        data[offset : offset + size] = resource + bytes(size - len(resource))
    struct.pack_into("<I", data, checksum_offset, _pe_checksum(data, checksum_offset))
    installer.write_bytes(data)


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
            f"/DAppNumericVersion={numeric_version(metadata['version'])}",
            f"/DAppArchitecture={architecture}",
            f"/DOutputDir={output_dir.resolve()}",
            f"/DOutputName={name}",
            ROOT / "packaging/windows/reshiki.iss",
        ]
    )
    output = output_dir / f"{name}.exe"
    normalize_windows_installer_metadata(output, metadata["version"])
    verify_windows_installer(output, folder)
    verify_windows_upgrade_with_running_agent(output, folder)
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
                    (destination / "reshiki-inchi-helper.exe").write_bytes(b"retired helper")
                    for old_name in (
                        "rust/legacy/LICENSE",
                        "sources/legacy/NOTICE",
                        "rust-dependencies.json",
                    ):
                        old = destination / "Licenses" / old_name
                        old.parent.mkdir(parents=True, exist_ok=True)
                        old.write_text("Legacy license payload", encoding="utf-8")
                run([installer, *flags], timeout=180)
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, ole_key) as key:
                    command, _ = winreg.QueryValueEx(key, "")
                if command != f'"{destination / "reshiki.exe"}" --ole-server':
                    raise ValueError("Installed Office editor registration is incorrect")
                verify_payload(destination)
                for old_name in ("rust", "sources", "rust-dependencies.json"):
                    if (destination / "Licenses" / old_name).exists():
                        raise ValueError("Installer retained the legacy license tree")
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
            binary = destination / "reshiki.exe"
            verify_binary(binary, "windows", metadata["architecture"])
            verify_inchi_worker(binary, INCHI_VERSION)
            verify_runtime(destination / "reshiki.exe", destination, user_data=user_data)
            verify_agent_api(destination / "reshiki.exe", destination)
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


def installed_intact(destination, source):
    """Every shipped file is installed byte for byte."""
    return all(
        (destination / original.relative_to(source)).is_file()
        and (destination / original.relative_to(source)).read_bytes() == original.read_bytes()
        for original in source.rglob("*")
        if original.is_file()
    )


def _uninstall_registered():
    """Whether Windows lists the per-user install under Apps (Inno Setup's
    `{AppId}_is1` key; PrivilegesRequired=lowest in packaging/windows/reshiki.iss)."""
    if sys.platform != "win32":
        raise ValueError("Windows installer verification requires Windows")
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, UNINSTALL_KEY):
            return True
    except OSError:
        return False


def uninstall_state(destination):
    """Which of setup's uninstaller files exist, and whether Windows lists the app."""
    files = tuple((destination / name).is_file() for name in ("unins000.exe", "unins000.dat"))
    return files, _uninstall_registered()


def verify_windows_upgrade_with_running_agent(installer, source):
    """Upgrade over a running `reshiki.exe --mcp` and record what setup does.

    Setup closes running copies without restarting them (CloseApplications=yes,
    RestartApplications=no in packaging/windows/reshiki.iss). Either outcome
    passes: setup succeeds with a working new install and the old server
    ended, or setup fails nonzero and leaves the old install intact. Either
    way the uninstaller and its Apps entry must stay as the first install left
    them. Setup still running after UPGRADE_LIMIT seconds fails.
    """
    if sys.platform != "win32":
        raise ValueError("Windows installer verification requires Windows")
    with InstallerCheckDirectory(prefix="ReShiki agent upgrade check ") as temporary:
        root = Path(temporary)
        destination = root / "Installed ReShiki"
        binary = destination / "reshiki.exe"
        flags = [
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/SP-",
            "/NOICONS",
            "/TASKS=",
            f"/DIR={destination}",
        ]
        environment = dict(os.environ, RESHIKI_DATA_DIR=str(root / "User data"))
        server = None
        try:
            run([installer, *flags], timeout=180)
            uninstall = uninstall_state(destination)
            server = StdioClient([binary, "--mcp"], cwd=root, env=environment)
            result(server.discover(AGENT_CALL))
            try:
                run([installer, *flags], timeout=UPGRADE_LIMIT)
                code = 0
            except subprocess.CalledProcessError as error:
                code = error.returncode
            except subprocess.TimeoutExpired:
                raise ValueError(
                    f"Setup hung for {UPGRADE_LIMIT} s while an MCP server was running"
                ) from None
            try:
                server.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
            ended = server.process.poll() is not None
            if uninstall_state(destination) != uninstall:
                raise ValueError(f"Setup ({code}) over a running MCP server broke the uninstaller")
            if code == 0:
                if not ended:
                    raise ValueError("Setup succeeded while the old MCP server kept running")
                if not installed_intact(destination, source):
                    raise ValueError(
                        "Setup succeeded over a running MCP server but broke the install"
                    )
                run([binary, "--cli", "info"], capture_output=True, timeout=120)
                outcome = "setup closed the server and upgraded"
            else:
                if not installed_intact(destination, source):
                    raise ValueError(f"Setup failed ({code}) and damaged the old install")
                outcome = f"setup failed ({code}) and kept the old install"
        finally:
            if server is not None:
                server.kill()
            uninstaller = destination / "unins000.exe"
            if uninstaller.is_file():
                run([uninstaller, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"], timeout=180)
    print(f"Windows upgrade with a running MCP server: {outcome}.")
    return outcome


def mac_disk_image(folder, output_dir, signed=False):
    metadata = json.loads((folder / "build.json").read_text(encoding="utf-8"))
    architecture = metadata["architecture"]
    if metadata["platform"] != "macos" or architecture not in {"x64", "arm64"}:
        raise ValueError("A macOS x64 or ARM64 build is required")
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
    verify_mac_disk_image(output, signed, architecture=architecture)
    checksum(output)
    return output


def verify_mac_disk_image(output, signed, *, architecture):
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
        verify_binary(installed / "Contents/MacOS/reshiki", "macos", architecture)
        verify_inchi_worker(installed / "Contents/MacOS/reshiki", INCHI_VERSION)
        verify_runtime(installed / "Contents/MacOS/reshiki", installed)
        verify_macos_workers(installed / "Contents/MacOS/reshiki")
        verify_agent_api(installed / "Contents/MacOS/reshiki", installed)
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
