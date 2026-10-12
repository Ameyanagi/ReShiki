//! Ordered export loss notices for each native response route.

use crate::document::Document;

pub(crate) const DATIVE_MOL_NOTICE: &str = "MOL retains directed dative chemistry and metal charge, but omits solid/hashed coordination projection appearance. Use native ReShiki or figure export to retain that drawing.";
pub(crate) const DATIVE_CDX_NOTICE: &str = "CDXML/CDX retains directed coordination chemistry and ReShiki projection attributes. ChemDraw 26 displays dative arrows and removes wedge/hash paint on save. Use native ReShiki or figure export for faithful projection appearance.";
pub(crate) const REACTION_ROLES_NOTICE: &str = "This drawing or molecule format does not retain reaction roles. Use RXN/reaction SMILES for reaction data, or .reshiki for the complete scheme.";
pub(crate) const UNVALIDATED_NOTICE: &str =
    "Drawing exported; chemical assignments remain unvalidated.";

pub(crate) enum Route<'a> {
    AttachmentEarlyExport,
    UnpreparedFallback,
    Prepared {
        source: &'a Document,
        figure_only: bool,
    },
    Reaction,
}

// Drawing routes share the CDXML/CDX and MOL writers, so the same loss gets
// the same notice regardless of route; route-specific notices come first.
pub(crate) fn notices(document: &Document, format: &str, route: Route<'_>) -> Vec<String> {
    match route {
        Route::AttachmentEarlyExport => {
            let mut notices = vec![crate::attachments::ANALYSIS_NOTICE.into()];
            notices.extend(drawing_losses(document, document, format, true));
            notices
        }
        Route::UnpreparedFallback => {
            let mut notices = vec![UNVALIDATED_NOTICE.into()];
            notices.extend(drawing_losses(document, document, format, true));
            notices
        }
        Route::Prepared {
            source,
            figure_only,
        } => drawing_losses(document, source, format, !figure_only),
        Route::Reaction => {
            // RXN and reaction SMILES use the same notices regardless of format.
            let mut notices = vec![crate::chemistry::reaction::EXPORT_WARNING.into()];
            if crate::rear_opacity::present(document) {
                notices.push(crate::rear_opacity::EXPORT_NOTICE.into());
            }
            notices
        }
    }
}

fn drawing_losses(
    document: &Document,
    appearance: &Document,
    format: &str,
    roles: bool,
) -> Vec<String> {
    let mut notices = Vec::new();
    if roles && !document.reactions.is_empty() {
        notices.push(REACTION_ROLES_NOTICE.into());
    }
    // MOL and identifiers omit arrows, so the link-loss notice applies only to CDXML/CDX.
    if matches!(format, "cdxml" | "cdx")
        && let Some(notice) = crate::arrow_anchors::export_notice(document)
    {
        notices.push(notice);
    }
    // Preparation can change appearance, so rear opacity reads the source document.
    // CDXML/CDX rejects rear opacity in the writer before returning any notices.
    if matches!(format, "mol" | "smiles" | "inchi") && crate::rear_opacity::present(appearance) {
        notices.push(crate::rear_opacity::EXPORT_NOTICE.into());
    }
    if document.bonds.iter().any(|bond| bond.is_projected_dative()) {
        match format {
            "mol" => notices.push(DATIVE_MOL_NOTICE.into()),
            "cdxml" | "cdx" => notices.push(DATIVE_CDX_NOTICE.into()),
            _ => {}
        }
    }
    notices
}
