"""Independent lexical SMILES oracle, calling native non-query atom/bond writers."""

import json
from itertools import product
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .ranking_reference import metadata
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from kekulize_reference import directions
    from ranking_reference import metadata
    from valence_reference import ORDERS, atom_molecule, graph


def options(**changes):
    return (
        dict(
            isomeric=True,
            kekule=False,
            all_hydrogens=False,
            all_bonds=False,
            non_tetrahedral=True,
        )
        | changes
    )


def emit(name, mol, opts, operation="atoms", left=None):
    source = graph(mol)
    meta = metadata(mol)
    props = [
        dict(
            custom_symbol=a.GetProp("smilesSymbol") if a.HasProp("smilesSymbol") else None,
            supplement=Chem.GetSupplementalSmilesLabel(a)
            if a.HasProp("_supplementalSmilesLabel")
            else None,
            broken_chirality=bool(a.HasProp("_brokenChirality")),
        )
        for a in mol.GetAtoms()
    ]
    cache = None
    expected = None
    failure = None
    Chem.SetAllowNontetrahedralChirality(opts["non_tetrahedral"])
    try:
        mol.UpdatePropertyCache(strict=False)
        cache = [
            dict(
                explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
                implicit_hydrogens=a.GetNumImplicitHs(),
            )
            for a in mol.GetAtoms()
        ]
        if operation == "atoms":
            # Ordinary atoms dispatch directly to SmilesWrite::GetAtomSmiles.
            expected = [
                a.GetSmarts(opts["kekule"], opts["all_hydrogens"], opts["isomeric"])
                for a in mol.GetAtoms()
            ]
        elif operation == "bonds":
            # Ordinary bonds dispatch directly to SmilesWrite::GetBondSmiles.
            expected = [b.GetSmarts(opts["all_bonds"]) for b in mol.GetBonds()]
        else:
            # The Python bond wrapper cannot select the traversal endpoint.
            # A two-atom fragment exposes either orientation through the real
            # serializer; explicit atom symbols leave just its bond text.
            fragment = Chem.MolFragmentToSmiles(
                mol,
                [0, 1],
                atomSymbols=["X", "X"],
                canonical=False,
                rootedAtAtom=left,
                isomericSmiles=opts["isomeric"],
                kekuleSmiles=opts["kekule"],
                allBondsExplicit=opts["all_bonds"],
            )
            if not fragment.startswith("X") or not fragment.endswith("X"):
                raise AssertionError(fragment)
            expected = [fragment[1:-1]]
    except (ValueError, RuntimeError, OverflowError) as error:
        failure = str(error).splitlines()[0]
    finally:
        Chem.SetAllowNontetrahedralChirality(True)
    print(
        json.dumps(
            dict(
                name=name,
                graph=source,
                metadata=meta,
                directions=directions(mol),
                cache=cache,
                options=opts,
                properties=props,
                operation=operation,
                left=left,
                expected=expected,
                failure=failure,
            )
        )
    )


