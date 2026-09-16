"""Versioned JSON-lines chemistry service. No editor state or UI dependencies."""
import json
import math
import sys
import xml.etree.ElementTree as ET

from rdkit import Chem, rdBase
from rdkit.Chem import Descriptors, rdDepictor, rdMolDescriptors

SCALE = 28.0
ORDERS = {1: Chem.BondType.SINGLE, 2: Chem.BondType.DOUBLE,
          3: Chem.BondType.TRIPLE, 4: Chem.BondType.AROMATIC}
STEREO = {"cis": Chem.BondStereo.STEREOCIS, "trans": Chem.BondStereo.STEREOTRANS,
          "z": Chem.BondStereo.STEREOZ, "e": Chem.BondStereo.STEREOE}
DIRECTIONS = {"wedge": Chem.BondDir.BEGINWEDGE, "hash": Chem.BondDir.BEGINDASH,
              "wavy": Chem.BondDir.UNKNOWN, "plain": Chem.BondDir.NONE}


def check_supported(mol):
    if mol.GetStereoGroups():
        raise ValueError("Enhanced stereo groups are not supported yet; import was cancelled.")
    for a in mol.GetAtoms():
        if a.HasQuery() or a.GetNumRadicalElectrons():
            raise ValueError("Query atoms and radicals are not supported yet; import was cancelled.")
        if a.GetChiralTag() not in (Chem.ChiralType.CHI_UNSPECIFIED,
                                   Chem.ChiralType.CHI_TETRAHEDRAL_CW,
                                   Chem.ChiralType.CHI_TETRAHEDRAL_CCW):
            raise ValueError("This stereochemistry class is not supported yet.")
    for b in mol.GetBonds():
        if b.HasQuery() or b.GetBondType() not in ORDERS.values():
            raise ValueError("This bond type is not supported yet.")
        if b.GetStereo() not in (*STEREO.values(), Chem.BondStereo.STEREONONE,
                                Chem.BondStereo.STEREOANY):
            raise ValueError("This bond stereochemistry is not supported yet.")


def from_document(doc):
    if doc.get("version") not in (1, 2):
        raise ValueError("Unsupported document version")
    rw = Chem.RWMol()
    ids = {}
    for item in doc["atoms"]:
        atom_id = item["id"]
        if atom_id in ids or not isinstance(atom_id, int) or atom_id < 1:
            raise ValueError("Atom IDs must be unique positive integers")
        a = Chem.Atom(item["element"])
        a.SetFormalCharge(item.get("charge", 0))
        a.SetIsotope(item.get("isotope", 0))
        a.SetNumExplicitHs(item.get("explicit_h", 0))
        a.SetNoImplicit(item.get("no_implicit", False))
        a.SetIsAromatic(item.get("aromatic", False))
        a.SetAtomMapNum(item.get("map_num", 0))
        a.SetProp("moruno_id", str(atom_id))
        ids[atom_id] = rw.AddAtom(a)
    for item in doc["bonds"]:
        rw.AddBond(ids[item["a"]], ids[item["b"]], ORDERS[item["order"]])
        b = rw.GetBondBetweenAtoms(ids[item["a"]], ids[item["b"]])
        b.SetBondDir(DIRECTIONS[item.get("display", "plain")])
    mol = rw.GetMol()
    conf = Chem.Conformer(mol.GetNumAtoms())
    conf.Set3D(False)
    for item in doc["atoms"]:
        x, y = item["position"]["x"], item["position"]["y"]
        if not math.isfinite(x) or not math.isfinite(y):
            raise ValueError("Invalid atom coordinates")
        conf.SetAtomPosition(ids[item["id"]], (x / SCALE, -y / SCALE, 0.0))
        a = mol.GetAtomWithIdx(ids[item["id"]])
        stereo = item.get("stereo")
        if stereo:
            current = [int(n.GetProp("moruno_id")) for n in a.GetNeighbors()]
            given = stereo["neighbors"]
            if set(current) != set(given) or len(current) != len(given):
                raise ValueError("Stereocenter neighbor mapping changed")
            permutation = [given.index(n) for n in current]
            odd = sum(permutation[i] > permutation[j] for i in range(len(permutation))
                      for j in range(i+1, len(permutation))) % 2
            clockwise = stereo["winding"] == "cw"
            if odd:
                clockwise = not clockwise
            a.SetChiralTag(Chem.ChiralType.CHI_TETRAHEDRAL_CW if clockwise
                          else Chem.ChiralType.CHI_TETRAHEDRAL_CCW)
    mol.AddConformer(conf)
    Chem.SanitizeMol(mol)
    Chem.AssignChiralTypesFromBondDirs(mol, replaceExistingTags=False)
    for item in doc["bonds"]:
        b = mol.GetBondBetweenAtoms(ids[item["a"]], ids[item["b"]])
        if item.get("stereo") in STEREO:
            neighbors = item["stereo_atoms"]
            b.SetStereoAtoms(ids[neighbors[0]], ids[neighbors[1]])
            b.SetStereo(STEREO[item["stereo"]])
        elif item.get("display") == "wavy" and item["order"] == 2:
            b.SetStereo(Chem.BondStereo.STEREOANY)
    # Infer double-bond stereo from drawn geometry where it is unspecified.
    Chem.DetectBondStereochemistry(mol, confId=0)
    Chem.AssignStereochemistry(mol, cleanIt=False, force=True)
    check_supported(mol)
    return mol


