"""Consolidated distribution: complete texts, attribution, deduplication and upgrades."""

import hashlib
import json
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from license_notices import PROJECT_FILES, THIRD_PARTY_FILE, dependency_metadata, write_notices


class LicenseNoticeTests(unittest.TestCase):
    def test_package_scope_keeps_transitive_normal_and_build_dependencies(self):
        def dependency(package, *kinds):
            return {"pkg": package, "dep_kinds": [{"kind": kind} for kind in kinds]}

        graph = {
            "python": [
                dependency("normal", None),
                dependency("build", "build"),
                dependency("dev", "dev"),
            ],
            "normal": [dependency("shared", "dev", None)],
            "build": [dependency("transitive", None)],
            "shared": [dependency("normal", None)],
            "transitive": [],
            "dev": [dependency("dev-only", None)],
            "dev-only": [],
            "desktop": [dependency("gui", None)],
            "gui": [],
        }
        metadata = {
            "packages": [{"id": package} for package in graph],
            "workspace_members": ["python", "desktop"],
            "resolve": {
                "root": "python",
                "nodes": [{"id": package, "deps": deps} for package, deps in graph.items()],
            },
        }
        scoped = dependency_metadata(metadata)
        self.assertEqual(
            {package["id"] for package in scoped["packages"]},
            {"normal", "build", "shared", "transitive"},
        )
        self.assertEqual(len(metadata["packages"]), len(graph))
        self.assertEqual(scoped["workspace_members"], metadata["workspace_members"])
        metadata["resolve"]["root"] = None
        with self.assertRaisesRegex(ValueError, "Select a package manifest"):
            dependency_metadata(metadata)

    def test_git_naming_components_collect_complete_crate_local_terms(self):
        repository = Path(__file__).resolve().parents[1]
        metadata = json.loads(
            subprocess.check_output(
                ["cargo", "metadata", "--locked", "--format-version", "1"],
                cwd=repository,
                text=True,
            )
        )
        runtime_names = {"nazuchem-ir", "nazuchem-kernel", "nazuchem-n2s", "nazuchem-s2n"}
        packages = [p for p in metadata["packages"] if p["name"].startswith("nazuchem")]
        self.assertEqual({p["name"] for p in packages}, runtime_names)
        workspace = tomllib.loads((repository / "Cargo.toml").read_text())["workspace"]
        declaration = workspace["dependencies"]["nazuchem-ir"]
        self.assertEqual(declaration["git"], "https://github.com/Ameyanagi/nazuchem")
        self.assertEqual(len(declaration["rev"]), 40)
        git_source = f"git+{declaration['git']}?rev={declaration['rev']}#{declaration['rev']}"
        sources = {}
        for package in packages:
            with self.subTest(component=package["name"]):
                self.assertEqual(package["source"], git_source)
                self.assertNotIn(package["id"], metadata["workspace_members"])
                source = Path(package["manifest_path"]).parent.resolve()
                self.assertFalse(source.is_relative_to(repository.resolve()))
                sources[package["name"]] = source
        self.assertEqual(len(packages), len(sources))
        self.assertEqual(len({source.parent for source in sources.values()}), 1)

        self.assertEqual(set(sources), runtime_names)
        self.assertEqual(
            next(p["license"] for p in packages if p["name"] == "nazuchem-s2n"),
            "MIT AND (MIT OR Apache-2.0) AND Python-2.0",
        )
        # The packaging root deliberately has no upstream source terms. Every
        # license and attribution below must be read from Cargo's git checkout.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            (root / "licenses/rust").mkdir(parents=True)
            (root / "licenses/rust/manifest.json").write_text("{}")
            write_notices(
                root,
                root / "output",
                {"packages": packages, "workspace_members": metadata["workspace_members"]},
            )
            result = (root / "output" / THIRD_PARTY_FILE).read_bytes()
            self.assertFalse((root / "licenses/nazuchem").exists())
            opsin_source = sources["nazuchem-n2s"]
            root_terms = {name: repository / name for name in ("LICENSE-MIT", "LICENSE-APACHE")}
            # License grants remain identical to the retained root copies.
            # Attribution notices also describe the extracted source layout,
            # so their canonical bytes come from the corresponding git crate.
            openclatura_terms = {
                f"licenses/openclatura/{name}": (
                    sources["nazuchem-s2n"] if name == "NOTICE" else repository
                )
                / "licenses/openclatura"
                / name
                for name in ("LICENSE", "NOTICE")
            }
            opsin_terms = {
                f"licenses/opsin/{name}": opsin_source / name for name in ("LICENSE", "NOTICE")
            }
            cpython_terms = {
                f"licenses/cpython/{name}": (
                    sources["nazuchem-s2n"] if name == "NOTICE" else repository
                )
                / "licenses/cpython"
                / name
                for name in ("LICENSE", "NOTICE")
            }
            kernel_terms = {
                f"licenses/{component}/{name}": (
                    sources["nazuchem-kernel"] if name == "NOTICE" else repository
                )
                / "licenses"
                / component
                / name
                for component, names in (
                    ("rdkit", ("LICENSE", "NOTICE")),
                    ("centres", ("LICENSE", "NOTICE")),
                    ("stdlib", ("llvm-LICENSE", "msvc-LICENSE", "NOTICE")),
                )
                for name in names
            }
            copies = {
                "nazuchem-ir": root_terms,
                "nazuchem-s2n": {
                    "LICENSE": repository / "LICENSE",
                    **root_terms,
                    **openclatura_terms,
                    **opsin_terms,
                    **cpython_terms,
                },
                "nazuchem-kernel": {
                    "LICENSE": repository / "LICENSE",
                    **root_terms,
                    **kernel_terms,
                },
                "nazuchem-n2s": {"LICENSE": opsin_source / "LICENSE"},
            }
            expected_labels = set()
            for package in packages:
                name, version = package["name"], package["version"]
                for relative, upstream in copies[name].items():
                    with self.subTest(component=name, relative=relative):
                        expected = upstream.read_bytes()
                        self.assertEqual((sources[name] / relative).read_bytes(), expected)
                        label = f"{name}@{version}/{relative}"
                        expected_labels.add(label)
                        self.assertIn(label.encode(), result)
                        self.assertIn(expected, result)
                for relative in ("NOTICE", "Cargo.toml"):
                    with self.subTest(component=name, relative=relative):
                        label = f"{name}@{version}/{relative}"
                        expected_labels.add(label)
                        self.assertIn(label.encode(), result)
                        self.assertIn((sources[name] / relative).read_bytes(), result)
            # All 26 crate-local license/notice files and four Cargo manifests
            # must be indexed directly from the four production dependencies.
            index = result.split(b"\nCOMPLETE TEXTS\n", 1)[0].decode()
            actual_labels = {
                line.strip() for line in index.splitlines() if line.startswith("  nazuchem")
            }
            self.assertEqual(len(expected_labels), 30)
            self.assertEqual(actual_labels, expected_labels)
            self.assertIn(f"source: {json.dumps(git_source)}".encode(), result)
            self.assertNotIn(b"Original workspace crate:", result)
            self.assertNotIn(b"Separately licensed workspace component:", result)

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
