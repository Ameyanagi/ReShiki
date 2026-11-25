//! Ordered export loss notices for each native response route.

use crate::document::Document;

pub(crate) const DATIVE_MOL_NOTICE: &str = "MOL retains directed dative chemistry and metal charge, but omits solid/hashed coordination projection appearance. Use native ReShiki or figure export to retain that drawing.";
pub(crate) const DATIVE_CDX_NOTICE: &str = "CDXML/CDX retains directed coordination chemistry and ReShiki projection attributes. ChemDraw 26 displays dative arrows and removes wedge/hash paint on save. Use native ReShiki or figure export for faithful projection appearance.";
pub(crate) const REACTION_ROLES_NOTICE: &str = "This drawing or molecule format does not retain reaction roles. Use RXN/reaction SMILES for reaction data, or .rsk for the complete scheme.";
pub(crate) const UNVALIDATED_NOTICE: &str =
    "Drawing exported; chemical assignments remain unvalidated.";
pub(crate) const CRYSTAL_DRAWING_NOTICE: &str = "CDXML/CDX exports the 2D atoms and bonds as an unvalidated drawing, without the unit cell, periodic copies or distance-derived bond provenance. Reimported bonds may be interpreted as chemical bonds. Use native ReShiki to retain that information.";
pub(crate) const DISTANCE_DRAWING_NOTICE: &str = "CDXML/CDX exports the atoms and bonds as an unvalidated drawing, without distance-derived bond provenance. Reimported bonds may be interpreted as chemical bonds. Use native ReShiki to retain that information.";

