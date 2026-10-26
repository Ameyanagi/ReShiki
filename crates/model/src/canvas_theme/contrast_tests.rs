use super::*;
use crate::{atom_labels::Carbons, document::Document};

fn ordinary_visibility_issues(doc: &Document) -> Vec<u64> {
    doc.atoms
        .iter()
        .filter(|atom| {
            doc.atom_visible(atom.id)
                && !crate::attachments::hidden(atom, doc)
                && (doc.abbreviation(atom.id).is_some() || crate::atom_labels::visible(atom, doc))
                && !crate::color_contrast::meets(
                    doc.canvas_theme.color(atom_color(doc, atom)),
                    &label_backgrounds(doc, &Palette::of(doc), atom),
                    crate::color_contrast::TEXT_MIN,
                )
        })
        .map(|atom| atom.id)
        .collect()
}

fn explicit_ink(doc: &mut Document, id: u64, rgb: [u8; 3]) {
    let atom = doc.atom_mut(id).unwrap();
    atom.display.color_override = true;
    atom.text_style = Some(crate::typography::TextStyle {
        color: Color::Custom(rgb),
        ..Default::default()
    });
}

fn check_issues(doc: &Document, expected: &[u64]) {
    let before = doc.clone();
    assert_eq!(ordinary_visibility_issues(doc), expected);
    assert_eq!(label_contrast_issues(doc), expected);
    assert_eq!(*doc, before);
}

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
            let expected = ordinary_visibility_issues(&doc);
            assert_eq!(
                label_contrast_issues(&doc),
                expected,
                "{canvas:?} canvas, {carbons:?} carbon labels",
            );
            assert_eq!(doc, before);
        }
    }
}

#[test]
fn hidden_members_preserve_overlapping_anchor_semantics_and_follow_each_edit() {
    use crate::{abbreviations::Abbreviation, document::Point};
    let mut doc = Document::default();
    let [a, b, c, d] = std::array::from_fn(|i| doc.add_atom("O", Point::new(i as f32 * 42., 0.)));
    for id in [a, b, c, d] {
        explicit_ink(&mut doc, id, [235; 3]);
    }
    let group = |anchor, members| Abbreviation {
        label_style: Some(crate::typography::TextStyle {
            color: Color::Custom([235; 3]),
            ..Default::default()
        }),
        label_color_override: true,
        highlight: None,
        label: "Label".into(),
        reverse_label: String::new(),
        anchor,
        members,
        alignment: Default::default(),
    };
    // Even an anchor is hidden if it is a non-anchor member of another
    // group. Preserve the ordinary rule for overlapping/intermediate data.
    doc.abbreviations = vec![group(b, vec![b, c]), group(a, vec![a, b])];
    check_issues(&doc, &[a, d]);
    doc.abbreviations.reverse();
    check_issues(&doc, &[a, d]);
    doc.abbreviations.remove(0);
    check_issues(&doc, &[a, b, d]);
    doc.abbreviations[0].anchor = c;
    check_issues(&doc, &[a, c, d]);
    doc.abbreviations.clear();
    check_issues(&doc, &[a, b, c, d]);
    doc.atoms.reverse();
    check_issues(&doc, &[d, c, b, a]);
}

#[test]
fn visibility_index_retains_dummy_carbon_ring_fill_and_highlight_filters() {
    use crate::{document::Point, ring_fills::RingFill};
    let mut doc = Document::default();
    let a = doc.add_atom("O", Point::new(0., 0.));
    let b = doc.add_atom("N", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(21., 36.));
    for (from, to) in [(a, b), (b, c), (c, a)] {
        doc.add_bond(from, to, 1, "plain");
    }
    let anonymous = doc.add_atom("*", Point::new(100., 0.));
    let labeled = doc.add_atom("*", Point::new(150., 0.));
    doc.atom_mut(labeled)
        .unwrap()
        .display
        .set_variable(Some("R".into()));
    for id in [a, b, c] {
        explicit_ink(&mut doc, id, [0; 3]);
    }
    for id in [anonymous, labeled] {
        explicit_ink(&mut doc, id, [235; 3]);
    }
    doc.ring_fills.push(RingFill {
        atoms: vec![a, b, c],
        color: Color::Custom([0; 3]),
    });
    doc.atom_mut(a).unwrap().display.highlight = Some(Color::Custom([255; 3]));
    doc.atom_labels.carbons = Carbons::All;
    check_issues(&doc, &[b, c, labeled]);
    doc.atom_labels.carbons = Carbons::Skeletal;
    check_issues(&doc, &[b, labeled]);
    doc.atom_labels.carbons = Carbons::All;
    doc.atom_mut(a).unwrap().display.highlight = None;
    check_issues(&doc, &[a, b, c, labeled]);
    explicit_ink(&mut doc, a, [255; 3]);
    check_issues(&doc, &[a, b, c, labeled]);
    doc.atom_mut(a).unwrap().display.highlight = Some(Color::Custom([0; 3]));
    check_issues(&doc, &[b, c, labeled]);
    doc.atom_mut(a).unwrap().display.highlight = None;
    doc.ring_fills.clear();
    check_issues(&doc, &[a, labeled]);
    doc.atom_mut(a).unwrap().display.highlight = Some(Color::Custom([0; 3]));
    check_issues(&doc, &[labeled]);
}
