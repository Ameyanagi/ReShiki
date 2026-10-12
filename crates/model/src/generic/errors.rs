//! Shared refusal text. Tests match these strings; keep them byte-identical.

/// The `&'static str` form of [`retained`], for consts and format strings.
macro_rules! retained_text {
    ($reason:literal) => {
        concat!($reason, "; the source drawing is retained")
    };
}
pub(super) use retained_text;

const RETAINED_SUFFIX: &str = retained_text!("");

pub(super) const ID_LIMIT: &str = "Generic object ID limit exceeded";

/// A refusal that leaves the source drawing unchanged.
pub(super) fn retained(reason: &str) -> String {
    format!("{reason}{RETAINED_SUFFIX}")
}

pub(super) fn variable_shared() -> String {
    retained("Variable attachment requires a detachable substituent with one anchor")
}

pub(super) fn variable_exterior() -> String {
    retained("Variable attachment has no supported exterior placement")
}

pub(super) fn variable_precision() -> String {
    retained("Variable attachment cannot preserve bond geometry at this coordinate precision")
}

pub(super) fn variable_collision() -> String {
    retained("Variable attachment cannot clear the preserved bonds and atom labels")
}
