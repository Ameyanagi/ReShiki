"""Full plain SMILES output from pinned RDKit, independent of the worker."""

import json
import random
from itertools import product
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .smiles_traversal_reference import molecules
    from .valence_reference import ORDERS
else:
    from perception_reference import snapshot
    from smiles_traversal_reference import molecules
    from valence_reference import ORDERS


def emit(name, original, **options):
    mol = Chem.Mol(original)
    settings = dict(
        canonical=True,
        root=None,
        clean_stereo=True,
        ignore_maps=False,
        include_dative=True,
        symbols=dict(
            isomeric=True,
            kekule=False,
            all_hydrogens=False,
            all_bonds=False,
            non_tetrahedral=True,
        ),
    )
    for key, value in options.items():
        if key in settings["symbols"]:
            settings["symbols"][key] = value
        else:
            settings[key] = value
    mol.UpdatePropertyCache(strict=False)
    # The cache starts symmetric; variants explicitly remove it before writing.
    Chem.GetSymmSSSR(mol)
    kind = "symmetric"
    if settings.pop("unperceived", False):
        mol.ClearComputedProps(includeRings=True)
        kind = "none"
    before = snapshot(mol, kind)
    properties = [
        dict(
            custom_symbol=a.GetProp("smilesSymbol") if a.HasProp("smilesSymbol") else None,
            supplement=a.GetProp("_supplementalSmilesLabel")
            if a.HasProp("_supplementalSmilesLabel")
            else None,
            broken_chirality=bool(a.HasProp("_brokenChirality")),
        )
        for a in mol.GetAtoms()
    ]
    params = Chem.SmilesWriteParams()
    params.canonical = settings["canonical"]
    params.rootedAtAtom = -1 if settings["root"] is None else settings["root"]
    params.cleanStereo = settings["clean_stereo"]
    params.ignoreAtomMapNumbers = settings["ignore_maps"]
    params.includeDativeBonds = settings["include_dative"]
    params.doIsomericSmiles = settings["symbols"]["isomeric"]
    params.doKekule = settings["symbols"]["kekule"]
    params.allHsExplicit = settings["symbols"]["all_hydrogens"]
    params.allBondsExplicit = settings["symbols"]["all_bonds"]
    expected, failure = None, None
    try:
        text = Chem.MolToSmiles(mol, params)
        expected = dict(
            text=text,
            atom_order=json.loads(mol.GetProp("_smilesAtomOutputOrder"))
            if mol.HasProp("_smilesAtomOutputOrder")
            else [],
            bond_order=json.loads(mol.GetProp("_smilesBondOutputOrder"))
            if mol.HasProp("_smilesBondOutputOrder")
            else [],
        )
    except (ValueError, RuntimeError) as error:
        failure = str(error).splitlines()[0]
    print(
        json.dumps(
            dict(
                name=name,
                before=before,
                options=settings,
                properties=properties,
                expected=expected,
                failure=failure,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(42315)
    emit("empty", Chem.Mol())
    extra = (
        "O.[Na+].[Cl-]",
        "C[C@H](O)F.C[C@@H](O)F",
        "C[C@H]1CCC[C@@H](C)C1.O",
        "C[C@@H]1CC[C@H](C)CC1.N.CC.O",
        "N->[Cu]<-N",
        "N1->[Cu]<-NCC1",
        "[NH2:2]C[CH3:1].[OH2:3]",
        "F/C(Cl)=C(Br)/I.CC/C=C/C",
        "[Pt@SP1](Cl)(F)(Br)I.[P@TB5](F)(Cl)(Br)(I)N",
        "[Co@OH23](F)(Cl)(Br)(I)(N)O",
        "C[C@H](F)[C@H](Cl)C |&1:1,3|",
        "[C@@H]1(C)[C@@H](C)CC[C@H](C)C1.O",
    )
    for i, (name, text) in enumerate([*molecules(), *((f"extra/{s}", s) for s in extra)]):
        mol = Chem.MolFromSmiles(text)
        if (
            mol is None
            or any(a.HasQuery() for a in mol.GetAtoms())
            or any(b.HasQuery() or b.GetBondType() not in ORDERS.values() for b in mol.GetBonds())
        ):
            continue
        emit(f"{name}/default", mol)
        emit(f"{name}/kekule", mol, kekule=True)
        emit(f"{name}/achiral", mol, isomeric=False)
        emit(f"{name}/unperceived", mol, unperceived=True)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(f"{name}/permuted", Chem.RenumberAtoms(mol, order))
        if i % 13 == 0 or name.startswith("extra/"):
            # Interleaved components exercise remapping and canonical sorting.
            joined = Chem.CombineMols(mol, Chem.MolFromSmiles("O.C[C@H](F)Cl"))
            order = list(range(joined.GetNumAtoms()))
            rng.shuffle(order)
            joined = Chem.RenumberAtoms(joined, order)
            emit(f"{name}/mixture", joined)
            emit(f"{name}/mixture-raw", joined, canonical=False, isomeric=False)
        if name.startswith(("extra/", "template/")):
            for canonical, isomeric, kekule, ignore_maps in product((True, False), repeat=4):
                emit(
                    f"{name}/options/{canonical}/{isomeric}/{kekule}/{ignore_maps}",
                    mol,
                    canonical=canonical,
                    isomeric=isomeric,
                    kekule=kekule,
                    ignore_maps=ignore_maps,
                    include_dative=False,
                    all_hydrogens=True,
                    all_bonds=True,
                    unperceived=True,
                    clean_stereo=False,
                )
            for root in range(mol.GetNumAtoms()):
                emit(f"{name}/root/{root}", mol, root=root)
    for text in ("F[C@H](Cl)Br", "CC.CC.O", "N->[Cu]<-N", "[CH3:0]CC", "C[C@H]1CCC[C@@H](C)C1.O"):
        for sample in range(50):
            mol = Chem.RWMol(Chem.MolFromSmiles(text))
            for atom in mol.GetAtoms():
                if sample % 3 == 0:
                    atom.SetAtomMapNum(rng.randrange(4))
                if sample % 7 == 0:
                    atom.SetIsotope(rng.choice((0, 2, 13, 15)))
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            mol = Chem.RenumberAtoms(mol, order)
            for canonical, ignore_maps in product((True, False), repeat=2):
                emit(
                    f"mutated/{text}/{sample}/{canonical}/{ignore_maps}",
                    mol,
                    canonical=canonical,
                    ignore_maps=ignore_maps,
                    root=sample % mol.GetNumAtoms(),
                )

    for text in ("F[C@H](Cl)Br", "CC.O.C", "[CH3:0]CC", "c1ccccc1"):
        for symbol, supplement in product((None, "", "R", "日本語"), (None, "", "{a}", "*")):
            mol = Chem.RWMol(Chem.MolFromSmiles(text))
            for a in mol.GetAtoms():
                if symbol is not None:
                    a.SetProp("smilesSymbol", symbol)
                if supplement is not None:
                    a.SetProp("_supplementalSmilesLabel", supplement)
                if a.GetChiralTag() != Chem.ChiralType.CHI_UNSPECIFIED:
                    a.SetBoolProp("_brokenChirality", False)
            for canonical in (True, False):
                emit(f"symbols/{text}/{symbol}/{supplement}/{canonical}", mol, canonical=canonical)
    for tag, maximum, limit in ((6, 4, 3), (7, 5, 20), (8, 6, 30)):
        for degree, permutation in product(range(2, maximum + 1), range(limit + 2)):
            mol = Chem.RWMol()
            atom = Chem.Atom(78)
            atom.SetNoImplicit(True)
            atom.SetChiralTag(Chem.ChiralType.values[tag])
            atom.SetUnsignedProp("_chiralPermutation", permutation)
            mol.AddAtom(atom)
            for number in (9, 17, 35, 53, 7, 8)[:degree]:
                mol.AddBond(0, mol.AddAtom(Chem.Atom(number)), Chem.BondType.SINGLE)
            for root in range(mol.GetNumAtoms()):
                emit(f"coordination/{tag}/{degree}/{permutation}/{root}", mol, root=root)
    for text in ("c", "cc", "c1cc1", "c1cccc1", "[nH]1cccc1", "C1=CC=CC=C1", "[C@H](F)(Cl)Br"):
        mol = Chem.MolFromSmiles(text, sanitize=False)
        for isomeric, canonical, clean_stereo, kekule in product((True, False), repeat=4):
            emit(
                f"unsanitized/{text}/{isomeric}/{canonical}/{clean_stereo}/{kekule}",
                mol,
                isomeric=isomeric,
                canonical=canonical,
                clean_stereo=clean_stereo,
                kekule=kekule,
            )


if __name__ == "__main__":
    main()
