use crate::qcread::{
    Error, JobKind, QcFile, read,
    tests::{fixture, open},
};
use reshiki_qcdata::{Beta, OrbitalSet, Shell, Status, Task};

const CARBON: &str = "cclib/molden5.7_C_bigbasis.molden";
const WATER: &str = "cclib/molden5.7_water_mp2.molden";
const IR: &str = "cclib/molden5.7_dvb_ir.molden";
const UNRESTRICTED: &str = "cclib/molden5.7_dvb_un_sp.molden";

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

fn modified(text: &str) -> QcFile {
    read("modified.molden", text.as_bytes()).unwrap()
}

fn spectrum(set: &OrbitalSet, aos: usize, levels: usize, filled: usize, frontier: [f64; 2]) {
    assert_eq!(set.energies_hartree.len(), levels);
    assert_eq!(set.rows, (0..levels as u32).collect::<Vec<_>>());
    assert_eq!(set.coefficients.len(), aos * levels);
    assert_eq!(set.symmetries, vec!["A"; levels]);
    assert_eq!(set.occupations.len(), levels);
    assert!(set.occupations[..filled].iter().all(|&n| n > 0.0));
    assert!(set.occupations[filled..].iter().all(|&n| n == 0.0));
    assert_eq!(set.energies_hartree[filled - 1..=filled], frontier);
}

#[test]
fn carbon_water_and_divinylbenzene_spectra() {
    // C_bigbasis: "C      1  6", 109 "Sym= A" records ending at "139   -0.000000".
    // "Ene=    -0.3584" is the third "Occup=   2.000000", followed by "Ene=     0.0189".
    // water_mp2: "H      3  1", "7   -0.808916", 7 alpha records, 5 doubly occupied.
    // "Ene=    -0.3882" and "Ene=     0.5707" bracket the occupation change.
    // dvb_ir: "H     20  1", "60    0.064756", 60 alpha records, 35 doubly occupied.
    // "Ene=    -0.1494" and "Ene=     0.0411" bracket the occupation change.
    // dvb_un_sp: "H     20  1", "60   -0.072278", 60 records per spin; 35 alpha and
    // 34 beta "Occup=   1.000000" in C10H10 give charge +1 and multiplicity 2.
    // Alpha "Ene=    -0.3662"/"Ene=    -0.1739", beta "Ene=    -0.3851"/"Ene=    -0.2968".
    for (name, atoms, aos, levels, occupied, frontier) in [
        (CARBON, 1, 139, 109, 3, [-0.3584, 0.0189]),
        (WATER, 3, 7, 7, 5, [-0.3882, 0.5707]),
        (IR, 20, 60, 60, 35, [-0.1494, 0.0411]),
        (UNRESTRICTED, 20, 60, 60, 35, [-0.3662, -0.1739]),
    ] {
        let file = open(name);
        assert_eq!(file.program, None); // [Title] contains only the molecule name.
        if name == CARBON {
            // 109 of 139 MOs is a partial spectrum, even though all Occup= are present.
            assert!(file.warnings.iter().any(|w| w.contains("assumed neutral")));
        } else {
            // Closed shells (occupations 0 or 2) and the unrestricted file need no assumption.
            assert!(file.warnings.is_empty(), "{name}: {:?}", file.warnings);
        }
        let [job] = file.jobs.as_slice() else {
            panic!("{name}")
        };
        let [step] = job.steps.as_slice() else {
            panic!("{name}")
        };
        assert_eq!(job.index, 0);
        assert_eq!(job.completion, None);
        assert_eq!(step.frame.numbers.len(), atoms);
        let unrestricted = name == UNRESTRICTED;
        assert_eq!(step.frame.charge, i32::from(unrestricted));
        assert_eq!(step.frame.multiplicity, 1 + u32::from(unrestricted));
        assert_eq!(step.energy_hartree, None);
        let result = job.result.as_ref().unwrap();
        assert_eq!(result.frame, step.frame.hash());
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.energy_hartree, None);
        if name == IR {
            assert_eq!(
                (job.kind, result.task),
                (JobKind::Frequencies, Task::Hessian)
            );
        } else {
            assert_eq!(
                (job.kind, result.task),
                (JobKind::SinglePoint, Task::Energy)
            );
            assert!(result.vibrations.is_none());
        }
        let orbitals = result.orbitals.as_ref().unwrap();
        assert!(orbitals.core_electrons.is_empty());
        let basis = orbitals.basis.as_ref().unwrap();
        assert_eq!(
            basis.shells.iter().map(Shell::n_functions).sum::<usize>(),
            aos
        );
        spectrum(&orbitals.alpha, aos, levels, occupied, frontier);
        if unrestricted {
            let Beta::Unrestricted(beta) = &orbitals.beta else {
                panic!()
            };
            spectrum(beta, aos, 60, 34, [-0.3851, -0.2968]);
        } else {
            assert_eq!(orbitals.beta, Beta::Restricted);
        }
    }
}

