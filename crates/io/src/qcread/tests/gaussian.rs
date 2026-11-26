use crate::qcread::tests::{fixture, open};
use crate::qcread::{Completion, Error, JobKind, read};
use reshiki_qcdata::{Beta, ChargeScheme, DEBYE_PER_E_BOHR, MAX_HESSIAN_ATOMS, Status, Task};

const GOPT: &str = "cclib/g16_dvb_gopt.out";
const IR: &str = "cclib/g16_dvb_ir.out";
const MP2: &str = "cclib/g16_water_mp2.log";
const LINK1: &str = "pymatgen/CH2O_CO2.log";
const WATER: &str = "pymatgen/H2O_gau_vib.out";
const METHANE: &str = "pymatgen/methane.log";
const SCAN: &str = "pymatgen/so2_scan.log";

#[test]
fn g16_optimization_steps_and_final_population() {
    let file = open(GOPT);
    // Gaussian 16:  ES64L-G16RevA.03 25-Dec-2016
    assert_eq!(file.program.as_deref(), Some("Gaussian"));
    assert_eq!(file.program_version.as_deref(), Some("16 Revision A.03"));
    assert_eq!(file.jobs.len(), 1);
    let job = &file.jobs[0];
    // Five SCF steps, then "Standard orientation:" again after "Optimization completed."
    assert_eq!(job.steps.len(), 6);
    assert_eq!(job.steps[0].frame.numbers.len(), 20);
    assert_eq!(job.kind, JobKind::Optimization);
    assert_eq!(job.completion, Some(Completion::Normal));
    assert_eq!(job.method.as_deref(), Some("RB3LYP"));
    assert_eq!(job.basis.as_deref(), Some("STO-3G"));
    // SCF Done:  E(RB3LYP) =  -382.294279146     A.U. after   12 cycles
    assert_eq!(job.steps[0].energy_hartree, Some(-382.294279146));
    // SCF Done:  E(RB3LYP) =  -382.308266602     A.U. after    7 cycles
    assert_eq!(job.steps[4].energy_hartree, Some(-382.308266602));
    assert_eq!(job.steps[5].energy_hartree, None);
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.frame, job.steps[4].frame.hash());
    assert_eq!(result.frame, job.steps[5].frame.hash());
    assert_eq!(result.task, Task::Optimize);
    assert_eq!(result.status, Status::Converged);
    // 1-AG\HF=-382.3082666\RMSD=5.687e-09
    assert_eq!(result.energy_hartree, Some(-382.3082666));
    assert_eq!(result.energy_terms["scf"], -382.308266602);
    let orbitals = result.orbitals.as_ref().unwrap();
    assert_eq!(orbitals.alpha.energies_hartree.len(), 60);
    assert_eq!(orbitals.beta, Beta::Restricted);
    assert!(orbitals.basis.is_none() && orbitals.alpha.rows.is_empty());
    // Alpha  occ. eigenvalues --  -10.02022 -10.02017 -10.00811 -10.00811 -10.00679
    assert_eq!(orbitals.alpha.energies_hartree[0], -10.02022);
    assert_eq!(orbitals.alpha.occupations[34..36], [2.0, 0.0]);
    //      1  C   -0.004504 (final Mulliken table)
    assert_eq!(result.charges[0].scheme, ChargeScheme::Mulliken);
    assert_eq!(result.charges[0].values.len(), 20);
    assert_eq!(result.charges[0].values[0], -0.004504);
    assert_eq!(
        result.trajectory.as_ref().unwrap().energies_hartree.len(),
        5
    );
}

