//! The two deliberate cleanup-refusal wording differences from the original worker.
use reshiki::chemistry::cleanup;

const LEGACY_CHEMISTRY: &str =
    "Cleanup could not preserve stereochemistry; try including the whole molecule";
const WHOLE_CHEMISTRY: &str = "Cleanup could not preserve stereochemistry; drawing unchanged";
const LEGACY_VISIBLE: &str =
    "Cleanup could not preserve visible stereochemistry; include the whole molecule";
const WHOLE_VISIBLE: &str = "Cleanup could not preserve visible stereochemistry; drawing unchanged";

/// PR #538 retains both stereo guards but distinguishes a component with no
/// fixed atoms: including more atoms cannot repair that whole-component refusal.
/// Adapt only its matching typed variant and exact original message. Every other
/// original error remains an exact comparison, including partial cleanup hints.
pub(super) fn expected_refusal<'a>(error: &cleanup::Error, original: &'a str) -> &'a str {
    match (error, original) {
        (cleanup::Error::ChemistryChangedWhole, LEGACY_CHEMISTRY) => WHOLE_CHEMISTRY,
        (cleanup::Error::VisibleChangedWhole, LEGACY_VISIBLE) => WHOLE_VISIBLE,
        _ => original,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_component_refusals_have_only_the_two_deliberate_message_changes() {
        for (error, original, expected) in [
            (
                cleanup::Error::ChemistryChangedWhole,
                LEGACY_CHEMISTRY,
                WHOLE_CHEMISTRY,
            ),
            (
                cleanup::Error::VisibleChangedWhole,
                LEGACY_VISIBLE,
                WHOLE_VISIBLE,
            ),
        ] {
            assert_eq!(expected_refusal(&error, original), expected);
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn partial_refusals_other_causes_and_unrecognized_messages_remain_exact() {
        for (error, original) in [
            (cleanup::Error::ChemistryChanged, LEGACY_CHEMISTRY),
            (cleanup::Error::VisibleChanged, LEGACY_VISIBLE),
            (cleanup::Error::ChemistryChangedWhole, LEGACY_VISIBLE),
            (cleanup::Error::VisibleChangedWhole, LEGACY_CHEMISTRY),
            (cleanup::Error::Fixed, LEGACY_VISIBLE),
        ] {
            assert_eq!(expected_refusal(&error, original), original);
        }
        assert_eq!(
            cleanup::Error::ChemistryChanged.to_string(),
            LEGACY_CHEMISTRY
        );
        assert_eq!(cleanup::Error::VisibleChanged.to_string(), LEGACY_VISIBLE);
        for original in [
            "Cleanup refused for another reason",
            "Cleanup could not preserve visible stereochemistry; include the whole molecule!",
            "Cleanup could not preserve stereochemistry; try including the whole molecule ",
        ] {
            for error in [
                cleanup::Error::ChemistryChangedWhole,
                cleanup::Error::VisibleChangedWhole,
            ] {
                assert_eq!(expected_refusal(&error, original), original);
            }
        }
    }
}
