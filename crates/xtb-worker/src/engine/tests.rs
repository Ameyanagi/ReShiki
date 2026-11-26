use super::*;
use std::fmt::Write;

const WATER: &str = include_str!("../../../../tests/fixtures/xtb/water.xyz");
const DRIVER_METHODS: [(Method, &str, &str); 3] = [
    (Method::Gfn2, "gfn2", "xtb"),
    (Method::Gxtb, "gxtb-v2.0.1", "gxtb"),
    (Method::Gfnff, "gfnff", "xtb"),
];

fn water(method: Method, conventions: Conventions) -> Open {
    let positions: Vec<[f64; 3]> = WATER
        .lines()
        .skip(2)
        .map(|line| {
            let mut coordinates = line.split_whitespace().skip(1);
            std::array::from_fn(|_| coordinates.next().unwrap().parse::<f64>().unwrap())
        })
        .collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "numbers": [8,1,1], "positions": positions, "units": "angstrom",
        "method": method, "conventions": conventions, "heap_bytes": 2_147_483_648_u64
    }))
    .unwrap()
}

const THERMO_WATER: &str = "3
water
O 0 0 0.119262
H 0 0.763239 -0.477047
H 0 -0.763239 -0.477047
";
const THERMO_CO2: &str = "3
linear carbon dioxide
O -1.16 0 0
C 0 0 0
O 1.16 0 0
";
const THERMO_AMMONIA: &str = "4
pyramidal ammonia
N 0 0 0.2
H 0.94 0 -0.2
H -0.47 0.8140638795573723 -0.2
H -0.47 -0.8140638795573723 -0.2
";
const THERMO_METHANE: &str = "5
tetrahedral methane
C 0 0 0
H 0.63 0.63 0.63
H 0.63 -0.63 -0.63
H -0.63 0.63 -0.63
H -0.63 -0.63 0.63
";
const THERMO_BENZENE: &str = "12
planar benzene
C 1.4 0 0
C 0.7 1.212435565298214 0
C -0.7 1.212435565298214 0
C -1.4 0 0
C -0.7 -1.212435565298214 0
C 0.7 -1.212435565298214 0
H 2.48 0 0
H 1.24 2.147743001385408 0
H -1.24 2.147743001385408 0
H -2.48 0 0
H -1.24 -2.147743001385408 0
H 1.24 -2.147743001385408 0
";
const THERMO_ETHYLENE: &str = "6
planar ethylene
C -0.67 0 0
C 0.67 0 0
H -1.235 0.932 0
H -1.235 -0.932 0
H 1.235 0.932 0
H 1.235 -0.932 0
";
const THERMO_WATER_DIMER: &str = "6
unoptimized water dimer
O 0 0 0.119262
H 0 0.763239 -0.477047
H 0 -0.763239 -0.477047
O 3.5 0.2 0.519262
H 3.5 0.963239 -0.077047
H 3.5 -0.563239 -0.077047
";
const THERMO_PLANAR_AMMONIA: &str = "4
planar ammonia
N 0 0 0
H 0.94 0 0
H -0.47 0.8140638795573723 0
H -0.47 -0.8140638795573723 0
";
const THERMO_SMALL_IMAGINARY_AMMONIA: &str = "4
small imaginary ammonia
N 0 0 0
H 0.94 0 0.2434640824794769
H -0.47 0.8140638795573723 0.2434640824794769
H -0.47 -0.8140638795573723 0.2434640824794769
";

fn thermo_xyz(xyz: &str) -> Open {
    let mut lines = xyz.lines();
    let count: usize = lines.next().unwrap().parse().unwrap();
    lines.next().unwrap();
    let mut numbers = Vec::new();
    let mut positions = Vec::new();
    for line in lines {
        let mut fields = line.split_whitespace();
        numbers.push(match fields.next().unwrap() {
            "H" => 1,
            "C" => 6,
            "N" => 7,
            "O" => 8,
            symbol => panic!("unsupported thermochemistry fixture atom {symbol}"),
        });
        positions.push(std::array::from_fn(|_| {
            fields.next().unwrap().parse::<f64>().unwrap()
        }));
        assert!(fields.next().is_none());
    }
    assert_eq!(numbers.len(), count);
    Open {
        numbers,
        positions,
        ..water(Method::Gfn2, Conventions::Axtb)
    }
}

fn public_thermo_errors(open: &Open, computed: &Computed) -> [f64; 5] {
    use reshiki_qcdata::vibrational::{ThermoInput, thermochemistry};

    let vibrations = computed.vibrations.as_ref().unwrap();
    let expected = computed.thermo.as_ref().unwrap();
    let actual = thermochemistry(&ThermoInput {
        frequencies_cm1: &vibrations.frequencies_cm1,
        masses_amu: &vibrations.masses_amu,
        positions_angstrom: &open.positions,
        symmetry_number: expected.symmetry_number,
        linear: None,
        temperature_k: expected.temperature_k,
        pressure_pa: expected.pressure_pa,
        frequency_scale: expected.frequency_scale,
        rotor_cutoff_cm1: expected.rotor_cutoff_cm1,
        imaginary_cutoff_cm1: expected.imaginary_cutoff_cm1,
    });
    [
        actual.zero_point_energy_hartree - expected.zero_point_energy_hartree,
        actual.enthalpy_correction_hartree - expected.enthalpy_correction_hartree,
        actual.gibbs_correction_hartree - expected.gibbs_correction_hartree,
        actual.entropy_hartree_per_k - expected.entropy_hartree_per_k,
        actual.heat_capacity_hartree_per_k.unwrap() - expected.heat_capacity_hartree_per_k.unwrap(),
    ]
    .map(f64::abs)
}

