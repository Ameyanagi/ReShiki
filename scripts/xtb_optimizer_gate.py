"""Compare an optimizer with xTB GFN2 on one molecule per XYZ file.

Example (quote paths and the path placeholders inside --ours):
    python scripts/xtb_optimizer_gate.py --xyz-dir "inputs" --xtb "/path/to/xtb" \
        --ours '"/path/to/reshiki" --cli calc --task opt --method gfn2 "{xyz}" \
                --json "{json}"' --level normal --out "reports/optimizer"

--out is a report stem, or a .md/.csv path, and writes both formats. The ours
template must contain {xyz} and {json}. It can also use {charge}, {uhf}, {level}
and {threads} to request matching settings. Relative executable paths resolve
from the invocation directory. Other relative paths resolve in the temporary
folder used by each program, so quote absolute paths for additional input files.
The JSON file must contain nonnegative integer steps and finite energy_hartree.
Optional positions_angstrom must follow the input atom order. RMSD aligns heavy
atoms by a proper rotation and translation, without remapping equivalent atoms.
Only the step ratio and energy difference gate the comparison. Run failures,
missing xTB convergence, and malformed results also fail the gate.
"""

import argparse
import csv
import json
import math
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STEP_RATIO_LIMIT = 1.5
ENERGY_LIMIT_HARTREE = 1e-6
CONVERGENCE = re.compile(r"GEOMETRY\s+OPTIMIZATION\s+CONVERGED\s+AFTER\s+(\d+)\s+ITERATIONS?", re.I)
ENERGY = re.compile(r"\bTOTAL\s+ENERGY\s+(\S+)\s+Eh\b", re.I)


@dataclass
class Optimization:
    steps: int | None = None
    energy_hartree: float | None = None
    seconds: float | None = None
    positions_angstrom: list[tuple[float, float, float]] | None = None
    problems: list[str] = field(default_factory=list)


@dataclass
class Comparison:
    molecule: str
    xtb: Optimization
    ours: Optimization
    step_ratio: float | None
    energy_delta_hartree: float | None
    heavy_rmsd_angstrom: float | None
    problems: list[str]

    @property
    def passed(self) -> bool:
        return not self.problems


def finite_number(value) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError("expected a finite number")
    value = float(value)
    if not math.isfinite(value):
        raise ValueError("expected a finite number")
    return value


