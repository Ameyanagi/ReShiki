use reshiki_qcdata::{
    AtomicCharges, BasisSet, Beta, BondOrder, CalcResult, ChargeScheme, Convergence, Frame,
    LIGHT_BOND_ORDER_MIN, MAX_IMPORTED_HEAVY_BYTES, OrbitalSet, Orbitals, Provenance, RunSummary,
    Settings, Shell, Source, Spin, Status, Task, Thermo, Trajectory, Vibrations, XtbMethod,
    XtbSettings,
};

fn unit_modes(n_modes: usize, width: usize, random: &mut impl FnMut() -> f64) -> Vec<f64> {
    let mut values: Vec<_> = (0..n_modes * width).map(|_| random()).collect();
    for mode in values.chunks_exact_mut(width) {
        let norm = mode.iter().map(|x| x * x).sum::<f64>().sqrt();
        for x in mode {
            *x /= norm;
        }
    }
    values
}

fn fixture(n_atoms: u32) -> (Frame, CalcResult) {
    let mut state = 0x67a4_f21d_u32;
    let mut random = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (f64::from(state) / 2_147_483_648.0) - 1.0
    };
    let n = n_atoms as usize;
    let width = n * 3;
    let n_modes = width - 6;
    let n_levels = if n == 50 { 200 } else { 600 };
    let frame = Frame {
        numbers: vec![6; n],
        positions_angstrom: (0..n_atoms)
            .map(|i| [f64::from(i) * 1.5026, random(), random()])
            .collect(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let mut shells: Vec<_> = (0..n_atoms)
        .map(|atom| Shell {
            atom,
            l: 1,
            pure: false,
            exponents_per_bohr2: vec![0.81643, 1.32234],
            coefficients: vec![0.51234, 0.48766],
        })
        .collect();
    if n == 50 {
        shells.extend((0..n_atoms).map(|atom| Shell {
            atom,
            l: 0,
            pure: false,
            exponents_per_bohr2: vec![1.23241],
            coefficients: vec![1.0],
        }));
    }
    let bond_orders = (0..n_atoms - 1)
        .map(|a| (a, a + 1))
        .chain((0..n_atoms / 10 + 1).map(|a| (a, a + 2)))
        .map(|(a, b)| BondOrder {
            a,
            b,
            order: 0.8 + 0.15 * random(),
        })
        .collect();
    let result = CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task: Task::Hessian,
        endpoints: None,
        settings: Settings::Xtb(XtbSettings {
            method: XtbMethod::Gfn2Xtb,
            conventions: Default::default(),
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            scf_retries: vec![],
            gfnff: None,
            threads: None,
            convergence: Default::default(),
            fixed_atoms: vec![],
        }),
        provenance: Provenance {
            program: "ReShiki".into(),
            program_version: None,
            method: Some("GFN2-xTB".into()),
            basis: None,
            source: Source::Calculated {
                engine_revision: "fixture".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: Some(-117.543_287),
        energy_terms: [
            ("electronic".into(), -119.502_136),
            ("repulsion".into(), 1.958_849),
        ]
        .into(),
        gradient_hartree_per_bohr: (0..width).map(|_| random() * 0.01).collect(),
        hessian_hartree_per_bohr2: (0..width * width).map(|_| random() * 0.1).collect(),
        charges: vec![AtomicCharges {
            scheme: ChargeScheme::Mulliken,
            values: (0..n).map(|_| 0.2 * random()).collect(),
        }],
        dipole_e_bohr: Some([0.3321, -0.771, 0.0129]),
        bond_orders,
        orbitals: Some(Orbitals {
            basis: Some(BasisSet { shells }),
            core_electrons: vec![2; n],
            alpha: OrbitalSet {
                energies_hartree: (0..n_levels)
                    .map(|i| f64::from(i) * 0.011_621 - 1.224_228)
                    .collect(),
                occupations: (0..n_levels)
                    .map(|i| if i < n_atoms * 2 { 2.0 } else { 0.0 })
                    .collect(),
                symmetries: vec![],
                rows: (0..n_levels).collect(),
                coefficients: (0..n_levels * n_levels).map(|_| random()).collect(),
            },
            beta: Beta::Restricted,
        }),
        vibrations: Some(Vibrations {
            frequencies_cm1: (0..width as u32 - 6)
                .map(|i| f64::from(i) * 5.527_106 + 22.331)
                .collect(),
            ir_intensities_km_mol: (0..n_modes).map(|_| 100.0 * random().abs()).collect(),
            modes: unit_modes(n_modes, width, &mut random),
            reduced_masses_amu: (0..n_modes).map(|_| 12.01 + random()).collect(),
            masses_amu: vec![12.011; n],
        }),
        thermo: Some(Thermo {
            temperature_k: 298.15,
            pressure_pa: 101_325.0,
            zero_point_energy_hartree: 0.11,
            enthalpy_correction_hartree: 0.13,
            entropy_hartree_per_k: 0.000_753,
            gibbs_correction_hartree: -0.0945,
            heat_capacity_hartree_per_k: Some(0.000_38),
            frequency_scale: 1.0,
            rotor_cutoff_cm1: 50.0,
            imaginary_cutoff_cm1: -25.0,
            symmetry_number: 1,
            point_group: Some("C1".into()),
        }),
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
        run: None,
    };
    (frame, result)
}

#[test]
fn encoded_modes_for_200_atoms_fit_budget() {
    let (frame, result) = fixture(200);
    result.validate(&frame).unwrap();
    let values = &result.vibrations.as_ref().unwrap().modes;
    assert_eq!(values.len(), 594 * 600);
    let bytes = serde_json::to_value(result.vibrations.as_ref().unwrap()).unwrap()["modes"]
        .as_str()
        .unwrap()
        .len();
    let pretty = serde_json::to_vec_pretty(values).unwrap().len();
    println!("200-atom normal modes: {bytes} bytes as q16, {pretty} bytes as pretty JSON numbers");
    assert!(bytes <= 1_500_000);
    assert!(bytes <= 950_400 * 101 / 100);
}

#[test]
fn light_gfn2_results_fit_both_budgets() {
    for (atoms, limit) in [(50, 50_000), (200, 200_000)] {
        let (frame, result) = fixture(atoms);
        result.validate(&frame).unwrap();
        let light = result.for_file(MAX_IMPORTED_HEAVY_BYTES, &[]);
        light.validate(&frame).unwrap();
        assert_eq!(light.heavy_bytes(), 0);
        assert_eq!(light.omitted_bytes, None);
        assert_eq!(light.charges, result.charges);
        assert_eq!(light.bond_orders, result.bond_orders);
        assert_eq!(light.thermo, result.thermo);
        let full_vibrations = result.vibrations.as_ref().unwrap();
        let light_vibrations = light.vibrations.as_ref().unwrap();
        assert_eq!(
            light_vibrations.reduced_masses_amu,
            full_vibrations.reduced_masses_amu
        );
        assert_eq!(light_vibrations.masses_amu, full_vibrations.masses_amu);
        assert!(light_vibrations.modes.is_empty());
        let bytes = result.saved_bytes(MAX_IMPORTED_HEAVY_BYTES, &[]);
        println!("{atoms}-atom light GFN2 result: {bytes} bytes");
        assert!(bytes <= limit);
    }
}

#[test]
fn light_50_atom_optimization_with_frequencies_fits_20_kb() {
    let (mut frame, mut result) = fixture(50);
    // A 50-atom hydrocarbon with the orbital levels the Calculate card requests.
    frame.numbers[16..].fill(1);
    result.frame = frame.hash();
    result.task = Task::Optimize;
    result.vibrations.as_mut().unwrap().masses_amu[16..].fill(1.008);
    let orbitals = result.orbitals.as_mut().unwrap();
    orbitals.basis = None;
    orbitals.core_electrons[16..].fill(0);
    orbitals.alpha.energies_hartree.truncate(98);
    orbitals.alpha.occupations = (0..98).map(|i| if i < 49 { 2.0 } else { 0.0 }).collect();
    orbitals.alpha.rows.clear();
    orbitals.alpha.coefficients.clear();
    let mut input = frame.clone();
    input.positions_angstrom[0][0] += 0.11;
    result.input_frame = Some(input.hash());
    result.trajectory = Some(Trajectory {
        numbers: frame.numbers.clone(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: frame.charge,
        multiplicity: frame.multiplicity,
        positions_angstrom: (0..12)
            .flat_map(|step| {
                frame
                    .positions_angstrom
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(move |(axis, &x)| {
                        x + if axis == 0 {
                            f64::from(11 - step) * 0.01
                        } else {
                            0.0
                        }
                    })
            })
            .collect(),
        energies_hartree: (0..12)
            .map(|step| result.energy_hartree.unwrap() + f64::from(11 - step) * 0.0001)
            .collect(),
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    });
    result.validate(&frame).unwrap();
    let saved = result.for_file(MAX_IMPORTED_HEAVY_BYTES, &[]);
    let json = serde_json::to_vec_pretty(&saved).unwrap();
    println!(
        "50-atom light optimization and frequencies: {} bytes",
        json.len()
    );
    assert!(json.len() < 20_000);
    let decoded: CalcResult = serde_json::from_slice(&json).unwrap();
    decoded.validate(&frame).unwrap();
    assert_eq!(decoded.task, Task::Optimize);
    assert_eq!(decoded.orbitals.unwrap().alpha.energies_hartree.len(), 98);
    let vibrations = decoded.vibrations.unwrap();
    assert_eq!(vibrations.frequencies_cm1.len(), 144);
    assert_eq!(vibrations.ir_intensities_km_mol.len(), 144);
    assert_eq!(vibrations.reduced_masses_amu.len(), 144);
    assert_eq!(vibrations.masses_amu.len(), 50);
    assert!(vibrations.modes.is_empty());
    assert_eq!(decoded.thermo, result.thermo);
    assert_eq!(decoded.trajectory.unwrap().energies_hartree.len(), 12);
}

#[test]
fn light_optimization_run_metadata_costs_at_most_two_kilobytes() {
    let (frame, result) = fixture(24);
    let mut result = result.light(&[]);
    result.task = Task::Optimize;
    result.validate(&frame).unwrap();
    let baseline = result.saved_bytes(0, &[]);
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!();
    };
    settings.threads = Some(4);
    settings.convergence = Some(Convergence::Tight);
    settings.fixed_atoms = vec![0, 11, 23];
    result.run = Some(RunSummary {
        seconds: 42.5,
        scf_iterations: (0..60).map(|i| 4 + i % 9).collect(),
        finished: Some("2026-11-26T05:03:00Z".into()),
    });
    let saved = result.for_file(0, &[]);
    saved.validate(&frame).unwrap();
    assert_eq!(saved.run, result.run);
    assert_eq!(saved.settings, result.settings);
    let bytes = result.saved_bytes(0, &[]);
    let added = bytes - baseline;
    eprintln!(
        "24-atom light optimization: {baseline} bytes without run metadata, {bytes} bytes with run metadata; {added} bytes added"
    );
    assert!(added <= 2 * 1024);
}

#[test]
fn imported_results_use_the_heavy_budget() {
    for atoms in [50, 200] {
        let (frame, mut result) = fixture(atoms);
        result.settings = Settings::Imported { keywords: vec![] };
        result.provenance.source = Source::Imported {
            format: "log".into(),
            file_name: "import.log".into(),
            file_sha256: None,
            job: 0,
        };
        result.validate(&frame).unwrap();
        let heavy = result.heavy_bytes();
        let saved = result.for_file(MAX_IMPORTED_HEAVY_BYTES, &[]);
        let saved_bytes = result.saved_bytes(MAX_IMPORTED_HEAVY_BYTES, &[]);
        println!("{atoms}-atom imported result: {heavy} heavy bytes, {saved_bytes} saved bytes");
        saved.validate(&frame).unwrap();
        if atoms == 50 {
            assert!(heavy < MAX_IMPORTED_HEAVY_BYTES);
            assert_eq!(*saved, result);
            assert_eq!(*result.for_file(heavy, &[]), result);
        } else {
            assert!(heavy > MAX_IMPORTED_HEAVY_BYTES);
            assert_eq!(saved.omitted_bytes, Some(heavy));
            assert_eq!(saved.heavy_bytes(), 0);
        }
    }
}

#[test]
fn keep_full_retains_x_tb_heavy_arrays() {
    let (frame, mut result) = fixture(50);
    result.keep_full = true;
    assert_eq!(*result.for_file(0, &[]), result);
    let bytes = result.saved_bytes(0, &[]);
    println!("50-atom keep_full GFN2 result: {bytes} bytes");
    let json = serde_json::to_vec_pretty(&result.for_file(0, &[])).unwrap();
    assert_eq!(bytes, json.len());
    let saved: CalcResult = serde_json::from_slice(&json).unwrap();
    saved.validate(&frame).unwrap();
}

#[test]
fn pinned_rows_keep_their_channels_and_basis_in_light_results() {
    let (frame, mut result) = fixture(50);
    result.bond_orders[0].order = LIGHT_BOND_ORDER_MIN;
    result.bond_orders[1].order = LIGHT_BOND_ORDER_MIN - 0.0001;
    let orbitals = result.orbitals.as_mut().unwrap();
    let width = orbitals.alpha.rows.len();
    for occupation in &mut orbitals.alpha.occupations {
        *occupation /= 2.0;
    }
    let mut beta = orbitals.alpha.clone();
    beta.coefficients.iter_mut().for_each(|v| *v = -*v);
    orbitals.beta = Beta::Unrestricted(beta);
    let pins = [
        (Spin::Beta, 3),
        (Spin::Alpha, 2),
        (Spin::Beta, 3),
        (Spin::Alpha, 800),
    ];
    let saved = result.for_file(0, &pins);
    saved.validate(&frame).unwrap();
    assert_eq!(saved.bond_orders.len(), result.bond_orders.len() - 1);
    assert_eq!(saved.bond_orders[0].order, LIGHT_BOND_ORDER_MIN);
    let saved_orbitals = saved.orbitals.as_ref().unwrap();
    let full = result.orbitals.as_ref().unwrap();
    assert_eq!(saved_orbitals.basis, full.basis);
    assert_eq!(saved_orbitals.alpha.rows, [2]);
    assert_eq!(
        saved_orbitals.alpha.coefficients,
        full.alpha.coefficients[2 * width..3 * width]
    );
    let (Beta::Unrestricted(saved_beta), Beta::Unrestricted(full_beta)) =
        (&saved_orbitals.beta, &full.beta)
    else {
        panic!("unrestricted orbitals required")
    };
    assert_eq!(saved_beta.rows, [3]);
    assert_eq!(
        saved_beta.coefficients,
        full_beta.coefficients[3 * width..4 * width]
    );
    println!(
        "50-atom GFN2 result with two pinned rows: {} bytes",
        result.saved_bytes(0, &pins)
    );
    drop(saved);

    for beta in [
        Beta::Restricted,
        Beta::SameOrbitals {
            occupations: vec![0.0; width],
        },
    ] {
        result.orbitals.as_mut().unwrap().beta = beta;
        let saved = result.light(&[(Spin::Beta, 7), (Spin::Alpha, 7), (Spin::Beta, 4)]);
        saved.validate(&frame).unwrap();
        let orbitals = saved.orbitals.unwrap();
        assert!(orbitals.basis.is_some());
        assert_eq!(orbitals.alpha.rows, [4, 7]);
        let full = &result.orbitals.as_ref().unwrap().alpha.coefficients;
        assert_eq!(
            &orbitals.alpha.coefficients[..width],
            &full[4 * width..5 * width]
        );
        assert_eq!(
            &orbitals.alpha.coefficients[width..],
            &full[7 * width..8 * width]
        );
    }
}
