"""Build molecular states with RDKit's direct API, without importing the worker."""

import copy
import json
import random
import struct
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .valence_reference import ORDERS
else:
    from perception_reference import snapshot
    from valence_reference import ORDERS

STEREO = {"z": 2, "e": 3, "cis": 4, "trans": 5}
DISPLAY = {"wedge": 1, "hash": 2, "hollow_wedge": 1, "bold": 1, "hashed": 2, "wavy": 6}


def prepare(doc):
    rw = Chem.RWMol()
    ids = {}
    for item in doc["atoms"]:
        if item["id"] in ids or item["id"] < 1:
            raise ValueError("Invalid atom ID")
        atom = Chem.Atom(item["element"])
        atom.SetFormalCharge(item.get("charge", 0))
        atom.SetIsotope(item.get("isotope", 0))
        atom.SetNumExplicitHs(item.get("explicit_h", 0))
        atom.SetNoImplicit(item.get("no_implicit", False))
        atom.SetIsAromatic(item.get("aromatic", False))
        atom.SetNumRadicalElectrons(item.get("radical_electrons", 0))
        atom.SetAtomMapNum(item.get("map_num", 0))
        ids[item["id"]] = rw.AddAtom(atom)
    for item in doc["bonds"]:
        a, b = ids[item["a"]], ids[item["b"]]
        rw.AddBond(a, b, ORDERS[item["order"]])
        if item["order"] == 1 or item["order"] == 2 and item.get("display") == "wavy":
            rw.GetBondBetweenAtoms(a, b).SetBondDir(
                Chem.BondDir.values[DISPLAY.get(item.get("display"), 0)]
            )
    for item in doc["bonds"]:
        if item["order"] != 0:
            continue
        h, other = rw.GetAtomWithIdx(ids[item["a"]]), rw.GetAtomWithIdx(ids[item["b"]])
        if (
            h.GetSymbol() != "H"
            or other.GetSymbol() not in ("N", "O", "F", "S")
            or other.GetFormalCharge() > 0
            or not any(b.GetBondType() == Chem.BondType.SINGLE for b in h.GetBonds())
        ):
            raise ValueError("Invalid hydrogen bond")
    mol = rw.GetMol()
    positions = Chem.Conformer(len(ids))
    positions.Set3D(False)
    for item in doc["atoms"]:
        i = ids[item["id"]]
        p = item["position"]
        positions.SetAtomPosition(i, (p["x"] / 28, -p["y"] / 28, 0))
        if stereo := item.get("stereo"):
            atom = mol.GetAtomWithIdx(i)
            given = [ids[value] for value in stereo["neighbors"]]
            current = [n.GetIdx() for n in atom.GetNeighbors()]
            if set(given) != set(current) or len(given) != len(current):
                raise ValueError("Invalid stereocenter references")
            order = [given.index(value) for value in current]
            odd = sum(a > b for i, a in enumerate(order) for b in order[i + 1 :]) % 2
            tag = 1 if (stereo["winding"] == "cw") ^ bool(odd) else 2
            atom.SetChiralTag(Chem.ChiralType.values[tag])
    mol.AddConformer(positions)
    Chem.SanitizeMol(mol)
    for item in doc["atoms"]:
        if item.get("radical_electrons", 0) not in (
            0,
            mol.GetAtomWithIdx(ids[item["id"]]).GetNumRadicalElectrons(),
        ):
            raise ValueError("Requested radical count changed")
    Chem.AssignChiralTypesFromBondDirs(mol, replaceExistingTags=False)
    for item in doc["bonds"]:
        bond = mol.GetBondBetweenAtoms(ids[item["a"]], ids[item["b"]])
        if item.get("stereo") in STEREO:
            left, right = item["stereo_atoms"]
            bond.SetStereoAtoms(ids[left], ids[right])
            bond.SetStereo(Chem.BondStereo.values[STEREO[item["stereo"]]])
        elif item.get("display") == "wavy" and item["order"] == 2:
            bond.SetStereo(Chem.BondStereo.STEREOANY)
    Chem.DetectBondStereochemistry(mol, confId=0)
    Chem.AssignStereochemistry(mol, cleanIt=False, force=True)
    if (
        mol.GetStereoGroups()
        or any(a.GetNumRadicalElectrons() > 2 or int(a.GetChiralTag()) > 2 for a in mol.GetAtoms())
        or any(int(b.GetStereo()) > 5 for b in mol.GetBonds())
    ):
        raise ValueError("Unsupported molecular state")
    return dict(
        rdkit_version=rdBase.rdkitVersion,
        ids=[a["id"] for a in doc["atoms"]],
        positions=[
            dict(x=p.x, y=p.y, z=p.z)
            for p in (positions.GetAtomPosition(i) for i in range(len(ids)))
        ],
        state=snapshot(mol, "symmetric"),
    )