#[test]
fn frequencies_intensities_and_unit_displacements() {
    let file = open(IR);
    let v = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .vibrations
        .as_ref()
        .unwrap();
    // [FREQ] "47.87000000" ... "3546.79000000"; [INT] "0.029579";
    // [FR-NORM-COORD] "vibration 54". Each of the 54 blocks has 20 XYZ triples.
    assert_eq!(v.frequencies_cm1.len(), 54);
    assert_eq!(v.frequencies_cm1[0], 47.87);
    assert_eq!(v.frequencies_cm1[53], 3546.79);
    assert_eq!(v.ir_intensities_km_mol.len(), 54);
    assert_eq!(v.ir_intensities_km_mol[0], 0.029579);
    assert_eq!(v.modes.len(), 54 * 60);
    for mode in v.modes.as_chunks::<60>().0 {
        assert!((mode.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() < 1e-12);
    }
    // Synthetic tiny imaginary mode and imaginary mode, with an optional Raman column.
    let altered = modified(
        &text(IR)
            .replace("47.87000000", "-0.00009")
            .replace("152.30000000", "-152.30000000")
            .replace("0.341420", "0.341420  10"),
    );
    let sorted = altered.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .vibrations
        .as_ref()
        .unwrap();
    assert_eq!(sorted.frequencies_cm1.len(), 54);
    assert_eq!(sorted.frequencies_cm1[..3], [-152.3, -0.00009, 81.18]);
    assert_eq!(sorted.ir_intensities_km_mol[0], 0.341420);
    assert_eq!(sorted.modes[..60], v.modes[120..180]);
}

#[test]
fn frequencies_follow_their_own_geometry() {
    // Synthetic change to the first "[FR-COORD]" position only.
    let displaced = modified(&text(IR).replace("2.67238738", "3.67238738"));
    let job = &displaced.jobs[0];
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.task, Task::Energy);
    assert!(result.vibrations.is_none());
    assert!(result.orbitals.is_some());
    assert!(
        displaced
            .warnings
            .iter()
            .any(|s| s.to_ascii_lowercase().contains("geometr"))
    );
    // A standalone vibration file consists only of the frequency sections of this fixture.
    let original = text(IR);
    let (_, vibrations) = original.split_once("[FREQ]").unwrap();
    let file = modified(&format!("[Molden Format]\n[FREQ]{vibrations}"));
    let job = &file.jobs[0];
    assert_eq!(job.steps[0].frame.numbers.len(), 20);
    let first_x = job.steps[0].frame.positions_angstrom[0][0];
    assert!((first_x - 2.67238738 * reshiki_qcdata::ANGSTROM_PER_BOHR).abs() < 1e-12);
    assert_eq!(job.result.as_ref().unwrap().task, Task::Hessian);
}

#[test]
fn splits_sp_and_ignores_obsolete_shell_scales() {
    // Recombine the fixture's split oxygen valence shells into a synthetic SP shell.
    let original = text(WATER);
    let start = original.find("s     3 1.00\n5.033151300").unwrap();
    let end = start + original[start..].find("\n\n").unwrap();
    let mut combined = original.clone();
    combined.replace_range(start..end, "Sp 3 1.00\n5.033151300D+00 -9.996722919e-02 1.559162750e-01\n1.169596100D+00 3.995128261e-01 6.076837186e-01\n3.803890000D-01 7.001154689e-01 3.919573931e-01");
    let expected = open(WATER);
    let expected = expected.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    let split = modified(&combined);
    assert_eq!(
        split.jobs[0].result.as_ref().unwrap().orbitals.as_ref(),
        Some(expected)
    );
    // Change only the obsolete synthetic SP scale; Molden says this field is nonfunctional.
    let scaled = modified(&combined.replace("Sp 3 1.00", "Sp 3 2.00"));
    let shells = &scaled.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap()
        .basis
        .as_ref()
        .unwrap()
        .shells;
    assert_eq!((shells[1].l, shells[2].l), (0, 1));
    assert_eq!(shells[1].exponents_per_bohr2[0], 5.033151300);
    assert_eq!(shells[2].exponents_per_bohr2, shells[1].exponents_per_bohr2);
}

