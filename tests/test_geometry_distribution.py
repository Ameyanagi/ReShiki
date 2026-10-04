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


class GeometrySourceTests(unittest.TestCase):
    def test_checked_in_archives_pin_versions_contents_and_cmake_hashes(self):
        provenance = geometry_source.verify()
        self.assertEqual(provenance["version"], "2026.03.6")
        self.assertEqual(provenance["revision"], "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985")
        self.assertEqual(provenance["boost_version"], "1.92.0")
        self.assertEqual(
            set(provenance["archives"]),
            {"rdkit-geometry-2026.03.6.tar.gz", "boost-geometry-1.92.0.tar.gz"},
        )
        vendor = geometry_source.ROOT / geometry_source.VENDOR
        hashes = (vendor / "source-hashes.cmake").read_text()
        for digest in provenance["archives"].values():
            self.assertIn(digest, hashes)
        manifest_bytes = (vendor / "source-manifest.json").read_bytes()
        self.assertEqual(provenance["manifest_sha256"], hashlib.sha256(manifest_bytes).hexdigest())
        manifest = json.loads(manifest_bytes)
        self.assertEqual(set(manifest["rdkit"]["libraries"]), geometry_source.LIBRARIES)
        for record in manifest["archives"].values():
            self.assertTrue(record["files"])
            self.assertFalse(
                any(name.endswith((".dll", ".so", ".dylib", ".py")) for name in record["files"])
            )

    def test_changed_archive_fails_offline_without_replacing_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            vendor = root / geometry_source.VENDOR
            shutil.copytree(geometry_source.ROOT / geometry_source.VENDOR, vendor)
            source = vendor / "rdkit-geometry-2026.03.6.tar.gz"
            original = source.read_bytes()
            source.write_bytes(b"changed source archive")
            with self.assertRaisesRegex(ValueError, "Geometry source archive changed"):
                geometry_source.verify(root)
            self.assertEqual(source.read_bytes(), b"changed source archive")
            source.write_bytes(original)
            self.assertEqual(geometry_source.verify(root), geometry_source.metadata(root))


class GeometryWorkerTests(unittest.TestCase):
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
                        patch("build_release.run", side_effect=run),
                        self.assertRaises((ValueError, subprocess.SubprocessError)),
                    ):
                        build_release.verify_geometry_worker(binary, "2026.03.6")

    def test_import_audit_rejects_chemistry_and_windows_redistributable_libraries(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "reshiki"
            binary.touch()
            for system in ("Darwin", "Windows", "Linux"):
                for name in (
                    "system",
                    "RDKitGraphMol",
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
            self.skipTest("Optional independently built application")
        build_release.verify_geometry_worker(Path(binary), "2026.03.6")


if __name__ == "__main__":
    unittest.main()
