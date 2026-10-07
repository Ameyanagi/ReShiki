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