#[test]
fn ecp_spin_units_and_optional_metadata() {
    // Synthetic ECP on oxygen leaves two core electrons out of the ten occupied electrons.
    // Change units and decorate the label in "O      1  8"; leave the coordinate numbers intact.
    let raw = text(WATER)
        .replace("[Atoms] Angs", "[aToMs] AU")
        .replace("O      1  8", "O1     1  6");
    let file = modified(&raw);
    let frame = &file.jobs[0].steps[0].frame;
    assert_eq!(frame.numbers, [8, 1, 1]);
    assert_eq!(frame.charge, -2);
    assert_eq!(
        frame.positions_angstrom[1][0],
        0.990000 * reshiki_qcdata::ANGSTROM_PER_BOHR
    );
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(orbitals.core_electrons, [2, 0, 0]);

    // Remove one Occup= header, then explicitly supply a triplet and charge.
    let partial = raw.replacen(" Occup=   2.000000\n", "", 1);
    let fallback = modified(&partial);
    assert_eq!(fallback.jobs[0].steps[0].frame.charge, 0);
    assert!(fallback.warnings.iter().any(
        |s| s == "Charge and spin are not recorded; assumed neutral and the lowest spin state"
    ));
    let explicit = modified(&format!(
        "{partial}\n[N_ATOMS]\n3\n[Charge]\n-2\n[Multiplicity]\n3\n"
    ));
    let frame = &explicit.jobs[0].steps[0].frame;
    assert_eq!((frame.charge, frame.multiplicity), (-2, 3));
    let invalid = format!("{partial}\n[N_ATOMS]\n2\n");
    assert!(read("test.molden", invalid.as_bytes()).is_err());
    // Turn a doubly occupied orbital into a singly occupied one. Its partner is absent.
    let doublet = modified(&text(WATER).replacen(" Occup=   2.000000", " Occup=   1.000000", 1));
    let job = &doublet.jobs[0];
    assert_eq!(
        (job.steps[0].frame.charge, job.steps[0].frame.multiplicity),
        (1, 2)
    );
    let orbitals = job.result.as_ref().unwrap().orbitals.as_ref().unwrap();
    let Beta::SameOrbitals { occupations } = &orbitals.beta else {
        panic!()
    };
    assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 5.0);
    assert_eq!(occupations.iter().sum::<f64>(), 4.0);

    // Four fractional water occupations sum to a pair. None is singly occupied.
    let fraction =
        |count| modified(&text(WATER).replacen("Occup=   2.000000", "Occup=   0.500000", count));
    let paired = fraction(4);
    let frame = &paired.jobs[0].steps[0].frame;
    assert_eq!((frame.charge, frame.multiplicity), (6, 1));
    assert!(
        paired
            .warnings
            .iter()
            .any(|w| w.contains("Spin-summed restricted"))
    );
    // With only three replacements the recorded total is nonintegral.
    let incomplete = fraction(3);
    assert_eq!(incomplete.jobs[0].steps[0].frame.charge, 0);
    assert!(
        incomplete
            .warnings
            .iter()
            .any(|w| w.contains("assumed neutral"))
    );
}

#[test]
fn sparse_mos_follow_energies_and_keep_equal_energy_order() {
    // Replace the water MO section with three reordered records. Include a tie and
    // omit all but one coefficient per row. Keep the original basis and AO indices.
    let original = text(WATER);
    let (structure, _) = original.split_once("[MO]").unwrap();
    let input = format!(
        "{structure}[mo]\nOccup= 0\nSpin= Alpha\nEne= 0.5707\nSym= first\n7 -0.775801\nEne= -20.2438\nOccup= 2\nSym= core\n1 0.994203\nSym= second\nEne= 0.5707\nOccup= 0\n6 -0.775801\n"
    );
    let file = modified(&input);
    let set = &file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap()
        .alpha;
    assert_eq!(set.symmetries, ["core", "first", "second"]);
    assert_eq!(set.occupations, [2.0, 0.0, 0.0]);
    assert_eq!(set.rows, [0, 1, 2]);
    assert_eq!(set.coefficients[0], 0.994203);
    assert_eq!(set.coefficients[13], -0.775801);
    assert_eq!(set.coefficients[19], -0.775801);
    assert_eq!(set.coefficients.iter().filter(|&&v| v == 0.0).count(), 18);
    let beyond_basis = input.replace("7 -0.775801", "8 -0.775801");
    assert!(matches!(
        read("test.molden", beyond_basis.as_bytes()),
        Err(Error::Invalid(_))
    ));
    // An absent energy cannot be stored as a spectrum level. Its occupation still counts.
    let partial = modified(&original.replace(" Ene=   -20.2438\n", ""));
    assert_eq!(partial.jobs[0].steps[0].frame.charge, 0);
    assert_eq!(
        partial.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .rows
            .len(),
        6
    );
    assert!(!partial.warnings.is_empty());
}

