use super::*;
use crate::{atom_labels::Carbons, document::Document};

#[test]
fn indexed_contrast_matches_label_visibility_for_themes_and_carbon_modes() {
    let mut doc: Document = serde_json::from_str(include_str!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    for (index, atom) in doc.atoms.iter_mut().enumerate() {
        if index % 3 == 0 {
            atom.display.color_override = true;
            atom.text_style = Some(crate::typography::TextStyle {
                color: Color::Custom([235; 3]),
                ..Default::default()
            });
        }
    }
    let isolated = doc.add_atom("C", Default::default());
    doc.atom_mut(isolated).unwrap().display.color_override = true;
    doc.atom_mut(isolated).unwrap().text_style = Some(crate::typography::TextStyle {
        color: Color::Custom([235; 3]),
        ..Default::default()
    });
    for canvas in CanvasTheme::ALL {
        for carbons in Carbons::ALL {
            doc.canvas_theme = canvas;
            doc.atom_labels.carbons = carbons;
            let before = doc.clone();
            let expected: Vec<_> = doc
                .atoms
                .iter()
                .filter(|atom| {
                    // Compare the indexed check with the ordinary visibility
                    // path: wrappers remain visible in skeletal mode, while
                    // hidden members do not contribute contrast warnings.
                    doc.atom_visible(atom.id)
                        && !crate::attachments::hidden(atom, &doc)
                        && (doc.abbreviation(atom.id).is_some()
                            || crate::atom_labels::visible(atom, &doc))
                        && !crate::color_contrast::meets(
                            doc.canvas_theme.color(atom_color(&doc, atom)),
                            &label_backgrounds(&doc, &Palette::of(&doc), atom),
                            crate::color_contrast::TEXT_MIN,
                        )
                })
                .map(|atom| atom.id)
                .collect();
            assert_eq!(
                label_contrast_issues(&doc),
                expected,
                "{canvas:?} canvas, {carbons:?} carbon labels",
            );
            assert_eq!(doc, before);
        }
    }
}
