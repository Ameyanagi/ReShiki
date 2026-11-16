"""Independent source framing and regression-report checks for ChemDraw fixtures."""

import copy
import hashlib
import importlib.util
import json
import struct
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


generator = module("chemdraw_corpus_generator", ROOT / "scripts/generate_chemdraw_import_corpus.py")
runner = module("chemdraw_corpus_runner", ROOT / "scripts/check_chemdraw_import_corpus.py")


def binary_tree(data):
    """Read CDX object/property framing without calling either application codec."""
    if data[:12] != b"VjCD0100\x04\x03\x02\x01":
        raise ValueError("Invalid CDX signature")
    offset = 22 if data[22:24] == b"\x00\x80" else 28

    def word():
        nonlocal offset
        value = struct.unpack_from("<H", data, offset)[0]
        offset += 2
        return value

    def object_body(tag):
        nonlocal offset
        identifier = struct.unpack_from("<I", data, offset)[0]
        offset += 4
        properties, children = {}, []
        while code := word():
            if code >= 0x8000:
                children.append(object_body(code))
            else:
                length = word()
                if length == 0xFFFF:
                    length = struct.unpack_from("<I", data, offset)[0]
                    offset += 4
                if offset + length > len(data):
                    raise ValueError("Truncated property")
                properties.setdefault(code, []).append(data[offset : offset + length])
                offset += length
        return tag, identifier, properties, children

    root = object_body(word())
    if data[offset:] not in (b"", b"\0\0"):
        raise ValueError("Unexpected binary suffix")
    return root


def object_inventory(tree):
    tags = {
        0x8000: "CDXML",
        0x8001: "page",
        0x8002: "group",
        0x8003: "fragment",
        0x8004: "n",
        0x8005: "b",
        0x8006: "t",
        0x8007: "graphic",
        0x8008: "curve",
        0x8009: "embeddedobject",
        0x800D: "scheme",
        0x800E: "step",
        0x8017: "bracketedgroup",
        0x8018: "bracketattachment",
        0x8019: "crossingbond",
        0x8021: "arrow",
    }
    result = Counter({tags[tree[0]]: 1})
    for child in tree[3]:
        result.update(object_inventory(child))
    return result


class CorpusGenerationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.outputs = generator.generate(generator.DEFAULT_SEED)
        cls.manifest = json.loads(cls.outputs["manifest.json"])

    def test_reproduction_matches_every_checked_in_byte(self):
        self.assertEqual(self.outputs, generator.generate(generator.DEFAULT_SEED))
        for name, data in self.outputs.items():
            with self.subTest(name=name):
                self.assertEqual(data, (generator.DESTINATION / name).read_bytes())

    def test_original_inputs_and_historical_failures_are_retained(self):
        digest = hashlib.sha256()
        for case in self.manifest["cases"][:104]:
            for kind, name in sorted(case["files"].items()):
                if name and kind != "cds":
                    digest.update(name.encode() + b"\0" + self.outputs[name])
        self.assertEqual(
            digest.hexdigest(), "1002b1420f1845193f5f1c1178a4508de2e0ca4852a8a7543bb0846702fa80c3"
        )
        fixed = [case for case in self.manifest["cases"] if case.get("historical_failure")]
        self.assertEqual(len(fixed), 10)
        for case in fixed:
            self.assertEqual(case["expected_import"]["outcome"], "accept")
            self.assertTrue(case["files"]["cdxml"].startswith("unsupported/"))

    def test_full_cds_preserves_inventory_text_defaults_and_native_arrows(self):
        for case in self.manifest["cases"]:
            if not case["files"]["cds"] or case["id"].endswith("truncated-syntax"):
                continue
            with self.subTest(case=case["id"]):
                source = ET.fromstring(self.outputs[case["files"]["cdxml"]])
                data = self.outputs[case["files"]["cds"]]
                tree = binary_tree(data)
                expected = Counter(
                    element.tag
                    for element in source.iter()
                    if element.tag not in {"fonttable", "font", "colortable", "color", "s"}
                )
                self.assertEqual(object_inventory(tree), expected)
                # Decode semantic text with the reference codec, separate from
                # this framing oracle and the production importer/exporter.
                decoded = ET.fromstring(generator.codec.from_cdx(data))
                self.assertEqual(
                    [span.text for span in decoded.iter("s")],
                    [span.text for span in source.iter("s")],
                )
                properties = {}

                def collect_properties(node):
                    properties[node[1]] = node[2]
                    for child in node[3]:
                        collect_properties(child)

                collect_properties(tree)
                geometry = {
                    "p": 0x0200,
                    "BoundingBox": 0x0204,
                    "BoundsInParent": 0x0206,
                    "Head3D": 0x0207,
                    "Tail3D": 0x0208,
                    "Center3D": 0x020D,
                    "MajorAxisEnd3D": 0x020E,
                    "MinorAxisEnd3D": 0x020F,
                    "CurvePoints": 0x0A23,
                    "BondLength": 0x0805,
                    "BoldWidth": 0x0806,
                    "LineWidth": 0x0807,
                    "MarginWidth": 0x0808,
                    "HashSpacing": 0x0809,
                }
                for element in source.iter():
                    for attribute in element.attrib.keys() & geometry:
                        payload = properties[int(element.get("id", "0"))][geometry[attribute]][0]
                        if attribute == "CurvePoints":
                            payload = payload[2:]
                        restored = [
                            value / 65536
                            for value in struct.unpack("<" + "i" * (len(payload) // 4), payload)
                        ]
                        if attribute in ("p", "CurvePoints"):
                            restored = [
                                value
                                for pair in zip(restored[1::2], restored[::2], strict=True)
                                for value in pair
                            ]
                        elif attribute in ("BoundingBox", "BoundsInParent"):
                            restored = [restored[index] for index in (1, 0, 3, 2)]
                        for original, binary in zip(
                            element.get(attribute).split(),
                            restored,
                            strict=True,
                        ):
                            self.assertAlmostEqual(float(original), binary, delta=1 / 65536)
                for prefix, tag in [("Label", 0x080A), ("Caption", 0x080B)]:
                    self.assertEqual(len(tree[2][tag]), 1)
                    defaults = struct.unpack("<HHHH", tree[2][tag][0])
                    self.assertEqual(
                        defaults,
                        tuple(
                            round(float(source.get(prefix + suffix)) * scale)
                            for suffix, scale in [
                                ("Font", 1),
                                ("Face", 1),
                                ("Size", 20),
                                ("Color", 1),
                            ]
                        ),
                    )

    def test_alpha_limitation_is_explicit_and_style_controls_stay_separate(self):
        alpha = next(case for case in self.manifest["cases"] if "translucent" in case["id"])
        self.assertIsNone(alpha["files"]["cdx"])
        self.assertIsNone(alpha["files"]["cds"])
        self.assertTrue(alpha["binary_encoder_limitations"])
        self.assertIn(b'alpha="0.5"', self.outputs[alpha["files"]["cdxml"]])
        for case in self.manifest["cases"]:
            if "style_cds" in case["files"]:
                style = binary_tree(self.outputs[case["files"]["style_cds"]])
                self.assertEqual(object_inventory(style), {"CDXML": 1, "page": 1})
                self.assertGreater(
                    object_inventory(binary_tree(self.outputs[case["files"]["cds"]]))["n"], 0
                )

    def test_fresh_mixes_are_reproducible_without_changing_existing_cases(self):
        fresh = generator.generate(72317, 4)
        self.assertEqual(fresh, generator.generate(72317, 4))
        manifest = json.loads(fresh["manifest.json"])
        self.assertEqual(manifest["case_count"], 108)
        base = generator.generate(72317, 0)
        for case in json.loads(base["manifest.json"])["cases"]:
            for name in case["files"].values():
                if name:
                    self.assertEqual(base[name], fresh[name])
        for case in manifest["cases"][104:]:
            self.assertIsNotNone(case["files"]["cds"])
            self.assertGreater(case["source_counts"]["scheme"], 1)
            self.assertGreater(case["source_counts"]["group"], 2)
            source = ET.fromstring(fresh[case["files"]["cdxml"]])
            self.assertNotEqual(source.get("BondLength"), "14.4")

    def test_regeneration_refuses_foreign_files_without_writing(self):
        for foreign in ["unrelated.txt", "supported/001-molecule-chain.cdxml"]:
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / foreign
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"preserve this input")
                result = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "scripts/generate_chemdraw_import_corpus.py"),
                        "--output",
                        directory,
                    ],
                    capture_output=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(path.read_bytes(), b"preserve this input")
                self.assertFalse((Path(directory) / "manifest.json").exists())

    def test_semantic_cds_retains_original_sources_and_unspecified_text_runs(self):
        directory = ROOT / "tests/fixtures/chemdraw-import-semantics/inputs"
        digest = hashlib.sha256()
        for path in sorted(directory.iterdir()):
            if path.suffix in (".cdxml", ".cdx"):
                digest.update(path.name.encode() + b"\0" + path.read_bytes())
        self.assertEqual(
            digest.hexdigest(), "3d74bb9af41b91c0d6efc7069e1366e701b271066ea8f5e2867ce12071ad2cc4"
        )
        for path in directory.glob("*.cdxml"):
            with self.subTest(case=path.stem):
                source = ET.fromstring(path.read_bytes())
                data = path.with_suffix(".cds").read_bytes()
                tree = binary_tree(data)
                self.assertEqual(
                    object_inventory(tree),
                    Counter(
                        element.tag
                        for element in source.iter()
                        if element.tag not in {"fonttable", "font", "colortable", "color", "s"}
                    ),
                )
                decoded = ET.fromstring(generator.codec.from_cdx(data))
                self.assertEqual(
                    [span.text for span in source.iter("s")],
                    [span.text for span in decoded.iter("s")],
                )
                for prefix, tag in [("Label", 0x080A), ("Caption", 0x080B)]:
                    font, face, size, _ = struct.unpack("<HHHH", tree[2][tag][0])
                    self.assertEqual(
                        (font, face, size),
                        (
                            int(source.get(prefix + "Font")),
                            int(source.get(prefix + "Face")),
                            round(float(source.get(prefix + "Size")) * 20),
                        ),
                    )
                for source_span, decoded_span in zip(
                    source.iter("s"), decoded.iter("s"), strict=True
                ):
                    if not source_span.attrib:
                        self.assertFalse(
                            decoded_span.attrib,
                            "default font provenance became an explicit text style",
                        )


