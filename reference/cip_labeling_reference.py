"""Differential oracle for the public pinned rdCIPLabeler.AssignCIPLabels API.

Native calls run in a restartable subprocess: a native crash is reported
separately from exceptions and is never silently accepted as a normal result.
The optional 300-compound validation corpus is consumed without redistribution.
Its RDKit source file is SHA256 pinned; upstream provenance is ValidationSuite
28d0fe05073905e74a1ba5e06b3bd6298686f6af, Hanson et al., JCIM 2018,
https://doi.org/10.1021/acs.jcim.8b00324. Native regression examples below are
from RDKit CIPLabeler/catch_tests.cpp, Copyright (C) 2020-2025 Schrödinger, LLC
and other RDKit contributors, BSD-3-Clause (licenses/rdkit/).
"""

import argparse
import contextlib
import hashlib
import json
import os
import random
import signal
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdCIPLabeler

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
else:
    from perception_reference import snapshot

CORPUS_SHA256 = "df178635c00b6c41fad820d2609fc4ff18403c63dec4e5c3e1756a6db5858059"
RDKIT_COMMIT = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"


def ring_kind(mol, previous):
    if previous != "none":
        return previous
    try:
        mol.GetRingInfo().NumRings()
    except RuntimeError:
        return "none"
    return "fast"


def selected(mol, options):
    atoms, bonds = options["atoms"], options["bonds"]
    # The public wrapper tests Python truthiness: two empty selections
    # (including None or empty lists) select the entire molecule.
    if not atoms and not bonds:
        atoms, bonds = range(mol.GetNumAtoms()), range(mol.GetNumBonds())
    return [
        ("atom", index)
        for index in sorted(set(atoms or []))
        if int(mol.GetAtomWithIdx(index).GetChiralTag()) in (1, 2)
    ] + [
        ("bond", index)
        for index in sorted(set(bonds or []))
        if int(mol.GetBondWithIdx(index).GetStereo()) in (2, 3, 4, 5, 6, 7)
    ]


def native_result(request):
    mol = Chem.Mol(bytes.fromhex(request["pickle"]))
    options = request["options"]
    kind = request["rings"]
    passes = []
    try:
        for _ in range(request["passes"]):
            rdCIPLabeler.AssignCIPLabels(
                mol, options["atoms"], options["bonds"], options["max_iterations"]
            )
            targets = selected(mol, options)
            kind = ring_kind(mol, kind)
            labels = []
            for target, index in targets:
                obj = mol.GetAtomWithIdx(index) if target == "atom" else mol.GetBondWithIdx(index)
                if not obj.HasProp("_CIPCode"):
                    continue
                order = list(obj.GetPropsAsDict(True, True)["_CIPNeighborOrder"])
                labels.append(
                    dict(
                        target=[target, index],
                        code=obj.GetProp("_CIPCode"),
                        neighbor_order=[None if i == 0xFFFFFFFF else i for i in order],
                        bond_stereo=[int(obj.GetStereo()), list(obj.GetStereoAtoms())]
                        if target == "bond" and int(obj.GetStereo()) in (4, 5)
                        else None,
                    )
                )
            assert mol.HasProp("_CIPComputed") and mol.GetBoolProp("_CIPComputed")
            passes.append(dict(state=snapshot(mol, kind), labels=labels))
    except (ValueError, RuntimeError, OverflowError, IndexError) as error:
        message = str(error)
        if "Max Iterations Exceeded" in message:
            category = "iterations"
        elif "Digraph generation failed: more than" in message:
            category = "nodes"
        else:
            category = "invalid"
        return dict(error=category, native_message=message)
    return dict(passes=passes)


