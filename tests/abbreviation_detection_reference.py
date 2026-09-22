"""Independent original-worker abbreviation oracle and pinned native query export.

Regenerate the production catalog with --write-presets. Expected detection and
validation always call the original Python functions, never Rust or its data.
"""

import copy
import json
import random
import re
import sys
from pathlib import Path

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdAbbreviations

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from perception_reference import snapshot  # noqa: E402
from valence_reference import ORDERS  # noqa: E402

from engine import abbreviations  # noqa: E402


def catalog():
    presets = []
    for label, (smiles, reverse_label) in abbreviations.PRESETS.items():
        query = rdAbbreviations.ParseAbbreviations(f"{label} {smiles}")[0].mol
        atoms = []
        for atom in query.GetAtoms():
            leaves = re.findall(r"(\w+) (-?\d+) (=|!=) val", atom.DescribeQuery())
            data = dict(number=0, aromatic=None, charge=None, degree=atom.GetDegree(), rings=0)
            for name, value, comparison in leaves:
                value = int(value)
                if atom.GetIdx() == 0:
                    assert (name, value, comparison) == ("AtomAtomicNum", 0, "!=")
                elif name == "AtomAtomicNum":
                    assert comparison == "="
                    data["number"] = value
                elif name == "AtomType":
                    assert comparison == "="
                    data.update(number=value % 1000, aromatic=value >= 1000)
                elif name == "AtomFormalCharge":
                    data["charge"] = value
                elif name == "AtomExplicitDegree":
                    assert value == atom.GetDegree()
                elif name == "AtomInNRings":
                    data["rings"] = value
                else:
                    raise AssertionError(atom.DescribeQuery())
            atoms.append(data)
        bonds = []
        for bond in query.GetBonds():
            description = bond.DescribeQuery().strip()
            if description == "SingleOrAromaticBond 1 = val":
                order = 0
            else:
                matched = re.fullmatch(r"BondOrder ([1-3]) = val", description)
                assert matched, description
                order = int(matched[1])
            bonds.append(dict(a=bond.GetBeginAtomIdx(), b=bond.GetEndAtomIdx(), order=order))
        presets.append(
            dict(label=label, reverse_label=reverse_label, smiles=smiles, atoms=atoms, bonds=bonds)
        )
    return dict(rdkit_version=rdBase.rdkitVersion, presets=presets)


def document(mol, high_ids=False):
    start = 2**64 - 100_000 if high_ids else 1
    ids = [start + i * 3 for i in range(mol.GetNumAtoms())]
    atoms = []
    for atom, identity in zip(mol.GetAtoms(), ids):
        atom.SetProp("reshiki_id", str(identity))
        atoms.append(
            dict(
                id=identity,
                element=atom.GetSymbol(),
                position=dict(x=atom.GetIdx() * 42, y=0),
                charge=atom.GetFormalCharge(),
                isotope=atom.GetIsotope(),
                explicit_h=atom.GetNumExplicitHs(),
                no_implicit=atom.GetNoImplicit(),
                aromatic=atom.GetIsAromatic(),
                radical_electrons=atom.GetNumRadicalElectrons(),
                map_num=atom.GetAtomMapNum(),
            )
        )
        if atom.GetChiralTag() in (
            Chem.ChiralType.CHI_TETRAHEDRAL_CW,
            Chem.ChiralType.CHI_TETRAHEDRAL_CCW,
        ):
            atoms[-1]["stereo"] = dict(
                winding="cw" if int(atom.GetChiralTag()) == 1 else "ccw",
                neighbors=[ids[n.GetIdx()] for n in atom.GetNeighbors()],
            )
    bonds = [
        dict(
            a=ids[b.GetBeginAtomIdx()],
            b=ids[b.GetEndAtomIdx()],
            order=next(order for order, kind in ORDERS.items() if kind == b.GetBondType()),
        )
        for b in mol.GetBonds()
    ]
    return dict(version=15, atoms=atoms, bonds=bonds, annotations=[], arrows=[])


