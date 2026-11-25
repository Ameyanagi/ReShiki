use super::*;
use crate::worker::Worker;

fn raw_frame(body: &str) -> Vec<u8> {
    let mut frame = Vec::from(*b"RSHXTBWK\x01\x00\x00\x00");
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
    invalid[8] = 2;
    assert!(matches!(decode_header(&invalid), Err(FrameError::Version)));
    let mut invalid = header;
    invalid[10] = 1;
    assert!(matches!(decode_header(&invalid), Err(FrameError::Kind)));
    let mut invalid = header;
    invalid[12..].copy_from_slice(&(MAX_JSON_BYTES as u32 + 1).to_le_bytes());
    assert!(matches!(decode_header(&invalid), Err(FrameError::Length)));

    assert!(matches!(
        encode_frame(&"x".repeat(MAX_JSON_BYTES)),
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
        &open.replace("\"heap_bytes\"", "\"other\":0,\"heap_bytes\""),
        &open.replace(
            "\"heap_bytes\"",
            r#""solvent":{"model":"alpb","name":"water","other":0},"heap_bytes""#,
        ),
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

#[test]
fn computed_wire_requires_a_gap_field_even_without_a_lumo() {
    let mut value = serde_json::to_value(Computed {
        method: Method::Gfn2,
        conventions: Conventions::Xtb68,
        numerics: "gfn2:1".into(),
        energy: -1.0,
        gradient: None,
        homo_lumo_gap_ev: None,
        near_degenerate: false,
        scf_iterations: 1,
        seconds: 0.0,
    })
    .unwrap();
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
fn near_degenerate_requires_a_gap_strictly_below_the_threshold() {
    assert!(is_near_degenerate(Some(NEAR_DEGENERATE_GAP_EV.next_down())));
    assert!(!is_near_degenerate(Some(NEAR_DEGENERATE_GAP_EV)));
    assert!(!is_near_degenerate(Some(NEAR_DEGENERATE_GAP_EV.next_up())));
    assert!(!is_near_degenerate(None));
}

#[test]
fn invalid_request_values() {
    let mut open = Open {
        numbers: vec![1; MAX_ATOMS + 1],
        positions: vec![[0.0; 3]; MAX_ATOMS + 1],
        units: Units::Bohr,
        charge: 0,
        uhf: None,
        method: Method::Gfn2,
        conventions: Conventions::Axtb,
        solvent: None,
        threads: 1,
        heap_bytes: 1 << 20,
    };
    assert_eq!(open.validate().unwrap_err().kind, ErrorKind::Limit);
    open.numbers.truncate(1);
    assert_eq!(open.validate().unwrap_err().kind, ErrorKind::Input);
    open.positions = vec![[f64::NAN, 0.0, 0.0]];
    assert_eq!(open.validate().unwrap_err().kind, ErrorKind::Input);
    open.positions[0][0] = 0.0;
    open.threads = 65;
    assert_eq!(open.validate().unwrap_err().kind, ErrorKind::Input);
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
    let name = "水".repeat((MAX_JSON_BYTES - prefix.len() - suffix.len()) / "水".len());
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
    assert_eq!(ok.protocol, 1);
    assert_eq!(ok.max_atoms, MAX_ATOMS);
    assert!(ok.engine.is_none());
    assert!(ok.methods.is_empty());
    assert!(ok.operations.is_empty());

    let response: Response<Opened> = read_frame(&mut output).unwrap().unwrap();
    assert_eq!(response.id, 2);
    let Outcome::Error { error } = response.result else {
        panic!("expected unavailable");
    };
    assert_eq!(error.kind, ErrorKind::Unavailable);
    assert_eq!(error.message, "xTB is not included in this build");
    assert!(output.is_empty());
}