#[test]
fn g16_ir_prefers_high_precision_modes() {
    let file = open(IR);
    assert_eq!(file.jobs.len(), 1);
    let job = &file.jobs[0];
    assert_eq!(job.steps.len(), 1);
    assert_eq!(job.steps[0].frame.numbers.len(), 20);
    assert_eq!(job.kind, JobKind::Frequencies);
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.task, Task::Hessian);
    assert_eq!(result.status, Status::Converged);
    assert_eq!(result.frame, job.steps[0].frame.hash());
    let vibrations = result.vibrations.as_ref().unwrap();
    // The last HPModes panel is headed "51        52        53        54".
    assert_eq!(vibrations.frequencies_cm1.len(), 54);
    // Frequencies ---    53.1981   84.7415  149.4005  179.3403  263.3734
    // Reduced masses ---     3.2266    2.4837    2.0822    3.3848    3.2244
    // IR Intensities ---     0.0342    0.0000    0.3727    0.2687    0.0000
    assert_eq!(vibrations.frequencies_cm1[0], 53.1981);
    assert_eq!(vibrations.reduced_masses_amu[0], 3.2266);
    assert_eq!(vibrations.ir_intensities_km_mol[0], 0.0342);
    // Frequencies ---  3467.0890 3470.0274 3548.3199 3548.3320
    assert_eq!(vibrations.frequencies_cm1[53], 3548.3320);
    assert_eq!(vibrations.modes.len(), 54 * 60);
    for mode in vibrations.modes.as_chunks::<60>().0 {
        let norm_squared: f64 = mode.iter().map(|x| x * x).sum();
        assert!((norm_squared - 1.0).abs() < 1e-12);
    }
    // Coord Atom Element:
    //    3     1     6         -0.08074   0.08509  -0.02967   0.00000   0.00000
    //    3     2     6         -0.09727   0.13198  -0.09339   0.00000   0.00000
    let ratio = vibrations.modes[2] / vibrations.modes[5];
    assert!((ratio - 0.08074 / 0.09727).abs() < 1e-12);
}

#[test]
fn g16_mp2_archive_and_step_energies() {
    let file = open(MP2);
    assert_eq!(file.jobs.len(), 1);
    let job = &file.jobs[0];
    assert_eq!(job.kind, JobKind::SinglePoint);
    assert_eq!(job.steps.len(), 1);
    assert_eq!(job.steps[0].frame.numbers, [8, 1, 1]);
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.frame, job.steps[0].frame.hash());
    assert_eq!(result.task, Task::Energy);
    // SCF Done:  E(RHF) =  -74.9643287914     A.U. after    7 cycles
    // E2 =    -0.3795333610D-01 EUMP2 =    -0.75002282127454D+02
    assert_eq!(result.energy_terms["scf"], -74.9643287914);
    assert_eq!(job.steps[0].energy_hartree, Some(-75.002282127454));
    // State=1-A1\HF=-74.9643288\MP2=-7 (continued) 5.0022821\RMSD=2.787e-10
    assert_eq!(result.energy_terms["hf"], -74.9643288);
    assert_eq!(result.energy_terms["mp2"], -75.0022821);
    assert_eq!(result.energy_hartree, Some(-75.0022821));
    // X= -0.0000    Y= -0.0000    Z= -1.5994  Tot= 1.5994
    assert_eq!(
        result.dipole_e_bohr,
        Some([0.0, 0.0, -1.5994 / DEBYE_PER_E_BOHR])
    );
    //      1  O   -0.319889
    assert_eq!(result.charges[0].values[0], -0.319889);
}

#[test]
fn gaussian_normal_termination_title_does_not_split_the_job() {
    // Replace the fixture's "Water" title in memory. The job still ends at
    // "Normal termination of Gaussian 16 at Wed Apr  4 10:14:17 2018."
    let source = String::from_utf8(fixture(MP2)).unwrap();
    let titled = source.replace(
        "\n Water\n",
        "\n Normal termination of Gaussian water benchmark\n",
    );
    let file = read(MP2, titled.as_bytes()).unwrap();
    let [job] = file.jobs.as_slice() else {
        panic!("the title and termination belong to one Gaussian job");
    };
    assert_eq!(job.completion, Some(Completion::Normal));
    // NAtoms=      3 NQM=        3 NQMF=       0 NMMI=      0 NMMIF=      0
    assert_eq!(job.steps[0].frame.numbers.len(), 3);
    // E2 =    -0.3795333610D-01 EUMP2 =    -0.75002282127454D+02
    assert_eq!(job.steps[0].energy_hartree, Some(-75.002282127454));

    let missing_at = titled.replace(
        "Normal termination of Gaussian 16 at ",
        "Normal termination of Gaussian 16 ",
    );
    let file = read(MP2, missing_at.as_bytes()).unwrap();
    assert_eq!(file.jobs[0].completion, Some(Completion::Truncated));
}

