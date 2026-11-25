//! Spot values below quote the supplied Gaussian files, rather than another parser.
use crate::qcread::tests::{fixture, open};
use crate::qcread::{Error, JobKind, read};
use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, Beta, ChargeScheme, MAX_BASIS_FUNCTIONS, MAX_COEFFICIENTS, Source, Status,
    Task,
};

#[test]
fn fchk_g09_carbon_bigbasis_keeps_80_levels_without_unverified_pure_f_g_shapes() {
    let file = open("cclib/g09_C_bigbasis.fchk");
    let job = &file.jobs[0];
    let frame = &job.steps[0].frame;
    let result = job.result.as_ref().unwrap();
    let orbitals = result.orbitals.as_ref().unwrap();
    // Lines 3,7–13: atoms=1, charge=0, multiplicity=1, Nalpha=Nbeta=3,
    // "Number of basis functions ... 80", "Number of independent functions ... 80".
    assert_eq!(frame.numbers, [6]);
    assert_eq!((frame.charge, frame.multiplicity), (0, 1));
    assert_eq!(job.kind, JobKind::SinglePoint);
    assert_eq!(job.method.as_deref(), Some("RHF"));
    assert_eq!(job.basis.as_deref(), Some("Aug-CC-pVQZ"));
    assert_eq!(result.task, Task::Energy);
    assert_eq!(result.status, Status::Converged);
    assert!(job.completion.is_none());
    assert_eq!(job.index, 0);
    // Shell types, lines 52–55, include four -2, three -3, and two -4 records.
    // The pure f/g shells prevent keeping any shapes, including the verified pure d.
    assert!(orbitals.basis.is_none());
    assert_eq!(orbitals.alpha.energies_hartree.len(), 80);
    assert!(orbitals.alpha.rows.is_empty());
    assert!(orbitals.alpha.coefficients.is_empty());
    assert!(matches!(orbitals.beta, Beta::Restricted));
    assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 6.0);
    // Alpha Orbital Energies, occupied level 3 and virtual level 4:
    // "-3.58430162E-01  1.88580582E-02". Restricted beta shares this spectrum.
    assert_eq!(orbitals.alpha.energies_hartree[2], -0.358430162);
    assert_eq!(orbitals.alpha.energies_hartree[3], 0.0188580582);
    // "Total Energy ... -3.760454245753944E+01" (line 135).
    assert_eq!(result.energy_hartree, Some(-37.60454245753944));
    assert!(result.vibrations.is_none()); // No frequency record in this SP file.
    assert_eq!(
        file.warnings,
        [
            "Orbital shapes left out: Gaussian pure f and pure g functions have no verified convention here; orbital energies are kept"
        ]
    );
    assert!(matches!(&result.provenance.source,
        Source::Imported { format, file_name, job: 0, file_sha256: Some(hash) }
        if format == "Gaussian fchk" && file_name == "g09_C_bigbasis.fchk" && hash.len() == 64));
    assert_eq!(result.frame, frame.hash());

    // Remove only "Alpha MO coefficients" from this fixture. Its shell types still
    // identify the unverified functions and all 80 recorded energies are available.
    let levels_only =
        remove_real_array(&text("cclib/g09_C_bigbasis.fchk"), "Alpha MO coefficients");
    let levels_only = read("carbon-levels.fchk", levels_only.as_bytes()).unwrap();
    assert_eq!(
        levels_only.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref(),
        Some(orbitals)
    );
    assert_eq!(levels_only.warnings, file.warnings);
}

#[test]
fn fchk_g09_dvb_ir_has_hessian_without_reported_normal_modes() {
    let file = open("cclib/g09_dvb_ir.fchk");
    let job = &file.jobs[0];
    let frame = &job.steps[0].frame;
    let result = job.result.as_ref().unwrap();
    let orbitals = result.orbitals.as_ref().unwrap();
    // Lines 3,7–13: 20 atoms, charge 0, multiplicity 1, 35 alpha/35 beta,
    // 60 basis functions and 60 independent functions.
    assert_eq!(frame.numbers.len(), 20);
    assert_eq!((frame.charge, frame.multiplicity), (0, 1));
    assert_eq!(frame.numbers.iter().filter(|&&z| z == 6).count(), 10);
    assert_eq!(job.kind, JobKind::Frequencies);
    assert_eq!(job.method.as_deref(), Some("RB3LYP"));
    assert_eq!(job.basis.as_deref(), Some("STO-3G"));
    assert_eq!(result.task, Task::Hessian);
    assert_eq!(
        orbitals
            .basis
            .as_ref()
            .unwrap()
            .shells
            .iter()
            .map(|s| s.n_functions())
            .sum::<usize>(),
        60
    );
    assert_eq!(orbitals.alpha.energies_hartree.len(), 60);
    assert_eq!(orbitals.alpha.rows.len(), 60);
    assert!(matches!(orbitals.beta, Beta::Restricted));
    assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 70.0);
    // Alpha Orbital Energies, levels 35 and 36: "-1.53085082E-01  3.74226642E-02".
    assert_eq!(orbitals.alpha.energies_hartree[34], -0.153085082);
    assert_eq!(orbitals.alpha.energies_hartree[35], 0.0374226642);
    // "Total Energy ... -3.823082665785660E+02" (line 269).
    assert_eq!(result.energy_hartree, Some(-382.308266578566));
    // Coordinates line 27 starts "5.09243398E-01 -2.66468392E+00" in bohr.
    assert_eq!(
        frame.positions_angstrom[0][0],
        0.509243398 * ANGSTROM_PER_BOHR
    );
    assert_eq!(
        frame.positions_angstrom[0][1],
        -2.66468392 * ANGSTROM_PER_BOHR
    );
    // Cartesian Gradient line 1465 begins "-3.72986025E-05  3.20859841E-05";
    // these are energy derivatives, retained without a force sign reversal.
    assert_eq!(result.gradient_hartree_per_bohr[0], -3.72986025e-5);
    assert_eq!(result.gradient_hartree_per_bohr[1], 3.20859841e-5);
    // Force Constants line 1478: lower triangle "7.26306291E-01 -3.50839215E-03
    // 7.06184792E-01 2.47363363E-12 -2.54499185E-12 1.57916727E-01".
    let hessian = &result.hessian_hartree_per_bohr2;
    assert_eq!(hessian.len(), 60 * 60);
    assert_eq!(hessian[0], 0.726306291);
    assert_eq!(hessian[1], -0.00350839215);
    assert_eq!(hessian[60], -0.00350839215);
    assert_eq!(hessian[61], 0.706184792);
    assert_eq!(hessian[2 * 60 + 2], 0.157916727);
    assert_eq!(hessian[2 * 60], hessian[2]);
    // G09 ends after Polarizability: no Number of Normal Modes, Vib-E2 or Vib-Modes.
    assert!(result.vibrations.is_none());
    assert!(file.warnings.is_empty());
}

