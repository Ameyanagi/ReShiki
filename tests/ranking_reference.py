"""Independent RDKit canonical/symmetry ranks and bond-assignment expectations."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .valence_reference import graph
else:
    from kekulize_reference import directions
    from valence_reference import graph


def metadata(mol):
    return dict(
        atoms=[
            dict(
                map_number=a.GetAtomMapNum(),
                chiral_tag=int(a.GetChiralTag()),
                chiral_permutation=a.GetUnsignedProp("_chiralPermutation")
                if a.HasProp("_chiralPermutation")
                else None,
                ring_stereo=bool(a.HasProp("_ringStereoAtoms")),
                non_stereo_rank=a.GetIntProp("_CanonicalRankingNumber")
                if a.HasProp("_CanonicalRankingNumber")
                else 0,
            )
            for a in mol.GetAtoms()
        ],
        bonds=[
            dict(stereo=int(b.GetStereo()), stereo_atoms=list(b.GetStereoAtoms()))
            for b in mol.GetBonds()
        ],
        groups=[
            dict(
                kind=int(g.GetGroupType()),
                atoms=[a.GetIdx() for a in g.GetAtoms()],
                bonds=[b.GetIdx() for b in g.GetBonds()],
                read_id=g.GetReadId(),
                write_id=g.GetWriteId(),
            )
            for g in mol.GetStereoGroups()
        ],
    )


def emit(
    name,
    original,
    fragment=False,
    break_ties=True,
    chirality=True,
    isotopes=True,
    maps=True,
    presence=False,
    ring_method="symmetric",
):
    mol = Chem.Mol(original)
    mol.ClearComputedProps(includeRings=True)
    mol.UpdatePropertyCache(strict=False)
    if ring_method == "fast":
        Chem.FastFindRings(mol)
    elif ring_method == "basis":
        Chem.GetSSSR(mol)
    else:
        Chem.GetSymmSSSR(mol)
    source = graph(mol)
    meta = metadata(mol)
    rings = list(mol.GetRingInfo().AtomRings())
    options = dict(
        break_ties=break_ties,
        include_chirality=chirality,
        include_isotopes=isotopes,
        include_maps=maps,
        include_chiral_presence=presence,
        include_stereo_groups=True,
        use_non_stereo_ranks=False,
        include_ring_stereo=True,
        fragment=fragment,
    )
    kwargs = dict(
        breakTies=break_ties,
        includeChirality=chirality,
        includeIsotopes=isotopes,
        includeAtomMaps=maps,
        includeChiralPresence=presence,
    )
    if fragment and mol.GetNumAtoms():
        # The pinned Python fragment wrapper forwards this into includeAtomMaps;
        # chiral-presence comparison stays false in its C++ call.
        if presence:
            raise ValueError("Pinned fragment wrapper cannot set chiral presence")
        kwargs["includeChiralPresence"] = maps
        expected = list(
            Chem.CanonicalRankAtomsInFragment(
                mol,
                atomsToUse=list(range(mol.GetNumAtoms())),
                bondsToUse=list(range(mol.GetNumBonds())),
                **kwargs,
            )
        )
    else:
        expected = list(Chem.CanonicalRankAtoms(mol, **kwargs))
    kekule = None
    dirs = directions(mol)
    if fragment and break_ties and chirality and isotopes and maps and not presence:
        try:
            Chem.Kekulize(mol, clearAromaticFlags=True, canonical=True)
            kekule = dict(graph=graph(mol), directions=directions(mol))
        except (ValueError, RuntimeError):
            kekule = False
    print(
        json.dumps(
            dict(
                name=name,
                graph=source,
                metadata=meta,
                rings=rings,
                options=options,
                expected=expected,
                directions=dirs,
                kekule=kekule,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.RWMol())
    texts = [
        "CCO",
        "C.C.C.C",
        "[Na+].[Cl-]",
        "[2H]O[H]",
        "[13CH3]C",
        "[CH3:8][CH3:2]",
        "[*:9]CC[*:1]",
        "N[C@@H](C)C(=O)O",
        "C[C@H](O)[C@H](O)C",
        "C[C@H](O)[C@@H](O)C",
        "F/C=C/F",
        "F/C=C\\F",
        "C/C(C)=C(Cl)/F",
        "C/C=C/C=C/C",
        "C/C=C/C=C\\C",
        "[C@H]1(CCCCC1)F",
        "C[C@H]1CCC[C@@H](C)C1",
        "C[C@H]1CCC[C@H](C)C1",
        "C[C@H]1CCC[C@@H](C)C1 |&1:1,5|",
        "C[C@H]1CCC[C@H](C)C1 |o1:1,5|",
        "C[C@H](O)[C@H](O)C |a:1,3|",
        "C[C@H](O)[C@H](O)C |&1:1,&2:3|",
        "C[C@H](O)[C@H](O)C |&1:1,o2:3|",
        "c1ccccc1",
        "c1ccc2ccccc2c1",
        "c1cc[nH]c1",
        "O=n1ccccc1",
        "C12C3C4C1C5C2C3C45",
        "C1C2CC3CC1CC(C2)C3",
        "C1CCC2(CC1)CCCC2",
        "C1=CC2=CC=C3C2=C1C=C3",
        "[n]1ccccc1",
        "*1c*c*1",
        "Cl[Pt@SP1](Cl)(N)N",
        "F[P@TB1](Cl)(Br)(I)N",
        "F[Co@OH1](Cl)(Br)(I)(N)O",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    rng = random.Random(6684)
    # Ring stereo is optional metadata, not necessarily present after a modern
    # SMILES parse. Exercise it explicitly, alongside neighbor-dependent stereo.
    for text in (
        "C[C@H]1CCC[C@@H](C)C1",
        "C[C@H]1CCC[C@H](C)C1",
        "C1[C@H]2CC[C@@H]1C2",
        "C[C@H]1CC[C@H](C)CC1",
    ):
        for sample in range(20):
            mol = Chem.MolFromSmiles(text)
            perm = list(range(mol.GetNumAtoms()))
            rng.shuffle(perm)
            mol = Chem.RenumberAtoms(mol, perm)
            # Ranking tests only property presence. Install this marker after
            # renumbering, which would interpret its unrelated vector payload.
            for atom in mol.GetAtoms():
                if atom.GetChiralTag() != Chem.ChiralType.CHI_UNSPECIFIED:
                    atom.SetIntProp("_ringStereoAtoms", 1)
            emit(f"ring stereo {text}/{sample}", mol)
            emit(f"ring stereo classes {text}/{sample}", mol, break_ties=False)
            emit(f"ring stereo fragment {text}/{sample}", mol, fragment=True)
    for text in ("Fc1cccc(F)c1-c1c(Cl)cccc1Cl", "CC(C)C(C)C"):
        for stereo in (Chem.BondStereo.STEREOATROPCW, Chem.BondStereo.STEREOATROPCCW):
            mol = Chem.MolFromSmiles(text)
            for bond in mol.GetBonds():
                if bond.GetBeginAtom().GetDegree() > 1 and bond.GetEndAtom().GetDegree() > 1:
                    modified = Chem.Mol(mol)
                    modified.GetBondWithIdx(bond.GetIdx()).SetStereo(stereo)
                    emit(f"atrop {text}/{bond.GetIdx()}/{stereo}", modified)
                    emit(f"atrop fragment {text}/{bond.GetIdx()}/{stereo}", modified, fragment=True)
    for sample in range(1000):
        size = rng.randrange(3, 25)
        mol = Chem.RWMol()
        for i in range(size):
            atom = Chem.Atom(6)
            atom.SetNoImplicit(True)
            if sample % 4 == 0:
                atom.SetIsotope(rng.choice((0, 0, 13)))
                atom.SetFormalCharge(rng.choice((-1, 0, 1)))
                atom.SetAtomMapNum(rng.choice((-1, 0, 1, 5)))
            mol.AddAtom(atom)
        # Regular graphs stress the special symmetry pass; irregular graphs
        # stress partition activation and class splitting.
        for i in range(size):
            mol.AddBond(i, (i + 1) % size, Chem.BondType.SINGLE)
        for i in range(size):
            for j in range(i + 1, size):
                if not mol.GetBondBetweenAtoms(i, j) and rng.random() < 0.15:
                    mol.AddBond(i, j, Chem.BondType.SINGLE)
        emit(f"synthetic graph {sample}", mol)
        emit(f"synthetic classes {sample}", mol, break_ties=False)
        emit(f"synthetic fast rings {sample}", mol, ring_method="fast")
    for size in range(3, 13):
        for distance in (1, 2, 3):
            mol = Chem.RWMol()
            for _ in range(size):
                atom = Chem.Atom(6)
                atom.SetNoImplicit(True)
                mol.AddAtom(atom)
            for i in range(size):
                for step in (1, distance):
                    j = (i + step) % size
                    if i != j and not mol.GetBondBetweenAtoms(i, j):
                        mol.AddBond(i, j, Chem.BondType.SINGLE)
            emit(f"regular graph {size}/{distance}", mol)
            emit(f"regular classes {size}/{distance}", mol, break_ties=False)
            emit(f"regular basis rings {size}/{distance}", mol, ring_method="basis")
    for text in texts:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid corpus syntax: {text}")
        for ties in (False, True):
            for fragment in (False, True):
                for chirality, isotopes, maps in (
                    (True, True, True),
                    (False, True, True),
                    (True, False, True),
                    (True, True, False),
                ):
                    emit(
                        text,
                        mol,
                        fragment=fragment,
                        break_ties=ties,
                        chirality=chirality,
                        isotopes=isotopes,
                        maps=maps,
                    )
            emit(text + " presence", mol, break_ties=ties, chirality=False, presence=True)
        # Cis/trans ranks depend on their defining neighbor atoms. Verify both
        # equivalent and reversed reference choices independently of E/Z labels.
        for bond in mol.GetBonds():
            if len(bond.GetStereoAtoms()) == 2:
                for stereo in (Chem.BondStereo.STEREOCIS, Chem.BondStereo.STEREOTRANS):
                    bond.SetStereo(stereo)
                    emit(text + " relative bond stereo", mol)
        if mol.GetNumAtoms():
            for sample in range(5):
                perm = list(range(mol.GetNumAtoms()))
                rng.shuffle(perm)
                p = Chem.RenumberAtoms(mol, perm)
                for atom in p.GetAtoms():
                    if rng.random() < 0.25:
                        atom.SetAtomMapNum(rng.randrange(1, 20))
                emit(f"{text} permuted/mapped {sample}", p)
                emit(f"{text} fragment {sample}", p, fragment=True)
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid corpus syntax: {text}")
        emit(text + " raw", mol)
        emit(text + " raw fast rings", mol, ring_method="fast")
        emit(text + " raw fragment", mol, fragment=True)
        try:
            Chem.SanitizeMol(mol)
        except (ValueError, RuntimeError):
            continue
        emit(text + " sanitized", mol)
        emit(text + " symmetry", mol, break_ties=False)
        emit(text + " fragment", mol, fragment=True)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order))
        emit(text + " explicit H", Chem.AddHs(mol))


if __name__ == "__main__":
    main()
