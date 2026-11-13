"""Lift the frozen worker's refusal of three or four radical electrons.

The original worker refused any atom with more than two radical electrons.
ReShiki keeps 0-4, as RDKit reads them (``[C]`` has four, ``[CH]`` and ``[N]``
three), so references compare those imports with the frozen worker's own
response computed without that one refusal. Its other checks still apply.
Import this module before anything imports ``check_supported`` by name.
"""

from rdkit import Chem

from engine import worker

_check_supported = worker.check_supported


def check_supported(mol):
    """The frozen check, applied to a copy without its counts of three or four.

    RDKit can derive more than four (``[LiH2]`` has five); ReShiki still
    refuses those, as the frozen worker does.
    """
    copy = Chem.RWMol(mol)
    for atom in copy.GetAtoms():
        if atom.GetNumRadicalElectrons() in (3, 4):
            atom.SetNumRadicalElectrons(0)
    _check_supported(copy)


worker.check_supported = check_supported
