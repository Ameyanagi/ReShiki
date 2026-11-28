"""The optional xTB worker, tested through the installed Python packages."""

import json
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest
import reshiki
from reshiki import ReShikiError, qc, xtb

FIXTURES = Path(__file__).resolve().parents[3] / "tests/fixtures/xtb"


def geometry(name: str) -> qc.Geometry:
    return qc.Geometry.from_xyz((FIXTURES / f"{name}.xyz").read_text())


@pytest.fixture
def worker():
    return pytest.importorskip("reshiki_xtb", reason="the optional reshiki-xtb wheel is missing")


def test_missing_engine(monkeypatch):
    monkeypatch.setitem(sys.modules, "reshiki_xtb", None)
    with pytest.raises(xtb.XtbNotInstalled, match=r'pip install "reshiki\[xtb\]"') as caught:
        xtb.energy(geometry("water"))
    assert caught.value.code == "xtb_not_installed"
    with pytest.raises(xtb.XtbNotInstalled, match=r'pip install "reshiki\[xtb\]"'):
        xtb.conformers("CCCC")
    assert not xtb.available()


def test_windows_uses_the_installed_engine_worker(monkeypatch):
    monkeypatch.setattr(sys, "platform", "win32")
    executable = Path("C:/Program Files/ReShiki/reshiki-xtb-worker.exe")
    installed = SimpleNamespace(__version__=reshiki.__version__, worker_path=lambda: executable)
    monkeypatch.setitem(sys.modules, "reshiki_xtb", installed)
    assert xtb._worker_path() == str(executable)


def test_unknown_method(monkeypatch, tmp_path):
    helium = qc.Geometry.from_symbols(["He"], [[0, 0, 0]])
    monkeypatch.setattr(xtb, "_worker_path", lambda: str(tmp_path / "unused-worker"))
    with pytest.raises(ReShikiError, match="Unknown xTB method") as caught:
        xtb.energy(helium, method="unknown")
    assert caught.value.code == "invalid_arguments"
    assert not isinstance(caught.value, xtb.XtbError)


@pytest.mark.parametrize("conventions", ["axtb", "xtb-6.8"])
@pytest.mark.parametrize("molecule", ["water", "caffeine", "ferrocene"])
def test_gfn2_reference(worker, molecule, conventions):
    reference = json.loads((FIXTURES / "reference.json").read_text())[conventions][molecule]
    result = xtb.energy(
        geometry(molecule), method="gfn2-XTB", gradient=True, conventions=conventions
    )
    assert result.method == "GFN2-xTB"
    assert result.program == "axtb " + xtb.engine()["version"]
    assert result.energy == pytest.approx(reference["energy"], rel=0, abs=1e-8)
    np.testing.assert_allclose(result.gradient, reference["gradient"], rtol=0, atol=1e-7)


def test_worker_version(worker):
    assert xtb.available()
    engine = xtb.engine()
    assert set(engine) == {"name", "version", "license", "source", "rev"}
    assert engine["name"] == "axtb"
    assert engine["license"] == "LGPL-3.0-or-later"


def test_water_optimization_and_frequencies(worker):
    water = geometry("water")
    result = xtb.optimize(water)
    assert result.converged
    assert result.steps > 0
    assert result.energy < xtb.energy(water).energy
    assert result.energies.shape == (result.steps + 1,)
    assert result.trajectory.shape == (result.steps + 1, len(water), 3)
    np.testing.assert_allclose(result.trajectory[0], water.positions)
    np.testing.assert_array_equal(result.trajectory[-1], result.geometry.positions)
    assert result.energies[-1] == result.energy
    assert result.gradient.shape == (3, 3)
    assert result.bond_orders
    assert result.orbital_energies.shape == result.occupations.shape

    modes = xtb.frequencies(result.geometry)
    assert modes.hessian.shape == (9, 9)
    assert modes.frequencies.shape == (3,)
    assert np.all(modes.frequencies > 0)
    assert modes.ir_intensities.shape == (3,)
    assert modes.normal_modes.shape == (3, 3, 3)
    assert modes.thermo is not None
    assert modes.thermo.temperature == 298.15
    assert modes.thermo.pressure == 101325.0