def suppress_worker_crash_dialogs():
    # Per-process settings, following CPython test.support's crash helpers.
    # The controlling test process and machine-wide error reporting stay intact.
    if os.name == "nt":
        import msvcrt

        msvcrt.SetErrorMode(msvcrt.GetErrorMode() | msvcrt.SEM_NOGPFAULTERRORBOX)
        if hasattr(msvcrt, "CrtSetReportMode"):
            for report in (msvcrt.CRT_WARN, msvcrt.CRT_ERROR, msvcrt.CRT_ASSERT):
                msvcrt.CrtSetReportMode(report, msvcrt.CRTDBG_MODE_FILE)
                msvcrt.CrtSetReportFile(report, msvcrt.CRTDBG_FILE_STDERR)


def fatal_status(status):
    if os.name == "nt":
        # CRT abort, illegal instruction, access violation, integer divide by
        # zero, stack overflow, corrupt heap, fast-fail, fatal application exit.
        return status & 0xFFFFFFFF in {
            3,
            0xC000001D,
            0xC0000005,
            0xC0000094,
            0xC00000FD,
            0xC0000374,
            0xC0000409,
            0x40000015,
        }
    signals = ("SIGSEGV", "SIGABRT", "SIGBUS", "SIGILL", "SIGFPE", "SIGTRAP")
    return status in {-int(getattr(signal, name)) for name in signals if hasattr(signal, name)}


def worker():
    suppress_worker_crash_dialogs()
    for line in sys.stdin:
        result = native_result(json.loads(line))
        print(json.dumps(result), flush=True)


