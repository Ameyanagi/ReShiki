"""Check restart arguments with stubs, without running an installer or GUI."""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class RestartArgumentsTests(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "Requires a POSIX shell")
    def test_shell_reopen_preserves_multiple_legacy_and_empty_paths(self):
        source = (ROOT / "src/updates/install.sh").read_text()
        # Execute only argument preparation and reopen(), with a shell stub.
        source = source.split('touch "$stage/ready"', 1)[0]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "installed app 資料"
            capture = root / "arguments"
            script = root / "reopen.sh"
            script.write_text(
                source.replace("/usr/bin/open", "record_arguments").replace(
                    '"$target/reshiki"', "record_arguments"
                )
                + '\nrecord_arguments() { printf "%s\\0" "$@" > "$RESHIKI_TEST_CAPTURE"; }\n'
                + '\nreopen "$@"\nwait\n'
            )
            paths = [
                str(root / "first drawing.rsk"),
                str(root / "構造 β.rsk"),
                str(root / "quotes ' \" $() ` ;.rsk"),
                str(root / "first drawing.rsk"),
            ]
            for platform in ("macos", "linux"):
                for drawings in ([], paths[:1], paths):
                    with self.subTest(platform=platform, drawings=drawings):
                        capture.unlink(missing_ok=True)
                        subprocess.run(
                            [
                                "/bin/sh",
                                str(script),
                                "123",
                                str(target),
                                "unused payload",
                                "unused stage",
                                drawings[0] if drawings else "",
                                platform,
                                *drawings[1:],
                            ],
                            env={**os.environ, "RESHIKI_TEST_CAPTURE": str(capture)},
                            check=True,
                            timeout=30,
                        )
                        expected = ["-n", str(target), "--args"] if platform == "macos" else []
                        expected += [arg for path in drawings for arg in ("--open", path)]
                        # printf emits a NUL even with no arguments.
                        raw = capture.read_bytes()
                        actual = raw[:-1].decode().split("\0") if raw != b"\0" else []
                        self.assertEqual(actual, expected)

    @unittest.skipUnless(
        shutil.which("pwsh") or shutil.which("powershell.exe"), "PowerShell unavailable"
    )
    def test_powershell_quotes_paths_and_accepts_the_legacy_form(self):
        executables = [
            executable for name in ("powershell.exe", "pwsh") if (executable := shutil.which(name))
        ]
        source = (ROOT / "src/updates/install.ps1").read_text()
        source = source.split("New-Item -ItemType File", 1)[0]
        with tempfile.TemporaryDirectory() as temporary:
            script = Path(temporary) / "arguments.ps1"
            script.write_text(
                source
                + "\n[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n"
                + "ConvertTo-Json -Compress -InputObject (Reopen-Arguments $drawings)\n"
            )
            paths = [r"C:\first drawing.rsk", r"C:\資料\構造 β.rsk", "C:\\trailing\\"]
            expected = [
                '--open "C:\\first drawing.rsk"',
                '--open "C:\\資料\\構造 β.rsk"',
                '--open "C:\\trailing\\\\"',
            ]
            for executable, count in (
                (executable, count) for executable in executables for count in (0, 1, len(paths))
            ):
                with self.subTest(executable=executable, count=count):
                    result = subprocess.run(
                        [
                            executable,
                            "-NoProfile",
                            "-NonInteractive",
                            "-File",
                            str(script),
                            "123",
                            "unused target",
                            "unused payload",
                            "unused stage",
                            paths[0] if count else "",
                            "windows",
                            *paths[1:count],
                        ],
                        check=True,
                        capture_output=True,
                        text=True,
                        encoding="utf-8",
                        timeout=10,
                    )
                    self.assertEqual(json.loads(result.stdout), " ".join(expected[:count]))


if __name__ == "__main__":
    unittest.main()
