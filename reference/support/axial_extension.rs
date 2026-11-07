//! #892 opens wedged atropisomer drawings that the original importer refused
//! as outside its editable chemistry contract. Accept such a case only when
//! ReShiki perceived an axis (bond stereo 6/7, as RDKit does for this input);
//! the axis hand and its M/P label have their own tests.
use serde_json::Value;

/// The original worker raises "This bond stereochemistry is not supported
/// yet." only for RDKit's atropisomer bond stereo. Its generic contract
/// rejections count only for the atropisomer corpus and RDKit's own wedged
/// biaryl reaction fixture for GitHub #7669 (added from the source tree on
/// Linux and Windows CI).
pub fn accepted(name: &str, failure: Option<&str>, actual: &Value) -> bool {
    let atropisomer = failure == Some("This bond stereochemistry is not supported yet.")
        || ((name.contains("atropisomer/") || name == "reaction source/github7669_1.rxn")
            && matches!(
                failure,
                Some(
                    "Outside the existing editable chemistry contract"
                        | "Outside the editable reaction contract"
                )
            ));
    atropisomer && has_axis(actual)
}

fn has_axis(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("metadata")
                .and_then(|m| m["bonds"].as_array())
                .is_some_and(|bonds| {
                    bonds
                        .iter()
                        .any(|b| matches!(b["stereo"].as_u64(), Some(6 | 7)))
                })
                || map.values().any(has_axis)
        }
        Value::Array(items) => items.iter().any(has_axis),
        _ => false,
    }
}
