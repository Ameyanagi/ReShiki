//! Fixture values below are quoted from the ORCA output, before any display rounding.
use super::*;
use crate::qcread::{
    read,
    tests::{fixture, open},
};
use reshiki_qcdata::Source;

const WATER6: &str = "cclib/orca60_water_mp2.out";
const WATER5: &str = "cclib/orca50_water_mp2.out";
const OPT: &str = "cclib/orca60_dvb_gopt.out";
const IR: &str = "cclib/orca60_dvb_ir.out";
const NMR: &str = "cclib/orca61_dvb_coupling_nmr.out";

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).expect("the fixture is UTF-8")
}

fn parsed(text: &str) -> QcFile {
    read("derived-orca.out", text.as_bytes()).expect("the derived fixture should parse")
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn water_mp2_orca_5_and_6_keep_printed_units_and_levels() {
    // v6: "Program Version 6.0.1", "FINAL SINGLE POINT ENERGY -74.999374598107";
    // v5: "Program Version 5.0.0", "FINAL SINGLE POINT ENERGY -74.999373814715".
    for (name, version, energy, correlation, dipole_x) in [
        (WATER6, "6.0.1", -74.999374598107, -0.035800358, 0.150239405),
        (WATER5, "5.0.0", -74.999373814715, -0.035799716, 0.15024),
    ] {
        let file = open(name);
        assert_eq!(file.program.as_deref(), Some("ORCA"));
        assert_eq!(file.program_version.as_deref(), Some(version));
        assert_eq!(file.jobs.len(), 1);
        assert_eq!(file.default_job(), Some(0));
        let job = &file.jobs[0];
        assert_eq!(job.index, 0);
        assert_eq!(job.kind, JobKind::SinglePoint);
        assert_eq!(job.method.as_deref(), Some("MP2"));
        assert!(
            job.basis
                .as_ref()
                .is_some_and(|basis| basis.eq_ignore_ascii_case("sto-3g"))
        );
        assert_eq!(job.completion, Some(Completion::Normal));
        assert_eq!(job.steps.len(), 1);
        let frame = &job.steps[0].frame;
        assert_eq!(frame.numbers, [8, 1, 1]);
        assert_eq!((frame.charge, frame.multiplicity), (0, 1));
        // Both versions print "O 1.784914 1.262422 0.511985" in ANGSTROEM.
        assert_eq!(frame.positions_angstrom[0], [1.784914, 1.262422, 0.511985]);
        let result = job.result.as_ref().expect("energy result");
        assert_eq!(result.frame, frame.hash());
        assert_eq!(result.task, Task::Energy);
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.energy_hartree, Some(energy));
        close(
            result.energy_terms["mp2 correlation energy"],
            correlation,
            1e-12,
        );
        close(
            result.dipole_e_bohr.expect("atomic-unit dipole")[0],
            dipole_x,
            1e-12,
        );
        assert_eq!(result.charges.len(), 1);
        assert_eq!(result.charges[0].scheme, ChargeScheme::Mulliken);
        // "0 O : -0.361722" (v6), "0 O : -0.361723" (v5).
        close(
            result.charges[0].values[0],
            if name == WATER6 { -0.361722 } else { -0.361723 },
            1e-12,
        );
        let orbitals = result.orbitals.as_ref().expect("orbital levels");
        assert_eq!(orbitals.alpha.energies_hartree.len(), 7);
        assert_eq!(orbitals.alpha.occupations, [2., 2., 2., 2., 2., 0., 0.]);
        // "0 2.0000 -20.242269 -550.8201" in the v6 orbital table.
        close(
            orbitals.alpha.energies_hartree[0],
            if name == WATER6 {
                -20.242269
            } else {
                -20.242233
            },
            1e-12,
        );
        assert_eq!(orbitals.beta, Beta::Restricted);
        assert!(orbitals.basis.is_none());
        assert!(orbitals.core_electrons.is_empty());
        assert!(orbitals.alpha.rows.is_empty());
        assert!(orbitals.alpha.coefficients.is_empty());
        assert!(result.vibrations.is_none());
        assert!(result.thermo.is_none());
        assert!(matches!(
            result.provenance.source,
            Source::Imported { job: 0, .. }
        ));
    }
}

#[test]
fn optimization_keeps_changed_stationary_geometry_and_builds_trajectory() {
    let file = open(OPT);
    assert_eq!(file.program_version.as_deref(), Some("6.0.0"));
    let job = &file.jobs[0];
    assert_eq!(job.kind, JobKind::Optimization);
    assert_eq!(job.method.as_deref(), Some("b3lyp"));
    assert_eq!(job.basis.as_deref(), Some("sto-3g"));
    assert_eq!(job.keywords, ["rks", "b3lyp", "sto-3g", "usesym", "opt"]);
    assert_eq!(job.steps.len(), 4);
    // All four "FINAL SINGLE POINT ENERGY" records, including the stationary-point run.
    assert_eq!(
        job.steps
            .iter()
            .map(|step| step.energy_hartree)
            .collect::<Vec<_>>(),
        [
            Some(-382.055108617704),
            Some(-382.055126570567),
            Some(-382.055133025246),
            Some(-382.055133399486),
        ]
    );
    // Third: "C -1.417672 0.228332 0.000000"; stationary: "C -1.417709 0.228462 0.000000".
    assert_eq!(
        job.steps[2].frame.positions_angstrom[0],
        [-1.417672, 0.228332, 0.0]
    );
    assert_eq!(
        job.steps[3].frame.positions_angstrom[0],
        [-1.417709, 0.228462, 0.0]
    );
    let result = job.result.as_ref().expect("optimization result");
    assert_eq!(result.task, Task::Optimize);
    assert_eq!(result.status, Status::Converged);
    assert_eq!(result.frame, job.steps[3].frame.hash());
    assert_eq!(result.input_frame, Some(job.steps[0].frame.hash()));
    let trajectory = result.trajectory.as_ref().expect("optimization path");
    assert_eq!(trajectory.energies_hartree.len(), 4);
    assert_eq!(trajectory.positions_angstrom.len(), 4 * 20 * 3);
    assert_eq!(
        result
            .orbitals
            .as_ref()
            .expect("last orbital block")
            .alpha
            .energies_hartree
            .len(),
        46
    );
    // Final table: "0 2.0000 -10.016736 -272.5692".
    close(
        result
            .orbitals
            .as_ref()
            .expect("levels")
            .alpha
            .energies_hartree[0],
        -10.016736,
        1e-12,
    );
    close(
        result.energy_terms["nuclear repulsion"],
        445.79468306799504,
        1e-12,
    );
    assert_eq!(result.charges[0].values.len(), 20);
}