#[test]
fn fchk_g09_unrestricted_dvb_has_separate_spin_spectra() {
    let file = open("cclib/g09_dvb_un_sp.fchk");
    let job = &file.jobs[0];
    let frame = &job.steps[0].frame;
    let result = job.result.as_ref().unwrap();
    let orbitals = result.orbitals.as_ref().unwrap();
    // Lines 3,7–13: 20 atoms, charge +1, multiplicity 2, 35 alpha/34 beta,
    // 60 basis functions and 60 independent functions.
    assert_eq!(frame.numbers.len(), 20);
    assert_eq!((frame.charge, frame.multiplicity), (1, 2));
    assert_eq!(job.kind, JobKind::SinglePoint);
    assert_eq!(job.method.as_deref(), Some("UB3LYP"));
    assert_eq!(
        orbitals
            .basis
            .as_ref()
            .unwrap()
            .shells
            .iter()
            .map(|s| s.n_functions())
            .sum::<usize>(),
        60
    );
    let Beta::Unrestricted(beta) = &orbitals.beta else {
        panic!("Expected beta spectrum");
    };
    for set in [&orbitals.alpha, beta] {
        assert_eq!(set.energies_hartree.len(), 60);
        assert_eq!(set.rows.len(), 60);
        assert_eq!(set.coefficients.len(), 3600);
        assert!(set.occupations.iter().all(|&n| n == 0.0 || n == 1.0));
    }
    assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 35.0);
    assert_eq!(beta.occupations.iter().sum::<f64>(), 34.0);
    // Alpha levels 35/36: "-3.69758157E-01 -1.77496292E-01";
    // beta levels 34/35: "-3.88816504E-01 -3.00528573E-01".
    assert_eq!(orbitals.alpha.energies_hartree[34], -0.369758157);
    assert_eq!(orbitals.alpha.energies_hartree[35], -0.177496292);
    assert_eq!(beta.energies_hartree[33], -0.388816504);
    assert_eq!(beta.energies_hartree[34], -0.300528573);
    // "Total Energy ... -3.820813952041232E+02" (line 269).
    assert_eq!(result.energy_hartree, Some(-382.0813952041232));
    assert!(result.vibrations.is_none());
    assert!(file.warnings.is_empty());
}

#[test]
fn fchk_g09_water_mp2_uses_total_energy_and_splits_sp_shell() {
    let file = open("cclib/g09_water_mp2.fchk");
    let job = &file.jobs[0];
    let frame = &job.steps[0].frame;
    let result = job.result.as_ref().unwrap();
    let orbitals = result.orbitals.as_ref().unwrap();
    // Lines 3,7–13: O,H,H (3 atoms), charge 0, multiplicity 1, 5 alpha/5 beta,
    // 7 basis functions and 7 independent functions.
    assert_eq!(frame.numbers, [8, 1, 1]);
    assert_eq!((frame.charge, frame.multiplicity), (0, 1));
    assert_eq!(job.method.as_deref(), Some("RMP2-FC"));
    assert_eq!(job.basis.as_deref(), Some("STO-3G"));
    let shells = &orbitals.basis.as_ref().unwrap().shells;
    assert_eq!(shells.iter().map(|s| s.n_functions()).sum::<usize>(), 7);
    assert_eq!(
        shells
            .iter()
            .map(|s| (s.atom, s.l, s.pure))
            .collect::<Vec<_>>(),
        [
            (0, 0, false),
            (0, 0, false),
            (0, 1, false),
            (1, 0, false),
            (2, 0, false)
        ]
    );
    assert_eq!(shells[1].exponents_per_bohr2, shells[2].exponents_per_bohr2);
    // Contraction coefficients line 63 has -9.99672292E-02 for the SP s part;
    // P(S=P) Contraction coefficients line 67 has 1.55916275E-01 for its p part.
    assert_eq!(shells[1].coefficients[0], -0.0999672292);
    assert_eq!(shells[2].coefficients[0], 0.155916275);
    assert_eq!(orbitals.alpha.energies_hartree.len(), 7);
    assert_eq!(orbitals.alpha.rows.len(), 7);
    assert_eq!(orbitals.alpha.coefficients.len(), 49);
    assert!(matches!(orbitals.beta, Beta::Restricted));
    // Alpha levels 5/6: "-3.88214643E-01  5.70748219E-01"; beta shares them.
    assert_eq!(orbitals.alpha.energies_hartree[4], -0.388214643);
    assert_eq!(orbitals.alpha.energies_hartree[5], 0.570748219);
    // SCF Energy=-7.496432879130536E+01; Total Energy=-7.500228212778517E+01.
    assert_eq!(result.energy_hartree, Some(-7.500228212778517E+01));
    // The very first two MO rows are separate seven-AO blocks, not AO-major columns.
    // Alpha MO coefficients lines 134–135: first=9.94202990E-01,
    // row 2 first=-2.34217947E-01.
    assert_eq!(orbitals.alpha.coefficients[0], 0.994202990);
    assert_eq!(orbitals.alpha.coefficients[7], -0.234217947);
    // Mulliken Charges line 164: -3.19888662E-01 1.59944331E-01 1.59944331E-01.
    assert_eq!(result.charges[0].scheme, ChargeScheme::Mulliken);
    assert_eq!(
        result.charges[0].values,
        [-0.319888662, 0.159944331, 0.159944331]
    );
    // Dipole Moment line 192 z=-6.29265630E-01, already in e bohr.
    assert_eq!(result.dipole_e_bohr.unwrap()[2], -0.629265630);
    assert!(result.vibrations.is_none());
    assert!(file.warnings.is_empty());
}

#[test]
fn fchk_sp_coefficients_copy_complete_rows_in_level_order() {
    let file = open("cclib/g09_water_mp2.fchk");
    let basis = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap()
        .basis
        .as_ref()
        .unwrap();
    assert!(basis.shells.iter().all(|shell| shell.l < 2));
    // Distinct signed coefficients expose any changes to the s/p component order.
    // The selected level order must still move each complete seven-AO row together.
    let values = (0..21).map(|i| f64::from(i - 10)).collect::<Vec<_>>();
    let levels = [(2, -3.0), (0, -2.0), (1, -1.0)];
    let actual = super::canonical_coefficients(basis, &values, 7, &levels).unwrap();
    assert_eq!(
        actual,
        [&values[14..21], &values[..7], &values[7..14]].concat()
    );
}

