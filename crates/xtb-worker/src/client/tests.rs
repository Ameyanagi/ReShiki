use super::*;
use crate::protocol::{ArrayHeader, ArrayName, Conventions, FrameKind, Method, OrbitalOutput};
use serde_json::json;

fn open() -> Open {
    serde_json::from_value(json!({
        "numbers": [2,2], "positions": [[0,0,0],[0,0,2]], "units": "bohr",
        "charge": 0, "uhf": 0, "method": "gfn2", "conventions": "xtb-6.8",
        "threads": 1, "heap_bytes": 1024
    }))
    .unwrap()
}

fn request() -> Compute {
    serde_json::from_value(json!({"units":"bohr", "gradient": false})).unwrap()
}

fn computed(gradient: Option<Vec<[f64; 3]>>) -> Computed {
    let mut computed: Computed = serde_json::from_value(json!({
        "method": "gfn2", "conventions": "xtb-6.8", "engine_conventions": "xtb", "numerics": "gfn2:1",
        "settings": {"charge": 0, "uhf": 0, "scf": {}, "threads": 1},
        "energy": -1, "energy_terms": {"electronic": -1},
        "charges": {"scheme": "mulliken", "values": [0,0]},
        "dipole_e_bohr": [0,0,0], "quadrupole_e_bohr2": [0,0,0,0,0,0],
        "homo_lumo_gap_ev": null, "near_degenerate": false,
        "scf": {"iterations": 3, "start": "previous", "electronic_temperature_k": 300},
        "arrays": [], "seconds": 0.1
    })).unwrap();
    computed.gradient = gradient;
    computed
}

fn opened() -> Opened {
    Opened {
        atoms: 2,
        method: Method::Gfn2,
        conventions: Conventions::Xtb68,
        engine_conventions: "xtb".into(),
        numerics: "gfn2:1".into(),
        basis_functions: Some(2),
        uhf: 0,
    }
}

fn validate(computed: &Computed, request: &Compute) -> std::result::Result<Vec<ArrayPlan>, String> {
    validate_computed_header(computed, Some(&opened()), Some(&open()), request)
}

fn header(name: ArrayName, shape: [u32; 2]) -> ArrayHeader {
    ArrayHeader {
        name,
        shape,
        chunks: protocol::chunk_count(shape[0] as usize * shape[1] as usize),
    }
}

fn orbital_rows(rows: &[u32]) -> reshiki_qcdata::Orbitals {
    serde_json::from_value(json!({
        "basis": {"shells": [
            {"atom":0,"l":0,"pure":false,"exponents_per_bohr2":[1],"coefficients":[1]},
            {"atom":1,"l":0,"pure":false,"exponents_per_bohr2":[1],"coefficients":[1]}
        ]},
        "core_electrons": [0,0], "alpha": {
            "energies_hartree": [-1,-0.5], "occupations": [2,2], "rows": rows
        }, "beta": "restricted"
    }))
    .unwrap()
}