#[test]
fn unsupported_shapes_preserve_spectra() {
    let raw = text(WATER);
    // Substitute a Slater basis section for the fixture's [GTO] header.
    assert_eq!(
        read("test.molden", raw.replace("[GTO]", "[sto]").as_bytes()),
        Err(Error::Unsupported(
            "Slater-type orbitals ([STO]) are not supported".into()
        ))
    );
    // Change the title and replace water's p shell by a higher-l shell; the original
    // seven coefficient indices form sparse rows for these larger synthetic bases.
    for (title, shell, flag, keep) in [
        ("orca_2mkl", "f", "[5D]\n[7F]", true),
        ("orca_2mkl", "f", "[5D]\n[10F]", false),
        ("orca_2mkl", "g", "[9G]", false),
        ("orca_2mkl", "d", "[5D]", true),
        ("Psi4", "d", "", false),
        ("Molden", "d", "", true),
        ("Molden", "d", "[5D10F]", true),
        ("PySCF", "d", "[5D10F]", true),
    ] {
        let input = raw
            .replace("water_mp2", title)
            .replace("p     3 1.00", &format!("{shell} 3 1.00"))
            .replace("[MO]", &format!("{flag}\n[MO]"));
        let file = modified(&input);
        let producer = if title == "orca_2mkl" { "ORCA" } else { title };
        assert_eq!(file.program.as_deref(), Some(producer));
        let orbitals = file.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap();
        assert_eq!(orbitals.basis.is_some(), keep);
        assert_eq!(orbitals.alpha.energies_hartree.len(), 7);
        assert_eq!(orbitals.alpha.energies_hartree[4], -0.3882);
        assert_eq!(orbitals.alpha.occupations.len(), 7);
        if !keep {
            assert!(orbitals.alpha.rows.is_empty() && orbitals.alpha.coefficients.is_empty());
            assert!(
                file.warnings
                    .iter()
                    .any(|w| w.contains("orbital energies are kept"))
            );
        }
    }
    // A generic Molden title names the interchange format. The generator comment
    // identifies the program whose orbital convention determines shape support.
    let generated = modified(
        &raw.replace("water_mp2", "Molden file\n# generated by orca_2mkl")
            .replace("p     3 1.00", "g 3 1.00")
            .replace("[MO]", "[9G]\n[MO]"),
    );
    assert_eq!(generated.program.as_deref(), Some("ORCA"));
    assert!(
        generated
            .warnings
            .iter()
            .any(|w| w.contains("pure g functions from ORCA"))
    );
}

#[test]
fn high_shell_shape_support_depends_on_molden_producer() {
    // Synthetic variants of water's "p     3 1.00" contraction, whose exponents
    // start at "5.033151300e+00". Its seven MO rows remain sparse in larger bases.
    // None of these variants proves a physical higher-l orbital convention.
    let raw = text(WATER);
    for (metadata, producer, keep_all) in [
        ("[Title]\nwater_mp2\n", None, true),
        ("[Title]\nMolden\n", Some("Molden"), true),
        ("made by pyscf v[2.15.0]\n", Some("PySCF"), true),
        ("[Title]\nPsi4\n", Some("Psi4"), false),
        ("[Title]\nGaussian\n", Some("Gaussian"), false),
        ("[Title]\norca_2mkl\n", Some("ORCA"), false),
    ] {
        for (shell, pure_flag, functions) in [
            ("d", "[5D10F]", [6, 5]),
            ("f", "[7F]", [10, 7]),
            ("g", "[9G]", [15, 9]),
        ] {
            for pure in [false, true] {
                let keep = keep_all || (producer == Some("ORCA") && pure && shell != "g");
                let input = raw
                    .replace("[Title]\nwater_mp2\n", metadata)
                    .replace("p     3 1.00", &format!("{shell} 3 1.00"))
                    .replace(
                        "[MO]",
                        &format!("{}\n[MO]", if pure { pure_flag } else { "" }),
                    );
                let file = modified(&input);
                assert_eq!(file.program.as_deref(), producer);
                let orbitals = file.jobs[0]
                    .result
                    .as_ref()
                    .unwrap()
                    .orbitals
                    .as_ref()
                    .unwrap();
                assert_eq!(
                    orbitals.basis.is_some(),
                    keep,
                    "{producer:?}, {shell}, {pure}"
                );
                assert_eq!(orbitals.alpha.energies_hartree[4..6], [-0.3882, 0.5707]);
                if let Some(basis) = &orbitals.basis {
                    let high = basis.shells.iter().find(|s| s.l >= 2).unwrap();
                    assert_eq!(high.pure, pure);
                    assert_eq!(orbitals.alpha.rows, (0..7).collect::<Vec<_>>());
                    assert_eq!(
                        orbitals.alpha.coefficients.len(),
                        7 * (4 + functions[usize::from(pure)])
                    );
                } else {
                    assert!(orbitals.alpha.rows.is_empty());
                    assert!(orbitals.alpha.coefficients.is_empty());
                    let representation = if pure { "pure" } else { "Cartesian" };
                    let warning = format!(
                        "Orbital shapes left out: {representation} {shell} functions from {} have no verified Molden convention; orbital energies are kept",
                        producer.unwrap()
                    );
                    assert!(file.warnings.contains(&warning));
                }
            }
        }
    }
    // A water basis with only s/p shells also retains its shapes under the ORCA adapter.
    let orca = modified(&raw.replace("water_mp2", "Generated by orca_2mkl"));
    assert!(
        orca.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap()
            .basis
            .is_some()
    );
    assert!(
        !orca
            .warnings
            .iter()
            .any(|w| w.starts_with("Orbital shapes left out"))
    );
    // A named upstream calculator does not override an explicit Molden exporter.
    let exported = modified(
        &text(CARBON).replace("C_bigbasis", "Generated by Molden 5.7 from GAMESS-US 2018"),
    );
    assert_eq!(exported.program.as_deref(), Some("Molden"));
    assert!(
        exported.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap()
            .basis
            .is_some()
    );
}

