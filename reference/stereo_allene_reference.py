"""stereo_reference.py's oracle with #892's allene cleanup applied first.

ReShiki's chirality cleanup clears tag 5 and its winding on atoms that are not
supported allene centres; RDKit keeps both. stereo_reference.py is pinned by
the aromatic goldens, so this wrapper runs it unchanged with that one edit
before each `Chem.CleanupChirality`.
"""

from typing import TYPE_CHECKING

from rdkit import Chem

if TYPE_CHECKING or __package__:
    from . import stereo_reference
else:
    import stereo_reference


def supported_allene(mol, center):
    """crates/chemistry/src/stereo/allene.rs `supported` for tag 5: a winding
    of 1 or 2 on a neutral sp carbon between two terminal carbons."""
    atom = mol.GetAtomWithIdx(center)
    if not atom.HasProp("_chiralPermutation") or atom.GetUnsignedProp("_chiralPermutation") not in (
        1,
        2,
    ):
        return False

    def carbon(a):
        return (
            a.GetAtomicNum() == 6
            and not a.GetIsAromatic()
            and a.GetFormalCharge() == 0
            and a.GetNumRadicalElectrons() == 0
        )

    def incident(index):
        return [b for b in mol.GetBonds() if index in (b.GetBeginAtomIdx(), b.GetEndAtomIdx())]

    ends = incident(center)
    if not carbon(atom) or atom.GetNumExplicitHs() != 0 or len(ends) != 2:
        return False
    for bond in ends:
        if bond.GetBondType() != Chem.BondType.DOUBLE or bond.GetIsAromatic():
            return False
        end = bond.GetOtherAtom(atom)
        if not carbon(end):
            return False
        others = [b for b in incident(end.GetIdx()) if b.GetIdx() != bond.GetIdx()]
        if any(b.GetBondType() != Chem.BondType.SINGLE or b.GetIsAromatic() for b in others):
            return False
        count, hydrogens = len(others), end.GetNumExplicitHs()
        if not 1 <= count <= 2 or hydrogens > 2 - count:
            return False
        if end.GetNoImplicit() and count + hydrogens != 2:
            return False
    return True


def clear_unsupported_allenes(mol):
    """#892: chirality cleanup clears tag 5 and its winding where
    `supported_allene` fails; RDKit keeps both. Clear them first so RDKit
    settles everything else, including the stereo groups."""
    for atom in mol.GetAtoms():
        if atom.GetChiralTag() == Chem.ChiralType.CHI_ALLENE and not supported_allene(
            mol, atom.GetIdx()
        ):
            atom.SetChiralTag(Chem.ChiralType.CHI_UNSPECIFIED)
            atom.ClearProp("_chiralPermutation")


class _Chem:
    """rdkit.Chem as stereo_reference.py sees it, with the edit above."""

    def __getattr__(self, name):
        return getattr(Chem, name)

    @staticmethod
    def CleanupChirality(mol):
        clear_unsupported_allenes(mol)
        Chem.CleanupChirality(mol)


if __name__ == "__main__":
    stereo_reference.Chem = _Chem()
    stereo_reference.main()