#[test]
fn g09_link1_and_internal_jobs_stay_separate() {
    let file = open(LINK1);
    // "Gaussian 09, Revision D.01," and three "Normal termination of Gaussian 09" lines.
    // The first route is "#N B3LYP/6-31G freq", the second "#N B3LYP/6-31G opt freq".
    // The latter prints "Link1:  Proceeding to internal job step number  2."
    assert_eq!(file.program_version.as_deref(), Some("09 Revision D.01"));
    assert_eq!(file.jobs.len(), 3);
    assert_eq!(file.default_job(), Some(2));
    for (i, job) in file.jobs.iter().enumerate() {
        assert_eq!(job.index as usize, i);
        assert_eq!(job.completion, Some(Completion::Normal));
    }
    let [ch2o, co2_opt, co2_freq] = file.jobs.as_slice() else {
        unreachable!()
    };
    assert_eq!(ch2o.kind, JobKind::Frequencies);
    assert_eq!(co2_opt.kind, JobKind::Optimization);
    assert_eq!(co2_freq.kind, JobKind::Frequencies);
    assert_eq!(ch2o.steps.len(), 1);
    assert_eq!(co2_opt.steps.len(), 4);
    assert_eq!(co2_freq.steps.len(), 1);
    assert_eq!(ch2o.steps[0].frame.numbers, [6, 1, 1, 8]);
    assert_eq!(co2_freq.steps[0].frame.numbers, [6, 8, 8]);
    let ch2o = ch2o.result.as_ref().unwrap();
    let co2 = co2_freq.result.as_ref().unwrap();
    // 1-A'\HF=-114.4611366 (CH2O) and 1-SGG\HF=-188.5002697 (CO2).
    assert_eq!(ch2o.energy_hartree, Some(-114.4611366));
    assert_eq!(co2.energy_hartree, Some(-188.5002697));
    // Frequencies --   1203.1940              1274.0172              1560.2688 (CH2O)
    // Frequencies --    563.0027               563.0027              1282.4483 (CO2)
    assert_eq!(ch2o.vibrations.as_ref().unwrap().frequencies_cm1.len(), 6);
    assert_eq!(
        ch2o.vibrations.as_ref().unwrap().frequencies_cm1[0],
        1203.1940
    );
    assert_eq!(co2.vibrations.as_ref().unwrap().frequencies_cm1.len(), 4);
    assert_eq!(
        co2.vibrations.as_ref().unwrap().frequencies_cm1[0],
        563.0027
    );
}

#[test]
fn g09_opt_freq_uses_the_same_standard_orientation() {
    let file = open(WATER);
    // "Link1:  Proceeding to internal job step number  2." follows the FOpt archive.
    assert_eq!(file.jobs.len(), 2);
    let opt = &file.jobs[0];
    let freq = &file.jobs[1];
    assert_eq!(opt.kind, JobKind::Optimization);
    assert_eq!(freq.kind, JobKind::Frequencies);
    assert_eq!(opt.steps.len(), 12);
    assert_eq!(freq.steps.len(), 1);
    let final_opt = &opt.steps.last().unwrap().frame;
    let freq_frame = &freq.steps[0].frame;
    // Both Standard orientation tables have 2  1  0  0.771217  -0.469245  0.000000.
    assert_eq!(freq_frame.numbers, [8, 1, 1]);
    assert_eq!(freq_frame.positions_angstrom[1], [0.771217, -0.469245, 0.0]);
    assert_eq!(freq_frame.hash(), final_opt.hash());
    let result = freq.result.as_ref().unwrap();
    assert_eq!(result.frame, freq_frame.hash());
    let vibrations = result.vibrations.as_ref().unwrap();
    // Frequencies --   1662.8033              3736.2607              3860.5869
    assert_eq!(
        vibrations.frequencies_cm1,
        [1662.8033, 3736.2607, 3860.5869]
    );
}