def atom_cases():
    yield "empty", Chem.RWMol()
    for number, charge, hs, radical, mode in product(
        range(119), (-128, -2, -1, 0, 1, 2, 127), (0, 1, 2, 4, 8), (0, 1), range(3)
    ):
        yield (
            f"element/{number}/{charge}/{hs}/{radical}/{mode}",
            atom_molecule(number, charge, hs, radical, mode != 0, mode == 2),
        )
    for number, other, order in product((0, 5, 6, 7, 8, 9, 15, 16, 17, 35, 53), range(119), (1, 5)):
        mol = atom_molecule(number, 0, 0, 0, False)
        mol.AddAtom(Chem.Atom(other))
        mol.AddBond(0, 1, ORDERS[order])
        yield f"neighbor/{number}/{other}/{order}", mol
    for tag, permutation, broken, map_number in product(
        range(9), (None, 0, 1, 2, 3, 4, 20, 21, 30, 31, 2**32 - 1), (False, True), (None, 0, 7)
    ):
        mol = atom_molecule(6, 0, 1, 0, True)
        a = mol.GetAtomWithIdx(0)
        a.SetChiralTag(Chem.ChiralType.values[tag])
        if permutation is not None:
            a.SetUnsignedProp("_chiralPermutation", permutation)
        if broken:
            a.SetProp("_brokenChirality", "0")
        if map_number is not None:
            a.SetIntProp("molAtomMapNumber", map_number)
        yield f"winding/{tag}/{permutation}/{broken}/{map_number}", mol
    for number, symbol, supplement, map_number in product(
        (0, 6, 7, 14, 16, 33, 34, 52, 53),
        (None, "", "C", "Se", "foo", "R1", "炭素", "C\x00R"),
        (None, "", "(F)", "\x00x"),
        (None, 0, -1, 2**31 - 1),
    ):
        mol = atom_molecule(number, -1, 2, 0, True, aromatic=True)
        a = mol.GetAtomWithIdx(0)
        a.SetIsotope(13)
        if symbol is not None:
            a.SetProp("smilesSymbol", symbol)
        if supplement is not None:
            Chem.SetSupplementalSmilesLabel(a, supplement)
        if map_number is not None:
            a.SetIntProp("molAtomMapNumber", map_number)
        yield f"properties/{number}/{symbol!r}/{supplement!r}/{map_number}", mol
    for number, isotope, charge in product(range(119), (1, 13, 65535), (-1, 0, 1)):
        mol = atom_molecule(number, charge, 1, 0, True)
        mol.GetAtomWithIdx(0).SetIsotope(isotope)
        yield f"isotope/{number}/{isotope}/{charge}", mol
    for text in (
        "CC(=O)Oc1ccccc1C(=O)O",
        "[nH]1cccc1",
        "[pH]1cccc1",
        "O=c1cc[nH]cc1",
        "C1CC2CCC1C2",
        "C/C=C/C=C\\C",
        "N[C@@H](C)C(=O)O",
        "[Pt@SP1](F)(Cl)(Br)I",
        "[P@TB20](F)(Cl)(Br)(I)N",
        "[Co@OH30](F)(Cl)(Br)(I)(N)O",
        "[H][C@]([2H])(O)C",
        "[CH3:0][OH:2147483639]",
        "[Na+].[Cl-]",
    ):
        params = Chem.SmilesParserParams()
        params.removeHs = False
        mol = Chem.MolFromSmiles(text, params)
        if mol is None:
            raise AssertionError(text)
        yield f"molecule/{text}", mol


def bond_cases():
    for order, aromatic, a, b, atom_aromatic, direction in product(
        range(8), (False, True), (0, 6), (0, 7), (False, True), range(7)
    ):
        mol = Chem.RWMol()
        for number in (a, b):
            atom = Chem.Atom(number)
            atom.SetIsAromatic(atom_aromatic)
            mol.AddAtom(atom)
        mol.AddBond(0, 1, ORDERS[order])
        bond = mol.GetBondWithIdx(0)
        bond.SetIsAromatic(aromatic)
        bond.SetBondDir(Chem.BondDir.values[direction])
        yield f"bond/{order}/{aromatic}/{a}/{b}/{atom_aromatic}/{direction}", mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for i, (name, mol) in enumerate(atom_cases()):
        emit(name + "/default", mol, options())
        # Rotate flags across the exhaustive element/valence matrix. Test every
        # setting where it changes lexical output (chirality and properties).
        flags = range(8) if name.startswith(("winding/", "properties/", "molecule/")) else (i % 8,)
        for flag in flags:
            emit(
                name + f"/flags/{flag}",
                mol,
                options(
                    isomeric=bool(flag & 1), kekule=bool(flag & 2), all_hydrogens=bool(flag & 4)
                ),
            )
        if name.startswith("winding/"):
            emit(name + "/no-coordination", mol, options(non_tetrahedral=False))
    for name, mol in bond_cases():
        for explicit in (False, True):
            emit(
                name + f"/explicit/{explicit}",
                mol,
                options(isomeric=False, all_bonds=explicit),
                "bonds",
            )
        # Fragment traversal clears directions; only compare undisplaced bonds.
        # Kekulization is a graph operation, so use the direct symbol wrapper
        # above for deliberately inconsistent aromatic test graphs.
        if (
            mol.GetBondWithIdx(0).GetBondDir() == Chem.BondDir.NONE
            and not mol.GetBondWithIdx(0).GetIsAromatic()
            and not any(a.GetIsAromatic() for a in mol.GetAtoms())
            and mol.GetBondWithIdx(0).GetBondType() != Chem.BondType.AROMATIC
        ):
            # Unlike GetBondSmiles, full fragment writing performs a strict
            # cache refresh. Invalid valence has already been checked through
            # the direct lexical writer above; it cannot isolate a bond here.
            try:
                mol.UpdatePropertyCache(strict=True)
            except (ValueError, RuntimeError):
                continue
            for left, isomeric, explicit in product((0, 1), (False, True), (False, True)):
                emit(
                    name + f"/fragment/{left}/{isomeric}/{explicit}",
                    mol,
                    options(isomeric=isomeric, all_bonds=explicit),
                    "fragment",
                    left,
                )


if __name__ == "__main__":
    main()
