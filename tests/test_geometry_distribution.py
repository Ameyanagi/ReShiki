"""The pinned geometry core must ship inside the one native application."""

import copy
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import build_release
import geometry_source
from license_notices import THIRD_PARTY_FILE, write_notices


class GeometrySourceTests(unittest.TestCase):
    def test_rust_dependency_pin_and_parameter_reference_provenance(self):
        provenance = geometry_source.verify()
        self.assertEqual(provenance["version"], "2026.03.6")
        self.assertEqual(provenance["backend"]["name"], "cosmolkit-core")
        self.assertEqual(provenance["backend"]["version"], "0.3.0")
        self.assertRegex(provenance["revision"], "^[0-9a-f]{40}$")
        self.assertEqual(provenance["revision"], provenance["backend"]["revision"])
        self.assertEqual(provenance["implementation"], "Rust")
        self.assertEqual(provenance["runtime"], "self-process-rust-core")
        self.assertFalse(provenance["runtime_python"])
        self.assertFalse(provenance["runtime_downloads"])
        self.assertEqual(provenance["parameter_reference"]["version"], "2026.03.1")
        self.assertEqual(
            provenance["parameter_reference"]["revision"],
            "351f8f378f8ad6bbd517980c38896e66bf907af8",
        )
        self.assertEqual(provenance["comparison_reference"]["version"], "2026.03.6")
        self.assertEqual(
            provenance["comparison_reference"]["revision"],
            "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        )
        self.assertEqual(len(provenance["parameter_data"]["files_sha256"]), 7)
        self.assertNotIn("archives", provenance)
        self.assertNotIn("boost_version", provenance)
        for key, name in (
            ("cargo_manifest_sha256", geometry_source.GEOMETRY_MANIFEST),
            ("cargo_lock_sha256", Path("Cargo.lock")),
        ):
            self.assertEqual(
                provenance[key],
                hashlib.sha256((geometry_source.ROOT / name).read_bytes()).hexdigest(),
            )

    def test_changed_git_pin_fails_offline_without_replacing_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / geometry_source.GEOMETRY_MANIFEST
            manifest.parent.mkdir(parents=True)
            shutil.copy2(geometry_source.ROOT / geometry_source.GEOMETRY_MANIFEST, manifest)
            lock = root / "Cargo.lock"
            original = (geometry_source.ROOT / "Cargo.lock").read_text()
            revision = geometry_source.dependency()["revision"]
            changed = original.replace(revision, "0" * 40)
            self.assertNotEqual(changed, original)
            lock.write_text(changed)
            with self.assertRaisesRegex(ValueError, "locked COSMolKit revision"):
                geometry_source.verify(root)
            self.assertEqual(lock.read_text(), changed)
            lock.write_text(original)
            self.assertEqual(geometry_source.verify(root), geometry_source.metadata(root))
            (manifest.parent / "cpp").mkdir()
            with self.assertRaisesRegex(ValueError, "Obsolete native geometry build input"):
                geometry_source.verify(root)

    def test_resolved_parameter_bytes_and_source_revision_are_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            core = Path(temporary)
            source = core / "src/chemistry/forcefield/rdkit/ForceField/UFF/Params.cpp"
            source.parent.mkdir(parents=True)
            original = b"embedded parameter fixture"
            source.write_bytes(original)
            package = dict(
                name="cosmolkit-core",
                version="0.3.0",
                source=geometry_source.dependency()["source"],
                manifest_path=str(core / "Cargo.toml"),
            )
            with patch(
                "geometry_source.PARAMETER_SHA256",
                {
                    "ForceField/UFF/Params.cpp": hashlib.sha256(original).hexdigest(),
                },
            ):
                geometry_source.verify(cargo_metadata={"packages": [package]})
                source.write_bytes(b"changed parameter data")
                with self.assertRaisesRegex(ValueError, "Geometry parameter data changed"):
                    geometry_source.verify(cargo_metadata={"packages": [package]})
                self.assertEqual(source.read_bytes(), b"changed parameter data")
                package["source"] = "unreviewed source"
                with self.assertRaisesRegex(ValueError, "Resolved COSMolKit source"):
                    geometry_source.verify(cargo_metadata={"packages": [package]})

    def test_geometry_data_and_new_rust_crate_licenses_survive_aggregation(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "output"
            packages = []
            for name in ("cosmolkit-core", "cosmolkit-macros", "cosmolkit-ringdecomposer"):
                source = Path(temporary) / name
                source.mkdir()
                (source / "Cargo.toml").write_text(f'name = "{name}"\nlicense = "MIT"\n')
                packages.append(
                    dict(
                        name=name,
                        version="0.3.0",
                        license="MIT",
                        manifest_path=str(source / "Cargo.toml"),
                    )
                )
            write_notices(geometry_source.ROOT, output, {"packages": packages})
            content = (output / THIRD_PARTY_FILE).read_bytes()
            for name in ("NOTICE", "RDKIT-LICENSE"):
                self.assertIn(
                    (geometry_source.ROOT / "licenses/geometry" / name).read_bytes(), content
                )
            for package in packages:
                self.assertIn(f"DEPENDENCY: {package['name']}@0.3.0".encode(), content)
            self.assertIn(b"Copyright (c) 2026 COSMolKit Contributors", content)
            self.assertFalse((geometry_source.ROOT / "licenses/geometry/BOOST-LICENSE").exists())


class GeometryWorkerTests(unittest.TestCase):
    @staticmethod
    def signed_bundle(root):
        app = root / "ReShiki.app"
        binary = app / "Contents/MacOS/reshiki"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"\xcf\xfa\xed\xfeapplication fixture")
        for relative, content in (
            ("Contents/Info.plist", b"signed bundle metadata"),
            ("Contents/_CodeSignature/CodeResources", b"sealed resources"),
            ("Contents/Resources/icon.icns", b"icon fixture"),
            ("Contents/Resources/Licenses/RDKIT-LICENSE", b"parameter attribution"),
        ):
            entry = app / relative
            entry.parent.mkdir(parents=True, exist_ok=True)
            entry.write_bytes(content)
        return binary.resolve()

    @staticmethod
    def geometry(field="MMFF94"):
        return dict(
            coordinates=[
                [0.0, 0.0, 0.0],
                [1.5, 0.0, 0.0],
                [2.9, 0.0, 0.0],
                [-0.3, 1.0, 0.0],
                [-0.3, -0.5, 0.9],
                [-0.3, -0.5, -0.9],
                [1.5, 0.7, 0.8],
                [1.5, 0.7, -0.8],
                [3.4, 0.8, 0.0],
            ],
            hydrogen_parents=[0, 0, 0, 1, 1, 2],
            original_count=3,
            initial_energy=12.0,
            energy=-1.2,
            gradient=None,
            converged=True,
            field=field,
            diagnostics=["ETKDGv3; selection=lowest energy among valid generated conformers"],
        )

    @classmethod
    def reply(cls, field="MMFF94", *, geometry=None, version="2026.03.6", result=None):
        body = json.dumps(
            dict(version=version, result=result or {"Ok": geometry or cls.geometry(field)})
        ).encode()
        return b"RSHGEOM1" + struct.pack("<HHI", 1, 0, len(body)) + body

    def test_response_rejects_truncated_oversized_wrong_protocol_and_trailing_frames(self):
        response = self.reply()
        oversized_body = response[16:] + b" " * (4 * 1024 * 1024 + 1 - len(response))
        oversized_frame = response[:12] + struct.pack("<I", len(oversized_body)) + oversized_body
        invalid = [response[:n] for n in range(len(response))]
        invalid.extend(
            [
                b"BADMAGIC" + response[8:],
                response[:8] + struct.pack("<H", 2) + response[10:],
                response[:10] + b"\x01\x00" + response[12:],
                response + b"extra",
                oversized_frame,
                self.reply(version="wrong"),
                self.reply(field="UFF"),
                self.reply(result={"Err": "force-field parameters unavailable"}),
            ]
        )
        for value in invalid:
            with self.subTest(length=len(value)), self.assertRaises((ValueError, UnicodeError)):
                build_release.verify_geometry_response(value, "MMFF94", "2026.03.6")
        for field in ("MMFF94", "UFF"):
            build_release.verify_geometry_response(self.reply(field), field, "2026.03.6")

    def test_response_requires_finite_converged_chemical_geometry_and_matching_hydrogens(self):
        invalid = []
        for key, value in (
            ("coordinates", [[0.0, 0.0, 0.0]] * 9),
            ("coordinates", [[float("nan"), 0.0, 0.0]] * 9),
            ("coordinates", [[True, 0.0, 0.0]] * 9),
            ("coordinates", self.geometry()["coordinates"][:-1]),
            ("hydrogen_parents", [0, 0, 0, 1, 1, 1]),
            ("hydrogen_parents", [0, 0, 0, 1, 1, True]),
            ("original_count", 4),
            ("initial_energy", float("inf")),
            ("energy", 100.0),
            ("converged", False),
            ("gradient", [[0.0, 0.0, 0.0]] * 9),
        ):
            geometry = self.geometry()
            geometry[key] = value
            invalid.append(geometry)
        misplaced = copy.deepcopy(self.geometry())
        misplaced["coordinates"][3] = [20.0, 0.0, 0.0]
        invalid.append(misplaced)
        for geometry in invalid:
            with self.subTest(geometry=geometry), self.assertRaises(ValueError):
                build_release.verify_geometry_response(
                    self.reply(geometry=geometry), "MMFF94", "2026.03.6"
                )

    def test_both_fields_run_from_only_relocated_executable_with_empty_search_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "reshiki"
            binary.write_bytes(b"application fixture")
            calls = []

            def run(command, **kwargs):
                executable = Path(command[0])
                self.assertEqual(command[1:], ["--geometry-worker"])
                self.assertNotEqual(executable, binary)
                self.assertTrue(executable.is_absolute())
                self.assertEqual(executable.read_bytes(), binary.read_bytes())
                self.assertEqual(list(executable.parent.iterdir()), [executable])
                env = kwargs["env"]
                self.assertFalse(any(Path(env["PATH"]).iterdir()))
                self.assertNotEqual(kwargs["cwd"], binary.parent)
                self.assertEqual(kwargs["timeout"], 120)
                for key in (
                    "PYTHONPATH",
                    "PYTHONHOME",
                    "VIRTUAL_ENV",
                    "LD_LIBRARY_PATH",
                    "LD_PRELOAD",
                    "DYLD_LIBRARY_PATH",
                    "DYLD_INSERT_LIBRARIES",
                    "RDBASE",
                    "RESHIKI_GEOMETRY_HELPER",
                ):
                    self.assertNotIn(key, env)
                for prefix in ("RESHIKI", "MORUNO"):
                    for key in (
                        "PYTHON",
                        "REFERENCE_PYTHON",
                        "UV",
                        "ROOT",
                        "RUNTIME_DIR",
                        "DATA_DIR",
                    ):
                        self.assertFalse(Path(env[f"{prefix}_{key}"]).exists())
                request = kwargs["input"]
                self.assertEqual(request[:12], b"RSHGEOM1\x01\x00\x00\x00")
                self.assertEqual(struct.unpack("<I", request[12:16])[0], len(request) - 16)
                body = json.loads(request[16:])
                self.assertEqual(body["heap_bytes"], 256 * 1024 * 1024)
                operation = body["operation"]
                self.assertEqual(operation["operation"], "Generate")
                self.assertEqual([atom["atomic_number"] for atom in operation["atoms"]], [6, 6, 8])
                self.assertEqual(operation["coordinates"], [])
                calls.append((operation["field"], kwargs))
                return subprocess.CompletedProcess(
                    command, 0, stdout=self.reply(operation["field"])
                )

            with (
                patch.dict(
                    os.environ,
                    {
                        "PYTHONPATH": "checkout",
                        "PYTHONHOME": "interpreter",
                        "VIRTUAL_ENV": "dev",
                        "LD_LIBRARY_PATH": "rdkit",
                        "LD_PRELOAD": "shim",
                        "DYLD_LIBRARY_PATH": "rdkit",
                        "DYLD_INSERT_LIBRARIES": "shim",
                        "RDBASE": "source",
                        "RESHIKI_GEOMETRY_HELPER": "external",
                    },
                ),
                patch("build_release.verify_geometry_dependencies") as dependencies,
                patch("build_release.platform.system", return_value="Linux"),
                patch("build_release.run", side_effect=run),
            ):
                build_release.verify_geometry_worker(binary, "2026.03.6")
            dependencies.assert_called_once_with(binary.resolve())
            self.assertEqual([field for field, _ in calls], ["MMFF94", "UFF"])
            self.assertFalse(Path(calls[0][1]["cwd"]).exists())

    def test_worker_failure_and_runtime_payload_creation_fail_package_verification(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "reshiki"
            binary.touch()
            for failure in ("exit", "timeout", "payload", "neighbor", "path"):
                with self.subTest(failure=failure):

                    def run(command, **kwargs):
                        if failure == "exit":
                            raise subprocess.CalledProcessError(1, command)
                        if failure == "timeout":
                            raise subprocess.TimeoutExpired(command, 120)
                        if failure == "payload":
                            (kwargs["cwd"] / "python.exe").touch()
                        elif failure == "neighbor":
                            Path(command[0]).with_name("RDKit.dll").touch()
                        elif failure == "path":
                            (Path(kwargs["env"]["PATH"]) / "uv").touch()
                        field = json.loads(kwargs["input"][16:])["operation"]["field"]
                        return subprocess.CompletedProcess(command, 0, stdout=self.reply(field))

                    with (
                        patch("build_release.verify_geometry_dependencies"),
                        patch("build_release.platform.system", return_value="Linux"),
                        patch("build_release.run", side_effect=run),
                        self.assertRaises((ValueError, subprocess.SubprocessError)),
                    ):
                        build_release.verify_geometry_worker(binary, "2026.03.6")

    def test_signed_mac_bundle_relocation_keeps_metadata_bytes_and_both_isolated_probes(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = self.signed_bundle(Path(temporary))
            source_app = binary.parents[2]
            original = build_release.geometry_payload_inventory(source_app)
            signatures = []
            fields = []
            copies = []

            def run(command, **kwargs):
                if command[0] == "codesign":
                    self.assertEqual(command[1:4], ["--verify", "--deep", "--strict"])
                    app = Path(command[4])
                    self.assertEqual(build_release.geometry_payload_inventory(app), original)
                    signatures.append(app)
                    return subprocess.CompletedProcess(command, 0)
                if command[0] == "ditto":
                    self.assertEqual(Path(command[1]), source_app)
                    destination = Path(command[2])
                    self.assertNotEqual(destination, source_app)
                    shutil.copytree(source_app, destination, symlinks=True)
                    copies.append(destination)
                    return subprocess.CompletedProcess(command, 0)
                executable = Path(command[0])
                self.assertEqual(command[1:], ["--geometry-worker"])
                self.assertEqual(executable, copies[0] / "Contents/MacOS/reshiki")
                self.assertEqual(executable.read_bytes(), binary.read_bytes())
                self.assertEqual(list(executable.parent.iterdir()), [executable])
                self.assertEqual(list(copies[0].parent.iterdir()), [copies[0]])
                self.assertEqual(
                    sorted(kwargs["cwd"].iterdir()),
                    sorted([copies[0].parent, Path(kwargs["env"]["PATH"])]),
                )
                self.assertFalse(any(Path(kwargs["env"]["PATH"]).iterdir()))
                for key in (
                    "PYTHONPATH",
                    "PYTHONHOME",
                    "DYLD_LIBRARY_PATH",
                    "DYLD_INSERT_LIBRARIES",
                ):
                    self.assertNotIn(key, kwargs["env"])
                for prefix in ("RESHIKI", "MORUNO"):
                    for key in (
                        "PYTHON",
                        "REFERENCE_PYTHON",
                        "UV",
                        "ROOT",
                        "RUNTIME_DIR",
                        "DATA_DIR",
                    ):
                        self.assertFalse(Path(kwargs["env"][f"{prefix}_{key}"]).exists())
                field = json.loads(kwargs["input"][16:])["operation"]["field"]
                fields.append(field)
                return subprocess.CompletedProcess(command, 0, stdout=self.reply(field))

            with (
                patch("build_release.platform.system", return_value="Darwin"),
                patch("build_release.verify_geometry_dependencies"),
                patch("build_release.run", side_effect=run),
            ):
                build_release.verify_geometry_worker(binary, "2026.03.6", signed=True)
            self.assertEqual(fields, ["MMFF94", "UFF"])
            self.assertEqual(len(copies), 1)
            self.assertEqual(signatures, [source_app, copies[0], copies[0]])
            self.assertEqual(build_release.geometry_payload_inventory(source_app), original)
            self.assertFalse(copies[0].exists())

    def test_signed_mac_admission_rejects_detached_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "reshiki"
            binary.touch()
            with (
                patch("build_release.platform.system", return_value="Darwin"),
                patch("build_release.verify_geometry_dependencies"),
                patch("build_release.run") as run,
                self.assertRaisesRegex(ValueError, "standard .app bundle"),
            ):
                build_release.verify_geometry_worker(binary, "2026.03.6", signed=True)
            run.assert_not_called()

    def test_unsigned_mac_bundle_keeps_bare_executable_admission(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = self.signed_bundle(Path(temporary))
            fields = []

            def run(command, **kwargs):
                executable = Path(command[0])
                self.assertEqual(command[1:], ["--geometry-worker"])
                self.assertEqual(list(executable.parent.iterdir()), [executable])
                self.assertNotEqual(executable, binary)
                self.assertEqual(executable.read_bytes(), binary.read_bytes())
                field = json.loads(kwargs["input"][16:])["operation"]["field"]
                fields.append(field)
                return subprocess.CompletedProcess(command, 0, stdout=self.reply(field))

            with (
                patch("build_release.platform.system", return_value="Darwin"),
                patch("build_release.verify_geometry_dependencies"),
                patch("build_release.run", side_effect=run),
            ):
                build_release.verify_geometry_worker(binary, "2026.03.6")
            self.assertEqual(fields, ["MMFF94", "UFF"])

    def test_signed_mac_admission_rejects_side_executable_or_python_before_copying(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = self.signed_bundle(Path(temporary))
            for relative, content, message in (
                (
                    "Contents/MacOS/geometry-helper",
                    b"\xcf\xfa\xed\xfehelper",
                    "only the application executable",
                ),
                ("Contents/Resources/python.exe", b"interpreter", "Python chemistry payload"),
            ):
                with self.subTest(relative=relative):
                    payload = binary.parents[2] / relative
                    payload.write_bytes(content)
                    with (
                        patch("build_release.platform.system", return_value="Darwin"),
                        patch("build_release.verify_geometry_dependencies"),
                        patch("build_release.run") as run,
                        self.assertRaisesRegex(ValueError, message),
                    ):
                        build_release.verify_geometry_worker(binary, "2026.03.6", signed=True)
                    self.assertEqual(run.call_count, 1)
                    self.assertEqual(run.call_args.args[0][0], "codesign")
                    payload.unlink()

    def test_signed_mac_copy_must_preserve_all_bundle_bytes_and_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = self.signed_bundle(Path(temporary))
            for fault in ("executable", "metadata", "extra_payload"):
                with self.subTest(fault=fault):
                    calls = []

                    def run(command, **_kwargs):
                        calls.append(command)
                        if command[0] == "ditto":
                            destination = Path(command[2])
                            shutil.copytree(command[1], destination)
                            if fault == "executable":
                                (destination / "Contents/MacOS/reshiki").write_bytes(b"changed")
                            elif fault == "metadata":
                                (destination / "Contents/Info.plist").write_bytes(b"changed")
                            else:
                                (destination / "Contents/Resources/worker.py").touch()
                        return subprocess.CompletedProcess(command, 0)

                    with (
                        patch("build_release.platform.system", return_value="Darwin"),
                        patch("build_release.verify_geometry_dependencies"),
                        patch("build_release.run", side_effect=run),
                        self.assertRaisesRegex(ValueError, "bundle bytes or entries changed"),
                    ):
                        build_release.verify_geometry_worker(binary, "2026.03.6", signed=True)
                    self.assertEqual([command[0] for command in calls], ["codesign", "ditto"])

    def test_signed_mac_signature_failure_is_fatal_without_retry_or_resigning(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = self.signed_bundle(Path(temporary))
            for failed_verification in (1, 2, 3):
                with self.subTest(failed_verification=failed_verification):
                    signatures = 0
                    fields = []
                    failure = subprocess.CalledProcessError(1, ["codesign", "--verify"])

                    def run(command, **kwargs):
                        nonlocal signatures
                        if command[0] == "codesign":
                            self.assertEqual(command[1:4], ["--verify", "--deep", "--strict"])
                            signatures += 1
                            if signatures == failed_verification:
                                raise failure
                        elif command[0] == "ditto":
                            shutil.copytree(command[1], command[2])
                        else:
                            self.assertEqual(command[1:], ["--geometry-worker"])
                            field = json.loads(kwargs["input"][16:])["operation"]["field"]
                            fields.append(field)
                            return subprocess.CompletedProcess(command, 0, stdout=self.reply(field))
                        return subprocess.CompletedProcess(command, 0)

                    with (
                        patch("build_release.platform.system", return_value="Darwin"),
                        patch("build_release.verify_geometry_dependencies"),
                        patch("build_release.run", side_effect=run),
                        self.assertRaises(subprocess.CalledProcessError) as raised,
                    ):
                        build_release.verify_geometry_worker(binary, "2026.03.6", signed=True)
                    self.assertIs(raised.exception, failure)
                    self.assertEqual(signatures, failed_verification)
                    self.assertEqual(fields, ["MMFF94", "UFF"] if failed_verification == 3 else [])

    def test_signed_mac_workers_cannot_create_or_modify_relocated_payload(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = self.signed_bundle(Path(temporary))
            for fault in ("outside_bundle", "side_executable", "python", "metadata"):
                with self.subTest(fault=fault):

                    def run(command, **kwargs):
                        if command[0] == "ditto":
                            shutil.copytree(command[1], command[2])
                        elif command[0] != "codesign":
                            executable = Path(command[0])
                            app = executable.parents[2]
                            if fault == "outside_bundle":
                                (kwargs["cwd"] / "helper").write_bytes(b"\x7fELFhelper")
                            elif fault == "side_executable":
                                executable.with_name("helper").write_bytes(
                                    b"\xcf\xfa\xed\xfehelper"
                                )
                            elif fault == "python":
                                (app / "Contents/Resources/python.exe").touch()
                            else:
                                (app / "Contents/Info.plist").write_bytes(b"changed")
                            field = json.loads(kwargs["input"][16:])["operation"]["field"]
                            return subprocess.CompletedProcess(command, 0, stdout=self.reply(field))
                        return subprocess.CompletedProcess(command, 0)

                    with (
                        patch("build_release.platform.system", return_value="Darwin"),
                        patch("build_release.verify_geometry_dependencies"),
                        patch("build_release.run", side_effect=run),
                        self.assertRaisesRegex(ValueError, "unexpected runtime payload"),
                    ):
                        build_release.verify_geometry_worker(binary, "2026.03.6", signed=True)

    def test_import_audit_rejects_chemistry_and_windows_redistributable_libraries(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "reshiki"
            binary.touch()
            for system in ("Darwin", "Windows", "Linux"):
                for name in (
                    "system",
                    "RDKitGraphMol",
                    "cosmolkit_core",
                    "boost_thread",
                    "python312",
                    "vcruntime140",
                ):
                    if system == "Darwin":
                        output = (
                            f"{binary}:\n\t/usr/lib/lib{name}.dylib (compatibility version 1.0.0)\n"
                        )
                    elif system == "Windows":
                        output = f"Dump of file {binary}\n  Image has the following dependencies:\n\n    {name}.dll\n"
                    else:
                        output = f" 0x0000000000000001 (NEEDED) Shared library: [lib{name}.so]\n"
                    forbidden = name != "system" and (name != "vcruntime140" or system == "Windows")
                    with (
                        self.subTest(system=system, name=name),
                        patch("build_release.platform.system", return_value=system),
                        patch(
                            "build_release.run",
                            return_value=subprocess.CompletedProcess([], 0, stdout=output),
                        ),
                    ):
                        if forbidden:
                            with self.assertRaisesRegex(ValueError, "external runtime library"):
                                build_release.verify_geometry_dependencies(binary)
                        else:
                            build_release.verify_geometry_dependencies(binary)

    def test_packaged_application_worker_executes_when_supplied_for_validation(self):
        binary = os.environ.get("RESHIKI_TEST_PACKAGED_APP")
        if binary is None:
            if os.environ.get("RESHIKI_REQUIRE_GEOMETRY_APP") == "1":
                self.fail("Required geometry admission needs RESHIKI_TEST_PACKAGED_APP")
            self.skipTest("Optional independently built application")
        build_release.verify_geometry_worker(Path(binary), "2026.03.6")

    def test_required_release_admission_cannot_skip_a_missing_application(self):
        with patch.dict(os.environ, {"RESHIKI_REQUIRE_GEOMETRY_APP": "1"}, clear=True):
            with self.assertRaisesRegex(AssertionError, "Required geometry admission"):
                self.test_packaged_application_worker_executes_when_supplied_for_validation()


if __name__ == "__main__":
    unittest.main()