fn chunk_body(index: u32, values: &[f64]) -> Vec<u8> {
    let mut body = index.to_le_bytes().to_vec();
    for value in values {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body
}

fn raw_frame(kind: FrameKind, body: &[u8]) -> Vec<u8> {
    let mut bytes = b"RSHXTBWK".to_vec();
    bytes.extend_from_slice(&protocol::VERSION.to_le_bytes());
    bytes.extend_from_slice(&(kind as u16).to_le_bytes());
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}

#[test]
fn cancel_is_shared_and_sticky() {
    let cancel = Cancel::default();
    let copy = cancel.clone();
    assert!(!cancel.cancelled());
    copy.cancel();
    assert!(cancel.cancelled());
    assert!(copy.cancelled());
}

/// Isolate warm gradient round trips from startup and optimizer work. Run the
/// same probe against the packaged engine used by the before/after CLI runs:
/// RESHIKI_XTB_BENCH_APP=/absolute/path/to/reshiki cargo test -p
/// reshiki-xtb-worker manual_profile_warm_gradient_roundtrips -- --ignored --nocapture
#[test]
#[ignore = "manual packaged-worker latency measurement; use --nocapture"]
fn manual_profile_warm_gradient_roundtrips() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let executable = std::env::var_os("RESHIKI_XTB_BENCH_APP")
        .ok_or("Set RESHIKI_XTB_BENCH_APP to the packaged executable")?;
    let limits = Limits::default();
    let open = Open {
        numbers: vec![8, 1, 1],
        positions: vec![[0., 0., 0.], [0.7586, 0., 0.5043], [-0.7586, 0., 0.5043]],
        units: protocol::Units::Angstrom,
        charge: 0,
        uhf: Some(0),
        method: Method::Gfn2,
        conventions: Conventions::Axtb,
        solvent: None,
        scf: Default::default(),
        gfnff: None,
        threads: 1,
        heap_bytes: limits.heap_bytes,
    };
    let request = Compute {
        gradient: true,
        ..Compute::default()
    };
    for repeat in 0..3 {
        let cancel = Cancel::default();
        let (mut session, _) = Session::start(Path::new(&executable), limits, &cancel)?;
        session.open(&open, &cancel)?;
        let mut computed = session.compute(&request, &cancel)?;
        let mut wall = Duration::ZERO;
        let mut engine = 0.;
        let mut scf_iterations = 0;
        for _ in 0..100 {
            let started = Instant::now();
            computed = session.compute(&request, &cancel)?;
            wall += started.elapsed();
            engine += computed.seconds;
            scf_iterations += computed.scf.as_ref().map_or(0, |scf| scf.iterations);
        }
        session.close(&cancel)?;
        println!(
            "{}",
            json!({
                "repeat": repeat,
                "requests": 100,
                "request_wall_ms": wall.as_secs_f64() * 1000.,
                "engine_ms": engine * 1000.,
                "non_engine_ms": (wall.as_secs_f64() - engine) * 1000.,
                "scf_iterations": scf_iterations,
                "energy_hartree": computed.energy,
                "gradient_hartree_per_bohr": computed.gradient,
            })
        );
    }
    Ok(())
}

#[test]
fn worker_command_removes_lapack_overrides() {
    let command = worker_command(Path::new("reshiki"));
    for name in ["AXTB_LAPACK", "AXTB_LAPACK_BLAS"] {
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == name && value.is_none())
        );
    }
}

#[test]
fn heap_exit_classification_accounts_for_supervisor_cleanup() {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    #[cfg(unix)]
    let status = ExitStatus::from_raw(75 << 8);
    #[cfg(windows)]
    let status = ExitStatus::from_raw(75);

    assert!(
        matches!(classify_exit(status, Some(status), "heap diagnostic".into()),
        Error::Limit(message) if message.contains("heap diagnostic"))
    );
    let error = classify_exit(status, None, String::new());
    #[cfg(unix)]
    assert!(matches!(error, Error::Limit(_)));
    #[cfg(windows)]
    assert!(matches!(error, Error::Exited { status: reaped, .. } if reaped == status));
}

#[test]
fn diagnostics_keep_the_last_four_kibibytes() {
    let mut tail = VecDeque::new();
    remember_stderr(&mut tail, &vec![b'a'; STDERR_BYTES + 10]);
    remember_stderr(&mut tail, b"last");
    assert_eq!(tail.len(), STDERR_BYTES);
    let text = String::from_utf8(tail.into()).unwrap();
    assert!(text.ends_with("last"));
    assert_eq!(
        text[..STDERR_BYTES - 4]
            .bytes()
            .filter(|byte| *byte == b'a')
            .count(),
        STDERR_BYTES - 4
    );
}

#[test]
fn hello_metadata_requires_exact_v2_capabilities_and_limits() {
    let mut hello: Hello = serde_json::from_value(json!({
        "protocol": 2, "engine": null, "methods": [], "max_atoms": 500, "max_hessian_atoms": 200
    }))
    .unwrap();
    assert!(validate_hello(&hello).is_ok());
    hello.methods.push(Method::Gfn2);
    assert!(validate_hello(&hello).is_err());
    hello.engine = Some(protocol::Engine {
        name: "axtb".into(),
        version: "0.2.0".into(),
        license: "LGPL-3.0-or-later".into(),
        source: "https://github.com/Ameyanagi/axtb".into(),
        rev: "revision".into(),
    });
    hello.methods = Method::ALL.to_vec();
    assert!(validate_hello(&hello).is_ok());
    hello.methods.swap(0, 1);
    assert!(validate_hello(&hello).is_err());
    hello.methods = Method::ALL.to_vec();
    hello.max_hessian_atoms = 201;
    assert!(validate_hello(&hello).is_err());
    hello.max_hessian_atoms = 199;
    assert!(validate_hello(&hello).is_err());
    hello.max_hessian_atoms = 0;
    assert!(validate_hello(&hello).is_err());
    hello.max_hessian_atoms = 200;
    hello.max_atoms = 0;
    assert!(validate_hello(&hello).is_err());
}