def to_document(mol, base=None):
    check_supported(mol)
    if not mol.GetNumConformers():
        rdDepictor.Compute2DCoords(mol)
    work = Chem.Mol(mol)
    Chem.Kekulize(work, clearAromaticFlags=True)
    Chem.WedgeMolBonds(work, work.GetConformer())
    ids = {a.GetIdx(): int(a.GetProp("moruno_id")) if a.HasProp("moruno_id")
           else a.GetIdx()+1 for a in work.GetAtoms()}
    atoms = []
    for a in work.GetAtoms():
        p = work.GetConformer().GetAtomPosition(a.GetIdx())
        stereo = None
        if a.GetChiralTag() != Chem.ChiralType.CHI_UNSPECIFIED:
            stereo = {"winding": "cw" if a.GetChiralTag() == Chem.ChiralType.CHI_TETRAHEDRAL_CW else "ccw",
                      "neighbors": [ids[n.GetIdx()] for n in a.GetNeighbors()]}
        atoms.append({"id": ids[a.GetIdx()], "element": a.GetSymbol(),
                      "position": {"x": p.x*SCALE, "y": -p.y*SCALE},
                      "charge": a.GetFormalCharge(), "isotope": a.GetIsotope(),
                      "explicit_h": a.GetNumExplicitHs(), "no_implicit": a.GetNoImplicit(),
                      "aromatic": a.GetIsAromatic(), "map_num": a.GetAtomMapNum(),
                      "stereo": stereo, "label_h": a.GetTotalNumHs()})
    bonds = []
    reverse_stereo = {value: key for key, value in STEREO.items()}
    for b in work.GetBonds():
        display = {Chem.BondDir.BEGINWEDGE: "wedge", Chem.BondDir.BEGINDASH: "hash",
                   Chem.BondDir.UNKNOWN: "wavy"}.get(b.GetBondDir(), "plain")
        if b.GetStereo() == Chem.BondStereo.STEREOANY:
            display = "wavy"
        bonds.append({"a": ids[b.GetBeginAtomIdx()], "b": ids[b.GetEndAtomIdx()],
                      "order": int(b.GetBondTypeAsDouble()), "display": display,
                      "stereo": reverse_stereo.get(b.GetStereo()),
                      "stereo_atoms": [ids[i] for i in b.GetStereoAtoms()]})
    return {"version": 2, "atoms": atoms, "bonds": bonds,
            "annotations": (base or {}).get("annotations", []),
            "arrows": (base or {}).get("arrows", [])}


