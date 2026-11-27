//! Shared descriptions of incomplete downhill IRC traces and endpoint relaxation.

use crate::qcdata::IrcEndpoint::{self, Product, Reactant};

/// Describe an incomplete trace in negative/positive path order. Relaxed
/// endpoints do not imply that the preceding downhill traces completed.
pub fn trace_detail(
    [negative_steps, positive_steps]: [usize; 2],
    completed: [bool; 2],
    [negative, positive]: [IrcEndpoint; 2],
    relaxed: bool,
) -> Option<String> {
    let step_count = |steps| format!("{steps} {}", if steps == 1 { "step" } else { "steps" });
    let branch = |steps, endpoint, other, direction| {
        let side = match endpoint {
            Reactant if other != Reactant => "on the reactant side",
            Product if other != Product => "on the product side",
            _ => direction,
        };
        format!("{} {side}", step_count(steps))
    };
    let negative_branch = || {
        branch(
            negative_steps,
            negative,
            positive,
            "in the negative direction",
        )
    };
    let positive_branch = || {
        branch(
            positive_steps,
            positive,
            negative,
            "in the positive direction",
        )
    };
    let mut traced = match completed {
        [true, true] => return None,
        [false, true] => format!("The path stopped after {}.", negative_branch()),
        [true, false] => format!("The path stopped after {}.", positive_branch()),
        [false, false] if negative_steps == positive_steps => format!(
            "The path was followed for {} in each direction.",
            step_count(negative_steps)
        ),
        [false, false] => format!(
            "The path was followed for {} and {}.",
            negative_branch(),
            positive_branch()
        ),
    };
    if relaxed {
        traced.push_str(" Both ends were then relaxed to the nearest minimum.");
    }
    Some(traced)
}
