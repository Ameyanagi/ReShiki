from __future__ import annotations

import warnings
from dataclasses import FrozenInstanceError
from pathlib import Path

import numpy as np
import pytest
from reshiki import ReShikiError, qc


@pytest.fixture
def water() -> qc.Geometry:
    return qc.Geometry.from_symbols(
        ["O", "H", "H"], [[0, 0, 0], [0, 0, 0.96], [0.92, 0, -0.24]], title="water"
    )


@pytest.mark.parametrize(
    ("filename", "format_name", "program", "energy", "atoms", "frequencies"),
    [
        ("g16_water_mp2.log", "Gaussian log", "Gaussian", -75.0022821, 3, 0),
        ("orca60_dvb_ir.out", "ORCA output", "ORCA", -382.055107107616, 20, 54),
        ("molden5.7_dvb_ir.molden", "Molden", None, None, 20, 54),
        ("g16_dvb_ir.fchk", "Gaussian fchk", "Gaussian", -382.3082666020143, 20, 54),
    ],
)
def test_read_reader_fixture_values(
    fixtures: Path,
    filename: str,
    format_name: str,
    program: str | None,
    energy: float | None,
    atoms: int,
    frequencies: int,
) -> None:
    # Values are quoted by the corresponding Rust reader tests, before display rounding.
    path = fixtures / "qc" / "cclib" / filename
    for source, options in [(path, {}), (path.read_bytes(), {"name": filename})]:
        file = qc.read(source, **options)
        assert file.format == format_name
        assert file.program == program
        assert len(file.jobs) == 1
        assert file.default_job is file.jobs[0]
        job = file.default_job
        assert job.geometry is not None
        assert len(job.geometry) == atoms
        result = job.result
        assert result is not None
        assert result.unpaired is None
        assert len(result.geometry) == atoms
        assert result.energy == energy
        assert (len(result.frequencies) if result.frequencies is not None else 0) == frequencies
        if frequencies:
            assert result.normal_modes is not None
            assert result.normal_modes.shape == (frequencies, atoms, 3)
            assert result.ir_intensities is not None
            assert result.ir_intensities.shape == (frequencies,)
            np.testing.assert_allclose(
                np.linalg.norm(result.normal_modes.reshape(frequencies, -1), axis=1),
                1,
                atol=1e-5,
            )


def test_imported_units_precision_and_read_only_arrays(fixtures: Path) -> None:
    gaussian = qc.read(fixtures / "qc/cclib/g16_water_mp2.log").default_job
    assert gaussian is not None and gaussian.result is not None
    assert gaussian.kind == "single-point"
    assert gaussian.completion == "normal"
    assert gaussian.result.program is not None
    assert gaussian.result.program.startswith("Gaussian ")
    np.testing.assert_allclose(gaussian.result.dipole, [0, 0, -1.5994], atol=1e-12)

    file = qc.read(fixtures / "qc/cclib/g09_dvb_ir.fchk")
    assert file.default_job is not None and file.default_job.result is not None
    result = file.default_job.result
    assert result.gradient is not None and result.hessian is not None
    assert result.gradient.shape == (20, 3)
    assert result.hessian.shape == (60, 60)
    assert result.gradient[0, 0] == -3.72986025e-5
    assert result.hessian[0, 0] == 0.726306291
    assert result.hessian[0, 1] == result.hessian[1, 0] == -0.00350839215
    with pytest.raises(ValueError, match="read-only"):
        result.gradient[0, 0] = 1


def test_unrestricted_orbitals(fixtures: Path) -> None:
    file = qc.read(fixtures / "qc/cclib/g09_dvb_un_sp.fchk")
    assert file.default_job is not None and file.default_job.result is not None
    result = file.default_job.result
    assert result.geometry.charge == 1 and result.geometry.multiplicity == 2
    assert result.occupations is not None and result.beta_occupations is not None
    assert result.orbital_energies is not None and result.beta_orbital_energies is not None
    assert result.orbital_energies.shape == result.beta_orbital_energies.shape == (60,)
    assert result.occupations.sum() == 35
    assert result.beta_occupations.sum() == 34
    homo = max(result.orbital_energies[34], result.beta_orbital_energies[33])
    lumo = min(result.orbital_energies[35], result.beta_orbital_energies[34])
    assert result.homo_lumo_gap == pytest.approx((lumo - homo) * 27.211386245988)