def read_xyz(path: Path) -> tuple[list[str], list[tuple[float, float, float]]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if len(lines) < 2:
        raise ValueError("XYZ needs an atom count and comment line")
    count = int(lines[0].strip())
    if count <= 0 or len(lines) < count + 2 or any(line.strip() for line in lines[count + 2 :]):
        raise ValueError("XYZ must contain exactly one nonempty molecule")
    symbols, positions = [], []
    for line in lines[2 : count + 2]:
        fields = line.split()
        if len(fields) != 4 or not re.fullmatch(r"[A-Za-z]{1,3}", fields[0]):
            raise ValueError("XYZ atom lines need a symbol and three coordinates")
        symbols.append(fields[0].capitalize())
        positions.append(
            tuple(
                finite_number(float(value.replace("D", "E").replace("d", "e")))
                for value in fields[1:]
            )
        )
    return symbols, positions


def parse_xtb_output(output: str) -> tuple[int | None, float | None]:
    """Use the last convergence marker and the final total energy, including D exponents."""
    steps = CONVERGENCE.findall(output)
    energies = ENERGY.findall(output)
    energy = (
        finite_number(float(energies[-1].replace("D", "E").replace("d", "e"))) if energies else None
    )
    return int(steps[-1]) if steps else None, energy


def parse_ours(path: Path, atom_count: int) -> Optimization:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("JSON result must be an object")
    steps = data["steps"]
    if isinstance(steps, bool) or not isinstance(steps, int) or steps < 0:
        raise ValueError("JSON steps must be a nonnegative integer")
    result = Optimization(steps=steps, energy_hartree=finite_number(data["energy_hartree"]))
    if data.get("converged") is False:
        result.problems.append("optimizer reports not converged")
    if "positions_angstrom" in data:
        positions = data["positions_angstrom"]
        if not isinstance(positions, list) or len(positions) != atom_count:
            raise ValueError("positions_angstrom must contain one position per input atom")
        if any(not isinstance(row, list) or len(row) != 3 for row in positions):
            raise ValueError("positions_angstrom must contain three coordinates per atom")
        result.positions_angstrom = [
            tuple(finite_number(value) for value in row) for row in positions
        ]
    return result


def largest_eigenvalue(matrix: list[list[float]]) -> float:
    """Diagonalize the symmetric 4 by 4 quaternion matrix with Jacobi rotations."""
    matrix = [row[:] for row in matrix]
    scale = max(abs(value) for row in matrix for value in row)
    for _ in range(80):
        p, q = max(
            ((p, q) for p in range(4) for q in range(p + 1, 4)),
            key=lambda pair: abs(matrix[pair[0]][pair[1]]),
        )
        if abs(matrix[p][q]) <= 1e-15 * scale:
            break
        angle = 0.5 * math.atan2(2 * matrix[p][q], matrix[q][q] - matrix[p][p])
        cosine, sine = math.cos(angle), math.sin(angle)
        pp, qq, pq = matrix[p][p], matrix[q][q], matrix[p][q]
        matrix[p][p] = cosine * cosine * pp - 2 * sine * cosine * pq + sine * sine * qq
        matrix[q][q] = sine * sine * pp + 2 * sine * cosine * pq + cosine * cosine * qq
        matrix[p][q] = matrix[q][p] = 0.0
        for k in range(4):
            if k not in (p, q):
                kp, kq = matrix[k][p], matrix[k][q]
                matrix[k][p] = matrix[p][k] = cosine * kp - sine * kq
                matrix[k][q] = matrix[q][k] = sine * kp + cosine * kq
    return max(matrix[i][i] for i in range(4))


def aligned_rmsd(reference, mobile) -> float:
    """Kabsch RMSD through the equivalent quaternion eigenvalue problem."""
    if not reference or len(reference) != len(mobile):
        raise ValueError("RMSD requires equally sized nonempty position arrays")
    centered = []
    for positions in (reference, mobile):
        center = [sum(row[i] for row in positions) / len(positions) for i in range(3)]
        centered.append([[row[i] - center[i] for i in range(3)] for row in positions])
    a, b = centered
    covariance = [
        [sum(x[i] * y[j] for x, y in zip(a, b, strict=True)) for j in range(3)] for i in range(3)
    ]
    (xx, xy, xz), (yx, yy, yz), (zx, zy, zz) = covariance
    quaternion = [
        [xx + yy + zz, yz - zy, zx - xz, xy - yx],
        [yz - zy, xx - yy - zz, xy + yx, zx + xz],
        [zx - xz, xy + yx, yy - xx - zz, yz + zy],
        [xy - yx, zx + xz, yz + zy, zz - xx - yy],
    ]
    norm = sum(value * value for positions in centered for row in positions for value in row)
    return math.sqrt(max(0.0, norm - 2 * largest_eigenvalue(quaternion)) / len(a))


def compare(molecule: str, symbols: list[str], xtb: Optimization, ours: Optimization) -> Comparison:
    problems = [f"xTB: {problem}" for problem in xtb.problems]
    problems.extend(f"ours: {problem}" for problem in ours.problems)
    if xtb.steps is None:
        problems.append("xTB: not converged (no convergence marker)")
    if xtb.energy_hartree is None:
        problems.append("xTB: no final TOTAL ENERGY in Eh")
    if ours.steps is None or ours.energy_hartree is None:
        problems.append("ours: no valid steps and energy result")
    ratio = delta = rmsd = None
    if xtb.steps is not None and ours.steps is not None:
        ratio = ours.steps / xtb.steps if xtb.steps else (1.0 if ours.steps == 0 else math.inf)
        if ratio > STEP_RATIO_LIMIT:
            problems.append(f"step ratio exceeds {STEP_RATIO_LIMIT}")
    if xtb.energy_hartree is not None and ours.energy_hartree is not None:
        delta = abs(ours.energy_hartree - xtb.energy_hartree)
        if delta > ENERGY_LIMIT_HARTREE:
            problems.append(f"energy difference exceeds {ENERGY_LIMIT_HARTREE:g} Eh")
    if xtb.positions_angstrom is not None and ours.positions_angstrom is not None:
        heavy = [i for i, symbol in enumerate(symbols) if symbol not in {"H", "D", "T"}]
        if heavy:
            rmsd = aligned_rmsd(
                [xtb.positions_angstrom[i] for i in heavy],
                [ours.positions_angstrom[i] for i in heavy],
            )
    return Comparison(molecule, xtb, ours, ratio, delta, rmsd, problems)


def run_program(command: list[str], cwd: Path, env=None) -> tuple[str, float, list[str]]:
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
            check=False,
        )
    except OSError as error:
        return "", time.perf_counter() - started, [str(error)]
    problems = [f"command exited {completed.returncode}"] if completed.returncode else []
    return completed.stdout, time.perf_counter() - started, problems


