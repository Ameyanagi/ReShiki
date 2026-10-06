use super::*;
use crate::{
    document::Document,
    engine::{PROTOCOL, Request},
};

#[test]
fn versions_keep_four_separate_numbers() {
    let versions = Versions::current("9.8.7");
    assert_eq!(versions.app, "9.8.7");
    assert_eq!(versions.operation_api, OPERATION_API_VERSION);
    assert_eq!(versions.engine_protocol, PROTOCOL);
    assert_eq!(PROTOCOL, 1);
    assert_eq!(versions.document, crate::document::VERSION);
}

#[test]
fn engine_requests_use_the_protocol_constant() {
    assert_eq!(Request::import("smiles", "C").protocol, PROTOCOL);
    assert_eq!(
        Request::molecule("analyze", Document::default()).protocol,
        PROTOCOL
    );
    let json = serde_json::to_string(&Request::import("smiles", "C")).unwrap();
    assert!(json.contains(r#""protocol":1"#), "{json}");
}

#[test]
fn validation_status_carries_transaction_rejection() {
    let rejection = Rejection::Invalid("Invalid group ID or membership".into());
    let envelope = Envelope {
        value: (),
        warnings: vec![],
        validation: ValidationStatus::Rejected(rejection.clone()),
        versions: Versions::current("9.8.7"),
    };
    assert_eq!(envelope.validation, ValidationStatus::Rejected(rejection));
    assert_ne!(envelope.validation, ValidationStatus::Valid);
    let ValidationStatus::Rejected(rejection) = envelope.validation else {
        panic!("expected a rejection");
    };
    assert_eq!(rejection.message(), "Invalid group ID or membership");
}

#[test]
fn worker_payload_protocol_serializes_identically() {
    let constant = serde_json::json!({"protocol": crate::engine::PROTOCOL});
    let literal = serde_json::json!({"protocol": 1});
    assert_eq!(constant, literal);
    assert_eq!(constant.to_string(), literal.to_string());
}