def test_fractional_occupations_preserve_electron_count_gap(fixtures: Path) -> None:
    path = fixtures / "qc/cclib/molden5.7_water_mp2.molden"
    original = qc.read(path).default_job.result
    occupied, virtual = path.read_text().rsplit("Occup=   2.000000", 1)
    smeared = (
        occupied
        + "Occup=   1.990000"
        + virtual.replace("Occup=   0.000000", "Occup=   0.010000", 1)
    )
    with pytest.warns(UserWarning, match="restricted MO occupations"):
        result = qc.read(smeared.encode(), name=path.name).default_job.result
    np.testing.assert_array_equal(result.orbital_energies, original.orbital_energies)
    assert result.occupations.sum() == original.occupations.sum() == 10
    np.testing.assert_array_equal(result.occupations[4:6], [1.99, 0.01])
    assert original.homo_lumo_gap == pytest.approx(26.093, abs=0.001)
    assert result.homo_lumo_gap == original.homo_lumo_gap
    assert not result.near_degenerate

    ambiguous = smeared.replace("Occup=   0.010000", "Occup=   0.020000", 1)
    with pytest.warns(UserWarning, match="Charge and spin are not recorded"):
        result = qc.read(ambiguous.encode(), name=path.name).default_job.result
    assert result.homo_lumo_gap is None
    assert not result.near_degenerate


def test_inverted_occupations_leave_gap_unset(fixtures: Path) -> None:
    path = fixtures / "qc/cclib/molden5.7_water_mp2.molden"
    occupied, virtual = path.read_text().rsplit("Occup=   2.000000", 1)
    inverted = (
        occupied
        + "Occup=   0.010000"
        + virtual.replace("Occup=   0.000000", "Occup=   1.990000", 1)
    )
    with pytest.warns(UserWarning, match="restricted MO occupations"):
        result = qc.read(inverted.encode(), name=path.name).default_job.result
    assert result.occupations.sum() == 10
    np.testing.assert_array_equal(result.occupations[4:6], [0.01, 1.99])
    assert result.homo_lumo_gap is None
    assert not result.near_degenerate


def test_default_job_and_result_own_geometry(fixtures: Path) -> None:
    file = qc.read(fixtures / "qc/pymatgen/CH2O_CO2.log")
    assert len(file.jobs) == 3
    assert file.default_job is file.jobs[2]
    for job in file.jobs:
        assert job.result is not None
        assert any(
            np.array_equal(job.result.geometry.positions, step.geometry.positions)
            for step in job.steps
        )
    assert file.jobs[0].geometry is not None
    assert file.jobs[0].geometry.symbols == ("C", "H", "H", "O")
    assert file.default_job.geometry is not None
    assert file.default_job.geometry.symbols == ("C", "O", "O")


def test_geometry_copies_arrays_and_resolves_spin(water: qc.Geometry) -> None:
    assert water.numbers.dtype == np.int64
    assert water.positions.dtype == np.float64
    assert water.symbols == ("O", "H", "H")
    assert len(water) == 3
    assert water.multiplicity == 1
    assert qc.Geometry.from_symbols([8, "h"], [[0, 0, 0], [0, 0, 1]]).multiplicity == 2
    assert qc.Geometry.from_symbols(["O", "H"], [[0, 0, 0], [0, 0, 1]], charge=-1).multiplicity == 1
    numbers = np.array([2], dtype=np.int64)
    positions = np.zeros((1, 3))
    geometry = qc.Geometry(numbers, positions)
    numbers[0] = 8
    positions[0, 0] = 10
    assert geometry.numbers[0] == 2 and geometry.positions[0, 0] == 0
    with pytest.raises(ValueError, match="read-only"):
        water.numbers[0] = 1
    with pytest.raises(ValueError, match="read-only"):
        water.positions[0, 0] = 1
    with pytest.raises(FrozenInstanceError):
        water.charge = 2  # type: ignore[misc]


def test_geometry_isotopes_are_copied_and_normalized(water: qc.Geometry) -> None:
    assert water.isotopes is None
    for natural in [[], [0, 0, 0]]:
        assert (
            qc.Geometry(water.numbers, water.positions, isotopes=np.array(natural)).isotopes is None
        )
    source = np.array([0, 2, 0])
    labeled = qc.Geometry(water.numbers, water.positions, isotopes=source)
    source[1] = 3
    np.testing.assert_array_equal(labeled.isotopes, [0, 2, 0])
    with pytest.raises(ValueError, match="read-only"):
        labeled.isotopes[1] = 1