def test_deuterated_water_lowers_frequencies(worker):
    water = xtb.optimize(geometry("water")).geometry
    heavy_water = qc.Geometry.from_symbols(water.symbols, water.positions, isotopes=[0, 2, 2])
    light = xtb.frequencies(water)
    heavy = xtb.frequencies(heavy_water, symmetry_number=2)
    np.testing.assert_array_equal(heavy.geometry.isotopes, [0, 2, 2])
    assert light.frequencies.shape == heavy.frequencies.shape == (3,)
    assert np.all(heavy.frequencies < light.frequencies)
    np.testing.assert_allclose(light.frequencies[1:] / heavy.frequencies[1:], np.sqrt(2), rtol=0.06)


def test_isotopologue_thermochemistry_symmetry(worker):
    water = xtb.optimize(geometry("water")).geometry
    d2o = qc.Geometry.from_symbols(water.symbols, water.positions, isotopes=[0, 2, 2])
    hdo = qc.Geometry.from_symbols(water.symbols, water.positions, isotopes=[0, 2, 0])
    with pytest.raises(ReShikiError, match="symmetry_number") as caught:
        xtb.frequencies(d2o)
    assert caught.value.code == "invalid_arguments"
    symmetric = xtb.frequencies(d2o, symmetry_number=2).thermo
    asymmetric = xtb.frequencies(hdo, symmetry_number=1).thermo
    assert symmetric is not None and asymmetric is not None
    assert symmetric.free_energy is not None and asymmetric.free_energy is not None
    assert symmetric.free_energy != pytest.approx(asymmetric.free_energy, rel=0, abs=1e-6)
    d2o_without_symmetry = xtb.frequencies(d2o, symmetry_number=1).thermo
    assert d2o_without_symmetry is not None and d2o_without_symmetry.free_energy is not None
    assert symmetric.free_energy > d2o_without_symmetry.free_energy + 1e-4


def test_cef3_can_set_valence_spin_independently(worker):
    cef3 = qc.Geometry.from_symbols(
        ["Ce", "F", "F", "F"], [[0, 0, 0], [2, 0, 0], [-2, 0, 0], [0, 2, 0]]
    )
    assert cef3.multiplicity == 2
    with pytest.raises(
        xtb.XtbError, match=r"xTB input: .*valence electron count and cannot exceed it.*unpaired="
    ):
        xtb.energy(cef3)
    with pytest.raises(
        xtb.XtbError, match=r"xTB input: .*valence electron count and cannot exceed it"
    ):
        xtb.energy(cef3, unpaired=100)
    result = xtb.energy(cef3, unpaired=0)
    assert np.isfinite(result.energy)
    assert result.geometry.multiplicity == 2
    assert result.unpaired == 0


def test_frequencies_preserve_the_worker_limit_error(worker):
    atoms = qc.Geometry.from_symbols(["He"] * 201, [[3.0 * i, 0.0, 0.0] for i in range(201)])
    with pytest.raises(
        xtb.XtbError, match=r"xTB resource limit: expected at most 200 Hessian atoms"
    ):
        xtb.frequencies(atoms)


def test_worker_gap_is_reported_without_requesting_orbitals(worker):
    water = geometry("water")
    plain = xtb.energy(water)
    with_orbitals = xtb.energy(water, orbitals=True)
    assert plain.orbital_energies is None
    assert with_orbitals.orbital_energies is not None
    assert plain.homo_lumo_gap > 0
    assert plain.homo_lumo_gap == with_orbitals.homo_lumo_gap
    assert plain.near_degenerate == with_orbitals.near_degenerate


def test_optimization_step_limit(worker):
    water = geometry("water")
    with pytest.warns(UserWarning, match="step limit"):
        result = xtb.optimize(water, max_steps=1, fixed_atoms=[0])
    assert not result.converged
    assert result.steps == 1
    assert result.warnings
    np.testing.assert_allclose(result.geometry.positions[0], water.positions[0])


