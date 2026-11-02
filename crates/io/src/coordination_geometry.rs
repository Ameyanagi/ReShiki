//! Recognize depiction-only wedge/hash bonds at coordination-frame metals.
use crate::document::{Bond, Document};

/// A single-bond wedge/hash narrowed at an unassigned metal shows frame depth.
pub(crate) fn frame_bond(document: &Document, bond: &Bond) -> bool {
    bond.order == 1
        && matches!(bond.display.as_str(), "wedge" | "hash")
        && document
            .atom(bond.a)
            .is_some_and(|a| crate::joining::coordination_metal(a) && a.stereo.is_none())
}