def emit(name, mol, doc=None, selection=None, label=None, rings="symmetric", prepare=False):
    doc = copy.deepcopy(doc) if doc is not None else document(mol)
    before = copy.deepcopy(doc)
    selection = selection or []
    native_state = snapshot(mol, rings)
    try:
        expected = abbreviations.find(doc, mol, selection, label)
        error = None
    except ValueError as exception:
        expected, error = None, str(exception)
    assert doc == before
    print(
        json.dumps(
            dict(
                name=name,
                operation="find",
                document=doc,
                state=native_state,
                selection=selection,
                label=label,
                expected=expected,
                error=error,
                prepare=prepare,
            )
        )
    )


def validation(name, doc):
    try:
        abbreviations.validate(doc)
        error = None
    except ValueError as exception:
        error = str(exception)
    print(json.dumps(dict(name=name, operation="validate", document=doc, error=error)))


def reorder_bonds(mol, order):
    result = Chem.RWMol()
    for atom in mol.GetAtoms():
        result.AddAtom(atom)
    for index in order:
        bond = mol.GetBondWithIdx(index)
        result.AddBond(bond.GetBeginAtomIdx(), bond.GetEndAtomIdx(), bond.GetBondType())
    result = result.GetMol()
    Chem.SanitizeMol(result)
    return result