#[test]
fn g03_methane_and_five_column_orientation() {
    let file = open(METHANE);
    // Gaussian 03:  IA32L-G03RevD.01 13-Oct-2005
    assert_eq!(file.program_version.as_deref(), Some("03 Revision D.01"));
    assert_eq!(file.jobs.len(), 1);
    let job = &file.jobs[0];
    assert_eq!(job.steps.len(), 4);
    assert_eq!(job.steps[0].frame.numbers, [6, 1, 1, 1, 1]);
    //    2          1             0        0.617765    0.617765    0.617765
    assert_eq!(job.steps[0].frame.positions_angstrom[1], [0.617765; 3]);
    // State=1-A1\HF=-39.9768776\RMSD=3.210e-09
    assert_eq!(
        job.result.as_ref().unwrap().energy_hartree,
        Some(-39.9768776)
    );

    // The fixture includes Atomic Type, so remove that column from a real table in memory.
    let text = String::from_utf8(fixture(METHANE)).unwrap();
    let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
    let start = lines
        .iter()
        .position(|l| l.trim() == "Standard orientation:")
        .unwrap();
    for row in &mut lines[start + 5..start + 10] {
        let mut fields: Vec<_> = row.split_whitespace().collect();
        fields.remove(2);
        *row = fields.join(" ");
    }
    let five_columns = read("methane.log", lines.join("\n").as_bytes()).unwrap();
    assert_eq!(five_columns.jobs[0].steps, job.steps);

    // Insert a dummy row into that five-column table, then change it to a ghost.
    lines.insert(start + 10, "6 -1 0.0 0.0 0.0".into());
    let dummy = read("methane.log", lines.join("\n").as_bytes()).unwrap();
    assert_eq!(dummy.jobs[0].steps, job.steps);
    lines[start + 10] = "6 0 0.0 0.0 0.0".into();
    assert_eq!(
        read("methane.log", lines.join("\n").as_bytes()).unwrap_err(),
        Error::Unsupported(format!(
            "Line {}: Gaussian ghost atoms (Bq) are unsupported",
            start + 11
        )),
    );
}

#[test]
fn g09_scan_retains_all_point_energies() {
    let file = open(SCAN);
    assert_eq!(file.jobs.len(), 1);
    let job = &file.jobs[0];
    assert_eq!(job.kind, JobKind::Scan);
    assert_eq!(job.steps[0].frame.numbers, [16, 8, 8]);
    // Summary of the potential surface scan: rows 1 through "21     3.0000   -548.02102".
    assert_eq!(job.steps.len(), 21);
    assert!(job.steps.iter().all(|step| step.energy_hartree.is_some()));
    // SCF Done:  E(RB3LYP) =  -547.050773534     A.U. after   12 cycles
    // SCF Done:  E(RB3LYP) =  -548.579124659     A.U. after   12 cycles
    // SCF Done:  E(RB3LYP) =  -548.021019862     A.U. after   19 cycles
    assert_eq!(job.steps[0].energy_hartree, Some(-547.050773534));
    assert_eq!(job.steps[5].energy_hartree, Some(-548.579124659));
    assert_eq!(job.steps[20].energy_hartree, Some(-548.021019862));
    let result = job.result.as_ref().unwrap();
    // The archive's HF list ends with ",-548.0210199\RMSD=1.155e-09".
    assert_eq!(result.energy_hartree, Some(-548.0210199));
    assert_eq!(result.frame, job.steps[20].frame.hash());
    assert_eq!(result.task, Task::Energy);
}

