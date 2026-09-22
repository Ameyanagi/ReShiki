"""Independent byte-level checks of the optional native process boundary."""

import math
import os
import struct
import subprocess
import unittest
from pathlib import Path

MAGIC = b"RSHINCHI"
ROOT = Path(__file__).resolve().parents[1]
HELPER = (
    ROOT
    / "artifacts/inchi-helper"
    / ("reshiki-inchi-helper.exe" if os.name == "nt" else "reshiki-inchi-helper")
)


def frame(body, *, version=1, operation=1, flags=0, length=None):
    return (
        MAGIC
        + struct.pack("<HBBI", version, operation, flags, len(body) if length is None else length)
        + body
    )


def carbon(*, x=0.0, element=b"C", neighbors=0):
    return struct.pack("<ddd6shb4bbB", x, 0.0, 0.0, element, 0, 0, -1, 0, 0, 0, 0, neighbors)


class ProtocolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not HELPER.is_file():
            if os.environ.get("RESHIKI_REQUIRE_INCHI_HELPER"):
                raise RuntimeError("Build the pinned native development helper first")
            raise unittest.SkipTest("Optional native development helper is not built")

    def run_frame(self, payload):
        result = subprocess.run([str(HELPER)], input=payload, capture_output=True, timeout=3)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLessEqual(len(result.stdout), 8 * 1024 * 1024)
        self.assertEqual(result.stdout[:10], MAGIC + struct.pack("<H", 1))
        version_size = struct.unpack_from("<I", result.stdout, 10)[0]
        self.assertEqual(result.stdout[14 : 14 + version_size], b"1.07.3")
        result_kind = struct.unpack_from("<H", result.stdout, 14 + version_size)[0]
        return result_kind, result.stdout[16 + version_size :]

    def assert_rejected(self, payload):
        kind, result = self.run_frame(payload)
        self.assertEqual(kind, 1)
        length = struct.unpack_from("<I", result)[0]
        self.assertGreater(length, 0)
        self.assertEqual(len(result), length + 4)

    def test_native_success_is_distinct_from_protocol_failure(self):
        kind, result = self.run_frame(frame(struct.pack("<HH", 1, 0) + carbon()))
        self.assertEqual(kind, 0)
        self.assertEqual(struct.unpack_from("<h", result)[0], 0)
        size = struct.unpack_from("<I", result, 2)[0]
        self.assertEqual(result[6 : 6 + size], b"InChI=1S/CH4/h1H4")

    def test_truncation_and_trailing_bytes(self):
        payload = frame(struct.pack("<HH", 1, 0) + carbon())
        for length in range(len(payload)):
            with self.subTest(length=length):
                self.assert_rejected(payload[:length])
        self.assert_rejected(payload + b"x")
        self.assert_rejected(frame(struct.pack("<HH", 1, 0) + carbon() + b"x"))

    def test_versions_counts_coordinates_and_record_validation(self):
        valid = struct.pack("<HH", 1, 0) + carbon()
        cases = [
            frame(valid, version=2),
            frame(valid, operation=2),
            frame(valid, flags=2),
            frame(b"", length=8 * 1024 * 1024),
            frame(b"", length=0xFFFFFFFF),
            frame(struct.pack("<HH", 32768, 0)),
            frame(struct.pack("<HH", 0, 32768)),
            b"BADMAGIC" + frame(valid)[8:],
        ]
        cases += [
            frame(struct.pack("<HH", 1, 0) + carbon(x=x)) for x in (math.nan, math.inf, -math.inf)
        ]
        cases += [
            frame(struct.pack("<HH", 1, 0) + carbon(element=e))
            for e in (b"", b"CCCCCC", b"C\0X", b"\xff", b"C1")
        ]
        cases += [frame(struct.pack("<HH", 1, 0) + carbon(neighbors=n)) for n in (21, 255)]
        cases += [
            frame(
                struct.pack("<HH", 1, 0)
                + carbon(neighbors=1)
                + struct.pack("<hbb", n, kind, stereo)
            )
            for n, kind, stereo in ((-1, 1, 0), (0, 1, 0), (1, 1, 0), (0, 4, 0), (0, 1, 2))
        ]
        cases += [
            frame(
                struct.pack("<HH", 2, 0)
                + carbon(neighbors=1)
                + struct.pack("<hbb", 1, kind, stereo)
                + carbon()
            )
            for kind, stereo in ((4, 0), (-1, 0), (1, 2), (1, -3))
        ]
        cases.append(
            frame(
                struct.pack("<HH", 2, 0)
                + carbon(neighbors=2)
                + struct.pack("<hbbhbb", 1, 1, 0, 1, 1, 0)
                + carbon()
            )
        )
        cases += [
            frame(
                struct.pack("<HH", 1, 1)
                + carbon()
                + struct.pack("<hhhhhbb", center, neighbor, 0, 0, 0, kind, parity)
            )
            for center, neighbor, kind, parity in (
                (-1, 0, 2, 1),
                (0, 0, 1, 1),
                (0, -1, 2, 1),
                (0, 2, 2, 1),
                (0, 0, 3, 1),
                (0, 0, 2, 4),
                (0, 0, 2, 0),
            )
        ]
        for index, payload in enumerate(cases):
            with self.subTest(case=index):
                self.assert_rejected(payload)


if __name__ == "__main__":
    unittest.main()
