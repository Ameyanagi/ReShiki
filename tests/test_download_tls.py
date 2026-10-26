"""Exercise the real CA-only context without network traffic or exploit inputs."""

import ast
import ssl
import subprocess
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import download_tls

ROOT = Path(__file__).resolve().parents[1]


class DownloadTLSTests(unittest.TestCase):
    def test_offline_packaging_imports_without_any_site_packages(self):
        result = subprocess.run(
            [
                sys.executable,
                "-I",
                "-S",
                "-c",
                "import sys; sys.path.insert(0, sys.argv[1]); "
                "import download_tls, inchi_source, build_release, sign_release, sign_windows_release",
                str(ROOT / "scripts"),
            ],
            capture_output=True,
            text=True,
            timeout=15,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_real_bundle_verifies_hostnames_without_adding_system_roots(self):
        with patch.object(ssl.SSLContext, "load_default_certs") as defaults:
            context = download_tls.verified_context()
        defaults.assert_not_called()
        self.assertEqual(context.verify_mode, ssl.CERT_REQUIRED)
        self.assertTrue(context.check_hostname)
        certificates = context.get_ca_certs()
        self.assertGreater(len(certificates), 50)
        names = {
            value
            for certificate in certificates
            for rdn in certificate["subject"]
            for key, value in rdn
            if key == "commonName"
        }
        self.assertNotIn("emSign ECC Root CA - G3", names)

    def test_old_unknown_or_missing_bundle_fails_closed(self):
        for version in ["2026.10.18", "2025.1.1", "unknown"]:
            with (
                patch.object(
                    download_tls.importlib,
                    "import_module",
                    return_value=SimpleNamespace(__version__=version),
                ),
                self.assertRaisesRegex(RuntimeError, "certifi>=2026.10.19"),
            ):
                download_tls.verified_context()
        with (
            patch.object(download_tls.importlib, "import_module", side_effect=ModuleNotFoundError),
            self.assertRaisesRegex(RuntimeError, "certifi>=2026.10.19"),
        ):
            download_tls.verified_context()

    def test_every_python_urlopen_uses_the_verified_context(self):
        found = set()
        for folder in [ROOT / "scripts", ROOT / "reference"]:
            for path in folder.rglob("*.py"):
                for call in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
                    if not isinstance(call, ast.Call):
                        continue
                    function = call.func
                    name = (
                        function.attr
                        if isinstance(function, ast.Attribute)
                        else function.id
                        if isinstance(function, ast.Name)
                        else ""
                    )
                    if name not in {"urlopen", "urlretrieve", "HTTPSConnection"}:
                        continue
                    found.add(path.relative_to(ROOT).as_posix())
                    context = next((k.value for k in call.keywords if k.arg == "context"), None)
                    self.assertIsInstance(context, ast.Call, str(path))
                    self.assertIsInstance(context.func, ast.Name, str(path))
                    self.assertEqual(context.func.id, "verified_context", str(path))
        self.assertEqual(
            found,
            {
                "scripts/inchi_source.py",
                "scripts/regenerate_cdx_schema.py",
                "scripts/regenerate_nmr.py",
                "scripts/setup_linux_depict_reference.py",
            },
        )


if __name__ == "__main__":
    unittest.main()
