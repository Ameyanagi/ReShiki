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

// Routes differ on purpose until q-36 makes them consistent; keep each list exact.
pub(crate) fn notices(document: &Document, format: &str, route: Route<'_>) -> Vec<String> {
    match route {
        Route::AttachmentEarlyExport => {
            let mut notices = vec![crate::attachments::ANALYSIS_NOTICE.into()];
            if format == "mol" && crate::rear_opacity::present(document) {
                notices.push(crate::rear_opacity::EXPORT_NOTICE.into());
            }
            notices
        }
        Route::UnpreparedFallback => {
            // This CDXML/CDX fallback uses the same notices regardless of format.
            std::iter::once(UNVALIDATED_NOTICE.into())
                .chain(crate::arrow_anchors::export_notice(document))
                .collect()
        }
        Route::Prepared {
            source,
            figure_only,
        } => {
            let mut notices = Vec::new();
            if !figure_only && !document.reactions.is_empty() {
                notices.push(REACTION_ROLES_NOTICE.into());
            }
            if matches!(format, "cdxml" | "cdx")
                && let Some(notice) = crate::arrow_anchors::export_notice(document)
            {
                notices.push(notice);
            }
            // Preparation can change appearance, so rear opacity reads the source document.
            if matches!(format, "mol" | "smiles" | "inchi") && crate::rear_opacity::present(source)
            {
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