#[test]
fn opened_requires_exact_identity_and_basis_only_for_scf() {
    let mut result = opened();
    assert!(validate_opened(&result, &open(), 500).is_ok());
    result.basis_functions = None;
    assert!(validate_opened(&result, &open(), 500).is_err());
    result.basis_functions = Some(0);
    assert!(validate_opened(&result, &open(), 500).is_err());
    result.basis_functions = Some(reshiki_qcdata::MAX_BASIS_FUNCTIONS as u32 + 1);
    assert!(validate_opened(&result, &open(), 500).is_ok());
    result.basis_functions = Some(2);
    result.engine_conventions = "axtb".into();
    assert!(validate_opened(&result, &open(), 500).is_err());
    let mut input = open();
    input.method = Method::Gfnff;
    result.method = Method::Gfnff;
    result.engine_conventions = "xtb".into();
    assert!(validate_opened(&result, &input, 500).is_err());
    result.basis_functions = None;
    assert!(validate_opened(&result, &input, 500).is_ok());
    input.method = Method::Gxtb;
    result.method = Method::Gxtb;
    result.engine_conventions = "gxtb".into();
    result.basis_functions = Some(2);
    result.numerics = "gxtb-v1.1:1".into();
    assert!(validate_opened(&result, &input, 500).is_err());
    result.numerics = "gxtb-v2.0.1:1".into();
    assert!(validate_opened(&result, &input, 500).is_ok());
}

#[test]
fn computed_requires_exact_requested_finite_gradient() {
    let mut request = request();
    assert!(validate(&computed(None), &request).is_ok());
    request.gradient = true;
    assert!(validate(&computed(None), &request).is_err());
    assert!(validate(&computed(Some(vec![[0.0; 3]; 2])), &request).is_ok());
    assert!(validate(&computed(Some(vec![[0.0; 3]; 1])), &request).is_err());
    assert!(validate(&computed(Some(vec![[f64::NAN; 3]; 2])), &request).is_err());
    request.gradient = false;
    assert!(validate(&computed(Some(vec![[0.0; 3]; 2])), &request).is_err());
    assert!(validate_computed_header(&computed(None), None, None, &request).is_err());
}

#[test]
fn computed_checks_gap_flag_energy_and_time() {
    let request = request();
    let mut result = computed(None);
    for gap in [-0.1, f64::INFINITY, f64::NAN] {
        result.homo_lumo_gap_ev = Some(gap);
        result.near_degenerate = protocol::is_near_degenerate(Some(gap));
        assert!(validate(&result, &request).is_err());
    }
    result.homo_lumo_gap_ev = Some(0.05);
    result.near_degenerate = false;
    assert!(validate(&result, &request).is_err());
    result.near_degenerate = true;
    assert!(validate(&result, &request).is_ok());
    result.homo_lumo_gap_ev = Some(0.1);
    assert!(validate(&result, &request).is_err());
    result.near_degenerate = false;
    assert!(validate(&result, &request).is_ok());
    result.energy = f64::INFINITY;
    assert!(validate(&result, &request).is_err());
    result.energy = -1.0;
    for seconds in [f64::NAN, -0.1] {
        result.seconds = seconds;
        assert!(validate(&result, &request).is_err());
    }
}

#[test]
fn computed_requires_method_settings_and_requested_parts() {
    let request = request();
    let mut result = computed(None);
    result.settings.threads = 2;
    assert!(validate(&result, &request).is_err());
    result.settings.threads = 1;
    result.settings.charge = 1;
    assert!(validate(&result, &request).is_err());
    result.settings.charge = 0;
    result.settings.uhf = 2;
    assert!(validate(&result, &request).is_err());
    result.settings.uhf = 0;
    result.charges = None;
    assert!(validate(&result, &request).is_err());
    result = computed(None);
    result.bond_orders = Some(vec![]);
    assert!(validate(&result, &request).is_err());
    result.bond_orders = None;
    result.hessian_method = Some("analytic".into());
    assert!(validate(&result, &request).is_err());
    result.hessian_method = None;
    result.numerics = "gfn2:2".into();
    assert!(validate(&result, &request).is_err());
}