#[test]
fn ir_modes_are_cartesian_unit_vectors_and_intensities_are_km_per_mol() {
    let file = open(IR);
    let job = &file.jobs[0];
    assert_eq!(job.kind, JobKind::Frequencies);
    assert_eq!(job.steps.len(), 1);
    assert_eq!(job.steps[0].frame.numbers.len(), 20);
    let result = job.result.as_ref().expect("frequency result");
    assert_eq!(result.task, Task::Hessian);
    assert_eq!(result.energy_hartree, Some(-382.055107107616));
    assert_eq!(result.status, Status::Converged);
    let vibrations = result.vibrations.as_ref().expect("vibrations");
    // Modes 0..5: "0.00 cm**-1"; "6: 43.87 cm**-1"; "59: 3546.41 cm**-1".
    assert_eq!(vibrations.frequencies_cm1.len(), 54);
    assert_eq!(vibrations.frequencies_cm1[0], 43.87);
    assert_eq!(vibrations.frequencies_cm1[53], 3546.41);
    assert_eq!(vibrations.imaginary_count(), 0);
    assert_eq!(vibrations.modes.len(), 54 * 60);
    for mode in vibrations.modes.as_chunks::<60>().0 {
        close(mode.iter().map(|x| x * x).sum::<f64>(), 1.0, 1e-12);
    }
    // Cartesian row 2, mode 6: 0.078313, already mass-unweighted by ORCA.
    close(vibrations.modes[2], 0.078313, 1e-6);
    // ORCA 6 header: "Mode freq eps Int T**2 TX TY TZ"; Int has units km/mol.
    // "6: 43.87 0.000005 0.03 ...", "26: 980.51 0.007280 36.79 ...".
    assert_eq!(vibrations.ir_intensities_km_mol[0], 0.03);
    assert_eq!(vibrations.ir_intensities_km_mol[20], 36.79);
    assert_eq!(
        result
            .orbitals
            .as_ref()
            .expect("levels")
            .alpha
            .energies_hartree
            .len(),
        46
    );
    assert!(result.thermo.is_none());
}

#[test]
fn orca_61_nmr_keeps_the_underlying_energy_and_geometry() {
    let file = open(NMR);
    assert_eq!(file.program_version.as_deref(), Some("6.1.0"));
    let job = &file.jobs[0];
    // "! b3lyp sto-3g usesym sp" followed by "%eprnmr" in the INPUT FILE echo.
    assert_eq!(job.kind, JobKind::Other);
    assert_eq!(job.keywords, ["b3lyp", "sto-3g", "usesym", "sp"]);
    assert_eq!(job.steps.len(), 1);
    assert_eq!(job.steps[0].frame.numbers.len(), 20);
    let result = job.result.as_ref().expect("ground-state result");
    assert_eq!(result.task, Task::Energy);
    assert_eq!(result.status, Status::Converged);
    // "FINAL SINGLE POINT ENERGY -382.055109258623" and "0 C : -0.004142".
    assert_eq!(result.energy_hartree, Some(-382.055109258623));
    assert_eq!(result.charges[0].values[0], -0.004142);
    assert_eq!(
        result
            .orbitals
            .as_ref()
            .expect("levels")
            .alpha
            .energies_hartree
            .len(),
        46
    );
    assert!(result.vibrations.is_none());
}

fn joined_jobs(second: &str) -> String {
    // Real water MP2 output, with only its normal marker removed, then a framed ORCA job
    // separator and real dvb IR output. Their 3/20 atom counts expose cross-job leakage.
    let first = text(WATER6).replace("****ORCA TERMINATED NORMALLY****", "");
    format!(
        "{first}\n$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$\n$                       JOB NUMBER 2                       $\n$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$\n{second}"
    )
}

#[test]
fn dollar_job_banner_splits_different_molecules_and_normal_completion() {
    let file = parsed(&joined_jobs(&text(IR)));
    assert_eq!(file.jobs.len(), 2);
    assert_eq!(file.default_job(), Some(1));
    for (index, (atoms, energy)) in [(3, -74.999374598107), (20, -382.055107107616)]
        .into_iter()
        .enumerate()
    {
        let job = &file.jobs[index];
        assert_eq!(job.index as usize, index);
        assert_eq!(job.completion, Some(Completion::Normal));
        assert_eq!(job.steps[0].frame.numbers.len(), atoms);
        let result = job.result.as_ref().expect("separate result");
        assert_eq!(result.energy_hartree, Some(energy));
        assert_eq!(result.frame, job.steps[0].frame.hash());
        assert_eq!(result.status, Status::Converged);
        assert!(
            matches!(result.provenance.source, Source::Imported { job, .. } if job as usize == index)
        );
    }
    assert!(
        file.jobs[0]
            .result
            .as_ref()
            .expect("water")
            .vibrations
            .is_none()
    );
    assert_eq!(
        file.jobs[1]
            .result
            .as_ref()
            .expect("dvb")
            .vibrations
            .as_ref()
            .expect("IR")
            .frequencies_cm1
            .len(),
        54
    );
}