#[test]
fn beta_with_any_missing_spin_drops_all_orbitals() {
    // The dvb_un_sp source has 60 MOs per channel, 35 occupied alpha and 34 beta.
    // Delete one Alpha spin header; its Spin= must not be guessed from neighbors.
    let raw = text(UNRESTRICTED).replacen(" Spin= Alpha\n", "", 1);
    for input in [
        raw.clone(),
        raw.replacen("Occup=   1.000000", "Occup=   2.000000", 1),
    ] {
        // Even a would-be out-of-range per-spin occupation cannot prevent the
        // geometry import once the ambiguous orbital section has been omitted.
        let file = modified(&input);
        assert_eq!(file.jobs[0].steps[0].frame.numbers.len(), 20);
        assert_eq!(file.jobs[0].steps[0].frame.charge, 0);
        assert!(file.jobs[0].result.as_ref().unwrap().orbitals.is_none());
        assert!(
            file.warnings
                .iter()
                .any(|w| w.contains("spin channels are ambiguous"))
        );
    }
    // A missing spin header in a shared alpha-only spectrum remains unambiguous.
    let shared = modified(&text(WATER).replacen(" Spin= Alpha\n", "", 1));
    assert!(shared.jobs[0].result.as_ref().unwrap().orbitals.is_some());
    assert!(shared.warnings.is_empty(), "{:?}", shared.warnings);
}

#[test]
fn absent_levels_or_occupations_do_not_report_an_attempted_spin_split() {
    // Remove water's [MO] section, leaving its source geometry and GTO records.
    // An explicit q=+1 doublet is valid without any orbital results to split.
    let raw = text(WATER);
    let (structure, _) = raw.split_once("[MO]").unwrap();
    let geometry = modified(&format!("{structure}\n[Charge]\n1\n[Multiplicity]\n2\n"));
    assert!(geometry.jobs[0].result.as_ref().unwrap().orbitals.is_none());
    assert!(geometry.warnings.is_empty());
    // Keep all seven source MO energies and rows, but remove every Occup= field.
    // There are no source occupations for a per-spin conversion to discard.
    let no_occupations = raw
        .lines()
        .filter(|line| !line.contains("Occup="))
        .collect::<Vec<_>>()
        .join("\n");
    let levels = modified(&format!(
        "{no_occupations}\n[Charge]\n1\n[Multiplicity]\n2\n"
    ));
    assert_eq!(
        levels.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .energies_hartree
            .len(),
        7
    );
    assert!(levels.warnings.is_empty());
    // Dropped ambiguous DVB orbitals likewise leave no levels to convert.
    let ambiguous = text(UNRESTRICTED).replacen(" Spin= Alpha\n", "", 1);
    let dropped = modified(&format!("{ambiguous}\n[Charge]\n1\n[Multiplicity]\n2\n"));
    assert!(dropped.jobs[0].result.as_ref().unwrap().orbitals.is_none());
    assert_eq!(dropped.warnings.len(), 1);
    assert!(dropped.warnings[0].contains("spin channels are ambiguous"));
}

#[test]
fn pseudo_metadata_maps_source_indices_and_survives_shape_omission() {
    // The original oxygen row is "O      1  8". Synthetically relabel its source
    // index to 7 (and its GTO centre), then declare effective charge 6 in [Pseudo].
    // All seven water MOs and their ten electrons remain: 2 core electrons => q=-2.
    let raw = text(WATER)
        .replace("O      1  8", "O1     7  8")
        .replace("  1 0\n", "  7 0\n");
    let (geometry, tail) = raw.split_once("[GTO]").unwrap();
    let (gto, mos) = tail.split_once("[MO]").unwrap();
    // One s shell and one Cartesian d shell keep the ORCA spectrum complete.
    let orca_gto = "\n  7 0\ns 1 1.00\n1.0 1.0\nd 1 1.00\n1.0 1.0\n";
    for (title, basis, keep) in [("water_mp2", gto, true), ("orca_2mkl", orca_gto, false)] {
        let geometry = geometry.replace("water_mp2", title);
        let input = format!("{geometry}[GTO]{basis}[MO]{mos}\n[Pseudo]\nO1 7 6.000000D+00\n");
        let file = modified(&input);
        let job = &file.jobs[0];
        assert_eq!(
            (job.steps[0].frame.charge, job.steps[0].frame.multiplicity),
            (-2, 1)
        );
        let orbitals = job.result.as_ref().unwrap().orbitals.as_ref().unwrap();
        assert_eq!(orbitals.core_electrons, [2, 0, 0]);
        assert_eq!(orbitals.basis.is_some(), keep);
        for invalid in [
            "O 8 6",
            "H 7 6",
            "O 7 8.1",
            "O 7 -1",
            "O 7 9",
            "O 7 6\nO 7 6",
        ] {
            let input = format!("{raw}\n[Pseudo]\n{invalid}\n");
            assert!(
                read("invalid.molden", input.as_bytes()).is_err(),
                "{invalid}"
            );
        }
    }
    // Contradictory effective-charge metadata cannot silently replace itself.
    let invalid = format!(
        "{}\n[Pseudo]\nO 7 4\n",
        raw.replace("O1     7  8", "O1     7  6")
    );
    assert!(read("invalid.molden", invalid.as_bytes()).is_err());
}