@pytest.mark.parametrize(
    ("numbers", "positions", "options", "message"),
    [
        ([], [], {}, "nonempty"),
        ([[1]], [[0, 0, 0]], {}, "shape"),
        ([0], [[0, 0, 0]], {}, "1 to 118"),
        ([119], [[0, 0, 0]], {}, "1 to 118"),
        ([1.5], [[0, 0, 0]], {}, "integers"),
        ([1], [[0, 0]], {}, "shape"),
        ([1], [[0, 0, np.nan]], {}, "finite"),
        ([1], [[0, 0, np.inf]], {}, "finite"),
        ([1], [[0, 0, 0]], {"multiplicity": 1}, "electrons"),
        ([1], [[0, 0, 0]], {"multiplicity": 0}, "electrons"),
        ([1], [[0, 0, 0]], {"multiplicity": 4}, "electrons"),
        ([1], [[0, 0, 0]], {"charge": 2}, "electrons"),
        ([1], [[0, 0, 0]], {"charge": 0.5}, "integer"),
        ([1], [[0, 0, 0]], {"multiplicity": 2.5}, "integer"),
        ([1], [[0, 0, 0]], {"isotopes": [2, 3]}, "one mass number"),
        ([1], [[0, 0, 0]], {"isotopes": [2.5]}, "Isotope mass numbers must be integers"),
        ([1], [[0, 0, 0]], {"isotopes": [-1]}, "Isotope mass numbers must be integers"),
    ],
)
def test_geometry_validation(numbers, positions, options, message: str) -> None:
    with pytest.raises(ReShikiError, match=message) as raised:
        qc.Geometry(np.asarray(numbers), np.asarray(positions), **options)
    assert raised.value.code == "invalid_arguments"


def test_unknown_element() -> None:
    with pytest.raises(ReShikiError, match="Unknown element"):
        qc.Geometry.from_symbols(["Xx"], [[0, 0, 0]])


def test_xyz_round_trip_and_first_frame(water: qc.Geometry) -> None:
    restored = qc.Geometry.from_xyz(water.to_xyz() + "this second frame is ignored")
    assert restored.title == "water"
    assert restored.symbols == water.symbols
    np.testing.assert_array_equal(restored.positions, water.positions)
    precise = qc.Geometry.from_symbols(["He"], [[0.12345678901234567, -1.2345678901234567, 0]])
    np.testing.assert_array_equal(
        qc.Geometry.from_xyz(precise.to_xyz()).positions, precise.positions
    )
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        atom = qc.Geometry.from_xyz("1\noxygen ion\nO 0 0 0\n", charge=1, multiplicity=2)
    assert atom.charge == 1 and atom.multiplicity == 2
    with pytest.warns(UserWarning, match="Charge and spin are not recorded in XYZ"):
        file = qc.read(b"1\noxygen\nO 0 0 0\n", name="oxygen.xyz")
    assert file.default_job.geometry.charge == 0


@pytest.mark.parametrize(("title", "multiplicity"), [("charge=9", None), ("multiplicity=4", 1)])
def test_xyz_electronic_state_overrides_title(title: str, multiplicity: int | None) -> None:
    helium = qc.Geometry.from_symbols(["He"], [[0, 0, 0]], title=title)
    restored = qc.Geometry.from_xyz(helium.to_xyz(), charge=0, multiplicity=multiplicity)
    assert restored.charge == 0 and restored.multiplicity == 1
    assert restored.title == title


def test_isotopes_round_trip_through_xyz_and_gaussian(tmp_path: Path) -> None:
    geometry = qc.Geometry.from_xyz("3\nlabeled water\n18O 0 0 0\nD 0 0 1\nT 1 0 0\n")
    assert geometry.symbols == ("O", "H", "H")
    np.testing.assert_array_equal(geometry.isotopes, [18, 2, 3])
    np.testing.assert_array_equal(qc.Geometry.from_xyz(geometry.to_xyz()).isotopes, [18, 2, 3])

    xyz = tmp_path / "labeled.xyz"
    assert qc.write_input(xyz, geometry) == [xyz]
    with pytest.warns(UserWarning, match="Charge and spin are not recorded in XYZ"):
        np.testing.assert_array_equal(qc.read(xyz).default_job.geometry.isotopes, [18, 2, 3])

    gaussian = tmp_path / "labeled.gjf"
    assert qc.write_input(gaussian, geometry) == [gaussian]
    np.testing.assert_array_equal(qc.read(gaussian).default_job.geometry.isotopes, [18, 2, 3])


