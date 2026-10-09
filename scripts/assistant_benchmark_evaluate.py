"""Independent RDKit scoring for public Assistant fixtures; no model call.

Coordinates are used only to read chemically meaningful drawn stereo when the
native graph has no authoritative assignment. Identity ignores layout. No
protonation/tautomer equivalence is allowed; aromatic/Kekule and explicit/implicit
ordinary hydrogen representations normalize through RDKit.
"""

import copy
import json
import sys
from collections import Counter
from pathlib import Path

from rdkit import Chem, rdBase
from rdkit.Chem import rdFMCS

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "reference"))
from engine.worker import from_document  # noqa: E402 — reference path is established above


def document_molecule(document):
    """Read actual retained chemistry through the development reference adapter.

    Its older document-version guard is overridden on a copy; chemistry is not
    dropped. Unsupported centroid/variable/coordination features receive an
    explicit unsupported score. Abbreviations retain all graph atoms and bonds.
    """
    doc = copy.deepcopy(document)
    if not 1 <= doc.get("version", 0) <= 19:
        raise ValueError("Unsupported native document version")
    for atom in doc["atoms"]:
        if (
            atom.get("centroid")
            or atom.get("attachment")
            or atom.get("display", {}).get("variable")
        ):
            raise ValueError(
                "Centroid/variable/haptic chemistry has no reference policy in this cohort"
            )
    doc["version"] = 15
    # The graph is complete even when an abbreviation's display metadata is bad;
    # score labels/member chemistry separately below.
    doc["abbreviations"] = []
    for bond in doc["bonds"]:
        if bond.get("projection") or bond.get("display") in ("bold", "dashed", "hashed"):
            bond["display"] = "plain"
    mol = from_document(doc)
    ids = {int(a.GetProp("reshiki_id")): a.GetIdx() for a in mol.GetAtoms()}
    # The reference adapter has already assigned atom chirality and native
    # cis/trans state. Clear derived neighbor directions before canonicalizing
    # it; otherwise cleanIt=True can overwrite explicit stereo with old layout.
    for bond in mol.GetBonds():
        bond.SetBondDir(Chem.BondDir.NONE)
    for bond in doc["bonds"]:
        if bond.get("stereo_authoritative") and not bond.get("stereo") and bond["order"] == 2:
            mol.GetBondBetweenAtoms(ids[bond["a"]], ids[bond["b"]]).SetStereo(
                Chem.BondStereo.STEREONONE
            )
    Chem.SetDoubleBondNeighborDirections(mol)
    Chem.AssignStereochemistry(mol, cleanIt=True, force=True)
    return Chem.RemoveHs(mol)


def attributes(atom):
    return (
        atom.GetAtomicNum(),
        atom.GetIsotope(),
        atom.GetFormalCharge(),
        atom.GetNumRadicalElectrons(),
    )


def atom_id(atom):
    return int(atom.GetProp("reshiki_id")) if atom.HasProp("reshiki_id") else atom.GetIdx() + 1


def atom_cip(atom):
    return atom.GetProp("_CIPCode") if atom.HasProp("_CIPCode") else None


def bond_kind(bond):
    return "aromatic" if bond.GetIsAromatic() else str(bond.GetBondType())


def bond_stereo(bond):
    return (
        str(bond.GetStereo())
        if bond.GetStereo() in (Chem.BondStereo.STEREOE, Chem.BondStereo.STEREOZ)
        else None
    )


def mapping_penalty(reference, observed, mapping):
    penalty = 0
    for a, b in mapping.items():
        ar, ao = reference.GetAtomWithIdx(a), observed.GetAtomWithIdx(b)
        penalty += 8 * (attributes(ar) != attributes(ao))
        penalty += 2 * (atom_cip(ar) != atom_cip(ao))
    for bond in reference.GetBonds():
        a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
        if a in mapping and b in mapping:
            other = observed.GetBondBetweenAtoms(mapping[a], mapping[b])
            penalty += 6 if other is None else 3 * (bond_kind(bond) != bond_kind(other))
    return penalty


