"""Unit checks for offline xTB source packaging, without compiling the application."""

import contextlib
import gzip
import hashlib
import io
import json
import os
import re
import shlex
import sys
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from prepare_nightly import stamp
from xtb_source_archive import (
    DIST_ENV,
    EXCLUDED_MEDIA,
    FIXED_MTIME,
    MANIFEST,
    build,
    build_instructions,
    check,
    evidence_media,
    exclude_axtb_source,
    file_hashes,
    merge_vendor_config,
    modify_driver_version,
    output,
    patch_arguments,
    verify_checksum,
    verify_manifest,
    write_archive,
    write_manifest,
)

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/release.yml"
CRATES = {
    "axtb": "axtb",
    "axtb-base": "base",
    "axtb-d4": "d4",
    "axtb-driver": "driver",
    "axtb-gfnff": "gfnff",
    "axtb-gxtb": "gxtb",
    "axtb-solvation": "solvation",
}


def fork_fixture(root):
    for name, directory in CRATES.items():
        path = root / "crates" / directory / "Cargo.toml"
        path.parent.mkdir(parents=True)
        path.write_text(f'[package]\nname = "{name}"\nversion = "0.2.0"\n', encoding="utf-8")
    response = root / "crates/driver/src/response.rs"
    response.parent.mkdir()
    response.write_text('pub const VERSION: &str = env!("CARGO_PKG_VERSION");\n', encoding="utf-8")


def workflow_job(workflow, job):
    return re.search(rf"^  {job}:\n(.*?)(?=^  [\w-]+:\n|\Z)", workflow, re.M | re.S)[1]


def artifact_setting(expression, artifact):
    for branch in expression.split(" || "):
        match = re.fullmatch(r"startsWith\(matrix\.artifact, '([^']+)'\) && '([^']+)'", branch)
        if match:
            if artifact.startswith(match[1]):
                return match[2]
            continue
        match = re.fullmatch(r"matrix\.artifact == '([^']+)' && '([^']+)'", branch)
        if match:
            if artifact == match[1]:
                return match[2]
            continue
        return re.fullmatch(r"'([^']+)'", branch)[1]
    raise ValueError(f"No matching workflow setting for {artifact}: {expression}")


def workflow_environment(block, indentation, artifact):
    environment = []
    in_environment = False
    for line in block.splitlines():
        if line == f"{' ' * (indentation - 2)}env:":
            in_environment = True
            continue
        if not in_environment or not line.strip() or line.lstrip().startswith("#"):
            continue
        if not line.startswith(" " * indentation):
            break
        environment.append(line)
    settings = {}
    for name, value in re.findall(
        rf"^{' ' * indentation}(CARGO_PROFILE_DIST_\w+|MACOSX_DEPLOYMENT_TARGET): (.+)$",
        "\n".join(environment),
        re.M,
    ):
        expression = re.fullmatch(r"\$\{\{ (.+) \}\}", value)
        if expression:
            settings[name] = artifact_setting(expression[1], artifact)
        else:
            literal = shlex.split(value, comments=True)
            if len(literal) != 1:
                raise ValueError(f"Unsupported workflow environment setting: {name}: {value}")
            settings[name] = literal[0]
    return settings