def analyze(mol):
    return {"smiles": Chem.MolToSmiles(mol), "formula": rdMolDescriptors.CalcMolFormula(mol),
            "mass": Descriptors.MolWt(mol), "exact_mass": Descriptors.ExactMolWt(mol),
            "logp": Descriptors.MolLogP(mol), "tpsa": Descriptors.TPSA(mol),
            "donors": rdMolDescriptors.CalcNumHBD(mol),
            "acceptors": rdMolDescriptors.CalcNumHBA(mol),
            "rings": rdMolDescriptors.CalcNumRings(mol),
            "inchi": Chem.MolToInchi(mol), "inchikey": Chem.MolToInchiKey(mol)}


def import_cdxml(text):
    root = ET.fromstring(text)
    allowed = {"CDXML", "page", "fragment", "n", "b", "t", "s", "fonttable", "font", "colortable", "color", "arrow"}
    if {el.tag for el in root.iter()} - allowed or len(list(root.iter("page"))) != 1:
        raise ValueError("CDXML contains unsupported drawing objects or multiple pages.")
    parts = Chem.MolsFromCDXML(text)
    if not parts:
        raise ValueError("No supported molecules found")
    mol = parts[0]
    for part in parts[1:]:
        mol = Chem.CombineMols(mol, part)
    scale = 42.0 / float(root.get("BondLength", "30"))
    base = {"annotations": [], "arrows": []}
    next_id = mol.GetNumAtoms() + 1
    def point(value):
        values = [float(v) for v in value.split()]
        if len(values) < 2 or not all(math.isfinite(v) for v in values):
            raise ValueError("Invalid CDXML coordinates")
        return {"x": values[0]*scale, "y": values[1]*scale}
    for page in root.iter("page"):
        for el in page:
            if el.tag == "t":
                base["annotations"].append({"id": next_id, "position": point(el.attrib["p"]),
                                            "text": "".join(el.itertext())})
                next_id += 1
            elif el.tag == "arrow":
                if el.get("ArrowheadTail", "None") != "None" or el.get("ArrowheadHead", "Full") != "Full" or el.get("AngularSize", "0") != "0":
                    raise ValueError("This CDXML arrow style is not supported; use native format to preserve it.")
                base["arrows"].append({"id": next_id, "start": point(el.attrib["Tail3D"]),
                                       "end": point(el.attrib["Head3D"]), "kind": "forward"})
                next_id += 1
    return mol, base


def export_cdxml(doc):
    # Coordinates and styles belong to the editor. Chemistry is checked first.
    from_document(doc)
    if any(a.get("kind", "forward") != "forward" for a in doc.get("arrows", [])):
        raise ValueError("CDXML currently supports forward arrows. Use native, SVG, PDF or PNG for other arrow styles.")
    root = ET.Element("CDXML", BondLength="42", LabelSize="11", CaptionSize="12")
    fonts = ET.SubElement(root, "fonttable")
    ET.SubElement(fonts, "font", id="3", charset="utf-8", name="Arial")
    page = ET.SubElement(root, "page", id="1", BoundingBox="0 0 612 792")
    fragment = ET.SubElement(page, "fragment", id="2")
    points = [a["position"] for a in doc["atoms"]]
    dx = 70-min((p["x"] for p in points), default=0)
    dy = 70-min((p["y"] for p in points), default=0)
    ids = {a["id"]: i+3 for i, a in enumerate(doc["atoms"])}
    for a in doc["atoms"]:
        attrs = {"id": str(ids[a["id"]]), "p": f'{a["position"]["x"]+dx:.4f} {a["position"]["y"]+dy:.4f}',
                 "Element": str(Chem.GetPeriodicTable().GetAtomicNumber(a["element"]))}
        if a.get("charge"): attrs["Charge"] = str(a["charge"])
        if a.get("isotope"): attrs["Isotope"] = str(a["isotope"])
        if a.get("explicit_h"): attrs["NumHydrogens"] = str(a["explicit_h"])
        ET.SubElement(fragment, "n", **attrs)
    next_id = len(ids)+3
    for b in doc["bonds"]:
        attrs = {"id": str(next_id), "B": str(ids[b["a"]]), "E": str(ids[b["b"]]),
                 "Order": str(b["order"]) if b["order"] != 4 else "1.5"}
        display = {"wedge": "WedgeBegin", "hash": "WedgedHashBegin", "wavy": "Wavy"}.get(b.get("display"))
        if display: attrs["Display"] = display
        ET.SubElement(fragment, "b", **attrs)
        next_id += 1
    for a in doc.get("annotations", []):
        t = ET.SubElement(page, "t", id=str(next_id), p=f'{a["position"]["x"]+dx:.4f} {a["position"]["y"]+dy:.4f}')
        ET.SubElement(t, "s", font="3", size="12").text = a["text"]
        next_id += 1
    for a in doc.get("arrows", []):
        ET.SubElement(page, "arrow", id=str(next_id), ArrowheadHead="Full", ArrowheadType="Solid",
                      Tail3D=f'{a["start"]["x"]+dx} {a["start"]["y"]+dy} 0',
                      Head3D=f'{a["end"]["x"]+dx} {a["end"]["y"]+dy} 0')
        next_id += 1
    return '<?xml version="1.0" encoding="UTF-8"?>\n'+ET.tostring(root, encoding="unicode")


