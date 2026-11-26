"""xTB energies, geometry optimizations and frequencies.

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
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Any, Literal, Sequence, cast

from . import __version__, _native
from ._errors import ReShikiError
from .qc import Geometry, Result, _geometry_data, _result_from_data

if TYPE_CHECKING:
    import numpy as np
    from numpy.typing import NDArray

__all__ = [
    "Optimization",
    "XtbError",
    "XtbNotInstalled",
    "available",
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
    import numpy as np

    energies = np.asarray(data["energies"], dtype=np.float64)
    trajectory = np.asarray(data["trajectory"], dtype=np.float64)
    energies.setflags(write=False)
    trajectory.setflags(write=False)
    return _result_from_data(
        data,
        Optimization,
        converged=data["converged"],
        steps=data["steps"],
        energies=energies,
        trajectory=trajectory,
    )


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
