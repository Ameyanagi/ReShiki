use super::*;
use crate::{
    atom_labels::{self, Settings, refresh::Refresh},
    document::Point,
    fischer::{self, Compound},
};

fn refresh(doc: &mut Document) {
    let refresh = Refresh::calculate(doc, &Refresh::default()).unwrap();
    assert!(refresh.notice.is_none(), "{:?}", refresh.notice);
    refresh.apply(doc);
}

fn alanine() -> (Document, fischer::Cross) {
    let doc = fischer::document(Compound::LAlanine, 42.).unwrap();
    let cross = fischer::recognize(&doc).crosses[0];
    (doc, cross)
}

fn enabled(doc: &mut Document) {
    doc.atom_labels.stereo = true;
    doc.atom_labels.dl_descriptors = true;
    refresh(doc);
}

#[test]
fn dl_descriptors_defaults_and_serialization_are_opt_in() {
    let old = serde_json::json!({"carbons":"skeletal", "hydrogens":true, "stereo":false});
    let settings: Settings = serde_json::from_value(old.clone()).unwrap();
    assert!(!settings.dl_descriptors);
    assert_eq!(serde_json::to_value(&settings).unwrap(), old);
    let mut settings = settings;
    settings.dl_descriptors = true;
    let saved = serde_json::to_value(&settings).unwrap();
    assert_eq!(saved["dl_descriptors"], true);
    assert_eq!(serde_json::from_value::<Settings>(saved).unwrap(), settings);
}

#[test]
fn dl_descriptors_amino_acids_follow_priorities_and_not_a_fixed_rs_mapping() {
    for (side_element, expected_cip) in [(None, "S"), (Some("O"), "S"), (Some("S"), "R")] {
        let (mut doc, cross) = alanine();
        if let Some(element) = side_element {
            doc.abbreviations.clear();
            let branch = doc.add_atom(element, Point::new(42., 100.));
            doc.add_bond(cross.down, branch, 1, "plain");
            doc.atom_mut(cross.center).unwrap().stereo = Some(cross.reading());
        }
        enabled(&mut doc);
        assert_eq!(
            doc.atom(cross.center).unwrap().cip_label.as_deref(),
            Some(expected_cip)
        );
        assert_eq!(
            descriptors(&doc).get(&cross.center),
            Some(&"L"),
            "{side_element:?}"
        );
        let stereo = doc.atom_mut(cross.center).unwrap().stereo.as_mut().unwrap();
        stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
        refresh(&mut doc);
        assert_eq!(
            descriptors(&doc).get(&cross.center),
            Some(&"D"),
            "{side_element:?}"
        );
    }
}

#[test]
fn dl_descriptors_amino_acid_center_accepts_implicit_or_counted_hydrogen() {
    for counted in [false, true] {
        let (mut doc, cross) = alanine();
        doc.atoms.retain(|a| a.id != cross.right);
        doc.bonds
            .retain(|b| b.a != cross.right && b.b != cross.right);
        let center = doc.atom_mut(cross.center).unwrap();
        center.no_implicit = counted;
        center.explicit_h = u32::from(counted);
        // Move the former horizontal H to the implicit fourth position.
        center.stereo = Some(AtomStereo {
            winding: "ccw".into(),
            neighbors: vec![cross.up, cross.down, cross.left],
        });
        enabled(&mut doc);
        assert_eq!(
            doc.atom(cross.center).unwrap().cip_label.as_deref(),
            Some("S")
        );
        assert_eq!(descriptors(&doc).get(&cross.center), Some(&"L"));
    }
}

#[test]
fn dl_descriptors_carbonyl_detection_accepts_aldehydes() {
    for explicit_h in [false, true] {
        let (mut doc, cross) = alanine();
        doc.abbreviations.clear();
        let hydroxyl = neighbors(&doc, cross.up)
            .into_iter()
            .find(|(a, order)| a.element == "O" && *order == 1)
            .unwrap()
            .0
            .id;
        doc.atoms.retain(|a| a.id != hydroxyl);
        doc.bonds.retain(|b| b.a != hydroxyl && b.b != hydroxyl);
        if explicit_h {
            let hydrogen = doc.add_atom("H", Point::new(-42., -84.));
            doc.add_bond(cross.up, hydrogen, 1, "plain");
        }
        doc.atom_mut(cross.center).unwrap().stereo = Some(cross.reading());
        enabled(&mut doc);
        assert_eq!(
            doc.atom(cross.center).unwrap().cip_label.as_deref(),
            Some("S")
        );
        assert_eq!(descriptors(&doc).get(&cross.center), Some(&"L"));
    }
}

