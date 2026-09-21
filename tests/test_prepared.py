"""The transport preserves native state and never re-runs molecule preparation."""

import copy
import json
import unittest
from pathlib import Path
from unittest.mock import patch

from rdkit import Chem, RDConfig
from rdkit.Chem import rdCIPLabeler

from engine import prepared, worker
from tests.document_preparation_reference import drawing, prepare
from tests.perception_reference import snapshot


class PreparedMoleculeTests(unittest.TestCase):
    def test_transport_preserves_complete_states(self):
        root = Path(__file__).resolve().parents[1]
        texts = [t["smiles"] for t in json.loads((root / "assets/templates.json").read_text())]
        texts += ["[2H]O[3H]", "[CH3]", "[CH2]", "[13CH3:9][C@H](F)O", "F/C=C/Cl", "F/C=C\\Cl"]
        texts += [
            line.split()[0]
            for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
        ]
        for text in texts:
            with self.subTest(text=text):
                original = Chem.MolFromSmiles(text)
                if original is None or any(
                    a.GetNumRadicalElectrons() > 2 for a in original.GetAtoms()
                ):
                    continue
                doc = drawing(original)
                payload = json.loads(json.dumps(prepare(doc)))
                mol = prepared.restore(payload, doc)
                self.assertEqual(
                    json.loads(json.dumps(snapshot(mol, "symmetric"))), payload["state"]
                )
                self.assertEqual(
                    [int(a.GetProp("reshiki_id")) for a in mol.GetAtoms()], payload["ids"]
                )
                self.assertEqual(
                    [
                        dict(x=p.x, y=p.y, z=p.z)
                        for p in (
                            mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms())
                        )
                    ],
                    payload["positions"],
                )

    def test_analyze_and_exports_do_not_reprepare_or_silently_fallback(self):
        for text in ("C[C@H](N)C(=O)O", "F/C=C/Cl", "c1ccc2[nH]ccc2c1", "[CH3]", "[2H]O[3H]"):
            doc = worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
                "document"
            ]
            payload = json.loads(json.dumps(prepare(doc)))
            for operation, format in (
                ("analyze", None),
                ("export", "mol"),
                ("export", "smiles"),
                ("export", "inchi"),
            ):
                with self.subTest(text=text, operation=operation, format=format):
                    request = dict(protocol=1, operation=operation, document=doc, format=format)
                    expected = worker.handle(request)
                    request["prepared_molecule"] = payload
                    with (
                        patch.object(
                            worker,
                            "from_document",
                            side_effect=AssertionError("Native document preparation called"),
                        ),
                        patch.object(
                            Chem,
                            "SanitizeMol",
                            side_effect=AssertionError("Native sanitizer called"),
                        ),
                        patch.object(
                            Chem,
                            "AssignChiralTypesFromBondDirs",
                            side_effect=AssertionError("Native wedge perception called"),
                        ),
                        patch.object(
                            Chem,
                            "DetectBondStereochemistry",
                            side_effect=AssertionError("Native geometry perception called"),
                        ),
                        patch.object(
                            Chem,
                            "AssignStereochemistry",
                            side_effect=AssertionError("Native stereo perception called"),
                        ),
                    ):
                        self.assertEqual(worker.handle(request), expected)
                        invalid = copy.deepcopy(request)
                        invalid["prepared_molecule"]["rdkit_version"] = "wrong"
                        with self.assertRaisesRegex(ValueError, "version mismatch"):
                            worker.handle(invalid)

    def test_invalid_transport_is_rejected(self):
        doc = drawing(Chem.MolFromSmiles("CCO"))
        payload = json.loads(json.dumps(prepare(doc)))
        for kind in range(6):
            value = copy.deepcopy(payload)
            if kind == 0:
                value["ids"].reverse()
            elif kind == 1:
                value["state"]["graph"]["bonds"][0]["b"] = 100
            elif kind == 2:
                value["positions"][0]["x"] = float("nan")
            elif kind == 3:
                value["state"]["valences"][0]["implicit_hydrogens"] = 99
            elif kind == 4:
                value["state"]["metadata"]["atoms"].pop()
            else:
                value["state"]["properties"]["atoms"][0]["ring_members"] = [1]
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                prepared.restore(value, doc)

    def test_stereo_completion_is_a_computed_molecular_property(self):
        doc = drawing(Chem.MolFromSmiles("C[C@H](O)Cl"))
        payload = json.loads(json.dumps(prepare(doc)))
        mol = prepared.restore(payload, doc)
        self.assertTrue(mol.HasProp("_StereochemDone"))
        self.assertNotIn("_StereochemDone", mol.GetPropNames(True, False))
        mol.ClearComputedProps()
        self.assertFalse(mol.HasProp("_StereochemDone"))

    def test_prepared_drawing_skips_native_drawing_conversion(self):
        for text in (
            "N[C@H](C)C(=O)O",
            "F/C=C/F",
            "F/C=C\\F",
            "c1ccc2[nH]ccc2c1",
            "[13CH3:1][OH:9]",
        ):
            doc = worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
                "document"
            ]
            chemical = json.loads(json.dumps(prepare(doc)))
            work = worker.from_document(doc)
            Chem.Kekulize(work, clearAromaticFlags=True)
            Chem.WedgeMolBonds(work, work.GetConformer())
            for item in list(work.GetAtoms()) + list(work.GetBonds()):
                if item.HasProp("_CIPCode"):
                    item.ClearProp("_CIPCode")
            drawing = {**chemical, "state": json.loads(json.dumps(snapshot(work, "symmetric")))}
            rdCIPLabeler.AssignCIPLabels(work, maxRecursiveIterations=1_250_000)
            expected_labels = dict(
                rdkit_version=chemical["rdkit_version"],
                atoms=[
                    a.GetProp("_CIPCode") if a.HasProp("_CIPCode") else None
                    for a in work.GetAtoms()
                ],
                bonds=[
                    dict(
                        code=b.GetProp("_CIPCode") if b.HasProp("_CIPCode") else None,
                        stereo=int(b.GetStereo()),
                        stereo_atoms=list(b.GetStereoAtoms()),
                    )
                    for b in work.GetBonds()
                ],
            )
            for operation, format in (
                ("analyze", None),
                ("export", "smiles"),
                ("export", "mol"),
                ("export", "inchi"),
            ):
                with self.subTest(text=text, operation=operation, format=format):
                    request = dict(protocol=1, operation=operation, format=format, document=doc)
                    expected = worker.handle(request)
                    expected["document"] = (
                        None  # Rust assembles the drawing after receiving CIP metadata.
                    )
                    expected["drawing_labels"] = expected_labels
                    request.update(prepared_molecule=chemical, prepared_drawing=drawing)
                    with (
                        patch.object(
                            worker,
                            "from_document",
                            side_effect=AssertionError("Native preparation called"),
                        ),
                        patch.object(
                            worker,
                            "to_document",
                            side_effect=AssertionError("Native drawing conversion called"),
                        ),
                        patch.object(
                            Chem,
                            "Kekulize",
                            side_effect=AssertionError("Native drawing Kekulize called"),
                        ),
                        patch.object(
                            Chem,
                            "WedgeMolBonds",
                            side_effect=AssertionError("Native wedging called"),
                        ),
                    ):
                        self.assertEqual(worker.handle(request), expected)


if __name__ == "__main__":
    unittest.main()