#[test]
fn truncated_optimization_keeps_the_last_evaluated_step() {
    // Cut the Gaussian 16 optimization from the fixture before the SCF of its fourth geometry.
    let text = String::from_utf8(fixture(GOPT)).unwrap();
    let cut = text
        .find(" SCF Done:  E(RB3LYP) =  -382.308265702")
        .unwrap();
    let file = read("interrupted.log", &text.as_bytes()[..cut]).unwrap();
    assert_eq!(file.jobs.len(), 1);
    let job = &file.jobs[0];
    assert_eq!(job.steps.len(), 4);
    assert_eq!(job.steps[3].energy_hartree, None);
    assert_eq!(job.completion, Some(Completion::Truncated));
    let result = job.result.as_ref().unwrap();
    assert_eq!(
        result.status,
        Status::NotConverged {
            reason: "the file ends before the job finished".into(),
        }
    );
    assert_eq!(result.frame, job.steps[2].frame.hash());
    assert_ne!(result.frame, job.steps[3].frame.hash());
    // SCF Done:  E(RB3LYP) =  -382.308239279     A.U. after   10 cycles
    assert_eq!(result.energy_hartree, Some(-382.308239279));
    assert_eq!(job.method.as_deref(), Some("b3lyp"));
    assert_eq!(job.basis.as_deref(), Some("sto-3g"));
}

#[test]
fn final_error_preserves_earlier_completed_jobs() {
    // Replace the last Gaussian 09 termination in the Link1 fixture in memory.
    let text = String::from_utf8(fixture(LINK1)).unwrap();
    let cut = text.rfind(" Normal termination of Gaussian").unwrap();
    let failure = "Convergence failure -- run terminated.";
    let changed = format!(
        "{} {failure}\n Error termination via Lnk1e in /opt/g09/l502.exe at Fri Oct 30 19:07:41 2015.\n",
        &text[..cut]
    );
    let original = open(LINK1);
    let file = read("failed.log", changed.as_bytes()).unwrap();
    assert_eq!(file.jobs.len(), 3);
    assert_eq!(file.default_job(), Some(1));
    for (before, after) in original.jobs[..2].iter().zip(&file.jobs[..2]) {
        assert_eq!(after.steps, before.steps);
        let mut expected = before.result.clone().unwrap();
        let actual = after.result.as_ref().unwrap();
        expected.provenance.source = actual.provenance.source.clone();
        assert_eq!(*actual, expected);
    }
    assert_eq!(
        file.jobs[2].completion,
        Some(Completion::Error {
            message: failure.into()
        })
    );
    assert_eq!(
        file.jobs[2].result.as_ref().unwrap().status,
        Status::Failed {
            reason: failure.into()
        }
    );

    // Keep just the CO2 input and its two calculations to exercise default job 0.
    let start = changed.rfind(" Initial command:").unwrap();
    let second_input = read("failed-co2.log", &changed.as_bytes()[start..]).unwrap();
    assert_eq!(second_input.jobs.len(), 2);
    assert_eq!(second_input.default_job(), Some(0));
}

#[test]
fn a_job_cut_before_geometry_has_no_result() {
    // Cut the Link1 fixture before the first geometry of the last job.
    let text = String::from_utf8(fixture(LINK1)).unwrap();
    let cut = text
        .rfind("                          Input orientation:")
        .unwrap();
    let file = read("no-last-geometry.log", &text.as_bytes()[..cut]).unwrap();
    assert_eq!(file.jobs.len(), 3);
    assert_eq!(file.default_job(), Some(1));
    let last = &file.jobs[2];
    assert_eq!(last.completion, Some(Completion::Truncated));
    assert!(last.steps.is_empty());
    assert!(last.result.is_none());

    // Cut the same fixture before its first job has printed any orientation.
    let cut = text
        .find("                          Input orientation:")
        .unwrap();
    assert_eq!(
        read("no-geometry.log", &text.as_bytes()[..cut]).unwrap_err(),
        Error::Invalid("The file has no geometry".into()),
    );
}

