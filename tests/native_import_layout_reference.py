"""Original full import calls, including layouts and raw CX rank properties."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_import_reference as reference
from rdkit import Chem, RDLogger, rdBase
from smiles_read_reference import cases


def main():
    import json

    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    seen = {}
    # Select by input ordinal/category only, before calling the original worker.
    # Every selected native acceptance and rejection remains in the comparison.
    for index, (name, text) in enumerate(cases()):
        category = name.split("/")[0]
        seen[category] = seen.get(category, 0) + 1
        if seen[category] <= 3 or index % 97 == 0:
            reference.emit("smiles/" + name, text, format="smiles")
    for smiles in ("C", "CC", "CCC", "CC(C)C", "C1CCCCC1", "CC1CCCCC1", "F/C=C/F"):
        for atom in range(min(3, Chem.MolFromSmiles(smiles).GetNumAtoms())):
            for prop in ("_CIPRank", "_chiralAtomRank"):
                for value in ("bad", "0", "-1", "4294967296", "+1", "1&#32;", "&#32;1"):
                    text = f"{smiles} |atomProp:{atom}.{prop}.{value}|"
                    reference.emit("rank/" + text, text, format="smiles")
    for text in (
        "[H]CC |atomProp:0._CIPRank.bad:2._chiralAtomRank.bad|",
        "[H]CC |atomProp:0._CIPRank.bad:1._chiralAtomRank.bad|",
        "[2H]CC |atomProp:1._chiralAtomRank.bad|",
        "CC |atomProp:1._CIPRank.1:1._chiralAtomRank.bad|",
        "CC |atomProp:1._CIPRank.bad:1._chiralAtomRank.1|",
        "CCO |(123,456,789;0,0,0;999,100,200)|",
        "F[C@](Cl)(Br)I |(0,0,1;1,0,0;1,1,0;1,0,1;1,0,-1)|",
        "[H]CO |(7,7,;8,8,;9,9,)|",
        "C1CCCCCCCCCC1",
        "C1CCCCCCCCCCC1",
        "C1CC2CCC1C2",
        "",
        " ",
        "invalid",
        "C~C",
        "[CH5]",
        "C |(nan,inf,-inf)|",
    ):
        reference.emit("target/" + text, text, format="smiles")
    molecular = (
        "C",
        "CCO",
        "[H]O[H]",
        "[2H]O[3H]",
        "[13CH3][C@@H](O)Cl",
        "F/C=C/F",
        "F/C=C\\F",
        "c1ccccc1",
        "C1CC2CCC1C2",
        "[Na+].[Cl-]",
        "CC(=O)O",
        "C[NH3+]",
        "[CH3]",
        "O=N(=O)c1ccccc1",
        "C1CCCCCCCCCCC1",
    )
    for text in molecular:
        identifier = Chem.MolToInchi(Chem.MolFromSmiles(text))
        reference.emit("inchi/" + text, identifier, format="inchi")
        reference.emit("inchi/nonstandard/" + text, identifier.replace("1S/", "1/"), format="inchi")
    for text in (
        "InChI=1S/",
        "InChI=1/",
        "InChI=1S/CH4/h1H4\0ignored",
        "",
        "invalid",
        "InChI=1S/invalid",
    ):
        reference.emit("inchi/edge/" + repr(text), text, format="inchi")
    for text in (
        "C>>O",
        "[CH3:1][OH:2]>>[CH2:1]=[O:2]",
        "CC.O>N>CCO",
        "F/C=C/F>>F/C=C\\F",
        "[H]CO>>CO",
        "[13CH3]O>>[13CH2]=O",
        "CC>>CC |atomProp:1._CIPRank.bad|",
        "CC>>CC |atomProp:0._CIPRank.bad|",
        "CC>>CC |atomProp:1._chiralAtomRank.-1|",
        "F[C@](Cl)(Br)I>>F[C@@](Cl)(Br)I |atomProp:1._CIPRank.bad:1._chiralAtomRank.bad|",
        "F/C=C/F>>F/C=C\\F |atomProp:1._CIPRank.bad:1._chiralAtomRank.bad|",
        "CC1CCCCC1>>CC1CCCCC1 |atomProp:1._CIPRank.bad|",
        "CC>>O |atomProp:1._CIPRank.1:1._chiralAtomRank.bad|",
        "CC>>O |atomProp:1._CIPRank.bad:1._chiralAtomRank.1|",
        "C1CCCCC1>>CC1CCCCC1",
        "CC>>O |(20,30,;40,50,;60,70,)|",
        "C>>O |()|",
        "",
        " ",
        "invalid",
        ">>O",
        "C>>",
        "[CH5]>>O",
    ):
        reference.emit("rsmi/" + text, text, format="rsmi")
    for xml in (
        "<CDXML><page/></CDXML>",
        "<CDXML><page><fragment id='1'><n id='2'/><n id='3' Element='8'/><b id='4' B='2' E='3'/></fragment></page></CDXML>",
        "<CDXML><page><fragment id='1'><n id='2'/></fragment><t id='3' p='20 40'><s>caption</s></t></page></CDXML>",
    ):
        reference.emit("cdxml/coordinates/" + xml, xml)


if __name__ == "__main__":
    main()