class DrawingAssessmentTests(unittest.TestCase):
    def test_semantics_ignore_ids_order_and_translation_but_detect_real_changes(self):
        before = {
            "atoms": [
                {"id": 9, "element": "C", "position": {"x": 30, "y": 40}},
                {"id": 4, "element": "O", "position": {"x": 50, "y": 40}},
            ],
            "bonds": [{"a": 9, "b": 4, "order": 1}],
            "generic": {"links": [{"atom": 4, "min": 1, "max": 3}]},
        }
        after = copy.deepcopy(before)
        after["atoms"].reverse()
        for atom in after["atoms"]:
            atom["id"] += 100
            atom["position"]["x"] += 170
            atom["position"]["y"] -= 25
        after["bonds"][0].update(a=109, b=104)
        after["generic"]["links"][0]["atom"] = 104
        self.assertIsNone(
            runner.first_difference(
                runner.drawing_semantics(before), runner.drawing_semantics(after)
            )
        )
        self.assertIsNone(runner.drawing_semantic_difference(before, after))
        after["generic"]["links"][0]["max"] = 4
        self.assertIsNotNone(
            runner.first_difference(
                runner.drawing_semantics(before), runner.drawing_semantics(after)
            )
        )
        after["generic"]["links"][0]["max"] = 3
        after["atoms"][0]["position"]["x"] += 2
        self.assertIsNotNone(runner.drawing_semantic_difference(before, after))

    def test_atom_matching_tolerates_rounding_boundaries_without_hiding_element_changes(self):
        before = {
            "atoms": [
                {"id": 1, "element": "C", "position": {"x": 0, "y": 0}},
                {"id": 2, "element": "O", "position": {"x": 10.04999, "y": 40}},
                {"id": 3, "element": "C", "position": {"x": 10.05001, "y": 20}},
            ]
        }
        after = copy.deepcopy(before)
        after["atoms"][1]["position"]["x"] += 0.00003
        after["atoms"].reverse()
        self.assertIsNone(runner.drawing_semantic_difference(before, after))
        after["atoms"][0]["element"] = "N"
        self.assertIsNotNone(runner.drawing_semantic_difference(before, after))

    def test_expected_negative_controls_and_export_write_errors(self):
        case = {"expected_import": {"outcome": "reject"}}
        self.assertEqual(runner.assess(case, "cds", {"import": {"status": "error"}}), [])
        result = {
            "mode": "style",
            "style": {
                "import": {"status": "ok", "style": {}},
                "export": {"status": "ok", "write_error": "disk full"},
            },
        }
        findings = runner.assess({}, "style_cds", result)
        self.assertEqual(findings[0]["kind"], "style_export_rejected")

    def test_atomless_captions_and_arrows_allow_translation_but_keep_relative_geometry(self):
        before = {
            "annotations": [
                {"id": 1, "text": "conditions", "position": {"x": 25, "y": 10}, "format": {}}
            ],
            "arrows": [
                {
                    "id": 2,
                    "start": {"x": 20, "y": 30},
                    "end": {"x": 100, "y": 30},
                    "kind": "forward",
                }
            ],
        }
        after = copy.deepcopy(before)
        for point in [
            after["annotations"][0]["position"],
            after["arrows"][0]["start"],
            after["arrows"][0]["end"],
        ]:
            point["x"] += 80
            point["y"] -= 50
        self.assertIsNone(runner.drawing_semantic_difference(before, after))
        after["annotations"][0]["position"]["y"] += 4
        self.assertIsNotNone(runner.drawing_semantic_difference(before, after))
        after["annotations"][0]["position"]["y"] -= 4
        after["arrows"][0]["end"]["x"] += 4
        self.assertIsNotNone(runner.drawing_semantic_difference(before, after))


if __name__ == "__main__":
    unittest.main()
