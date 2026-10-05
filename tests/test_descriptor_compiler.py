"""The descriptor generator must preserve the committed query AST and error boundaries."""

import copy
import json
import sys
import unittest
from pathlib import Path

from rdkit import rdBase

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from regenerate_descriptor_data import VERSION, Compiler


class DescriptorCompilerTests(unittest.TestCase):
    def test_committed_crippen_rules_replay_recursive_pattern_order_exactly(self):
        expected = json.loads((ROOT / "crates/chemistry/src/descriptor_data.json").read_text())
        compiler = Compiler()
        with rdBase.BlockLogs():
            for rule in expected["crippen"]:
                with self.subTest(label=rule["label"], smarts=rule["smarts"]):
                    self.assertEqual(compiler.compile(rule["smarts"]), rule["pattern"])
        self.assertEqual(compiler.patterns, expected["patterns"][: len(compiler.patterns)])

    def test_nested_negation_preserves_child_order_values_and_input(self):
        node = {
            "descr": "AtomAnd",
            "children": [
                {"descr": "AtomAtomicNum", "val": 6},
                {"descr": "AtomHCount", "val": 0, "negated": True},
                {"descr": "AtomNull"},
            ],
            "negated": True,
        }
        original = copy.deepcopy(node)
        compiler = Compiler()
        self.assertEqual(
            compiler.expr(node),
            {
                "op": "not",
                "children": [
                    {
                        "op": "all",
                        "children": [
                            {"op": "number", "value": 6},
                            {"op": "not", "children": [{"op": "hydrogens", "value": 0}]},
                            {"op": "always"},
                        ],
                    }
                ],
            },
        )
        self.assertEqual(node, original)
        self.assertEqual(compiler.patterns, [])

    def test_unsupported_query_and_encoding_errors_preserve_partial_pattern_state(self):
        compiler = Compiler()
        unknown = {"descr": "UnsupportedQuery", "val": 2}
        with self.assertRaises(ValueError) as error:
            compiler.expr(unknown)
        self.assertEqual(str(error.exception), f"Unsupported query: {unknown}")
        for version, toolkit in ((9, VERSION), (10, "another-version")):
            with self.subTest(version=version, toolkit=toolkit):
                molecule = {
                    "extensions": [
                        {
                            "name": "rdkitQueries",
                            "formatVersion": version,
                            "toolkitVersion": toolkit,
                        }
                    ]
                }
                with self.assertRaises(ValueError) as error:
                    compiler.molecule(molecule)
                self.assertEqual(str(error.exception), "Unexpected query encoding")
        with rdBase.BlockLogs(), self.assertRaises(ValueError) as error:
            compiler.compile("[")
        self.assertEqual(str(error.exception), "Invalid source SMARTS: [")
        self.assertEqual(compiler.patterns, [])


if __name__ == "__main__":
    unittest.main()
