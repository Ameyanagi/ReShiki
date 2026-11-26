"""xTB energies, geometry optimizations, frequencies and conformer searches.

Install with ``pip install "reshiki[xtb]"``. Each calculation uses the geometry's
charge and multiplicity unless you override them. Choose ``method="gfn2"``
(GFN2-xTB), ``"gfnff"`` (GFN-FF) or ``"gxtb"`` (g-xTB). The full method names
also work, regardless of case.

``solvent="water"`` uses ALPB by default. Set ``solvent_model="gbsa"`` for
GBSA. In solution, g-xTB adds the GFN2-xTB solvation energy for the chosen model
(ALPB or GBSA) to the gas-phase g-xTB energy. ``electronic_temperature`` is in kelvin.
``accuracy`` and ``max_iterations`` control SCF convergence. Use
``conventions="xtb-6.8"`` for the xTB 6.8 conventions where supported.
Each worker uses one thread by default.
Ctrl+C cancels a calculation running on the main thread, while a calculation
started in another thread runs to completion.
"""

from __future__ import annotations

import importlib
import json
import sys
from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING, Any, Literal, Sequence, cast

from . import Molecule, __version__, _native, parse
from ._errors import ReShikiError
from .qc import (
    Geometry,
    Result,
    _geometry_data,
    _geometry_from_data,
    _import_numpy,
    _result_from_data,
)

if TYPE_CHECKING:
    import numpy as np
    from numpy.typing import NDArray

__all__ = [
    "Conformer",
    "ConformerSearch",
    "Optimization",
    "XtbError",
    "XtbNotInstalled",
    "available",
    "conformers",
    "energy",
    "engine",
    "frequencies",
    "optimize",
]

_INSTALL = 'xTB calculations need the reshiki-xtb package: pip install "reshiki[xtb]"'


class XtbNotInstalled(ReShikiError):
    """The optional xTB engine is missing or unavailable on this platform."""

    def __init__(self, message: str = _INSTALL) -> None:
        super().__init__(message, "xtb_not_installed")


class XtbError(ReShikiError):
    """An xTB calculation failed, or the installed worker is incompatible."""


@dataclass(frozen=True, kw_only=True)
class Optimization(Result):
    """An optimization and its accepted geometries, including the input.

    ``steps`` counts accepted displacements. ``energies`` contains one energy
    in hartree per geometry. ``trajectory`` has shape (frames, atoms, 3) in
    ångströms. The result includes the properties at the last geometry.
    """

    converged: bool
    steps: int
    energies: NDArray[np.float64]
    trajectory: NDArray[np.float64]


@dataclass(frozen=True)
class Conformer:
    """A ranked geometry with energy in hartree and RMSD to rank 1 in ångströms.

    ``delta`` and ``start_delta`` are in kcal/mol. ``start_delta`` is present
    for force-field starts. ``population`` is a Boltzmann fraction at 298.15 K.
    ``stereo`` records whether the drawing's stereochemistry holds, or ``None``
    if the check was unavailable.
    """

    rank: int
    geometry: Geometry
    energy: float
    delta: float
    start_delta: float | None
    population: float
    rmsd: float
    cluster_size: int
    stereo: bool | None = None


@dataclass(frozen=True)
class ConformerSearch:
    """The energy-ranked ensemble, best optimization and search summary."""

    conformers: tuple[Conformer, ...]
    best: Optimization
    starts: int
    clusters: int
    reranked: int
    planned: int
    notes: tuple[str, ...]
    method: str
    _xyz: str = field(repr=False)

    def to_xyz(self) -> str:
        """Return the ensemble as multi-frame XYZ in energy order."""
        return self._xyz


def _worker_path() -> str:
    if sys.platform == "win32":
        raise XtbNotInstalled("xTB calculations are not available on Windows yet.")
    try:
        distribution = importlib.import_module("reshiki_xtb")
    except ImportError as error:
        raise XtbNotInstalled() from error
    version = distribution.__version__
    if version != __version__:
        raise XtbError(
            f"reshiki {__version__} needs reshiki-xtb {__version__}, found {version}: "
            f'pip install "reshiki[xtb]=={__version__}"'
        )
    try:
        return str(distribution.worker_path())
    except FileNotFoundError as error:
        raise XtbNotInstalled(str(error)) from error


def available() -> bool:
    """Return whether the matching xTB worker can start."""
    try:
        engine()
    except (XtbNotInstalled, XtbError):
        return False
    return True


def engine() -> dict[str, str]:
    """Return the installed engine's name, version, license, source URL and revision."""
    path = _worker_path()
    try:
        return cast(dict[str, str], json.loads(_native.xtb_engine(path)))
    except ReShikiError as error:
        raise XtbError(str(error)) from error


