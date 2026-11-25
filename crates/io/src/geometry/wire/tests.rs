use super::*;
#[test]
fn frames_reject_truncation_trailing_bytes_unknown_fields_and_oversize() {
    let frame = encode(&serde_json::json!({"value":1}), 128).unwrap();
    let value: serde_json::Value = decode(&frame, 128).unwrap();
    assert_eq!(value["value"], 1);
    for n in 0..frame.len() {
        assert!(decode::<serde_json::Value>(&frame[..n], 128).is_err());
    }
    let mut extra = frame.clone();
    extra.push(0);
    assert!(decode::<serde_json::Value>(&extra, 128).is_err());
    assert!(decode::<serde_json::Value>(&frame, frame.len() - 1).is_err());
    assert!(encode(&vec!["overlimit"; 100], 32).is_err());
    assert!(
        decode::<Request>(
            &encode(&serde_json::json!({"unexpected":true}), 128).unwrap(),
            128
        )
        .is_err()
    );
}

#[test]
fn legacy_protocol_defaults_to_strict_with_no_approximations_or_ensemble() {
    let mut document = crate::document::Document::default();
    document.add_atom("C", crate::document::Point::new(0., 0.));
    let prepared = super::super::Prepared::new(&document, &[]).unwrap();
    let request = prepared
        .generation_request(super::super::ForceField::Uff, super::super::DEFAULT_SEED)
        .unwrap();
    let mut value = serde_json::to_value(Request {
        heap_bytes: 64 * 1024 * 1024,
        capacity: None,
        operation: request,
    })
    .unwrap();
    value["operation"]
        .as_object_mut()
        .unwrap()
        .remove("relaxed");
    assert!(value["operation"].get("ensemble").is_none());
    let frame = encode(&value, MAX_REQUEST_BYTES).unwrap();
    let legacy: Request = decode(&frame, MAX_REQUEST_BYTES).unwrap();
    assert!(!legacy.operation.relaxed);
    assert!(legacy.operation.ensemble.is_none());
    legacy.operation.validate().unwrap();

    let response = serde_json::json!({
        "version": reshiki_geometry::RDKIT_VERSION,
        "result": { "Ok": {
            "coordinates": [[0., 0., 0.]], "hydrogen_parents": [], "original_count": 1,
            "initial_energy": 0., "energy": 0., "gradient": null, "converged": true,
            "field": "UFF", "diagnostics": []
        }}
    });
    let frame = encode(&response, MAX_RESPONSE_BYTES).unwrap();
    let legacy: Response = decode(&frame, MAX_RESPONSE_BYTES).unwrap();
    let legacy = legacy.result.unwrap();
    assert!(legacy.approximations.is_empty());
    assert!(legacy.ensemble.is_empty());
    assert!(
        serde_json::to_value(legacy)
            .unwrap()
            .get("ensemble")
            .is_none()
    );
}