#[test]
fn truncated_second_job_keeps_first_job_as_default() {
    let ir = text(IR);
    // Cut the actual IR output halfway through its lines, before FINAL SINGLE POINT ENERGY.
    let half = ir
        .lines()
        .take(ir.lines().count() / 2)
        .collect::<Vec<_>>()
        .join("\n");
    let file = parsed(&joined_jobs(&half));
    assert_eq!(file.jobs.len(), 2);
    assert_eq!(file.default_job(), Some(0));
    assert_eq!(file.jobs[0].completion, Some(Completion::Normal));
    let job = &file.jobs[1];
    assert_eq!(job.index, 1);
    assert_eq!(job.completion, Some(Completion::Truncated));
    assert_eq!(job.steps[0].frame.numbers.len(), 20);
    let result = job.result.as_ref().expect("partial second-job result");
    assert_eq!(result.energy_hartree, None);
    assert_eq!(
        result.status,
        Status::NotConverged {
            reason: "the file ends before the job finished".into()
        }
    );
    assert!(result.vibrations.is_none());
}

#[test]
fn optimization_cut_in_middle_binds_last_energy_bearing_step() {
    let original = text(OPT);
    // The fixture's third ANGSTROEM block starts at line 3932; retain it and no third energy.
    let cut = original.lines().take(3960).collect::<Vec<_>>().join("\n");
    let file = parsed(&cut);
    let job = &file.jobs[0];
    assert_eq!(job.completion, Some(Completion::Truncated));
    assert_eq!(job.steps.len(), 3);
    assert_eq!(job.steps[2].energy_hartree, None);
    let result = job.result.as_ref().expect("partial optimization");
    assert_eq!(result.frame, job.steps[1].frame.hash());
    assert_eq!(result.energy_hartree, Some(-382.055126570567));
    assert_eq!(
        result.status,
        Status::NotConverged {
            reason: "the file ends before the job finished".into()
        }
    );
    assert_eq!(
        result
            .trajectory
            .as_ref()
            .expect("completed path prefix")
            .energies_hartree
            .len(),
        2
    );
    // No orbital or population block follows the second geometry: never reuse the first's.
    assert!(result.orbitals.is_none());
    assert!(result.charges.is_empty());
    assert!(result.dipole_e_bohr.is_none());
}

#[test]
fn identical_stationary_reprint_folds_and_uses_its_last_blocks() {
    let original = text(OPT);
    let starts: Vec<_> = original
        .match_indices(COORDINATES)
        .map(|(index, _)| index)
        .collect();
    let block_end = |start: usize| start + original[start..].find("\n\n").expect("geometry end");
    let previous = &original[starts[2]..block_end(starts[2])];
    // Copy only the preceding fixture's exact printed geometry into its stationary reprint.
    let modified = format!(
        "{}{}{}",
        &original[..starts[3]],
        previous,
        &original[block_end(starts[3])..]
    );
    let file = parsed(&modified);
    let job = &file.jobs[0];
    assert_eq!(job.steps.len(), 3);
    let result = job.result.as_ref().expect("stationary result");
    assert_eq!(result.frame, job.steps[2].frame.hash());
    assert_eq!(result.energy_hartree, Some(-382.055133399486));
    assert_eq!(
        result
            .orbitals
            .as_ref()
            .expect("stationary levels")
            .alpha
            .energies_hartree[0],
        -10.016736
    );
    assert_eq!(
        result
            .trajectory
            .as_ref()
            .expect("folded path")
            .energies_hartree
            .len(),
        3
    );
}

#[test]
fn failure_and_unconverged_optimization_preserve_explanations() {
    let water = text(WATER6);
    let failed = water.replace(
        "****ORCA TERMINATED NORMALLY****",
        "ORCA finished by error termination in SCF",
    );
    let file = parsed(&failed);
    let message = "ORCA finished by error termination in SCF".to_owned();
    assert_eq!(
        file.jobs[0].completion,
        Some(Completion::Error {
            message: message.clone()
        })
    );
    assert_eq!(
        file.jobs[0]
            .result
            .as_ref()
            .expect("failed partial result")
            .status,
        Status::Failed { reason: message }
    );
    let opt = text(OPT).replace(
        "THE OPTIMIZATION HAS CONVERGED",
        "The optimization did not converge: maximum number of cycles reached",
    );
    let file = parsed(&opt);
    assert_eq!(file.jobs[0].completion, Some(Completion::Normal));
    assert!(
        matches!(&file.jobs[0].result.as_ref().expect("unconverged").status, Status::NotConverged { reason } if reason.contains("maximum number of cycles reached"))
    );
}

