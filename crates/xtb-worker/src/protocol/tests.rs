use super::*;
use crate::worker::Worker;

fn raw_frame(body: &str) -> Vec<u8> {
    let mut frame = Vec::from(*b"RSHXTBWK\x02\x00\x00\x00");
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(body.as_bytes());
    frame
}

#[test]
fn header_round_trip() {
    let request = Request {
        id: 42,
        op: RequestOp::Hello {},
    };
    let frame = encode_frame(&request).unwrap();
    assert_eq!(frame, raw_frame(r#"{"id":42,"op":"hello"}"#));
    let mut input = frame.as_slice();
    let request: Request = read_frame(&mut input).unwrap().unwrap();
    assert_eq!(request.id, 42);
    assert!(matches!(request.op, RequestOp::Hello {}));
    assert!(read_frame::<Request>(&mut input).unwrap().is_none());
}

#[test]
fn invalid_headers() {
    let frame = raw_frame(r#"{"id":1,"op":"hello"}"#);
    let header: [u8; HEADER_LEN] = frame[..HEADER_LEN].try_into().unwrap();
    let mut invalid = header;
    invalid[0] = b'X';
    assert!(matches!(decode_header(&invalid), Err(FrameError::Magic)));
    let mut invalid = header;
    invalid[8] = 1;
    assert!(matches!(decode_header(&invalid), Err(FrameError::Version)));
    let mut invalid = header;
    invalid[10] = 2;
    assert!(matches!(decode_header(&invalid), Err(FrameError::Kind)));
    let mut invalid = header;
    invalid[12..].copy_from_slice(&(MAX_FRAME_BYTES as u32 + 1).to_le_bytes());
    assert!(matches!(decode_header(&invalid), Err(FrameError::Length)));

    assert!(matches!(
        encode_frame(&"x".repeat(MAX_FRAME_BYTES)),
        Err(FrameError::Length)
    ));
}

#[test]
fn truncation_is_not_clean_eof() {
    let frame = raw_frame(r#"{"id":1,"op":"hello"}"#);
    for length in [1, HEADER_LEN - 1, HEADER_LEN, frame.len() - 1] {
        let err = read_frame::<Request>(&mut &frame[..length]).unwrap_err();
        assert!(matches!(err, FrameError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof));
    }
}

#[test]
fn strict_json_messages() {
    let open = r#"{"id":1,"op":"open","numbers":[1],"positions":[[0.1,0,0]],"units":"angstrom","method":"gfn2","heap_bytes":1048576}"#;
    let request: Request = serde_json::from_str(open).unwrap();
    let RequestOp::Open(opened) = request.op else {
        panic!("expected open");
    };
    assert_eq!(opened.charge, 0);
    assert_eq!(opened.uhf, None);
    assert_eq!(opened.conventions, Conventions::Axtb);
    assert_eq!(opened.threads, 1);
    assert_eq!(opened.scf, ScfSettings::default());
    assert!(opened.gfnff.is_none());
    assert!(opened.solvent.is_none());
    assert_eq!(opened.positions[0][0].to_bits(), 0.1_f64.to_bits());
    let compatible = open.replace("\"heap_bytes\"", r#""conventions":"xtb-6.8","heap_bytes""#);
    let request: Request = serde_json::from_str(&compatible).unwrap();
    assert!(matches!(
        request.op,
        RequestOp::Open(Open {
            conventions: Conventions::Xtb68,
            ..
        })
    ));

    for malformed in [
        r#"{"id":1,"op":"hello","extra":1}"#,
        r#"{"id":2,"op":"close","extra":1}"#,
        r#"{"id":3,"op":"compute","units":"bohr","gradient":false,"extra":1}"#,
        r#"{"id":4,"op":"compute","gradient":false}"#,
        r#"{"id":5,"op":"compute","units":"nm","gradient":false}"#,
        r#"{"id":6,"op":"compute","units":"bohr","gradient":false,"positions":[[1e400,0,0]]}"#,
        &open.replace("\"gfn2\"", "\"gfn1\""),
        &compatible.replace("\"xtb-6.8\"", "\"xtb\""),
        &compatible.replace("\"xtb-6.8\"", "null"),
        r#"{"id":7,"op":"compute","units":"bohr","gradient":false,"hessian":{"thermo":{"other":1}}}"#,
        &open.replace("\"heap_bytes\"", "\"other\":0,\"heap_bytes\""),
        &open.replace(
            "\"heap_bytes\"",
            r#""solvent":{"model":"alpb","name":"water","other":0},"heap_bytes""#,
        ),
        &open.replace(
            "\"heap_bytes\"",
            r#""scf":{"retries":[{"max_iterations":20,"other":0}]},"heap_bytes""#,
        ),
        &open.replace("\"heap_bytes\"", r#""gfnff":{"other":0},"heap_bytes""#),
    ] {
        assert!(
            serde_json::from_str::<Request>(malformed).is_err(),
            "{malformed}"
        );
    }
    assert!(serde_json::from_str::<Response<Closed>>(r#"{"id":1,"ok":{}}"#).is_ok());
    for malformed in [
        r#"{"id":1,"ok":{"extra":1}}"#,
        r#"{"id":1,"ok":{},"extra":1}"#,
        r#"{"id":1,"ok":{},"error":{"kind":"state","message":"bad"}}"#,
        r#"{"id":1,"error":{"kind":"state","message":"bad","extra":1}}"#,
    ] {
        assert!(
            serde_json::from_str::<Response<Closed>>(malformed).is_err(),
            "{malformed}"
        );
    }
}

fn computed() -> Computed {
    Computed {
        method: Method::Gfn2,
        conventions: Conventions::Xtb68,
        engine_conventions: "xtb".into(),
        numerics: "gfn2:1".into(),
        settings: SessionSettings {
            charge: 0,
            uhf: 0,
            solvent: None,
            scf: ScfSettings::default(),
            gfnff: None,
            threads: 1,
        },
        energy: -1.0,
        energy_terms: BTreeMap::from([("repulsion".into(), 0.1)]),
        gradient: Some(vec![[0.25; 3]; 2]),
        charges: Some(AtomicCharges {
            scheme: reshiki_qcdata::ChargeScheme::Mulliken,
            values: vec![0.0; 2],
        }),
        dipole_e_bohr: Some([0.0; 3]),
        quadrupole_e_bohr2: None,
        bond_orders: Some(vec![BondOrder {
            a: 0,
            b: 1,
            order: 1.0,
        }]),
        orbitals: None,
        homo_lumo_gap_ev: None,
        near_degenerate: false,
        scf: Some(ScfReport {
            iterations: 1,
            start: "eeq".into(),
            electronic_temperature_k: 300.0,
            retry: None,
        }),
        composite: None,
        gfnff: None,
        hessian_method: None,
        vibrations: None,
        thermo: None,
        arrays: vec![],
        seconds: 0.0,
        hessian: vec![],
        dipole_derivatives: vec![],
        normal_modes: vec![],
    }
}

#[test]
fn computed_wire_requires_a_gap_field_even_without_a_lumo() {
    let mut value = serde_json::to_value(computed()).unwrap();
    assert_eq!(value["conventions"], "xtb-6.8");
    assert_eq!(
        value.get("homo_lumo_gap_ev"),
        Some(&serde_json::Value::Null)
    );
    assert!(serde_json::from_value::<Computed>(value.clone()).is_ok());
    value.as_object_mut().unwrap().remove("homo_lumo_gap_ev");
    assert!(serde_json::from_value::<Computed>(value).is_err());
}

#[test]
fn computed_wire_omits_binary_coefficients() {
    use reshiki_qcdata::OrbitalSet;

    let mut computed = computed();
    let set = OrbitalSet {
        energies_hartree: vec![-1.0],
        occupations: vec![1.0],
        symmetries: vec![],
        rows: vec![0],
        coefficients: vec![0.25],
    };
    computed.orbitals = Some(Orbitals {
        basis: None,
        core_electrons: vec![],
        alpha: set.clone(),
        beta: Beta::Unrestricted(set),
    });
    let mut json = serde_json::to_value(&computed).unwrap();
    assert_eq!(json["orbitals"]["alpha"]["coefficients"], "");
    assert_eq!(json["orbitals"]["beta"]["unrestricted"]["coefficients"], "");
    let got: Computed = serde_json::from_value(json.clone()).unwrap();
    assert!(got.array_values(ArrayName::CoefficientsAlpha).is_empty());
    assert!(got.array_values(ArrayName::CoefficientsBeta).is_empty());
    json["orbitals"]["alpha"]["coefficients"] = "f32:anything".into();
    assert!(serde_json::from_value::<Computed>(json).is_err());
}

#[test]
fn scientific_result_has_session_settings_and_provenance() {
    let open = open(Method::Gfn2, 2);
    let frame = open.admit(None, None, MAX_ATOMS).unwrap();
    let mut computed = computed();
    computed.settings.scf.accuracy = Some(0.5);
    computed.settings.scf.max_iterations = Some(500);
    computed.settings.scf.retries = vec![ScfRetry {
        electronic_temperature_k: Some(1000.0),
        max_iterations: None,
    }];
    let engine = Engine {
        name: "axtb".into(),
        version: "0.2.0".into(),
        license: crate::AXTB_LICENSE.into(),
        source: crate::AXTB_SOURCE.into(),
        rev: crate::AXTB_REV.into(),
    };
    let result = computed
        .to_calc_result(&frame, Task::Gradient, &engine, None)
        .unwrap();
    result.validate(&frame).unwrap();
    assert_eq!(result.frame, frame.hash());
    assert_eq!(result.gradient_hartree_per_bohr, vec![0.25; 6]);
    assert_eq!(result.provenance.program, "axtb");
    assert_eq!(result.provenance.program_version, Some(engine.version));
    assert_eq!(result.provenance.method.as_deref(), Some("GFN2-xTB"));
    assert_eq!(
        result.provenance.source,
        Source::Calculated {
            engine_revision: engine.rev
        }
    );
    let Settings::Xtb(settings) = result.settings else {
        panic!("expected xTB settings");
    };
    assert_eq!(settings.conventions, Conventions::Xtb68);
    assert_eq!(settings.accuracy, Some(0.5));
    assert_eq!(settings.max_iterations, Some(500));
    assert_eq!(settings.scf_retries, computed.settings.scf.retries);
}

#[test]
fn near_degenerate_requires_a_gap_strictly_below_the_threshold() {
    assert!(is_near_degenerate(Some(NEAR_DEGENERATE_GAP_EV.next_down())));
    assert!(!is_near_degenerate(Some(NEAR_DEGENERATE_GAP_EV)));
    assert!(!is_near_degenerate(Some(NEAR_DEGENERATE_GAP_EV.next_up())));
    assert!(!is_near_degenerate(None));
}

fn open(method: Method, atoms: usize) -> Open {
    Open {
        numbers: vec![2; atoms],
        positions: vec![[0.0; 3]; atoms],
        units: Units::Bohr,
        charge: 0,
        uhf: None,
        method,
        conventions: Conventions::Axtb,
        solvent: None,
        scf: ScfSettings::default(),
        gfnff: None,
        threads: 1,
        heap_bytes: 1 << 20,
    }
}

#[test]
fn invalid_request_values() {
    let mut open = open(Method::Gfn2, MAX_ATOMS + 1);
    let kind = |open: &Open| open.admit(None, None, MAX_ATOMS).unwrap_err().kind;
    assert_eq!(kind(&open), ErrorKind::Limit);
    open.numbers.truncate(1);
    assert_eq!(kind(&open), ErrorKind::Input);
    open.positions = vec![[f64::NAN, 0.0, 0.0]];
    assert_eq!(kind(&open), ErrorKind::Input);
    open.positions[0][0] = 0.0;
    open.threads = 65;
    assert_eq!(kind(&open), ErrorKind::Input);
    open.heap_bytes = 0;
    assert_eq!(open.heap_budget().unwrap_err().kind, ErrorKind::Limit);
}

#[test]
fn validates_method_and_task_combinations() {
    let mut ff = open(Method::Gfnff, 2);
    for (compute, message) in [
        (
            Compute {
                orbitals: OrbitalOutput::All,
                ..Compute::default()
            },
            "GFN-FF does not provide orbitals",
        ),
        (
            Compute {
                bond_orders: true,
                ..Compute::default()
            },
            "GFN-FF does not provide bond orders",
        ),
    ] {
        let error = ff.admit(Some(&compute), None, MAX_ATOMS).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unsupported);
        assert_eq!(error.message, message);
    }
    let mut solvent = Solvent {
        model: SolventModel::Alpb,
        name: "water".into(),
        state: SolvationState::Gsolv,
        correction: None,
    };
    ff.solvent = Some(solvent.clone());
    assert_eq!(
        ff.admit(None, None, MAX_ATOMS).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let mut gxtb = open(Method::Gxtb, 2);
    gxtb.solvent = Some(solvent.clone());
    assert_eq!(
        gxtb.admit(None, None, MAX_ATOMS).unwrap_err().kind,
        ErrorKind::Input
    );
    let mut gfn2 = open(Method::Gfn2, 2);
    solvent.correction = Some(SolventCorrection::Gfn2);
    gfn2.solvent = Some(solvent);
    assert_eq!(
        gfn2.admit(None, None, MAX_ATOMS).unwrap_err().kind,
        ErrorKind::Input
    );
}

#[test]
fn validates_hessian_limits_and_orbital_rows() {
    let large = open(Method::Gfn2, MAX_HESSIAN_ATOMS + 1);
    let hessian = Compute {
        hessian: Some(HessianRequest::default()),
        ..Compute::default()
    };
    assert_eq!(
        large
            .admit(Some(&hessian), Some(500), MAX_ATOMS)
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    let small = open(Method::Gfn2, 2);
    for rows in [vec![0, 2], vec![1, 0], vec![1, 1]] {
        let compute = Compute {
            orbitals: OrbitalOutput::Rows(rows),
            ..Compute::default()
        };
        assert_eq!(
            small
                .admit(Some(&compute), Some(2), MAX_ATOMS)
                .unwrap_err()
                .kind,
            ErrorKind::Input
        );
    }
    let compute = Compute {
        orbitals: OrbitalOutput::Rows(vec![0, 1]),
        ..Compute::default()
    };
    small.admit(Some(&compute), Some(2), MAX_ATOMS).unwrap();
}

#[test]
fn validates_thermochemistry_ranges_in_admission() {
    let open = open(Method::Gfn2, 2);
    let mut compute = Compute {
        hessian: Some(HessianRequest::default()),
        ..Compute::default()
    };
    for thermo in [
        ThermoRequest {
            temperature_k: Some(0.0),
            ..ThermoRequest::default()
        },
        ThermoRequest {
            pressure_pa: Some(-1.0),
            ..ThermoRequest::default()
        },
        ThermoRequest {
            frequency_scale: Some(0.0),
            ..ThermoRequest::default()
        },
        ThermoRequest {
            symmetry_number: Some(0),
            ..ThermoRequest::default()
        },
        ThermoRequest {
            rotor_cutoff_cm1: Some(f64::INFINITY),
            ..ThermoRequest::default()
        },
        ThermoRequest {
            imaginary_cutoff_cm1: Some(f64::NAN),
            ..ThermoRequest::default()
        },
    ] {
        compute.hessian.as_mut().unwrap().thermo = thermo;
        assert_eq!(
            open.admit(Some(&compute), None, MAX_ATOMS)
                .unwrap_err()
                .kind,
            ErrorKind::Input
        );
    }
    compute.hessian.as_mut().unwrap().thermo = ThermoRequest {
        rotor_cutoff_cm1: Some(-50.0),
        imaginary_cutoff_cm1: Some(-20.0),
        ..ThermoRequest::default()
    };
    open.admit(Some(&compute), None, MAX_ATOMS).unwrap();
}

#[test]
fn ids_must_be_positive_and_increasing() {
    for ids in [vec![0], vec![1, 1], vec![2, 1]] {
        let input: Vec<u8> = ids
            .iter()
            .flat_map(|id| raw_frame(&format!(r#"{{"id":{id},"op":"hello"}}"#)))
            .collect();
        let mut output = Vec::new();
        assert_eq!(
            Worker::default().serve_io(&mut input.as_slice(), &mut output),
            2
        );
        let mut output = output.as_slice();
        for _ in 1..ids.len() {
            let hello: Response<Hello> = read_frame(&mut output).unwrap().unwrap();
            assert!(matches!(hello.result, Outcome::Ok { .. }));
        }
        let response: Response<Closed> = read_frame(&mut output).unwrap().unwrap();
        assert_eq!(response.id, 0);
        let Outcome::Error { error } = response.result else {
            panic!("expected protocol error");
        };
        assert_eq!(error.kind, ErrorKind::Protocol);
        assert!(output.is_empty());
    }
}

#[test]
fn malformed_request_exits_with_protocol_error() {
    let mut input = raw_frame(r#"{"id":1,"op":"hello","extra":1}"#);
    input.extend(raw_frame(r#"{"id":2,"op":"hello"}"#));
    let mut output = Vec::new();
    assert_eq!(
        Worker::default().serve_io(&mut input.as_slice(), &mut output),
        2
    );
    let mut output = output.as_slice();
    let response: Response<Closed> = read_frame(&mut output).unwrap().unwrap();
    assert_eq!(response.id, 0);
    let Outcome::Error { error } = response.result else {
        panic!("expected protocol error");
    };
    assert_eq!(error.kind, ErrorKind::Protocol);
    assert!(output.is_empty());
}

#[test]
fn oversized_unknown_operation_still_returns_a_protocol_error_frame() {
    let prefix = r#"{"id":1,"op":""#;
    let suffix = r#""}"#;
    let name = "水".repeat((MAX_FRAME_BYTES - prefix.len() - suffix.len()) / "水".len());
    let input = raw_frame(&format!("{prefix}{name}{suffix}"));
    let mut output = Vec::new();
    assert_eq!(
        Worker::default().serve_io(&mut input.as_slice(), &mut output),
        2
    );
    let mut output = output.as_slice();
    let response: Response<Closed> = read_frame(&mut output).unwrap().unwrap();
    assert_eq!(response.id, 0);
    let Outcome::Error { error } = response.result else {
        panic!("expected protocol error");
    };
    assert_eq!(error.kind, ErrorKind::Protocol);
    assert!(error.message.contains("unknown variant"));
    assert!(error.message.len() <= 1024);
    assert!(output.is_empty());
}

#[cfg(not(feature = "engine"))]
#[test]
fn disabled_worker_answers_hello_and_refuses_open() {
    let mut input = raw_frame(r#"{"id":1,"op":"hello"}"#);
    input.extend(raw_frame(
        r#"{"id":2,"op":"open","numbers":[1],"positions":[[0,0,0]],"units":"bohr","method":"gfn2","heap_bytes":1048576}"#,
    ));
    let mut output = Vec::new();
    assert_eq!(
        Worker::default().serve_io(&mut input.as_slice(), &mut output),
        0
    );
    let mut output = output.as_slice();
    let hello: Response<Hello> = read_frame(&mut output).unwrap().unwrap();
    assert_eq!(hello.id, 1);
    let Outcome::Ok { ok } = hello.result else {
        panic!("expected hello");
    };
    assert_eq!(ok.protocol, VERSION);
    assert_eq!(ok.max_atoms, MAX_ATOMS);
    assert!(ok.engine.is_none());
    assert!(ok.methods.is_empty());
    assert_eq!(ok.max_hessian_atoms, MAX_HESSIAN_ATOMS);

    let response: Response<Opened> = read_frame(&mut output).unwrap().unwrap();
    assert_eq!(response.id, 2);
    let Outcome::Error { error } = response.result else {
        panic!("expected unavailable");
    };
    assert_eq!(error.kind, ErrorKind::Unavailable);
    assert_eq!(error.message, "xTB is not included in this build");
    assert!(output.is_empty());
}

#[test]
fn chunks_round_trip_at_frame_boundaries() {
    for (len, count) in [
        (0, 0),
        (1, 1),
        (CHUNK_VALUES - 1, 1),
        (CHUNK_VALUES, 1),
        (CHUNK_VALUES + 1, 2),
        (2 * CHUNK_VALUES, 2),
    ] {
        let values: Vec<_> = (0..len)
            .map(|i| if i == 0 { -0.0 } else { i as f64 / 17.0 })
            .collect();
        let mut encoded = Vec::new();
        write_array(&mut encoded, &values).unwrap();
        let mut frames = encoded.as_slice();
        assert_eq!(chunk_count(len), count);
        let mut actual = Vec::new();
        for (index, chunk) in values.chunks(CHUNK_VALUES).enumerate() {
            let header = frames.split_off(..HEADER_LEN).unwrap().try_into().unwrap();
            let (kind, bytes) = decode_header(&header).unwrap();
            assert_eq!(kind, FrameKind::Chunk);
            assert_eq!(bytes, 4 + size_of_val(chunk));
            let body = frames.split_off(..bytes).unwrap();
            decode_chunk(body, index as u32, chunk.len(), &mut actual).unwrap();
        }
        assert!(frames.is_empty());
        assert_eq!(actual.len(), values.len());
        assert!(
            actual
                .iter()
                .zip(values)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
}

#[test]
fn invalid_chunks_and_chunk_requests_are_rejected() {
    let frame = encode_chunk(2, &[1.0]);
    assert!(matches!(
        read_frame::<Request>(&mut frame.as_slice()),
        Err(FrameError::Kind)
    ));
    let body = &frame[HEADER_LEN..];
    let mut output = Vec::new();
    assert!(decode_chunk(body, 3, 1, &mut output).is_err());
    assert!(decode_chunk(body, 2, 2, &mut output).is_err());
    let nonfinite = encode_chunk(2, &[f64::INFINITY]);
    assert!(decode_chunk(&nonfinite[HEADER_LEN..], 2, 1, &mut output).is_err());
}

#[test]
fn orbital_output_wire_forms_and_compute_defaults() {
    for (wire, expected) in [
        (r#""none""#, OrbitalOutput::None),
        (r#""levels""#, OrbitalOutput::Levels),
        (r#""all""#, OrbitalOutput::All),
        (r#"{"rows":[0,2]}"#, OrbitalOutput::Rows(vec![0, 2])),
    ] {
        assert_eq!(
            serde_json::from_str::<OrbitalOutput>(wire).unwrap(),
            expected
        );
        assert_eq!(serde_json::to_string(&expected).unwrap(), wire);
    }
    let compute: Compute = serde_json::from_str(r#"{"units":"bohr","gradient":false}"#).unwrap();
    assert!(compute.positions.is_none());
    assert!(compute.hessian.is_none());
    assert!(!compute.bond_orders);
    assert_eq!(compute.orbitals, OrbitalOutput::None);
    assert_eq!(
        serde_json::to_value(compute).unwrap(),
        serde_json::to_value(Compute::default()).unwrap()
    );
    let compute: Compute =
        serde_json::from_str(r#"{"units":"bohr","gradient":false,"hessian":{}}"#).unwrap();
    assert_eq!(
        serde_json::to_value(compute.hessian.unwrap().thermo).unwrap(),
        serde_json::json!({})
    );
}
