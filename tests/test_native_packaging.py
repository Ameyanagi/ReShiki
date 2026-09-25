"""Native package verification must fail closed on legacy payloads and runtime setup."""

import contextlib
import json
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import build_release
import check_runtime_dependencies as runtime

ETHANOL = json.dumps({"analysis": {"formula": "C2H6O", "smiles": "CCO"}})


class NativeRuntimeTests(unittest.TestCase):
    def test_both_launches_disable_python_uv_checkout_and_external_helper(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)
            binary = package / "reshiki"
            binary.touch()
            calls = []

            def run(command, **kwargs):
                self.assertEqual(command, [str(binary.resolve()), "--engine-check"])
                self.assertTrue(kwargs["check"])
                self.assertEqual(kwargs["timeout"], 120)
                env = kwargs["env"]
                self.assertFalse(any(Path(env["PATH"]).iterdir()))
                for prefix in ("RESHIKI", "MORUNO"):
                    for key in ("PYTHON", "REFERENCE_PYTHON", "UV", "ROOT", "RUNTIME_DIR"):
                        self.assertFalse(Path(env[f"{prefix}_{key}"]).exists())
                for key in ("RESHIKI_INCHI_HELPER", "VIRTUAL_ENV", "PYTHONPATH", "PYTHONHOME"):
                    self.assertNotIn(key, env)
                self.assertNotEqual(kwargs["cwd"], package)
                self.assertEqual(env["UV_OFFLINE"], "1")
                calls.append(kwargs)
                return subprocess.CompletedProcess(command, 0, stdout=ETHANOL)

            with (
                patch.dict(os.environ, {"RESHIKI_INCHI_HELPER": "external", "VIRTUAL_ENV": "dev"}),
                patch("check_runtime_dependencies.subprocess.run", side_effect=run),
            ):
                runtime.verify_runtime(binary, package)
            self.assertEqual(len(calls), 2)
            self.assertEqual(calls[0]["env"], calls[1]["env"])
            self.assertFalse(Path(calls[0]["cwd"]).exists())

    def test_legacy_payloads_fail_before_process_creation_but_notices_remain(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)
            binary = package / "reshiki"
            binary.touch()
            notice = package / "Licenses/sources/rdkit/NOTICE"
            notice.parent.mkdir(parents=True)
            notice.write_text("Adapted algorithm attribution")
            runtime.verify_payload(package)
            for relative in (
                "chemistry/engine/worker.py",
                "ReShiki.app/Contents/Resources/chemistry/uv.lock",
                ".venv/pyvenv.cfg",
                "pyproject.toml",
                "python.exe",
                "lib/libpython3.12.so",
                "rdkit.libs/libRDKit.so",
            ):
                with self.subTest(relative=relative):
                    entry = package / relative
                    entry.parent.mkdir(parents=True, exist_ok=True)
                    entry.touch()
                    with patch("check_runtime_dependencies.subprocess.run") as run:
                        with self.assertRaisesRegex(ValueError, "Python chemistry payload"):
                            runtime.verify_runtime(binary, package)
                        run.assert_not_called()
                    entry.unlink()
                    parent = entry.parent
                    while parent != package:
                        parent.rmdir()
                        parent = parent.parent

    def test_runtime_errors_bad_answers_and_setup_side_effects_are_not_accepted(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)
            binary = package / "reshiki"
            binary.touch()
            for mode in ("exit", "timeout", "json", "wrong", "runtime", "uv", "python", "payload"):
                with self.subTest(mode=mode):

                    def run(command, **kwargs):
                        if mode == "exit":
                            raise subprocess.CalledProcessError(1, command, stderr="Python missing")
                        if mode == "timeout":
                            raise subprocess.TimeoutExpired(command, 120)
                        if mode == "runtime":
                            Path(kwargs["env"]["RESHIKI_RUNTIME_DIR"]).mkdir()
                        if mode == "uv":
                            Path(kwargs["env"]["UV_CACHE_DIR"]).mkdir()
                        if mode == "python":
                            (kwargs["cwd"] / "pyvenv.cfg").touch()
                        if mode == "payload":
                            (package / "worker.py").touch()
                        output = (
                            "invalid" if mode == "json" else "{}" if mode == "wrong" else ETHANOL
                        )
                        return subprocess.CompletedProcess(command, 0, stdout=output)

                    with patch("check_runtime_dependencies.subprocess.run", side_effect=run):
                        with self.assertRaises((ValueError, subprocess.SubprocessError)):
                            runtime.verify_runtime(binary, package)
                    (package / "worker.py").unlink(missing_ok=True)

    def test_checker_requires_an_executable_inside_the_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "package"
            package.mkdir()
            binary = root / "external"
            binary.touch()
            with self.assertRaisesRegex(ValueError, "must belong"):
                runtime.verify_runtime(binary, package)

    def test_extracted_archive_requires_native_runtime_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "package.zip"
            metadata = {
                "platform": "windows",
                "architecture": "arm64",
                "inchi_helper": {"version": "1.07.3"},
            }
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("package/build.json", json.dumps(metadata))
                stream.writestr("package/reshiki.exe", "binary")
                stream.writestr("package/reshiki-inchi-helper.exe", "helper")
            with (
                patch("build_release.platform.system", return_value="Windows"),
                patch("build_release.verify_binary"),
                patch("build_release.verify_inchi_helper") as helper,
                patch("build_release.verify_runtime") as check,
            ):
                build_release.verify_archive(archive)
            self.assertEqual(helper.call_args.args[0].name, "reshiki-inchi-helper.exe")
            self.assertEqual(check.call_args.args[0].parent, check.call_args.args[1])
            self.assertEqual(check.call_count, 1)

    def test_windows_upgrade_cleanup_is_scoped_to_the_app(self):
        script = (build_release.ROOT / "packaging/windows/reshiki.iss").read_text()
        section = script.split("[InstallDelete]\n", 1)[1].split("\n[", 1)[0]
        directives = [line for line in section.splitlines() if line and not line.startswith(";")]
        self.assertEqual(directives, ['Type: filesandordirs; Name: "{app}\\chemistry"'])

    def test_windows_verifier_detects_retained_workers_and_changed_user_sentinels(self):
        import installers

        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source"
            source.mkdir()
            for name in ("reshiki.exe", "reshiki-inchi-helper.exe"):
                (source / name).touch()
            (source / "build.json").write_text(json.dumps({"architecture": "arm64"}))
            for fault in ("retained_worker", "user_drawing", "user_cache"):
                with self.subTest(fault=fault):
                    installations = []
                    uninstalled = []

                    def run(command, **_kwargs):
                        executable = Path(command[0])
                        if executable.name == "setup.exe":
                            destination = Path(
                                next(
                                    arg.removeprefix("/DIR=")
                                    for arg in command
                                    if str(arg).startswith("/DIR=")
                                )
                            )
                            if installations:
                                if fault != "retained_worker":
                                    shutil.rmtree(destination / "chemistry")
                                if fault == "user_drawing":
                                    (destination.parent / "User data/keep.reshiki").write_text(
                                        "changed"
                                    )
                                if fault == "user_cache":
                                    (
                                        destination.parent / "User cache/chemistry/keep.txt"
                                    ).write_text("changed")
                            shutil.copytree(source, destination, dirs_exist_ok=True)
                            (destination / "unins000.exe").touch()
                            installations.append(destination)
                        elif executable.name == "unins000.exe":
                            uninstalled.append(executable)
                            shutil.rmtree(executable.parent)

                    registry = types.SimpleNamespace(
                        HKEY_CURRENT_USER=1,
                        OpenKey=lambda *_: contextlib.nullcontext(1),
                        QueryValueEx=lambda *_: (
                            f'"{installations[-1] / "reshiki.exe"}" --ole-server',
                            0,
                        ),
                    )
                    with (
                        patch("installers.sys.platform", "win32"),
                        patch.dict(sys.modules, {"winreg": registry}),
                        patch("installers.run", side_effect=run),
                        patch("installers.verify_binary"),
                        patch("installers.verify_inchi_helper"),
                        patch("installers.verify_runtime"),
                    ):
                        with self.assertRaises(ValueError):
                            installers.verify_windows_installer(Path("setup.exe"), source)
                    self.assertEqual(len(installations), 2)
                    self.assertEqual(len(uninstalled), 1)

    def test_reused_mac_worker_symlink_does_not_remove_its_external_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "target/debug/reshiki"
            binary.parent.mkdir(parents=True)
            binary.touch()
            app = root / "ReShiki.app"
            chemistry = app / "Contents/Resources/chemistry"
            chemistry.parent.mkdir(parents=True)
            external = root / "old-worker"
            external.mkdir()
            sentinel = external / "worker.py"
            sentinel.write_text("Keep external development files")
            try:
                chemistry.symlink_to(external, target_is_directory=True)
            except OSError:
                self.skipTest("Creating symlinks requires Windows developer mode")

            def run(command, **_kwargs):
                if command[0] == "swiftc":
                    Path(command[command.index("-o") + 1]).touch()

            with (
                patch("build_release.target_directory", return_value=root / "target"),
                patch("build_release.run", side_effect=run),
                patch("build_release.notices"),
            ):
                build_release.mac_bundle(app, "debug")
            with (app / "Contents/Info.plist").open("rb") as stream:
                info = plistlib.load(stream)
            self.assertEqual(info["CFBundleIconFile"], "ReShiki.icns")
            document_type = info["CFBundleDocumentTypes"][0]
            self.assertEqual(document_type["CFBundleTypeRole"], "Editor")
            self.assertEqual(document_type["LSItemContentTypes"], ["dev.reshiki.drawing"])
            self.assertEqual(document_type["CFBundleTypeExtensions"], ["rsk", "reshiki", "moruno"])
            exported_type = info["UTExportedTypeDeclarations"][0]
            self.assertEqual(exported_type["UTTypeIdentifier"], "dev.reshiki.drawing")
            self.assertEqual(exported_type["UTTypeConformsTo"], ["public.json"])
            self.assertEqual(
                exported_type["UTTypeTagSpecification"]["public.filename-extension"],
                document_type["CFBundleTypeExtensions"],
            )
            self.assertEqual(
                (app / "Contents/Resources/ReShiki.icns").read_bytes(),
                (build_release.ROOT / "assets/branding/reshiki.icns").read_bytes(),
            )
            self.assertFalse(chemistry.exists())
            self.assertEqual(sentinel.read_text(), "Keep external development files")


if __name__ == "__main__":
    unittest.main()
