"""Molecular geometries, quantum chemistry files, and calculation results.

Coordinates use ångströms, and energies use hartree. Making geometries and reading
calculation data require NumPy.
Read a calculation with ``qc.read("water.log")`` or make a geometry with
``Geometry.from_symbols(["O", "H", "H"], [[0, 0, 0], [0, 0, 1], [1, 0, 0]])``.
The same :class:`Result` describes imported calculations and ``reshiki.xtb`` results.
"""

from __future__ import annotations

import json
import warnings
from collections.abc import Sequence
from dataclasses import dataclass, field
from numbers import Integral
from os import PathLike
from pathlib import Path
from typing import TYPE_CHECKING, Any, TypeVar, overload

from . import _native
from ._errors import ReShikiError

if TYPE_CHECKING:
    import numpy as np
    from numpy.typing import ArrayLike, NDArray

__all__ = ["Geometry", "Thermo", "Result", "Step", "Job", "QcFile", "read", "write_input"]

_SYMBOLS: tuple[str, ...] = tuple(json.loads(_native.qc_elements()))
_NUMBERS = {symbol.lower(): number for number, symbol in enumerate(_SYMBOLS) if number}
_NEAR_DEGENERATE_WARNING = (
    "Near-degenerate electronic structure: results can differ between computers. "
    "Try a higher electronic temperature or more unpaired electrons."
)


def _invalid(message: str) -> ReShikiError:
    return ReShikiError(message, "invalid_arguments")


def _integer(value: int, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, Integral):
        raise _invalid(f"{name} must be an integer.")
    return int(value)


@dataclass(frozen=True)
class Geometry:
    """Atoms and their electronic state, with read-only NumPy arrays.

    ``numbers`` has shape ``(N,)`` and atomic numbers 1–118. ``positions`` has
    shape ``(N, 3)`` in ångströms. An omitted multiplicity becomes 1 for an even
    electron count or 2 for an odd count. Arrays are copied when constructed.

    Example: ``Geometry.from_symbols(["H", "H"], [[0, 0, 0], [0, 0, 0.74]])``.
    """

    numbers: NDArray[np.int64]
    positions: NDArray[np.float64]
    charge: int = 0
    multiplicity: int | None = None
    title: str = ""

    def __post_init__(self) -> None:
        import numpy as np

        numbers = np.asarray(self.numbers)
        if numbers.ndim != 1 or numbers.size == 0:
            raise _invalid("Atomic numbers must be a nonempty array with shape (N,).")
        if not np.issubdtype(numbers.dtype, np.integer) or np.any((numbers < 1) | (numbers > 118)):
            raise _invalid("Atomic numbers must be integers from 1 to 118.")
        numbers = np.array(numbers, dtype=np.int64, copy=True)
        try:
            positions = np.array(self.positions, dtype=np.float64, copy=True)
        except (TypeError, ValueError) as error:
            raise _invalid("Positions must be numbers with shape (N, 3).") from error
        if positions.shape != (numbers.size, 3):
            raise _invalid("Positions must have shape (N, 3), one xyz position per atom.")
        if not np.isfinite(positions).all():
            raise _invalid("Positions must be finite numbers.")
        charge = _integer(self.charge, "Charge")
        if not -(2**31) <= charge < 2**31:
            raise _invalid("Charge is outside the supported integer range.")
        electrons = int(numbers.sum()) - charge
        multiplicity = (
            1 + electrons % 2
            if self.multiplicity is None
            else _integer(self.multiplicity, "Multiplicity")
        )
        unpaired = multiplicity - 1
        if unpaired < 0 or unpaired > electrons or (electrons - unpaired) % 2:
            raise _invalid("Charge and multiplicity do not fit the number of electrons.")
        if not isinstance(self.title, str):
            raise _invalid("Title must be a string.")
        numbers.setflags(write=False)
        positions.setflags(write=False)
        object.__setattr__(self, "numbers", numbers)
        object.__setattr__(self, "positions", positions)
        object.__setattr__(self, "charge", charge)
        object.__setattr__(self, "multiplicity", multiplicity)

    @property
    def symbols(self) -> tuple[str, ...]:
        """Element symbols in atom order, for example ``("O", "H", "H")``."""
        return tuple(_SYMBOLS[int(number)] for number in self.numbers)

    def __len__(self) -> int:
        """The number of atoms."""
        return len(self.numbers)

    @classmethod
    def from_symbols(
        cls,
        symbols: Sequence[str | int],
        positions: ArrayLike,
        *,
        charge: int = 0,
        multiplicity: int | None = None,
        title: str = "",
    ) -> Geometry:
        """Build a geometry from element symbols or atomic numbers.

        Symbols are case-insensitive: ``Geometry.from_symbols(["He"], [[0, 0, 0]])``.
        Positions use ångströms and follow the same atom order as ``symbols``.
        """
        import numpy as np

        numbers = []
        for symbol in symbols:
            if isinstance(symbol, str):
                number = _NUMBERS.get(symbol.lower())
                if number is None:
                    raise _invalid(f"Unknown element symbol: '{symbol}'.")
            else:
                number = _integer(symbol, "Atomic number")
            numbers.append(number)
        return cls(
            np.asarray(numbers, dtype=np.int64),
            np.asarray(positions, dtype=np.float64),
            charge,
            multiplicity,
            title,
        )

    @classmethod
    def from_xyz(cls, text: str, *, charge: int = 0, multiplicity: int | None = None) -> Geometry:
        """Read the first frame of an XYZ string.

        Example: ``Geometry.from_xyz("1\\nhelium\\nHe 0 0 0\\n")``.
        The keyword arguments set the electronic state.
        """
        lines = text.splitlines()
        try:
            count = int(lines[0].strip())
        except (IndexError, ValueError) as error:
            raise _invalid("An XYZ file must start with the number of atoms.") from error
        first = "\n".join(lines[: count + 2]) + "\n"
        # The constructor sets the electronic state instead of using the file
        # reader's charge and spin assumption.
        file = json.loads(_native.qc_read("geometry.xyz", first.encode()))
        geometry = file["jobs"][0]["steps"][0]["geometry"]
        return cls.from_symbols(
            geometry["numbers"],
            geometry["positions"],
            charge=charge,
            multiplicity=multiplicity,
            title=geometry["title"],
        )

    def to_xyz(self) -> str:
        """Write one XYZ frame, retaining coordinate precision and the title.

        Example: ``Path("water.xyz").write_text(water.to_xyz())``.
        For an XYZ round trip, pass the charge and multiplicity when reading it again.
        """
        title = self.title.replace("\r", " ").replace("\n", " ")
        rows = [str(len(self)), title]
        rows.extend(
            f"{symbol} {x:.17g} {y:.17g} {z:.17g}"
            for symbol, (x, y, z) in zip(self.symbols, self.positions.tolist())
        )
        return "\n".join(rows) + "\n"