def test_version_mismatch(worker, monkeypatch):
    monkeypatch.setattr(worker, "__version__", "0.0.0")
    with pytest.raises(xtb.XtbError, match=r"needs reshiki-xtb .*found 0\.0\.0") as caught:
        xtb.energy(geometry("water"))
    assert caught.value.code == "failed"
    assert f'pip install "reshiki[xtb]=={reshiki.__version__}"' in str(caught.value)
    assert not xtb.available()


@pytest.mark.skipif(sys.platform == "win32", reason="The xTB worker is unavailable on Windows")
def test_missing_worker_keeps_os_error(monkeypatch, tmp_path):
    monkeypatch.setitem(
        sys.modules,
        "reshiki_xtb",
        SimpleNamespace(
            __version__=reshiki.__version__, worker_path=lambda: tmp_path / "missing-worker"
        ),
    )
    with pytest.raises(xtb.XtbError, match=r"Could not start bounded xTB worker: .*\(os error 2\)"):
        xtb.energy(geometry("water"))


_INTERRUPT_SCRIPT = """
import _thread, signal, sys, threading, time
import numpy as np
from reshiki import qc, xtb

# A shell's background job starts with SIGINT ignored, and then interrupt_main()
# does nothing; Ctrl+C in a terminal arrives with the default handler.
signal.signal(signal.SIGINT, signal.default_int_handler)

caffeine = qc.Geometry.from_xyz(open(sys.argv[1]).read())
distorted = qc.Geometry(
    caffeine.numbers,
    caffeine.positions + np.random.default_rng(1).normal(0, 0.15, caffeine.positions.shape),
)
start = time.perf_counter()
try:
    threading.Timer(0.05, _thread.interrupt_main).start()
    xtb.optimize(distorted, level="tight")
    print("finished")
except KeyboardInterrupt:
    print("interrupted", time.perf_counter() - start)
print("energy", xtb.energy(caffeine).energy)
"""


def test_keyboard_interrupt(worker):
    # In its own interpreter: the interrupt must not depend on what other tests
    # left in this process, and a failure cannot hang the suite. The distorted
    # caffeine takes dozens of optimizer steps, several 100 ms signal checks.
    done = subprocess.run(
        [sys.executable, "-c", _INTERRUPT_SCRIPT, str(FIXTURES / "caffeine.xyz")],
        capture_output=True,
        text=True,
        timeout=120,
        check=True,
    )
    lines = done.stdout.splitlines()
    assert lines[0].startswith("interrupted"), done.stdout + done.stderr
    assert lines[1].startswith("energy -42.")


def test_butane_conformers(worker):
    result = xtb.conformers("CCCC")
    repeated = xtb.conformers("CCCC")
    np.testing.assert_equal(asdict(result), asdict(repeated))
    assert result.conformers
    energies = [conformer.energy for conformer in result.conformers]
    assert energies == sorted(energies)
    assert sum(conformer.population for conformer in result.conformers) == pytest.approx(
        1.0, rel=0, abs=1e-9
    )
    lowest = result.conformers[0]
    assert result.best.energy == lowest.energy
    assert lowest.rank == 1
    assert lowest.delta == lowest.rmsd == 0.0
    np.testing.assert_array_equal(result.best.geometry.positions, lowest.geometry.positions)
    lines = result.to_xyz().splitlines()
    frame_length = len(lowest.geometry) + 2
    assert len(lines) == len(result.conformers) * frame_length
    for conformer, comment in zip(result.conformers, lines[1::frame_length], strict=True):
        assert f"conformer {conformer.rank} · ΔE {conformer.delta:.2f} kcal/mol" in comment
        assert comment.endswith(f"E = {conformer.energy:.10f} Eh")