def _calculate(
    request: dict[str, Any],
    geometry: Geometry,
    method: str,
    charge: int | None,
    multiplicity: int | None,
    unpaired: int | None,
    solvent: str | None,
    solvent_model: str,
    electronic_temperature: float | None,
    accuracy: float | None,
    max_iterations: int | None,
    conventions: str,
    threads: int,
) -> dict[str, Any]:
    path = _worker_path()
    if charge is not None or multiplicity is not None:
        geometry = replace(
            geometry,
            charge=geometry.charge if charge is None else charge,
            multiplicity=geometry.multiplicity if multiplicity is None else multiplicity,
        )
    options = {
        "geometry": _geometry_data(geometry),
        "method": method,
        "unpaired": unpaired,
        "solvent": solvent,
        "solvent_model": solvent_model,
        "electronic_temperature": electronic_temperature,
        "accuracy": accuracy,
        "max_iterations": max_iterations,
        "conventions": conventions,
        "threads": threads,
        "request": request,
    }
    try:
        data = json.loads(_native.xtb_calculate(path, json.dumps(options, allow_nan=False)))
    except ValueError as error:
        if isinstance(error, ReShikiError) and error.code == "invalid_arguments":
            raise
        raise XtbError(str(error)) from error
    return data


def energy(
    geometry: Geometry,
    *,
    gradient: bool = False,
    bond_orders: bool = False,
    orbitals: bool = False,
    method: str = "gfn2",
    charge: int | None = None,
    multiplicity: int | None = None,
    unpaired: int | None = None,
    solvent: str | None = None,
    solvent_model: Literal["alpb", "gbsa"] = "alpb",
    electronic_temperature: float | None = None,
    accuracy: float | None = None,
    max_iterations: int | None = None,
    conventions: Literal["axtb", "xtb-6.8"] = "axtb",
    threads: int = 1,
) -> Result:
    """Calculate a single-point energy in hartree.

    For example, ``energy(water, gradient=True, solvent="water")`` returns
    the energy and gradient in water. Request orbital energies and occupations
    with ``orbitals=True`` or bond orders with ``bond_orders=True``.
    ``unpaired`` sets the number of unpaired electrons in the engine's valence
    space and defaults to the multiplicity minus one. For lanthanides, pass
    ``unpaired=result.unpaired`` to follow-up calls to keep the same valence spin.
    """
    request = {
        "operation": "energy",
        "gradient": gradient,
        "bond_orders": bond_orders,
        "orbitals": orbitals,
    }
    return _result_from_data(
        _calculate(
            request,
            geometry,
            method,
            charge,
            multiplicity,
            unpaired,
            solvent,
            solvent_model,
            electronic_temperature,
            accuracy,
            max_iterations,
            conventions,
            threads,
        )
    )


def optimize(
    geometry: Geometry,
    *,
    level: Literal["normal", "tight"] = "normal",
    max_steps: int | None = None,
    fixed_atoms: Sequence[int] = (),
    method: str = "gfn2",
    charge: int | None = None,
    multiplicity: int | None = None,
    unpaired: int | None = None,
    solvent: str | None = None,
    solvent_model: Literal["alpb", "gbsa"] = "alpb",
    electronic_temperature: float | None = None,
    accuracy: float | None = None,
    max_iterations: int | None = None,
    conventions: Literal["axtb", "xtb-6.8"] = "axtb",
    threads: int = 1,
) -> Optimization:
    """Relax a geometry and return the final properties and accepted steps.

    For example, ``optimize(water, level="tight", fixed_atoms=[0])`` holds the
    first atom fixed. Indices start at zero. The default limit is 500 steps.
    A calculation that reaches the limit returns with ``converged=False``.
    ``unpaired`` sets the number of unpaired electrons in the engine's valence
    space and defaults to the multiplicity minus one. For lanthanides, pass
    ``unpaired=result.unpaired`` to follow-up calls to keep the same valence spin.
    """
    request = {
        "operation": "optimize",
        "level": level,
        "max_steps": max_steps,
        "fixed_atoms": list(fixed_atoms),
    }
    data = _calculate(
        request,
        geometry,
        method,
        charge,
        multiplicity,
        unpaired,
        solvent,
        solvent_model,
        electronic_temperature,
        accuracy,
        max_iterations,
        conventions,
        threads,
    )
    return _result_from_data(data, Optimization, **_optimization_fields(data))


def _optimization_fields(data: dict[str, Any]) -> dict[str, Any]:
    np = _import_numpy()

    energies = np.asarray(data["energies"], dtype=np.float64)
    trajectory = np.asarray(data["trajectory"], dtype=np.float64)
    energies.setflags(write=False)
    trajectory.setflags(write=False)
    return {
        "converged": data["converged"],
        "steps": data["steps"],
        "energies": energies,
        "trajectory": trajectory,
    }


