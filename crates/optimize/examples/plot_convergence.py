#!/usr/bin/env python3
"""Plot MMFF94 optimization traces from the benchmark JSON output.

Run with matplotlib installed, for example

    uv run --no-project --with matplotlib python \
        crates/optimize/examples/plot_convergence.py traces.json plots \
        --molecules aspirin,caffeine,ibuprofen,naproxen

The first four molecules in the JSON are selected when --molecules is omitted.
All traces contribute to each molecule's reference minimum. Only the normal
ANC-RFO and L-BFGS traces appear in the plots.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

from matplotlib.axes import Axes
from matplotlib.backends.backend_agg import FigureCanvasAgg
from matplotlib.figure import Figure
from matplotlib.ticker import MaxNLocator


def load_traces(path: Path) -> dict[str, dict[str, list[float]]]:
    """Read finite energy sequences, preserving the JSON molecule order."""
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict) or not raw:
        raise ValueError("The traces JSON must contain a nonempty molecule object.")

    traces: dict[str, dict[str, list[float]]] = {}
    for molecule, methods in raw.items():
        if not isinstance(methods, dict):
            raise ValueError(f"{molecule!r} must contain a method object.")
        traces[molecule] = {}
        for method, energies in methods.items():
            if not isinstance(energies, list):
                raise ValueError(f"{molecule!r}, {method!r} must contain an energy list.")
            if any(
                isinstance(energy, bool)
                or not isinstance(energy, (int, float))
                or not math.isfinite(energy)
                for energy in energies
            ):
                raise ValueError(f"{molecule!r}, {method!r} contains an invalid energy.")
            traces[molecule][method] = [float(energy) for energy in energies]
    return traces


def select_molecules(traces: dict[str, dict[str, list[float]]], selection: str | None) -> list[str]:
    """Select four distinct molecules with the required normal traces."""
    molecules = (
        [name.strip() for name in selection.split(",")]
        if selection is not None
        else list(traces)[:4]
    )
    if len(molecules) != 4 or len(set(molecules)) != 4:
        raise ValueError("Select exactly four distinct molecules with --molecules a,b,c,d.")

    for molecule in molecules:
        if molecule not in traces:
            raise ValueError(f"Molecule {molecule!r} is missing from the traces JSON.")
        if not molecule or molecule in {".", ".."} or "/" in molecule or "\\" in molecule:
            raise ValueError(f"Molecule {molecule!r} cannot be used in an output filename.")
        for method in ("ANC-RFO", "L-BFGS"):
            if method not in traces[molecule]:
                raise ValueError(f"Molecule {molecule!r} is missing the {method!r} trace.")
        if not any(traces[molecule].values()):
            raise ValueError(f"Molecule {molecule!r} needs at least one nonempty energy trace.")
    return molecules


def draw_convergence(axes: Axes, molecule: str, traces: dict[str, list[float]]) -> None:
    """Draw normal methods relative to the lowest energy across every trace."""
    minimum = min(energy for energies in traces.values() for energy in energies)
    last_step = 1
    for method, color in (("ANC-RFO", "#0072B2"), ("L-BFGS", "#D55E00")):
        energies = traces[method]
        differences = [max(energy - minimum, 1e-6) for energy in energies]
        axes.plot(
            range(len(energies)), differences, label=method, color=color, linewidth=1.8, marker=""
        )
        last_step = max(last_step, len(energies) - 1)

    axes.set_title(f"{molecule}: MMFF94 energy vs step", fontsize=14)
    axes.set_xlabel("Step", fontsize=12)
    axes.set_ylabel(r"$E - E_{\min}$ (kcal/mol)", fontsize=12)
    axes.set_yscale("log")
    axes.set_ylim(bottom=8e-7)
    axes.set_xlim(0, last_step)
    axes.xaxis.set_major_locator(MaxNLocator(integer=True))
    axes.tick_params(labelsize=11)
    axes.grid(which="both", linestyle=":", alpha=0.3)
    axes.legend(fontsize=11)


def save_plots(
    traces: dict[str, dict[str, list[float]]], molecules: list[str], output_directory: Path
) -> list[Path]:
    """Save four 1200 by 800 plots and a two by two overview at 150 DPI."""
    output_directory.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    for molecule in molecules:
        figure = Figure(figsize=(1200 / 150, 800 / 150), dpi=150, layout="constrained")
        FigureCanvasAgg(figure)
        draw_convergence(figure.subplots(), molecule, traces[molecule])
        path = output_directory / f"convergence-{molecule}.png"
        figure.savefig(path, dpi=150)
        paths.append(path)

    grid = Figure(figsize=(2400 / 150, 1600 / 150), dpi=150, layout="constrained")
    FigureCanvasAgg(grid)
    for axes, molecule in zip(grid.subplots(2, 2).flat, molecules, strict=True):
        draw_convergence(axes, molecule, traces[molecule])
    path = output_directory / "convergence-grid.png"
    grid.savefig(path, dpi=150)
    paths.append(path)
    return paths


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traces", type=Path, help="Benchmark traces JSON file")
    parser.add_argument("out_dir", type=Path, help="Directory for the PNG plots")
    parser.add_argument(
        "--molecules",
        metavar="a,b,c,d",
        help="Four molecule names in display order, defaulting to the first four in the JSON",
    )
    args = parser.parse_args(argv)
    try:
        traces = load_traces(args.traces)
        molecules = select_molecules(traces, args.molecules)
        paths = save_plots(traces, molecules, args.out_dir)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    for path in paths:
        print(path)


if __name__ == "__main__":
    main()