def test_borane_conformers_use_uff_for_the_first_model(worker):
    result = xtb.conformers("B", minimizer="uff", starts=1)
    assert result.conformers
    assert result.method.startswith("UFF ")

    # MMFF has no boron parameters, so generation falls back to UFF and says so.
    fallback = xtb.conformers("B", starts=1)
    assert fallback.conformers
    assert fallback.method.startswith("UFF ")


def test_lysine_conformers_preserve_stereo(worker):
    lysine = reshiki.Molecule.from_smiles("NCCCC[C@H](N)C(=O)O")
    result = xtb.conformers(lysine)
    assert len(result.conformers) > 1
    assert all(conformer.stereo is True for conformer in result.conformers)


def test_conformers_refresh_stale_model_in_native_bytes(worker):
    path = FIXTURES.parent / "models3d" / "model-frame-methanol.rsk"
    drawing = json.loads(path.read_bytes())
    # A chemistry edit saves the old methanol geometry with its original fingerprint.
    drawing["version"] = 33
    model = drawing["models"][0]
    model["fingerprint"] = "27f09dccaae6f1e0"
    model["force_field"] = "uff"
    model["seed"] = 91_337
    model["linked_atoms"] = [1, 2]
    model["generated"] = {"sources": [1, 2], "elements": ["C", "O"], "bonds": [[0, 1]]}
    for atom in model["atoms"]:
        atom["id"] -= 1
    for hydrogen in model["hydrogens"]:
        hydrogen["parent"] -= 1
    drawing["atoms"][1]["element"] = "C"

    result = xtb.conformers(json.dumps(drawing).encode(), starts=1)
    np.testing.assert_array_equal(np.sort(result.best.geometry.numbers), [1] * 6 + [6] * 2)


@pytest.mark.parametrize("threads", [None, 1, 8])
def test_conformers_accepts_auto_and_search_thread_limits(worker, threads):
    assert xtb.conformers("CCCC", starts=1, threads=threads).conformers


@pytest.mark.parametrize(
    "options",
    [
        {"starts": 0},
        {"starts": 301},
        {"window": 0.09},
        {"window": 100.1},
        {"representatives": 0},
        {"representatives": 61},
        {"per_cluster": 2},
        {"minimizer": "unknown"},
        {"diversity": "unknown"},
        {"accuracy": 1001.0},
        {"electronic_temperature": -1.0},
        {"max_iterations": 10001},
        {"threads": 0},
        {"threads": 9},
    ],
)
def test_conformer_argument_validation(worker, options):
    with pytest.raises(reshiki.ReShikiError) as caught:
        xtb.conformers("CCCC", **options)
    assert caught.value.code == "invalid_arguments"


@pytest.mark.parametrize("value", [float("nan"), float("inf")])
@pytest.mark.parametrize("option", ["window", "accuracy", "electronic_temperature"])
def test_conformers_reject_nonfinite_options(worker, option, value):
    with pytest.raises(ReShikiError) as caught:
        xtb.conformers("CCCC", **{option: value})
    assert caught.value.code == "invalid_arguments"


@pytest.mark.parametrize(
    ("limit", "minimizer"),
    [(200, "mmff94s"), (120, "gfn2")],
)
def test_conformer_atom_limits(worker, limit, minimizer):
    with pytest.raises(reshiki.ReShikiError, match=f"up to {limit} atoms") as caught:
        xtb.conformers("C" * (limit + 1), minimizer=minimizer)
    assert caught.value.code == "invalid_arguments"


@pytest.mark.parametrize(
    ("carbons", "limit", "minimizer"),
    [(67, 200, "mmff94s"), (40, 120, "gfn2")],
)
def test_conformer_atom_limits_include_hydrogens(worker, carbons, limit, minimizer):
    # The alkane has 2n + 2 implicit hydrogens and n drawn carbons.
    assert carbons <= limit < 3 * carbons + 2
    with pytest.raises(ReShikiError, match=f"up to {limit} atoms") as caught:
        xtb.conformers("C" * carbons, starts=1, minimizer=minimizer)
    assert caught.value.code == "invalid_arguments"
