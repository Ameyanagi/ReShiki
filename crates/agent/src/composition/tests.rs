use super::*;
use crate::editing::{self, Transform};

/// Two regular structures rotated off their 30-degree drawing axes. (The
/// bond-join fixture's 45 and 112.5 degree bonds keep it below the alignment
/// threshold at every rotation.)
fn rotated() -> Document {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/ui-expanded.reshiki"
    ))
    .unwrap();
    let ids = doc.all_ids();
    editing::transform(&mut doc, &ids, Transform::Rotate(7.));
    doc
}

#[test]
fn straighten_all_realigns_rotated_structures_once() {
    let mut doc = rotated();
    let before = doc.clone();
    let straightened = straighten_all(&mut doc);
    assert!(straightened > 0);
    assert_ne!(doc, before);
    assert_eq!(straighten_all(&mut doc), 0);
}