def frequencies(
    geometry: Geometry,
    *,
    temperature: float = 298.15,
    pressure: float = 101325.0,
    symmetry_number: int | None = None,
    method: str = "gfn2",
    charge: int | None = None,
    multiplicity: int | None = None,
    unpaired: int | None = None,
    solvent: str | None = None,
    solvent_model: Literal["alpb", "gbsa"] = "alpb",
    electronic_temperature: float | None = None,
    accuracy: float | None = None,
    max_iterations: int | None = None,
    conventions: Literal["axtb", "xtb-6.8"] = "axtb",
    threads: int = 1,
) -> Result:
    """Calculate the Hessian, vibrations and thermochemistry.

    Use an optimized geometry, for example ``frequencies(optimize(water).geometry)``.
    Specify the thermochemistry temperature in kelvin and pressure in pascals.
    Frequencies are in cm⁻¹, with imaginary modes reported as negative values.
    By default the engine chooses the rotational symmetry number from the geometry.
    The engine's symmetry ignores isotopes, so a geometry with isotopes needs an
    explicit ``symmetry_number``: for example 2 for D₂O and 1 for HDO.
    ``unpaired`` sets the number of unpaired electrons in the engine's valence
    space and defaults to the multiplicity minus one. For lanthanides, pass
    ``unpaired=result.unpaired`` to follow-up calls to keep the same valence spin.
    """
    if symmetry_number is None and geometry.isotopes is not None and geometry.isotopes.any():
        raise ReShikiError(
            "Pass symmetry_number for a geometry with isotopes (for example 2 for D2O, 1 for HDO); "
            "automatic symmetry ignores isotopes.",
            "invalid_arguments",
        )
    request = {
        "operation": "frequencies",
        "temperature": temperature,
        "pressure": pressure,
        "symmetry_number": symmetry_number,
    }
    return _result_from_data(
        _calculate(
            request,
            geometry,
            method,
            charge,
            multiplicity,
            unpaired,
            solvent,
            solvent_model,
            electronic_temperature,
            accuracy,
            max_iterations,
            conventions,
            threads,
        )
    )


def conformers(
    molecule: Molecule | str | bytes,
    *,
    starts: int = 100,
    minimizer: Literal["mmff94s", "mmff94", "uff", "gfnff", "gfn2"] = "mmff94s",
    diversity: Literal["fine", "normal", "coarse"] = "normal",
    window: float = 15.0,
    representatives: int = 20,
    per_cluster: Literal[1, 3] = 1,
    method: str = "gfn2",
    charge: int | None = None,
    unpaired: int | None = None,
    solvent: str | None = None,
    solvent_model: Literal["alpb", "gbsa"] = "alpb",
    electronic_temperature: float | None = None,
    accuracy: float | None = None,
    max_iterations: int | None = None,
    conventions: Literal["axtb", "xtb-6.8"] = "axtb",
    threads: int | None = None,
) -> ConformerSearch:
    """Search one molecule using the desktop app's conformer pipeline.

    Pass a ``Molecule`` or molecular structure text or bytes accepted by
    ``reshiki.parse``. The energy ``window`` is in kcal/mol. With a force-field
    minimizer, xTB re-ranks cluster representatives using ``method``.
    ``threads`` sets the maximum number of concurrent xTB jobs, from 1 to 8.
    Each job uses one engine thread. ``threads=None`` uses the app's Auto setting.

    .. code-block:: python

        from pathlib import Path
        from reshiki import xtb
        search = xtb.conformers("CCCC")
        print(search.method)
        print(search.best.energy)
        Path("butane.xyz").write_text(search.to_xyz(), encoding="utf-8")
    """
    path = _worker_path()
    drawing = molecule if isinstance(molecule, Molecule) else parse(molecule)
    options = {
        "starts": starts,
        "minimizer": minimizer,
        "diversity": diversity,
        "window": window,
        "representatives": representatives,
        "per_cluster": per_cluster,
        "method": method,
        "charge": charge,
        "unpaired": unpaired,
        "solvent": solvent,
        "solvent_model": solvent_model,
        "electronic_temperature": electronic_temperature,
        "accuracy": accuracy,
        "max_iterations": max_iterations,
        "conventions": conventions,
        "threads": threads,
    }
    try:
        request_json = json.dumps(options, allow_nan=False)
    except ValueError as error:
        raise ReShikiError(f"Invalid conformer settings: {error}", "invalid_arguments") from error
    try:
        data = json.loads(_native.xtb_conformers(path, drawing._data, request_json))
    except ReShikiError as error:
        if error.code == "invalid_arguments":
            raise
        raise XtbError(str(error)) from error
    best = data["best"]
    return ConformerSearch(
        conformers=tuple(
            Conformer(
                rank=row["rank"],
                geometry=_geometry_from_data(row["geometry"]),
                energy=row["energy"],
                delta=row["delta"],
                start_delta=row["start_delta"],
                population=row["population"],
                rmsd=row["rmsd"],
                cluster_size=row["cluster_size"],
                stereo=row.get("stereo"),
            )
            for row in data["conformers"]
        ),
        best=_result_from_data(best, Optimization, **_optimization_fields(best)),
        starts=data["starts"],
        clusters=data["clusters"],
        reranked=data["reranked"],
        planned=data["planned"],
        notes=tuple(data["notes"]),
        method=data["method"],
        _xyz=data["xyz"],
    )
