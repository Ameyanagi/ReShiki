"""Target checks reject malformed and wrong-platform native helper headers."""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "inchi_builder", ROOT / "scripts/build_inchi_helper.py"
)
assert SPEC is not None and SPEC.loader is not None
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class TargetTests(unittest.TestCase):
    def check_header(self, data, target):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "helper"
            path.write_bytes(data)
            self.assertEqual(BUILDER.verify_executable(path, target), target)
            for other in BUILDER.SUPPORTED_TARGETS:
                if other != target:
                    with self.assertRaises(ValueError):
                        BUILDER.verify_executable(path, other)

    def test_all_supported_target_headers(self):
        for machine, target in ((62, "x86_64"), (183, "aarch64")):
            header = bytearray(64)
            header[:6] = b"\x7fELF\x02\x01"
            header[18:20] = machine.to_bytes(2, "little")
            self.check_header(header, target + "-unknown-linux-gnu")
        header = bytearray(64)
        header[:4] = b"\xcf\xfa\xed\xfe"
        header[4:8] = (0x100000C).to_bytes(4, "little")
        self.check_header(header, "aarch64-apple-darwin")
        for machine, target in ((0x8664, "x86_64"), (0xAA64, "aarch64")):
            header = bytearray(70)
            header[:2] = b"MZ"
            header[60:64] = (64).to_bytes(4, "little")
            header[64:68] = b"PE\0\0"
            header[68:70] = machine.to_bytes(2, "little")
            self.check_header(header, target + "-pc-windows-msvc")

    def test_malformed_and_unsupported_headers(self):
        for data in (b"", b"MZ", b"\x7fELF", b"\xcf\xfa\xed\xfe", b"MZ" + b"\xff" * 62):
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "helper"
                path.write_bytes(data)
                with self.assertRaises(ValueError):
                    BUILDER.verify_executable(path, None)


if __name__ == "__main__":
    unittest.main()
