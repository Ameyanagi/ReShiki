"""Inno identity fixes must preserve the loader's resource table and appended payload."""

import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from installers import _pe_checksum, normalize_windows_installer_metadata

VERSION = "1.2.3-nightly.20261008.37815092244.1"


def version_block(key, value=b"", *, kind=1, children=()):
    block = bytearray(6) + (key + "\0").encode("utf-16le")
    block.extend(b"\0" * (-len(block) % 4))
    block.extend(value)
    for child in children:
        block.extend(b"\0" * (-len(block) % 4))
        block.extend(child)
    struct.pack_into("<HHH", block, 0, len(block), len(value) // (2 if kind else 1), kind)
    return bytes(block)


def installer_fixture(*, pe_plus=False, first_extra_allocation=0):
    # Mirror the pinned compiler's 60/50/20-character placeholders independently
    # of the production parser/encoder, including two resource languages and
    # two StringFileInfo tables in each resource.
    tables = []
    for language in ("040904b0", "041104b0"):
        values = {
            "CompanyName": "ReShiki contributors".ljust(60),
            "ProductName": "ReShiki".ljust(60),
            "ProductVersion": VERSION.ljust(50),
            "FileVersion": VERSION[:20],
            "PrivateBuild": "Preserve this unrelated value",
        }
        children = [
            version_block(key, (value + "\0").encode("utf-16le")) for key, value in values.items()
        ]
        tables.append(version_block(language, children=children))
    translation = version_block("Translation", struct.pack("<4H", 1033, 1200, 1041, 1200), kind=0)
    variable_info = version_block("VarFileInfo", children=[translation])
    fixed = struct.pack(
        "<13I", 0xFEEF04BD, 0x10000, 0x10002, 0x30000, 0x10002, 0x30000, 63, 0, 4, 1, 0, 0, 0
    )
    version_info = version_block(
        "VS_VERSION_INFO",
        fixed,
        kind=0,
        children=[version_block("StringFileInfo", children=tables), variable_info],
    )
    resource_rva, raw_offset = 0x1000, 0x200
    first_offset = 0x200
    first_size = len(version_info) + first_extra_allocation
    second_offset = (first_offset + first_size + 3) & ~3
    raw_size = (second_offset + len(version_info) + 511) & ~511
    resource = bytearray(raw_size)

    def directory(offset, entries):
        struct.pack_into("<IIHHHH", resource, offset, 0, 0, 0, 0, 0, len(entries))
        for index, entry in enumerate(entries):
            struct.pack_into("<II", resource, offset + 16 + index * 8, *entry)

    directory(0, [(10, 0x800000A0), (16, 0x80000040)])
    directory(0x40, [(1, 0x80000060)])
    directory(0x60, [(1033, 0x80), (1041, 0x90)])
    struct.pack_into("<4I", resource, 0x80, resource_rva + first_offset, first_size, 0, 0)
    struct.pack_into("<4I", resource, 0x90, resource_rva + second_offset, len(version_info), 0, 0)
    directory(0xA0, [(11111, 0x800000C0)])
    directory(0xC0, [(1033, 0xE0)])
    struct.pack_into("<4I", resource, 0xE0, resource_rva + 0x120, 64, 0, 0)
    resource[0x120:0x160] = bytes(range(64))
    resource[first_offset : first_offset + len(version_info)] = version_info
    resource[second_offset : second_offset + len(version_info)] = version_info
    header = bytearray(raw_offset)
    header[:2] = b"MZ"
    struct.pack_into("<I", header, 0x3C, 0x80)
    header[0x80:0x84] = b"PE\0\0"
    optional_size = 240 if pe_plus else 224
    struct.pack_into(
        "<HHIIIHH", header, 0x84, 0x8664 if pe_plus else 0x14C, 1, 0, 0, 0, optional_size, 2
    )
    optional = 0x98
    directory_start = 112 if pe_plus else 96
    struct.pack_into("<H", header, optional, 0x20B if pe_plus else 0x10B)
    struct.pack_into("<I", header, optional + directory_start - 4, 16)
    struct.pack_into("<II", header, optional + directory_start + 16, resource_rva, raw_size)
    section = optional + optional_size
    header[section : section + 8] = b".rsrc\0\0\0"
    struct.pack_into("<4I", header, section + 8, raw_size, resource_rva, raw_size, raw_offset)
    overlay = b"Inno loader appended payload must remain byte-identical\0" * 11
    data = header + resource + overlay
    return bytes(data), {
        "allocations": [
            (raw_offset + first_offset, first_size),
            (raw_offset + second_offset, len(version_info)),
        ],
        "checksum": optional + 64,
        "security": optional + directory_start + 32,
        "fixed": fixed,
        "translation": variable_info,
        "overlay_start": raw_offset + raw_size,
    }


def text_values(data, key):
    # Decode the node's declared UTF-16 value directly from its binary header;
    # use neither the production resource parser nor whitespace/NUL trimming.
    needle = (key + "\0").encode("utf-16le")
    values = []
    position = 0
    while (position := data.find(needle, position)) != -1:
        _, value_length, kind = struct.unpack_from("<HHH", data, position - 6)
        if kind != 1:
            raise AssertionError("Expected a UTF-16 string resource")
        value_start = (position + len(needle) + 3) & ~3
        values.append(data[value_start : value_start + value_length * 2].decode("utf-16le"))
        position += len(needle)
    return values


class WindowsInstallerMetadataTests(unittest.TestCase):
    def normalize(self, data, version=VERSION):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "fixture-setup.exe"
            target.write_bytes(data)
            normalize_windows_installer_metadata(target, version)
            return target.read_bytes()

    def test_exact_nightly_identity_in_every_language_preserves_other_bytes(self):
        for pe_plus in (False, True):
            with self.subTest(pe_plus=pe_plus):
                before, layout = installer_fixture(pe_plus=pe_plus)
                self.assertEqual(text_values(before, "FileVersion"), [VERSION[:20] + "\0"] * 4)
                after = self.normalize(before)
                self.assertEqual(len(after), len(before))
                self.assertEqual(text_values(after, "ProductName"), ["ReShiki\0"] * 4)
                for field in ("ProductVersion", "FileVersion"):
                    self.assertEqual(text_values(after, field), [VERSION + "\0"] * 4)
                self.assertEqual(
                    text_values(after, "CompanyName"), text_values(before, "CompanyName")
                )
                self.assertEqual(
                    text_values(after, "PrivateBuild"), text_values(before, "PrivateBuild")
                )
                self.assertEqual(after.count(layout["fixed"]), 2)
                self.assertEqual(after.count(layout["translation"]), 2)
                self.assertEqual(
                    after[layout["overlay_start"] :], before[layout["overlay_start"] :]
                )
                restored = bytearray(after)
                for offset, size in layout["allocations"]:
                    restored[offset : offset + size] = before[offset : offset + size]
                offset = layout["checksum"]
                restored[offset : offset + 4] = before[offset : offset + 4]
                self.assertEqual(restored, before)
                self.assertEqual(
                    struct.unpack_from("<I", after, offset)[0], _pe_checksum(after, offset)
                )
                for resource_offset, size in layout["allocations"]:
                    length = struct.unpack_from("<H", after, resource_offset)[0]
                    self.assertLessEqual(length, size)
                    self.assertEqual(
                        after[resource_offset + length : resource_offset + size],
                        bytes(size - length),
                    )

    def test_normalization_is_idempotent_and_handles_stable_versions(self):
        before, _layout = installer_fixture()
        normalized = self.normalize(before, version="1.2.3")
        self.assertEqual(text_values(normalized, "FileVersion"), ["1.2.3\0"] * 4)
        self.assertEqual(self.normalize(normalized, version="1.2.3"), normalized)

    def test_signed_malformed_mismatched_and_over_capacity_inputs_change_nothing(self):
        before, layout = installer_fixture()
        signed = bytearray(before)
        struct.pack_into("<II", signed, layout["security"], len(signed), 8)
        malformed = bytearray(before)
        struct.pack_into("<H", malformed, layout["allocations"][1][0], 65535)
        generous_first, _layout = installer_fixture(first_extra_allocation=4096)
        cases = [
            (signed, VERSION, "unsigned"),
            (malformed, VERSION, "Invalid Windows version resource"),
            (before, "2.2.3", "numeric versions"),
            (generous_first, "1.2.3-" + "x" * 600, "exceeds its resource allocation"),
        ]
        for data, version, message in cases:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as temporary:
                target = Path(temporary) / "fixture-setup.exe"
                target.write_bytes(data)
                with self.assertRaisesRegex(ValueError, message):
                    normalize_windows_installer_metadata(target, version)
                self.assertEqual(target.read_bytes(), data)

    def test_checksum_excludes_old_field_and_includes_odd_final_byte(self):
        data = bytearray(range(101))
        expected = 0
        for offset in range(0, len(data), 2):
            if 4 <= offset < 8:
                continue
            word = data[offset] + (data[offset + 1] * 256 if offset + 1 < len(data) else 0)
            expected += word
            expected = (expected & 0xFFFF) + (expected >> 16)
        expected = (expected & 0xFFFF) + (expected >> 16)
        self.assertEqual(_pe_checksum(data, 4), expected + len(data))


if __name__ == "__main__":
    unittest.main()