#[test]
fn dl_descriptors_skip_alpha_amino_ketones() {
    let (mut doc, cross) = alanine();
    doc.abbreviations.clear();
    let hydroxyl = neighbors(&doc, cross.up)
        .into_iter()
        .find(|(a, order)| a.element == "O" && *order == 1)
        .unwrap()
        .0
        .id;
    doc.atom_mut(hydroxyl).unwrap().element = "C".into();
    doc.atom_mut(hydroxyl).unwrap().label_h = 0;
    doc.atom_mut(cross.center).unwrap().stereo = Some(cross.reading());
    enabled(&mut doc);
    assert!(doc.atom(cross.center).unwrap().cip_label.is_some());
    assert!(descriptors(&doc).is_empty());
}

#[test]
fn dl_descriptors_both_amino_acid_residues_in_a_dipeptide_are_named() {
    let (mut doc, first) = alanine();
    doc.abbreviations.clear();
    let leaving = neighbors(&doc, first.up)
        .into_iter()
        .find(|(a, order)| a.element == "O" && *order == 1)
        .unwrap()
        .0
        .id;
    doc.atoms.retain(|a| a.id != leaving);
    doc.bonds.retain(|b| b.a != leaving && b.b != leaving);
    let second_n = doc.add_atom("N", Point::new(100., -60.));
    let second = doc.add_atom("C", Point::new(142., -60.));
    let acid = doc.add_atom("C", Point::new(142., -123.));
    let oxygen = doc.add_atom("O", Point::new(180., -144.));
    let hydroxyl = doc.add_atom("O", Point::new(104., -144.));
    let methyl = doc.add_atom("C", Point::new(142., 3.));
    let hydrogen = doc.add_atom("H", Point::new(184., -60.));
    for (a, b, order) in [
        (first.up, second_n, 1),
        (second_n, second, 1),
        (second, acid, 1),
        (acid, oxygen, 2),
        (acid, hydroxyl, 1),
        (second, methyl, 1),
        (second, hydrogen, 1),
    ] {
        doc.add_bond(a, b, order, "plain");
    }
    // Adding the amide bond invalidates the old carbonyl neighbor's cache.
    doc.atom_mut(first.center).unwrap().stereo = Some(first.reading());
    doc.atom_mut(second).unwrap().stereo = Some(fischer::reading(acid, hydrogen, methyl, second_n));
    enabled(&mut doc);
    assert_eq!(
        descriptors(&doc),
        HashMap::from([(first.center, "L"), (second, "L")])
    );
}

#[test]
fn dl_descriptors_omit_glycine_unrelated_centres_and_sphere_two_ties() {
    let (mut glycine, cross) = alanine();
    glycine.abbreviations.clear();
    glycine.atom_mut(cross.down).unwrap().element = "H".into();
    enabled(&mut glycine);
    assert!(descriptors(&glycine).is_empty());

    let (mut unrelated, cross) = alanine();
    unrelated.atom_mut(cross.left).unwrap().element = "O".into();
    enabled(&mut unrelated);
    assert!(descriptors(&unrelated).is_empty());
    assert!(
        atom_labels::indicators(&unrelated)
            .iter()
            .any(|i| i.text == "(S)")
    );

    let (mut tied, cross) = alanine();
    tied.abbreviations.clear();
    // C(OH)3 and C(=O)OH both have [O,O,O] at sphere two. The R group
    // remains chemically distinct at greater depth, beyond this recognizer.
    for x in [-42., 0., 42.] {
        let oxygen = tied.add_atom("O", Point::new(x, 120.));
        tied.add_bond(cross.down, oxygen, 1, "plain");
    }
    tied.atom_mut(cross.center).unwrap().stereo = Some(cross.reading());
    enabled(&mut tied);
    assert!(tied.atom(cross.center).unwrap().cip_label.is_some());
    assert!(descriptors(&tied).is_empty());
}

#[test]
fn dl_descriptors_are_optional_and_follow_indicator_visibility() {
    let (mut doc, cross) = alanine();
    enabled(&mut doc);
    assert!(
        atom_labels::indicators(&doc)
            .iter()
            .any(|i| i.text == "L (S)")
    );
    doc.atom_labels.dl_descriptors = false;
    assert!(
        atom_labels::indicators(&doc)
            .iter()
            .any(|i| i.text == "(S)")
    );
    doc.atom_labels.dl_descriptors = true;
    doc.atom_labels.stereo = false;
    assert!(atom_labels::indicators(&doc).is_empty());
    doc.atom_mut(cross.center).unwrap().display.stereo.show = Some(true);
    assert!(
        atom_labels::indicators(&doc)
            .iter()
            .any(|i| i.text == "L (S)")
    );
}

