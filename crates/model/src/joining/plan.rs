//! Route classification shared by Join admission and shortcut execution.
use super::{coordination_metal, source_intersects_generic};
use crate::{document::Document, generic};

/// The operation selected by the Join shortcut, in its existing route precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinPlan {
    /// Add an in-place donor-to-metal contact without sharing either atom.
    Coordinate { donor: u64, metal: u64 },
    /// Move the source fragment so its atom shares the target atom.
    ShareAtom { source: u64, target: u64 },
    /// Move the first selected bond onto the second, in document bond order.
    FuseBond {
        source: (u64, u64),
        target: (u64, u64),
    },
    /// Merge the selected atoms without preparing a moving fragment.
    Merge,
}

/// Classify Join without validating whether the selected IDs are atoms or unique.
/// Callers retain their own admission rules and execution error precedence.
pub fn join_plan(doc: &Document, selected: &[u64]) -> Option<JoinPlan> {
    if let [source, target] = selected {
        let source_metal = doc.atom(*source).is_some_and(coordination_metal);
        let target_metal = doc.atom(*target).is_some_and(coordination_metal);
        return Some(if source_metal || target_metal {
            let (donor, metal) = if source_metal {
                (*target, *source)
            } else {
                (*source, *target)
            };
            JoinPlan::Coordinate { donor, metal }
        } else {
            JoinPlan::ShareAtom {
                source: *source,
                target: *target,
            }
        });
    }
    if selected.len() == 4 {
        let bonds: Vec<_> = doc
            .bonds
            .iter()
            .filter(|bond| selected.contains(&bond.a) && selected.contains(&bond.b))
            .collect();
        if let [source, target] = bonds.as_slice()
            && source.a != target.a
            && source.a != target.b
            && source.b != target.a
            && source.b != target.b
        {
            return Some(JoinPlan::FuseBond {
                source: (source.a, source.b),
                target: (target.a, target.b),
            });
        }
    }
    (selected.len() >= 3).then_some(JoinPlan::Merge)
}

impl JoinPlan {
    /// Check the generic dependencies touched by this route. Merge uses the
    /// caller's selection because its plan does not carry individual atom IDs.
    pub fn generic_blocked(&self, doc: &Document, selected: &[u64]) -> bool {
        match *self {
            Self::Coordinate { donor, metal } => generic::touches(doc, &[donor, metal]),
            Self::ShareAtom { source, target } => {
                source_intersects_generic(doc, &[source]) || generic::touches(doc, &[target])
            }
            Self::FuseBond { source, target } => {
                source_intersects_generic(doc, &[source.0, source.1])
                    || generic::touches(doc, &[target.0, target.1])
            }
            Self::Merge => generic::touches(doc, selected),
        }
    }
}