#[test]
fn composite_is_present_exactly_for_solvated_gxtb_with_finite_components() {
    let request = request();
    let mut input = open();
    input.method = Method::Gxtb;
    input.solvent = Some(serde_json::from_value(json!({"model":"alpb", "name":"water"})).unwrap());
    let mut opened = opened();
    opened.method = Method::Gxtb;
    opened.engine_conventions = "gxtb".into();
    opened.numerics = "gxtb-v2.0.1:1".into();
    let mut result = computed(None);
    result.method = opened.method;
    result.engine_conventions = opened.engine_conventions.clone();
    result.numerics = opened.numerics.clone();
    result.settings.solvent = input.solvent.clone();
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    result.composite = Some(
        serde_json::from_value(json!({
            "components":{"total":-1}, "parts":{}
        }))
        .unwrap(),
    );
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    let parts = |iterations: Option<usize>| {
        let part = json!({
            "method":"gfn2", "conventions":"xtb", "electronic_temperature_k":300,
            "max_iterations":250, "iterations":iterations, "retry":null
        });
        serde_json::from_value(json!({"gfn2_gas":part, "gfn2_solvated":part, "gxtb":part})).unwrap()
    };
    result.composite.as_mut().unwrap().parts = parts(Some(3));
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_ok());
    for iterations in [None, Some(0)] {
        result.composite.as_mut().unwrap().parts = parts(iterations);
        assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    }
    let composite = result.composite.as_mut().unwrap();
    composite.parts = parts(Some(3));
    composite.hessian_parts = Some(parts(None));
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());

    let (hessian_request, mut hessian) = hessian_result();
    hessian.method = result.method;
    hessian
        .engine_conventions
        .clone_from(&result.engine_conventions);
    hessian.numerics.clone_from(&result.numerics);
    hessian
        .settings
        .solvent
        .clone_from(&result.settings.solvent);
    hessian.vibrations.as_mut().unwrap().conventions = "xtb-6.7.1".into();
    hessian.composite = result.composite.take();
    let validate_hessian = |result: &Computed| {
        validate_computed_header(result, Some(&opened), Some(&input), &hessian_request)
    };
    assert!(validate_hessian(&hessian).is_ok());
    hessian.composite.as_mut().unwrap().hessian_parts = Some(parts(Some(0)));
    assert!(validate_hessian(&hessian).is_err());
    hessian.composite.as_mut().unwrap().hessian_parts = Some(parts(Some(3)));
    assert!(validate_hessian(&hessian).is_ok());
    let mut hessian_parts = hessian
        .composite
        .as_mut()
        .unwrap()
        .hessian_parts
        .take()
        .unwrap();
    assert!(validate_hessian(&hessian).is_err());
    hessian_parts.remove("gfn2_gas");
    hessian.composite.as_mut().unwrap().hessian_parts = Some(hessian_parts);
    assert!(validate_hessian(&hessian).is_err());
    result.composite = hessian.composite.take();
    result.composite.as_mut().unwrap().hessian_parts = None;

    result
        .composite
        .as_mut()
        .unwrap()
        .components
        .insert("total".into(), f64::NAN);
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    result.composite.as_mut().unwrap().components.clear();
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    result
        .composite
        .as_mut()
        .unwrap()
        .components
        .insert("total".into(), -1.0);
    input.solvent = None;
    result.settings.solvent = None;
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    result.composite = None;
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_ok());
    input.method = Method::Gfn2;
    input.solvent = Some(serde_json::from_value(json!({"model":"alpb", "name":"water"})).unwrap());
    opened.method = input.method;
    opened.engine_conventions = "xtb".into();
    opened.numerics = "gfn2:1".into();
    result.method = opened.method;
    result.engine_conventions = opened.engine_conventions.clone();
    result.numerics = opened.numerics.clone();
    result.settings.solvent = input.solvent.clone();
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_ok());
    result.composite = Some(
        serde_json::from_value(json!({
            "components":{"total":-1}, "parts":{}
        }))
        .unwrap(),
    );
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
}

#[test]
fn array_headers_reject_missing_extra_order_shape_oversize_and_wrong_chunks() {
    let expected = [
        (ArrayName::Hessian, [6, 6]),
        (ArrayName::DipoleDerivatives, [6, 3]),
    ];
    let correct = vec![
        header(ArrayName::Hessian, [6, 6]),
        header(ArrayName::DipoleDerivatives, [6, 3]),
    ];
    assert!(validate_array_headers(&correct, &expected).is_ok());
    assert!(validate_array_headers(&correct[..1], &expected).is_err());
    let mut invalid = correct.clone();
    invalid.push(header(ArrayName::NormalModes, [1, 6]));
    assert!(validate_array_headers(&invalid, &expected).is_err());
    invalid = correct.clone();
    invalid.swap(0, 1);
    assert!(validate_array_headers(&invalid, &expected).is_err());
    invalid = correct.clone();
    invalid[0].shape = [6, 5];
    assert!(validate_array_headers(&invalid, &expected).is_err());
    invalid = correct;
    invalid[0].chunks = 2;
    assert!(validate_array_headers(&invalid, &expected).is_err());
    let huge = header(ArrayName::CoefficientsAlpha, [2049, 2048]);
    assert!(validate_array_headers(&[huge], &[(huge.name, huge.shape)]).is_err());
    let empty = header(ArrayName::NormalModes, [0, 6]);
    assert!(validate_array_headers(&[empty], &[(empty.name, empty.shape)]).is_err());
}

