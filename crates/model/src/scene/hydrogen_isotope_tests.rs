use super::*;
use crate::style::HydrogenIsotopes;

#[test]
fn hydrogen_isotope_symbols_follow_the_style_and_connected_fragment() {
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let d = doc.add_atom("H", Point::new(42., 0.));
    doc.atom_mut(d).unwrap().isotope = 2;
    doc.add_bond(c, d, 1, "plain");
    for position in [
        Point::new(-42., 0.),
        Point::new(0., 42.),
        Point::new(0., -42.),
    ] {
        let cl = doc.add_atom("Cl", position);
        doc.add_bond(c, cl, 1, "plain");
    }
    // An isotope in a separate molecule cannot change this fragment's labels.
    let separate = doc.add_atom("C", Point::new(300., 0.));
    doc.atom_mut(separate).unwrap().isotope = 13;
    let labels = |doc: &Document, id| {
        atom_label_runs(doc.atom(id).unwrap(), doc)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&doc, d), ["D"]);
    // A hydrogen bond to the labelled molecule does not join the molecules.
    doc.add_bond(d, separate, 0, "plain");
    assert_eq!(labels(&doc, d), ["D"]);
    let paint = crate::rear_opacity::Paint::new(&doc);
    assert!(
        collect_atom_labels(&doc, &paint).runs[&d]
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "D"))
    );
    doc.atom_mut(c).unwrap().isotope = 13;
    assert_eq!(labels(&doc, d), ["H", "2"]);
    doc.drawing_style.hydrogen_isotopes = HydrogenIsotopes::Letters;
    assert_eq!(labels(&doc, d), ["D"]);
    doc.drawing_style.hydrogen_isotopes = HydrogenIsotopes::MassNumbers;
    doc.atom_mut(c).unwrap().isotope = 0;
    assert_eq!(labels(&doc, d), ["H", "2"]);
    doc.drawing_style.hydrogen_isotopes = HydrogenIsotopes::Auto;
    let t = doc.add_atom("H", Point::new(84., 0.));
    doc.atom_mut(t).unwrap().isotope = 3;
    doc.add_bond(d, t, 1, "plain");
    assert_eq!(labels(&doc, d), ["D"]);
    assert_eq!(labels(&doc, t), ["T"]);
    let h = doc.add_atom("H", Point::new(126., 0.));
    doc.atom_mut(h).unwrap().isotope = 1;
    doc.add_bond(t, h, 1, "plain");
    assert_eq!(labels(&doc, d), ["H", "2"]);
    assert_eq!(labels(&doc, t), ["H", "3"]);
}

#[test]
fn hydrogen_isotope_letters_use_normal_label_size_and_export_without_mutating_atoms() {
    for (mass, symbol) in [(2, "D"), (3, "T")] {
        let mut doc = Document::default();
        let id = doc.add_atom("H", Point::new(20., 30.));
        let style = crate::typography::TextStyle {
            size_pt: 12.,
            color: crate::palette::Color::Custom([80, 20, 10]),
            ..doc.drawing_style.text_style()
        };
        let atom = doc.atom_mut(id).unwrap();
        atom.isotope = mass;
        atom.text_style = Some(style.clone());

        for mode in HydrogenIsotopes::ALL {
            doc.drawing_style.hydrogen_isotopes = mode;
            let before = serde_json::to_vec(&doc).unwrap();
            let runs = atom_label_runs(doc.atom(id).unwrap(), &doc);
            let text: Vec<_> = runs
                .iter()
                .filter_map(|run| match run {
                    Primitive::Text {
                        text,
                        size,
                        position,
                        color,
                        style,
                    } => Some((text.as_str(), *size, *position, *color, style)),
                    _ => None,
                })
                .collect();
            if mode == HydrogenIsotopes::MassNumbers {
                assert_eq!(text.len(), 2);
                assert_eq!(text[0].0, "H");
                assert_eq!(text[1].0, mass.to_string());
                assert_eq!(text[0].1, style.size());
                assert_eq!(text[1].1, style.size() * 0.7);
                assert!(text[1].2.y < text[0].2.y);
            } else {
                assert_eq!(text.len(), 1);
                assert_eq!(text[0].0, symbol);
                assert_eq!(text[0].1, style.size());
            }
            assert!(
                text.iter()
                    .all(|run| run.3 == style.color.rgb() && run.4 == &style)
            );

            let xml = svg(&doc);
            let tree = roxmltree::Document::parse(&xml).unwrap();
            let exported: Vec<_> = tree
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .filter_map(|node| node.text())
                .collect();
            let expected = if mode == HydrogenIsotopes::MassNumbers {
                vec!["H".to_owned(), mass.to_string()]
            } else {
                vec![symbol.to_owned()]
            };
            assert_eq!(exported, expected);
            assert_eq!(serde_json::to_vec(&doc).unwrap(), before);
        }
    }
}

#[test]
fn auto_considers_isotopes_inside_contracted_groups_and_only_aliases_hydrogen_two_and_three() {
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let hidden = doc.add_atom("C", Point::new(-42., 0.));
    let d = doc.add_atom("H", Point::new(42., 0.));
    doc.add_bond(c, hidden, 1, "plain");
    doc.add_bond(c, d, 1, "plain");
    doc.atom_mut(hidden).unwrap().isotope = 13;
    doc.atom_mut(d).unwrap().isotope = 2;
    doc.contract(&[c, hidden], "Et", "Et").unwrap();
    assert!(!doc.atom_visible(hidden));
    let texts = |doc: &Document, id| {
        atom_label_runs(doc.atom(id).unwrap(), doc)
            .into_iter()
            .filter_map(|run| match run {
                Primitive::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(&doc, d), ["H", "2"]);
    doc.drawing_style.hydrogen_isotopes = HydrogenIsotopes::Letters;
    assert_eq!(texts(&doc, d), ["D"]);
    doc.drawing_style.hydrogen_isotopes = HydrogenIsotopes::Auto;
    doc.atom_mut(hidden).unwrap().isotope = 0;
    assert_eq!(texts(&doc, d), ["D"]);

    for mode in HydrogenIsotopes::ALL {
        doc.drawing_style.hydrogen_isotopes = mode;
        for mass in [0, 1, 4] {
            doc.atom_mut(d).unwrap().isotope = mass;
            let expected = if mass == 0 {
                vec!["H".to_owned()]
            } else {
                vec!["H".to_owned(), mass.to_string()]
            };
            assert_eq!(texts(&doc, d), expected);
        }
    }
}