#[test]
fn fixed_width_orbital_fields_may_touch() {
    // Replace two fields in the last population block of the Gaussian 16 optimization.
    let text = String::from_utf8(fixture(GOPT)).unwrap();
    let text = text.replace("  -10.02022 -10.02017", " -110.02022-110.02017");
    let file = read("touching-levels.log", text.as_bytes()).unwrap();
    let result = file.jobs[0].result.as_ref().unwrap();
    let energies = &result.orbitals.as_ref().unwrap().alpha.energies_hartree;
    assert_eq!(energies[..2], [-110.02022, -110.02017]);
}

#[test]
fn a_cut_inside_analysis_keeps_the_evaluated_frame() {
    // Cut real GOPT output in its last orbital or Mulliken block, mid-number.
    let text = String::from_utf8(fixture(GOPT)).unwrap();
    let orbital = "Alpha  occ. eigenvalues --  -10.02022 -";
    let charge = "1  C   -0.004504";
    for cut in [
        text.rfind(orbital).unwrap() + orbital.len(),
        text.rfind(charge).unwrap() + "1  C   -".len(),
    ] {
        let file = read("partial-population.log", &text.as_bytes()[..cut]).unwrap();
        let job = &file.jobs[0];
        assert_eq!(job.steps.len(), 6);
        assert_eq!(job.completion, Some(Completion::Truncated));
        assert_eq!(
            job.result.as_ref().unwrap().energy_hartree,
            Some(-382.308266602)
        );
    }

    // Cut the IR fixture partway through its first HPModes property row.
    let text = String::from_utf8(fixture(IR)).unwrap();
    let marker = "Reduced masses ---     3.2266";
    let cut = text.find(marker).unwrap() + marker.len();
    let file = read("partial-vibrations.log", &text.as_bytes()[..cut]).unwrap();
    assert_eq!(file.jobs[0].completion, Some(Completion::Truncated));
    let result = file.jobs[0].result.as_ref().unwrap();
    // SCF Done:  E(RB3LYP) =  -382.308266602     A.U. after    1 cycles
    assert_eq!(result.energy_hartree, Some(-382.308266602));
    assert!(result.vibrations.is_none());
}

#[test]
fn termination_like_titles_do_not_end_jobs() {
    // Replace "Title Card Required" in the GOPT fixture in memory, keeping its real
    // "Normal termination of Gaussian 16 at Wed Apr  4 10:19:28 2018." record.
    let text = String::from_utf8(fixture(GOPT)).unwrap();
    for title in [
        "Normal termination benchmark",
        "Error termination benchmark",
    ] {
        let changed = text.replacen("Title Card Required", title, 1);
        let file = read("termination-title.log", changed.as_bytes()).unwrap();
        assert_eq!(file.jobs.len(), 1);
        let job = &file.jobs[0];
        assert_eq!(job.completion, Some(Completion::Normal));
        // SCF Done:  E(RB3LYP) =  -382.308266602     A.U. after    7 cycles
        assert_eq!(job.steps[4].energy_hartree, Some(-382.308266602));
    }
}

#[test]
fn truncated_standard_orientation_keeps_the_input_step() {
    // Cut the GOPT fixture in memory after the fourth "Input orientation:" table, and
    // then again inside its following "Standard orientation:" table.
    let text = String::from_utf8(fixture(GOPT)).unwrap();
    let (input, _) = text.match_indices("Input orientation:").nth(3).unwrap();
    let (end, separator) = text[input..]
        .match_indices("---------------------------------------------------------------------")
        .nth(2)
        .unwrap();
    let input_end = input + end + separator.len();
    let standard = text[input_end..].find("Standard orientation:").unwrap() + input_end;
    for cut in [input_end, standard + "Standard orientation:".len()] {
        for heading in ["Input orientation:", "Z-Matrix orientation:"] {
            let changed = text[..cut].replace("Input orientation:", heading);
            let file = read("interrupted-orientation.log", changed.as_bytes()).unwrap();
            let job = &file.jobs[0];
            assert_eq!(job.completion, Some(Completion::Truncated));
            assert_eq!(job.steps.len(), 4);
            let last = &job.steps[3];
            // 1          6           0       -0.075714   -0.000000    0.027160
            assert_eq!(last.frame.positions_angstrom[0], [-0.075714, 0.0, 0.027160]);
            assert_eq!(last.energy_hartree, None);
            let result = job.result.as_ref().unwrap();
            assert_eq!(result.frame, job.steps[2].frame.hash());
            assert_ne!(result.frame, last.frame.hash());
            // SCF Done:  E(RB3LYP) =  -382.308239279     A.U. after   10 cycles
            assert_eq!(result.energy_hartree, Some(-382.308239279));
        }
    }
}

