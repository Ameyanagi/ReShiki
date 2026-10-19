import sys
import rdkit
from rdkit import Chem
from rdkit.Chem import AllChem
from rdkit.Chem import rdForceFieldHelpers as H
print("rdkit", rdkit.__version__)
SMI = sys.argv[1:] or ["c1ccncc1", "Cn1cnc2c1c(=O)n(C)c(=O)n2C", "CC(=O)NC", "c1ccccc1", "CC(=O)Oc1ccccc1C(=O)O"]
for smi in SMI:
    mol = Chem.AddHs(Chem.MolFromSmiles(smi))
    p = AllChem.ETKDGv3()
    p.randomSeed = 0x52534B
    p.numThreads = 1
    p.maxIterations = 100
    p.timeout = 10
    p.enforceChirality = True
    p.pruneRmsThresh = 0.1
    p.clearConfs = True
    p.enableSequentialRandomSeeds = True
    p.trackFailures = True
    ids = list(AllChem.EmbedMultipleConfs(mol, 4, p))
    fails = [ (i,c) for i,c in enumerate(p.GetFailureCounts()) if c]
    print("==", smi, "confs", len(ids), "fail", fails)
    for field in ["MMFF94", "MMFF94s", "UFF"]:
        rows = []
        for cid in ids:
            m = Chem.Mol(mol)
            if field == "UFF":
                ff = H.UFFGetMoleculeForceField(m, vdwThresh=1e9, confId=cid, ignoreInterfragInteractions=True)
            else:
                props = H.MMFFGetMoleculeProperties(m, mmffVariant=field)
                ff = H.MMFFGetMoleculeForceField(m, props, nonBondedThresh=1e9, confId=cid, ignoreInterfragInteractions=True)
            e0 = ff.CalcEnergy()
            st = ff.Minimize(maxIts=500, forceTol=1e-4, energyTol=1e-6)
            rows.append((ff.CalcEnergy(), e0, st, cid))
        best = min(rows)
        print(f"  {field}: per-conf initial={[round(r[1],4) for r in rows]} final={[round(r[0],4) for r in rows]} -> selected initial={best[1]:.4f} final={best[0]:.6f} conv={best[2]==0}")