#[test]
fn charge_inference_requires_complete_occupied_spectra_in_each_channel() {
    // The source water has seven AOs and seven MO records. Remove its two virtual
    // MOs by cutting before "Ene=     0.5707"; occupied-only records do not prove q.
    let original = text(WATER);
    let mut records = original.split(" Sym= A\n");
    let structure = records.next().unwrap();
    let occupied = records
        .take(5)
        .map(|s| format!(" Sym= A\n{s}"))
        .collect::<String>();
    let partial = modified(&format!("{structure}{occupied}"));
    assert_eq!(partial.jobs[0].steps[0].frame.charge, 0);
    assert!(
        partial
            .warnings
            .iter()
            .any(|w| w.contains("assumed neutral"))
    );
    assert_eq!(
        partial.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .energies_hartree
            .len(),
        5
    );

    // Removing one Occup= also prevents inference from an otherwise full spectrum.
    let incomplete = modified(&original.replacen(" Occup=   2.000000\n", "", 1));
    assert!(
        incomplete
            .warnings
            .iter()
            .any(|w| w.contains("assumed neutral"))
    );
    // Ene= is optional for electron counting: all seven Occup= records still exist.
    let no_energy = modified(&original.replace(" Ene=   -20.2438\n", ""));
    assert_eq!(no_energy.jobs[0].steps[0].frame.charge, 0);
    assert!(
        !no_energy
            .warnings
            .iter()
            .any(|w| w.contains("assumed neutral"))
    );

    // Source unrestricted DVB has 60 alpha and 60 beta levels. Remove only the
    // final beta block: 60 alpha + 59 beta is incomplete even with every Occup=.
    let raw = text(UNRESTRICTED);
    let end = raw.rfind(" Sym= A\n").unwrap();
    let one_short = modified(&raw[..end]);
    assert_eq!(one_short.jobs[0].steps[0].frame.charge, 0);
    assert!(
        one_short
            .warnings
            .iter()
            .any(|w| w.contains("assumed neutral"))
    );
    // With no GTO, seven headers do not prove how many MOs were omitted.
    let (before_basis, after_basis) = original.split_once("[GTO]").unwrap();
    let (_, mo) = after_basis.split_once("[MO]").unwrap();
    let levels = mo
        .lines()
        .filter(|line| line.contains('='))
        .collect::<Vec<_>>()
        .join("\n");
    let no_basis = modified(&format!("{before_basis}[MO]\n{levels}\n"));
    assert!(
        no_basis
            .warnings
            .iter()
            .any(|w| w.contains("assumed neutral"))
    );
}

#[test]
fn fractional_unrestricted_populations_keep_known_charge_and_ambiguous_spin() {
    // Source DVB has 35 occupied alpha and 34 occupied beta MOs over 60 AOs per
    // channel. Transfer half an electron from the first alpha Occup=1 to a beta
    // virtual Occup=0: 34.5 + 34.5 still accounts for 69 electrons and charge +1.
    let raw = text(UNRESTRICTED).replacen("Occup=   1.000000", "Occup=   0.500000", 1);
    let (alpha, beta) = raw.split_once(" Spin= Beta\n").unwrap();
    let fractional = format!(
        "{alpha} Spin= Beta\n{}",
        beta.replacen("Occup=   0.000000", "Occup=   0.500000", 1)
    );
    for explicit in [None, Some(4)] {
        let input = if let Some(multiplicity) = explicit {
            format!("{fractional}\n[Multiplicity]\n{multiplicity}\n")
        } else {
            fractional.clone()
        };
        let file = modified(&input);
        let job = &file.jobs[0];
        assert_eq!(job.steps[0].frame.charge, 1);
        assert_eq!(job.steps[0].frame.multiplicity, explicit.unwrap_or(2));
        let orbitals = job.result.as_ref().unwrap().orbitals.as_ref().unwrap();
        let Beta::Unrestricted(beta) = &orbitals.beta else {
            panic!()
        };
        assert_eq!(orbitals.alpha.occupations.iter().sum::<f64>(), 34.5);
        assert_eq!(beta.occupations.iter().sum::<f64>(), 34.5);
        assert_eq!(orbitals.alpha.rows.len(), 60);
        assert_eq!(beta.rows.len(), 60);
        if explicit.is_none() {
            assert!(
                file.warnings
                    .iter()
                    .any(|w| w.contains("Fractional unrestricted"))
            );
        } else {
            assert!(file.warnings.is_empty());
        }
    }
}

