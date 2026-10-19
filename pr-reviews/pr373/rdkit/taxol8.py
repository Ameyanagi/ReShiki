import sys
sys.path.insert(0, "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ck-compare/rdkit")
from fixture_check import load, params
from rdkit import Chem, rdBase
from rdkit.Chem import AllChem
from rdkit.Chem import rdForceFieldHelpers as H
print(rdBase.rdkitVersion)
m = load("taxol70-request.json"); p = params()
print("ncauses", len(p.GetFailureCounts()))
ids = list(AllChem.EmbedMultipleConfs(m, 8, p))
for field in ["MMFF94", "UFF"]:
    rows = []
    for cid in ids:
        x = Chem.Mol(m)
        ff = (H.UFFGetMoleculeForceField(x, vdwThresh=1e9, confId=cid, ignoreInterfragInteractions=True) if field == "UFF"
              else H.MMFFGetMoleculeForceField(x, H.MMFFGetMoleculeProperties(x, mmffVariant=field), nonBondedThresh=1e9, confId=cid, ignoreInterfragInteractions=True))
        e0 = ff.CalcEnergy(); st = ff.Minimize(maxIts=500, forceTol=1e-4, energyTol=1e-6)
        rows.append((round(ff.CalcEnergy(), 4), round(e0, 4), st))
    print(field, sorted(rows))