class XtbSourceArchiveTests(unittest.TestCase):
    def test_source_archive_shares_its_release_and_is_never_deleted_alone(self):
        """Prune with the binaries: one release holds both, and no asset is deleted alone."""
        for workflow in ("release.yml", "nightly.yml"):
            # Join shell continuation lines, then find the release of the app downloads.
            text = (ROOT / ".github/workflows" / workflow).read_text(encoding="utf-8")
            commands = [
                line.split()
                for line in text.replace("\\\n", " ").splitlines()
                if "gh release create" in line and "*.zip" in line
            ]
            self.assertEqual(len(commands), 1, workflow)
            for asset in ("*.tar.gz", "*-source.tar.gz.sha256", "SHA256SUMS"):
                self.assertIn(asset, commands[0], f"{workflow}: {asset}")
        # Nothing may add, replace or delete single assets of an application
        # release; only the separate web-demo release replaces its own asset.
        single_asset = re.compile(
            r"gh release upload|--clobber|delete-asset|releases/assets"
            r"|requests\.delete|method=[\"']DELETE"
        )
        offenders = [
            f"{path.relative_to(ROOT).as_posix()}: {line.strip()}"
            for folder in (".github", "scripts")
            for path in sorted((ROOT / folder).rglob("*"))
            if path.is_file() and path.suffix in {".yml", ".yaml", ".py", ".sh", ".ps1"}
            for line in path.read_text(encoding="utf-8", errors="replace").splitlines()
            if single_asset.search(line) and "web-demo" not in line
        ]
        self.assertEqual(offenders, [], "release assets must only be deleted with their release")

    def test_patch_arguments_cover_exactly_seven_real_crate_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "axtb-source"
            fork_fixture(source)
            arguments = patch_arguments(source)
            self.assertEqual(arguments[::2], ["--config"] * 7)
            document = tomllib.loads("\n".join(arguments[1::2]))
            patches = document["patch"]["https://github.com/Ameyanagi/axtb"]
            self.assertEqual(set(patches), set(CRATES))
            for name, directory in CRATES.items():
                expected = f"axtb-source/crates/{directory}"
                self.assertEqual(patches[name], {"path": expected})
                self.assertTrue((source.parent / expected / "Cargo.toml").is_file())

    def test_patch_arguments_reject_missing_or_wrong_crate(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            fork_fixture(source)
            manifest = source / "crates/base/Cargo.toml"
            manifest.write_text('[package]\nname = "wrong-crate"\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Expected crate axtb-base"):
                patch_arguments(source)
            manifest.unlink()
            with self.assertRaisesRegex(ValueError, "Missing axtb crate manifest"):
                patch_arguments(source)

    def test_staged_workspace_excludes_the_entire_fork(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "Cargo.toml"
            original = '[workspace]\nmembers = ["crates/worker"]\n'
            manifest.write_text(original, encoding="utf-8")
            exclude_axtb_source(root)
            staged = manifest.read_text(encoding="utf-8")
            self.assertEqual(
                tomllib.loads(staged)["workspace"],
                {"members": ["crates/worker"], "exclude": ["axtb-source"]},
            )
            exclude_axtb_source(root)
            self.assertEqual(manifest.read_text(encoding="utf-8"), staged)

    def test_manifest_hashes_every_file_and_detects_changes_missing_and_extra_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "source").mkdir()
            original = root / "source/main.rs"
            original.write_bytes(b"fn main() {}\n")
            (root / "Cargo.lock").write_bytes(b"locked dependency graph\n")
            details = {"reshiki": {"commit": "a" * 40}, "axtb": {"rev": "b" * 40}}
            manifest = write_manifest(root, details)
            expected = {
                "Cargo.lock": hashlib.sha256(b"locked dependency graph\n").hexdigest(),
                "source/main.rs": hashlib.sha256(b"fn main() {}\n").hexdigest(),
            }
            self.assertEqual(manifest["files"], expected)
            self.assertEqual(verify_manifest(root), {**details, "files": expected})
            self.assertNotIn(MANIFEST, file_hashes(root))
            original.write_bytes(b"changed\n")
            with self.assertRaisesRegex(ValueError, "source/main.rs"):
                verify_manifest(root)
            original.unlink()
            with self.assertRaisesRegex(ValueError, "source/main.rs"):
                verify_manifest(root)
            original.write_bytes(b"fn main() {}\n")
            (root / "unexpected").touch()
            with self.assertRaisesRegex(ValueError, "unexpected"):
                verify_manifest(root)

    def test_manifest_hashes_symlink_target_without_following_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "original").write_bytes(b"original")
            try:
                (root / "link").symlink_to("original")
            except OSError as error:
                if getattr(error, "winerror", None) != 1314:
                    raise
                self.skipTest("Windows requires privileges to create a symbolic link")
            write_manifest(root, {})
            self.assertEqual(file_hashes(root)["link"], hashlib.sha256(b"original").hexdigest())
            (root / "link").unlink()
            (root / "link").symlink_to("another-file")
            with self.assertRaisesRegex(ValueError, "link"):
                verify_manifest(root)

    def test_vendor_config_preserves_existing_env_and_relative_source_replacements(self):
        existing = (
            '[env]\n# Keep the system compiler.\nCC_aarch64_apple_darwin = "/usr/bin/clang"\n'
        )
        vendor = """[source.crates-io]
replace-with = "vendored-sources"

[source."git+https://github.com/Ameyanagi/axtb?rev=abc"]
git = "https://github.com/Ameyanagi/axtb"
rev = "abc"
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendored-crates"
"""
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / ".cargo/config.toml"
            path.parent.mkdir()
            path.write_text(existing, encoding="utf-8")
            merge_vendor_config(path, vendor)
            merged = path.read_text(encoding="utf-8")
            self.assertTrue(merged.startswith(existing))
            parsed = tomllib.loads(merged)
            self.assertEqual(parsed["env"], {"CC_aarch64_apple_darwin": "/usr/bin/clang"})
            self.assertEqual(parsed["source"]["vendored-sources"], {"directory": "vendored-crates"})
            self.assertEqual(parsed["source"]["crates-io"]["replace-with"], "vendored-sources")
            with self.assertRaises(tomllib.TOMLDecodeError):
                merge_vendor_config(path, vendor)
            self.assertEqual(path.read_text(encoding="utf-8"), merged)

    def test_archive_entries_are_sorted_and_have_fixed_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            root = temporary / "reshiki-fixture-source"
            (root / "z").mkdir(parents=True)
            (root / "a").write_bytes(b"first\n")
            (root / "z-extra").touch()
            executable = root / "z/run.sh"
            executable.write_bytes(b"#!/bin/sh\n")
            executable.chmod(0o755)
            archive = temporary / "first.tar.gz"
            write_archive(root, archive)
            verify_checksum(archive)
            with tarfile.open(archive) as stream:
                entries = stream.getmembers()
                self.assertEqual(
                    [entry.name for entry in entries], sorted(entry.name for entry in entries)
                )
                for entry in entries:
                    self.assertEqual((entry.mtime, entry.uid, entry.gid), (FIXED_MTIME, 0, 0))
                    self.assertEqual((entry.uname, entry.gname), ("", ""))
                    self.assertEqual(entry.pax_headers, {})
                self.assertEqual(stream.getmember(f"{root.name}/a").mode, 0o644)
                self.assertEqual(
                    stream.getmember(f"{root.name}/z/run.sh").mode,
                    0o755 if executable.stat().st_mode & 0o111 else 0o644,
                )
            self.assertEqual(int.from_bytes(archive.read_bytes()[4:8], "little"), FIXED_MTIME)
            with gzip.open(archive) as contents:
                self.assertTrue(contents.read())
            os.utime(executable, (123456, 123456))
            second = temporary / "different-filename.tar.gz"
            write_archive(root, second)
            self.assertEqual(archive.read_bytes(), second.read_bytes())
            archive.write_bytes(b"corrupted archive")
            with self.assertRaisesRegex(ValueError, "Archive SHA-256 mismatch"):
                verify_checksum(archive)

    def test_rebuild_instructions_match_toolchain_prerequisites_and_patch_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fork_fixture(root / "axtb-source")
            (root / "docs").mkdir()
            for relative in ("rust-toolchain.toml", "docs/development.md"):
                (root / relative).write_bytes((ROOT / relative).read_bytes())
            instructions = build_instructions(root)
            toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"][
                "channel"
            ]
            self.assertIn(f"Rust {toolchain}", instructions)
            self.assertIn("build-essential pkg-config", instructions)
            self.assertIn("libxkbcommon-dev", instructions)
            self.assertIn("libudev-dev", instructions)
            self.assertIn("xcode-select --install", instructions)
            for environment in DIST_ENV.values():
                self.assertIn(
                    "\n".join(f"export {key}={value}" for key, value in environment.items()),
                    instructions,
                )
            self.assertIn(
                "cargo build --profile dist --locked --offline --features xtb --bin reshiki",
                instructions,
            )
            command = next(
                line
                for line in instructions.replace("\\\n", "").splitlines()
                if line.startswith("cargo build --offline")
            )
            arguments = shlex.split(command)
            self.assertNotIn("--locked", arguments)
            self.assertIn("--offline", arguments)
            self.assertEqual(
                arguments[arguments.index("--config") :], patch_arguments(root / "axtb-source")
            )
            self.assertIn("codesign --force --sign - target/dist/reshiki", instructions)
            self.assertIn("bundle_metadata", instructions)
            self.assertIn("licenses/axtb/", instructions)
            self.assertIn("sources for building and testing the ReShiki\napplication", instructions)
            self.assertIn("The website cannot be built from this archive", instructions)
            self.assertIn("`docs/images/` assets", instructions)
            for directory in EXCLUDED_MEDIA:
                self.assertIn(f"`{directory}**`", instructions)

    def assert_dist_environment_matches_release_workflow(self, workflow):
        job = workflow_job(workflow, "build")
        build_steps = [
            step
            for step in re.findall(r"^      - .+?(?=^      - |\Z)", job, re.M | re.S)
            if "scripts/build_release.py" in step
        ]
        self.assertTrue(build_steps, "No desktop build steps found")
        platforms = {
            "macos-arm64": "macOS (Apple Silicon and Intel)",
            "macos-x64": "macOS (Apple Silicon and Intel)",
            "linux-arm64": "Linux arm64",
            "linux-x64": "Linux x64",
        }
        self.assertEqual(
            set(re.findall(r'"artifact": "((?:macos|linux)-[^"]+)"', job)), platforms.keys()
        )
        self.assertEqual(set(platforms.values()), DIST_ENV.keys())
        for artifact, platform in platforms.items():
            for step in build_steps:
                environment = workflow_environment(job, 6, artifact)
                environment.update(workflow_environment(step, 10, artifact))
                if not artifact.startswith("macos-"):
                    environment.pop("MACOSX_DEPLOYMENT_TARGET", None)
                self.assertEqual(DIST_ENV[platform], environment, artifact)

    def test_dist_environment_matches_release_workflow(self):
        self.assert_dist_environment_matches_release_workflow(WORKFLOW.read_text(encoding="utf-8"))

    def test_dist_environment_detects_step_overrides(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        build_env = "      - name: Build and test extracted portable archive\n        env:\n"
        self.assertIn(build_env, workflow)
        overrides = {
            "CARGO_PROFILE_DIST_LTO": '"fat"',
            "CARGO_PROFILE_DIST_CODEGEN_UNITS": "2",
            "CARGO_PROFILE_DIST_STRIP": '"none"',
            "CARGO_PROFILE_DIST_SPLIT_DEBUGINFO": '"off"',
            "CARGO_PROFILE_DIST_OPT_LEVEL": "1",
            "MACOSX_DEPLOYMENT_TARGET": '"13.0"',
        }
        for name, value in overrides.items():
            with self.subTest(name=name):
                if name == "MACOSX_DEPLOYMENT_TARGET":
                    changed = workflow.replace(
                        build_env + '          MACOSX_DEPLOYMENT_TARGET: "14.0"\n',
                        build_env + f"          {name}: {value}\n",
                        1,
                    )
                else:
                    changed = workflow.replace(
                        build_env, build_env + f"          {name}: {value}\n", 1
                    )
                with self.assertRaises(AssertionError):
                    self.assert_dist_environment_matches_release_workflow(changed)

    def test_dist_environment_detects_literal_job_overrides(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        original = re.search(r"^      CARGO_PROFILE_DIST_LTO: .+$", workflow, re.M)[0]
        for value in ("false", '"fat"', "'thin'"):
            with self.subTest(value=value):
                changed = workflow.replace(original, f"      CARGO_PROFILE_DIST_LTO: {value}", 1)
                environment = workflow_environment(workflow_job(changed, "build"), 6, "macos-arm64")
                self.assertEqual(environment["CARGO_PROFILE_DIST_LTO"], shlex.split(value)[0])
                with self.assertRaises(AssertionError):
                    self.assert_dist_environment_matches_release_workflow(changed)

    def test_dist_environment_applies_step_deployment_target_over_literal_job_value(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        job = workflow_job(workflow, "build")
        changed_job = job.replace(
            "    env:\n", '    env:\n      MACOSX_DEPLOYMENT_TARGET: "13.0"\n', 1
        )
        changed = workflow.replace(job, changed_job, 1)
        self.assert_dist_environment_matches_release_workflow(changed)

        build_env = "      - name: Build and test extracted portable archive\n        env:\n"
        changed = changed.replace(
            build_env + '          MACOSX_DEPLOYMENT_TARGET: "14.0"\n', build_env, 1
        )
        with self.assertRaises(AssertionError):
            self.assert_dist_environment_matches_release_workflow(changed)

    def test_dist_environment_ignores_settings_outside_env_blocks(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        job = workflow_job(workflow, "build")
        command = re.search(r"^        run: (.+scripts/build_release\.py.+)$", job, re.M)[0]
        changes = {
            "job outputs": job.replace(
                "    steps:\n",
                '    outputs:\n      CARGO_PROFILE_DIST_LTO: "fat"\n'
                '      MACOSX_DEPLOYMENT_TARGET: "13.0"\n    steps:\n',
                1,
            ),
            "step run block": job.replace(
                command,
                "        run: |\n          cat <<'ENV_EXAMPLE'\n"
                '          CARGO_PROFILE_DIST_LTO: "fat"\n'
                '          MACOSX_DEPLOYMENT_TARGET: "13.0"\n'
                "          ENV_EXAMPLE\n          " + command.removeprefix("        run: "),
                1,
            ),
        }
        for location, changed_job in changes.items():
            with self.subTest(location=location):
                self.assert_dist_environment_matches_release_workflow(
                    workflow.replace(job, changed_job, 1)
                )

    def test_source_and_desktop_jobs_share_nightly_stamp_before_packaging(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        validate = workflow_job(workflow, "validate")
        source = workflow_job(workflow, "source")
        desktop = workflow_job(workflow, "build")
        self.assertIn("nightly_version: ${{ steps.nightly.outputs.version }}", validate)
        self.assertRegex(
            source,
            r"(?m)^      - &stamp_nightly_version\n"
            r"        name: Stamp nightly package version\n"
            r"        if: inputs\.nightly\n"
            r"        shell: bash\n"
            r"        env:\n"
            r"          NIGHTLY_VERSION: \$\{\{ needs\.validate\.outputs\.nightly_version \}\}\n"
            r"        run: .*scripts/prepare_nightly\.py --version \"\$NIGHTLY_VERSION\"$",
        )
        self.assertLess(
            source.index("&stamp_nightly_version"), source.index("xtb_source_archive.py build")
        )
        self.assertLess(
            desktop.index("*stamp_nightly_version"),
            desktop.index("scripts/build_release.py --target"),
        )

    def test_archive_uses_stamped_working_tree_and_excludes_unused_media(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            repo = base / "repo"
            repo.mkdir()
            for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "docs/development.md"):
                path = repo / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((ROOT / name).read_bytes())
            kept = "docs/images/fixture.jpg"
            omitted = ["docs/images/screenshot.jpg", "website/public/media/promo.webp"]
            for name in [kept, *omitted]:
                path = repo / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"media")
            tracked = repo / "tracked.txt"
            tracked.write_bytes(b"committed")

            def git(*args):
                return output(["git", *args], cwd=repo)

            git("init", "-q")
            git("add", "--all")
            git(
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                f"core.hooksPath={base / 'disabled-hooks'}",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "Base",
            )
            version = stamp(repo, identifier="20261125.12.1")
            tracked.write_bytes(b"working tree")
            (repo / "untracked").touch()

            def cargo_output(command, **kwargs):
                if command[0] == "git":
                    return output(command, **kwargs)
                if command[:2] == ["cargo", "vendor"]:
                    return "[source.vendored-sources]\ndirectory = 'vendored-crates'"
                return "test tool version"

            with (
                patch("xtb_source_archive.ROOT", repo),
                patch("build_release.ROOT", repo),
                patch("xtb_source_archive.evidence_media", return_value={kept}),
                patch("xtb_source_archive.fork_checkout", return_value=(repo, "a" * 40, "0.2.0")),
                patch(
                    "xtb_source_archive.git_archive",
                    side_effect=lambda _, __, dest: fork_fixture(dest),
                ),
                patch("xtb_source_archive.output", side_effect=cargo_output),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                archive = build(base / "artifacts")
            top = f"reshiki-{version}-source"
            self.assertEqual(archive.name, f"{top}.tar.gz")
            with tarfile.open(archive) as stream:
                self.assertEqual(
                    stream.extractfile(f"{top}/{tracked.name}").read(), b"working tree"
                )
                for name in ("Cargo.toml", "Cargo.lock"):
                    self.assertNotIn(version, git("show", f"HEAD:{name}"))
                    actual = tomllib.loads(stream.extractfile(f"{top}/{name}").read().decode())
                    expected = tomllib.loads((repo / name).read_text(encoding="utf-8"))
                    if name == "Cargo.toml":
                        expected["workspace"]["exclude"] = ["axtb-source"]
                    self.assertEqual(actual, expected)
                manifest = json.load(stream.extractfile(f"{top}/{MANIFEST}"))
                self.assertEqual(manifest["reshiki"]["version"], version)
                self.assertEqual(manifest["exclusions"], [f"{path}**" for path in EXCLUDED_MEDIA])
                self.assertEqual(manifest["retained_media"], [kept])
                self.assertIn(f"{top}/{kept}", stream.getnames())
                for name in [*omitted, "untracked"]:
                    self.assertNotIn(f"{top}/{name}", stream.getnames())

    def test_media_read_by_evidence_verifiers_is_retained(self):
        retained = evidence_media(ROOT)
        for name in (
            "docs/images/coordination/co-en3-assembled-reopened.jpg",
            "docs/images/nmr/mock-alignment/approved-generated-mock.png",
            "docs/images/retained-plus-lactic-acid/02-plus-lactic-inserted.jpg",
        ):
            self.assertIn(name, retained)

    def test_modify_driver_version_changes_only_the_constant_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            fork_fixture(source)
            response = source / "crates/driver/src/response.rs"
            original = response.read_text()
            response.write_text("// Before\n" + original + "// After\n", encoding="utf-8")
            self.assertEqual(modify_driver_version(source, "0.2.0"), "0.2.0+modified")
            self.assertEqual(
                response.read_text(),
                '// Before\npub const VERSION: &str = "0.2.0+modified";\n// After\n',
            )

    def test_check_builds_from_empty_cargo_home_with_documented_patches(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            root = temporary / "reshiki-fixture-source"
            fork_fixture(root / "axtb-source")
            write_manifest(root, {"axtb": {"rev": "a" * 40, "version": "0.2.0"}})
            archive = temporary / "fixture.tar.gz"
            write_archive(root, archive)

            calls = []

            def cargo(command, *, cwd, env):
                calls.append((command, env["CARGO_HOME"]))
                self.assertIn(" ", str(cwd))
                self.assertEqual(env["CARGO_NET_OFFLINE"], "true")
                self.assertEqual(Path(env["CARGO_TARGET_DIR"]), cwd / "target")
                response = (cwd / "axtb-source/crates/driver/src/response.rs").read_text()
                if len(calls) == 1:
                    self.assertEqual(
                        command,
                        [
                            "cargo",
                            "check",
                            "--locked",
                            "--offline",
                            "--features",
                            "xtb",
                            "--bin",
                            "reshiki",
                        ],
                    )
                    self.assertEqual(list(Path(env["CARGO_HOME"]).iterdir()), [])
                    self.assertIn('env!("CARGO_PKG_VERSION")', response)
                    return
                self.assertEqual(env["CARGO_HOME"], calls[0][1])
                self.assertEqual(
                    command[:7],
                    ["cargo", "build", "--offline", "--features", "xtb", "--bin", "reshiki"],
                )
                self.assertEqual(command[7:], patch_arguments(cwd / "axtb-source"))
                self.assertNotIn("--locked", command)
                self.assertIn('"0.2.0+modified"', response)

            def verify(binary, metadata):
                self.assertEqual(binary.parent.name, "debug")
                self.assertEqual(
                    metadata["xtb"],
                    {"included": True, "rev": "a" * 40, "version": "0.2.0+modified"},
                )
                print("axtb 0.2.0+modified water energy verified")

            summary = io.StringIO()
            with (
                patch("xtb_source_archive.run", side_effect=cargo) as builder,
                patch("build_release.verify_xtb_worker", side_effect=verify) as verifier,
                contextlib.redirect_stdout(summary),
            ):
                self.assertGreaterEqual(check(archive), 0)
            self.assertEqual(builder.call_count, 2)
            verifier.assert_called_once()
            self.assertIn("Pristine offline cargo check passed in", summary.getvalue())
            self.assertIn("Modified axtb build passed in", summary.getvalue())
            self.assertIn("0.2.0+modified", summary.getvalue())
            self.assertIn("water energy verified", summary.getvalue())

    def test_check_refuses_bad_manifest_before_modification_or_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            root = temporary / "reshiki-fixture-source"
            root.mkdir()
            (root / "file").write_bytes(b"original")
            write_manifest(root, {})
            (root / "file").write_bytes(b"tampered")
            archive = temporary / "fixture.tar.gz"
            write_archive(root, archive)
            with (
                patch("xtb_source_archive.run") as builder,
                patch("xtb_source_archive.modify_driver_version") as modifier,
                self.assertRaisesRegex(ValueError, "Source manifest SHA-256 mismatch"),
            ):
                check(archive)
            modifier.assert_not_called()
            builder.assert_not_called()
            self.assertEqual(
                json.loads((root / MANIFEST).read_text())["files"]["file"],
                hashlib.sha256(b"original").hexdigest(),
            )


if __name__ == "__main__":
    unittest.main()