def ours_command(options, xyz: Path, json_path: Path) -> list[str]:
    command = shlex.split(
        options.ours.format(
            xyz=str(xyz),
            json=str(json_path),
            charge=options.charge,
            uhf=options.uhf,
            level=options.level,
            threads=options.threads,
        )
    )
    if not command:
        raise ValueError("empty command")
    if "/" in command[0] or os.sep in command[0] or command[0].startswith("~"):
        command[0] = str(Path(command[0]).expanduser().resolve())
    return command


def run_molecule(path: Path, options) -> Comparison:
    symbols, _ = read_xyz(path)
    parent = Path(tempfile.gettempdir()).resolve()
    if parent.is_relative_to(ROOT):
        raise ValueError("set TMPDIR to a location outside the repository")
    with tempfile.TemporaryDirectory(prefix="reshiki-xtb-gate-", dir=parent) as temporary:
        work = Path(temporary)
        xtb_dir, ours_dir = work / "xtb", work / "ours"
        xtb_dir.mkdir()
        ours_dir.mkdir()
        output, elapsed, problems = run_program(
            [
                str(options.xtb),
                str(path),
                "--opt",
                options.level,
                "--gfn",
                "2",
                "--chrg",
                str(options.charge),
                "--uhf",
                str(options.uhf),
            ],
            xtb_dir,
            {**os.environ, "OMP_NUM_THREADS": str(options.threads)},
        )
        xtb = Optimization(seconds=elapsed, problems=problems)
        try:
            xtb.steps, xtb.energy_hartree = parse_xtb_output(output)
            optimized_symbols, xtb.positions_angstrom = read_xyz(xtb_dir / "xtbopt.xyz")
            if optimized_symbols != symbols:
                xtb.positions_angstrom = None
                xtb.problems.append("xtbopt.xyz atom order differs from the input")
        except (OSError, ValueError) as error:
            xtb.problems.append(str(error))
        json_path = ours_dir / "result.json"
        command = ours_command(options, path, json_path)
        _, elapsed, problems = run_program(command, ours_dir)
        try:
            ours = parse_ours(json_path, len(symbols))
            ours.problems.extend(problems)
            ours.seconds = elapsed
        except (OSError, ValueError, KeyError, OverflowError) as error:
            ours = Optimization(seconds=elapsed, problems=[*problems, str(error)])
        return compare(path.name, symbols, xtb, ours)


def report_paths(out: Path) -> tuple[Path, Path]:
    stem = out.with_suffix("") if out.suffix.lower() in {".md", ".csv"} else out
    return Path(f"{stem}.md"), Path(f"{stem}.csv")


def number(value: float | None, spec: str) -> str:
    return format(value, spec) if value is not None else "n/a"


