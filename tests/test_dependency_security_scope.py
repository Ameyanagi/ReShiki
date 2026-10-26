import copy
import sys
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from check_dependency_security_scope import (
    bun_json,
    check,
    direct_http_cache_import,
    font_api_violations,
    http_cache_scope_failures,
    lru_scope_failures,
)


class DependencySecurityScopeTests(unittest.TestCase):
    def test_current_repository_keeps_reviewed_dependency_scope(self):
        self.assertEqual(check(ROOT), [])

    def test_pixel_operations_remain_allowed(self):
        self.assertEqual(
            font_api_violations(
                "from PIL import Image, ImageOps\nImage.open(data).convert('RGBA')"
            ),
            [],
        )

    def test_font_paths_require_explicit_reassessment(self):
        for source in (
            "from PIL import ImageFont as F\nF.truetype(path)",
            "import PIL.ImageDraw",
            "from PIL import _imagingft",
            "from rdkit.Chem import Draw\nDraw.MolToImage(molecule)",
            "from rdkit import Chem\nChem.Draw.MolToImage(molecule)",
            "import importlib as loader\nloader.import_module('PIL.ImageFont')",
            "from importlib import import_module as load\nload('PIL.ImageFont')",
            "__import__('rdkit.Chem.Draw')",
            "import PIL\ngetattr(PIL, 'ImageFont')",
        ):
            with self.subTest(source=source):
                self.assertTrue(font_api_violations(source))

    def test_lru_disposition_rejects_new_consumers_or_changed_key_source(self):
        cargo = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
        extra = copy.deepcopy(cargo)
        extra.append({"name": "other-consumer", "dependencies": ["lru"]})
        self.assertTrue(lru_scope_failures(extra))
        changed = copy.deepcopy(cargo)
        next(package for package in changed if package["name"] == "cosmic-text")["checksum"] = (
            "0" * 64
        )
        self.assertTrue(lru_scope_failures(changed))

    def test_http_cache_disposition_rejects_server_new_consumer_or_changed_source(self):
        packages = bun_json((ROOT / "bun.lock").read_text())["packages"]
        config = (ROOT / "website/astro.config.mjs").read_text()
        self.assertTrue(
            http_cache_scope_failures(
                packages, config.replace('output: "static"', 'output: "server"')
            )
        )
        extra = copy.deepcopy(packages)
        extra["other-consumer"] = [
            "other-consumer@1.0.0",
            "",
            {"dependencies": {"http-cache-semantics": "4.2.0"}},
        ]
        self.assertTrue(http_cache_scope_failures(extra, config))
        changed = copy.deepcopy(packages)
        changed["astro"][-1] = "changed-source"
        self.assertTrue(http_cache_scope_failures(changed, config))

    def test_lock_parser_preserves_string_contents_and_ignores_only_trailing_commas(self):
        self.assertEqual(bun_json('{"text": ", }", "array": [1,],}'), {"text": ", }", "array": [1]})

    def test_direct_shared_cache_import_requires_reassessment(self):
        for source in (
            'import Policy from "http-cache-semantics";',
            'const Policy = require("http-cache-semantics");',
            'const Policy = await import("http-cache-semantics");',
        ):
            self.assertTrue(direct_http_cache_import(source))
        self.assertFalse(direct_http_cache_import('const response = await fetch("/image.png");'))


if __name__ == "__main__":
    unittest.main()
