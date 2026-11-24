"""Package the browser module graph without building WebAssembly."""

import hashlib
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(
    os.name == "posix"
    and all(shutil.which(tool) for tool in ["bun", "bash", "perl", "shasum", "gzip", "tar"]),
    "The demo packager requires Bun and its POSIX packaging tools",
)
class WebDemoPackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts/package_web_demo.sh", self.root / "scripts")
        (self.root / "Cargo.toml").write_text('version = "1.0.0"\n')
        self.package = self.root / "packages/editor"
        shutil.copytree(ROOT / "packages/editor/src", self.package / "src")
        (self.package / "examples").mkdir()
        shutil.copy(ROOT / "packages/editor/examples/demo.html", self.package / "examples")
        for folder, name in [("editor", "reshiki"), ("worker", "reshiki_web_worker")]:
            dist = self.package / "dist" / folder
            dist.mkdir(parents=True)
            (dist / f"{name}.js").write_text(
                f'export const url = new URL("{name}_bg.wasm", import.meta.url);\n'
                # This resembles a wasm-bindgen import key, not a module request.
                f'export const imports = {{ "./{name}_bg.js": {{}} }};\n'
            )
            (dist / f"{name}_bg.wasm").write_bytes(b"\0asm\x01\0\0\0")
        (self.package / "dist/licenses").mkdir()
        (self.package / "dist/licenses/LICENSE").write_text("Fixture\n")

    def package_demo(self, name="demo.tar.gz", previous=None, *, succeeds=True):
        archive = self.root / name
        command = ["bash", str(self.root / "scripts/package_web_demo.sh")]
        if previous is not None:
            command += ["--previous", str(previous)]
        result = subprocess.run(command + [str(archive)], text=True, capture_output=True)
        if not succeeds:
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertFalse(archive.exists())
            return result.stderr
        self.assertEqual(result.returncode, 0, result.stderr)
        with tarfile.open(archive) as tar:
            files = {}
            for member in tar.getmembers():
                if member.isfile():
                    data = tar.extractfile(member)
                    assert data is not None
                    files[member.name] = data.read()
        return archive, files

    def test_helpers_are_hashed_referenced_and_counted_in_the_first_download(self):
        _, files = self.package_demo()
        for name, data in files.items():
            if name.endswith((".js", ".wasm")):
                self.assertEqual(name.split(".")[-2], hashlib.sha256(data).hexdigest()[:12])
        for parent, helper in [("editor", "initialization"), ("worker", "worker-handler")]:
            helper_name = next(name for name in files if f"/src/{helper}." in name)
            parent_data = next(data for name, data in files.items() if f"/src/{parent}." in name)
            self.assertIn(Path(helper_name).name.encode(), parent_data)
        first_load = [
            data
            for name, data in files.items()
            if any(
                prefix in name
                for prefix in [
                    "/src/editor.",
                    "/src/fonts.",
                    "/src/initialization.",
                    "/dist/editor/reshiki.",
                    "/dist/editor/reshiki_bg.",
                ]
            )
        ]
        self.assertEqual(len(first_load), 5)
        estimate = sum(
            len(
                subprocess.run(["gzip", "-9nc"], input=data, capture_output=True, check=True).stdout
            )
            for data in first_load
        )
        page = files["demo/index.html"].decode()
        match = re.search(r'data-gzip-bytes="(\d+)"', page)
        assert match is not None
        self.assertEqual(int(match[1]), estimate)

    def test_missing_editor_or_worker_dependency_fails_packaging(self):
        for parent in ["editor", "worker-handler"]:
            with self.subTest(parent=parent):
                source = self.package / "src" / f"{parent}.js"
                original = source.read_text()
                try:
                    source.write_text(original + '\nimport "./missing-helper.js";\n')
                    self.assertIn("missing-helper.js", self.package_demo(succeeds=False))
                finally:
                    source.write_text(original)

    def test_previous_archive_keeps_the_previous_hashed_helpers(self):
        previous, old_files = self.package_demo("previous.tar.gz")
        for helper in ["initialization", "worker-handler"]:
            source = self.package / "src" / f"{helper}.js"
            source.write_text(source.read_text() + "\n// Next release.\n")
        _, files = self.package_demo(previous=previous)
        for name, data in old_files.items():
            if name.endswith((".js", ".wasm")):
                self.assertEqual(files.get(name), data, name)
        for helper in ["initialization", "worker-handler"]:
            self.assertEqual(sum(f"/src/{helper}." in name for name in files), 2)


if __name__ == "__main__":
    unittest.main()
