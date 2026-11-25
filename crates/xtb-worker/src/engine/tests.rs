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
                            let order = matrix.data[a * 3 + b];
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
        ..Request::default()
    })
    .unwrap();
    assert_eq!(actual.energy.to_bits(), expected.energy.unwrap().to_bits());
    assert_eq!(
        actual.hessian,
        expected.hessian.as_ref().unwrap().hessian.data
    );
    assert_eq!(actual.hessian_method.as_deref(), Some("analytic"));
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
    request.hessian.as_mut().unwrap().masses_amu = Some(masses.clone());
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
    frame.masses_amu[1] = 2.0;
    assert!(matches!(
        heavy.to_calc_result(&frame, reshiki_qcdata::Task::Hessian, &engine, None),
        Err(reshiki_qcdata::Error::Invalid(message))
            if message == "frame masses differ from the vibrational analysis masses"
    ));
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
    for (actual_parts, expected_parts) in [
        (&actual.parts, &expected.parts),
        (
            actual.hessian_parts.as_ref().unwrap(),
            expected.hessian_parts.as_ref().unwrap(),
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
            assert_eq!(part.iterations, expected.iterations);
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
    replacement[1][0] += 0.7;
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
        let count = array.shape[0] as usize * array.shape[1] as usize;
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