pub(crate) enum Route<'a> {
    AttachmentEarlyExport,
    UnpreparedFallback,
    DistanceDrawing,
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
        Route::DistanceDrawing => {
            let mut notices = vec![crate::models3d::crystal::analysis_notice(document).into()];
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
    if matches!(format, "cdxml" | "cdx") && crate::models3d::crystal::touches(appearance, &[]) {
        notices.push(
            if crate::models3d::crystal::present(appearance) {
                CRYSTAL_DRAWING_NOTICE
            } else {
                DISTANCE_DRAWING_NOTICE
            }
            .into(),
        );
    }
    if roles
        && if matches!(format, "cdxml" | "cdx") {
            crate::exchange::drawing::has_unwritten_reactions(document)
        } else {
            !document.reactions.is_empty()
        }
    {
        notices.push(REACTION_ROLES_NOTICE.into());
    }
    // MOL and identifiers omit arrows, so the link-loss notice applies only to CDXML/CDX.
    if matches!(format, "cdxml" | "cdx")
        && let Some(notice) = crate::arrow_anchors::export_notice(document)
    {
        notices.push(notice);
    }
    if matches!(format, "cdxml" | "cdx")
        && let Some(notice) = crate::arrow_labels::export_notice(document)
    {
        notices.push(notice);
    }
    if matches!(format, "cdxml" | "cdx")
        && let Some(notice) = crate::shape_text::export_notice(document)
    {
        notices.push(notice);
    }
    if matches!(format, "cdxml" | "cdx")
        && let Some(notice) = crate::arrows::export_notice(document)
    {
        notices.push(notice);
    }
    if matches!(format, "cdxml" | "cdx") {
        notices.extend(crate::graphics::export_notice(appearance));
        notices.extend(chemdraw_lacks(appearance, format));
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

/// What ChemDraw has no place for: panels and object purpose. ReShiki keeps
/// them in CDXML for itself; CDX drops them.
fn chemdraw_lacks(document: &Document, format: &str) -> Option<String> {
    use crate::purpose::Purpose;
    let panels = document
        .graphics
        .iter()
        .filter(|g| g.panel.is_some())
        .count();
    // Arrows ChemDraw gets as shapes have their own notice.
    let document = crate::arrows::lowered(document);
    let purposes: Vec<_> = [Purpose::Reference, Purpose::Illustration]
        .into_iter()
        .filter(|p| {
            document.atoms.iter().any(|a| a.purpose == *p)
                // An arrow through nodes opens again as a path.
                || document
                    .arrows
                    .iter()
                    .any(|a| a.purpose == *p && a.nodes.is_empty())
        })
        .map(Purpose::label)
        .collect();
    let mut lacks = Vec::new();
    let mut opens = Vec::new();
    if panels > 0 {
        lacks.push("panels");
        opens.push(match panels {
            1 => "1 panel as grouped shapes and text".to_owned(),
            n => format!("{n} panels as grouped shapes and text"),
        });
    }
    if !purposes.is_empty() {
        lacks.push("object purpose");
        opens.push(format!("{} objects as Chemistry", purposes.join(" and ")));
    }
    if lacks.is_empty() {
        return None;
    }
    let keep = if format == "cdxml" {
        "ReShiki reads them back from this CDXML file."
    } else {
        "Save a .rsk file to keep them."
    };
    Some(format!(
        "ChemDraw has no {}. It opens {}. {keep}",
        lacks.join(" or "),
        opens.join(", and ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        arrow_labels::{ArrowLabel, EXPORT_NOTICE, Side},
        arrows::{ArrowStyle, Body, Preset},
        document::{Annotation, Arrow, Point},
        reactions::{Participant, Reaction},
    };

    fn add_reaction(document: &mut Document, body: Body, curved: bool) {
        let y = document.arrows.len() as f32 * 100.;
        let reactant = document.add_atom("C", Point::new(-40., y));
        let product = document.add_atom("O", Point::new(160., y));
        let mut style = ArrowStyle::preset(Preset::Forward);
        style.set_body(body);
        let arrow = document.next_id();
        let mut drawing = Arrow::new(
            arrow,
            Point::new(0., y),
            Point::new(120., y),
            Preset::Forward,
            style,
        );
        if curved {
            drawing.control = Some(Point::new(60., y + 40.));
        }
        document.arrows.push(drawing);
        let mut reaction = Reaction::new(arrow);
        reaction.reactants.push(Participant {
            atoms: vec![reactant],
            coefficient: 1,
        });
        reaction.products.push(Participant {
            atoms: vec![product],
            coefficient: 1,
        });
        document.reactions.push(reaction);
    }

    #[test]
    fn editable_exchange_reports_only_roles_lost_with_lowered_reaction_arrows() {
        for (body, curved, lost) in [
            (Body::Line, false, false),
            (Body::Block, false, false),
            (Body::Block, true, true),
            (Body::Chevron, false, true),
        ] {
            let mut doc = Document::default();
            add_reaction(&mut doc, body, curved);
            doc.validate().unwrap();
            for format in ["cdxml", "cdx"] {
                assert_eq!(
                    notices(&doc, format, Route::UnpreparedFallback)
                        .iter()
                        .any(|notice| notice == REACTION_ROLES_NOTICE),
                    lost,
                    "{format}: {body:?}, curved={curved}"
                );
            }
            assert!(
                notices(
                    &doc,
                    "mol",
                    Route::Prepared {
                        source: &doc,
                        figure_only: false,
                    }
                )
                .iter()
                .any(|notice| notice == REACTION_ROLES_NOTICE)
            );
        }
    }

    #[test]
    fn only_lowered_arrows_with_explicit_roles_add_the_notice() {
        let mut doc = Document::default();
        add_reaction(&mut doc, Body::Line, false);
        add_reaction(&mut doc, Body::Chevron, false);
        doc.validate().unwrap();
        let count = |doc: &Document| {
            notices(doc, "cdxml", Route::AttachmentEarlyExport)
                .iter()
                .filter(|notice| notice.as_str() == REACTION_ROLES_NOTICE)
                .count()
        };
        assert_eq!(count(&doc), 1);
        doc.reactions.pop();
        assert_eq!(count(&doc), 0);
    }

    #[test]
    fn editable_exchange_reports_incomplete_and_overlapping_reaction_roles() {
        let mut doc = Document::default();
        add_reaction(&mut doc, Body::Line, false);
        let reactant = doc.reactions[0].reactants[0].atoms[0];
        let product = doc.reactions[0].products[0].atoms[0];
        let mut no_reactants = doc.clone();
        no_reactants.reactions[0].reactants.clear();
        let mut no_products = doc.clone();
        no_products.reactions[0].products.clear();
        let hydrogen = doc.add_atom("H", Point::new(-20., 0.));
        doc.add_bond(reactant, hydrogen, 1, "plain");
        doc.add_bond(hydrogen, product, 0, "dotted");
        doc.reactions[0].reactants[0].atoms.push(hydrogen);
        for doc in [no_reactants, no_products, doc] {
            doc.validate().unwrap();
            let xml = crate::exchange::drawing::write(&doc, Default::default()).unwrap();
            assert!(!xml.contains("<step "));
            for format in ["cdxml", "cdx"] {
                assert!(
                    notices(&doc, format, Route::UnpreparedFallback)
                        .iter()
                        .any(|notice| notice == REACTION_ROLES_NOTICE),
                    "{format}"
                );
            }
        }
    }

    #[test]
    fn editable_exchange_reports_omitted_arrow_condition_links() {
        let mut doc = Document::default();
        doc.arrows.push(Arrow::new(
            1,
            Point::new(0., 0.),
            Point::new(120., 0.),
            Preset::Forward,
            ArrowStyle::preset(Preset::Forward),
        ));
        doc.annotations.push(Annotation {
            angle: 0.,
            id: 2,
            position: Point::new(40., -40.),
            text: "heat".into(),
            format: Default::default(),
        });
        doc.arrow_labels.push(ArrowLabel {
            arrow: 1,
            annotation: 2,
            side: Side::Above,
            gap: None,
        });
        doc.validate().unwrap();
        let route = || Route::Prepared {
            source: &doc,
            figure_only: false,
        };
        for format in ["cdxml", "cdx"] {
            assert!(notices(&doc, format, route()).contains(&EXPORT_NOTICE.to_owned()));
        }
        assert!(!notices(&doc, "mol", route()).contains(&EXPORT_NOTICE.to_owned()));
        // The writer keeps the text and the arrow.
        let xml = crate::exchange::drawing::write(&doc, Default::default()).unwrap();
        assert!(xml.contains("<arrow") && xml.contains("heat"));
    }

    #[test]
    fn chemdraw_exports_name_panels_and_purpose() {
        use crate::{
            graphics::{Graphic, GraphicKind, Panel},
            purpose::Purpose,
        };
        let mut doc = Document::default();
        let mut arrow = Arrow::new(
            1,
            Point::new(0., 0.),
            Point::new(120., 0.),
            Preset::Forward,
            ArrowStyle::preset(Preset::Forward),
        );
        arrow.purpose = Purpose::Reference;
        doc.arrows.push(arrow);
        let mut panel = Graphic::dragged(
            2,
            GraphicKind::RoundedRectangle,
            Point::new(0., 0.),
            Point::new(200., 100.),
            Default::default(),
            Default::default(),
            false,
        );
        panel.panel = Some(Panel::default());
        doc.graphics.push(panel);
        let route = || Route::Prepared {
            source: &doc,
            figure_only: false,
        };
        let lacks = "ChemDraw has no panels or object purpose. It opens 1 panel as grouped shapes and text, and Reference objects as Chemistry.";
        assert!(notices(&doc, "cdxml", route()).contains(&format!(
            "{lacks} ReShiki reads them back from this CDXML file."
        )));
        assert!(
            notices(&doc, "cdx", route())
                .contains(&format!("{lacks} Save a .rsk file to keep them."))
        );
        assert!(
            !notices(&doc, "mol", route())
                .iter()
                .any(|n| n.starts_with("ChemDraw has no panels"))
        );
    }
}