def test_bytes_need_name() -> None:
    with pytest.raises(ReShikiError, match="name="):
        qc.read(b"1\nhelium\nHe 0 0 0\n")


@pytest.mark.parametrize("extension", ["gjf", "com"])
def test_write_gaussian(tmp_path: Path, water: qc.Geometry, extension: str) -> None:
    path = tmp_path / f"water.{extension}"
    assert qc.write_input(path, water) == [path]
    text = path.read_text()
    assert "%nprocshared=8\n%mem=16GB\n%chk=water.chk" in text
    assert "#p B3LYP/Def2SVP EmpiricalDispersion=GD3BJ Opt Freq" in text
    assert "water\n\n0 1" in text
    file = qc.read(path)
    assert file.default_job is not None and file.default_job.geometry is not None
    np.testing.assert_allclose(file.default_job.geometry.positions, water.positions, atol=1e-8)


def test_write_orca(tmp_path: Path, water: qc.Geometry) -> None:
    path = tmp_path / "water.inp"
    assert qc.write_input(path, water, program="ORCA") == [path]
    text = path.read_text()
    assert "! B3LYP D3BJ def2-SVP Opt Freq" in text
    assert "%pal nprocs 8 end" in text
    assert "%maxcore 1536" in text
    assert "* xyz 0 1" in text
    file = qc.read(path)
    assert file.format == "ORCA input"


def test_write_gamess(tmp_path: Path, water: qc.Geometry) -> None:
    path = tmp_path / "water.inp"
    with pytest.warns(UserWarning, match="number of cores"):
        written = qc.write_input(path, water, program="GAMESS", basis="6-31G(d)")
    assert written == [path]
    text = path.read_text()
    assert "$CONTRL SCFTYP=RHF RUNTYP=OPTIMIZE" in text
    assert "GBASIS=N31 NGAUSS=6 NDFUNC=1" in text
    assert "HSSEND=.T." in text
    assert " $DATA\nwater\nC1" in text
    assert qc.read(path).format == "GAMESS input"


def test_write_xyz(tmp_path: Path, water: qc.Geometry) -> None:
    path = tmp_path / "water.xyz"
    assert qc.write_input(path, water) == [path]
    restored = qc.Geometry.from_xyz(path.read_text())
    assert restored.symbols == water.symbols
    np.testing.assert_allclose(restored.positions, water.positions, atol=1e-8)


def test_inp_requires_program(tmp_path: Path, water: qc.Geometry) -> None:
    with pytest.raises(ReShikiError, match="program='orca'.*program='gamess'") as raised:
        qc.write_input(tmp_path / "water.inp", water)
    assert raised.value.code == "invalid_arguments"


def test_orca_hessian_companion(tmp_path: Path, fixtures: Path) -> None:
    file = qc.read(fixtures / "qc/cclib/g09_dvb_ir.fchk")
    assert file.default_job is not None and file.default_job.result is not None
    result = file.default_job.result
    path = tmp_path / "transition.inp"
    written = qc.write_input(path, result.geometry, program="orca", job="ts", hessian=result)
    assert written == [path, tmp_path / "transition_xtb.hess"]
    assert 'InHessName "transition_xtb.hess"' in path.read_text()
    assert "$hessian\n60" in written[1].read_text()
    assert "$atoms\n20" in written[1].read_text()
    other = qc.Geometry(result.geometry.numbers, result.geometry.positions + 0.1)
    with pytest.raises(ReShikiError, match="another geometry"):
        qc.write_input(tmp_path / "wrong.inp", other, program="orca", job="ts", hessian=result)


def test_reader_warnings_are_retained(fixtures: Path) -> None:
    with pytest.warns(UserWarning, match="Orbital shapes left out") as captured:
        file = qc.read(fixtures / "qc/cclib/g09_C_bigbasis.fchk")
    assert tuple(str(warning.message) for warning in captured) == file.warnings
