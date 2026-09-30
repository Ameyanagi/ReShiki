"""Independent framing, input validation and heap checks of the Rust helper."""

import copy
import json
import os
import struct
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPER = (
    ROOT
    / "artifacts/inchi-helper"
    / ("reshiki-inchi-helper.exe" if os.name == "nt" else "reshiki-inchi-helper")
)


def request(text="InChI=1S/CH4/h1H4", budget=64 * 1024 * 1024):
    return dict(
        heap_bytes=budget,
        operation={"Read": dict(inchi=text, options=dict(sanitize=True, remove_hydrogens=False))},
    )


def frame(value, version=3, flags=0):
    body = (
        json.dumps(value, separators=(",", ":")).encode() if not isinstance(value, bytes) else value
    )
    return b"RSHINCHI" + struct.pack("<HHI", version, flags, len(body)) + body


class ProtocolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not HELPER.is_file():
            if os.environ.get("RESHIKI_REQUIRE_INCHI_HELPER"):
                raise RuntimeError("Build the Rust helper first")
            raise unittest.SkipTest("Optional Rust helper is not built")

    def run_frame(self, payload):
        result = subprocess.run([str(HELPER)], input=payload, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLessEqual(len(result.stdout), 8 * 1024 * 1024)
        self.assertEqual(result.stdout[:12], b"RSHINCHI\x03\x00\x00\x00")
        self.assertEqual(struct.unpack("<I", result.stdout[12:16])[0], len(result.stdout) - 16)
        response = json.loads(result.stdout[16:])
        self.assertEqual(response["version"], "1.07.5")
        return response["result"]

    def assert_rejected(self, payload):
        result = self.run_frame(payload)
        self.assertIsInstance(result["Err"], str)
        self.assertTrue(result["Err"])

    def test_generation_and_import_preserve_chemistry_status(self):
        imported = self.run_frame(frame(request()))["Ok"]["Imported"]
        self.assertEqual(imported["status"], 0)
        self.assertEqual(imported["state"]["graph"]["atoms"][0]["atomic_number"], 6)
        generated = self.run_frame(
            frame(
                dict(
                    heap_bytes=64 * 1024 * 1024,
                    operation={"Generate": dict(state=imported["state"], positions=None)},
                )
            )
        )["Ok"]["Generated"]
        self.assertEqual(generated["status"], 0)
        self.assertEqual(generated["inchi"], "InChI=1S/CH4/h1H4")
        self.assertTrue(generated["auxiliary"].startswith("AuxInfo="))
        for text in ("", "invalid", "InChI=1S/CH4/h1H4\0ignored"):
            imported = self.run_frame(frame(request(text)))["Ok"]["Imported"]
            self.assertEqual(imported["status"] in (0, 1), text.startswith("InChI="))

    def test_heap_limits_are_typed_repeatable_and_isolated(self):
        for budget in (1, 64, 128, 1024):
            outcomes = [
                subprocess.run(
                    [str(HELPER)],
                    input=frame(request(budget=budget)),
                    capture_output=True,
                    timeout=5,
                )
                for _ in range(3)
            ]
            self.assertEqual(len({r.stderr for r in outcomes}), 1)
            for result in outcomes:
                self.assertEqual(result.returncode, 75)
                prefix, actual, used, requested = result.stderr.decode().split()
                self.assertEqual(prefix, "RESHIKI_HEAP_LIMIT")
                self.assertEqual(int(actual), budget)
                self.assertLessEqual(int(used), budget)
                self.assertGreater(int(requested), 0)
        self.assertIn("Ok", self.run_frame(frame(request())))

    def test_truncation_trailing_bytes_and_protocol_versions(self):
        valid = frame(request())
        for size in range(len(valid)):
            self.assert_rejected(valid[:size])
        for payload in (
            valid + b"x",
            frame(request(), version=2),
            frame(request(), flags=1),
            b"BADMAGIC" + valid[8:],
            frame(b"{} trailing"),
            frame(b"\xff"),
            frame(b'{"heap_bytes":1,"heap_bytes":2}'),
        ):
            self.assert_rejected(payload)

    def test_unknown_fields_limits_and_invalid_molecules(self):
        for value in (
            request(budget=0),
            request(budget=512 * 1024 * 1024 + 1),
            request("x" * (2 * 1024 * 1024 + 1)),
            {**request(), "extra": True},
            dict(heap_bytes=1024, operation={"Unknown": {}}),
        ):
            self.assert_rejected(frame(value))
        state = self.run_frame(frame(request()))["Ok"]["Imported"]["state"]
        malformed = []
        for field, value in (("atomic_number", 255), ("explicit_hydrogens", 1000)):
            changed = copy.deepcopy(state)
            changed["graph"]["atoms"][0][field] = value
            malformed.append(changed)
        changed = copy.deepcopy(state)
        changed["valences"] = []
        malformed.append(changed)
        changed = copy.deepcopy(state)
        changed["graph"]["bonds"] = [dict(a=0, b=2, order=1, aromatic=False)]
        malformed.append(changed)
        for changed in malformed:
            self.assert_rejected(
                frame(
                    dict(
                        heap_bytes=64 * 1024 * 1024,
                        operation={"Generate": dict(state=changed, positions=None)},
                    )
                )
            )
        for positions in ([], [dict(x=float("nan"), y=0, z=0)], [dict(x=float("inf"), y=0, z=0)]):
            self.assert_rejected(
                frame(
                    dict(
                        heap_bytes=64 * 1024 * 1024,
                        operation={"Generate": dict(state=state, positions=positions)},
                    )
                )
            )


if __name__ == "__main__":
    unittest.main()