#[test]
fn orbital_headers_require_exact_rows_and_payload_validates_current_frame() {
    let mut request = request();
    request.orbitals = OrbitalOutput::Rows(vec![0]);
    let mut result = computed(None);
    result.orbitals = Some(orbital_rows(&[0]));
    result.arrays = vec![header(ArrayName::CoefficientsAlpha, [1, 2])];
    assert!(validate(&result, &request).is_ok());
    result.orbitals.as_mut().unwrap().alpha.rows = vec![1];
    assert!(validate(&result, &request).is_err());
    result.orbitals.as_mut().unwrap().alpha.rows = vec![0];
    let frame = open().admit(Some(&request), Some(2), 500).unwrap();
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
    install_array(
        &mut result,
        ArrayName::CoefficientsAlpha,
        vec![1, 0].into_iter().map(f64::from).collect(),
    );
    assert!(validate_computed_payload(&result, Some(&frame)).is_ok());
    result.settings.uhf = 2;
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
}

#[test]
fn chunk_validation_requires_exact_index_size_finite_values_and_last_remainder() {
    let mut out = vec![];
    append_chunk(&mut out, &chunk_body(0, &[1, 2].map(f64::from)), 0, 2).unwrap();
    assert_eq!(out, vec![1.0, 2.0]);
    for body in [
        chunk_body(1, &[1.0, 2.0]),
        chunk_body(0, &[1.0]),
        chunk_body(0, &[1.0, 2.0, 3.0]),
        chunk_body(0, &[1.0, f64::INFINITY]),
        vec![0; 5],
    ] {
        assert!(append_chunk(&mut vec![], &body, 0, 2).is_err());
    }
    assert!(
        append_chunk(
            &mut vec![],
            &chunk_body(0, &[1.0]),
            0,
            protocol::CHUNK_VALUES + 1
        )
        .is_err()
    );
    let mut partial = vec![0.0; protocol::CHUNK_VALUES];
    append_chunk(
        &mut partial,
        &chunk_body(1, &[3.0]),
        1,
        protocol::CHUNK_VALUES + 1,
    )
    .unwrap();
    assert_eq!(partial.last(), Some(&3.0));
}

#[test]
fn coefficient_reader_requires_a_full_first_chunk_and_ordered_short_final_chunk() {
    let width = 2048;
    let rows = protocol::CHUNK_VALUES / width + 1;
    let shape = [rows as u32, width as u32];
    let plans = validate_array_headers(
        &[header(ArrayName::CoefficientsAlpha, shape)],
        &[(ArrayName::CoefficientsAlpha, shape)],
    )
    .unwrap();
    let plan = &plans[0];
    assert_eq!(plan.chunks, 2);
    assert!(plan.values > protocol::CHUNK_VALUES);
    let full = vec![1.0; protocol::CHUNK_VALUES];
    let tail = vec![3.0; plan.values - protocol::CHUNK_VALUES];
    assert!(tail.len() < protocol::CHUNK_VALUES);
    let read = |first: &[f64], second_index| {
        let mut input = ResponseInput {
            bytes: [
                raw_frame(FrameKind::Chunk, &chunk_body(0, first)),
                raw_frame(FrameKind::Chunk, &chunk_body(second_index, &tail)),
            ]
            .concat(),
            closed: false,
        };
        read_array(plan, || {
            input
                .take_frame(FrameKind::Chunk)
                .map_err(Error::Protocol)
                .map(Option::unwrap)
        })
    };
    let values = read(&full, 1).unwrap();
    assert_eq!(values.len(), plan.values);
    assert_eq!(&values[..protocol::CHUNK_VALUES], full);
    assert_eq!(&values[protocol::CHUNK_VALUES..], tail);
    assert!(matches!(read(&full, 0), Err(Error::Protocol(_))));
    assert!(matches!(
        read(&full[..full.len() - 1], 1),
        Err(Error::Protocol(_))
    ));
}

