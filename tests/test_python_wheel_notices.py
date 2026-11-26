"""Final wheels must match the generated license files and declare each one."""

import json
import sys
import tempfile
import unittest
import zipfile
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from check_python_wheel_notices import (
    AXTB_ATTRIBUTIONS,
    RELEASE_URL,
    main,
    release_version,
    verify_release_wheel,
    verify_wheel,
)

DEFAULT_SBOM = object()


class WheelFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.licenses = self.directory / "wheel-licenses"
        self.licenses.mkdir()
        self.files = {
            "LICENSE": b"Project terms\r\n",
            "THIRD-PARTY-NOTICES.txt": (
                b"SOURCE ATTRIBUTIONS\n-------------------\n"
                b"  licenses/liberation-fonts/LICENSE\n"
                b"  licenses/liberation-fonts/NOTICE\n"
                b"Dependency terms\n"
            ),
        }
        for name, content in self.files.items():
            (self.licenses / name).write_bytes(content)

    distribution = "reshiki"
    crate = "reshiki-python"
    dependencies = ("pyo3", "reshiki-io")

    def sbom(self, *dependencies):
        return {
            "metadata": {"component": {"name": self.crate}},
            "components": [{"type": "library", "name": name} for name in dependencies],
        }

    def wheel(self, *, files=None, declarations=None, sbom=DEFAULT_SBOM, sbom_path=None):
        files = self.files if files is None else files
        if sbom is DEFAULT_SBOM:
            sbom = self.sbom(*self.dependencies)
        declarations = self.files if declarations is None else declarations
        dist_info = f"{self.distribution}-1.0.dist-info"
        path = self.directory / f"{self.distribution}-1.0-cp310-abi3-any.whl"
        metadata = f"Metadata-Version: 2.4\nName: {self.distribution}\nVersion: 1.0\n"
        metadata += "".join(f"License-File: wheel-licenses/{name}\n" for name in declarations)
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(f"{dist_info}/METADATA", metadata + "\n")
            for name, content in files.items():
                archive.writestr(f"{dist_info}/licenses/wheel-licenses/{name}", content)
            if sbom is not None:
                archive.writestr(
                    sbom_path or f"{dist_info}/sboms/{self.crate}.cyclonedx.json",
                    json.dumps(sbom),
                )
        return path


class PythonWheelNoticeTests(WheelFixture):
    def test_matching_files_and_declarations_pass(self):
        verify_wheel(self.wheel(), self.licenses)

    def test_missing_generated_notices_are_rejected(self):
        (self.licenses / "THIRD-PARTY-NOTICES.txt").unlink()
        with self.assertRaisesRegex(ValueError, "No generated notices"):
            verify_wheel(self.wheel(files={}, declarations={}), self.licenses)

    def test_missing_bundled_font_attribution_is_rejected(self):
        name = "THIRD-PARTY-NOTICES.txt"
        for attribution in (
            "licenses/liberation-fonts/LICENSE",
            "licenses/liberation-fonts/NOTICE",
        ):
            content = self.files[name].replace(
                f"  {attribution}\n".encode(), f"Mentioned elsewhere: {attribution}\n".encode()
            )
            (self.licenses / name).write_bytes(content)
            with (
                self.subTest(attribution=attribution),
                self.assertRaisesRegex(ValueError, f"Missing source attribution: {attribution}"),
            ):
                verify_wheel(self.wheel(files={**self.files, name: content}), self.licenses)

    def test_missing_or_extra_file_is_rejected(self):
        for name in self.files:
            files = {key: content for key, content in self.files.items() if key != name}
            with (
                self.subTest(missing=name),
                self.assertRaisesRegex(ValueError, "License files differ"),
            ):
                verify_wheel(self.wheel(files=files), self.licenses)
        with self.assertRaisesRegex(ValueError, "License files differ"):
            verify_wheel(
                self.wheel(files={**self.files, "extra.txt": b"Extra terms"}), self.licenses
            )

    def test_missing_license_file_declaration_is_rejected(self):
        for name in self.files:
            declarations = [key for key in self.files if key != name]
            with (
                self.subTest(file=name),
                self.assertRaisesRegex(ValueError, "Missing License-File"),
            ):
                verify_wheel(self.wheel(declarations=declarations), self.licenses)

    def test_differing_bytes_are_rejected(self):
        for name, content in self.files.items():
            files = {**self.files, name: content + b"\n"}
            with self.subTest(file=name), self.assertRaisesRegex(ValueError, "differing bytes"):
                verify_wheel(self.wheel(files=files), self.licenses)

    def test_axtb_notices_are_rejected_in_the_base_wheel(self):
        name = "THIRD-PARTY-NOTICES.txt"
        content = self.files[name] + b"  licenses/axtb/SOURCE\n"
        (self.licenses / name).write_bytes(content)
        with self.assertRaisesRegex(ValueError, "base reshiki wheel must not contain"):
            verify_wheel(self.wheel(files={**self.files, name: content}), self.licenses)

    def test_engine_crates_in_the_sbom_are_rejected_in_the_base_wheel(self):
        with self.assertRaisesRegex(ValueError, "base reshiki wheel contains the xTB engine"):
            verify_wheel(self.wheel(sbom=self.sbom("pyo3", "axtb-driver")), self.licenses)

    def test_missing_sbom_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "no Rust SBOM"):
            verify_wheel(self.wheel(sbom=None), self.licenses)

    def test_sbom_requires_a_components_list(self):
        for sbom in ({}, {"components": None}, {"components": {}}, {"components": "axtb"}):
            with self.subTest(sbom=sbom), self.assertRaisesRegex(ValueError, "no Rust SBOM"):
                verify_wheel(self.wheel(sbom=sbom), self.licenses)

    def test_sbom_requires_a_nonempty_inventory(self):
        with self.assertRaisesRegex(ValueError, "no Rust SBOM component inventory"):
            verify_wheel(self.wheel(sbom=self.sbom()), self.licenses)

    def test_sbom_requires_the_wheel_crate(self):
        sbom = self.sbom(*self.dependencies)
        sbom["metadata"]["component"]["name"] = "reshiki-xtb-worker"
        with self.assertRaisesRegex(ValueError, "Rust SBOM must identify reshiki-python"):
            verify_wheel(self.wheel(sbom=sbom), self.licenses)

    def test_misplaced_sbom_is_rejected(self):
        for path in (
            "reshiki-python.cyclonedx.json",
            "other-1.0.dist-info/sboms/reshiki-python.cyclonedx.json",
            "reshiki-1.0.dist-info/sboms/other.cyclonedx.json",
        ):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "no Rust SBOM"):
                verify_wheel(self.wheel(sbom_path=path), self.licenses)