#[test]
fn complete_hpmodes_survive_a_cut_in_the_rounded_copy() {
    // Cut the IR fixture in memory after line 1517 ("AU ... BG ... AU"), then
    // after line 1543 ("4 ... 5 ... 6"), before the second panel's frequencies.
    let text = String::from_utf8(fixture(IR)).unwrap();
    let original = open(IR);
    for cut in [1517, 1543] {
        let changed = text.lines().take(cut).collect::<Vec<_>>().join("\n");
        let file = read("interrupted-rounded-modes.log", changed.as_bytes()).unwrap();
        let job = &file.jobs[0];
        assert_eq!(job.completion, Some(Completion::Truncated));
        let result = job.result.as_ref().unwrap();
        assert_eq!(result.frame, job.steps[0].frame.hash());
        let modes = result.vibrations.as_ref().unwrap();
        // Frequencies ---  3467.0890 3470.0274 3548.3199 3548.3320 (modes 51–54)
        assert_eq!(modes.frequencies_cm1.len(), 54);
        assert_eq!(modes.frequencies_cm1[53], 3548.3320);
        assert_eq!(modes.modes.len(), 54 * 60);
        assert_eq!(
            Some(modes),
            original.jobs[0]
                .result
                .as_ref()
                .unwrap()
                .vibrations
                .as_ref()
        );
    }
}

#[test]
fn oversized_frequency_rows_are_rejected_before_number_parsing() {
    // In memory, replace a real IR frequency row in each precision. The last
    // token is invalid, so the error must report the column count first.
    let text = String::from_utf8(fixture(IR)).unwrap();
    for (line, original) in [
        (
            777,
            "Frequencies ---    53.1981   84.7415  149.4005  179.3403  263.3734",
        ),
        (
            1518,
            "Frequencies --     53.1981                84.7415               149.4005",
        ),
    ] {
        let label = original
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let changed = text.replacen(
            original,
            &format!("{label}{} bad", " 53.1981".repeat(99_999)),
            1,
        );
        assert_eq!(
            read("oversized-panel.log", changed.as_bytes()).unwrap_err(),
            Error::at(line, "invalid Gaussian normal mode count in a panel")
        );
    }

    // Enlarge the IR fixture's geometry in memory by repeating its carbon row
    // "19          6           0        1.325389    0.457058   -0.000000".
    // Keep output through "SCF Done:  E(RB3LYP) =  -382.308266602", then
    // use its harmonic-frequency heading with an oversized row.
    let mut lines: Vec<_> = text.lines().take(457).map(str::to_owned).collect();
    let (_, carbon) = lines[372].trim_start().split_once(' ').unwrap();
    let carbon = carbon.to_owned();
    lines.splice(
        374..374,
        (21..=MAX_HESSIAN_ATOMS + 1).map(|i| format!("{i} {carbon}")),
    );
    lines.push(text.lines().nth(770).unwrap().to_owned());
    lines.push(format!("Frequencies ---{}", " 53.1981".repeat(100_000)));
    let file = read("oversized-geometry.log", lines.join("\n").as_bytes()).unwrap();
    let job = &file.jobs[0];
    assert_eq!(job.kind, JobKind::Frequencies);
    assert_eq!(job.steps[0].frame.numbers.len(), MAX_HESSIAN_ATOMS + 1);
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.task, Task::Energy);
    assert_eq!(result.energy_hartree, Some(-382.308266602));
    assert!(result.vibrations.is_none());
    assert_eq!(
        file.warnings,
        [format!(
            "Job 1: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms"
        )]
    );
}