#[test]
fn framed_input_preserves_next_frame_and_rejects_bad_kind_version_length_and_magic() {
    let json = raw_frame(FrameKind::Json, b"{}");
    let chunk = raw_frame(FrameKind::Chunk, &chunk_body(0, &[1.0]));
    let mut input = ResponseInput::default();
    for byte in &json[..json.len() - 1] {
        input.bytes.push(*byte);
        assert!(input.take_frame(FrameKind::Json).unwrap().is_none());
    }
    input.bytes.extend_from_slice(&json[json.len() - 1..]);
    input.bytes.extend_from_slice(&chunk);
    assert_eq!(
        input.take_frame(FrameKind::Json).unwrap(),
        Some(b"{}".to_vec())
    );
    assert_eq!(input.bytes, chunk);
    assert!(input.take_frame(FrameKind::Json).is_err());
    assert_eq!(
        input.take_frame(FrameKind::Chunk).unwrap(),
        Some(chunk_body(0, &[1.0]))
    );
    assert!(input.bytes.is_empty());
    for offset in [0, 8, 10, 15] {
        let mut invalid = json.clone();
        invalid[offset] = 255;
        input.bytes = invalid;
        assert!(input.take_frame(FrameKind::Json).is_err());
    }
    input.bytes = json;
    input.bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
    assert!(input.take_frame(FrameKind::Json).is_err());
}

#[test]
fn local_admission_keeps_error_categories() {
    assert!(matches!(
        admission_error(protocol::ResponseError::new(
            ErrorKind::Unsupported,
            "no orbitals"
        )),
        Error::Unsupported(_)
    ));
    assert!(matches!(
        admission_error(protocol::ResponseError::new(
            ErrorKind::Limit,
            "too many atoms"
        )),
        Error::Limit(_)
    ));
    assert!(matches!(
        admission_error(protocol::ResponseError::new(ErrorKind::Input, "positions")),
        Error::Input(_)
    ));
}

#[test]
fn absent_input_spin_is_anchored_to_the_opened_driver_spin() {
    let mut input = open();
    input.uhf = None;
    let mut opened = opened();
    opened.uhf = 3;
    assert!(validate_opened(&opened, &input, 500).is_ok());
    let mut result = computed(None);
    result.settings.uhf = 3;
    assert!(validate_settings(&result, &opened, &input).is_ok());
    result.settings.uhf = 1;
    assert!(validate_settings(&result, &opened, &input).is_err());
    input.uhf = Some(0);
    assert!(validate_opened(&opened, &input, 500).is_err());
}

fn hessian_result() -> (Compute, Computed) {
    let mut request = request();
    request.hessian =
        Some(serde_json::from_value(json!({"thermo":{"temperature_k":300}})).unwrap());
    let mut result = computed(Some(vec![[0.0; 3]; 2]));
    result.hessian_method = Some("analytic".into());
    result.vibrations = Some(
        serde_json::from_value(json!({
            "conventions":"xtb","frequencies_cm1":[100],"ir_intensities_km_mol":[1],
            "reduced_masses_amu":[1],"force_constants_mdyn_per_angstrom":[1],"masses_amu":[4,4],
            "linear":true,"point_group":"dinfh"
        }))
        .unwrap(),
    );
    result.thermo = Some(
        serde_json::from_value(json!({
            "temperature_k":300,"pressure_pa":101325,"zero_point_energy_hartree":0.01,
            "enthalpy_correction_hartree":0.02,"entropy_hartree_per_k":0.00001,
            "gibbs_correction_hartree":0.017,"frequency_scale":1,"rotor_cutoff_cm1":50,
            "imaginary_cutoff_cm1":20,"symmetry_number":1
        }))
        .unwrap(),
    );
    result.arrays = vec![
        header(ArrayName::Hessian, [6, 6]),
        header(ArrayName::DipoleDerivatives, [6, 3]),
        header(ArrayName::NormalModes, [1, 6]),
    ];
    (request, result)
}