fn assert_thermo_maximum_errors(errors: &[(String, [f64; 5])]) {
    let quantities = ["ZPVE", "H", "G", "S", "heat capacity"];
    let tolerances = [1e-7, 1e-7, 1e-7, 1e-10, 1e-10];
    let mut maxima = [0.0_f64; 5];
    let mut cases = [""; 5];
    for (case, errors) in errors {
        for ((&error, quantity), (maximum, maximum_case)) in errors
            .iter()
            .zip(quantities)
            .zip(maxima.iter_mut().zip(&mut cases))
        {
            assert!(error.is_finite(), "{quantity} is nonfinite: {case}");
            if error >= *maximum {
                *maximum = error;
                *maximum_case = case;
            }
        }
    }
    for ((quantity, maximum), case) in quantities.into_iter().zip(maxima).zip(cases) {
        eprintln!(
            "maximum {} absolute error = {:.12e} ({})",
            quantity, maximum, case
        );
    }
    for (((quantity, maximum), tolerance), case) in quantities
        .into_iter()
        .zip(maxima)
        .zip(tolerances)
        .zip(cases)
    {
        assert!(
            maximum <= tolerance,
            "{} error {:.12e} exceeds {}: {}",
            quantity,
            maximum,
            tolerance,
            case
        );
    }
}

#[test]
fn public_rotational_symmetry_matches_the_unlabelled_worker() {
    use reshiki_qcdata::vibrational::rotational_symmetry_number;

    for (name, xyz, expected) in [
        ("water", THERMO_WATER, 2),
        ("ammonia", THERMO_AMMONIA, 3),
        ("methane", THERMO_METHANE, 12),
        ("benzene", THERMO_BENZENE, 12),
        ("ethylene", THERMO_ETHYLENE, 4),
        ("CO2", THERMO_CO2, 2),
    ] {
        let open = thermo_xyz(xyz);
        let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
        // Leave masses and the symmetry override unset so the worker supplies
        // its ordinary, unlabelled thermochemistry symmetry number.
        let computed = session
            .compute(Compute {
                hessian: Some(Default::default()),
                ..Compute::default()
            })
            .unwrap();
        let engine_symmetry = computed.thermo.as_ref().unwrap().symmetry_number;
        let masses = &computed.vibrations.as_ref().unwrap().masses_amu;
        let labels: Vec<_> = open.numbers.iter().map(|&z| u32::from(z) * 1000).collect();
        let public_symmetry = rotational_symmetry_number(&labels, &open.positions, masses);

        assert_eq!(engine_symmetry, expected, "{name}: worker symmetry");
        assert_eq!(
            public_symmetry, engine_symmetry,
            "{name}: public symmetry versus worker"
        );
    }
}