#[test]
fn cut_unrestricted_orbitals_without_beta_are_omitted() {
    // Change the GOPT route "#p b3lyp/sto-3g opt" to UB3LYP in memory and cut
    // after its final alpha eigenvalues, where beta levels would print.
    let text = String::from_utf8(fixture(GOPT)).unwrap();
    let last_alpha = text.rfind("Alpha virt. eigenvalues --").unwrap();
    let cut = last_alpha + text[last_alpha..].find('\n').unwrap();
    let method_in_route = text[..cut].replacen("b3lyp/sto-3g", "ub3lyp/sto-3g", 1);
    // With a general basis, take the spin label from the modified SCF Done record.
    let method_in_scf = text[..cut]
        .replacen("b3lyp/sto-3g", "ub3lyp gen", 1)
        .replace("SCF Done:  E(RB3LYP)", "SCF Done:  E(UB3LYP)");
    for (changed, method) in [(method_in_route, Some("ub3lyp")), (method_in_scf, None)] {
        let file = read("interrupted-uhf-population.log", changed.as_bytes()).unwrap();
        let job = &file.jobs[0];
        assert_eq!(job.completion, Some(Completion::Truncated));
        assert_eq!(job.method.as_deref(), method);
        let result = job.result.as_ref().unwrap();
        // Charge =  0 Multiplicity = 1
        assert_eq!(job.result_step().unwrap().frame.multiplicity, 1);
        // SCF Done:  E(RB3LYP) =  -382.308266602     A.U. after    7 cycles
        assert_eq!(result.energy_hartree, Some(-382.308266602));
        assert!(result.orbitals.is_none());
    }
}

#[test]
fn appended_gaussian_invocations_keep_their_versions() {
    // Concatenate the complete methane and GOPT fixtures in memory.
    let mut combined = fixture(METHANE);
    combined.push(b'\n');
    combined.extend(fixture(GOPT));
    let file = read("appended-gaussian.log", &combined).unwrap();
    // Gaussian 03, Revision D.01, and Gaussian 16, Revision A.03,
    assert_eq!(file.program_version.as_deref(), Some("03 Revision D.01"));
    assert_eq!(file.jobs.len(), 2);
    for (job, version) in file
        .jobs
        .iter()
        .zip(["03 Revision D.01", "16 Revision A.03"])
    {
        assert_eq!(job.completion, Some(Completion::Normal));
        assert_eq!(
            job.result
                .as_ref()
                .unwrap()
                .provenance
                .program_version
                .as_deref(),
            Some(version)
        );
    }
}

#[test]
fn job_limit_counts_invocations_without_geometry() {
    let text = String::from_utf8(fixture(MP2)).unwrap();
    let invocation = text.lines().next().unwrap();
    let termination = text.lines().last().unwrap();
    let limit = crate::qcread::MAX_JOBS;
    // Keep the real water result, then append invocations which printed no geometry.
    // Exercise jobs ending at another invocation, at termination, and at EOF.
    for ending in ["", termination] {
        let job = format!("{invocation}\n{ending}\n");
        let mut combined = format!("{text}\n{}", job.repeat(limit - 1));
        let file = read("many-gaussian.log", combined.as_bytes()).unwrap();
        assert_eq!(file.jobs.len(), limit);
        assert!(file.jobs[0].result.is_some());
        assert!(file.jobs[1..].iter().all(|job| job.steps.is_empty()));

        combined.push_str(&job);
        assert_eq!(
            read("many-gaussian.log", combined.as_bytes()).unwrap_err(),
            Error::Unsupported(format!("The file holds more than {limit} Gaussian jobs"))
        );
    }
}
