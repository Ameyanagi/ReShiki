import sys
sys.path.insert(0, "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ck-compare/rdkit")
from fixture_check import load, params
from rdkit import Chem
from rdkit.Chem import AllChem
from rdkit.Chem import rdForceFieldHelpers as H
m = load("taxol70-request.json"); p = params()
ids = list(AllChem.EmbedMultipleConfs(m, 8, p))
print("taxol 8conf ids", ids, "failures", " ".join(f"{i}:{c}" for i, c in enumerate(p.GetFailureCounts()) if c))
m = load("c60-request.json"); p = params(); p.useBasicKnowledge = False
ids = list(AllChem.EmbedMultipleConfs(m, 8, p))
print("c60 cage-ETDG 8conf prune0.1 ids", ids)
sf6 = Chem.AddHs(Chem.MolFromSmiles("FS(F)(F)(F)(F)F"))
print("SF6 MMFFHasAllMoleculeParams", H.MMFFHasAllMoleculeParams(sf6), "UFFHasAllMoleculeParams", H.UFFHasAllMoleculeParams(sf6))
