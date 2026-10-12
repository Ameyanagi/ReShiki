use super::ErrorKind;
use std::collections::HashSet;

const KIND_COUNT: usize = 13;

// Exhaustiveness makes adding a variant require updating this independent
// inventory check, even if ALL accidentally omits the new kind.
fn kind_index(kind: ErrorKind) -> usize {
    match kind {
        ErrorKind::UnknownTool => 0,
        ErrorKind::InvalidArguments => 1,
        ErrorKind::UnknownDocument => 2,
        ErrorKind::UnknownObject => 3,
        ErrorKind::Stale => 4,
        ErrorKind::Busy => 5,
        ErrorKind::Budget => 6,
        ErrorKind::Timeout => 7,
        ErrorKind::Rejected => 8,
        ErrorKind::Unsupported => 9,
        ErrorKind::Access => 10,
        ErrorKind::Failed => 11,
        ErrorKind::Cancelled => 12,
    }
}

#[test]
fn all_lists_every_kind_and_code_once() {
    let mut indices: Vec<_> = ErrorKind::ALL.iter().copied().map(kind_index).collect();
    indices.sort_unstable();
    assert_eq!(indices, (0..KIND_COUNT).collect::<Vec<_>>());

    let codes: HashSet<_> = ErrorKind::ALL.iter().map(|kind| kind.code()).collect();
    assert_eq!(codes.len(), KIND_COUNT);
    for kind in ErrorKind::ALL {
        assert!(!kind.meaning().trim().is_empty(), "{kind:?}");
        assert_eq!(
            kind.is_protocol_error(),
            *kind == ErrorKind::UnknownTool,
            "{kind:?}"
        );
    }
}

#[test]
fn operation_codes_remain_stable() {
    let codes: Vec<_> = ErrorKind::ALL.iter().map(|kind| kind.code()).collect();
    assert_eq!(
        codes,
        [
            "unknown_tool",
            "invalid_arguments",
            "unknown_document",
            "unknown_object",
            "stale",
            "busy",
            "budget",
            "timeout",
            "rejected",
            "unsupported",
            "access_denied",
            "failed",
            "cancelled",
        ]
    );
}