#[test]
fn explicit_state_wins_and_complete_spectrum_contradictions_warn() {
    // Seven water MOs account for ten electrons and a paired singlet. Explicit
    // charge +2 and multiplicity 3 are frame-valid but contradict those records.
    let file = modified(&format!(
        "{}\n[Charge]\n2\n[Multiplicity]\n3\n",
        text(WATER)
    ));
    assert_eq!(
        (
            file.jobs[0].steps[0].frame.charge,
            file.jobs[0].steps[0].frame.multiplicity
        ),
        (2, 3)
    );
    assert!(
        file.warnings
            .iter()
            .any(|w| w.contains("[Charge] disagrees"))
    );
    assert!(
        file.warnings
            .iter()
            .any(|w| w.contains("[Multiplicity] disagrees"))
    );
    // A charge with opposite parity also changes the lowest allowed spin.
    let changed_parity = modified(&format!("{}\n[Charge]\n1\n", text(WATER)));
    assert_eq!(
        (
            changed_parity.jobs[0].steps[0].frame.charge,
            changed_parity.jobs[0].steps[0].frame.multiplicity
        ),
        (1, 2)
    );
    assert!(
        changed_parity
            .warnings
            .iter()
            .any(|w| w.contains("explicit charge; assumed the lowest spin"))
    );
    // Incomplete records have no complete-spectrum contradiction to report.
    let partial = text(WATER).replacen(" Occup=   2.000000\n", "", 1);
    let explicit = modified(&format!("{partial}\n[Charge]\n2\n[Multiplicity]\n3\n"));
    assert!(!explicit.warnings.iter().any(|w| w.contains("disagrees")));
}

#[test]
fn restricted_single_occupations_choose_lowest_consistent_spin() {
    // Turn two of water's five "Occup=   2.000000" records into Occup=1.
    // The spin-summed spectrum has eight electrons but does not establish triplet coupling.
    let raw = text(WATER).replacen("Occup=   2.000000", "Occup=   1.000000", 2);
    let file = modified(&raw);
    let job = &file.jobs[0];
    assert_eq!(
        (job.steps[0].frame.charge, job.steps[0].frame.multiplicity),
        (2, 1)
    );
    assert_eq!(
        job.result.as_ref().unwrap().orbitals.as_ref().unwrap().beta,
        Beta::Restricted
    );
    assert!(
        file.warnings
            .iter()
            .any(|w| w.contains("Spin-summed restricted"))
    );
    // An explicit triplet can resolve this same spectrum without a contradiction.
    let triplet = modified(&format!("{raw}\n[Multiplicity]\n3\n"));
    assert_eq!(triplet.jobs[0].steps[0].frame.multiplicity, 3);
    assert!(!triplet.warnings.iter().any(|w| w.contains("disagrees")));
    // Change the explicit charge as well as multiplicity to a frame-valid odd
    // electron doublet. Its parity still contradicts this complete eight-electron
    // source spectrum, so both explicit fields need a warning.
    let parity = modified(&format!("{raw}\n[Charge]\n1\n[Multiplicity]\n2\n"));
    assert_eq!(
        (
            parity.jobs[0].steps[0].frame.charge,
            parity.jobs[0].steps[0].frame.multiplicity
        ),
        (1, 2)
    );
    assert!(
        parity
            .warnings
            .iter()
            .any(|w| w.contains("[Charge] disagrees"))
    );
    assert!(
        parity
            .warnings
            .iter()
            .any(|w| w.contains("[Multiplicity] disagrees"))
    );
    // Three singly occupied records require only one net unpaired electron in
    // the lowest state; their shared-orbital canonical populations must reflect it.
    let odd = modified(&text(WATER).replacen("Occup=   2.000000", "Occup=   1.000000", 3));
    let orbitals = odd.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    let Beta::SameOrbitals { occupations } = &orbitals.beta else {
        panic!()
    };
    assert_eq!(odd.jobs[0].steps[0].frame.multiplicity, 2);
    assert!(orbitals.alpha.occupations.is_empty());
    assert!(occupations.is_empty());
    assert!(
        odd.warnings
            .iter()
            .any(|w| w.contains("Per-spin occupations left out"))
    );
    let incompatible = modified(&format!("{}\n[Multiplicity]\n3\n", text(WATER)));
    let orbitals = incompatible.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert!(orbitals.alpha.occupations.is_empty());
    assert!(matches!(&orbitals.beta, Beta::SameOrbitals { occupations } if occupations.is_empty()));
    assert!(
        incompatible
            .warnings
            .iter()
            .any(|w| w.contains("[Multiplicity] disagrees"))
    );
}