#[test]
fn unrestricted_levels_have_separate_spin_occupations_without_shapes() {
    let original = text(WATER6);
    let start = original.find("ORBITAL ENERGIES\n").expect("orbital block");
    let rows_start = start + original[start..].find("   0   2.0000").expect("first MO");
    let rows_end = rows_start + original[rows_start..].find("*Only").expect("MO table end");
    let rows = original[rows_start..rows_end].replace("2.0000", "1.0000");
    // The real water orbital rows are repeated as two singly occupied spin tables; all
    // energies remain the fixture values. This tests the ORCA SPIN UP/DOWN table framing.
    let orbitals = format!(
        "ORBITAL ENERGIES\n----------------\nSPIN UP ORBITALS\nNO OCC E(Eh) E(eV)\n{rows}\nSPIN DOWN ORBITALS\nNO OCC E(Eh) E(eV)\n{rows}\n"
    );
    let modified = format!(
        "{}{}{}",
        &original[..start],
        orbitals,
        &original[rows_end..]
    );
    let file = parsed(&modified);
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .expect("result")
        .orbitals
        .as_ref()
        .expect("levels");
    let Beta::Unrestricted(beta) = &orbitals.beta else {
        panic!("separate beta spectrum expected");
    };
    assert_eq!(orbitals.alpha.energies_hartree.len(), 7);
    assert_eq!(beta.energies_hartree, orbitals.alpha.energies_hartree);
    assert_eq!(beta.occupations, [1., 1., 1., 1., 1., 0., 0.]);
    assert!(beta.rows.is_empty());
    assert!(beta.coefficients.is_empty());
}

#[test]
fn linear_rigid_modes_imaginary_sorting_and_alternate_ir_columns_stay_aligned() {
    let original = text(IR);
    // Turn mode 5 from a rigid zero into a 20 cm^-1 vibration (five leading rigid zeros),
    // and make fixture mode 6 imaginary. The mode/IR indices must survive sorting.
    let modified = original
        .replace("5:       0.00 cm**-1", "5:      20.00 cm**-1")
        .replace(
            "6:      43.87 cm**-1",
            "6:     -43.87 cm**-1 ***imaginary mode***",
        );
    let mut in_ir = false;
    let mut output = String::new();
    for line in modified.lines() {
        if line.trim() == "IR SPECTRUM" {
            in_ir = true;
        }
        // Exercise the alternate header without eps: "Mode freq Int T**2 TX TY TZ".
        // Remove that column only from the fixture IR table, keeping actual km/mol values.
        let words: Vec<_> = line.split_whitespace().collect();
        if in_ir && words.first() == Some(&"Mode") {
            output.push_str("Mode freq Int T**2 TX TY TZ\n");
        } else if in_ir
            && words.first().is_some_and(|word| {
                word.ends_with(':') && word.trim_end_matches(':').parse::<usize>().is_ok()
            })
        {
            let mut words = words;
            words.remove(2);
            output.push_str(&words.join(" "));
            output.push('\n');
        } else {
            if in_ir && line.contains("THERMOCHEMISTRY") {
                in_ir = false;
            }
            output.push_str(line);
            output.push('\n');
        }
    }
    let file = parsed(&output);
    let vibrations = file.jobs[0]
        .result
        .as_ref()
        .expect("result")
        .vibrations
        .as_ref()
        .expect("vibrations");
    assert_eq!(vibrations.frequencies_cm1.len(), 55);
    assert_eq!(&vibrations.frequencies_cm1[..3], &[-43.87, 20., 77.06]);
    assert_eq!(vibrations.imaginary_count(), 1);
    assert_eq!(&vibrations.ir_intensities_km_mol[..3], &[0.03, 0., 0.]);
    close(vibrations.modes[2], 0.078313, 1e-6);
    assert_eq!(vibrations.modes.len(), 55 * 60);
}

#[test]
fn malformed_coordinates_and_unsupported_atom_suffixes_have_chemist_messages() {
    let original = text(WATER6);
    // Mutate the printed ANGSTROEM block, not the input echo.
    let printed = "  O      1.784914    1.262422    0.511985\n";
    let nan = original.replace(printed, "  O      NaN    1.262422    0.511985\n");
    let error = read("derived.out", nan.as_bytes()).expect_err("nonfinite coordinate");
    assert!(
        matches!(error, Error::Invalid(message) if message.contains("Line 217") && message.contains("x coordinate must be finite"))
    );
    let ghost = original.replace(printed, "  O:     1.784914    1.262422    0.511985\n");
    let error = read("derived.out", ghost.as_bytes()).expect_err("ghost center");
    assert!(
        matches!(error, Error::Unsupported(message) if message.contains("O:") && message.contains("ghost atoms and atom suffixes are not supported"))
    );
}

#[test]
fn partial_last_coordinate_table_keeps_the_preceding_result_for_truncation_and_error() {
    let original = text(OPT);
    // Cut in the third fixture geometry, after only a few of its 20 atoms.
    let cut = original.lines().take(3940).collect::<Vec<_>>().join("\n");
    for suffix in ["", "\nORCA finished by error termination in geometry"] {
        let file = parsed(&format!("{cut}{suffix}"));
        let job = &file.jobs[0];
        assert_eq!(job.steps.len(), 2);
        let result = job.result.as_ref().expect("last complete step");
        assert_eq!(result.frame, job.steps[1].frame.hash());
        assert_eq!(result.energy_hartree, Some(-382.055126570567));
        if suffix.is_empty() {
            assert_eq!(job.completion, Some(Completion::Truncated));
        } else {
            assert!(matches!(result.status, Status::Failed { .. }));
        }
    }
}

#[test]
fn dollar_banners_preserve_empty_failed_jobs_and_ignore_initial_prologue_banner() {
    let water = text(WATER6);
    let prologue_end = water.find("INPUT FILE").expect("echo heading");
    // Retain the real program prologue, followed by an initial banner, an early error,
    // and a second banner with a complete real fixture. Job 0 never printed geometry.
    let modified = format!(
        "{}\n$ JOB NUMBER 1 $\nORCA finished by error termination before geometry\n$ JOB NUMBER 2 $\n{water}",
        &water[..prologue_end]
    );
    let file = parsed(&modified);
    assert_eq!(file.jobs.len(), 2);
    assert_eq!(file.default_job(), Some(1));
    assert_eq!(file.jobs[0].index, 0);
    assert!(file.jobs[0].steps.is_empty());
    assert!(file.jobs[0].result.is_none());
    assert!(matches!(
        file.jobs[0].completion,
        Some(Completion::Error { .. })
    ));
    assert_eq!(file.jobs[1].index, 1);
    assert_eq!(file.jobs[1].steps[0].frame.numbers.len(), 3);
}

