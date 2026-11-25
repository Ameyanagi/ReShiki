use super::*;
use crate::protocol::{Conventions, Method};

fn computed(gradient: Option<Vec<[f64; 3]>>) -> Computed {
    Computed {
        method: Method::Gfn2,
        conventions: Conventions::Xtb68,
        numerics: "gfn2:1".into(),
        energy: -1.0,
        gradient,
        homo_lumo_gap_ev: None,
        near_degenerate: false,
        scf_iterations: 3,
        seconds: 0.1,
    }
}

fn opened() -> Opened {
    Opened {
        atoms: 2,
        method: Method::Gfn2,
        conventions: Conventions::Xtb68,
        numerics: "gfn2:1".into(),
    }
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
fn hello_metadata_distinguishes_unavailable_and_inconsistent_workers() {
    let mut hello = Hello {
        protocol: protocol::VERSION,
        engine: None,
        methods: vec![],
        operations: vec![],
        max_atoms: protocol::MAX_ATOMS,
    };
    assert!(validate_hello(&hello).is_ok());
    hello.methods.push(protocol::Method::Gfn2);
    assert!(validate_hello(&hello).is_err());
    hello.engine = Some(protocol::Engine {
        name: "axtb".into(),
        version: "0.2.0".into(),
        license: "LGPL-3.0-or-later".into(),
        source: "https://github.com/Ameyanagi/axtb".into(),
        rev: "revision".into(),
    });
    hello.operations = vec![protocol::Operation::Energy, protocol::Operation::Gradient];
    assert!(validate_hello(&hello).is_ok());
    hello.operations = vec![protocol::Operation::Energy; 2];
    assert!(validate_hello(&hello).is_err());
    hello.max_atoms = 0;
    assert!(validate_hello(&hello).is_err());
}

#[test]
fn computed_validation_requires_exact_requested_finite_gradient() {
    let opened = Some(&opened());
    assert!(validate_computed(&computed(None), opened, false).is_ok());
    assert!(validate_computed(&computed(None), opened, true).is_err());
    assert!(validate_computed(&computed(Some(vec![[0.0; 3]; 2])), opened, true).is_ok());
    assert!(validate_computed(&computed(Some(vec![[0.0; 3]; 1])), opened, true).is_err());
    assert!(validate_computed(&computed(Some(vec![[0.0; 3]; 2])), opened, false).is_err());
    assert!(validate_computed(&computed(Some(vec![[f64::NAN; 3]; 2])), opened, true).is_err());
    assert!(validate_computed(&computed(None), None, false).is_err());
    let mut invalid = computed(None);
    invalid.energy = f64::INFINITY;
    assert!(validate_computed(&invalid, opened, false).is_err());
    invalid.energy = -1.0;
    invalid.seconds = f64::NAN;
    assert!(validate_computed(&invalid, opened, false).is_err());
    invalid.seconds = -0.1;
    assert!(validate_computed(&invalid, opened, false).is_err());
}

#[test]
fn computed_validation_checks_the_gap_and_its_flag() {
    let opened = Some(&opened());
    let mut result = computed(None);
    result.near_degenerate = true;
    assert!(validate_computed(&result, opened, false).is_err());
    for gap in [-0.1, f64::INFINITY, f64::NAN] {
        result.homo_lumo_gap_ev = Some(gap);
        result.near_degenerate = protocol::is_near_degenerate(Some(gap));
        assert!(validate_computed(&result, opened, false).is_err());
    }
    result.homo_lumo_gap_ev = Some(0.05);
    assert!(validate_computed(&result, opened, false).is_err());
    result.near_degenerate = true;
    assert!(validate_computed(&result, opened, false).is_ok());
    result.homo_lumo_gap_ev = Some(protocol::NEAR_DEGENERATE_GAP_EV);
    assert!(validate_computed(&result, opened, false).is_err());
    result.near_degenerate = false;
    assert!(validate_computed(&result, opened, false).is_ok());
}

#[test]
fn computed_validation_checks_open_session_metadata() {
    let opened = Some(&opened());
    let mut result = computed(None);
    result.conventions = Conventions::Axtb;
    assert!(validate_computed(&result, opened, false).is_err());
    result.conventions = Conventions::Xtb68;
    result.numerics = "gfn2:2".into();
    assert!(validate_computed(&result, opened, false).is_err());
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
fn one_frame_is_parsed_across_short_reads_and_trailing_data_is_rejected() {
    let frame = protocol::encode_frame(&serde_json::json!({"id": 1, "ok": {}})).unwrap();
    let mut bytes = Vec::new();
    let mut expected = None;
    for byte in &frame {
        append_output(&mut bytes, &mut expected, &[*byte]).unwrap();
    }
    assert_eq!(expected, Some(frame.len()));
    assert!(append_output(&mut bytes, &mut expected, b"x").is_err());
    assert!(append_output(&mut Vec::new(), &mut None, &[0; protocol::HEADER_LEN]).is_err());
}