class XtbWheelNoticeTests(WheelFixture):
    distribution = "reshiki_xtb"
    crate = "reshiki-xtb-worker"
    dependencies = ("axtb", "axtb-driver", "faer")

    def setUp(self):
        super().setUp()
        self.files["THIRD-PARTY-NOTICES.txt"] = (
            "SOURCE ATTRIBUTIONS\n-------------------\n"
            + "".join(f"  {name}\n" for name in AXTB_ATTRIBUTIONS)
        ).encode()
        for name, content in self.files.items():
            (self.licenses / name).write_bytes(content)

    def test_matching_files_and_declarations_pass(self):
        verify_wheel(self.wheel(), self.licenses, xtb=True)

    def test_missing_axtb_attribution_is_rejected(self):
        name = "THIRD-PARTY-NOTICES.txt"
        for attribution in AXTB_ATTRIBUTIONS:
            content = self.files[name].replace(
                f"  {attribution}\n".encode(), f"Mentioned elsewhere: {attribution}\n".encode()
            )
            (self.licenses / name).write_bytes(content)
            with (
                self.subTest(attribution=attribution),
                self.assertRaisesRegex(ValueError, f"Missing source attribution: {attribution}"),
            ):
                verify_wheel(
                    self.wheel(files={**self.files, name: content}), self.licenses, xtb=True
                )

    def test_engine_wheel_sbom_must_list_the_engine(self):
        with self.assertRaisesRegex(ValueError, "SBOM does not list the axtb engine"):
            verify_wheel(self.wheel(sbom=self.sbom("faer")), self.licenses, xtb=True)

    def test_missing_sbom_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "no Rust SBOM"):
            verify_wheel(self.wheel(sbom=None), self.licenses, xtb=True)

    def test_sbom_requires_a_components_list(self):
        for sbom in ({}, {"components": None}, {"components": {}}, {"components": "axtb"}):
            with self.subTest(sbom=sbom), self.assertRaisesRegex(ValueError, "no Rust SBOM"):
                verify_wheel(self.wheel(sbom=sbom), self.licenses, xtb=True)

    def test_no_font_attribution_is_required(self):
        # Engine wheels do not need the drawing package's fonts.
        verify_wheel(self.wheel(), self.licenses, xtb=True)

    def test_axtb_notices_are_rejected_in_the_base_wheel(self):
        with self.assertRaisesRegex(ValueError, "Missing source attribution"):
            verify_wheel(self.wheel(), self.licenses)


class PythonWheelReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)

    def wheel(self, version="1.2.3", *, metadata_version=None, body=None, headers="", xtb=True):
        distribution = "reshiki_xtb" if xtb else "reshiki"
        path = self.directory / f"{distribution}-{version}-py3-none-any.whl"
        metadata_version = version if metadata_version is None else metadata_version
        if body is None:
            body = f"[Corresponding source]({RELEASE_URL}v1.2.3)." if xtb else ""
        metadata = (
            f"Metadata-Version: 2.4\nName: {distribution.replace('_', '-')}\n"
            f"Version: {metadata_version}\n{headers}\n{body}\n"
        )
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(f"{distribution}-{version}.dist-info/METADATA", metadata)
        return path

    def test_stable_and_prerelease_wheels_match_the_raw_release_tag(self):
        for tag, expected in (
            ("v1.2.3", "1.2.3"),
            ("v1.2.3-alpha.1", "1.2.3a1"),
            ("v1.2.3-beta.2", "1.2.3b2"),
            ("v1.2.3-rc.1", "1.2.3rc1"),
        ):
            with self.subTest(tag=tag):
                self.assertEqual(release_version(tag), expected)
                verify_release_wheel(
                    self.wheel(expected, body=f"[Source]({RELEASE_URL}{tag})."), tag
                )
                verify_release_wheel(self.wheel(expected, xtb=False), tag)

    def test_mismatched_filename_version_is_rejected(self):
        for xtb in (False, True):
            with (
                self.subTest(xtb=xtb),
                self.assertRaisesRegex(ValueError, "must have release version 1.2.4rc1"),
            ):
                verify_release_wheel(self.wheel(xtb=xtb), "v1.2.4-rc.1")

    def test_mismatched_or_duplicate_metadata_versions_are_rejected(self):
        for options in (
            {"metadata_version": "1.2.2"},
            {"headers": "Version: 1.2.3\n"},
        ):
            for xtb in (False, True):
                with (
                    self.subTest(options=options, xtb=xtb),
                    self.assertRaisesRegex(ValueError, "metadata must have release version 1.2.3"),
                ):
                    verify_release_wheel(self.wheel(**options, xtb=xtb), "v1.2.3")

    def test_source_link_must_name_only_the_exact_raw_tag(self):
        tag = "v1.2.3-rc.1"
        for body in (
            "Source is on GitHub.",
            f"[Source]({RELEASE_URL}v1.2.2).",
            f"[Source]({RELEASE_URL}v1.2.3rc1).",
            f"[Source]({RELEASE_URL}{tag}0).",
            f"[Source]({RELEASE_URL}{tag}). [Old source]({RELEASE_URL}v1.2.2).",
        ):
            with self.subTest(body=body), self.assertRaisesRegex(ValueError, "source links"):
                verify_release_wheel(self.wheel("1.2.3rc1", body=body), tag)

    def test_source_project_url_is_accepted_with_the_readme_template(self):
        verify_release_wheel(
            self.wheel(
                body=f"Release address: `{RELEASE_URL}v<version>`.",
                headers=f"Project-URL: Source, {RELEASE_URL}v1.2.3\n",
            ),
            "v1.2.3",
        )

    def test_engine_metadata_is_required(self):
        wheel = self.wheel()
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("reshiki_xtb-1.2.3.dist-info/METADATA", "Name: reshiki\n")
        with self.assertRaisesRegex(ValueError, "Expected reshiki-xtb metadata"):
            verify_release_wheel(wheel, "v1.2.3")
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("README.md", "No metadata")
        with self.assertRaisesRegex(ValueError, "Expected one wheel METADATA"):
            verify_release_wheel(wheel, "v1.2.3")

    def test_unsupported_release_tags_are_rejected(self):
        for tag in ("1.2.3", "v01.2.3", "v1.2.3-unrelated", "v1.2.3/extra"):
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, "Unsupported"):
                release_version(tag)

    def test_release_cli_checks_every_candidate_and_requires_a_match(self):
        for xtb, distribution in ((False, "reshiki"), (True, "reshiki_xtb")):
            self.wheel(xtb=xtb)
            pattern = str(self.directory / f"{distribution}-*.whl")
            with (
                self.subTest(distribution=distribution),
                patch.object(sys, "argv", ["check", "--release-tag", "v1.2.3", pattern]),
                redirect_stdout(StringIO()) as output,
            ):
                main()
                self.assertIn("Verified release version", output.getvalue())
                self.wheel("1.2.2", xtb=xtb)
                with self.assertRaisesRegex(ValueError, "must have release version 1.2.3"):
                    main()
        missing = str(self.directory / "missing-*.whl")
        with (
            patch.object(sys, "argv", ["check", "--release-tag", "v1.2.3", missing]),
            redirect_stderr(StringIO()) as output,
            self.assertRaises(SystemExit) as error,
        ):
            main()
        self.assertEqual(error.exception.code, 2)
        self.assertIn("No wheel files matched", output.getvalue())

    def test_release_cli_rejects_an_empty_tag(self):
        wheel = self.wheel(xtb=False)
        with (
            patch.object(sys, "argv", ["check", "--release-tag", "", str(wheel)]),
            self.assertRaisesRegex(ValueError, "Unsupported Python release tag"),
        ):
            main()


if __name__ == "__main__":
    unittest.main()