#[test]
fn hessian_header_requires_all_requested_arrays_and_thermo_settings() {
    let (request, mut result) = hessian_result();
    assert!(validate(&result, &request).is_ok());
    result.arrays.remove(1);
    assert!(validate(&result, &request).is_err());
    result.vibrations.as_mut().unwrap().ir_intensities_km_mol = None;
    assert!(validate(&result, &request).is_ok());
    result.vibrations.as_mut().unwrap().ir_intensities_km_mol = Some(vec![1.0]);
    result
        .arrays
        .insert(1, header(ArrayName::DipoleDerivatives, [6, 3]));
    result.arrays.pop();
    assert!(validate(&result, &request).is_err());
    result.arrays.push(header(ArrayName::NormalModes, [1, 6]));
    result.thermo.as_mut().unwrap().temperature_k = 298.15;
    assert!(validate(&result, &request).is_err());
    result.thermo.as_mut().unwrap().temperature_k = 300.0;
    result.vibrations.as_mut().unwrap().reduced_masses_amu[0] = f64::NAN;
    assert!(validate(&result, &request).is_err());
}

#[test]
fn hessian_header_requires_exact_requested_masses() {
    let (mut request, mut result) = hessian_result();
    let hessian = request.hessian.as_mut().unwrap();
    hessian.masses_amu = Some(vec![4.0, 4.0]);
    hessian.thermo.symmetry_number = Some(1);
    assert!(validate(&result, &request).is_ok());
    result.vibrations.as_mut().unwrap().masses_amu[0] = 4.0_f64.next_up();
    assert_eq!(
        validate(&result, &request).unwrap_err(),
        "Worker returned vibrational masses that differ from the request"
    );
}

#[test]
fn vibrational_counts_are_exact_for_linear_and_nonlinear_molecules() {
    let (_, mut result) = hessian_result();
    let vibrations = result.vibrations.as_mut().unwrap();
    let mut opened = opened();
    assert!(validate_vibrations(vibrations, &opened).is_ok());
    vibrations.linear = false;
    assert!(validate_vibrations(vibrations, &opened).is_err());
    vibrations.frequencies_cm1.clear();
    vibrations.ir_intensities_km_mol.as_mut().unwrap().clear();
    vibrations.reduced_masses_amu.clear();
    vibrations.force_constants_mdyn_per_angstrom.clear();
    assert!(validate_vibrations(vibrations, &opened).is_err());
    opened.atoms = 3;
    vibrations.masses_amu.push(4.0);
    for (linear, modes) in [(false, 3), (true, 4)] {
        vibrations.linear = linear;
        vibrations.frequencies_cm1.resize(modes, 100.0);
        vibrations.ir_intensities_km_mol = Some(vec![1.0; modes]);
        vibrations.reduced_masses_amu.resize(modes, 1.0);
        vibrations
            .force_constants_mdyn_per_angstrom
            .resize(modes, 1.0);
        assert!(validate_vibrations(vibrations, &opened).is_ok());
        vibrations.frequencies_cm1.pop();
        vibrations.ir_intensities_km_mol.as_mut().unwrap().pop();
        vibrations.reduced_masses_amu.pop();
        vibrations.force_constants_mdyn_per_angstrom.pop();
        assert!(validate_vibrations(vibrations, &opened).is_err());
    }
}

#[test]
fn zero_mode_ir_presence_matches_dipole_derivatives() {
    let (request, mut result) = hessian_result();
    let mut input = open();
    input.numbers.pop();
    input.positions.pop();
    let mut opened = opened();
    opened.atoms = 1;
    result.gradient.as_mut().unwrap().pop();
    result.charges.as_mut().unwrap().values.pop();
    let vibrations = result.vibrations.as_mut().unwrap();
    vibrations.frequencies_cm1.clear();
    vibrations.ir_intensities_km_mol = Some(vec![]);
    vibrations.reduced_masses_amu.clear();
    vibrations.force_constants_mdyn_per_angstrom.clear();
    vibrations.masses_amu.pop();
    result.arrays = vec![
        header(ArrayName::Hessian, [3, 3]),
        header(ArrayName::DipoleDerivatives, [3, 3]),
    ];
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_ok());
    result.vibrations.as_mut().unwrap().ir_intensities_km_mol = None;
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
    result.arrays.pop();
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_ok());
    result.vibrations.as_mut().unwrap().ir_intensities_km_mol = Some(vec![]);
    assert!(validate_computed_header(&result, Some(&opened), Some(&input), &request).is_err());
}

#[test]
fn hessian_payload_requires_exact_symmetry_and_one_row_per_frequency() {
    let (request, mut result) = hessian_result();
    let frame = open().admit(Some(&request), Some(2), 500).unwrap();
    result.hessian = vec![0.0; 36];
    result.normal_modes = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    assert!(validate_computed_payload(&result, Some(&frame)).is_ok());
    result.hessian[1] = f64::EPSILON;
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
    result.hessian[6] = f64::EPSILON;
    assert!(validate_computed_payload(&result, Some(&frame)).is_ok());
    result.hessian.pop();
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
    result.hessian.push(0.0);
    result.normal_modes.push(0.0);
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
    result.normal_modes = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0].repeat(2);
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
    result.normal_modes.clear();
    assert!(validate_computed_payload(&result, Some(&frame)).is_err());
}

