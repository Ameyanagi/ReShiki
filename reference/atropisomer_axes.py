"""RDKit's atropisomer axes for reference/support/axial_extension.rs.

Each input line is {"format", "text"}; each output line lists the molecules
the original worker would read, in its order (reaction reactants, products,
then agents), with every bond RDKit marks as an atropisomer axis.
"""

import json
import sys

from rdkit import Chem, RDLogger
from rdkit.Chem import rdChemReactions, rdCIPLabeler

# ReShiki keeps RDKit's legacy ring symmetrization; child processes inherit it.
Chem.SetUseLegacyRingFinding(True)


def molecules(fmt, text):
    if fmt == "mol":
        return [Chem.MolFromMolBlock(text, removeHs=False)]
    if fmt == "smiles":
        return [Chem.MolFromSmiles(text)]
    if fmt == "rxn":
        reaction = rdChemReactions.ReactionFromRxnBlock(text, sanitize=True, removeHs=False)
        result = []
        for original in (*reaction.GetReactants(), *reaction.GetProducts(), *reaction.GetAgents()):
            molecule = Chem.RWMol(original)
            # As reference/engine/reactions.py: strip the RXN element wrapper.
            for atom in molecule.GetAtoms():
                if atom.HasQuery():
                    molecule.ReplaceAtom(atom.GetIdx(), Chem.Atom(atom), preserveProps=True)
            Chem.SanitizeMol(molecule)
            result.append(molecule)
        return result
    raise ValueError(f"Unsupported format {fmt}")


def axes(mol):
    rdCIPLabeler.AssignCIPLabels(mol)
    return [
        dict(
            atoms=sorted([b.GetBeginAtomIdx(), b.GetEndAtomIdx()]),
            stereo=int(b.GetStereo()),
            label=b.GetProp("_CIPCode") if b.HasProp("_CIPCode") else None,
        )
        for b in mol.GetBonds()
        if int(b.GetStereo()) in (6, 7)
    ]


def main():
    RDLogger.DisableLog("rdApp.*")
    for line in sys.stdin:
        request = json.loads(line)
        try:
            result = [axes(m) for m in molecules(request["format"], request["text"])]
        except (ValueError, RuntimeError, AttributeError) as error:
            result = str(error)
        print(json.dumps(result), flush=True)


if __name__ == "__main__":
    main()