def markdown(results: list[Comparison], options) -> str:
    lines = [
        f"xTB optimizer gate passed {sum(result.passed for result in results)} of {len(results)} molecules.",
        f"GFN2, {options.level} optimization, charge {options.charge}, UHF {options.uhf}, xTB threads {options.threads}.",
        "Pass requires convergence, successful commands, ours/xTB steps <= 1.5 and absolute energy difference <= 1e-6 Eh.",
        "Times measure subprocess wall seconds. Heavy-atom RMSD aligns input atom order and has no gate threshold.",
        "",
        "| Molecule | xTB steps | Ours steps | Steps ratio | xTB energy (Eh) | Ours energy (Eh) | Abs ΔE (Eh) | xTB (s) | Ours (s) | Heavy RMSD (Å) | Gate | Details |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |",
    ]
    for result in results:
        cells = [
            result.molecule,
            str(result.xtb.steps) if result.xtb.steps is not None else "not converged",
            str(result.ours.steps) if result.ours.steps is not None else "n/a",
            number(result.step_ratio, ".3f"),
            number(result.xtb.energy_hartree, ".12f"),
            number(result.ours.energy_hartree, ".12f"),
            number(result.energy_delta_hartree, ".3e"),
            number(result.xtb.seconds, ".3f"),
            number(result.ours.seconds, ".3f"),
            number(result.heavy_rmsd_angstrom, ".6f"),
            "PASS" if result.passed else "FAIL",
            ", ".join(result.problems) or "",
        ]
        lines.append(
            "| "
            + " | ".join(
                cell.replace("|", "\\|").replace("\n", " ").replace("\r", " ") for cell in cells
            )
            + " |"
        )
    return "\n".join(lines) + "\n"


def write_reports(results: list[Comparison], options) -> str:
    text = markdown(results, options)
    markdown_path, csv_path = report_paths(options.out)
    markdown_path.parent.mkdir(parents=True, exist_ok=True)
    markdown_path.write_text(text, encoding="utf-8")
    with csv_path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output)
        writer.writerow(
            [
                "molecule",
                "passed",
                "xtb_steps",
                "ours_steps",
                "step_ratio",
                "xtb_energy_hartree",
                "ours_energy_hartree",
                "abs_energy_delta_hartree",
                "xtb_seconds",
                "ours_seconds",
                "heavy_rmsd_angstrom",
                "problems",
            ]
        )
        for result in results:
            writer.writerow(
                [
                    result.molecule,
                    result.passed,
                    result.xtb.steps,
                    result.ours.steps,
                    result.step_ratio,
                    result.xtb.energy_hartree,
                    result.ours.energy_hartree,
                    result.energy_delta_hartree,
                    result.xtb.seconds,
                    result.ours.seconds,
                    result.heavy_rmsd_angstrom,
                    ", ".join(result.problems),
                ]
            )
    return text


def positive_int(value: str) -> int:
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return number


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--xyz-dir",
        required=True,
        type=Path,
        help="directory containing one molecule per .xyz file",
    )
    parser.add_argument("--xtb", required=True, type=Path, help="path to the xTB binary")
    parser.add_argument(
        "--ours", required=True, help='command template, with quoted "{xyz}" and "{json}" paths'
    )
    parser.add_argument("--level", choices=("normal", "tight"), default="normal")
    parser.add_argument(
        "--out", required=True, type=Path, help="output stem for .md and .csv reports"
    )
    parser.add_argument("--charge", type=int, default=0)
    parser.add_argument("--uhf", type=int, default=0)
    parser.add_argument("--threads", type=positive_int, default=1)
    options = parser.parse_args(argv)
    options.xtb = options.xtb.expanduser().resolve()
    if not options.xtb.is_file():
        parser.error("--xtb must name an existing binary file")
    if not options.xyz_dir.is_dir():
        parser.error("--xyz-dir must name an existing directory")
    if "{xyz}" not in options.ours or "{json}" not in options.ours:
        parser.error('--ours requires both "{xyz}" and "{json}" placeholders')
    try:
        ours_command(options, Path("input.xyz"), Path("result.json"))
    except (ValueError, KeyError, IndexError, AttributeError) as error:
        parser.error(f"invalid --ours template: {error}")
    paths = sorted(
        path.resolve()
        for path in options.xyz_dir.iterdir()
        if path.is_file() and path.suffix.lower() == ".xyz"
    )
    if not paths:
        print("No .xyz molecules found.", file=sys.stderr)
        return 1
    results = []
    for path in paths:
        try:
            results.append(run_molecule(path, options))
        except (OSError, ValueError, KeyError, IndexError, OverflowError) as error:
            results.append(
                Comparison(
                    path.name, Optimization(), Optimization(), None, None, None, [str(error)]
                )
            )
    try:
        print(write_reports(results, options), end="")
    except OSError as error:
        print(f"Cannot write reports: {error}", file=sys.stderr)
        return 1
    return int(any(not result.passed for result in results))


if __name__ == "__main__":
    raise SystemExit(main())
