//! Translate only recognized native chemistry exceptions at the selected-scope
//! warning boundary. Resource and invariant failures never become successful edits.
use crate::chemistry::{document::Error, sanitize};

pub(super) fn native_message(error: &Error) -> Option<String> {
    match error {
        // Native throws a build-specific C++ assertion here. Preserve the
        // recoverable chemistry failure without pretending Rust produced it.
        Error::Drawing(message) if message.starts_with("Unknown element ") => Some(message.clone()),
        Error::Drawing(message)
            if message == "Radical count conflicts with the atom valence or explicit hydrogens"
                || message
                    == "A hydrogen bond must start at a covalently bound explicit H and end at an acceptor (N, O, F or S)"
                || message
                    == "Bond stereo references must be attached to the corresponding endpoints" =>
        {
            Some(message.clone())
        }
        Error::Sanitization(error) => {
            let sanitize::Cause::Invalid(message) = &error.cause else {
                return None;
            };
            if let Some(rest) = message.strip_prefix("Atom ") {
                let (index, reason) = rest.split_once(": ")?;
                let index = index.parse::<usize>().ok()?.checked_sub(1)?;
                if let Some(rest) = reason.strip_prefix("Explicit valence ") {
                    let (valence, symbol) = rest.split_once(" is too large for ")?;
                    return Some(format!(
                        "Explicit valence for atom # {index} {symbol}, {valence}, is greater than permitted"
                    ));
                }
                if reason == "Unreasonable formal charge on hydrogen" {
                    return Some(format!("Unreasonable formal charge on atom # {index}."));
                }
                if reason == "Aromatic atom has no matching allowed valence" {
                    return Some(format!(
                        "Explicit valence for aromatic atom # {index} not equal to any accepted valence\n"
                    ));
                }
                if let Some(symbol) =
                    reason.strip_prefix("Valence including radicals is too large for ")
                {
                    return Some(format!(
                        "Explicit valence for atom # {index} {symbol} greater than permitted"
                    ));
                }
            }
            if let Some(index) = message
                .strip_prefix("Non-ring atom ")
                .and_then(|s| s.strip_suffix(" marked aromatic"))
            {
                return Some(format!(
                    "non-ring atom {} marked aromatic",
                    index.parse::<usize>().ok()?.checked_sub(1)?
                ));
            }
            if error.stage == sanitize::Stage::Kekulize
                && message.starts_with("Can't kekulize mol.  Unkekulized atoms:")
            {
                return Some(message.clone());
            }
            None
        }
        _ => None,
    }
}