#[test]
fn method_basis_and_kind_inference_use_only_the_echo_and_preserve_case() {
    let original = text(WATER6);
    for (keywords, expected_method, expected_basis, kind) in [
        (
            "RIJCOSX def2-SVP/J UB3LYP ma-def2-TZVP TightOpt Freq",
            Some("UB3LYP"),
            Some("ma-def2-TZVP"),
            JobKind::Optimization,
        ),
        (
            "DLPNO-CCSD(T) cc-pVTZ SP",
            Some("DLPNO-CCSD(T)"),
            Some("cc-pVTZ"),
            JobKind::SinglePoint,
        ),
        (
            "mystery def2-SVP/C NumFreq",
            None,
            None,
            JobKind::Frequencies,
        ),
        (
            "PBE0 def2-SVP/JK pcseg-2 EOM-CCSD",
            Some("PBE0"),
            Some("pcseg-2"),
            JobKind::Other,
        ),
    ] {
        let modified = original.replace("!  MP2 sto-3g", &format!("! {keywords}"));
        let file = parsed(&modified);
        let job = &file.jobs[0];
        assert_eq!(job.method.as_deref(), expected_method);
        assert_eq!(job.basis.as_deref(), expected_basis);
        assert_eq!(job.kind, kind);
        assert_eq!(
            job.keywords,
            keywords.split_whitespace().collect::<Vec<_>>()
        );
    }
    // Scan inside %geom takes precedence over the ordinary SP keyword.
    let scan = original.replace("|  2> \n", "|  2> %geom Scan B 0 1 = 0.9, 1.2, 3 end end\n");
    assert_eq!(parsed(&scan).jobs[0].kind, JobKind::Scan);
}

#[test]
fn frequencies_remain_on_the_evaluated_frame_after_an_uncomputed_final_geometry() {
    let original = text(IR).replace("****ORCA TERMINATED NORMALLY****", "");
    let start = original.find(COORDINATES).expect("fixture geometry");
    let end = start + original[start..].find("\n\n").expect("geometry end");
    // Append a changed copy of the real fixture geometry after its energy/frequency
    // calculation, with no subsequent energy. It is another step in the same job.
    let geometry = original[start..end].replacen("-1.415253", "-1.415254", 1);
    let modified = format!("{original}\n{geometry}\n\n");
    let file = parsed(&modified);
    let job = &file.jobs[0];
    assert_eq!(job.steps.len(), 2);
    let result = job.result.as_ref().expect("last energy-bearing result");
    assert_eq!(result.frame, job.steps[0].frame.hash());
    assert_eq!(result.energy_hartree, Some(-382.055107107616));
    assert_eq!(
        result
            .vibrations
            .as_ref()
            .expect("evaluated vibrations")
            .frequencies_cm1
            .len(),
        54
    );
}

#[test]
fn cut_inside_final_energy_line_keeps_a_prior_completed_energy() {
    let original = text(OPT);
    // Keep all of the third geometry, then interrupt its energy record before a number.
    let cut = original.lines().take(4200).collect::<Vec<_>>().join("\n");
    for tail in ["FINAL SINGLE POINT ENERGY", "FINAL SINGLE POINT ENERGY -"] {
        let file = parsed(&format!("{cut}\n{tail}"));
        let job = &file.jobs[0];
        assert_eq!(job.steps.len(), 3);
        assert_eq!(job.steps[2].energy_hartree, None);
        let result = job.result.as_ref().expect("prior completed energy");
        assert_eq!(result.frame, job.steps[1].frame.hash());
        assert_eq!(result.energy_hartree, Some(-382.055126570567));
        assert_eq!(job.completion, Some(Completion::Truncated));
    }
}

#[test]
fn repeated_tables_and_scalar_records_use_the_last_complete_occurrence() {
    let water = text(WATER6).replace("****ORCA TERMINATED NORMALLY****", "");
    let orbital_start = water.find("ORBITAL ENERGIES\n").expect("orbital heading");
    let orbital_end = orbital_start + water[orbital_start..].find("*Only").expect("orbital end");
    let orbitals = water[orbital_start..orbital_end].replace("-20.242269", "-20.000000");
    let mulliken_start = water
        .find("MULLIKEN ATOMIC CHARGES\n")
        .expect("population heading");
    let mulliken_end = mulliken_start
        + water[mulliken_start..]
            .find("Sum of atomic charges:")
            .expect("population end");
    let charges = water[mulliken_start..mulliken_end]
        .replace("-0.361722", "-0.400000")
        .replace("0.180857", "0.200000")
        .replace("0.180866", "0.200000");
    // Repeat real fixture blocks with changed values, without adding another geometry.
    // These records must replace that geometry's earlier table/scalar occurrences.
    let modified = format!(
        "{water}\n{orbitals}\n{charges}\nFINAL SINGLE POINT ENERGY -75.000000000000\nTotal Dipole Moment : 0.250000000 -0.112066055 -0.649766104\n"
    );
    let file = parsed(&modified);
    let job = &file.jobs[0];
    assert_eq!(job.steps.len(), 1);
    let result = job.result.as_ref().expect("result");
    assert_eq!(result.energy_hartree, Some(-75.));
    assert_eq!(result.charges[0].values, [-0.4, 0.2, 0.2]);
    assert_eq!(result.dipole_e_bohr.expect("last dipole")[0], 0.25);
    assert_eq!(
        result
            .orbitals
            .as_ref()
            .expect("last orbital block")
            .alpha
            .energies_hartree[0],
        -20.
    );
}

