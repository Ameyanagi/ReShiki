//! Exact generic MOL/RXN rejection contracts derived independently from input,
//! and documented differences that import with a warning instead.
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Deserialize, Serialize)]
pub struct GenericRestriction {
    pub category: String,
    /// The exact rejection; none when ReShiki imports the input with a warning
    /// and its result differs from the original by design.
    pub error: Option<String>,
}

pub fn rejection_difference<T, E: Display>(
    restriction: &GenericRestriction,
    original_success: bool,
    actual: &Result<T, E>,
) -> Option<String> {
    if !original_success && restriction.error.is_some() {
        return Some(format!(
            "{} restriction lacks original success",
            restriction.category
        ));
    }
    match (actual, &restriction.error) {
        (Ok(_), None) => None,
        (Err(_), None) if !original_success => None,
        (Err(error), None) => Some(format!(
            "{} documented difference was rejected: {error}",
            restriction.category
        )),
        (Err(error), Some(expected)) if error.to_string() == *expected => None,
        (Err(error), Some(expected)) => Some(format!(
            "{} restriction returned {error}, expected {expected}",
            restriction.category
        )),
        (Ok(_), Some(expected)) => Some(format!(
            "{} restriction unexpectedly succeeded; expected {expected}",
            restriction.category
        )),
    }
}

#[test]
fn restrictions_require_original_success_and_exact_current_rejection() {
    let restriction = GenericRestriction {
        category: "polymer_crossing_membership".into(),
        error: Some("intended error".into()),
    };
    assert!(rejection_difference::<(), _>(&restriction, true, &Err("intended error")).is_none());
    assert!(rejection_difference::<(), _>(&restriction, false, &Err("intended error")).is_some());
    assert!(rejection_difference::<(), _>(&restriction, true, &Err("different error")).is_some());
    assert!(rejection_difference::<_, &str>(&restriction, true, &Ok(())).is_some());
    let warned = GenericRestriction {
        category: "substance_group_charge_range".into(),
        error: None,
    };
    assert!(rejection_difference::<_, &str>(&warned, true, &Ok(())).is_none());
    assert!(rejection_difference::<(), _>(&warned, true, &Err("any error")).is_some());
    // The wrapped value can be why the original rejected the input.
    assert!(rejection_difference::<_, &str>(&warned, false, &Ok(())).is_none());
    assert!(rejection_difference::<(), _>(&warned, false, &Err("any error")).is_none());
}