#[test]
fn public_thermochemistry_matches_the_worker_over_temperature_and_pressure() {
    let mut errors = Vec::new();
    for (name, xyz) in [
        ("water", THERMO_WATER),
        ("linear CO2", THERMO_CO2),
        ("unoptimized water dimer", THERMO_WATER_DIMER),
        ("planar ammonia", THERMO_PLANAR_AMMONIA),
        ("small imaginary ammonia", THERMO_SMALL_IMAGINARY_AMMONIA),
    ] {
        let open = thermo_xyz(xyz);
        let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
        for temperature_k in [298.15, 150.0, 600.0] {
            for pressure_pa in [101_325.0, 100_000.0] {
                let computed = session
                    .compute(Compute {
                        hessian: Some(protocol::HessianRequest {
                            thermo: protocol::ThermoRequest {
                                temperature_k: Some(temperature_k),
                                pressure_pa: Some(pressure_pa),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        ..Compute::default()
                    })
                    .unwrap();
                let vibrations = computed.vibrations.as_ref().unwrap();
                let frequencies = &vibrations.frequencies_cm1;
                let thermo = computed.thermo.as_ref().unwrap();
                assert_eq!(thermo.temperature_k, temperature_k, "{name}: temperature");
                assert_eq!(thermo.pressure_pa, pressure_pa, "{name}: pressure");
                assert_eq!(vibrations.linear, name == "linear CO2", "{name}: linearity");
                assert_eq!(
                    frequencies.len(),
                    3 * open.numbers.len() - if vibrations.linear { 5 } else { 6 },
                    "{name}: vibrational mode count"
                );
                match name {
                    "unoptimized water dimer" => {
                        assert!(
                            frequencies.iter().any(|&f| f > 1.0 && f < 50.0),
                            "{name}: expected a positive mode below 50 cm^-1"
                        );
                        assert_eq!(thermo.symmetry_number, 1, "{name}: asymmetric rotor");
                    }
                    "planar ammonia" => {
                        assert!(
                            frequencies.iter().any(|&f| f < -20.0),
                            "{name}: large imaginary mode"
                        );
                    }
                    "small imaginary ammonia" => {
                        assert!(
                            frequencies.iter().any(|&f| f > -20.0 && f < -1.0),
                            "{name}: small imaginary mode"
                        );
                        assert!(
                            frequencies.iter().all(|&f| f > -20.0),
                            "{name}: no large imaginary mode"
                        );
                    }
                    _ => assert!(
                        frequencies.iter().all(|&f| f > 100.0),
                        "{name}: positive modes"
                    ),
                }
                errors.push((
                    format!("{name}, T={temperature_k} K, p={pressure_pa} Pa"),
                    public_thermo_errors(&open, &computed),
                ));
            }
        }
    }
    assert_thermo_maximum_errors(&errors);
}

#[test]
fn public_thermochemistry_matches_frequency_scaling_and_sub_wavenumber_modes() {
    let mut errors = Vec::new();
    for (name, xyz, frequency_scale, masses_amu) in [
        ("water scaled across 1 cm-1", THERMO_WATER, 0.0003, None),
        ("water scale 1e-6", THERMO_WATER, 1e-6, None),
        ("water scale 1e-10", THERMO_WATER, 1e-10, None),
        (
            "planar ammonia scaled across imaginary cutoff",
            THERMO_PLANAR_AMMONIA,
            0.01,
            None,
        ),
        (
            "water analysis masses multiplied by 1e8",
            THERMO_WATER,
            1.0,
            Some(vec![15.999e8, 1.008e8, 1.008e8]),
        ),
    ] {
        let open = thermo_xyz(xyz);
        let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
        let computed = session
            .compute(Compute {
                hessian: Some(protocol::HessianRequest {
                    thermo: protocol::ThermoRequest {
                        frequency_scale: Some(frequency_scale),
                        symmetry_number: Some(if xyz == THERMO_WATER { 2 } else { 6 }),
                        ..Default::default()
                    },
                    masses_amu,
                }),
                ..Compute::default()
            })
            .unwrap();
        let frequencies = &computed.vibrations.as_ref().unwrap().frequencies_cm1;
        match name {
            "water scaled across 1 cm-1" => {
                assert!(frequencies.iter().all(|&f| f > 1.0));
                assert!(frequencies.iter().any(|&f| f * frequency_scale < 1.0));
                assert!(frequencies.iter().any(|&f| f * frequency_scale > 1.0));
            }
            "planar ammonia scaled across imaginary cutoff" => {
                assert!(
                    frequencies
                        .iter()
                        .any(|&f| f < -20.0 && f * frequency_scale > -20.0)
                );
            }
            "water analysis masses multiplied by 1e8" => {
                assert_eq!(frequencies.len(), 3);
                assert!(frequencies.iter().all(|&f| f > 0.0 && f < 1.0));
            }
            "water scale 1e-6" | "water scale 1e-10" => {
                assert!(frequencies.iter().all(|&f| f * frequency_scale < 0.01));
            }
            _ => unreachable!(),
        }
        errors.push((name.to_owned(), public_thermo_errors(&open, &computed)));
    }
    assert_thermo_maximum_errors(&errors);
}

#[test]
fn public_thermochemistry_matches_the_imaginary_cutoff_boundary() {
    let open = thermo_xyz(THERMO_SMALL_IMAGINARY_AMMONIA);
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let baseline = session
        .compute(Compute {
            hessian: Some(Default::default()),
            ..Compute::default()
        })
        .unwrap();
    let cutoff = baseline
        .vibrations
        .as_ref()
        .unwrap()
        .frequencies_cm1
        .first()
        .unwrap()
        .abs();
    assert!(cutoff > 1.0 && cutoff < 20.0);
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let computed = session
        .compute(Compute {
            hessian: Some(protocol::HessianRequest {
                thermo: protocol::ThermoRequest {
                    imaginary_cutoff_cm1: Some(cutoff),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Compute::default()
        })
        .unwrap();
    assert_eq!(
        computed.vibrations.as_ref().unwrap().frequencies_cm1,
        baseline.vibrations.as_ref().unwrap().frequencies_cm1
    );
    assert_thermo_maximum_errors(&[(
        "imaginary mode at cutoff".to_owned(),
        public_thermo_errors(&open, &computed),
    )]);
}

#[test]
fn angstrom_conversion_matches_the_driver_bit_for_bit() {
    for xyz in [
        WATER,
        include_str!("../../../../tests/fixtures/xtb/caffeine.xyz"),
        include_str!("../../../../tests/fixtures/xtb/ferrocene.xyz"),
        &random_xyz(),
    ] {
        let angstrom: Vec<[f64; 3]> = xyz
            .lines()
            .skip(2)
            .map(|line| {
                let mut coordinates = line.split_whitespace().skip(1);
                std::array::from_fn(|_| coordinates.next().unwrap().parse().unwrap())
            })
            .collect();
        for (method, name, compatible) in DRIVER_METHODS {
            for (conventions, expected_conventions) in [
                (Conventions::Axtb, "axtb"),
                (Conventions::Xtb68, compatible),
            ] {
                let (_, expected) = axtb_driver::read_xyz(
                    xyz,
                    axtb_driver::Method::parse(name).unwrap(),
                    Some(expected_conventions),
                )
                .unwrap();
                let scale = angstrom_to_bohr(
                    driver_method(method),
                    driver_conventions(method, conventions),
                )
                .unwrap();
                let actual = to_bohr(angstrom.clone(), Units::Angstrom, scale).unwrap();
                assert_eq!(actual.len(), expected.len());
                for (actual, expected) in actual.into_iter().zip(expected) {
                    assert_eq!(
                        actual.map(f64::to_bits),
                        expected.map(f64::to_bits),
                        "{method:?}/{conventions:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn water_matches_the_pinned_driver_for_every_method_and_conventions() {
    for (method, name, compatible) in DRIVER_METHODS {
        for (conventions, expected_conventions) in [
            (Conventions::Axtb, "axtb"),
            (Conventions::Xtb68, compatible),
        ] {
            let open = water(method, conventions);
            let (mut session, opened) =
                Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
            let compute = Compute {
                gradient: true,
                bond_orders: method != Method::Gfnff,
                orbitals: if method == Method::Gfnff {
                    OrbitalOutput::None
                } else {
                    OrbitalOutput::Levels
                },
                ..Compute::default()
            };
            let actual = session.compute(compute).unwrap();
            let (numbers, positions) = axtb_driver::read_xyz(
                WATER,
                axtb_driver::Method::parse(name).unwrap(),
                Some(expected_conventions),
            )
            .unwrap();
            let expected = axtb_driver::run(&Request {
                numbers,
                positions,
                method: Some(name.into()),
                conventions: Some(expected_conventions.into()),
                restart: (method != Method::Gfnff).then(|| "previous".into()),
                gradient: true,
                bond_orders: method != Method::Gfnff,
                ..Request::default()
            })
            .unwrap();
            assert_eq!(
                actual.energy.to_bits(),
                expected.energy.unwrap().to_bits(),
                "{method:?}/{conventions:?}"
            );
            assert_eq!(
                actual.charges.as_ref().unwrap().values,
                expected.charges.unwrap()
            );
            assert_eq!(actual.gradient, expected.gradient);
            assert_eq!(opened.engine_conventions, expected.conventions);
            assert_eq!(actual.numerics, expected.numerics);
            if let Some(scf) = expected.scf {
                let orbitals = actual.orbitals.unwrap();
                assert_eq!(orbitals.alpha.energies_hartree, scf.orbital_energies_alpha);
                assert_eq!(
                    orbitals.alpha.occupations,
                    scf.occupations_alpha
                        .iter()
                        .zip(&scf.occupations_beta)
                        .map(|(a, b)| a + b)
                        .collect::<Vec<_>>()
                );
                let matrix = scf.bond_orders.unwrap();
                let expected = (0..3)
                    .flat_map(|a| {
                        let matrix = &matrix;
                        (a + 1..3).filter_map(move |b| {
                            let order = *matrix.data.get(a * 3 + b).unwrap();
                            (order >= 0.1).then_some(BondOrder {
                                a: a as u32,
                                b: b as u32,
                                order,
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual.bond_orders.unwrap(), expected);
            } else {
                assert!(opened.basis_functions.is_none());
                assert_eq!(actual.charges.as_ref().unwrap().scheme, ChargeScheme::Eeq);
                let settings = actual.settings.gfnff.unwrap();
                assert_eq!(settings.reference_positions_bohr, session.positions);
                assert_eq!(settings.fragment_charges, vec![0]);
            }
        }
    }
}

#[test]
fn water_analytic_hessian_frequencies_match_the_driver() {
    let open = water(Method::Gfn2, Conventions::Axtb);
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let actual = session
        .compute(Compute {
            hessian: Some(Default::default()),
            bond_orders: true,
            orbitals: protocol::OrbitalOutput::Levels,
            ..Compute::default()
        })
        .unwrap();
    let (numbers, positions) =
        axtb_driver::read_xyz(WATER, axtb_driver::Method::Gfn2, Some("axtb")).unwrap();
    let expected = axtb_driver::run(&Request {
        numbers,
        positions,
        method: Some("gfn2".into()),
        restart: Some("previous".into()),
        hessian: true,
        hessian_method: Some("analytic".into()),
        bond_orders: true,
        ..Request::default()
    })
    .unwrap();
    assert_eq!(actual.energy.to_bits(), expected.energy.unwrap().to_bits());
    assert_eq!(
        actual.hessian,
        expected.hessian.as_ref().unwrap().hessian.data
    );
    assert_eq!(actual.hessian_method.as_deref(), Some("analytic"));
    assert!(!actual.bond_orders.as_ref().unwrap().is_empty());
    let levels = &actual.orbitals.as_ref().unwrap().alpha.energies_hartree;
    assert!(!levels.is_empty());
    let frame = open.admit(None, None, protocol::MAX_ATOMS).unwrap();
    let result = actual
        .to_calc_result(
            &frame,
            reshiki_qcdata::Task::Hessian,
            &hello().engine.unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(result.thermo.unwrap().point_group.as_deref(), Some("C2v"));
    let vibration = actual.vibrations.unwrap();
    let reference = expected.vibrations.unwrap();
    assert_eq!(vibration.frequencies_cm1, reference.frequencies);
    assert_eq!(vibration.ir_intensities_km_mol, reference.ir_intensities);
    assert_eq!(vibration.reduced_masses_amu, reference.reduced_masses);
    assert_eq!(
        actual.normal_modes,
        reference
            .normal_modes
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        actual.thermo.unwrap().gibbs_correction_hartree.to_bits(),
        reference.thermo.unwrap().last.g_rrho.to_bits()
    );
}

#[test]
fn explicit_isotope_masses_lower_water_frequencies() {
    let open = water(Method::Gfn2, Conventions::Axtb);
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let mut request = Compute {
        hessian: Some(Default::default()),
        ..Compute::default()
    };
    let light = session
        .compute(request.clone())
        .unwrap()
        .vibrations
        .unwrap();
    let masses = vec![16.0, 2.014, 2.014];
    let hessian = request.hessian.as_mut().unwrap();
    hessian.masses_amu = Some(masses.clone());
    hessian.thermo.symmetry_number = Some(2);
    let heavy = session.compute(request).unwrap();
    let vibrations = heavy.vibrations.as_ref().unwrap();
    assert_eq!(vibrations.masses_amu, masses);
    assert_eq!(vibrations.frequencies_cm1.len(), 3);
    for (deuterated, normal) in vibrations.frequencies_cm1.iter().zip(light.frequencies_cm1) {
        assert!(*deuterated < normal);
    }

    let mut frame = open.admit(None, None, protocol::MAX_ATOMS).unwrap();
    let engine = hello().engine.unwrap();
    frame.isotopes = vec![0, 2, 2];
    assert!(matches!(
        heavy.to_calc_result(&frame, reshiki_qcdata::Task::Hessian, &engine, None),
        Err(reshiki_qcdata::Error::Unsupported(message))
            if message == "isotopes need explicit masses for the vibrational analysis"
    ));
    frame.masses_amu = masses;
    heavy
        .to_calc_result(&frame, reshiki_qcdata::Task::Hessian, &engine, None)
        .unwrap();
    *frame.masses_amu.get_mut(1).unwrap() = 2.0;
    assert!(matches!(
        heavy.to_calc_result(&frame, reshiki_qcdata::Task::Hessian, &engine, None),
        Err(reshiki_qcdata::Error::Invalid(message))
            if message == "frame masses differ from the vibrational analysis masses"
    ));
}

#[test]
fn isotopologue_symmetry_numbers_fix_the_entropy() {
    use reshiki_qcdata::vibrational::rotational_symmetry_number;

    // xTB 6.8.0 optimized geometries.
    const OPTIMIZED_WATER: &str = "3
water, E = -5.070544442589 Eh
O 0 0 0.10571016604457
H 0 0.77208696468061 -0.46340508302229
H 0 -0.77208696468061 -0.46340508302229
";
    const OPTIMIZED_METHANE: &str = "5
methane, E = -4.175218521783 Eh
C 0 0 0
H 0.624789300332 0.624789300332 0.624789300332
H -0.624789300332 -0.624789300332 0.624789300332
H -0.624789300332 0.624789300332 -0.624789300332
H 0.624789300332 -0.624789300332 -0.624789300332
";
    const DEUTERIUM_MASS: f64 = 2.014_101_778;
    const TEMPERATURE_K: f64 = 298.15;
    const BOLTZMANN_HARTREE_PER_K: f64 = 1.380_649e-23 / 4.359_744_722_206e-18;
    const KCAL_PER_MOL_PER_HARTREE: f64 = 627.509_474;

    struct Isotopologue {
        name: &'static str,
        xyz: &'static str,
        isotopes_and_masses: &'static [(u16, f64)],
        symmetry: u32,
        xtb_gibbs_correction_hartree: f64,
    }

    // The raw xTB comparison is informational because xTB 6.8.0 uses the
    // original masses for rigid-body projection in its isotope runs.
    println!(
        "{:<8} {:>2}  {:>14}  {:>14}  {:>14}  {:>20}  {:>20}  {:>20}",
        "Molecule",
        "σ",
        "ZPVE (Eh)",
        "G ours (Eh)",
        "G xTB raw (Eh)",
        "ΔG vs xTB (kcal/mol)",
        "ΔG vs σ=1 (kcal/mol)",
        "S ours (cal/(mol·K))"
    );
    for isotope in [
        Isotopologue {
            name: "D2O",
            xyz: OPTIMIZED_WATER,
            isotopes_and_masses: &[(0, 15.999), (2, DEUTERIUM_MASS), (2, DEUTERIUM_MASS)],
            symmetry: 2,
            xtb_gibbs_correction_hartree: -0.004_106_040_027,
        },
        Isotopologue {
            name: "HDO",
            xyz: OPTIMIZED_WATER,
            isotopes_and_masses: &[(0, 15.999), (2, DEUTERIUM_MASS), (0, 1.008)],
            symmetry: 1,
            xtb_gibbs_correction_hartree: -0.000_824_325_779,
        },
        Isotopologue {
            name: "CD4",
            xyz: OPTIMIZED_METHANE,
            isotopes_and_masses: &[
                (0, 12.011),
                (2, DEUTERIUM_MASS),
                (2, DEUTERIUM_MASS),
                (2, DEUTERIUM_MASS),
                (2, DEUTERIUM_MASS),
            ],
            symmetry: 12,
            xtb_gibbs_correction_hartree: 0.009_036_700_801,
        },
    ] {
        let name = isotope.name;
        let open = Open {
            conventions: Conventions::Xtb68,
            ..thermo_xyz(isotope.xyz)
        };
        let labels: Vec<_> = open
            .numbers
            .iter()
            .zip(isotope.isotopes_and_masses)
            .map(|(&z, &(isotope, _))| u32::from(z) * 1000 + u32::from(isotope))
            .collect();
        let masses: Vec<_> = isotope
            .isotopes_and_masses
            .iter()
            .map(|&(_, mass)| mass)
            .collect();
        let symmetry = rotational_symmetry_number(&labels, &open.positions, &masses);
        assert_eq!(symmetry, isotope.symmetry, "{name}: rotational symmetry");

        let heap_budget = open.heap_budget().unwrap();
        let (mut session, _) = Session::open(open, heap_budget).unwrap();
        let mut compute = |masses_amu, symmetry_number| {
            session
                .compute(Compute {
                    hessian: Some(protocol::HessianRequest {
                        masses_amu,
                        thermo: protocol::ThermoRequest {
                            temperature_k: Some(TEMPERATURE_K),
                            symmetry_number,
                            ..Default::default()
                        },
                    }),
                    ..Compute::default()
                })
                .unwrap()
        };
        let computed = compute(Some(masses.clone()), Some(symmetry));
        let thermo = computed.thermo.unwrap();
        let unit_symmetry = compute(Some(masses), Some(1)).thermo.unwrap();
        assert_eq!(thermo.symmetry_number, symmetry, "{name}: worker symmetry");

        let delta_entropy = thermo.entropy_hartree_per_k - unit_symmetry.entropy_hartree_per_k;
        let expected_delta_entropy = -BOLTZMANN_HARTREE_PER_K * f64::from(symmetry).ln();
        assert!(
            (delta_entropy - expected_delta_entropy).abs() <= 1e-12,
            "{name}: entropy shift {delta_entropy:.12e} Eh/K, expected {expected_delta_entropy:.12e} Eh/K"
        );

        let zpve = thermo.zero_point_energy_hartree;
        let gibbs = thermo.gibbs_correction_hartree;
        let delta_gibbs = gibbs - unit_symmetry.gibbs_correction_hartree;
        // The symmetry correction lowers S, so G = H - T*S increases.
        let expected_delta_gibbs = -TEMPERATURE_K * expected_delta_entropy;
        assert!(
            (delta_gibbs - expected_delta_gibbs).abs() <= 1e-9,
            "{name}: Gibbs correction shift {delta_gibbs:.12} Eh, expected {expected_delta_gibbs:.12} Eh"
        );

        let xtb_gibbs = isotope.xtb_gibbs_correction_hartree;
        let entropy_cal_per_mol_k =
            thermo.entropy_hartree_per_k * KCAL_PER_MOL_PER_HARTREE * 1000.0;
        let delta_g_vs_xtb = (gibbs - xtb_gibbs) * KCAL_PER_MOL_PER_HARTREE;
        let delta_g_vs_unit_symmetry = delta_gibbs * KCAL_PER_MOL_PER_HARTREE;
        println!(
            "{name:<8} {symmetry:>2}  {zpve:>14.10}  {gibbs:>14.10}  {xtb_gibbs:>14.10}  {delta_g_vs_xtb:>20.6}  {delta_g_vs_unit_symmetry:>20.6}  {entropy_cal_per_mol_k:>20.6}"
        );

        if name == "D2O" {
            let d2o: [f64; 3] = computed
                .vibrations
                .unwrap()
                .frequencies_cm1
                .try_into()
                .expect("three D2O frequencies");
            let h2o: [f64; 3] = compute(None, None)
                .vibrations
                .unwrap()
                .frequencies_cm1
                .try_into()
                .expect("three H2O frequencies");
            for (mode, (heavy, light)) in ["bend", "lower stretch", "upper stretch"]
                .into_iter()
                .zip(d2o.into_iter().zip(h2o))
            {
                let ratio = heavy / light;
                assert!(
                    light > 0.0 && (0.70..=0.76).contains(&ratio),
                    "{mode}: D2O/H2O frequency ratio was {ratio:.6} ({heavy:.2}/{light:.2} cm^-1)"
                );
            }
        }
    }
}

#[test]
fn cef3_resolves_spin_from_valence_electrons() {
    let mut open = Open {
        numbers: vec![58, 9, 9, 9],
        positions: vec![[0.0; 3], [2.0, 0.0, 0.0], [-2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        ..water(Method::Gfn2, Conventions::Axtb)
    };
    let frame = open.admit(None, None, protocol::MAX_ATOMS).unwrap();
    let engine = hello().engine.unwrap();
    for uhf in [Some(0), None] {
        open.uhf = uhf;
        let (mut session, opened) =
            Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
        assert_eq!(opened.uhf, 0);
        assert_eq!(session.settings.uhf, 0);
        let computed = session.compute(Compute::default()).unwrap();
        assert!(matches!(
            computed.to_calc_result(&frame, reshiki_qcdata::Task::Energy, &engine, None),
            Err(reshiki_qcdata::Error::Unsupported(message))
                if message == "the xTB valence spin (uhf 0) differs from the frame multiplicity 2; this cannot be stored yet"
        ));
    }
}

#[test]
fn water_gxtb_alpb_reports_each_composite_part() {
    let open = Open {
        solvent: Some(protocol::Solvent {
            model: SolventModel::Alpb,
            name: "water".into(),
            state: protocol::SolvationState::Gsolv,
            correction: Some(protocol::SolventCorrection::Gfn2),
        }),
        ..water(Method::Gxtb, Conventions::Axtb)
    };
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let computed = session
        .compute(Compute {
            hessian: Some(Default::default()),
            ..Compute::default()
        })
        .unwrap();
    let actual = computed.composite.as_ref().unwrap();
    let (numbers, positions) =
        axtb_driver::read_xyz(WATER, axtb_driver::Method::GxtbV2_0_1, Some("axtb")).unwrap();
    let expected = axtb_driver::run(&Request {
        numbers,
        positions,
        method: Some("gxtb-v2.0.1".into()),
        conventions: Some("axtb".into()),
        restart: Some("previous".into()),
        alpb: Some("water".into()),
        solvation_correction: Some("gfn2".into()),
        hessian: true,
        hessian_method: Some("analytic".into()),
        ..Request::default()
    })
    .unwrap()
    .composite
    .unwrap();
    for (actual_parts, expected_parts, hessian) in [
        (&actual.parts, &expected.parts, false),
        (
            actual.hessian_parts.as_ref().unwrap(),
            expected.hessian_parts.as_ref().unwrap(),
            true,
        ),
    ] {
        assert_eq!(
            actual_parts.keys().map(String::as_str).collect::<Vec<_>>(),
            ["gfn2_gas", "gfn2_solvated", "gxtb"]
        );
        for (name, part) in actual_parts {
            let expected = expected_parts.get(name).unwrap();
            assert_eq!(part.method, expected.method);
            assert_eq!(part.conventions, expected.conventions);
            assert_eq!(part.electronic_temperature_k, expected.etemp);
            assert_eq!(part.max_iterations, expected.max_iter);
            if hessian {
                assert_eq!(expected.iterations, 0);
                assert_eq!(part.iterations, None);
            } else {
                assert!(expected.iterations > 0);
                assert_eq!(part.iterations, Some(expected.iterations));
            }
            assert_eq!(part.retry, expected.retry);
        }
    }
    assert_eq!(actual.components.len(), expected.components.0.len());
    let frame = open.admit(None, None, protocol::MAX_ATOMS).unwrap();
    let result = computed
        .to_calc_result(
            &frame,
            reshiki_qcdata::Task::Hessian,
            &hello().engine.unwrap(),
            None,
        )
        .unwrap();
    for (name, energy) in expected.components.0 {
        assert_eq!(actual.components[name], energy);
        assert_eq!(result.energy_terms[&format!("composite.{name}")], energy);
    }
}

#[test]
fn water_scf_retries_with_the_requested_iteration_limit() {
    let mut open = water(Method::Gfn2, Conventions::Axtb);
    open.scf.max_iterations = Some(1);
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    assert_eq!(
        session.compute(Compute::default()).unwrap_err().kind,
        ErrorKind::Scf
    );

    open.scf.retries.push(protocol::ScfRetry {
        max_iterations: Some(250),
        electronic_temperature_k: None,
    });
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let result = session.compute(Compute::default()).unwrap();
    let retry = result.scf.unwrap().retry.unwrap();
    assert_eq!((retry.rung, retry.of), (1, 1));
}

#[test]
fn scf_restarts_and_refused_gfnff_requests_leave_geometry_intact() {
    for method in [Method::Gfn2, Method::Gxtb] {
        let open = water(method, Conventions::Axtb);
        let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
        let first = session.compute(Compute::default()).unwrap();
        assert_ne!(first.scf.unwrap().start, "previous");
        let second = session.compute(Compute::default()).unwrap();
        assert_eq!(second.scf.unwrap().start, "previous");
    }
    let open = water(Method::Gfnff, Conventions::Axtb);
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let first = session.compute(Compute::default()).unwrap();
    let retained = session.positions.clone();
    let mut replacement = retained.clone();
    replacement.get_mut(1).unwrap()[0] += 0.7;
    for compute in [
        Compute {
            positions: Some(replacement.clone()),
            orbitals: OrbitalOutput::Levels,
            ..Compute::default()
        },
        Compute {
            positions: Some(replacement),
            bond_orders: true,
            ..Compute::default()
        },
    ] {
        assert_eq!(
            session.compute(compute).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(session.positions, retained);
    }
    assert_eq!(
        session
            .compute(Compute::default())
            .unwrap()
            .energy
            .to_bits(),
        first.energy.to_bits()
    );
}

#[test]
fn unsupported_gfnff_open_settings_are_refused_before_setup() {
    for fields in [
        serde_json::json!({"solvent": {"model":"alpb", "name":"water"}}),
        serde_json::json!({"scf": {"accuracy":1.0}}),
        serde_json::json!({"uhf":2}),
    ] {
        let mut open = serde_json::to_value(water(Method::Gfnff, Conventions::Axtb)).unwrap();
        open.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let open: Open = serde_json::from_value(open).unwrap();
        let error = match Session::open(open.clone(), open.heap_budget().unwrap()) {
            Ok(_) => panic!("unsupported GFN-FF settings were accepted"),
            Err(error) => error,
        };
        assert_eq!(error.kind, ErrorKind::Unsupported);
    }
}

fn random_xyz() -> String {
    let mut xyz = String::from("100\nDeterministic random coordinates\n");
    let mut seed = 0x5209_ab8d_u32;
    let mut coordinate = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (f64::from(seed) / f64::from(u32::MAX) - 0.5) * 1000.0
    };
    for _ in 0..100 {
        writeln!(xyz, "H {} {} {}", coordinate(), coordinate(), coordinate()).unwrap();
    }
    xyz
}

#[test]
fn worker_streams_the_computed_arrays_in_announced_order() {
    use crate::protocol::{Closed, FrameKind, Outcome, RequestOp, Response};
    use std::io::{Cursor, Read};

    let open = water(Method::Gfn2, Conventions::Axtb);
    let compute = Compute {
        hessian: Some(Default::default()),
        orbitals: OrbitalOutput::All,
        ..Compute::default()
    };
    let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
    let expected = session.compute(compute.clone()).unwrap();
    let mut input = Vec::new();
    for (id, op) in [
        (1, RequestOp::Open(open)),
        (2, RequestOp::Compute(compute)),
        (3, RequestOp::Close {}),
    ] {
        input.extend(protocol::encode_frame(&protocol::Request { id, op }).unwrap());
    }
    let mut output = Vec::new();
    assert_eq!(
        crate::worker::Worker::default().serve_io(&mut input.as_slice(), &mut output),
        0
    );
    let mut reader = Cursor::new(output);
    let opened: Response<Opened> = protocol::read_frame(&mut reader).unwrap().unwrap();
    assert!(matches!(opened.result, Outcome::Ok { .. }));
    let computed: Response<Computed> = protocol::read_frame(&mut reader).unwrap().unwrap();
    assert_eq!(computed.id, 2);
    let Outcome::Ok { ok: computed } = computed.result else {
        panic!("compute failed");
    };
    assert_eq!(computed.energy.to_bits(), expected.energy.to_bits());
    assert_eq!(computed.arrays.len(), expected.arrays.len());
    for array in computed.arrays {
        let mut values = Vec::new();
        let count = *array.shape.first().unwrap() as usize * *array.shape.get(1).unwrap() as usize;
        for index in 0..array.chunks {
            let mut header = [0; protocol::HEADER_LEN];
            reader.read_exact(&mut header).unwrap();
            let (kind, size) = protocol::decode_header(&header).unwrap();
            assert_eq!(kind, FrameKind::Chunk);
            let mut body = vec![0; size];
            reader.read_exact(&mut body).unwrap();
            let remaining = count - values.len();
            protocol::decode_chunk(
                &body,
                index,
                remaining.min(protocol::CHUNK_VALUES),
                &mut values,
            )
            .unwrap();
        }
        assert_eq!(
            values,
            expected.array_values(array.name),
            "{:?}",
            array.name
        );
    }
    let closed: Response<Closed> = protocol::read_frame(&mut reader).unwrap().unwrap();
    assert_eq!(closed.id, 3);
    assert!(matches!(closed.result, Outcome::Ok { .. }));
    assert_eq!(reader.position() as usize, reader.get_ref().len());
}
