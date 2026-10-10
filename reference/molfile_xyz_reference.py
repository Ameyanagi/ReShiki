"""Check an actual ReShiki executable against independently parsed MOL fixtures.

Uses only the executable's public headless MCP file operations. Outputs and
receipts are retained; no UI interaction, production Python worker, or Rust
parser is used as the chemical reference.
"""

import argparse
import json
import math
import select
import struct
import subprocess
import time
from pathlib import Path

from rdkit import Chem, rdBase
from rdkit.Chem import rdMolDescriptors, rdMolTransforms


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def unknown_single(bond):
    # RDKit consumes the file direction during 3D assignment and retains the
    # explicit single-bond unknown in its parser property, rather than BondDir.
    return bond.GetBondType() == Chem.BondType.SINGLE and (
        bond.GetBondDir() == Chem.BondDir.UNKNOWN
        or bond.HasProp("_UnknownStereo")
        and bond.GetIntProp("_UnknownStereo") != 0
    )


def identity(molecule):
    molecule = Chem.RemoveHs(molecule)
    Chem.AssignStereochemistry(molecule, cleanIt=True, force=True)
    return dict(
        smiles=Chem.MolToSmiles(molecule, canonical=True, isomericSmiles=True),
        formula=rdMolDescriptors.CalcMolFormula(molecule),
        charge=sum(atom.GetFormalCharge() for atom in molecule.GetAtoms()),
        radicals=sum(atom.GetNumRadicalElectrons() for atom in molecule.GetAtoms()),
    )


def native_content(drawing):
    # Native file_open clears cached CIP paint labels for later recomputation;
    # atom tags, bond controls and all XYZ remain persistent chemistry/data.
    value = json.loads(json.dumps(drawing))
    for item in value["atoms"] + value["bonds"]:
        item.pop("cip_label", None)
    return value