#[test]
fn thermochemistry_echo_accepts_finite_rotor_cutoffs_and_imaginary_magnitudes() {
    let (mut request, mut result) = hessian_result();
    let hessian = request.hessian.as_mut().unwrap();
    let thermo = result.thermo.as_mut().unwrap();
    for cutoff in [-50.0, 0.0, 50.0] {
        hessian.thermo.rotor_cutoff_cm1 = Some(cutoff);
        thermo.rotor_cutoff_cm1 = cutoff;
        assert!(validate_thermo(thermo, hessian).is_ok());
    }
    hessian.thermo.imaginary_cutoff_cm1 = Some(-20.0);
    thermo.imaginary_cutoff_cm1 = 20.0;
    assert!(validate_thermo(thermo, hessian).is_ok());
    thermo.imaginary_cutoff_cm1 = -20.0;
    assert!(validate_thermo(thermo, hessian).is_ok());
    thermo.imaginary_cutoff_cm1 = 21.0;
    assert!(validate_thermo(thermo, hessian).is_err());
    thermo.imaginary_cutoff_cm1 = 20.0;
    for cutoff in [f64::INFINITY, f64::NAN] {
        thermo.rotor_cutoff_cm1 = cutoff;
        assert!(validate_thermo(thermo, hessian).is_err());
    }
}

#[test]
fn gfnff_settings_verify_reference_fragments_and_explicit_charges() {
    let mut input = open();
    input.method = Method::Gfnff;
    input.gfnff = Some(protocol::GfnFfOpen {
        reference_positions: None,
        fragment_charges: Some(vec![0, 0]),
    });
    let mut result = computed(None);
    result.settings.gfnff = Some(protocol::GfnFfSettings {
        reference_positions_bohr: input.positions.clone(),
        fragment_charges: vec![0, 0],
    });
    result.gfnff = Some(protocol::GfnFfReport {
        fragments: vec![0, 1],
    });
    assert!(validate_settings(&result, &opened(), &input).is_ok());
    result
        .settings
        .gfnff
        .as_mut()
        .unwrap()
        .reference_positions_bohr[1][2] = 3.0;
    assert!(validate_settings(&result, &opened(), &input).is_err());
    result
        .settings
        .gfnff
        .as_mut()
        .unwrap()
        .reference_positions_bohr = input.positions.clone();
    result.gfnff.as_mut().unwrap().fragments = vec![1, 0];
    assert!(validate_settings(&result, &opened(), &input).is_err());
    result.gfnff.as_mut().unwrap().fragments = vec![0, 1];
    result.settings.gfnff.as_mut().unwrap().fragment_charges = vec![1, -1];
    assert!(validate_settings(&result, &opened(), &input).is_err());
}

#[test]
fn gfnff_topology_is_fixed_after_the_first_validated_result() {
    let mut input = open();
    input.method = Method::Gfnff;
    let mut result = computed(None);
    result.settings.gfnff = Some(protocol::GfnFfSettings {
        reference_positions_bohr: input.positions.clone(),
        fragment_charges: vec![0, 0],
    });
    result.gfnff = Some(protocol::GfnFfReport {
        fragments: vec![0, 1],
    });
    let mut previous = None;
    validate_settings(&result, &opened(), &input).unwrap();
    remember_gfnff_topology(&mut previous, &result).unwrap();
    remember_gfnff_topology(&mut previous, &result).unwrap();
    result.gfnff.as_mut().unwrap().fragments = vec![0, 0];
    result.settings.gfnff.as_mut().unwrap().fragment_charges = vec![0];
    validate_settings(&result, &opened(), &input).unwrap();
    assert!(remember_gfnff_topology(&mut previous, &result).is_err());
    result.gfnff.as_mut().unwrap().fragments = vec![0, 1];
    result.settings.gfnff.as_mut().unwrap().fragment_charges = vec![0, 0];
    remember_gfnff_topology(&mut previous, &result).unwrap();
    result
        .settings
        .gfnff
        .as_mut()
        .unwrap()
        .reference_positions_bohr[1][2] = 3.0;
    assert!(remember_gfnff_topology(&mut previous, &result).is_err());
}
