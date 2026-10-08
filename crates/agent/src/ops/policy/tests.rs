use super::*;

#[test]
fn the_policy_matrix() {
    let unknown = Err(OpError::new(ErrorKind::UnknownDocument, UNKNOWN_DOCUMENT));
    let read_only = Err(OpError::new(
        ErrorKind::Rejected,
        "live documents are read-only in this version",
    ));
    let cases = [
        (HandleKind::Session, Access::Read, true, Ok(())),
        (HandleKind::Session, Access::Edit, true, Ok(())),
        (HandleKind::Session, Access::Read, false, unknown.clone()),
        (HandleKind::Session, Access::Edit, false, unknown),
        (HandleKind::Live, Access::Read, true, Ok(())),
        (HandleKind::Live, Access::Read, false, Ok(())),
        (HandleKind::Live, Access::Edit, true, read_only.clone()),
        (HandleKind::Live, Access::Edit, false, read_only),
    ];
    for (kind, access, owner_matches, expected) in cases {
        assert_eq!(
            check(kind, access, owner_matches),
            expected,
            "{kind:?} {access:?} owner_matches={owner_matches}"
        );
    }
}

#[test]
fn effects_serialize_in_snake_case() {
    assert_eq!(
        serde_json::to_value(Effect::Applied).unwrap(),
        serde_json::json!("applied")
    );
    assert_eq!(
        serde_json::to_value(Effect::Proposed).unwrap(),
        serde_json::json!("proposed")
    );
}
