//! Exact generic MOL/RXN rejection contracts derived independently from input.
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Deserialize, Serialize)]
pub struct GenericRestriction {
    pub category: String,
    pub error: String,
}

pub fn rejection_difference<T, E: Display>(
    restriction: &GenericRestriction,
    original_success: bool,
    actual: &Result<T, E>,
) -> Option<String> {
    if !original_success {
        return Some(format!(
            "{} restriction lacks original success",
            restriction.category
        ));
    }
    match actual {
        Err(error) if error.to_string() == restriction.error => None,
        Err(error) => Some(format!(
            "{} restriction returned {error}, expected {}",
            restriction.category, restriction.error
        )),
        Ok(_) => Some(format!(
            "{} restriction unexpectedly succeeded; expected {}",
            restriction.category, restriction.error
        )),
    }
}

#[test]
fn restrictions_require_original_success_and_exact_current_rejection() {
    let restriction = GenericRestriction {
        category: "polymer_crossing_membership".into(),
        error: "intended error".into(),
    };
    assert!(rejection_difference::<(), _>(&restriction, true, &Err("intended error")).is_none());
    assert!(rejection_difference::<(), _>(&restriction, false, &Err("intended error")).is_some());
    assert!(rejection_difference::<(), _>(&restriction, true, &Err("different error")).is_some());
    assert!(rejection_difference::<_, &str>(&restriction, true, &Ok(())).is_some());
}
