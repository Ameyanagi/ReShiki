"""Saved Windows expectations must come from a known independent native capture."""

import gzip
import hashlib
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from depict_windows_profile import PHYSICAL_UCRT, PRIMITIVES, RINGS, classify

FIXTURES = Path(__file__).parent / "fixtures"
AUDIT = json.loads((FIXTURES / "depict-windows-server2022-profile.json").read_text())


class WindowsProfileTests(unittest.TestCase):
    def test_independent_python_and_native_witnesses_select_the_capture(self):
        for profile, evidence in (
            ("server2022", AUDIT["capture"]["crt"]),
            ("no-fma3", AUDIT["emulated_no_fma3"]["crt"]),
        ):
            with self.subTest(profile=profile):
                for field in ("module_sha256", "primitive_bits", "ring_bits"):
                    self.assertEqual(evidence["python"][field], evidence["native"][field])
                native = evidence["native"]
                self.assertEqual(
                    classify(
                        native["module_sha256"], native["primitive_bits"], native["ring_bits"]
                    ),
                    profile,
                )

    def test_both_explicit_physical_profiles_remain_available(self):
        for profile in ("fma3", "no-fma3"):
            self.assertEqual(classify(PHYSICAL_UCRT, PRIMITIVES[profile], RINGS[profile]), profile)

    def test_unmeasured_dll_or_dispatch_cannot_select_saved_expectations(self):
        witness = AUDIT["capture"]["crt"]["native"]
        self.assertEqual(
            classify("0" * 64, witness["primitive_bits"], witness["ring_bits"]), "uncaptured"
        )
        self.assertEqual(
            classify(witness["module_sha256"], PRIMITIVES["fma3"], witness["ring_bits"]),
            "uncaptured",
        )
        self.assertEqual(
            classify(witness["module_sha256"], witness["primitive_bits"], RINGS["no-fma3"]),
            "uncaptured",
        )

    def test_saved_rows_and_provenance_match_the_hosted_capture(self):
        for stage, evidence in AUDIT["stages"].items():
            with self.subTest(stage=stage):
                compressed = (FIXTURES / evidence["fixture"]).read_bytes()
                self.assertEqual(hashlib.sha256(compressed).hexdigest(), evidence["fixture_sha256"])
                raw = gzip.decompress(compressed)
                rows = raw.splitlines()
                self.assertEqual(len(rows) - 1, evidence["cases"])
                self.assertEqual(
                    hashlib.sha256(b"\n".join(rows[1:]) + b"\n").hexdigest(),
                    evidence["captured_rows_sha256"],
                )
                provenance = json.loads(rows[0])["provenance"]
                self.assertEqual(provenance["commit"], AUDIT["capture"]["source_commit"])
                if not evidence["reuses_existing_rows"]:
                    self.assertEqual(
                        hashlib.sha256(compressed).hexdigest(), evidence["captured_sha256"]
                    )
                    self.assertEqual(
                        hashlib.sha256(raw).hexdigest(), evidence["captured_jsonl_sha256"]
                    )


if __name__ == "__main__":
    unittest.main()