def handle(request):
    if request.get("protocol") != 1:
        raise ValueError("Unsupported protocol version")
    operation = request["operation"]
    response = {"engine_version": rdBase.rdkitVersion, "warnings": []}
    if operation == "import":
        fmt, text = request.get("format", "smiles"), request.get("text", "")
        if not text.strip(): raise ValueError("Enter a structure first")
        base = None
        if fmt == "smiles": mol = Chem.MolFromSmiles(text)
        elif fmt == "mol": mol = Chem.MolFromMolBlock(text, removeHs=False)
        elif fmt == "inchi": mol = Chem.MolFromInchi(text, removeHs=False)
        elif fmt == "cdxml":
            mol, base = import_cdxml(text)
        else: raise ValueError("Unsupported import format")
        if mol is None: raise ValueError("Could not parse this structure")
        check_supported(mol)
        if fmt in ("smiles", "inchi") or not mol.GetNumConformers(): rdDepictor.Compute2DCoords(mol)
        response.update(document=to_document(mol, base), analysis=analyze(mol))
    elif operation in ("analyze", "clean", "export"):
        doc = request["document"]
        if not doc["atoms"]: raise ValueError("Draw or import a molecule first")
        mol = from_document(doc)
        if operation == "clean":
            rdDepictor.Compute2DCoords(mol, canonOrient=False)
            # Keep the molecular drawing centered where the user placed it.
            old_x = sum(a["position"]["x"] for a in doc["atoms"])/len(doc["atoms"])
            old_y = sum(a["position"]["y"] for a in doc["atoms"])/len(doc["atoms"])
            conf = mol.GetConformer()
            new_x = sum(conf.GetAtomPosition(i).x for i in range(mol.GetNumAtoms()))/mol.GetNumAtoms()
            new_y = sum(conf.GetAtomPosition(i).y for i in range(mol.GetNumAtoms()))/mol.GetNumAtoms()
            for i in range(mol.GetNumAtoms()):
                p = conf.GetAtomPosition(i)
                conf.SetAtomPosition(i, (p.x-new_x+old_x/SCALE, p.y-new_y-old_y/SCALE, 0))
        result_doc = to_document(mol, doc)
        response.update(document=result_doc, analysis=analyze(mol))
        if operation == "export":
            fmt = request["format"]
            if fmt == "mol": response["output"] = Chem.MolToMolBlock(mol)
            elif fmt == "smiles": response["output"] = Chem.MolToSmiles(mol)
            elif fmt == "inchi": response["output"] = Chem.MolToInchi(mol)
            elif fmt == "cdxml": response["output"] = export_cdxml(result_doc)
            else: raise ValueError("Unsupported export format")
    else: raise ValueError("Unknown chemistry operation")
    return response


def main():
    for line in sys.stdin:
        request = {}
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                request = {}
                raise ValueError("Request must be a JSON object")
            response = {"id": request.get("id"), "ok": True, "result": handle(request)}
        except Exception as error:
            response = {"id": request.get("id"), "ok": False, "error": str(error)}
        print(json.dumps(response, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
