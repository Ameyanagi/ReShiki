"""Independent source-MOL expectations for the documented display-H folding.

The frozen response remains the oracle for every unrelated response field.
RDKit parses the original file, removes only eligible ordinary H, adjusts
stereochemistry, and supplies the replacement drawing/SMILES. No actual Rust
response or Rust parser output enters this calculation. XYZ/unknown controls
are applied separately by the shared source-coordinate reference guard.
"""

from collections import Counter
from copy import deepcopy

from rdkit import Chem
from rdkit.Chem import rdCIPLabeler

from engine.worker import analyze, to_document

ORDINARY_H_PARENTS = frozenset((5, 6, 7, 8, 9, 14, 15, 16, 17, 34, 35, 53))


def _unknown(item):
    return bool(item.HasProp("_UnknownStereo") and item.GetIntProp("_UnknownStereo"))


def _annotated_atoms(mol):
    protected = set()
    for group in Chem.GetMolSubstanceGroups(mol):
        protected.update(group.GetAtoms())
        protected.update(group.GetParentAtoms())
        for attachment in group.GetAttachPoints():
            protected.add(attachment.aIdx)
            if attachment.lvIdx >= 0:
                protected.add(attachment.lvIdx)
        for bond_index in group.GetBonds():
            bond = mol.GetBondWithIdx(bond_index)
            protected.update((bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()))
    return protected


def _ordinary_h(atom, annotated):
    if (
        atom.GetAtomicNum() != 1
        or atom.GetIsotope()
        or atom.GetFormalCharge()
        or atom.GetNumRadicalElectrons()
        or atom.HasProp("molAtomMapNumber")
        or _unknown(atom)
        or atom.HasProp("molAttchpt")
        or atom.HasProp("dummyLabel")
        or atom.GetIdx() in annotated
        or atom.GetDegree() != 1
    ):
        return False
    bond = next(iter(atom.GetBonds()))
    return (
        bond.GetOtherAtom(atom).GetAtomicNum() in ORDINARY_H_PARENTS
        and bond.GetBondType() == Chem.BondType.SINGLE
        and bond.GetStereo() == Chem.BondStereo.STEREONONE
        and bond.GetBondDir() != Chem.BondDir.UNKNOWN
        and not _unknown(bond)
    )


def _inventory(mol):
    result = Counter()
    for atom in mol.GetAtoms():
        result[
            atom.GetAtomicNum(),
            atom.GetIsotope(),
            atom.GetFormalCharge(),
            atom.GetNumRadicalElectrons(),
        ] += 1
        result[1, 0, 0, 0] += atom.GetNumExplicitHs() + atom.GetNumImplicitHs()
    return +result


def _cip(mol):
    work = Chem.Mol(mol)
    for item in list(work.GetAtoms()) + list(work.GetBonds()):
        if item.HasProp("_CIPCode"):
            item.ClearProp("_CIPCode")
    rdCIPLabeler.AssignCIPLabels(work, maxRecursiveIterations=1_250_000)

    def label(item):
        return item.GetProp("_CIPCode") if item.HasProp("_CIPCode") else None

    return (
        {int(a.GetProp("reshiki_id")): label(a) for a in work.GetAtoms()},
        {
            tuple(
                sorted(
                    (
                        int(b.GetBeginAtom().GetProp("reshiki_id")),
                        int(b.GetEndAtom().GetProp("reshiki_id")),
                    )
                )
            ): label(b)
            for b in work.GetBonds()
        },
    )


