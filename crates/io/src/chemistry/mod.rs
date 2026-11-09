//! The reshiki-chemistry core and reshiki-model adapters, plus this crate's adapters.
pub use reshiki_model::chemistry::*;

pub mod cdxml;
pub mod cleanup;
pub(crate) mod layout_refinement;
pub mod molfile;
pub mod reaction;

/// Check #1064's possible axis gain only for an axis-free source. Other
/// drawings, including ones that cannot be prepared, keep their import policy.
pub(crate) fn crossed_double_has_axis(drawing: &crate::document::Document) -> bool {
    drawing
        .bonds
        .iter()
        .any(|bond| !bond.projection && bond.order == 2 && bond.display == "wavy")
        && document::prepare(drawing).is_ok_and(|molecule| {
            molecule
                .state
                .metadata
                .bonds
                .iter()
                .any(|bond| matches!(bond.stereo, 6 | 7))
        })
}

#[cfg(test)]
mod allene_generated_tests;
#[cfg(test)]
mod allene_tests;
#[cfg(test)]
mod axial_tests;