@dataclass(frozen=True)
class Thermo:
    """Harmonic thermochemistry at ``temperature`` K and ``pressure`` Pa.

    ``zero_point`` is a correction in hartree. When electronic energy is available,
    ``enthalpy`` and ``free_energy`` include it and use hartree. ``entropy`` uses hartree/K.
    """

    temperature: float
    pressure: float
    zero_point: float | None = None
    enthalpy: float | None = None
    entropy: float | None = None
    free_energy: float | None = None


@dataclass(frozen=True)
class Result:
    """Calculated values belonging to exactly one ``geometry``.

    Missing data is ``None``. Energy and orbital energies use hartree. The
    gradient ``(N, 3)`` uses hartree/bohr and the Hessian ``(3N, 3N)`` uses hartree/bohr².
    Charges ``(N,)`` use electron charges, the dipole ``(3,)`` uses debye, and
    ``homo_lumo_gap`` uses eV. Bond-order keys are zero-based atom pairs.

    Frequencies ``(K,)`` use cm⁻¹, with negative imaginary modes. IR intensities
    ``(K,)`` use km/mol. Normal modes ``(K, N, 3)`` are Cartesian displacement
    vectors normalized as supplied by the program. Orbital energies and
    occupations refer to alpha orbitals. Beta arrays are present when separate
    beta data is available. NumPy arrays are read-only.
    """

    method: str
    program: str | None
    geometry: Geometry
    energy: float | None = None
    gradient: NDArray[np.float64] | None = None
    hessian: NDArray[np.float64] | None = None
    charges: NDArray[np.float64] | None = None
    dipole: NDArray[np.float64] | None = None
    bond_orders: dict[tuple[int, int], float] | None = None
    orbital_energies: NDArray[np.float64] | None = None
    occupations: NDArray[np.float64] | None = None
    beta_orbital_energies: NDArray[np.float64] | None = None
    beta_occupations: NDArray[np.float64] | None = None
    homo_lumo_gap: float | None = None
    near_degenerate: bool = False
    frequencies: NDArray[np.float64] | None = None
    ir_intensities: NDArray[np.float64] | None = None
    normal_modes: NDArray[np.float64] | None = None
    thermo: Thermo | None = None
    warnings: tuple[str, ...] = ()