def fold_source_hydrogens(source):
    """Return RDKit's folded copy, retaining source atom IDs and protected H.

    Temporary nonzero maps make ineligible H opaque to public RemoveHs; maps
    do not change CIP priority and are restored before any comparison. RDKit's
    own defining-double-stereo and non-tetrahedral guards remain enabled.
    """
    before = Chem.Mol(source)
    for atom in before.GetAtoms():
        atom.SetProp("reshiki_id", str(atom.GetIdx() + 1))
    work = Chem.Mol(before)
    annotated = _annotated_atoms(work)
    candidates = set()
    temporary_maps = {}
    for atom in work.GetAtoms():
        if _ordinary_h(atom, annotated):
            candidates.add(atom.GetIdx() + 1)
        elif atom.GetAtomicNum() == 1:
            identity = atom.GetIdx() + 1
            temporary_maps[identity] = (
                atom.GetIntProp("molAtomMapNumber") if atom.HasProp("molAtomMapNumber") else None
            )
            atom.SetAtomMapNum(1)
    if not candidates:
        return before
    parameters = Chem.RemoveHsParameters()
    parameters.updateExplicitCount = True
    parameters.removeMapped = False
    parameters.removeInSGroups = False
    parameters.showWarnings = False
    folded = Chem.RemoveHs(work, parameters, sanitize=True)
    for atom in folded.GetAtoms():
        identity = int(atom.GetProp("reshiki_id"))
        if identity in temporary_maps:
            original = temporary_maps[identity]
            if original is None:
                atom.ClearProp("molAtomMapNumber")
            else:
                # SetAtomMapNum(0) drops the property; an explicitly present
                # map zero is chemically distinct in the frozen source path.
                atom.SetIntProp("molAtomMapNumber", original)
        if atom.GetNumExplicitHs() > 255:
            raise ValueError("Source MOL folded H count exceeds byte storage")
    Chem.AssignStereochemistry(folded, cleanIt=True, force=True, flagPossibleStereoCenters=True)
    kept = {int(a.GetProp("reshiki_id")) for a in folded.GetAtoms()}
    removed = set(range(1, before.GetNumAtoms() + 1)) - kept
    if not removed <= candidates:
        raise ValueError("RDKit removed a protected source MOL hydrogen")
    if _inventory(before) != _inventory(folded):
        raise ValueError("Source MOL hydrogen folding changed composition")
    old_atoms, old_bonds = _cip(before)
    new_atoms, new_bonds = _cip(folded)
    if any(old_atoms[k] != v for k, v in new_atoms.items()) or any(
        old_bonds[k] != v for k, v in new_bonds.items()
    ):
        raise ValueError("Source MOL hydrogen folding changed full CIP descriptors")
    return folded


def display_expectation(text, legacy_response):
    """Emit an exact old-shape guard and independently calculated folded shape.

    None means the source has no H removed by the display policy. Metadata is
    deliberately restricted to the changed drawing arrays and canonical SMILES;
    all other drawing and response fields keep their original exact guard.
    """
    if legacy_response is None:
        return None
    source = Chem.MolFromMolBlock(text, removeHs=False, strictParsing=True)
    if source is None:
        raise ValueError("RDKit could not independently read accepted source MOL")
    if not any(a.GetAtomicNum() == 1 for a in source.GetAtoms()):
        return None
    folded = fold_source_hydrogens(source)
    kept = [int(a.GetProp("reshiki_id")) for a in folded.GetAtoms()]
    removed = sorted(set(range(1, source.GetNumAtoms() + 1)) - set(kept))
    if not removed:
        return None
    legacy_document = to_document(source)
    legacy_analysis = analyze(source)
    if legacy_response.get("document") != legacy_document:
        raise ValueError("Frozen MOL response no longer matches independent old drawing")
    if legacy_response.get("analysis") != legacy_analysis:
        raise ValueError("Frozen MOL response no longer matches independent old analysis")
    replacement = to_document(folded)
    document = deepcopy(legacy_document)
    document["atoms"] = replacement["atoms"]
    document["bonds"] = replacement["bonds"]
    return dict(
        removed_atom_ids=removed,
        kept_atom_ids=kept,
        legacy_document=legacy_document,
        legacy_analysis=legacy_analysis,
        document=document,
        analysis_smiles=Chem.MolToSmiles(folded),
    )
