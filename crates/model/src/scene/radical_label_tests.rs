use super::*;

#[test]
fn automatic_radical_ions_put_dots_before_the_charge_by_default() {
    for charge in [-2, -1, 1, 2] {
        for radicals in [1, 2] {
            for charge_first in [false, true] {
                let mut doc = Document::default();
                let id = doc.add_atom("N", Point::default());
                let atom = doc.atom_mut(id).unwrap();
                atom.charge = charge;
                atom.radical_electrons = radicals;
                doc.drawing_style.charge_before_radical = charge_first;
                let before = serde_json::to_vec(&doc).unwrap();
                let runs = atom_label_runs(doc.atom(id).unwrap(), &doc);
                let texts: Vec<_> = runs
                    .iter()
                    .filter_map(|run| match run {
                        Primitive::Text {
                            text,
                            position,
                            size,
                            style,
                            ..
                        } => Some((text.as_str(), *position, *size, style)),
                        _ => None,
                    })
                    .collect();
                let radical = if radicals == 1 {
                    "•".into()
                } else {
                    format!("{radicals}•")
                };
                let sign = if charge > 0 { "+" } else { "−" };
                let charge = if charge.unsigned_abs() > 1 {
                    format!("{}{sign}", charge.unsigned_abs())
                } else {
                    sign.into()
                };
                let suffix = if charge_first {
                    [charge.as_str(), radical.as_str()]
                } else {
                    [radical.as_str(), charge.as_str()]
                };
                assert_eq!(
                    texts.iter().map(|run| run.0).collect::<Vec<_>>(),
                    ["N", suffix[0], suffix[1]]
                );
                let (_, first, size, style) = texts[1];
                let (_, second, _, _) = texts[2];
                assert_eq!(first.y, second.y);
                let width = crate::style::styled_text_width(suffix[0], size, style);
                assert!((second.x - first.x - width).abs() < 0.001);
                let svg = svg(&doc);
                let tree = roxmltree::Document::parse(&svg).unwrap();
                let exported: Vec<_> = tree
                    .descendants()
                    .filter(|node| node.has_tag_name("text"))
                    .filter_map(|node| node.text())
                    .collect();
                assert_eq!(exported, ["N", suffix[0], suffix[1]]);
                assert_eq!(serde_json::to_vec(&doc).unwrap(), before);
            }
        }
    }
}

#[test]
fn neutral_radicals_keep_their_superscript_slot_without_charge_placement() {
    for radicals in [1, 2] {
        let mut doc = Document::default();
        let n = doc.add_atom("N", Point::default());
        let c = doc.add_atom("C", Point::new(36., -21.));
        doc.add_bond(n, c, 1, "plain");
        doc.atom_mut(n).unwrap().radical_electrons = radicals;
        let atom = doc.atom(n).unwrap();
        let runs = atom_label_runs(atom, &doc);
        let unplaced = atom_label_runs_with_charge_layout(atom, &doc, ChargeLayout::Unplaced);
        assert_eq!(format!("{runs:?}"), format!("{unplaced:?}"));
        assert!(runs.iter().any(|run| matches!(run,
            Primitive::Text { text, .. } if text == if radicals == 1 { "•" } else { "2•" }
        )));
    }
}
