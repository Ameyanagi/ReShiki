"""The transport preserves native state and never re-runs molecule preparation."""

import copy
import json
import unittest
from itertools import product
from pathlib import Path
from unittest.mock import patch

from rdkit import Chem, RDConfig
from rdkit.Chem import rdChemReactions, rdCIPLabeler, rdDepictor

from engine import prepared, worker
from tests.document_preparation_reference import drawing, prepare
from tests.molfile_import_reference import annotation_cases, native_molecules, read
from tests.perception_reference import snapshot
from tests.reaction_drawing_reference import expected as reaction_drawing


class PreparedMoleculeTests(unittest.TestCase):
    def test_local_cip_skips_native_assignment_and_checks_its_boundary(self):
        for text in ("F[C@](Cl)(Br)I", "F/C=C/F", "c1ccccc1", "[13CH3:4][NH3+]", ""):
            doc = drawing(Chem.MolFromSmiles(text))
            molecule = json.loads(json.dumps(prepare(doc)))
            for operation, fmt in (
                ("import", "mol"),
                ("import", "smiles"),
                ("analyze", None),
                ("finish_abbreviation", None),
                ("export", "mol"),
                ("export", "smiles"),
                ("export", "inchi"),
                ("export", "cdxml"),
                ("export", "cdx"),
            ):
                if not text and operation not in ("import", "finish_abbreviation"):
                    continue
                request = dict(
                    protocol=1,
                    operation=operation,
                    format=fmt,
                    document=doc,
                    prepared_molecule=molecule,
                    prepared_drawing=molecule,
                )
                if operation == "import":
                    request["prepared_import"] = dict(
                        is_3d=False,
                        attachment_points=[None] * len(doc["atoms"]),
                        dummy_labels=[None] * len(doc["atoms"]),
                    )
                if fmt in ("cdxml", "cdx"):
                    request["local_drawing_output"] = True
                expected = worker.handle(request)
                expected.pop("drawing_labels")
                request["local_cip"] = True
                with (
                    self.subTest(text=text, operation=operation, format=fmt),
                    patch.object(
                        prepared, "label_drawing", side_effect=AssertionError("Native labels")
                    ),
                    patch.object(
                        rdCIPLabeler, "AssignCIPLabels", side_effect=AssertionError("Native CIP")
                    ),
                    patch.object(worker, "to_document", side_effect=AssertionError("Native draw")),
                ):
                    before = copy.deepcopy(request)
                    self.assertEqual(worker.handle(request), expected)
                    self.assertEqual(request, before)
                    for value in (None, 1, 0, "true", [], {}):
                        with self.assertRaisesRegex(ValueError, "must be a boolean"):
                            worker.handle({**request, "local_cip": value})
                    for override in (
                        dict(prepared_molecule=None),
                        dict(prepared_drawing=None),
                        dict(operation="clean"),
                        dict(operation="aromatic"),
                        dict(operation="abbreviate"),
                        dict(operation="layout_import"),
                        dict(operation="export", format="rxn"),
                        dict(operation="import", format="inchi"),
                    ):
                        with self.assertRaises(ValueError):
                            worker.handle({**request, **override})
                    invalid = copy.deepcopy(request)
                    invalid["prepared_drawing"]["ids"].append(999)
                    with self.assertRaises(ValueError):
                        worker.handle(invalid)

    def test_reaction_layout_preserves_prepared_chemistry(self):
        for text in (
            "[H][C@](F)(Cl)Br>O>F/C=C/F",
            "C1CC1.N>O.CC>CCO",
            "[13CH3:0][NH3+]>N->[Cu]>c1ccccc1",
            "*C>>CO",
        ):
            with self.subTest(text=text):
                native = rdChemReactions.ReactionFromSmarts(text, useSmiles=True)
                inputs, positions = [], []
                next_id = 1
                for mol in [*native.GetReactants(), *native.GetAgents(), *native.GetProducts()]:
                    Chem.SanitizeMol(mol)
                    n = mol.GetNumAtoms()
                    before = snapshot(mol, "symmetric")
                    inputs.append(
                        dict(
                            molecule=dict(
                                rdkit_version=worker.rdBase.rdkitVersion,
                                ids=list(range(next_id, next_id + n)),
                                positions=[dict(x=0, y=0, z=0)] * n,
                                state=before,
                            ),
                            file=dict(
                                is_3d=False, attachment_points=[None] * n, dummy_labels=[None] * n
                            ),
                        )
                    )
                    next_id += n
                    rdDepictor.Compute2DCoords(mol)
                    self.assertEqual(snapshot(mol, "symmetric"), before)
                    positions.append(
                        [
                            dict(x=p.x, y=p.y, z=p.z)
                            for p in (mol.GetConformer().GetAtomPosition(i) for i in range(n))
                        ]
                    )
                inputs = json.loads(json.dumps(inputs))
                request = dict(
                    protocol=1, operation="layout_reaction", format="rsmi", prepared_parts=inputs
                )
                with (
                    patch.object(
                        rdChemReactions,
                        "ReactionFromSmarts",
                        side_effect=AssertionError("Native reaction read"),
                    ),
                    patch.object(
                        Chem, "MolFromSmiles", side_effect=AssertionError("Native SMILES read")
                    ),
                    patch.object(
                        Chem, "SanitizeMol", side_effect=AssertionError("Native sanitize")
                    ),
                    patch.object(
                        Chem, "AssignStereochemistry", side_effect=AssertionError("Native stereo")
                    ),
                    patch.object(
                        worker, "to_document", side_effect=AssertionError("Native drawing")
                    ),
                ):
                    self.assertEqual(
                        worker.handle(request),
                        dict(rdkit_version=worker.rdBase.rdkitVersion, positions=positions),
                    )
                    self.assertEqual(
                        worker.handle({**request, "prepared_parts": inputs[:1]})["positions"],
                        positions[:1],
                    )
                    for override in (
                        dict(prepared_parts=[]),
                        dict(prepared_parts=[inputs[0]] * 2),
                        dict(format="smiles"),
                        dict(prepared_molecule=inputs[0]["molecule"]),
                        dict(prepared_import=inputs[0]["file"]),
                        dict(document={}),
                        dict(prepared_reaction=True),
                        dict(protocol=2),
                    ):
                        with self.assertRaises(ValueError):
                            worker.handle({**request, **override})

                    n = len(inputs[0]["molecule"]["ids"])
                    for fields in (
                        None,
                        [],
                        {},
                        [None] * n,
                        [[[]]] * n,
                        [[[[], [256]]]] * n,
                        [[[[], [-1]]]] * n,
                        [[[[], [True]]]] * n,
                        [[["name", []]]] * n,
                        [[[[], [0] * (1024 * 1024)]]] * n,
                    ):
                        with self.assertRaises(ValueError):
                            worker.handle(
                                {
                                    **request,
                                    "prepared_parts": [{**inputs[0], "atom_properties": fields}],
                                }
                            )

    def test_reaction_import_bridge_only_labels_and_analyzes_prepared_graphs(self):
        for format, text in product(
            ("rxn", "rsmi"),
            (
                "CCO.O>O>CC=O.O",
                "F[C@](Cl)(Br)I>>F[C@@](Cl)(Br)I",
                "c1ccccc1>C[C@@H](N)C(=O)O>c1ccccc1O",
                "F/C=C/F>>F/C=C\\F",
            ),
        ):
            with self.subTest(text=text, format=format):
                reaction = rdChemReactions.ReactionFromSmarts(text, useSmiles=True)
                block = (
                    rdChemReactions.ReactionToV3KRxnBlock(reaction, separateAgents=True)
                    if format == "rxn"
                    else text
                )
                drawing = json.loads(json.dumps(reaction_drawing(block, format)))
                expected = worker.handle(
                    dict(protocol=1, operation="import", format=format, text=block)
                )
                analysis_graph = json.loads(json.dumps(prepare(drawing["document"])))
                label_request = dict(
                    protocol=1,
                    operation="label_reaction",
                    format=format,
                    prepared_parts=drawing["participants"],
                )
                analysis_request = dict(
                    protocol=1,
                    operation="import",
                    format=format,
                    text="Must not be reparsed",
                    prepared_reaction=True,
                    prepared_molecule=analysis_graph,
                    document=drawing["document"],
                )
                with (
                    patch.object(
                        rdChemReactions,
                        "ReactionFromSmarts",
                        side_effect=AssertionError("Native reaction read"),
                    ),
                    patch.object(
                        Chem, "MolFromSmiles", side_effect=AssertionError("Native SMILES read")
                    ),
                    patch.object(
                        rdChemReactions,
                        "ReactionFromRxnBlock",
                        side_effect=AssertionError("Native RXN read"),
                    ),
                    patch.object(
                        Chem, "MolFromMolBlock", side_effect=AssertionError("Native MOL read")
                    ),
                    patch.object(
                        Chem, "SanitizeMol", side_effect=AssertionError("Native sanitize")
                    ),
                    patch.object(
                        Chem, "AssignStereochemistry", side_effect=AssertionError("Native stereo")
                    ),
                    patch.object(Chem, "Kekulize", side_effect=AssertionError("Native Kekulé")),
                    patch.object(
                        Chem, "WedgeMolBonds", side_effect=AssertionError("Native wedges")
                    ),
                    patch.object(
                        rdDepictor, "Compute2DCoords", side_effect=AssertionError("Native layout")
                    ),
                    patch.object(
                        worker, "from_document", side_effect=AssertionError("Native preparation")
                    ),
                    patch.object(
                        worker, "to_document", side_effect=AssertionError("Native drawing")
                    ),
                ):
                    self.assertEqual(
                        worker.handle(label_request),
                        dict(rdkit_version=worker.rdBase.rdkitVersion, labels=drawing["labels"]),
                    )
                    expected["document"] = None
                    self.assertEqual(worker.handle(analysis_request), expected)
                    local_expected = copy.deepcopy(expected)
                    local_expected["analysis"]["smiles"] = ""
                    with patch.object(
                        Chem,
                        "MolToSmiles",
                        side_effect=AssertionError("Native reaction SMILES analysis"),
                    ):
                        self.assertEqual(
                            worker.handle({**analysis_request, "local_smiles": True}),
                            local_expected,
                        )
                    for override in (
                        dict(prepared_parts=None),
                        dict(prepared_parts=[]),
                        dict(prepared_parts={}),
                        dict(prepared_parts=[{}]),
                        dict(prepared_parts=[drawing["participants"][0]] * 2),
                        dict(prepared_parts=drawing["participants"] * 10001),
                        dict(operation="analyze"),
                        dict(format="mol"),
                        dict(prepared_reaction=True),
                        dict(prepared_molecule=analysis_graph),
                        dict(protocol=2),
                    ):
                        with self.assertRaises(ValueError):
                            worker.handle({**label_request, **override})
                    for override in (
                        dict(prepared_molecule=None),
                        dict(document=None),
                        dict(format="mol"),
                        dict(operation="analyze"),
                        dict(prepared_reaction="true"),
                        dict(prepared_drawing=analysis_graph),
                    ):
                        with self.assertRaises(ValueError):
                            worker.handle({**analysis_request, **override})

    def test_smiles_import_only_uses_native_layout_identifiers_and_full_cip(self):
        for text in (
            "CCO",
            "c1ccccc1",
            "F[C@](Cl)(Br)I",
            "[H][C@](F)(Cl)Br",
            "C[C@H]1CC[C@@H](C)CC1",
            "C/C=C/C",
            "F:O=C/F",
            "[2H]:C",
            "N->[Cu+2]",
            "[13CH3:90][NH3+]",
            "[13CH3:0][NH3+]",
            "C* |atomProp:1.dummyLabel.R1|",
            "CC |(nan,inf,-inf;1,0,)| named",
            "FC(Cl)(Br)I |(0,0,;1,0,;0,1,;-1,-1,;1,1,),wU:1.0|",
            "FC(Cl)(Br)I |(0,0,1;1,0,;0,1,;-1,-1,;1,1,)|",
        ):
            with self.subTest(text=text):
                original = Chem.MolFromSmiles(text)
                state = snapshot(original, "symmetric")
                expected = worker.handle(
                    dict(protocol=1, operation="import", format="smiles", text=text)
                )
                rdDepictor.Compute2DCoords(original)
                self.assertEqual(snapshot(original, "symmetric"), state)

                def payload(mol):
                    conf = mol.GetConformer()
                    return json.loads(
                        json.dumps(
                            dict(
                                rdkit_version=worker.rdBase.rdkitVersion,
                                ids=list(range(1, mol.GetNumAtoms() + 1)),
                                positions=[
                                    dict(x=p.x, y=p.y, z=p.z)
                                    for p in (
                                        conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms())
                                    )
                                ],
                                state=snapshot(mol, "symmetric"),
                            )
                        )
                    )

                reference = payload(original)
                work = Chem.Mol(original)
                Chem.Kekulize(work, clearAromaticFlags=True)
                Chem.WedgeMolBonds(work, work.GetConformer())
                drawing = payload(work)
                expected["document"] = None
                expected["drawing_labels"] = prepared.label_drawing(work)
                request = dict(
                    protocol=1,
                    operation="layout_import",
                    format="smiles",
                    text="Must not be reparsed",
                    prepared_molecule={
                        **reference,
                        "positions": [dict(x=0, y=0, z=0)] * original.GetNumAtoms(),
                    },
                    prepared_import=dict(
                        is_3d=False,
                        attachment_points=[None] * original.GetNumAtoms(),
                        dummy_labels=[
                            a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                            for a in original.GetAtoms()
                        ],
                    ),
                )
                with (
                    patch.object(Chem, "MolFromSmiles", side_effect=AssertionError("Native read")),
                    patch.object(Chem, "RemoveHs", side_effect=AssertionError("Native H removal")),
                    patch.object(
                        Chem, "SanitizeMol", side_effect=AssertionError("Native sanitize")
                    ),
                    patch.object(
                        Chem, "AssignStereochemistry", side_effect=AssertionError("Native stereo")
                    ),
                    patch.object(Chem, "Kekulize", side_effect=AssertionError("Native Kekulé")),
                    patch.object(
                        Chem, "WedgeMolBonds", side_effect=AssertionError("Native wedges")
                    ),
                    patch.object(
                        worker, "from_document", side_effect=AssertionError("Native prep")
                    ),
                    patch.object(worker, "to_document", side_effect=AssertionError("Native draw")),
                ):
                    layout = worker.handle(request)
                    self.assertEqual(layout["rdkit_version"], worker.rdBase.rdkitVersion)
                    self.assertEqual(layout["positions"], reference["positions"])
                    request["prepared_molecule"]["positions"] = layout["positions"]
                    for override in (
                        dict(prepared_import=None),
                        dict(prepared_molecule=None),
                        dict(prepared_drawing=drawing),
                        dict(format="mol"),
                        dict(operation="analyze"),
                        dict(protocol=2),
                    ):
                        with self.assertRaises(ValueError):
                            worker.handle({**request, **override})
                    request.update(operation="import", prepared_drawing=drawing)
                    with patch.object(
                        rdDepictor, "Compute2DCoords", side_effect=AssertionError("Repeated layout")
                    ):
                        self.assertEqual(worker.handle(request), expected)
                        request.pop("format")
                        self.assertEqual(worker.handle(request), expected)

    def test_prepared_mol_import_skips_native_reader_and_drawing_passes(self):
        for text in ("", "c1ccccc1", "F[C@](Cl)(Br)I", "C[C@H]1CC[C@@H](C)CC1"):
            original = Chem.MolFromMolBlock(
                Chem.MolToMolBlock(Chem.MolFromSmiles(text)), removeHs=False
            )
            block = Chem.MolToMolBlock(original)
            expected = worker.handle(dict(protocol=1, operation="import", format="mol", text=block))
            work = Chem.Mol(original)
            Chem.Kekulize(work, clearAromaticFlags=True)
            Chem.WedgeMolBonds(work, work.GetConformer())

            def payload(mol):
                conf = mol.GetConformer()
                return json.loads(
                    json.dumps(
                        dict(
                            rdkit_version=worker.rdBase.rdkitVersion,
                            ids=list(range(1, mol.GetNumAtoms() + 1)),
                            positions=[
                                dict(x=p.x, y=p.y, z=p.z)
                                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
                            ],
                            state=snapshot(mol, "symmetric"),
                        )
                    )
                )

            request = dict(
                protocol=1,
                operation="import",
                format="mol",
                text="Must not be reparsed",
                prepared_molecule=payload(original),
                prepared_drawing=payload(work),
                prepared_import=dict(
                    is_3d=original.GetConformer().Is3D(),
                    attachment_points=[None] * original.GetNumAtoms(),
                    dummy_labels=[None] * original.GetNumAtoms(),
                ),
            )
            expected["document"] = None
            expected["drawing_labels"] = prepared.label_drawing(work)
            with (
                patch.object(Chem, "MolFromMolBlock", side_effect=AssertionError("Native reader")),
                patch.object(
                    Chem, "AssignStereochemistry", side_effect=AssertionError("Native stereo")
                ),
                patch.object(Chem, "SanitizeMol", side_effect=AssertionError("Native sanitizer")),
                patch.object(Chem, "Kekulize", side_effect=AssertionError("Native Kekulé")),
                patch.object(Chem, "WedgeMolBonds", side_effect=AssertionError("Native wedges")),
                patch.object(
                    worker, "from_document", side_effect=AssertionError("Native preparation")
                ),
                patch.object(worker, "to_document", side_effect=AssertionError("Native drawing")),
            ):
                self.assertEqual(worker.handle(request), expected)
                for key in ("prepared_molecule", "prepared_drawing"):
                    with self.assertRaisesRegex(ValueError, "requires a molecular"):
                        worker.handle({**request, key: None})
                for value in (False, True, 1, "true"):
                    with self.assertRaisesRegex(ValueError, "requires a molecular"):
                        worker.handle({**request, "prepared_import": value})

    def test_import_transport_preserves_file_and_ring_stereo_annotations(self):
        counts = dict(cases=0, spatial=0, rings=0, attachments=0, labels=0)

        def verify(name, text):
            with self.subTest(name=name):
                original = read(text)
                conf = original.GetConformer()
                data = dict(
                    rdkit_version=worker.rdBase.rdkitVersion,
                    ids=list(range(1, original.GetNumAtoms() + 1)),
                    positions=[
                        dict(x=p.x, y=p.y, z=p.z)
                        for p in (conf.GetAtomPosition(i) for i in range(original.GetNumAtoms()))
                    ],
                    state=snapshot(original, "symmetric"),
                )
                file = dict(
                    is_3d=conf.Is3D(),
                    attachment_points=[
                        a.GetIntProp("molAttchpt") if a.HasProp("molAttchpt") else None
                        for a in original.GetAtoms()
                    ],
                    dummy_labels=[
                        a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                        for a in original.GetAtoms()
                    ],
                )
                counts["cases"] += 1
                counts["spatial"] += int(file["is_3d"])
                counts["rings"] += sum(a.HasProp("_ringStereoAtoms") for a in original.GetAtoms())
                counts["attachments"] += sum(a.HasProp("molAttchpt") for a in original.GetAtoms())
                counts["labels"] += sum(a.HasProp("dummyLabel") for a in original.GetAtoms())
                with (
                    patch.object(
                        Chem, "MolFromMolBlock", side_effect=AssertionError("Native reader")
                    ),
                    patch.object(
                        Chem, "AssignStereochemistry", side_effect=AssertionError("Native stereo")
                    ),
                    patch.object(
                        Chem, "SanitizeMol", side_effect=AssertionError("Native sanitizer")
                    ),
                ):
                    restored = prepared.restore(json.loads(json.dumps(data)), file=file)
                self.assertEqual(
                    json.loads(json.dumps(snapshot(restored, "symmetric"))),
                    json.loads(json.dumps(data["state"])),
                )
                self.assertEqual(restored.GetConformer().Is3D(), conf.Is3D())
                self.assertEqual(
                    list(restored.GetConformer().GetPositions().flat),
                    list(conf.GetPositions().flat),
                )
                self.assertEqual(
                    [a.GetSymbol() for a in restored.GetAtoms()],
                    [a.GetSymbol() for a in original.GetAtoms()],
                )
                self.assertEqual(Chem.MolToSmiles(restored), Chem.MolToSmiles(original))
                for a, b in zip(restored.GetAtoms(), original.GetAtoms(), strict=True):
                    self.assertEqual(a.HasProp("molAttchpt"), b.HasProp("molAttchpt"))
                    if b.HasProp("molAttchpt"):
                        self.assertEqual(a.GetIntProp("molAttchpt"), b.GetIntProp("molAttchpt"))

        annotation_cases(verify)
        for name, molecule in native_molecules():
            # Queries have an independent rejection test; they cannot be transported.
            if any(a.GetAtomicNum() == 0 for a in molecule.GetAtoms()):
                continue
            for v3000 in (False, True):
                verify(name, Chem.MolToMolBlock(molecule, forceV3000=v3000))
        from tests.molfile_import_reference import v3_block

        for label in ("R", "R1", "R#", "Pol", "Mod"):
            verify(label, v3_block([f"1 {label} 0 0 0 0"]))
        self.assertGreater(counts["cases"], 500)
        self.assertGreater(counts["rings"], 20)
        self.assertGreater(counts["spatial"], 20)
        self.assertGreater(counts["attachments"], 100)
        self.assertEqual(counts["labels"], 5)

    def test_ring_annotation_transport_rejects_invalid_members(self):
        mol = Chem.MolFromSmiles("CC")
        for members in (
            [],
            [None],
            [None, None, None],
            [[0], None],
            [[3], None],
            [[-(2**31)], None],
            [[True], None],
        ):
            with self.subTest(members=members), self.assertRaisesRegex(ValueError, "ring-stereo"):
                prepared._ring_annotations(mol, members)

    def test_prepared_editable_export_skips_all_native_drawing_writers(self):
        for text in ("c1ccccc1", "C[C@H](N)C(=O)O", "[13CH3:90][NH3+]", "N->[Cu+2]"):
            doc = worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
                "document"
            ]
            payload = json.loads(json.dumps(prepare(doc)))
            for fmt in ("cdxml", "cdx"):
                request = dict(protocol=1, operation="export", document=doc, format=fmt)
                expected = worker.handle(request)
                request.update(
                    prepared_molecule=payload, prepared_drawing=payload, local_drawing_output=True
                )
                with (
                    patch.object(
                        worker, "export_cdxml", side_effect=AssertionError("Native XML writer")
                    ),
                    patch.object(
                        worker, "to_cdx", side_effect=AssertionError("Native binary writer")
                    ),
                    patch.object(worker, "from_document", side_effect=AssertionError("Reprepared")),
                    patch.object(
                        worker, "to_document", side_effect=AssertionError("Reconstructed")
                    ),
                ):
                    actual = worker.handle(request)
                    self.assertIsNone(actual["output"])
                    self.assertIsNone(actual["document"])
                    self.assertEqual(actual["analysis"], expected["analysis"])
                    self.assertEqual(actual["warnings"], expected["warnings"])
                    self.assertIn("drawing_labels", actual)
                    for field in ("prepared_molecule", "prepared_drawing"):
                        with self.assertRaisesRegex(ValueError, "requires a prepared"):
                            worker.handle({**request, field: None})
                    for value in ("true", 1, None):
                        with self.assertRaisesRegex(ValueError, "must be a boolean"):
                            worker.handle({**request, "local_drawing_output": value})
        empty = dict(
            protocol=1,
            operation="export",
            format="cdxml",
            document=dict(atoms=[], bonds=[]),
            local_drawing_output=True,
        )
        with patch.object(
            worker, "export_cdxml", side_effect=AssertionError("Empty drawing writer")
        ):
            self.assertIsNone(worker.handle(empty)["output"])
        with self.assertRaisesRegex(ValueError, "requires a prepared"):
            worker.handle({**empty, "format": "smiles"})

    def test_prepared_smiles_skips_native_serialization(self):
        for text in (
            "c1ccccc1",
            "C[C@H](N)C(=O)O",
            "C[C@H]1CC[C@@H](C)CC1.O",
            "[13CH3:90][NH3+]",
            "N->[Cu+2]",
        ):
            doc = worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
                "document"
            ]
            payload = json.loads(json.dumps(prepare(doc)))
            for operation in ("analyze", "export", "finish_abbreviation"):
                request = dict(protocol=1, operation=operation, document=doc, format="smiles")
                expected = worker.handle(request)
                expected["analysis"]["smiles"] = ""
                if operation == "export":
                    expected["output"] = None
                request.update(prepared_molecule=payload, local_smiles=True)
                with patch.object(
                    Chem, "MolToSmiles", side_effect=AssertionError("Native SMILES writer")
                ):
                    self.assertEqual(worker.handle(request), expected)
                    for value in ("true", 1, None):
                        with self.assertRaisesRegex(ValueError, "must be a boolean"):
                            worker.handle({**request, "local_smiles": value})
                    with self.assertRaisesRegex(ValueError, "requires a prepared"):
                        worker.handle({**request, "prepared_molecule": None})
                    with self.assertRaisesRegex(ValueError, "requires a prepared"):
                        worker.handle({**request, "operation": "aromatic"})

    def test_prepared_mol_export_skips_native_writer(self):
        for text in ("c1ccccc1", "C[C@H](N)C(=O)O", "[13CH3:90][NH3+]", "N->[Cu+2]"):
            doc = worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
                "document"
            ]
            request = dict(protocol=1, operation="export", document=doc, format="mol")
            expected = worker.handle(request)
            expected["output"] = None
            request["prepared_molecule"] = json.loads(json.dumps(prepare(doc)))
            request["local_mol_output"] = True
            with patch.object(
                Chem, "MolToMolBlock", side_effect=AssertionError("Native MOL writer")
            ):
                self.assertEqual(worker.handle(request), expected)
                for value in ("true", 1, None):
                    with self.assertRaisesRegex(ValueError, "must be a boolean"):
                        worker.handle({**request, "local_mol_output": value})
                with self.assertRaisesRegex(ValueError, "requires a prepared"):
                    worker.handle({**request, "prepared_molecule": None})

    def test_aromatic_transport_only_analyzes_the_verified_state(self):
        for text in ("c1ccccc1", "c1cc[nH]c1", "c1ccc2ccccc2c1.CCO", "C[C@H](O)c1ccccc1"):
            doc = worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
                "document"
            ]
            for _ in range(2):
                request = dict(
                    protocol=1,
                    operation="aromatic",
                    document=doc,
                    selected_ids=[a["id"] for a in doc["atoms"]],
                )
                expected = worker.handle(request)
                changed = expected["document"]
                after = json.loads(json.dumps(prepare(changed)))
                request["prepared_aromatic"] = after
                request["local_smiles"] = True
                expected["document"] = None
                expected["analysis"]["smiles"] = ""
                with (
                    patch.object(
                        Chem, "MolToSmiles", side_effect=AssertionError("Native identity writer")
                    ),
                    patch.object(worker, "from_document", side_effect=AssertionError("Reprepared")),
                    patch.object(
                        worker, "to_document", side_effect=AssertionError("Reconstructed")
                    ),
                    patch.object(
                        worker.aromatic, "toggle", side_effect=AssertionError("Retoggled")
                    ),
                    patch.object(Chem, "Kekulize", side_effect=AssertionError("Kekulized")),
                    patch.object(Chem, "SanitizeMol", side_effect=AssertionError("Sanitized")),
                ):
                    self.assertEqual(worker.handle(request), expected)
                    invalid = copy.deepcopy(request)
                    invalid["prepared_aromatic"]["rdkit_version"] = "wrong"
                    with self.assertRaisesRegex(ValueError, "version mismatch"):
                        worker.handle(invalid)
                doc = changed

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
                ("finish_abbreviation", None),
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
                            worker.abbreviations,
                            "find",
                            side_effect=AssertionError("Native abbreviation matching called"),
                        ),
                        patch.object(
                            worker.abbreviations,
                            "replace",
                            side_effect=AssertionError("Native abbreviation replacement called"),
                        ),
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
                ("finish_abbreviation", None),
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