@dataclass(frozen=True)
class Step:
    """One geometry, its optional energy in hartree, and its file label."""

    geometry: Geometry
    energy: float | None
    label: str | None


@dataclass(frozen=True)
class Job:
    """One calculation or geometry collection, with its steps in file order.

    ``kind`` is ``geometry``, ``single-point``, ``optimization``, ``frequencies``,
    ``scan`` or ``other``. ``completion`` is ``normal``, ``error``, ``truncated``
    or ``None`` when the format does not record how the calculation ended.
    """

    index: int
    kind: str
    method: str | None
    basis: str | None
    keywords: tuple[str, ...]
    completion: str | None
    completion_message: str | None
    steps: tuple[Step, ...]
    result: Result | None

    @property
    def geometry(self) -> Geometry | None:
        """The last geometry in the job, or ``None`` if no geometry was read."""
        return self.steps[-1].geometry if self.steps else None


@dataclass(frozen=True)
class QcFile:
    """A quantum chemistry file, its jobs, and warnings about omitted data."""

    format: str
    program: str | None
    program_version: str | None
    jobs: tuple[Job, ...]
    warnings: tuple[str, ...]
    _default_job_index: int | None = field(default=None, repr=False)

    @property
    def default_job(self) -> Job | None:
        """The last normally completed job with atoms, else the last job with atoms."""
        return self.jobs[self._default_job_index] if self._default_job_index is not None else None


def _geometry_data(geometry: Geometry) -> dict[str, Any]:
    if not isinstance(geometry, Geometry):
        raise TypeError("geometry must be a reshiki.qc.Geometry")
    return {
        "numbers": geometry.numbers.tolist(),
        "positions": geometry.positions.tolist(),
        "charge": geometry.charge,
        "multiplicity": geometry.multiplicity,
        "title": geometry.title,
    }


def _geometry_from_data(data: dict[str, Any]) -> Geometry:
    import numpy as np

    return Geometry(
        np.asarray(data["numbers"], dtype=np.int64),
        np.asarray(data["positions"], dtype=np.float64),
        data["charge"],
        data["multiplicity"],
        data.get("title", ""),
    )


def _array(data: Any, shape: tuple[int, ...] = (-1,)) -> NDArray[np.float64] | None:
    if data is None:
        return None
    import numpy as np

    array = np.array(data, dtype=np.float64).reshape(shape)
    array.setflags(write=False)
    return array


_ResultT = TypeVar("_ResultT", bound=Result)


@overload
def _result_from_data(data: dict[str, Any], cls: type[_ResultT], **extra: Any) -> _ResultT: ...


@overload
def _result_from_data(data: dict[str, Any], **extra: Any) -> Result: ...


def _result_from_data(data: dict[str, Any], cls: type[Result] = Result, **extra: Any) -> Result:
    geometry = _geometry_from_data(data["geometry"])
    n = len(geometry)
    messages = list(data.get("warnings", ()))
    if data.get("near_degenerate", False) and _NEAR_DEGENERATE_WARNING not in messages:
        messages.append(_NEAR_DEGENERATE_WARNING)
    for message in messages:
        warnings.warn(message, UserWarning, stacklevel=3)
    return cls(
        method=data["method"],
        program=data.get("program"),
        geometry=geometry,
        energy=data.get("energy"),
        gradient=_array(data.get("gradient"), (n, 3)),
        hessian=_array(data.get("hessian"), (3 * n, 3 * n)),
        charges=_array(data.get("charges")),
        dipole=_array(data.get("dipole"), (3,)),
        bond_orders=(
            {(a, b): value for a, b, value in data["bond_orders"]}
            if data.get("bond_orders") is not None
            else None
        ),
        orbital_energies=_array(data.get("orbital_energies")),
        occupations=_array(data.get("occupations")),
        beta_orbital_energies=_array(data.get("beta_orbital_energies")),
        beta_occupations=_array(data.get("beta_occupations")),
        homo_lumo_gap=data.get("homo_lumo_gap"),
        near_degenerate=data.get("near_degenerate", False),
        frequencies=_array(data.get("frequencies")),
        ir_intensities=_array(data.get("ir_intensities")),
        normal_modes=_array(data.get("normal_modes"), (-1, n, 3)),
        thermo=Thermo(**data["thermo"]) if data.get("thermo") is not None else None,
        warnings=tuple(messages),
        **extra,
    )