class Session:
    def __init__(self, executable, fixtures, outputs):
        self.stderr = (outputs / "mcp-stderr.log").open("w")
        self.process = subprocess.Popen(
            [
                str(executable),
                "--mcp",
                "--allow-read",
                str(fixtures),
                "--allow-read",
                str(outputs),
                "--allow-write",
                str(outputs),
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr,
            text=True,
            bufsize=1,
        )
        self.serial = 0
        self.receipts = []
        result = self.call(
            "initialize",
            dict(
                protocolVersion="2025-06-18",
                capabilities={},
                clientInfo=dict(name="mol-xyz-reference", version="1"),
            ),
        )
        assert "result" in result, result
        self.send(dict(jsonrpc="2.0", method="notifications/initialized"))

    def send(self, value):
        self.process.stdin.write(json.dumps(value) + "\n")
        self.process.stdin.flush()

    def call(self, method, params):
        self.serial += 1
        self.send(dict(jsonrpc="2.0", id=self.serial, method=method, params=params))
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            remaining = max(0, deadline - time.monotonic())
            if not select.select([self.process.stdout], [], [], remaining)[0]:
                raise TimeoutError(method)
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("MCP executable exited")
            value = json.loads(line)
            if value.get("id") == self.serial:
                return value
        raise TimeoutError(method)

    def tool(self, name, arguments):
        result = self.call("tools/call", dict(name=name, arguments=arguments))
        self.receipts.append(dict(name=name, arguments=arguments, result=result))
        assert "error" not in result, result
        content = result["result"]
        if "structuredContent" in content:
            value = content["structuredContent"]
        else:
            value = json.loads(next(c["text"] for c in content["content"] if c["type"] == "text"))
        assert "error" not in value, value
        return value.get("value", value)

    def open(self, path, format):
        return self.tool("file_open", dict(path=str(path), format=format))["document"]

    def save(self, document, path, format):
        return self.tool(
            "file_save",
            dict(
                document=document,
                path=str(path),
                format=format,
                overwrite=False,
                pages=None,
            ),
        )

    def close(self, document):
        self.tool("document_close", dict(document=document))

    def finish(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.stderr.close()


def check(fixture, source, drawing, interchange, baseline):
    source_atoms = list(source.GetAtoms())
    expected_count = source.GetNumAtoms()
    if not baseline:
        if fixture.stem.startswith("alkane-3d"):
            expected_count = 6
        elif fixture.stem.startswith(
            (
                "chiral-3d",
                "chiral-s-3d",
                "chiral-h-",
                "alkene-3d",
                "alkene-z-3d",
                "alkene-unknown-3d",
                "mixed-isotope-r",
            )
        ):
            expected_count = 4
    assert len(drawing["atoms"]) == expected_count, (fixture.name, expected_count)
    kept = {atom["id"] - 1 for atom in drawing["atoms"]}
    conformer = source.GetConformer()
    reference_xyz, native_xyz = [], []
    for atom in drawing["atoms"]:
        index = atom["id"] - 1
        original = source_atoms[index]
        assert atom["element"] == original.GetSymbol(), fixture.name
        assert atom["isotope"] == original.GetIsotope(), fixture.name
        assert atom["charge"] == original.GetFormalCharge(), fixture.name
        assert atom["radical_electrons"] == original.GetNumRadicalElectrons(), fixture.name
        if original.HasProp("_CIPCode"):
            assert atom.get("cip_label") == original.GetProp("_CIPCode"), fixture.name
        point = conformer.GetAtomPosition(index)
        expected = [f32(point.x * 28), f32(-point.y * 28), f32(point.z * 28)]
        actual = [atom["position"]["x"], atom["position"]["y"], atom.get("depth", 0)]
        if baseline:
            expected[2] = 0
        assert all(abs(a - b) <= 1e-5 for a, b in zip(actual, expected)), (
            fixture.name,
            index,
            actual,
            expected,
        )
        if original.GetAtomicNum() != 1:
            reference_xyz.append([point.x, point.y, point.z])
            native_xyz.append([actual[0] / 28, -actual[1] / 28, actual[2] / 28])
    bond_orders = {"SINGLE": 1, "DOUBLE": 2, "TRIPLE": 3, "AROMATIC": 4, "DATIVE": 5}

    def edge(a, b, order):
        return (a, b, order) if order == 5 else (min(a, b), max(a, b), order)

    expected_bonds = {
        edge(b.GetBeginAtomIdx() + 1, b.GetEndAtomIdx() + 1, bond_orders[str(b.GetBondType())])
        for b in source.GetBonds()
        if b.GetBeginAtomIdx() in kept and b.GetEndAtomIdx() in kept
    }
    actual_bonds = {edge(b["a"], b["b"], b["order"]) for b in drawing["bonds"]}
    assert expected_bonds == actual_bonds, fixture.name
    unknown = sum(b.GetStereo() == Chem.BondStereo.STEREOANY for b in source.GetBonds())
    if not (baseline and unknown):
        assert identity(source) == identity(interchange), (
            fixture.name,
            identity(source),
            identity(interchange),
        )
        assert unknown == sum(
            b.GetStereo() == Chem.BondStereo.STEREOANY for b in interchange.GetBonds()
        ), fixture.name
    if not baseline:
        assert unknown == sum(b.get("stereo") == "any" for b in drawing["bonds"]), fixture.name
        unknown_singles = sum(unknown_single(b) for b in source.GetBonds())
        assert unknown_singles == sum(
            b["order"] == 1 and b["display"] == "wavy" and not b.get("projection", False)
            for b in drawing["bonds"]
        ), fixture.name
    maximum_distance_error, torsion_error = 0, None
    if not baseline:
        for i, point in enumerate(reference_xyz):
            for j in range(i):
                maximum_distance_error = max(
                    maximum_distance_error,
                    abs(
                        math.dist(point, reference_xyz[j]) - math.dist(native_xyz[i], native_xyz[j])
                    ),
                )
        assert maximum_distance_error <= 2e-6, (fixture.name, maximum_distance_error)
        if len(reference_xyz) >= 4:
            conformers = []
            for points in (reference_xyz, native_xyz):
                conformer = Chem.Conformer(4)
                for index, point in enumerate(points[:4]):
                    conformer.SetAtomPosition(index, point)
                conformers.append(conformer)
            angles = [rdMolTransforms.GetDihedralRad(c, 0, 1, 2, 3) for c in conformers]
            if all(math.isfinite(a) for a in angles):
                torsion_error = abs(math.remainder(angles[0] - angles[1], 2 * math.pi))
                assert torsion_error <= 2e-6, (fixture.name, torsion_error)
    return dict(
        fixture=fixture.name,
        source_atoms=source.GetNumAtoms(),
        drawing_atoms=len(drawing["atoms"]),
        visible_hydrogens=sum(a["element"] == "H" for a in drawing["atoms"]),
        nonzero_z=sum(a.get("depth", 0) != 0 for a in drawing["atoms"]),
        maximum_heavy_distance_error=maximum_distance_error,
        heavy_torsion_error=torsion_error,
        identity=identity(source),
        mol_interchange_identity=identity(interchange),
        source_unknown_doubles=unknown,
        native_unknown_doubles=sum(b.get("stereo") == "any" for b in drawing["bonds"]),
        mol_interchange_unknown_doubles=sum(
            b.GetStereo() == Chem.BondStereo.STEREOANY for b in interchange.GetBonds()
        ),
        source_unknown_singles=sum(unknown_single(b) for b in source.GetBonds()),
        native_unknown_singles=sum(
            b["order"] == 1 and b["display"] == "wavy" for b in drawing["bonds"]
        ),
        mol_interchange_unknown_singles=sum(unknown_single(b) for b in interchange.GetBonds()),
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument(
        "--fixtures",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "tests/fixtures/mol-import-3d",
    )
    parser.add_argument(
        "--baseline",
        action="store_true",
        help="Assert the frozen baseline's flattened, explicit-H behavior",
    )
    args = parser.parse_args()
    fixtures, outputs = args.fixtures.resolve(), args.output_dir.resolve()
    outputs.mkdir(parents=True, exist_ok=False)
    session = Session(args.executable.resolve(), fixtures, outputs)
    records = []
    try:
        for fixture in sorted(fixtures.glob("*.mol")):
            source = Chem.MolFromMolFile(str(fixture), removeHs=False, strictParsing=True)
            assert source is not None, fixture.name
            native_path, mol_path = (
                outputs / (fixture.stem + ".rsk"),
                outputs / (fixture.stem + ".mol"),
            )
            handle = session.open(fixture, "mol")
            session.save(handle, native_path, "reshiki")
            session.save(handle, mol_path, "mol")
            session.close(handle)
            drawing = json.loads(native_path.read_text())
            handle = session.open(native_path, "reshiki")
            roundtrip = outputs / (fixture.stem + "-roundtrip.rsk")
            session.save(handle, roundtrip, "reshiki")
            session.close(handle)
            assert native_content(json.loads(roundtrip.read_text())) == native_content(drawing), (
                fixture.name
            )
            interchange = Chem.MolFromMolFile(str(mol_path), removeHs=False, strictParsing=True)
            assert interchange is not None, fixture.name
            record = check(fixture, source, drawing, interchange, args.baseline)
            records.append(record)
            print(json.dumps(record), flush=True)
    finally:
        session.finish()
        (outputs / "receipts.json").write_text(json.dumps(session.receipts, indent=2) + "\n")
        (outputs / "results.json").write_text(
            json.dumps(
                dict(
                    rdkit_version=rdBase.rdkitVersion,
                    baseline=args.baseline,
                    records=records,
                ),
                indent=2,
            )
            + "\n"
        )
    print(
        json.dumps(
            dict(
                passed_contract_controls=len(records),
                rdkit_version=rdBase.rdkitVersion,
                recorded_baseline_identity_regressions=sum(
                    r["identity"] != r["mol_interchange_identity"] for r in records
                ),
            )
        )
    )


if __name__ == "__main__":
    main()
