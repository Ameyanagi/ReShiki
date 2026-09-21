"""Original reaction canvas assembly and native drawing labels, without Rust."""

import json
import random
import sys
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdChemReactions, rdCIPLabeler, rdDepictor

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import reactions
from engine.worker import check_supported, to_document

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .reaction_import_reference import cases as parser_cases
    from .reaction_import_reference import query_cases
else:
    from perception_reference import snapshot
    from reaction_import_reference import cases as parser_cases
    from reaction_import_reference import query_cases


def expected(text):
    participants, labels = [], []

    def draw(mol):
        if mol.GetNumAtoms():
            work = Chem.Mol(mol)
            Chem.Kekulize(work, clearAromaticFlags=True)
            Chem.WedgeMolBonds(work, work.GetConformer())
            for obj in list(work.GetAtoms()) + list(work.GetBonds()):
                if obj.HasProp("_CIPCode"):
                    obj.ClearProp("_CIPCode")
            conf = work.GetConformer()
            participants.append(
                dict(
                    molecule=dict(
                        rdkit_version=rdBase.rdkitVersion,
                        ids=[int(a.GetProp("reshiki_id")) for a in work.GetAtoms()],
                        positions=[
                            dict(x=p.x, y=p.y, z=p.z)
                            for p in (conf.GetAtomPosition(i) for i in range(work.GetNumAtoms()))
                        ],
                        state=snapshot(work, "symmetric"),
                    ),
                    file=dict(
                        is_3d=conf.Is3D(),
                        attachment_points=[
                            a.GetIntProp("molAttchpt") if a.HasProp("molAttchpt") else None
                            for a in work.GetAtoms()
                        ],
                        dummy_labels=[
                            a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                            for a in work.GetAtoms()
                        ],
                    ),
                )
            )
            rdCIPLabeler.AssignCIPLabels(work, maxRecursiveIterations=1_250_000)
            labels.append(
                dict(
                    rdkit_version=rdBase.rdkitVersion,
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
            )
        return to_document(mol)

    doc = reactions.import_reaction(text, "rxn", draw, check_supported)
    return dict(document=doc, participants=participants, labels=labels)


def emit(name, text):
    try:
        result, failure = expected(text), None
    except (ValueError, RuntimeError, KeyError, AttributeError, OverflowError) as error:
        result, failure = None, str(error)
    print(json.dumps(dict(name=name, text=text, expected=result, failure=failure)))


def cases(emit):
    index = 0

    def sample(name, text):
        nonlocal index
        index += 1
        if (
            name.startswith(("native/", "reaction source/"))
            or "/source/" in name
            or index % 31 == 0
        ):
            emit(name, text)

    parser_cases(sample)
    query_cases(emit)
    rng = random.Random(590130)
    for counts, version in product(product(range(4), repeat=3), (2000, 3000)):
        reaction = rdChemReactions.ChemicalReaction()
        for count, add in zip(
            counts,
            (reaction.AddReactantTemplate, reaction.AddProductTemplate, reaction.AddAgentTemplate),
            strict=True,
        ):
            for _ in range(count):
                mol = Chem.MolFromSmiles(
                    rng.choice(("O", "CCO", "c1ccccc1", "C[C@H](N)O", "F/C=C/F"))
                )
                rdDepictor.Compute2DCoords(mol)
                conf = mol.GetConformer()
                for atom in mol.GetAtoms():
                    p = conf.GetAtomPosition(atom.GetIdx())
                    conf.SetAtomPosition(atom.GetIdx(), (p.x + 123.4567, p.y - 83.2109, 0))
                add(mol)
        writer = (
            rdChemReactions.ReactionToV3KRxnBlock
            if version == 3000
            else rdChemReactions.ReactionToRxnBlock
        )
        emit(f"rows/{counts}/{version}", writer(reaction, separateAgents=True))
    for scale, dx, dy in product(
        (0, 0.001, 1, 100, 1e6, 1e20, 1e80), (0, -1.23456789, 1e15), (0, 0.123456789, -1e15)
    ):
        reaction = rdChemReactions.ChemicalReaction()
        for add in (
            reaction.AddReactantTemplate,
            reaction.AddProductTemplate,
            reaction.AddAgentTemplate,
        ):
            for smiles in ("O", "C[C@H](N)C(=O)O"):
                mol = Chem.MolFromSmiles(smiles)
                rdDepictor.Compute2DCoords(mol)
                conf = mol.GetConformer()
                for i in range(mol.GetNumAtoms()):
                    p = conf.GetAtomPosition(i)
                    conf.SetAtomPosition(i, (scale * p.x + dx, scale * p.y + dy, 0))
                add(mol)
        emit(
            f"coordinates/{scale}/{dx}/{dy}",
            rdChemReactions.ReactionToV3KRxnBlock(reaction, separateAgents=True),
        )


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases(emit)