def emit(name, doc):
    try:
        expected, failure = prepare(doc), None
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, document=doc, expected=expected, failure=failure)))


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def drawing(original):
    mol = Chem.Mol(original)
    if not mol.GetNumConformers():
        rdDepictor.Compute2DCoords(mol)
    Chem.WedgeMolBonds(mol, mol.GetConformer())
    ids = {i: 9007199254741011 + 17 * i for i in range(mol.GetNumAtoms())}
    atoms = []
    for a in mol.GetAtoms():
        p = mol.GetConformer().GetAtomPosition(a.GetIdx())
        atoms.append(
            dict(
                id=ids[a.GetIdx()],
                element=a.GetSymbol(),
                charge=a.GetFormalCharge(),
                isotope=a.GetIsotope(),
                explicit_h=a.GetNumExplicitHs(),
                radical_electrons=a.GetNumRadicalElectrons(),
                no_implicit=a.GetNoImplicit(),
                aromatic=a.GetIsAromatic(),
                map_num=a.GetAtomMapNum(),
                label_h=99,
                position=dict(x=f32(p.x * 28), y=f32(-p.y * 28)),
                stereo=None
                if not int(a.GetChiralTag())
                else dict(
                    winding="cw" if int(a.GetChiralTag()) == 1 else "ccw",
                    neighbors=[ids[n.GetIdx()] for n in a.GetNeighbors()],
                ),
            )
        )
    bonds = []
    for b in mol.GetBonds():
        bonds.append(
            dict(
                a=ids[b.GetBeginAtomIdx()],
                b=ids[b.GetEndAtomIdx()],
                order={v: k for k, v in ORDERS.items()}[b.GetBondType()],
                display={1: "wedge", 2: "hash", 6: "wavy"}.get(int(b.GetBondDir()), "plain"),
                stereo={v: k for k, v in STEREO.items()}.get(int(b.GetStereo())),
                stereo_atoms=[ids[i] for i in b.GetStereoAtoms()],
            )
        )
    return dict(version=15, atoms=atoms, bonds=bonds)


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(946027)
    emit("empty", dict(version=15, atoms=[], bonds=[]))
    for donor in ("H", "C"):
        for acceptor in ("N", "O", "F", "S", "C"):
            for charge in (0, 1):
                for bound in (False, True):
                    emit(
                        f"hydrogen bond/{donor}/{acceptor}/{charge}/{bound}",
                        dict(
                            version=15,
                            atoms=[
                                dict(id=1, element="O", position=dict(x=0, y=0)),
                                dict(id=2, element=donor, position=dict(x=28, y=0)),
                                dict(
                                    id=3, element=acceptor, charge=charge, position=dict(x=56, y=0)
                                ),
                            ],
                            bonds=([dict(a=1, b=2, order=1)] if bound else [])
                            + [dict(a=2, b=3, order=0, display="dotted")],
                        ),
                    )
    for order in (5, 6, 7):
        for left, right in (("N", "Cu"), ("C", "C"), ("Mo", "Mo"), ("*", "*")):
            emit(
                f"special bond/{order}/{left}/{right}",
                dict(
                    version=15,
                    atoms=[
                        dict(id=i + 1, element=symbol, position=dict(x=i * 28, y=0))
                        for i, symbol in enumerate((left, right))
                    ],
                    bonds=[dict(a=1, b=2, order=order)],
                ),
            )
    for number in range(119):
        for charge in (-2, -1, 0, 1, 2):
            for hs in (0, 1, 2):
                emit(
                    f"element/{number}/{charge}/{hs}",
                    dict(
                        version=15,
                        atoms=[
                            dict(
                                id=1,
                                element=Chem.GetPeriodicTable().GetElementSymbol(number),
                                charge=charge,
                                explicit_h=hs,
                                no_implicit=True,
                                position=dict(x=0, y=0),
                            )
                        ],
                        bonds=[],
                    ),
                )
    examples = [
        "[2H]O[3H]",
        "[NH4+]",
        "[13CH3][C@H](F)C(=O)O",
        "[CH3]",
        "[CH2]",
        "[O]",
        "F/C=C/Cl",
        "F/C=C\\Cl",
        "F[C@](Cl)(Br)I",
        "N[C@H](C)C(=O)O",
        "c1cc[nH]c1",
        "[O-][N+](=O)c1ccccc1",
        "CS(=O)(=O)N",
        "C1CCC2(CC1)CCCC2",
        "C12C3C4C1C5C2C3C45",
        "[NH3]->[Cu+2]<-[NH3]",
        "[CH3:1][OH:9]",
        "*CC",
        "[H][H]",
    ]
    root = Path(__file__).resolve().parents[1]
    examples += [t["smiles"] for t in json.loads((root / "assets/templates.json").read_text())]
    examples += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
        if line.strip()
    ]
    for i, text in enumerate(examples):
        mol = Chem.MolFromSmiles(text)
        if mol is None or any(a.GetNumRadicalElectrons() > 2 for a in mol.GetAtoms()):
            continue
        doc = drawing(mol)
        if text == "[CH3:1][OH:9]":
            doc["atoms"][-1]["map_num"] = 2147483647
        emit(text + "/aromatic", doc)
        Chem.Kekulize(mol, clearAromaticFlags=True)
        changed = drawing(mol)
        rng.shuffle(changed["atoms"])
        rng.shuffle(changed["bonds"])
        for a in changed["atoms"]:
            a["position"]["x"] = -a["position"]["x"]
            if i % 3 == 0:
                a["stereo"] = None
        emit(text + "/reordered-mirrored", changed)
    base = drawing(Chem.MolFromSmiles("N[C@H](C)C(=O)O"))
    for i in range(600):
        doc = copy.deepcopy(base)
        for atom in doc["atoms"]:
            if i % 4 == 0:
                atom["stereo"] = None
            atom["position"] = dict(x=f32(rng.uniform(-100, 100)), y=f32(rng.uniform(-100, 100)))
            atom["radical_electrons"] = rng.choice((0, 0, 0, 1, 2)) if i % 3 == 0 else 0
        for bond in doc["bonds"]:
            bond["display"] = rng.choice(
                ("plain", "wedge", "hash", "wavy", "bold", "hashed", "hollow_wedge")
                if bond["order"] == 1
                else ("plain", "wavy")
            )
        emit(f"modified drawing/{i}", doc)


if __name__ == "__main__":
    main()
