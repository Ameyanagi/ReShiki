import json, sys
import rdkit
from rdkit import Chem
from rdkit.Chem import AllChem
from rdkit.Chem import rdForceFieldHelpers as H
print("rdkit", rdkit.__version__)
FIX = "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ck-impl/native/geometry/tests/fixtures/"
TAG = {0: Chem.ChiralType.CHI_UNSPECIFIED, 1: Chem.ChiralType.CHI_TETRAHEDRAL_CW, 2: Chem.ChiralType.CHI_TETRAHEDRAL_CCW}

def load(name):
    d = json.load(open(FIX + name))
    m = Chem.RWMol()
    for a in d["atoms"]:
        at = Chem.Atom(a["atomic_number"])
        at.SetFormalCharge(a["charge"]); at.SetNumExplicitHs(a["explicit_h"])
        at.SetNoImplicit(a["no_implicit"]); at.SetIsAromatic(a["aromatic"])
        at.SetChiralTag(TAG[a["chiral_tag"]])
        m.AddAtom(at)
    for b in d["bonds"]:
        t = {1: Chem.BondType.SINGLE, 2: Chem.BondType.DOUBLE, 3: Chem.BondType.TRIPLE, 12: Chem.BondType.AROMATIC}[b["order"]]
        m.AddBond(b["a"], b["b"], t)
        if b["aromatic"]: m.GetBondBetweenAtoms(b["a"], b["b"]).SetIsAromatic(True)
    m = m.GetMol(); Chem.SanitizeMol(m)
    return Chem.AddHs(m)

def params(basic=True):
    p = AllChem.ETKDGv3()
    p.randomSeed = 0x52534B; p.numThreads = 1; p.maxIterations = 100; p.timeout = 10
    p.enforceChirality = True; p.pruneRmsThresh = 0.1; p.clearConfs = True
    p.enableSequentialRandomSeeds = True; p.trackFailures = True
    if not basic:
        p.useBasicKnowledge = False; p.pruneRmsThresh = -1.0
    return p

def energies(mol, cid):
    out = {}
    for field in ["MMFF94", "MMFF94s", "UFF"]:
        m = Chem.Mol(mol)
        if field == "UFF":
            ff = H.UFFGetMoleculeForceField(m, vdwThresh=1e9, confId=cid, ignoreInterfragInteractions=True)
        else:
            ff = H.MMFFGetMoleculeForceField(m, H.MMFFGetMoleculeProperties(m, mmffVariant=field), nonBondedThresh=1e9, confId=cid, ignoreInterfragInteractions=True)
        e0 = ff.CalcEnergy(); st = ff.Minimize(maxIts=500, forceTol=1e-4, energyTol=1e-6)
        g = ff.CalcGrad(); import math
        out[field] = (round(e0, 4), round(ff.CalcEnergy(), 4), "conv" if st == 0 else "notconv", round(math.sqrt(sum(x*x for x in g)/len(g)), 5))
    return out

for name in sys.argv[1:]:
    mol = load(name)
    p = params()
    ids = list(AllChem.EmbedMultipleConfs(mol, 1, p))
    fails = " ".join(f"{i}:{c}" for i, c in enumerate(p.GetFailureCounts()) if c)
    print("==", name, "ETKDGv3 1conf ids", ids, "failures", fails)
    if ids: print("  ", energies(mol, ids[0]))
    else:
        p = params(basic=False)
        ids = list(AllChem.EmbedMultipleConfs(mol, 1, p))
        fails = " ".join(f"{i}:{c}" for i, c in enumerate(p.GetFailureCounts()) if c)
        print("   ETDG fallback ids", ids, "failures", fails)
        if ids: print("  ", energies(mol, ids[0]))