def graph_mapping(reference, observed):
    """Bounded topology mapping; localization is descriptive, not a unique edit proof."""
    if not reference.GetNumAtoms() or not observed.GetNumAtoms():
        return {}, False
    common = rdFMCS.FindMCS(
        [reference, observed],
        timeout=2,
        atomCompare=rdFMCS.AtomCompare.CompareAny,
        bondCompare=rdFMCS.BondCompare.CompareAny,
        matchValences=False,
        matchChiralTag=False,
    )
    if not common.smartsString:
        return {}, common.canceled
    query = Chem.MolFromSmarts(common.smartsString)
    references = reference.GetSubstructMatches(query, uniquify=False, maxMatches=64)
    observations = observed.GetSubstructMatches(query, uniquify=False, maxMatches=64)
    candidates = []
    for r in references:
        for o in observations:
            mapping = dict(zip(r, o))
            # Complete disconnected fragments/remaining atoms deterministically.
            # Prefer equal atom attributes and already mapped neighbor connectivity.
            available = set(range(observed.GetNumAtoms())) - set(mapping.values())
            for a in range(reference.GetNumAtoms()):
                if a in mapping or not available:
                    continue
                ar = reference.GetAtomWithIdx(a)
                ranked = []
                for b in available:
                    ao = observed.GetAtomWithIdx(b)
                    neighbors = sum(
                        observed.GetBondBetweenAtoms(b, mapping[n.GetIdx()]) is not None
                        for n in ar.GetNeighbors()
                        if n.GetIdx() in mapping
                    )
                    ranked.append(
                        (
                            attributes(ar) != attributes(ao),
                            -neighbors,
                            abs(ar.GetDegree() - ao.GetDegree()),
                            b,
                        )
                    )
                best = min(ranked)[-1]
                # Do not map a missing isolated ion to an unrelated extra atom.
                if ar.GetDegree() == 0 and attributes(ar) != attributes(
                    observed.GetAtomWithIdx(best)
                ):
                    continue
                mapping[a] = best
                available.remove(best)
            candidates.append(
                (mapping_penalty(reference, observed, mapping), sorted(mapping.items()), mapping)
            )
    return min(candidates, key=lambda item: (item[0], item[1]))[
        2
    ] if candidates else {}, common.canceled


def abbreviation_scores(document, observed, expected):
    groups = document.get("abbreviations", [])
    expected_labels = Counter(item["label"] for item in expected)
    actual_labels = Counter(group["label"] for group in groups)
    errors = []
    by_id = {atom_id(a): a.GetIdx() for a in observed.GetAtoms()}
    patterns = {
        item["label"]: Chem.MolToSmiles(Chem.MolFromSmiles(item["smiles"])) for item in expected
    }
    for group in groups:
        if group["label"] not in patterns:
            continue
        members = group.get("members", [])
        anchor = group.get("anchor")
        if (
            anchor not in members
            or len(set(members)) != len(members)
            or any(i not in by_id for i in members)
        ):
            errors.append({"label": group["label"], "error": "invalid member IDs/anchor"})
            continue
        rw = Chem.RWMol()
        indices = {}
        for ident in members:
            indices[by_id[ident]] = rw.AddAtom(Chem.Atom(observed.GetAtomWithIdx(by_id[ident])))
        for bond in observed.GetBonds():
            a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
            if a in indices and b in indices:
                rw.AddBond(indices[a], indices[b], bond.GetBondType())
        external = [
            b
            for b in observed.GetAtomWithIdx(by_id[anchor]).GetBonds()
            if b.GetOtherAtomIdx(by_id[anchor]) not in indices
        ]
        if len(external) != 1:
            errors.append(
                {"label": group["label"], "error": "expected exactly one outside attachment"}
            )
            continue
        dummy = rw.AddAtom(Chem.Atom(0))
        rw.AddBond(dummy, indices[by_id[anchor]], external[0].GetBondType())
        try:
            fragment = rw.GetMol()
            Chem.SanitizeMol(fragment)
            value = Chem.MolToSmiles(fragment)
            if value != patterns[group["label"]]:
                errors.append(
                    {
                        "label": group["label"],
                        "expected": patterns[group["label"]],
                        "observed": value,
                    }
                )
        except ValueError as error:
            errors.append({"label": group["label"], "error": str(error)})
    return {
        "missing_display_labels": dict(expected_labels - actual_labels),
        "extra_display_labels": dict(actual_labels - expected_labels),
        "member_chemistry_errors": errors,
        "policy": "Complete retained graph is scored regardless of collapsed/expanded display. Label changes are separate from chemical identity.",
    }