#[test]
fn incomplete_final_population_or_dipole_keeps_the_last_complete_block() {
    let original = text(WATER6).replace("****ORCA TERMINATED NORMALLY****", "");
    // Append interrupted repetitions of the real fixture's optional-property records.
    for tail in [
        "MULLIKEN ATOMIC CHARGES\n-----------------------\n0 O : -",
        "Total Dipole Moment : 0.1 -",
        "Total Dipole Moment : 0.1 0.2 -",
    ] {
        let file = parsed(&format!("{original}\n{tail}"));
        let result = file.jobs[0]
            .result
            .as_ref()
            .expect("partial calculation result");
        assert_eq!(result.energy_hartree, Some(-74.999374598107));
        assert_eq!(result.charges[0].values[0], -0.361722);
        assert_eq!(
            result.dipole_e_bohr.expect("last complete dipole")[0],
            0.150239405
        );
        assert_eq!(file.jobs[0].completion, Some(Completion::Truncated));
    }
}

#[test]
fn each_geometry_job_needs_its_own_state_but_xyzfile_echo_can_supply_it() {
    let original = text(WATER6);
    // Remove only the fixture's printed Total Charge / Multiplicity metadata, retaining
    // its own echoed `*xyz 0 1` as the electronic-state source.
    let without_fields = original
        .lines()
        .filter(|line| {
            !(line.trim_start().starts_with("Total Charge") && line.contains("....")
                || line.trim_start().starts_with("Multiplicity") && line.contains("...."))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parsed(&without_fields).jobs[0].steps[0].frame.charge, 0);
    let xyzfile = without_fields.replace("*xyz 0 1", "*xyzfile 0 1 water.xyz");
    assert_eq!(parsed(&xyzfile).jobs[0].steps[0].frame.multiplicity, 1);
    // The same charge/multiplicity fields follow the ORCA int/gzmt geometry commands.
    for command in ["int", "gzmt"] {
        let output = without_fields.replace("*xyz 0 1", &format!("*{command} 0 1"));
        assert_eq!(parsed(&output).jobs[0].steps[0].frame.multiplicity, 1);
    }
    let missing = without_fields.replace("*xyz 0 1", "*xyz");
    let error = read("derived.out", missing.as_bytes()).expect_err("missing job electronic state");
    assert!(
        matches!(error, Error::Invalid(message) if message.starts_with("Line 215:") && message.contains("does not report its total charge"))
    );
}

#[test]
fn appended_invocations_keep_their_own_method_energy_and_completion() {
    let water = text(WATER6);
    let ir = text(IR);
    // Literal fixture records: "* O   R   C   A *", "****ORCA TERMINATED NORMALLY****",
    // "!  MP2 sto-3g", "! rks b3lyp sto-3g usesym freq", and both final energies below.
    let without_prologue = &ir[ir.find("INPUT FILE").expect("second input")..];
    for (first, second, failed) in [
        (water.clone(), ir.as_str(), false),
        (water.clone(), without_prologue, false),
        (
            water.replace(
                "****ORCA TERMINATED NORMALLY****",
                "ORCA finished by error termination in SCF",
            ),
            without_prologue,
            true,
        ),
    ] {
        let file = parsed(&format!("{first}\n{second}"));
        assert_eq!(file.jobs.len(), 2);
        for (job, method, energy) in [
            (&file.jobs[0], "MP2", -74.999374598107),
            (&file.jobs[1], "b3lyp", -382.055107107616),
        ] {
            assert_eq!(job.method.as_deref(), Some(method));
            assert_eq!(
                job.result.as_ref().expect("own energy").energy_hartree,
                Some(energy)
            );
        }
        assert_eq!(file.jobs[1].completion, Some(Completion::Normal));
        if failed {
            assert!(matches!(
                file.jobs[0].completion,
                Some(Completion::Error { .. })
            ));
        } else {
            assert_eq!(file.jobs[0].completion, Some(Completion::Normal));
        }
    }
    // A new invocation's prologue also bounds an earlier run whose end was cut off.
    let first = water
        .split("****ORCA TERMINATED NORMALLY****")
        .next()
        .expect("first run");
    assert_eq!(parsed(&format!("{first}\n{ir}")).jobs.len(), 2);
    // Even a prologue cut before its INPUT FILE is a separate attempted invocation.
    let first = water.lines().take(10).collect::<Vec<_>>().join("\n");
    let file = parsed(&format!("{first}\n{ir}"));
    assert_eq!(file.jobs.len(), 2);
    assert!(file.jobs[0].steps.is_empty());
    assert_eq!(file.jobs[0].completion, Some(Completion::Truncated));
}

#[test]
fn one_compound_echo_supplies_each_numbered_jobs_keywords_and_state() {
    let water = text(WATER6);
    // Fixture echo: "|  1> !  MP2 sto-3g", "|  3> *xyz 0 1", then
    // "****END OF INPUT****". Derive one compound echo with ORCA's $new_job separator.
    let prologue = &water[..water.find("INPUT FILE").expect("echo")];
    let body = water
        .lines()
        .skip(208)
        .filter(|line| {
            !(line.trim_start().starts_with("Total Charge") && line.contains("....")
                || line.trim_start().starts_with("Multiplicity") && line.contains("...."))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let first = body.replace("****ORCA TERMINATED NORMALLY****", "");
    let output = format!(
        "{prologue}INPUT FILE\n| 1> ! HF STO-3G SP\n| 2> *xyz 0 1\n| 3> $new_job\n| 4> ! MP2 def2-SVP Opt\n| 5> *xyzfile 1 2 water.xyz\n| 6> ****END OF INPUT****\n$ JOB NUMBER 1 $\n{first}\n$ JOB NUMBER 2 $\n{body}"
    );
    let file = parsed(&output);
    assert_eq!(
        file.jobs.len(),
        2,
        "the echo before JOB NUMBER 1 is its prologue"
    );
    for (index, method, basis, kind, state) in [
        (0, "HF", "STO-3G", JobKind::SinglePoint, (0, 1)),
        (1, "MP2", "def2-SVP", JobKind::Optimization, (1, 2)),
    ] {
        let job = &file.jobs[index];
        assert_eq!(job.method.as_deref(), Some(method));
        assert_eq!(job.basis.as_deref(), Some(basis));
        assert_eq!(job.kind, kind);
        assert_eq!(
            job.keywords,
            [method, basis, if index == 0 { "SP" } else { "Opt" }]
        );
        let frame = &job.steps[0].frame;
        assert_eq!((frame.charge, frame.multiplicity), state);
    }
    // An unexecuted third $new_job echo cannot supply metadata to a new invocation
    // whose prologue ends before its own INPUT FILE record.
    let output = output.replace(
        "| 6> ****END OF INPUT****",
        "| 6> $new_job\n| 7> ! PBE def2-TZVP SP\n| 8> *xyz 0 1\n| 9> ****END OF INPUT****",
    );
    let cut_prologue = water.lines().take(10).collect::<Vec<_>>().join("\n");
    for banner in ["", "$ JOB NUMBER 1 $\n"] {
        let file = parsed(&format!("{output}\n{banner}{cut_prologue}"));
        assert_eq!(file.jobs.len(), 3);
        assert!(file.jobs[2].keywords.is_empty());
        assert!(file.jobs[2].method.is_none());
    }
}

#[test]
fn correlated_keywords_override_the_hartree_fock_reference() {
    let water = text(WATER6);
    // Replace the fixture's literal "!  MP2 sto-3g" with a reference plus the same method.
    for reference in ["HF", "RHF", "UHF", "ROHF"] {
        for method in [
            "MP2",
            "MP3",
            "MP4(SDQ)",
            "CISD",
            "QCISD",
            "QCISD(T)",
            "RI-MP2",
            "DLPNO-CCSD(T)",
        ] {
            let output = water.replace("!  MP2 sto-3g", &format!("! {reference} {method} sto-3g"));
            assert_eq!(parsed(&output).jobs[0].method.as_deref(), Some(method));
        }
    }
}

#[test]
fn empty_optional_headings_are_bounded_and_orbital_scanning_is_linear() {
    let water = text(WATER6);
    // Fixture heading at line 535: "ORBITAL ENERGIES". Keep its real preceding geometry
    // and repeat this empty heading 20,000 times; none may scan through later tables.
    let before = &water[..water.find("ORBITAL ENERGIES\n").expect("orbital heading")];
    let output = format!("{before}{}", "ORBITAL ENERGIES\n".repeat(20_000));
    let start = std::time::Instant::now();
    assert!(
        parsed(&output).jobs[0]
            .result
            .as_ref()
            .expect("geometry")
            .orbitals
            .is_none()
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "empty orbital headings must parse in under one second: {:?}",
        start.elapsed()
    );

    // The fixture also prints "VIBRATIONAL FREQUENCIES", "NORMAL MODES", and
    // "IR SPECTRUM". Empty headings cannot consume a later, unrelated numbered table.
    let output = format!(
        "{before}VIBRATIONAL FREQUENCIES\nNORMAL MODES\nIR SPECTRUM\nORBITAL ENERGIES\nNO OCC E(Eh) E(eV)\n0 2.0000 -20.242269 -550.8201\n\n"
    );
    let file = parsed(&output);
    let result = file.jobs[0].result.as_ref().expect("bounded tables");
    assert!(result.vibrations.is_none());
    assert_eq!(
        result
            .orbitals
            .as_ref()
            .expect("last table")
            .alpha
            .energies_hartree,
        [-20.242269]
    );
}

#[test]
fn incomplete_vibrational_tables_keep_energy_and_earlier_complete_data() {
    let original = text(IR);
    // Fixture lines 1422 "VIBRATIONAL FREQUENCIES", 1491 "NORMAL MODES", and
    // 1881 "IR SPECTRUM" follow "FINAL SINGLE POINT ENERGY -382.055107107616".
    // Line 1434 is inside the frequency rows; line 1887 is the first IR row (mode 6).
    for cut in [1434, 1887] {
        let output = original.lines().take(cut).collect::<Vec<_>>().join("\n");
        let file = parsed(&output);
        let result = file.jobs[0].result.as_ref().expect("earlier energy");
        assert_eq!(result.energy_hartree, Some(-382.055107107616));
        assert_eq!(file.jobs[0].completion, Some(Completion::Truncated));
        if cut == 1434 {
            assert!(result.vibrations.is_none());
        } else {
            let vibrations = result.vibrations.as_ref().expect("complete frequencies");
            assert_eq!(vibrations.frequencies_cm1.len(), 54);
            assert_eq!(vibrations.modes.len(), 54 * 60);
            assert!(vibrations.ir_intensities_km_mol.is_empty());
        }
    }
    let complete = original.replace("****ORCA TERMINATED NORMALLY****", "");
    for (start, end) in [(1421, 1434), (1490, 1530), (1880, 1888)] {
        let tail = original
            .lines()
            .skip(start)
            .take(end - start)
            .collect::<Vec<_>>()
            .join("\n");
        let file = parsed(&format!("{complete}\n{tail}"));
        let result = file.jobs[0].result.as_ref().expect("last complete tables");
        let vibrations = result
            .vibrations
            .as_ref()
            .expect("last complete vibrations");
        assert_eq!(result.energy_hartree, Some(-382.055107107616));
        assert_eq!(vibrations.frequencies_cm1.len(), 54);
        assert_eq!(vibrations.modes.len(), 54 * 60);
        assert_eq!(vibrations.ir_intensities_km_mol[0], 0.03);
        assert_eq!(vibrations.ir_intensities_km_mol[20], 36.79);
    }
    // Cuts within the printed unit and a normal-mode column number are also partial
    // records, even when the preceding numeric tokens can already be parsed.
    for tail in [
        "VIBRATIONAL FREQUENCIES\n0: 0.00 cm**-",
        "NORMAL MODES\n10 11 12 13 14 15 16 17 18 1",
    ] {
        let file = parsed(&format!("{complete}\n{tail}"));
        let vibrations = file.jobs[0]
            .result
            .as_ref()
            .expect("earlier energy")
            .vibrations
            .as_ref()
            .expect("earlier complete tables");
        assert_eq!(vibrations.frequencies_cm1.len(), 54);
        assert_eq!(vibrations.modes.len(), 54 * 60);
    }
    // A blank or an explicit error following only the first two IR rows cannot make
    // that incomplete repeat replace the complete mode26 intensity of36.79 km/mol.
    let tail = original
        .lines()
        .skip(1880)
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");
    for ending in ["\n\n", "\nORCA finished by error termination in IR"] {
        let file = parsed(&format!("{complete}\n{tail}{ending}"));
        let vibrations = file.jobs[0]
            .result
            .as_ref()
            .expect("earlier energy")
            .vibrations
            .as_ref()
            .expect("earlier complete IR");
        assert_eq!(vibrations.ir_intensities_km_mol[20], 36.79);
    }
}

#[test]
fn normally_terminated_scf_failure_is_bound_to_the_evaluated_geometry() {
    let water = text(WATER6);
    // Fixture line 500: "SCF CONVERGED AFTER   8 CYCLES". Derive the corresponding
    // failure banner while retaining its real final energy and normal termination.
    let output = water.replace("SCF CONVERGED AFTER", "SCF NOT CONVERGED AFTER");
    let file = parsed(&output);
    assert_eq!(file.jobs[0].completion, Some(Completion::Normal));
    assert!(
        matches!(&file.jobs[0].result.as_ref().expect("partial SCF").status,
        Status::NotConverged { reason } if reason.contains("SCF NOT CONVERGED AFTER"))
    );
    // An earlier optimization step's failed SCF must not describe its converged final step.
    let output = text(OPT).replacen("SCF CONVERGED AFTER", "SCF NOT CONVERGED AFTER", 1);
    assert_eq!(
        parsed(&output).jobs[0]
            .result
            .as_ref()
            .expect("final step")
            .status,
        Status::Converged
    );
    // Fixture line 4218 is "SCF CONVERGED AFTER   6 CYCLES". The stationary
    // reevaluation later converges at line 5646 and supplies -382.055133399486 Eh.
    // Fold its geometry onto the preceding frame, then fail only that earlier SCF.
    let output = text(OPT).replace(
        "SCF CONVERGED AFTER   6 CYCLES",
        "SCF NOT CONVERGED AFTER   6 CYCLES",
    );
    let starts: Vec<_> = output
        .match_indices(COORDINATES)
        .map(|(at, _)| at)
        .collect();
    let end = |at: usize| at + output[at..].find("\n\n").expect("geometry end");
    let output = format!(
        "{}{}{}",
        &output[..starts[3]],
        &output[starts[2]..end(starts[2])],
        &output[end(starts[3])..]
    );
    let file = parsed(&output);
    let result = file.jobs[0].result.as_ref().expect("folded reevaluation");
    assert_eq!(file.jobs[0].steps.len(), 3);
    assert_eq!(result.energy_hartree, Some(-382.055133399486));
    assert_eq!(result.status, Status::Converged);
}

#[test]
fn shared_open_shell_levels_split_total_occupations_into_two_spins() {
    let water = text(WATER6);
    // Fixture rows "0 2.0000 -20.242269 -550.8201" through "6 0.0000 ..." are
    // one shared table. Derive a doublet with total occupations 2/1/0, then one whose
    // five occupied rows all have total 1; both use the explicit printed multiplicity.
    for (charge, totals, alpha, beta) in [
        (
            1,
            false,
            vec![1., 1., 1., 1., 1., 0., 0.],
            vec![1., 1., 1., 1., 0., 0., 0.],
        ),
        (5, true, vec![1., 1., 1., 1., 1., 0., 0.], vec![0.; 7]),
    ] {
        let mut output = water
            .replace("*xyz 0 1", &format!("*xyz {charge} 2"))
            .replace(
                "Charge          ....    0",
                &format!("Charge          ....    {charge}"),
            )
            .replace("Mult            ....    1", "Mult            ....    2");
        output = if totals {
            output.replace("2.0000", "1.0000")
        } else {
            output.replace("   4   2.0000", "   4   1.0000")
        };
        let file = parsed(&output);
        let orbitals = file.jobs[0]
            .result
            .as_ref()
            .expect("doublet")
            .orbitals
            .as_ref()
            .expect("shared levels");
        assert_eq!(orbitals.alpha.occupations, alpha);
        assert_eq!(orbitals.beta, Beta::SameOrbitals { occupations: beta });
    }
}
