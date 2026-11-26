"""The optional xTB worker, tested through the installed Python packages."""

import _thread
import json
import sys
from pathlib import Path
from threading import Event, Thread

import numpy as np
import pytest
import reshiki
from reshiki import qc, xtb

FIXTURES = Path(__file__).resolve().parents[3] / "tests/fixtures/xtb"


def geometry(name: str) -> qc.Geometry:
    return qc.Geometry.from_xyz((FIXTURES / f"{name}.xyz").read_text())


@pytest.fixture
def worker():
    return pytest.importorskip("reshiki_xtb", reason="the optional reshiki-xtb wheel is missing")


@pytest.mark.skipif(sys.platform == "win32", reason="Windows has a separate availability message")
def test_missing_engine(monkeypatch):
    monkeypatch.setitem(sys.modules, "reshiki_xtb", None)
    with pytest.raises(xtb.XtbNotInstalled, match=r'pip install "reshiki\[xtb\]"') as caught:
        xtb.energy(geometry("water"))
    assert caught.value.code == "xtb_not_installed"
    assert not xtb.available()


def test_windows_availability(monkeypatch):
    water = geometry("water")
    monkeypatch.setattr(sys, "platform", "win32")
    with pytest.raises(xtb.XtbNotInstalled, match="not available on Windows"):
        xtb.energy(water)
    assert not xtb.available()


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
    heavy = xtb.frequencies(heavy_water)
    np.testing.assert_array_equal(heavy.geometry.isotopes, [0, 2, 2])
    assert light.frequencies.shape == heavy.frequencies.shape == (3,)
    assert np.all(heavy.frequencies < light.frequencies)
    np.testing.assert_allclose(light.frequencies[1:] / heavy.frequencies[1:], np.sqrt(2), rtol=0.06)


def test_cef3_can_set_valence_spin_independently(worker):
    cef3 = qc.Geometry.from_symbols(
        ["Ce", "F", "F", "F"], [[0, 0, 0], [2, 0, 0], [-2, 0, 0], [0, 2, 0]]
    )
    assert cef3.multiplicity == 2
    with pytest.raises(xtb.XtbError, match=r"valence electron count.*unpaired="):
        xtb.energy(cef3)
    result = xtb.energy(cef3, unpaired=0)
    assert np.isfinite(result.energy)
    assert result.geometry.multiplicity == 2


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


def test_keyboard_interrupt(worker):
    caffeine = geometry("caffeine")
    stop = Event()

    def interrupt():
        # Keep interrupting until the call has raised, so a fast machine cannot
        # finish the calculation between one interrupt and the next check.
        while not stop.wait(0.01):
            _thread.interrupt_main()

    thread = Thread(target=interrupt)
    with pytest.raises(KeyboardInterrupt):
        thread.start()
        try:
            xtb.frequencies(caffeine)
        finally:
            stop.set()
            thread.join()
    assert xtb.energy(geometry("water")).energy is not None