#[test]
fn missing_coefficient_rows_keep_levels_without_inventing_zero_mos() {
    // Keep all seven fixture headers, but remove the first row's AO records
    // (starting "1    0.994203"). Other MOs still prove that coefficients exist.
    let original = text(WATER);
    let start = original.find("   1    0.994203").unwrap();
    let end = start + original[start..].find(" Sym= A\n").unwrap();
    let mut input = original.clone();
    input.replace_range(start..end, "");
    let file = modified(&input);
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(orbitals.alpha.energies_hartree.len(), 7);
    assert_eq!(orbitals.alpha.energies_hartree[0], -20.2438);
    assert_eq!(orbitals.alpha.rows, [1, 2, 3, 4, 5, 6]);
    assert_eq!(orbitals.alpha.coefficients.len(), 6 * 7);
    // Explicitly listed zero coefficients remain an actual sparse shape row.
    let zeros = modified(&original.replace("   1    0.994203", "   1    0.000000"));
    assert_eq!(
        zeros.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .rows
            .len(),
        7
    );
}

#[test]
fn conflicting_representation_flags_are_rejected_even_without_gto() {
    // Add synthetic global declarations to the water fixture in memory.
    for flags in [
        "[5D]\n[7F]",
        "[5D]\n[5D10F]",
        "[5D10F]\n[7F]",
        "[9G]\n[15G]",
        "[6D]\n[5D10F]",
    ] {
        let input = format!("{}\n{flags}\n", text(WATER));
        assert!(
            matches!(read("flags.molden", input.as_bytes()), Err(Error::Invalid(message)) if message.contains("Conflicting")),
            "{flags}"
        );
        let raw = text(WATER);
        let (geometry, _) = raw.split_once("[GTO]").unwrap();
        let input = format!("{geometry}\n{flags}\n");
        assert!(read("flags.molden", input.as_bytes()).is_err(), "{flags}");
    }
    // Identical declarations and unrelated angular momenta are compatible.
    assert!(
        read(
            "flags.molden",
            format!("{}\n[5D]\n[5D7F]\n[9G]\n", text(WATER)).as_bytes()
        )
        .is_ok()
    );
    // [7F] can accompany an explicit declaration of Cartesian d.
    let input = format!("{}\n[6D]\n[7F]\n", text(WATER));
    let file = modified(&input);
    // H      3  1    -0.272880     0.951649     0.000000
    assert_eq!(file.jobs[0].steps[0].frame.numbers, [8, 1, 1]);
}

#[test]
fn only_exact_zero_frequencies_are_discarded_with_aligned_data() {
    // Source [FREQ] starts 47.87, 81.18, 152.30 and [INT] starts 0.029579.
    // Replace these with exact zero, tiny positive and tiny negative frequencies.
    let input = text(IR)
        .replace("47.87000000", "0.0")
        .replace("81.18000000", "0.0000000001")
        .replace("152.30000000", "-0.0000000002");
    let original = open(IR);
    let original = original.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .vibrations
        .as_ref()
        .unwrap();
    let file = modified(&input);
    let v = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .vibrations
        .as_ref()
        .unwrap();
    assert_eq!(v.frequencies_cm1.len(), 53);
    assert_eq!(v.frequencies_cm1[..2], [-2e-10, 1e-10]);
    assert_eq!(
        v.ir_intensities_km_mol[..2],
        [
            original.ir_intensities_km_mol[2],
            original.ir_intensities_km_mol[1]
        ]
    );
    assert_eq!(v.modes[..60], original.modes[120..180]);
    assert_eq!(v.modes[60..120], original.modes[60..120]);
}

#[test]
fn optimization_energy_stays_with_its_geometry() {
    // Append two synthetic XYZ optimization steps to the water text. Use the original
    // final H position ("H      2  1     0.990000") and perturb the earlier geometry.
    let raw = text(WATER);
    let path = "[GEOCONV]\nenergy\n-74.9\n-75.0\nmax-force\n0.2\n0.01\n[GEOMETRIES] XYZ\n3\nfirst\nO 0 0 0\nH 1.1 0 0\nH -0.272880 0.951649 0\n3\n\nO 0 0 0\nH 0.990000 0 0\nH -0.272880 0.951649 0\n";
    for (second_x, n_steps, energy) in [("0.9900004", 2, Some(-75.0)), ("1.0", 3, None)] {
        let file = modified(&format!("{raw}\n{}", path.replace("0.990000", second_x)));
        let job = &file.jobs[0];
        assert_eq!(job.steps.len(), n_steps);
        assert_eq!(job.kind, JobKind::Optimization);
        assert_eq!(job.steps[0].energy_hartree, Some(-74.9));
        assert_eq!(job.steps[1].energy_hartree, Some(-75.0));
        let result = job.result.as_ref().unwrap();
        assert_eq!(result.task, Task::Optimize);
        assert_eq!(result.energy_hartree, energy);
        assert_eq!(result.frame, job.steps.last().unwrap().frame.hash());
        assert!(result.trajectory.is_some());
    }
}