#[test]
fn fchk_g16_dvb_ir_vibration_blocks_match_quoted_gaussian_output() {
    let file = open("cclib/g16_dvb_ir.fchk");
    let job = &file.jobs[0];
    let frame = &job.steps[0].frame;
    let result = job.result.as_ref().unwrap();
    let orbitals = result.orbitals.as_ref().unwrap();
    // Lines 3,11–17: 20 atoms, charge 0, multiplicity 1, 35 alpha/35 beta,
    // 60 basis functions and 60 independent functions.
    assert_eq!(frame.numbers.len(), 20);
    assert_eq!((frame.charge, frame.multiplicity), (0, 1));
    assert_eq!(job.kind, JobKind::Frequencies);
    assert_eq!(
        orbitals
            .basis
            .as_ref()
            .unwrap()
            .shells
            .iter()
            .map(|s| s.n_functions())
            .sum::<usize>(),
        60
    );
    assert_eq!(orbitals.alpha.energies_hartree.len(), 60);
    assert_eq!(orbitals.alpha.rows.len(), 60);
    assert!(matches!(orbitals.beta, Beta::Restricted));
    // Alpha occupied level 35=-1.53084448E-01; virtual level 36=3.74165639E-02.
    assert_eq!(orbitals.alpha.energies_hartree[34], -0.153084448);
    assert_eq!(orbitals.alpha.energies_hartree[35], 0.0374165639);
    // Total Energy line 441: -3.823082666020143E+02.
    assert_eq!(result.energy_hartree, Some(-382.3082666020143));
    // Gaussian Version C N=2 (line 5417): "ES64L-G16RevA.03        ".
    assert_eq!(file.program.as_deref(), Some("Gaussian"));
    assert_eq!(file.program_version.as_deref(), Some("ES64L-G16RevA.03"));
    assert_eq!(result.provenance.program_version, file.program_version);
    // Route C N=5 (line 9): "#p b3lyp/sto-3g guess=read freq=hpmodes geom=allcheck".
    assert_eq!(
        job.keywords,
        ["#p b3lyp/sto-3g guess=read freq=hpmodes geom=allcheck"]
    );
    let v = result.vibrations.as_ref().unwrap();
    // Number of Normal Modes=54; Vib-E2 N=756=14*54. Its first four blocks
    // begin 5.31980918E+01, 3.22659371E+00, 5.38005349E-03, 3.41845151E-02.
    // g16_dvb_ir.out lines 1518–1521 independently print:
    // "Frequencies -- 53.1981 84.7415 149.4005"
    // "Red. masses -- 3.2266 2.4837 2.0822"
    // "Frc consts  -- 0.0054 0.0105 0.0274"
    // "IR Inten    -- 0.0342 0.0000 0.3727".
    assert_eq!(v.frequencies_cm1.len(), 54);
    assert_eq!(v.frequencies_cm1[0], 53.1980918);
    assert_eq!(v.frequencies_cm1[53], 3548.33202);
    assert_eq!(v.reduced_masses_amu[0], 3.22659371);
    assert_eq!(v.reduced_masses_amu[53], 1.11823333);
    assert_eq!(v.ir_intensities_km_mol[0], 0.0341845151);
    assert_eq!(v.ir_intensities_km_mol[53], 2.15655626e-15);
    // Output lines 1977–1980 finish at 3548.3320 cm^-1 with IR intensity 0.0000.
    assert_eq!(v.modes.len(), 54 * 60);
    assert_eq!(v.masses_amu.len(), 20);
    assert_eq!(v.masses_amu[0], 12.0);
    assert_eq!(v.masses_amu[5], 1.00782504);
    for mode in v.modes.as_chunks::<60>().0 {
        assert!((mode.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() < 1e-12);
    }
    assert_eq!(v.imaginary_count(), 0);
    let output = String::from_utf8(fixture("cclib/g16_dvb_ir.out")).unwrap();
    for (label, values) in [
        ("Frequencies -- ", &v.frequencies_cm1),
        ("Red. masses -- ", &v.reduced_masses_amu),
        ("IR Inten    -- ", &v.ir_intensities_km_mol),
    ] {
        // A trailing space after two hyphens excludes the separate three-hyphen
        // high-precision frequency table. Compare all 54 printed four-decimal values.
        let reported = output
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix(label))
            .flat_map(|line| line.split_whitespace().map(|n| n.parse::<f64>().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(reported.len(), 54, "{label}");
        for (mode, (&actual, &printed)) in values.iter().zip(&reported).enumerate() {
            assert!(
                (actual - printed).abs() <= 0.0000501,
                "{label} mode {}: {actual} versus {printed}",
                mode + 1
            );
        }
    }
    assert!(file.warnings.is_empty());
}

// Mutations below start with supplied fixture text and alter only explicitly named records.
fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

fn replace_real_array(source: &str, name: &str, transform: impl FnOnce(&mut Vec<f64>)) -> String {
    let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
    let start = lines
        .iter()
        .position(|line| line.starts_with(name))
        .unwrap();
    let count = lines[start]
        .split_once("N=")
        .unwrap()
        .1
        .trim()
        .parse::<usize>()
        .unwrap();
    let rows = count.div_ceil(5);
    let mut values = lines[start + 1..=start + rows]
        .iter()
        .flat_map(|line| {
            line.split_whitespace()
                .map(|value| value.parse::<f64>().unwrap())
        })
        .collect::<Vec<_>>();
    transform(&mut values);
    let mut replacement = vec![format!("{name:<40}   R   N={:>12}", values.len())];
    replacement.extend(values.chunks(5).map(|row| {
        row.iter()
            .map(|value| format!("{value:16.8E}"))
            .collect::<String>()
    }));
    lines.splice(start..=start + rows, replacement);
    lines.join("\n") + "\n"
}

fn replace_integer(source: &str, name: &str, value: impl std::fmt::Display) -> String {
    source
        .lines()
        .map(|line| {
            if line.starts_with(name) {
                format!("{name:<40}   I     {value:>12}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn replace_integer_array(
    source: &str,
    name: &str,
    transform: impl FnOnce(&mut Vec<i64>),
) -> String {
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.starts_with(name))
        .unwrap();
    let count = lines[start]
        .split_once("N=")
        .unwrap()
        .1
        .trim()
        .parse::<usize>()
        .unwrap();
    let rows = count.div_ceil(6);
    let mut values = lines[start + 1..=start + rows]
        .iter()
        .flat_map(|line| line.split_whitespace().map(|n| n.parse::<i64>().unwrap()))
        .collect::<Vec<_>>();
    transform(&mut values);
    let mut replacement = vec![format!("{name:<40}   I   N={:>12}", values.len())];
    replacement.extend(values.chunks(6).map(|row| {
        row.iter()
            .map(|value| format!("{value:12}"))
            .collect::<String>()
    }));
    lines.splice(start..=start + rows, replacement);
    lines.join("\n") + "\n"
}

fn remove_real_array(source: &str, name: &str) -> String {
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.starts_with(name))
        .unwrap();
    let count = lines[start]
        .split_once("N=")
        .unwrap()
        .1
        .trim()
        .parse::<usize>()
        .unwrap();
    lines.drain(start..=start + count.div_ceil(5));
    lines.join("\n") + "\n"
}

fn description(source: &str, calculation: &str, method: &str) -> String {
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    lines[1] = format!("{calculation:<10}{method:<30}STO-3G");
    lines.join("\n") + "\n"
}

#[test]
fn fchk_high_shell_shapes_require_gaussian_pure_d() {
    for (l, name) in [(2usize, "d"), (3, "f"), (4, "g"), (5, "h")] {
        for pure in [false, true] {
            // Water's Shell types "0 -1 0 0" becomes "0 +/-l 0 0".
            // Keep all twelve primitives and the four centres; widen each of the
            // seven MO rows to match the replacement shell's function count.
            let n_ao = 3 + if pure {
                2 * l + 1
            } else {
                (l + 1) * (l + 2) / 2
            };
            let source = replace_integer(
                &text("cclib/g09_water_mp2.fchk"),
                "Number of basis functions",
                n_ao,
            );
            let source = replace_integer_array(&source, "Shell types", |types| {
                types[1] = if pure { -(l as i64) } else { l as i64 };
            });
            let source = replace_real_array(&source, "Alpha MO coefficients", |values| {
                *values = values
                    .as_chunks::<7>()
                    .0
                    .iter()
                    .flat_map(|row| row.iter().copied().chain(vec![0.0; n_ao - 7]))
                    .collect();
            });
            let cases = if l == 2 && pure {
                // Rename the marker records without changing their type, width or values.
                let no_ilsw = source.replacen("\nILSW ", "\nXLSW ", 1);
                let no_iopcl = source.replacen("\nIOpCl ", "\nXOpCl ", 1);
                let neither = no_ilsw.replacen("\nIOpCl ", "\nXOpCl ", 1);
                let with_version = |source, version| {
                    format!("{source}{:<40}   C     {version}\n", "Gaussian Version")
                };
                vec![
                    (source.clone(), true),
                    (with_version(&neither, "ES64L-G16RevA.03"), true),
                    (with_version(&neither, "EM64L-G09RevD.01"), true),
                    // An explicit version overrides the marker-pair fallback.
                    (with_version(&source, "G03 C.02"), false),
                    (with_version(&source, ""), false),
                    (with_version(&source, "G160 A.03"), false),
                    (no_ilsw, false),
                    (no_iopcl, false),
                    (neither, false),
                ]
            } else {
                vec![(source, true)]
            };
            for (source, gaussian) in cases {
                let file = read("high-shell.fchk", source.as_bytes()).unwrap();
                let orbitals = file.jobs[0]
                    .result
                    .as_ref()
                    .unwrap()
                    .orbitals
                    .as_ref()
                    .unwrap();
                assert_eq!(orbitals.alpha.energies_hartree.len(), 7);
                assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 10.0);
                if gaussian && l == 2 && pure {
                    let shell = &orbitals.basis.as_ref().unwrap().shells[1];
                    assert_eq!((shell.l, shell.pure), (2, true));
                    assert_eq!(orbitals.alpha.rows, (0..7).collect::<Vec<_>>());
                    assert_eq!(orbitals.alpha.coefficients.len(), 7 * n_ao);
                    assert!(file.warnings.is_empty());
                } else {
                    assert!(orbitals.basis.is_none());
                    assert!(orbitals.alpha.rows.is_empty());
                    assert!(orbitals.alpha.coefficients.is_empty());
                    let label = if gaussian {
                        let representation = if pure { "pure" } else { "Cartesian" };
                        format!("Gaussian {representation} {name} functions")
                    } else {
                        "pure d functions in an fchk file whose Gaussian 09 or 16 origin could not be verified".into()
                    };
                    assert_eq!(
                        file.warnings,
                        [format!(
                            "Orbital shapes left out: {label} have no verified convention here; orbital energies are kept"
                        )]
                    );
                }
            }
        }
    }
}

#[test]
fn fchk_unrestricted_without_beta_records_does_not_invent_shared_orbitals() {
    // UB3LYP fixture line 2 and electron records 35/34 prove an unrestricted doublet.
    // Delete its actual beta spectrum and MO block, retaining the alpha data.
    let source = remove_real_array(&text("cclib/g09_dvb_un_sp.fchk"), "Beta Orbital Energies");
    let source = remove_real_array(&source, "Beta MO coefficients");
    for source in [
        source.clone(),
        // IROHF=0 plus unequal spin counts is sufficient even without a U method.
        description(&source, "SP", "B3LYP"),
        // The U method remains sufficient even when IROHF is absent.
        source.replace(
            "IROHF                                      I                0\n",
            "",
        ),
    ] {
        let file = read("incomplete-unrestricted.fchk", source.as_bytes()).unwrap();
        let result = file.jobs[0].result.as_ref().unwrap();
        assert!(result.orbitals.is_none());
        assert_eq!(result.energy_hartree, Some(-382.0813952041232));
        assert_eq!(
            file.warnings,
            ["Orbitals left out: unrestricted Gaussian orbital records have no beta spectrum"]
        );
    }
    // A U-prefixed singlet also cannot be inferred to have shared spatial orbitals.
    let source = description(&text("cclib/g09_water_mp2.fchk"), "SP", "UHF");
    let file = read("unrestricted-singlet.fchk", source.as_bytes()).unwrap();
    assert!(file.jobs[0].result.as_ref().unwrap().orbitals.is_none());
    assert_eq!(file.warnings.len(), 1);
}

#[test]
fn fchk_beta_energies_without_coefficients_remain_a_valid_spin_spectrum() {
    // Remove only the UB3LYP beta coefficient block. Its sixty beta energies stay.
    let source = remove_real_array(&text("cclib/g09_dvb_un_sp.fchk"), "Beta MO coefficients");
    let file = read("beta-levels.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    let Beta::Unrestricted(beta) = &orbitals.beta else {
        panic!("beta spectrum")
    };
    assert_eq!(beta.energies_hartree.len(), 60);
    assert_eq!(beta.occupations.iter().sum::<f64>(), 34.0);
    assert!(beta.rows.is_empty());
    assert_eq!(orbitals.alpha.rows.len(), 60);
    assert!(file.warnings.is_empty());
    // Retaining the beta coefficients while deleting its energies is malformed.
    let source = remove_real_array(&text("cclib/g09_dvb_un_sp.fchk"), "Beta Orbital Energies");
    assert!(
        read("missing-beta-levels.fchk", source.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("Beta MO coefficients require Beta Orbital Energies")
    );
}

#[test]
fn fchk_force_constants_are_only_a_calculated_hessian_for_frequency_jobs() {
    // The actual G09 Freq fixture has 1,830 packed constants, beginning
    // "7.26306291E-01 -3.50839215E-03". Change only its calculation field.
    let source = text("cclib/g09_dvb_ir.fchk");
    for calculation in ["FOpt", "POpt", "SP", "Force"] {
        let source = description(&source, calculation, "RB3LYP");
        let file = read("other-job.fchk", source.as_bytes()).unwrap();
        let result = file.jobs[0].result.as_ref().unwrap();
        assert!(result.hessian_hartree_per_bohr2.is_empty(), "{calculation}");
        assert_eq!(result.gradient_hartree_per_bohr.len(), 60);
        assert_eq!(result.energy_hartree, Some(-382.308266578566));
        assert!(file.warnings.is_empty());
        if calculation == "Force" {
            assert_eq!(result.task, Task::Gradient);
        }
    }
}

#[test]
fn fchk_job_status_zero_preserves_results_as_not_converged() {
    // Synthetic status: G16 fixture line 445 reports "Job Status ... 1".
    // Alter only that value; line 441 still has Total Energy -382.3082666020143.
    let source = replace_integer(&text("cclib/g16_dvb_ir.fchk"), "Job Status", 0);
    let file = read("not-converged.fchk", source.as_bytes()).unwrap();
    let result = file.jobs[0].result.as_ref().unwrap();
    assert!(
        matches!(&result.status, Status::NotConverged { reason } if reason.contains("Job Status"))
    );
    assert!(result.orbitals.is_some());
    assert!(result.vibrations.is_some());
    assert_eq!(result.hessian_hartree_per_bohr2.len(), 3600);
    assert_eq!(result.energy_hartree, Some(-382.3082666020143));
}

#[test]
fn fchk_job_status_error_preserves_results_as_failed() {
    // Synthetic status: G16 line 445 has "Job Status ... 1"; line 470 declares
    // 3,600 Alpha MO coefficients. Neither the spectrum nor Hessian is removed.
    let source = replace_integer(&text("cclib/g16_dvb_ir.fchk"), "Job Status", 3);
    let file = read("failed.fchk", source.as_bytes()).unwrap();
    let result = file.jobs[0].result.as_ref().unwrap();
    assert_eq!(
        result.status,
        Status::Failed {
            reason: "Gaussian job status 3 (error)".into()
        }
    );
    assert!(result.orbitals.is_some());
    assert!(result.vibrations.is_some());
    assert_eq!(result.hessian_hartree_per_bohr2.len(), 3600);
    assert_eq!(result.energy_hartree, Some(-382.3082666020143));
}

#[test]
fn fchk_job_status_in_progress_preserves_results_as_not_converged() {
    // Synthetic status: change only "Job Status ... 1" at G16 line 445 to -1.
    // Its 60 orbital energies (line 457) remain useful partial results.
    let source = replace_integer(&text("cclib/g16_dvb_ir.fchk"), "Job Status", -1);
    let file = read("in-progress.fchk", source.as_bytes()).unwrap();
    let result = file.jobs[0].result.as_ref().unwrap();
    assert_eq!(
        result.status,
        Status::NotConverged {
            reason: "job in progress".into()
        }
    );
    assert!(result.orbitals.is_some());
    assert!(result.vibrations.is_some());
    assert_eq!(result.hessian_hartree_per_bohr2.len(), 3600);
    assert_eq!(result.energy_hartree, Some(-382.3082666020143));
}

#[test]
fn fchk_job_status_success_and_absence_preserve_converged_results() {
    // Synthetic statuses: G16 line 445 has "Job Status ... 1". Status 2 and
    // absence retain the same successful result, including line 441's energy.
    let original = text("cclib/g16_dvb_ir.fchk");
    for source in [
        original.clone(),
        replace_integer(&original, "Job Status", 2),
        original
            .lines()
            .filter(|line| !line.starts_with("Job Status"))
            .collect::<Vec<_>>()
            .join("\n"),
    ] {
        let file = read("successful.fchk", source.as_bytes()).unwrap();
        let result = file.jobs[0].result.as_ref().unwrap();
        assert_eq!(result.status, Status::Converged);
        assert!(result.orbitals.is_some());
        assert!(result.vibrations.is_some());
        assert_eq!(result.energy_hartree, Some(-382.3082666020143));
    }
}

#[test]
fn fchk_natural_orbital_route_does_not_import_occupations_as_energies() {
    // Water lines 130–132 supply seven ordinary HF-reference orbital energies
    // beginning "-2.02437548E+01"; line 133 declares 49 Alpha MO coefficients.
    // A synthetic Route character record changes only the interpretation of its
    // orbital records; Total Energy and geometry are still ordinary fchk data.
    let source = text("cclib/g09_water_mp2.fchk");
    for keyword in [
        "Guess=NaturalOrbitals",
        "Guess=(Only,NaturalOrbitals)",
        "Guess=(Save,Only,NaturalOrbitals)",
        "Guess = ( Read , nAtOrB )",
        "Guess(Save,Only,NaturalOrbitals)",
        "Guess=Only NaturalOrbitals",
        "Guess=Only Pop=NaturalOrbitals",
        "SaveNO",
        "SaveNatural",
        "SaveNaturalOrbitals",
        "Pop=(Full,SaveNO)",
        "Pop=SaveNaturalTransitionOrbitals",
        "Pop=SaveNTO",
        "Pop=SaveNBOs",
        "Pop=SaveNLMOs",
        "Pop=SaveMixed",
    ] {
        let source = source.clone() + &format!("{:<40}   C     #p hf/sto-3g {keyword}\n", "Route");
        let file = read("natural.fchk", source.as_bytes()).unwrap();
        let result = file.jobs[0].result.as_ref().unwrap();
        assert!(result.orbitals.is_none(), "{keyword}");
        assert_eq!(result.energy_hartree, Some(-75.00228212778517));
        assert_eq!(
            file.warnings,
            [
                "Orbitals left out: natural-orbital records store occupations instead of orbital energies"
            ]
        );
    }
}

#[test]
fn fchk_population_natural_orbital_analysis_preserves_canonical_spectrum() {
    // Synthetic Route records on water's unchanged seven-level spectrum:
    // lines 130–132 begin -20.2437548 and finish 0.708591547. Pop requests an
    // analysis; explicit Guess natural options or orbital SAVE options replace MOs.
    let source = text("cclib/g09_water_mp2.fchk");
    let original = open("cclib/g09_water_mp2.fchk");
    let expected = original.jobs[0].result.as_ref().unwrap();
    for keyword in [
        "Pop=NO",
        "Pop=NaturalOrbitals",
        "pop = ( full , no )",
        "Pop=(Full,NaturalOrbitals)",
        "Guess=Only",
        "Pop=None",
        "Pop=Normal",
        "Pop=(Full,None)",
        "NaturalOrbitals",
        "NatOrb",
        "NoNaturalOrbitals",
        "SaveNormal",
        "OtherPop=NO",
        "OtherGuess=NaturalOrbitals",
        "Guess=Read Pop=NaturalOrbitals",
        "Guess=(Only,Read) Pop=(Full,NO)",
        "Pop=SaveMulliken",
        "Pop=SaveNPA",
    ] {
        let source = source.clone() + &format!("{:<40}   C     #p hf/sto-3g {keyword}\n", "Route");
        let file = read("ordinary.fchk", source.as_bytes()).unwrap();
        let actual = file.jobs[0].result.as_ref().unwrap();
        assert_eq!(actual.orbitals, expected.orbitals, "{keyword}");
        assert_eq!(actual.energy_hartree, expected.energy_hartree, "{keyword}");
        assert!(file.warnings.is_empty(), "{keyword}");
    }
}

#[test]
fn fchk_real_atomic_weights_identify_isotopes_in_the_result_frame() {
    // Water line 31 has "1.59949146E+01 1.00782504E+00 1.00782504E+00".
    // Default O-16/H-1 masses match the common nuclides and keep natural identity.
    let original = open("cclib/g09_water_mp2.fchk");
    assert!(original.jobs[0].steps[0].frame.isotopes.is_empty());
    // Replace only the masses: oxygen-18, deuterium, and unchanged ordinary H.
    let source = replace_real_array(
        &text("cclib/g09_water_mp2.fchk"),
        "Real atomic weights",
        |masses| {
            masses[0] = 17.9991596;
            masses[1] = 2.01410178;
        },
    );
    let file = read("isotopic.fchk", source.as_bytes()).unwrap();
    let frame = &file.jobs[0].steps[0].frame;
    assert_eq!(frame.isotopes, [18, 2, 0]);
    assert_ne!(frame.hash(), original.jobs[0].steps[0].frame.hash());
    assert_eq!(file.jobs[0].result.as_ref().unwrap().frame, frame.hash());
    // A synthetic +0.49 amu perturbation from the common-nuclide mass is below
    // the requested half-amu threshold, even when ordinary rounding changes.
    let source = replace_real_array(&source, "Real atomic weights", |masses| {
        for (mass, number) in masses.iter_mut().zip([8, 1, 1]) {
            *mass = crate::chemistry::properties(&[crate::chemistry::AtomFacts {
                atomic_number: number,
                isotope: crate::chemistry::ELEMENTS[usize::from(number)].common_isotope,
                charge: 0,
                hydrogens: 0,
                radical_electrons: 0,
            }])
            .unwrap()
            .mass
                + 0.49;
        }
    });
    let file = read("near-natural.fchk", source.as_bytes()).unwrap();
    assert!(file.jobs[0].steps[0].frame.isotopes.is_empty());
}

#[test]
fn fchk_common_bromine_mass_and_half_amu_threshold_use_the_common_nuclide() {
    // Synthetic BrH2+ geometry from water: its Atomic numbers record (lines
    // 16–17) is "8 1 1" and Real atomic weights (lines 30–31) starts 15.9949146.
    // Substitute Br for O, keep an even electron count with charge +1, and remove
    // the seven water orbitals (lines 130–143); no Br calculation is claimed.
    let source = replace_integer_array(&text("cclib/g09_water_mp2.fchk"), "Atomic numbers", |v| {
        v[0] = 35;
    });
    let source = replace_integer(&source, "Charge", 1);
    let source = remove_real_array(&source, "Alpha Orbital Energies");
    let source = remove_real_array(&source, "Alpha MO coefficients");
    // Br-79's exact mass is 78.9183371 amu. Its natural-average mass (~79.904)
    // differs enough to make both the ordinary Br case and the +0.51 case wrong.
    for (mass, expected) in [
        (78.9183371, Vec::new()),
        (78.9183371 + 0.49, Vec::new()),
        (78.9183371 + 0.5, Vec::new()),
        (78.9183371 + 0.51, vec![79, 0, 0]),
        (78.9183371 - 0.5, Vec::new()),
        (78.9183371 - 0.51, vec![78, 0, 0]),
    ] {
        let source = replace_real_array(&source, "Real atomic weights", |masses| {
            masses[0] = mass;
        });
        let file = read("common-bromine.fchk", source.as_bytes()).unwrap();
        let frame = &file.jobs[0].steps[0].frame;
        assert_eq!(frame.numbers, [35, 1, 1]);
        assert_eq!(frame.isotopes, expected, "mass {mass}");
        assert_eq!(file.jobs[0].result.as_ref().unwrap().frame, frame.hash());
        assert!(file.warnings.is_empty());
    }
}

// Expand the actual seven-AO water fixture with one-primitive s functions on O.
// Keep its original seven MOs and add zero virtual rows and higher virtual energies.
// All format metadata remains dimensionally consistent; this is synthetic size data,
// not a claim that the added contractions represent a useful chemical basis.
fn expanded_water(n_ao: usize, n_mo: usize) -> String {
    let additional = n_ao - 7;
    let source = text("cclib/g09_water_mp2.fchk");
    let source = replace_integer(&source, "Number of basis functions", n_ao);
    let source = replace_integer(&source, "Number of independent functions", n_mo);
    let source = replace_integer(&source, "Number of contracted shells", 4 + additional);
    let source = replace_integer(&source, "Number of primitive shells", 12 + additional);
    let source = replace_integer_array(&source, "Shell types", |v| v.resize(4 + additional, 0));
    let source = replace_integer_array(&source, "Number of primitives per shell", |v| {
        v.resize(4 + additional, 1);
    });
    let source = replace_integer_array(&source, "Shell to atom map", |v| {
        v.resize(4 + additional, 1);
    });
    let source = replace_real_array(&source, "Primitive exponents", |v| {
        v.resize(12 + additional, 1.0);
    });
    let source = replace_real_array(&source, "Contraction coefficients", |v| {
        v.resize(12 + additional, 1.0);
    });
    let source = replace_real_array(&source, "P(S=P) Contraction coefficients", |v| {
        v.resize(12 + additional, 0.0);
    });
    let source = replace_real_array(&source, "Coordinates of each shell", |v| {
        for _ in 0..additional {
            v.extend_from_within(..3);
        }
    });
    let source = replace_real_array(&source, "Alpha Orbital Energies", |v| {
        for i in 7..n_mo {
            v.push(i as f64);
        }
    });
    replace_real_array(&source, "Alpha MO coefficients", |v| {
        let original = std::mem::take(v);
        v.resize(n_ao * n_mo, 0.0);
        for (row, original) in original.as_chunks::<7>().0.iter().enumerate() {
            v[row * n_ao..row * n_ao + 7].copy_from_slice(original);
        }
    })
}

#[test]
fn fchk_shape_budgets_drop_shapes_before_building_a_canonical_matrix() {
    // Independently exceed each actual qcdata shape budget. The coefficient-limit
    // case is ~65 MB, below MAX_FILE_BYTES, with a valid 2049-AO/2048-MO block.
    for (n_ao, n_mo, message) in [
        (MAX_BASIS_FUNCTIONS + 1, 7, "basis has"),
        (2049, 2048, "each spin has"),
    ] {
        if message == "each spin has" {
            assert!(n_ao <= MAX_BASIS_FUNCTIONS);
            assert!(n_ao * n_mo > MAX_COEFFICIENTS);
        }
        let source = expanded_water(n_ao, n_mo);
        assert!(source.len() < crate::qcread::MAX_FILE_BYTES);
        let file = read("oversized-shapes.fchk", source.as_bytes()).unwrap();
        let result = file.jobs[0].result.as_ref().unwrap();
        let orbitals = result.orbitals.as_ref().unwrap();
        assert!(orbitals.basis.is_none());
        assert!(orbitals.alpha.rows.is_empty());
        assert!(orbitals.alpha.coefficients.is_empty());
        assert_eq!(orbitals.alpha.energies_hartree.len(), n_mo);
        assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 10.0);
        assert_eq!(result.energy_hartree, Some(-75.00228212778517));
        assert_eq!(file.warnings.len(), 1);
        assert!(file.warnings[0].contains(message));
        assert!(file.warnings[0].contains("orbital energies are kept"));
    }
}

#[test]
fn fchk_independent_mos_can_be_fewer_than_aos() {
    // Water fixture: delete its last virtual energy and coefficient row, and change
    // Number of independent functions 7->6. The physical seven-AO basis is unchanged.
    let source = text("cclib/g09_water_mp2.fchk").replace(
        "Number of independent functions            I                7",
        "Number of independent functions            I                6",
    );
    let source = replace_real_array(&source, "Alpha Orbital Energies", |v| v.truncate(6));
    let source = replace_real_array(&source, "Alpha MO coefficients", |v| v.truncate(42));
    let file = read("fewer.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(orbitals.alpha.energies_hartree.len(), 6);
    assert_eq!(orbitals.alpha.rows, [0, 1, 2, 3, 4, 5]);
    assert_eq!(orbitals.alpha.coefficients.len(), 42);
    assert_eq!(
        orbitals
            .basis
            .as_ref()
            .unwrap()
            .shells
            .iter()
            .map(|s| s.n_functions())
            .sum::<usize>(),
        7
    );
    // Number of electrons is optional cross-check metadata; the spin counts remain.
    let source = source.replace(
        "Number of electrons                        I               10\n",
        "",
    );
    assert!(read("fewer-without-total.fchk", source.as_bytes()).is_ok());
}

#[test]
fn fchk_rohf_without_beta_uses_separate_occupations() {
    // Water fixture: remove one electron (charge 0->+1, multiplicity 1->2,
    // total 10->9, beta 5->4) and change IROHF 0->1. Keep the common MO rows.
    let source = text("cclib/g09_water_mp2.fchk")
        .replace(
            "Charge                                     I                0",
            "Charge                                     I                1",
        )
        .replace(
            "Multiplicity                               I                1",
            "Multiplicity                               I                2",
        )
        .replace(
            "Number of electrons                        I               10",
            "Number of electrons                        I                9",
        )
        .replace(
            "Number of beta electrons                   I                5",
            "Number of beta electrons                   I                4",
        )
        .replace(
            "IROHF                                      I                0",
            "IROHF                                      I                1",
        );
    // Both explicit IROHF and a RO method identify shared orbitals. An IROHF=0
    // unequal-spin variant is tested below as an incomplete unrestricted spectrum.
    let mut ro_method = source.lines().map(str::to_owned).collect::<Vec<_>>();
    ro_method[1] = format!("{:<10}{:<30}{}", "SP", "ROHF", "STO-3G");
    let ro_method = ro_method.join("\n").replace(
        "IROHF                                      I                1\n",
        "",
    );
    for source in [source, ro_method] {
        let file = read("rohf.fchk", source.as_bytes()).unwrap();
        let orbitals = file.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap();
        let Beta::SameOrbitals { occupations } = &orbitals.beta else {
            panic!("Expected shared orbitals");
        };
        assert_eq!(
            orbitals.alpha.occupations,
            [1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]
        );
        assert_eq!(occupations, &[1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
    }
    // The singlet fixture with only IROHF 0->1 also uses separate 0/1 occupations.
    let source = text("cclib/g09_water_mp2.fchk").replace(
        "IROHF                                      I                0",
        "IROHF                                      I                1",
    );
    let file = read("rohf-singlet.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(
        orbitals.alpha.occupations,
        [1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]
    );
    assert!(
        matches!(&orbitals.beta, Beta::SameOrbitals { occupations } if occupations == &orbitals.alpha.occupations)
    );
}

#[test]
fn fchk_ecp_core_counts_follow_nuclear_charges() {
    // Water fixture: replace oxygen Z=8 by sulfur Z=16 while retaining its effective
    // nuclear charge 8 and ten explicit electrons. This represents an eight-electron ECP.
    let source = text("cclib/g09_water_mp2.fchk").replace(
        "           8           1           1\n",
        "          16           1           1\n",
    );
    let file = read("ecp.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(file.jobs[0].steps[0].frame.numbers, [16, 1, 1]);
    assert_eq!(orbitals.core_electrons, [8, 0, 0]);
}

#[test]
fn fchk_energy_sort_keeps_shared_spin_occupations_and_mo_rows_together() {
    let original = open("cclib/g09_water_mp2.fchk");
    let original = original.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    // Water fixture: change only IROHF 0->1 and swap occupied energy 5 with virtual
    // energy 6. The source MO rows and the first-five-per-spin occupations stay in place.
    let source = text("cclib/g09_water_mp2.fchk").replace(
        "IROHF                                      I                0",
        "IROHF                                      I                1",
    );
    let source = replace_real_array(&source, "Alpha Orbital Energies", |v| v.swap(4, 5));
    let file = read("source-order.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(
        orbitals.alpha.energies_hartree,
        original.alpha.energies_hartree
    );
    assert_eq!(
        orbitals.alpha.occupations,
        [1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0]
    );
    let Beta::SameOrbitals { occupations } = &orbitals.beta else {
        panic!("Expected shared orbitals");
    };
    assert_eq!(occupations, &orbitals.alpha.occupations);
    assert_eq!(
        &orbitals.alpha.coefficients[4 * 7..5 * 7],
        &original.alpha.coefficients[5 * 7..6 * 7]
    );
    assert_eq!(
        &orbitals.alpha.coefficients[5 * 7..6 * 7],
        &original.alpha.coefficients[4 * 7..5 * 7]
    );
}

#[test]
fn fchk_unproven_cartesian_d_keeps_levels_and_names_the_functions() {
    // Carbon fixture: switch one five-function pure d shell to six-function Cartesian d,
    // increase AO count 80->81, and add a zero column to every MO. Energies are unchanged.
    let source = text("cclib/g09_C_bigbasis.fchk")
        .replace(
            "Number of basis functions                  I               80",
            "Number of basis functions                  I               81",
        )
        .replacen(
            "           1           1           1           1           1          -2",
            "           1           1           1           1           1           2",
            1,
        );
    let source = replace_real_array(&source, "Alpha MO coefficients", |v| {
        *v = v
            .as_chunks::<80>()
            .0
            .iter()
            .flat_map(|row| row.iter().copied().chain([0.0]))
            .collect();
    });
    let file = read("cartesian.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert!(orbitals.basis.is_none());
    assert!(orbitals.alpha.rows.is_empty());
    assert!(orbitals.alpha.coefficients.is_empty());
    assert_eq!(orbitals.alpha.energies_hartree.len(), 80);
    assert_eq!(
        file.warnings,
        [
            "Orbital shapes left out: Gaussian Cartesian d, pure f and pure g functions have no verified convention here; orbital energies are kept"
        ]
    );
}

#[test]
fn fchk_unproven_pure_h_keeps_levels_and_names_the_functions() {
    // Carbon fixture: change its first pure g to pure h (-4->-5), increase AO count
    // 80->82, and append two zero coefficient columns to each MO. No energies change.
    let source = text("cclib/g09_C_bigbasis.fchk")
        .replace(
            "Number of basis functions                  I               80",
            "Number of basis functions                  I               82",
        )
        .replacen("          -4          -4", "          -5          -4", 1);
    let source = replace_real_array(&source, "Alpha MO coefficients", |v| {
        *v = v
            .as_chunks::<80>()
            .0
            .iter()
            .flat_map(|row| row.iter().copied().chain([0.0, 0.0]))
            .collect();
    });
    let file = read("pure-h.fchk", source.as_bytes()).unwrap();
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert!(orbitals.basis.is_none());
    assert!(orbitals.alpha.rows.is_empty());
    assert!(orbitals.alpha.coefficients.is_empty());
    assert_eq!(orbitals.alpha.energies_hartree.len(), 80);
    assert_eq!(
        file.warnings,
        [
            "Orbital shapes left out: Gaussian pure f, pure g and pure h functions have no verified convention here; orbital energies are kept"
        ]
    );
}

#[test]
fn fchk_rejects_truncated_oversized_nonfinite_and_mismatched_records() {
    let source = text("cclib/g09_water_mp2.fchk");
    // Truncate the actual fixture in its coordinate record; no closing newline is required.
    let cutoff = source.find("Current cartesian coordinates").unwrap();
    let truncated =
        &source[..cutoff + "Current cartesian coordinates              R   N=           9\n".len()];
    assert!(
        read("short.fchk", truncated.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("Current cartesian coordinates array is truncated")
    );
    // The same record claims 999999999 values in a small fixture.
    let oversized = source.replace(
        "Current cartesian coordinates              R   N=           9",
        "Current cartesian coordinates              R   N=   999999999",
    );
    assert!(
        read("large.fchk", oversized.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("declared Current cartesian coordinates array is larger than the file")
    );
    // Replace one original coordinate, -2.46519033E-32, with a width-preserving NaN.
    let nonfinite = source.replacen(" -2.46519033E-32", "             NaN", 1);
    assert!(
        read("nan.fchk", nonfinite.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("not finite")
    );
    // Retain only the first six coordinates despite Number of atoms=3.
    let mismatched =
        replace_real_array(&source, "Current cartesian coordinates", |v| v.truncate(6));
    assert!(
        matches!(read("mismatch.fchk", mismatched.as_bytes()), Err(Error::Invalid(message)) if message.contains("must contain 9 values"))
    );
    // A Gaussian primitive exponent is positive independently of component conventions.
    let negative_exponent = replace_real_array(&source, "Primitive exponents", |v| v[0] = -v[0]);
    assert!(
        read("negative-exponent.fchk", negative_exponent.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("Primitive exponents must be positive")
    );
}

#[test]
fn fchk_header_columns_and_record_types_are_respected() {
    // Start with the water fixture, modify only the line-2 fields, and append documented
    // scalar/array logical records and character records, including the documented 9A8 H
    // array variant. Spaces stay in A30 method.
    let source = text("cclib/g09_water_mp2.fchk");
    for (kind, expected_kind, expected_task) in [
        ("SP", JobKind::SinglePoint, Task::Energy),
        ("FOpt", JobKind::Optimization, Task::Optimize),
        ("POpt", JobKind::Optimization, Task::Optimize),
        ("Freq", JobKind::Frequencies, Task::Hessian),
        ("Scan", JobKind::Scan, Task::Energy),
        ("Force", JobKind::Other, Task::Gradient),
    ] {
        let mut lines = source.lines().map(str::to_string).collect::<Vec<_>>();
        lines[1] = format!(
            "{kind:<10}{:<30}{}",
            "R Method With Spaces", "Basis With Spaces"
        );
        let modified = lines.join("\n")
            + "\n"
            + &format!("{:<40}   L     T\n", "Test scalar logical")
            + &format!("{:<40}   L   N={:>12}\nTFT\n", "Test logical array", 3)
            + &format!("{:<40}   C     value\n", "Test scalar character")
            + &format!(
                "{:<40}   H   N={:>12}\n{:<8}{:<8}\n",
                "Test short character array", 2, "One", "Two"
            );
        let file = read("columns.fchk", modified.as_bytes()).unwrap();
        assert_eq!(file.jobs[0].kind, expected_kind);
        assert_eq!(file.jobs[0].result.as_ref().unwrap().task, expected_task);
        assert_eq!(file.jobs[0].method.as_deref(), Some("R Method With Spaces"));
        assert_eq!(file.jobs[0].basis.as_deref(), Some("Basis With Spaces"));
    }
}

#[test]
fn fchk_frequency_atom_limit_applies_without_hessian_or_modes() {
    use reshiki_qcdata::MAX_HESSIAN_ATOMS;

    // Keep the DVB fixture's "Freq      RB3LYP ... STO-3G", charge, geometry and
    // "Total Energy ... -3.823082665785660E+02". Omit all orbital and frequency
    // records and extend the geometry with carbon atoms on the x axis.
    let source = text("cclib/g09_dvb_ir.fchk");
    let (header, _) = source.split_once("Number of electrons").unwrap();
    let (_, atoms) = source.split_once("Atomic numbers").unwrap();
    let (geometry, _) = atoms.split_once("Force Field").unwrap();
    let energy = source
        .lines()
        .find(|s| s.starts_with("Total Energy"))
        .unwrap();
    let template = remove_real_array(
        &format!("{header}Atomic numbers{geometry}{energy}\n"),
        "Nuclear charges",
    );
    for n in [MAX_HESSIAN_ATOMS, MAX_HESSIAN_ATOMS + 1] {
        let input = replace_integer(&template, "Number of atoms", n);
        let input = replace_integer_array(&input, "Atomic numbers", |atoms| atoms.resize(n, 6));
        let input = replace_real_array(&input, "Current cartesian coordinates", |xyz| {
            xyz.extend((xyz.len() / 3..n).flat_map(|i| [i as f64, 0.0, 0.0]));
        });
        let file = read("size-limit.fchk", input.as_bytes()).unwrap();
        let job = &file.jobs[0];
        assert_eq!(job.kind, JobKind::Frequencies);
        assert_eq!(job.result_step().unwrap().frame.numbers.len(), n);
        let result = job.result.as_ref().unwrap();
        assert_eq!(result.energy_hartree, Some(-382.308266578566));
        if n == MAX_HESSIAN_ATOMS {
            assert_eq!(result.task, Task::Hessian);
            assert!(file.warnings.is_empty());
        } else {
            assert_eq!(result.task, Task::Energy);
            assert_eq!(
                file.warnings,
                [format!(
                    "Job 1: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms"
                )]
            );
        }
    }
}