def score_document(document, expected):
    reference = Chem.MolFromSmiles(expected["smiles"])
    Chem.AssignStereochemistry(reference, cleanIt=True, force=True)
    try:
        observed = document_molecule(document)
    except (ValueError, KeyError, RuntimeError) as error:
        return {
            "graph_identity": False,
            "reference_adapter_error": str(error),
            "observed_atoms": len(document.get("atoms", [])),
            "observed_bonds": len(document.get("bonds", [])),
            "localized_metrics_available": False,
        }
    mapping, capped = graph_mapping(reference, observed)
    inverse = {b: a for a, b in mapping.items()}
    atoms, hydrogens, stereochemistry = [], [], []
    for a, b in mapping.items():
        ar, ao = reference.GetAtomWithIdx(a), observed.GetAtomWithIdx(b)
        base = {"reference_atom": a + 1, "observed_atom_id": atom_id(ao)}
        if attributes(ar) != attributes(ao):
            atoms.append(dict(base, expected=attributes(ar), observed=attributes(ao)))
        if ar.GetTotalNumHs() != ao.GetTotalNumHs():
            hydrogens.append(dict(base, expected=ar.GetTotalNumHs(), observed=ao.GetTotalNumHs()))
        if atom_cip(ar) != atom_cip(ao):
            stereochemistry.append(dict(base, expected=atom_cip(ar), observed=atom_cip(ao)))
    missing_atoms = [
        {"reference_atom": a.GetIdx() + 1, "element": a.GetSymbol()}
        for a in reference.GetAtoms()
        if a.GetIdx() not in mapping
    ]
    extra_atoms = [
        {"observed_atom_id": atom_id(a), "element": a.GetSymbol()}
        for a in observed.GetAtoms()
        if a.GetIdx() not in inverse
    ]
    missing_bonds, extra_bonds, bond_errors = [], [], []
    for bond in reference.GetBonds():
        a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
        other = (
            observed.GetBondBetweenAtoms(mapping[a], mapping[b])
            if a in mapping and b in mapping
            else None
        )
        if other is None:
            missing_bonds.append({"reference_atoms": [a + 1, b + 1], "order": bond_kind(bond)})
        else:
            base = {
                "reference_atoms": [a + 1, b + 1],
                "observed_atom_ids": [
                    atom_id(observed.GetAtomWithIdx(mapping[a])),
                    atom_id(observed.GetAtomWithIdx(mapping[b])),
                ],
            }
            if bond_kind(bond) != bond_kind(other):
                bond_errors.append(dict(base, expected=bond_kind(bond), observed=bond_kind(other)))
            if bond_stereo(bond) != bond_stereo(other):
                stereochemistry.append(
                    dict(base, expected=bond_stereo(bond), observed=bond_stereo(other))
                )
    for bond in observed.GetBonds():
        a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
        if (
            a not in inverse
            or b not in inverse
            or reference.GetBondBetweenAtoms(inverse[a], inverse[b]) is None
        ):
            extra_bonds.append(
                {
                    "observed_atom_ids": [
                        atom_id(observed.GetAtomWithIdx(a)),
                        atom_id(observed.GetAtomWithIdx(b)),
                    ],
                    "order": bond_kind(bond),
                }
            )
    reference_smiles = Chem.MolToSmiles(reference, isomericSmiles=True)
    observed_smiles = Chem.MolToSmiles(observed, isomericSmiles=True)
    return {
        "graph_identity": reference_smiles == observed_smiles,
        "reference_smiles": reference_smiles,
        "observed_smiles": observed_smiles,
        "reference_atoms": reference.GetNumAtoms(),
        "observed_atoms": observed.GetNumAtoms(),
        "reference_bonds": reference.GetNumBonds(),
        "observed_bonds": observed.GetNumBonds(),
        "atom_errors": atoms,
        "hydrogen_errors": hydrogens,
        "missing_atoms": missing_atoms,
        "extra_atoms": extra_atoms,
        "bond_errors": bond_errors,
        "missing_bonds": missing_bonds,
        "extra_bonds": extra_bonds,
        "stereochemistry_errors": stereochemistry,
        "reference_fragments": len(Chem.GetMolFrags(reference)),
        "observed_fragments": len(Chem.GetMolFrags(observed)),
        "missing_fragments": [
            list(a + 1 for a in f)
            for f in Chem.GetMolFrags(reference)
            if all(a not in mapping for a in f)
        ],
        "extra_fragments": [
            list(atom_id(observed.GetAtomWithIdx(a)) for a in f)
            for f in Chem.GetMolFrags(observed)
            if all(a not in inverse for a in f)
        ],
        "abbreviations": abbreviation_scores(document, observed, expected.get("abbreviations", [])),
        "localization_search_timed_out": capped,
        "localized_metrics_available": True,
        "reference_checker": f"RDKit {rdBase.rdkitVersion}",
    }


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--document", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = score_document(
        json.loads(args.document.read_text()), json.loads(args.reference.read_text())
    )
    value = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(value)
    else:
        print(value, end="")