def read(source: str | PathLike[str] | bytes, *, name: str | None = None) -> QcFile:
    """Read a path, or file bytes with ``name=`` identifying the format.

    Supports XYZ, SDF, Molden, Gaussian fchk/log/input, ORCA output/input, and
    GAMESS input. For example, ``qc.read(Path("water.log"))`` or
    ``qc.read(downloaded_bytes, name="water.fchk")``. Reader warnings are emitted
    with ``warnings.warn`` and retained on the returned file.
    """
    if isinstance(source, bytes):
        if name is None:
            raise _invalid("Reading bytes needs name=, for example name='water.log'.")
        data = source
    else:
        path = Path(source)
        data = path.read_bytes()
        name = name if name is not None else path.name
    raw = json.loads(_native.qc_read(name, data))
    for message in raw["warnings"]:
        warnings.warn(message, UserWarning, stacklevel=2)
    jobs = tuple(
        Job(
            index=job["index"],
            kind=job["kind"],
            method=job["method"],
            basis=job["basis"],
            keywords=tuple(job["keywords"]),
            completion=job["completion"],
            completion_message=job["completion_message"],
            steps=tuple(
                Step(_geometry_from_data(step["geometry"]), step["energy"], step["label"])
                for step in job["steps"]
            ),
            result=_result_from_data(job["result"]) if job["result"] is not None else None,
        )
        for job in raw["jobs"]
    )
    return QcFile(
        raw["format"],
        raw["program"],
        raw["program_version"],
        jobs,
        tuple(raw["warnings"]),
        raw["default_job"],
    )


def write_input(
    path: str | PathLike[str],
    geometry: Geometry,
    *,
    program: str | None = None,
    job: str = "opt-freq",
    method: str = "B3LYP-D3BJ",
    basis: str = "def2-SVP",
    cores: int = 8,
    memory_gb: int = 16,
    title: str = "",
    extra: str = "",
    hessian: Result | None = None,
    reactant: Geometry | None = None,
    product: Geometry | None = None,
) -> list[Path]:
    """Write an input and its companion files in the same directory and return their paths.

    ``qc.write_input("water.gjf", water)`` makes a Gaussian optimization and
    frequency input. ``.com`` also selects Gaussian, and ``.xyz`` selects XYZ.
    For ``.inp``, choose ``program="orca"`` or ``program="gamess"``. Program
    names are case-insensitive. GAMESS supports STO-3G, 6-31G, and 6-31G(d).

    Jobs include ``single-point``, ``optimize``, ``opt-freq``, ``transition-state``,
    ``qst2``, ``qst3``, ``irc`` and ``neb-ts``, where the program supports them.
    A ``hessian`` result supplies an ORCA starting Hessian for a TS/IRC job.
    ``reactant`` and ``product`` supply the endpoints for QST2/QST3/NEB-TS.
    Writer warnings are emitted with ``warnings.warn``.
    """
    path = Path(path)
    if program is None:
        program = {".gjf": "gaussian", ".com": "gaussian", ".xyz": "xyz"}.get(path.suffix.lower())
        if program is None:
            raise _invalid("Choose program='orca' or program='gamess' for .inp files.")
    program = program.lower()
    if program not in {"gaussian", "orca", "gamess", "xyz"}:
        raise _invalid(f"Unknown calculation program: '{program}'.")
    cores = _integer(cores, "Cores")
    memory_gb = _integer(memory_gb, "Memory in GB")
    if not 1 <= cores < 2**32 or not 1 <= memory_gb < 2**32:
        raise _invalid("Cores and memory must be positive integers below 4294967296.")
    hessian_data = None
    if hessian is not None:
        hessian_data = {
            "geometry": _geometry_data(hessian.geometry),
            "hessian": hessian.hessian.ravel().tolist() if hessian.hessian is not None else None,
            "method": hessian.method,
            "frequencies": hessian.frequencies.tolist()
            if hessian.frequencies is not None
            else None,
        }
    options = {
        "geometry": _geometry_data(geometry),
        "program": program,
        "job": job,
        "method": method,
        "basis": basis,
        "cores": cores,
        "memory_gb": memory_gb,
        "title": title or geometry.title,
        "extra": extra,
        "stem": path.stem,
        "hessian": hessian_data,
        "reactant": _geometry_data(reactant) if reactant is not None else None,
        "product": _geometry_data(product) if product is not None else None,
    }
    output = json.loads(_native.qc_write_input(json.dumps(options, allow_nan=False)))
    for message in output["warnings"]:
        warnings.warn(message, UserWarning, stacklevel=2)
    written = []
    for index, file in enumerate(output["files"]):
        target = path if index == 0 else path.parent / file["name"]
        target.write_text(file["text"], encoding="utf-8")
        written.append(target)
    return written