def main():
    RDLogger.DisableLog("rdApp.*")
    if "--write-presets" in sys.argv:
        path = Path(__file__).resolve().parents[1] / "src/chemistry/abbreviations/presets.json"
        path.write_text(json.dumps(catalog(), indent=2) + "\n")
        return
    print(json.dumps(catalog()))
    rng = random.Random(48361)
    for label, (smiles, _) in abbreviations.PRESETS.items():
        for outside in ["C", "N", "O", "[NH3+]", "c1ccccc1", "*", "[13CH3]", "[CH3:7]"]:
            mol = Chem.MolFromSmiles(outside + smiles[1:])
            assert mol is not None, (label, outside)
            for chosen in [None, label, "", "unknown"]:
                emit(
                    f"preset/{label}/{outside}/{chosen}",
                    mol,
                    label=chosen,
                    prepare=outside == "N" and chosen == label,
                )
        original = Chem.MolFromSmiles("N" + smiles[1:])
        for permutation in range(5):
            indices = list(range(original.GetNumAtoms()))
            rng.shuffle(indices)
            mol = Chem.RenumberAtoms(original, indices) if indices else Chem.Mol(original)
            order = list(range(mol.GetNumBonds()))
            rng.shuffle(order)
            mol = reorder_bonds(mol, order)
            emit(f"permuted/{label}/{permutation}", mol, doc=document(mol, high_ids=True))
        for hydrogens in [False, True]:
            mol = Chem.AddHs(original) if hydrogens else Chem.Mol(original)
            emit(f"explicit-h/{label}/{hydrogens}", mol, label=label)
        # Native ring-cache semantics: initialized fast/basis caches are retained.
        for ring_kind in ["fast", "basis", "none"]:
            mol = Chem.Mol(original)
            mol.ClearComputedProps(includeRings=True)
            mol.UpdatePropertyCache(strict=False)
            if ring_kind == "fast":
                Chem.FastFindRings(mol)
            elif ring_kind == "basis":
                Chem.GetSSSR(mol)
            emit(f"ring-cache/{label}/{ring_kind}", mol, rings=ring_kind)
        # Exclusions happen after native overlap resolution, on each member.
        for index in range(1, original.GetNumAtoms()):
            for field, value in [
                ("isotope", 13),
                ("map_num", 17),
                ("stereo", dict(winding="cw", neighbors=[])),
                ("marks", [dict(kind="lone_pair", offset=dict(x=2, y=3))]),
                ("display", dict(number=dict(text="1"))),
            ]:
                doc = document(original, high_ids=True)
                doc["atoms"][index][field] = value
                emit(f"excluded/{label}/{index}/{field}", original, doc)
        doc = document(original, high_ids=True)
        ids = [a["id"] for a in doc["atoms"]]
        for selected in [ids, ids[1:], ids[2:], ids[:1], [2**63], ids + [2**63], ids + ids]:
            emit(f"selection/{label}/{selected}", original, doc, selection=selected)
        found = abbreviations.find(doc, original, [], label)
        emit(f"already-collapsed/{label}", original, found)
        if found.get("abbreviations"):
            found["abbreviations"][0]["label"] = "Custom"
            emit(f"custom-existing/{label}", original, found)
    for left, (a, _) in abbreviations.PRESETS.items():
        for right, (b, _) in abbreviations.PRESETS.items():
            mol = Chem.MolFromSmiles(f"N({a[1:]}){b[1:]}")
            assert mol is not None, (left, right)
            for chosen in [None, left, right]:
                emit(f"pair/{left}/{right}/{chosen}", mol, label=chosen)
    for smiles in [
        "",
        "C",
        "CC",
        "CCC",
        "CCCC",
        "CCCCC",
        "CCCCCC",
        "COC",
        "CCOCC",
        "CCOC",
        "CC(C)C",
        "CC(C)(C)C",
        "COc1ccc(NC(=O)OC(C)(C)C)cc1",
        "CC(=O)[O-]",
        "[CH2]C",
        "[CH2-]C",
        "CC(=O)[OH2+]",
        "c1ccccc1",
        "c1ccc2ccccc2c1",
        "CC.[Na+].[Cl-]",
        "[H]OC",
        "C[O+]C",
        "O=C(C)c1ccccc1",
    ]:
        original = Chem.MolFromSmiles(smiles)
        assert original is not None, smiles
        for permutation in range(8):
            indices = list(range(original.GetNumAtoms()))
            rng.shuffle(indices)
            mol = Chem.RenumberAtoms(original, indices) if indices else Chem.Mol(original)
            emit(f"competition/{smiles}/{permutation}", mol)
    path = Path(RDConfig.RDDataDir) / "NCI/first_5K.smi"
    for index, line in enumerate(path.read_text().splitlines()[:1500]):
        mol = Chem.MolFromSmiles(line.split()[0])
        if mol is not None:
            emit(f"nci/{index}", mol)
    # More than 1,000 native unique maps proves the native cap is reproduced.
    for smiles in [".".join(["NC"] * 1004), ".".join(["NCC"] * 1004)]:
        mol = Chem.MolFromSmiles(smiles)
        emit(
            f"native-match-limit/{mol.GetNumAtoms()}",
            mol,
            label="Me" if mol.GetNumAtoms() == 2008 else "Et",
        )
    mol = Chem.MolFromSmiles("CCO.C")
    base = document(mol)
    good = dict(label="Et", reverse_label="", anchor=4, members=[1, 4])
    for version in [1, 9, 10, 15]:
        doc = copy.deepcopy(base)
        doc.update(version=version, abbreviations=[good])
        validation(f"validation/version/{version}", doc)
    for text in [
        "",
        " ",
        "\x1c",
        "\x00",
        "\x7f",
        "\x80",
        "\x9f",
        "A" * 32,
        "A" * 33,
        "α" * 32,
        "🧪" * 32,
        "A\u200b",
        "\u00a0",
    ]:
        for field in ["label", "reverse_label"]:
            doc = copy.deepcopy(base)
            group = copy.deepcopy(good)
            group[field] = text
            doc["abbreviations"] = [group]
            validation(f"validation/text/{field}/{text!r}", doc)
    for members, anchor in [
        ([], 1),
        ([1, 1], 1),
        ([999], 999),
        ([1], 4),
        ([1, 7], 1),
        ([1, 4, 10], 4),
        ([1, 4], 1),
        ([4], 4),
        ([10], 10),
        ([1, 4, 7], 1),
    ]:
        doc = copy.deepcopy(base)
        doc["abbreviations"] = [dict(label="X", reverse_label="", members=members, anchor=anchor)]
        validation(f"validation/members/{members}/{anchor}", doc)
        emit(f"find/invalid-existing/{members}/{anchor}", mol, doc)
    doc = copy.deepcopy(base)
    doc["abbreviations"] = [good, copy.deepcopy(good)]
    validation("validation/overlap", doc)
    emit("find/overlap", mol, doc)


if __name__ == "__main__":
    main()