class Oracle:
    def __init__(self, stack):
        self.stack = stack
        self.child = self.start()

    def start(self):
        child = self.stack.enter_context(
            subprocess.Popen(
                [sys.executable, str(Path(__file__).resolve()), "--worker"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
        )
        self.stack.callback(child.kill)
        return child

    def evaluate(self, mol, options, rings, passes):
        assert self.child.stdin and self.child.stdout
        self.child.stdin.write(
            json.dumps(
                dict(
                    pickle=mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
                    options=options,
                    rings=rings,
                    passes=passes,
                )
            )
            + "\n"
        )
        self.child.stdin.flush()
        line = self.child.stdout.readline()
        if line:
            return json.loads(line)
        status = self.child.wait()
        if not fatal_status(status):
            diagnostic = self.child.stderr.read() if self.child.stderr else ""
            raise RuntimeError(f"Oracle failed with status {status}: {diagnostic}")
        self.child = self.start()
        return dict(error="native_crash", native_status=status)

    def finish(self):
        assert self.child.stdin
        self.child.stdin.close()
        assert self.child.wait() == 0


def curated():
    texts = [
        "",
        "CC",
        "N[C@@H](C)C(=O)O",
        "F[C@](Cl)(Br)I",
        "[1H][C@H](F)Cl",
        "C[S@H]=O",
        "C[S@](=O)CC",
        "C[P@](F)Cl",
        "C[As@H](F)",
        "C[C@H]1CCCCO1",
        "F/C=C/F",
        "Cl/C(F)=C(/Br)I",
        "[1H]/C=C/F",
        "C/C=C/C=C/C",
        "C/C=C\\C",
        "F/C=C(/[C@H](F)Cl)[C@@H](F)Cl",
        "F/C=C(\\[C@H](F)Cl)[C@@H](F)Cl",
        "[2H]/C(=C(/[1H])\\[H])/[H]",
        "C\\C=C/[C@@H](\\C=C\\O)[C@H](C)[C@H](\\C=C/C)\\C=C\\O",
        "C\\C=C/[C@@H](\\C=C\\C)[C@H](C)[C@H](\\C=C/C)\\C=C\\C",
        "CC[C@H](C)CCCCC[C@H]1CC[C@@H](C)CC1",
        "OC(=O)[C@H]1CC[C@@H](CC1)O[C@@H](F)Cl",
        "OC(=O)[C@H]1CC[C@@H](CC1)O[CH](Cl)Cl",
        "*[C@](F)(Cl)Br",
        "*[C@](*)(Cl)Br",
        "C1CC[C@](*)2CCCC[C@H]2C1",
        "*C1C[C@H](CCC)[C@@H](C)[C@H](C)C1",
        "C1CC[C@H]2C/C(=C3\\C[C@H]4CCCC[C@H]4C3)C[C@H]2C1",
        "C1/C(=C2\\C[C@H]3CCCC[C@H]3C2)C[C@H]2CCCC[C@@H]12",
        "C[C@H](c1ncccc1)O",
        "[13CH3][C@H]([14CH3])[18OH]",
        "[999CH3][C@H]([3H])[13*]",
        "N->[Cu]<-N.C[C@H](F)Cl",
        "C1[C@]2(CCCC1)CC=CC2",
        "C[C@](C)(C)C",
        "C[C@](CC)(CC)C",
        "[C@]12(CCCCC1)CCCCC2",
        "[C@]",
        "[C@H3]F",
        "[P@H](F)Cl",
        "F[C@]([C@H](F)Cl)([C@@H](F)Cl)Br",
    ]
    rng = random.Random(832014)
    for index, text in enumerate(texts):
        for raw in (False, True):
            params = Chem.SmilesParserParams()
            params.removeHs = not raw
            params.sanitize = not raw
            mol = Chem.MolFromSmiles(text, params)
            assert mol is not None, text
            if raw:
                # Input construction follows RDKit's own validation reader;
                # no sanitization or cleanup is performed after native labeling.
                Chem.SanitizeMol(mol)
                Chem.SetBondStereoFromDirections(mol)
            yield f"curated/{index}/{raw}", mol
            if mol.GetNumAtoms() and index % 3 == 0:
                order = list(range(mol.GetNumAtoms()))
                rng.shuffle(order)
                yield f"permuted/{index}/{raw}", Chem.RenumberAtoms(mol, order)
    for index, text in enumerate(
        (
            "Cc1cccc(Br)c1-c1c(Cl)cccc1F",
            "C1(N2C(C)=CC=C2Br)=C(C)C(C)=C(N2C(C)=CC=C2Br)C(C)=C1C",
            "N1(n2c(C)ccc2Br)C(=O)[C@H](C)[C@H](C)C1=O",
            "C1CCC2(CC1)CCCCC2",
        )
    ):
        for winding in (6, 7):
            mol = Chem.MolFromSmiles(text)
            assert mol is not None
            for bond in mol.GetBonds():
                if (
                    bond.GetBondType() == Chem.BondType.SINGLE
                    and bond.GetBeginAtom().IsInRing()
                    and bond.GetEndAtom().IsInRing()
                    and (not bond.IsInRing() or index == 3)
                ):
                    bond.SetStereo(Chem.BondStereo.values[winding])
            yield f"atrop/{index}/{winding}", mol

    dense = Chem.RWMol()
    for number in (6, 9, 17, *([6] * 18)):
        atom = Chem.Atom(number)
        atom.SetNoImplicit(True)
        dense.AddAtom(atom)
    for neighbor in (1, 2, 3, 12):
        dense.AddBond(0, neighbor, Chem.BondType.SINGLE)
    for start in (3, 12):
        for a in range(start, start + 9):
            for b in range(a + 1, start + 9):
                dense.AddBond(a, b, Chem.BondType.SINGLE)
    dense.GetAtomWithIdx(0).SetChiralTag(Chem.ChiralType.CHI_TETRAHEDRAL_CW)
    dense.UpdatePropertyCache(strict=False)
    yield "dense/symmetric-cages", dense


def corpus_path(requested):
    root = Path(__file__).resolve().parents[1]
    choices = (
        [requested]
        if requested
        else [
            os.environ.get("RESHIKI_CIP_VALIDATION_SUITE"),
            root / "artifacts/cip-validation-compounds.smi",
            root.parent / "rdkit/Code/GraphMol/test_data/compounds.smi",
        ]
    )
    for candidate in choices:
        if candidate and Path(candidate).is_file():
            path = Path(candidate)
            assert hashlib.sha256(path.read_bytes()).hexdigest() == CORPUS_SHA256, path
            return path
    if (
        requested
        or os.environ.get("RESHIKI_CIP_VALIDATION_SUITE")
        or os.environ.get("RESHIKI_REQUIRE_CIP_VALIDATION_SUITE")
    ):
        raise RuntimeError("Pinned compounds.smi validation corpus is required")
    return None


def inputs(corpus):
    yield from curated()
    if corpus:
        lines = corpus.read_text().splitlines()
        assert len(lines) == 300
        for line in lines:
            text, name, *_ = line.split("\t")
            mol = Chem.MolFromSmiles(text, sanitize=False)
            assert mol is not None, name
            Chem.SanitizeMol(mol)
            Chem.SetBondStereoFromDirections(mol)
            yield f"validation/{name}", mol


def options_for(mol, variant):
    n, e = mol.GetNumAtoms(), mol.GetNumBonds()
    choices = [
        (None, None),
        ([], []),
        (None, []),
        ([], None),
        (list(reversed(range(n))) + list(range(n)), None),
        (None, list(reversed(range(e))) + list(range(e))),
        (list(range(0, n, 2)), list(range(1, e, 2))),
        ([n], []),
        ([], [e]),
    ]
    atoms, bonds = choices[variant] if variant < len(choices) else (None, None)
    budget = {9: 1, 10: 2, 11: 31, 12: 0}.get(variant, 1_250_000)
    return dict(atoms=atoms, bonds=bonds, max_iterations=budget)


def cases(corpus):
    for index, (name, original) in enumerate(inputs(corpus)):
        variants = (
            (0, 6, 9, 12)
            if name.startswith("validation/")
            else (0, 9)
            if name.startswith("dense/")
            else range(14)
        )
        for variant in variants:
            mol = Chem.Mol(original)
            rings = ("symmetric", "none", "fast", "basis")[(index + variant) % 4]
            if rings == "none":
                mol.ClearComputedProps(includeRings=True)
            elif rings == "fast":
                Chem.FastFindRings(mol)
            elif rings == "basis":
                Chem.GetSSSR(mol)
            else:
                Chem.GetSymmSSSR(mol)
            mol.UpdatePropertyCache(strict=False)
            if variant in (4, 5, 6, 13):
                for atom in mol.GetAtoms():
                    atom.SetProp("_CIPCode", ("legacy", "", "r")[atom.GetIdx() % 3])
                for bond in mol.GetBonds():
                    bond.SetProp("_CIPCode", "legacy")
                mol.SetBoolProp("_CIPComputed", False)
            options = options_for(mol, variant)
            if name.startswith("dense/") and variant == 0:
                options["max_iterations"] = 3000
            yield (
                mol,
                dict(
                    name=f"{name}/{variant}",
                    before=snapshot(mol, rings),
                    options=options,
                    passes=2 if variant in (0, 12, 13) else 1,
                ),
            )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker", action="store_true")
    parser.add_argument("--validation-suite", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    RDLogger.DisableLog("rdApp.*")
    assert rdBase.rdkitVersion == "2026.03.6"
    Chem.SetUseLegacyStereoPerception(True)
    if args.worker:
        worker()
        return
    corpus = corpus_path(args.validation_suite)
    with contextlib.ExitStack() as stack:
        output = stack.enter_context(args.output.open("w")) if args.output else sys.stdout
        print(
            json.dumps(
                dict(
                    rdkit_version=rdBase.rdkitVersion,
                    rdkit_commit=RDKIT_COMMIT,
                    validation_compounds=300 if corpus else 0,
                    validation_sha256=CORPUS_SHA256 if corpus else None,
                )
            ),
            file=output,
        )
        oracle = Oracle(stack)
        for mol, case in cases(corpus):
            expected = oracle.evaluate(
                mol, case["options"], case["before"]["rings"]["kind"], case["passes"]
            )
            diagnostic = {
                key: expected.pop(key)
                for key in ("native_message", "native_status")
                if key in expected
            }
            print(json.dumps({**case, "expected": expected, **diagnostic}), file=output, flush=True)
        oracle.finish()


if __name__ == "__main__":
    main()