#[test]
fn dl_descriptors_haworth_templates_name_only_the_configurational_carbon() {
    let mut checked = 0;
    for template in crate::haworth::templates().unwrap() {
        let mut doc = template.document;
        if !doc.atoms.iter().any(|a| a.stereo.is_some()) {
            continue;
        }
        let expected = if template.name.contains("-L-") {
            "L"
        } else {
            "D"
        };
        enabled(&mut doc);
        let names = descriptors(&doc);
        assert_eq!(names.len(), 1, "{}: {names:?}", template.name);
        assert_eq!(
            *names.values().next().unwrap(),
            expected,
            "{}",
            template.name
        );
        let center = *names.keys().next().unwrap();
        // Stored parity, including neighbor permutations and inversions, is
        // authoritative even when the original projection ink is retained.
        let stereo = doc.atom_mut(center).unwrap().stereo.as_mut().unwrap();
        stereo.neighbors.swap(0, 1);
        stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
        assert_eq!(descriptors(&doc), names, "{}", template.name);
        let stereo = doc.atom_mut(center).unwrap().stereo.as_mut().unwrap();
        stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
        assert_eq!(
            descriptors(&doc).get(&center),
            Some(&if expected == "D" { "L" } else { "D" }),
            "{}",
            template.name
        );
        checked += 1;
    }
    assert_eq!(checked, 48);
}

#[test]
fn dl_descriptors_fischer_aldoses_use_lowest_center_and_stored_parity() {
    for (compound, expected) in [
        (Compound::DGlyceraldehyde, "D"),
        (Compound::LGlyceraldehyde, "L"),
        (Compound::DGlucose, "D"),
        (Compound::DRibose, "D"),
    ] {
        let mut doc = fischer::document(compound, 42.).unwrap();
        enabled(&mut doc);
        let names = descriptors(&doc);
        assert_eq!(names.len(), 1, "{compound:?}");
        let (center, name) = names.into_iter().next().unwrap();
        assert_eq!(name, expected, "{compound:?}");
        // A quarter turn draws the enantiomer; the name follows the chemistry
        // instead of disappearing with the conventional orientation.
        let mut turned = doc.clone();
        let ids = turned.all_ids();
        crate::editing::transform(&mut turned, &ids, crate::editing::Transform::Rotate(90.));
        assert!(crate::transaction::reconcile(&mut turned, doc.clone()).is_ok());
        refresh(&mut turned);
        assert_eq!(
            descriptors(&turned).get(&center),
            Some(&if expected == "D" { "L" } else { "D" }),
            "{compound:?}, quarter turn"
        );
        for (mirror_x, mirror_y) in [(true, false), (true, true)] {
            let mut transformed = doc.clone();
            for atom in &mut transformed.atoms {
                if mirror_x {
                    atom.position.x = -atom.position.x;
                }
                if mirror_y {
                    atom.position.y = -atom.position.y;
                }
            }
            assert_eq!(
                descriptors(&transformed).get(&center),
                Some(&expected),
                "{compound:?}, mirror x={mirror_x}, y={mirror_y}"
            );
        }
        let crosses = fischer::recognize(&doc).crosses;
        assert!(
            crosses
                .iter()
                .all(|cross| doc.atom(cross.center).unwrap().position.y
                    <= doc.atom(center).unwrap().position.y)
        );
        let stereo = doc.atom_mut(center).unwrap().stereo.as_mut().unwrap();
        stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
        assert_eq!(
            descriptors(&doc).get(&center),
            Some(&if expected == "D" { "L" } else { "D" })
        );
    }
}

#[test]
fn dl_descriptors_prefix_and_cip_format_runs_respect_roman_parentheses() {
    for (text, roman, expected) in [
        ("L (S)", false, vec![("L ", false), ("(S)", true)]),
        (
            "D (R)",
            true,
            vec![("D (", false), ("R", true), (")", false)],
        ),
        ("(S)", false, vec![("(S)", true)]),
        ("(R)", true, vec![("(", false), ("R", true), (")", false)]),
    ] {
        let format = atom_labels::indicator_format(text, &atom_labels::stereo_style(), roman);
        let runs: Vec<_> = atom_labels::indicator_runs(text, &format)
            .map(|(text, style)| (text, style.italic))
            .collect();
        assert_eq!(runs, expected);
    }
}

#[test]
fn dl_descriptors_do_not_restyle_atom_numbers_that_resemble_stereo_text() {
    for italic in [false, true] {
        let (mut doc, cross) = alanine();
        enabled(&mut doc);
        let mut style = atom_labels::number_style();
        style.italic = italic;
        doc.atom_mut(cross.center).unwrap().display.number = Some(atom_labels::Number {
            text: "L (S)".into(),
            offset: None,
            style,
        });
        let indicator = atom_labels::indicators(&doc)
            .into_iter()
            .find(|i| i.owner == atom_labels::Owner::Number(cross.center))
            .unwrap();
        let primitives = indicator.primitives();
        let [crate::scene::Primitive::Text { text, style, .. }] = primitives.as_slice() else {
            panic!("atom number must retain one style")
        };
        assert_eq!(text, "L (S)");
        assert_eq!(style.italic, italic);
    }
}
