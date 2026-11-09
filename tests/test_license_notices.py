"""Consolidated distribution: complete texts, attribution, deduplication and upgrades."""

import hashlib
import json
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from license_notices import PROJECT_FILES, THIRD_PARTY_FILE, write_notices


class LicenseNoticeTests(unittest.TestCase):
    def test_checked_in_naming_components_collect_complete_crate_local_terms(self):
        repository = Path(__file__).resolve().parents[1]
        packages = []
        for name in ("nazuchem-s2n", "nazuchem-kernel", "nazuchem-n2s", "nazuchem"):
            manifest = repository / "crates" / name / "Cargo.toml"
            declaration = tomllib.loads(manifest.read_text())["package"]
            packages.append(
                {
                    "id": name,
                    "name": name,
                    "version": declaration["version"],
                    "license": declaration["license"],
                    "manifest_path": str(manifest),
                }
            )
        self.assertEqual(packages[0]["license"], "MIT AND (MIT OR Apache-2.0) AND Python-2.0")
        # The source tree has no root upstream terms in this miniature release.
        # All third-party bytes must therefore arrive through crate-local files.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            (root / "licenses/rust").mkdir(parents=True)
            (root / "licenses/rust/manifest.json").write_text("{}")
            write_notices(
                root,
                root / "output",
                {"packages": packages, "workspace_members": [p["id"] for p in packages]},
            )
            result = (root / "output" / THIRD_PARTY_FILE).read_bytes()
            copies = {
                "nazuchem-s2n": {
                    "licenses/openclatura/LICENSE": "licenses/openclatura/LICENSE",
                    "licenses/openclatura/NOTICE": "licenses/openclatura/NOTICE",
                    "licenses/opsin/LICENSE": "crates/nazuchem-n2s/LICENSE",
                    "licenses/opsin/NOTICE": "crates/nazuchem-n2s/NOTICE",
                    "licenses/cpython/LICENSE": "licenses/cpython/LICENSE",
                    "licenses/cpython/NOTICE": "licenses/cpython/NOTICE",
                },
                "nazuchem-kernel": {
                    "licenses/rdkit/LICENSE": "licenses/rdkit/LICENSE",
                    "licenses/centres/LICENSE": "licenses/centres/LICENSE",
                    "licenses/stdlib/llvm-LICENSE": "licenses/stdlib/llvm-LICENSE",
                    "licenses/stdlib/msvc-LICENSE": "licenses/stdlib/msvc-LICENSE",
                },
                "nazuchem-n2s": {"LICENSE": "crates/nazuchem-n2s/LICENSE"},
                "nazuchem": {
                    "licenses/opsin/LICENSE": "crates/nazuchem-n2s/LICENSE",
                    "licenses/opsin/NOTICE": "crates/nazuchem-n2s/NOTICE",
                    "licenses/openclatura/LICENSE": "licenses/openclatura/LICENSE",
                    "licenses/openclatura/NOTICE": "licenses/openclatura/NOTICE",
                    "licenses/cpython/LICENSE": "licenses/cpython/LICENSE",
                    "licenses/cpython/NOTICE": "licenses/cpython/NOTICE",
                    "licenses/rdkit/LICENSE": "licenses/rdkit/LICENSE",
                    "licenses/centres/LICENSE": "licenses/centres/LICENSE",
                    "licenses/stdlib/llvm-LICENSE": "licenses/stdlib/llvm-LICENSE",
                    "licenses/stdlib/msvc-LICENSE": "licenses/stdlib/msvc-LICENSE",
                },
            }
            for package in packages:
                name, version = package["name"], package["version"]
                for relative, upstream in copies[name].items():
                    with self.subTest(component=name, relative=relative):
                        expected = (repository / upstream).read_bytes()
                        self.assertEqual(
                            (repository / "crates" / name / relative).read_bytes(), expected
                        )
                        self.assertIn(f"{name}@{version}/{relative}".encode(), result)
                        self.assertIn(expected, result)
                self.assertIn(f"{name}@{version}/NOTICE".encode(), result)
                self.assertIn((repository / "crates" / name / "NOTICE").read_bytes(), result)
            self.assertNotIn(b"Original workspace crate:", result)

    def test_workspace_kernel_collects_nested_terms_with_either_manifest_expression(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            (root / "licenses/rust").mkdir(parents=True)
            (root / "licenses/rust/manifest.json").write_text("{}")
            source = root / "crates/nazuchem-kernel"
            source.mkdir(parents=True)
            terms = {
                "LICENSE-MIT": "Complete local MIT terms",
                "NOTICE": "Kernel source attribution and unresolved sort policy",
                "licenses/rdkit/LICENSE": "Complete RDKit BSD-3-Clause terms",
                "licenses/centres/LICENSE": "Complete centres BSD-2-Clause terms",
                "licenses/stdlib/llvm-LICENSE": "Complete LLVM terms and exception",
                "licenses/stdlib/msvc-LICENSE": "Complete Microsoft terms and exception",
            }
            for relative, text in terms.items():
                target = source / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text)
            package = {
                "id": "kernel",
                "name": "nazuchem-kernel",
                "version": "0.1.0",
                "manifest_path": str(source / "Cargo.toml"),
            }
            for expression in (
                "MIT OR Apache-2.0",
                "(MIT OR Apache-2.0) AND BSD-3-Clause AND BSD-2-Clause "
                "AND (Apache-2.0 WITH LLVM-exception)",
            ):
                with self.subTest(expression=expression):
                    package["license"] = expression
                    (source / "Cargo.toml").write_text(f'license = "{expression}"\n')
                    write_notices(
                        root,
                        root / "output",
                        {"packages": [package], "workspace_members": ["kernel"]},
                    )
                    result = (root / "output" / THIRD_PARTY_FILE).read_text()
                    for relative, text in terms.items():
                        self.assertIn(f"nazuchem-kernel@0.1.0/{relative}", result)
                        self.assertIn(text, result)
                    self.assertNotIn("Original workspace crate:", result)

    def test_separately_licensed_workspace_port_keeps_its_own_terms(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            (root / "licenses/rust").mkdir(parents=True)
            (root / "licenses/rust/manifest.json").write_text("{}")
            packages = []
            for name, license_name in [("reshiki", "MIT OR Apache-2.0"), ("nazuchem-n2s", "MIT")]:
                source = root / name
                source.mkdir()
                (source / "Cargo.toml").write_text(f'name = "{name}"\nlicense = "{license_name}"\n')
                packages.append(
                    {
                        "id": name,
                        "name": name,
                        "version": "1",
                        "license": license_name,
                        "manifest_path": str(source / "Cargo.toml"),
                    }
                )
            upstream = b"Original OPSIN copyright and complete MIT terms\r\n"
            attribution = b"Ported from the pinned upstream source, not original project code.\n"
            (root / "nazuchem-n2s/LICENSE").write_bytes(upstream)
            (root / "nazuchem-n2s/NOTICE").write_bytes(attribution)
            write_notices(
                root,
                root / "output",
                {"packages": packages, "workspace_members": ["reshiki", "nazuchem-n2s"]},
            )
            result = (root / "output" / THIRD_PARTY_FILE).read_bytes()
            self.assertIn(b"nazuchem-n2s@1/LICENSE", result)
            self.assertIn(b"nazuchem-n2s@1/NOTICE", result)
            self.assertIn(b"nazuchem-n2s@1/Cargo.toml", result)
            self.assertIn(upstream, result)
            self.assertIn(attribution, result)
            self.assertEqual(result.count(b"Original workspace crate:"), 1)
            self.assertEqual(result.count(b"Separately licensed workspace component:"), 1)

    def test_nested_licenses_and_versioned_supplements_survive_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            supplements = root / "licenses/rust"
            supplements.mkdir(parents=True)
            upstream = b"Upstream terms and original copyright\r\n"
            (supplements / "terms.txt").write_bytes(upstream)
            declaration = {
                "license": "MIT",
                "files": [{"path": "terms.txt", "sha256": hashlib.sha256(upstream).hexdigest()}],
            }
            (supplements / "manifest.json").write_text(json.dumps({"omitted@1.0": declaration}))
            packages = []
            for name in ["nested", "omitted"]:
                source = root / name
                source.mkdir()
                (source / "Cargo.toml").write_text(
                    'license = "MIT"\nauthors = ["Original Author"]\n'
                )
                packages.append(
                    {
                        "name": name,
                        "version": "1.0",
                        "manifest_path": str(source / "Cargo.toml"),
                        "license": "MIT",
                        "authors": ["Original Author"],
                    }
                )
            nested = root / "nested/vendor/library"
            nested.mkdir(parents=True)
            (nested / "NOTICE").write_text("Original nested attribution")
            output = root / "output"
            (output / "rust/legacy").mkdir(parents=True)
            (output / "rust/legacy/LICENSE").write_text("old")
            (output / "sources").mkdir()
            (output / "sources/old").write_text("old")
            (output / "rust-dependencies.json").write_text("{}")
            write_notices(root, output, {"packages": packages})
            content = (output / THIRD_PARTY_FILE).read_bytes()
            self.assertIn(b"nested@1.0/vendor/library/NOTICE", content)
            self.assertIn(b"Original nested attribution", content)
            self.assertIn(b"omitted@1.0/upstream/terms.txt", content)
            self.assertIn(upstream, content)
            self.assertIn(b"Original Author", content)
            self.assertEqual({p.name for p in output.iterdir()}, {*PROJECT_FILES, THIRD_PARTY_FILE})
            for name in PROJECT_FILES:
                self.assertEqual((root / name).read_bytes(), (output / name).read_bytes())
            # A new version must not silently inherit a stale notice record.
            packages[1]["version"] = "2.0"
            with self.assertRaisesRegex(ValueError, "Missing license text"):
                write_notices(root, output, {"packages": packages})
            self.assertEqual((output / THIRD_PARTY_FILE).read_bytes(), content)
            packages[1]["version"] = "1.0"
            packages[1]["license"] = "BSD-3-Clause"
            with self.assertRaisesRegex(ValueError, "License changed"):
                write_notices(root, output, {"packages": packages})
            packages[1]["license"] = "MIT"
            (supplements / "terms.txt").write_bytes(b"Accidental replacement")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                write_notices(root, output, {"packages": packages})

    def test_identical_texts_are_shared_without_merging_different_copyrights(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            (root / "licenses/rust").mkdir(parents=True)
            (root / "licenses/rust/manifest.json").write_text("{}")
            shared = b"Full license terms, including the disclaimer.\n"
            packages = []
            for name in ["one", "two"]:
                source = root / name
                source.mkdir()
                (source / "Cargo.toml").write_text(f'name = "{name}"\nlicense = "MIT"\n')
                (source / "LICENSE").write_bytes(shared)
                (source / "COPYRIGHT").write_text(f"Copyright {name}")
                packages.append(
                    {
                        "name": name,
                        "version": "1",
                        "license": "MIT",
                        "manifest_path": str(source / "Cargo.toml"),
                    }
                )
            write_notices(root, root / "output", {"packages": packages})
            result = (root / "output" / THIRD_PARTY_FILE).read_bytes()
            digest = hashlib.sha256(shared).hexdigest().encode()
            self.assertEqual(result.count(b"BEGIN TEXT SHA-256 " + digest), 1)
            self.assertEqual(result.count(b"Text SHA-256: " + digest), 2)
            self.assertIn(b"one@1/LICENSE", result)
            self.assertIn(b"two@1/LICENSE", result)
            self.assertIn(b"Copyright one", result)
            self.assertIn(b"Copyright two", result)
            self.assertEqual(result.count(shared), 1)
            # Output is deterministic regardless of metadata package order.
            write_notices(root, root / "again", {"packages": list(reversed(packages))})
            self.assertEqual((root / "again" / THIRD_PARTY_FILE).read_bytes(), result)


if __name__ == "__main__":
    unittest.main()
