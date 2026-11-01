use super::*;
use crate::{abbreviations::Abbreviation, atom_labels::refresh::Refresh};

fn methoxide(metal: &str) -> (Document, u64, u64, u64) {
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::new(-42., 0.));
    let o = doc.add_atom("O", Point::default());
    let m = doc.add_atom(metal, Point::new(42., 0.));
    doc.add_bond(c, o, 1, "plain");
    doc.add_bond(o, m, 1, "plain");
    (doc, c, o, m)
}

#[test]
fn sodium_methoxide_becomes_separate_charged_ions_without_protonation() {
    let (mut doc, c, o, na) = methoxide("Na");
    doc.atom_mut(c).unwrap().label_h = 99;
    doc.atom_mut(o).unwrap().label_h = 99;
    doc.atom_mut(na).unwrap().label_h = 99;
    doc.atom_mut(o).unwrap().cip_label = Some("R".into());
    doc.atom_mut(o).unwrap().display.hide_charge = true;
    doc.atom_mut(na).unwrap().display.hide_charge = true;
    let c_position = doc.atom(c).unwrap().position;
    let o_position = doc.atom(o).unwrap().position;
    let metal_position = doc.atom(na).unwrap().position;
    assert_eq!(eligible_bonds(&doc, &[c, o, na]), vec![(na, o)]);
    assert_eq!(make_ionic(&mut doc, &[o, na]), Ok(1));
    assert_eq!(doc.bonds.len(), 1);
    assert!(doc.bond_between(c, o).is_some());
    assert!(doc.bond_between(o, na).is_none());
    assert_eq!(doc.atom(c).unwrap().position, c_position);
    assert_eq!(doc.atom(o).unwrap().position, o_position);
    assert_eq!(doc.atom(o).unwrap().charge, -1);
    assert_eq!(doc.atom(na).unwrap().charge, 1);
    assert!(!doc.atom(o).unwrap().display.hide_charge);
    assert!(!doc.atom(na).unwrap().display.hide_charge);
    assert_eq!(doc.atom(o).unwrap().cip_label, None);
    assert_eq!(doc.atom(c).unwrap().label_h, 3);
    assert_eq!(doc.atom(o).unwrap().label_h, 0);
    assert_eq!(doc.atom(na).unwrap().label_h, 0);
    assert_eq!(
        doc.atom(na).unwrap().position,
        metal_position.offset(doc.drawing_style.bond_length_world, 0.)
    );
    let before = doc.clone();
    assert!(make_ionic(&mut doc, &[o, na]).is_err());
    assert_eq!(doc, before);
}

#[test]
fn sodium_acetate_keeps_its_carbonyl_and_hydrogen_counts() {
    let mut doc = Document::default();
    let methyl = doc.add_atom("C", Point::new(-42., 0.));
    let carbonyl = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(0., -42.));
    let anion = doc.add_atom("O", Point::new(42., 0.));
    let sodium = doc.add_atom("Na", Point::new(84., 0.));
    doc.add_bond(methyl, carbonyl, 1, "plain");
    doc.add_bond(carbonyl, oxygen, 2, "plain");
    doc.add_bond(carbonyl, anion, 1, "plain");
    doc.add_bond(anion, sodium, 1, "plain");
    let selected = doc.all_ids();
    assert_eq!(make_ionic(&mut doc, &selected), Ok(1));
    assert_eq!(doc.bond_between(carbonyl, oxygen).unwrap().1.order, 2);
    assert_eq!(doc.atom(anion).unwrap().charge, -1);
    assert_eq!(doc.atom(sodium).unwrap().charge, 1);
    for (id, h) in [
        (methyl, 3),
        (carbonyl, 0),
        (oxygen, 0),
        (anion, 0),
        (sodium, 0),
    ] {
        assert_eq!(doc.atom(id).unwrap().label_h, h);
    }
    let refresh = Refresh::calculate(&doc, &Refresh::default()).unwrap();
    assert_eq!(refresh.notice, None);
}

#[test]
fn calcium_aggregates_unit_directions_and_gains_two_charges() {
    let mut doc = Document::default();
    let calcium = doc.add_atom("Ca", Point::default());
    let a = doc.add_atom("O", Point::new(-84., 0.));
    let b = doc.add_atom("O", Point::new(0., -21.));
    doc.add_bond(calcium, a, 1, "plain");
    doc.add_bond(b, calcium, 1, "plain");
    assert_eq!(make_ionic(&mut doc, &[a, b, calcium]), Ok(2));
    assert!(doc.bonds.is_empty());
    assert_eq!(doc.atom(calcium).unwrap().charge, 2);
    assert_eq!(doc.atom(a).unwrap().charge, -1);
    assert_eq!(doc.atom(b).unwrap().charge, -1);
    let expected = doc.drawing_style.bond_length_world / 2_f32.sqrt();
    let actual = doc.atom(calcium).unwrap().position;
    assert!((actual.x - expected).abs() < 0.0001);
    assert!((actual.y - expected).abs() < 0.0001);
}

#[test]
fn cancelling_directions_keep_the_metal_in_place() {
    let mut doc = Document::default();
    let calcium = doc.add_atom("Ca", Point::default());
    let a = doc.add_atom("O", Point::new(-42., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(calcium, a, 1, "plain");
    doc.add_bond(calcium, b, 1, "plain");
    assert_eq!(make_ionic(&mut doc, &[a, b, calcium]), Ok(2));
    assert_eq!(doc.atom(calcium).unwrap().position, Point::default());
    assert_eq!(doc.atom(calcium).unwrap().charge, 2);
}

#[test]
fn supported_element_pairs_are_direction_independent() {
    for metal in ["Li", "Na", "K", "Rb", "Cs", "Ca", "Sr", "Ba"] {
        for other in ["O", "N", "S", "F", "Cl", "Br", "I"] {
            let mut doc = Document::default();
            let a = doc.add_atom(metal, Point::default());
            let b = doc.add_atom(other, Point::new(42., 0.));
            doc.add_bond(a, b, 1, "plain");
            assert_eq!(eligible_bonds(&doc, &[a, b]), [(a, b)], "{metal}–{other}");
            doc.bonds.first_mut().unwrap().reverse();
            assert_eq!(eligible_bonds(&doc, &[a, b]), [(a, b)], "{other}–{metal}");
        }
    }
}

#[test]
fn covalent_metals_and_multiple_or_unselected_bonds_are_unchanged() {
    for metal in ["Mg", "Zn", "B", "Fe", "Cu", "Al", "Be"] {
        let (mut doc, c, o, m) = methoxide(metal);
        let before = doc.clone();
        assert!(eligible_bonds(&doc, &[m, o]).is_empty(), "{metal}");
        assert!(make_ionic(&mut doc, &[c, o, m]).is_err(), "{metal}");
        assert_eq!(doc, before);
    }
    for order in [0, 2, 3, 4, 5, 6, 7] {
        let (mut doc, _, o, na) = methoxide("Na");
        doc.bonds.last_mut().unwrap().order = order;
        let before = doc.clone();
        assert!(eligible_bonds(&doc, &[o, na]).is_empty());
        assert!(make_ionic(&mut doc, &[o, na]).is_err());
        assert_eq!(doc, before);
    }
    let (mut doc, _, o, _) = methoxide("Na");
    let before = doc.clone();
    assert!(eligible_bonds(&doc, &[]).is_empty());
    assert!(make_ionic(&mut doc, &[]).is_err());
    assert!(make_ionic(&mut doc, &[o]).is_err());
    assert_eq!(doc, before);
}

#[test]
fn condensed_labels_and_abbreviations_stay_intact() {
    let (mut doc, c, o, na) = methoxide("Na");
    doc.abbreviations.push(Abbreviation {
        label: "ONa".into(),
        reverse_label: "NaO".into(),
        anchor: o,
        members: vec![o, na],
        label_style: None,
        label_color_override: false,
        highlight: None,
        alignment: Default::default(),
    });
    let before = doc.clone();
    assert!(eligible_bonds(&doc, &[c, o, na]).is_empty());
    assert!(make_ionic(&mut doc, &[c, o, na]).is_err());
    assert_eq!(doc, before);
    for label in ["ONa", "CO2Na"] {
        let mut doc = Document::default();
        let label_id = doc.add_atom("*", Point::default());
        doc.atom_mut(label_id).unwrap().display.variable = Some(label.into());
        let sodium = doc.add_atom("Na", Point::new(42., 0.));
        doc.add_bond(label_id, sodium, 1, "plain");
        let before = doc.clone();
        assert!(make_ionic(&mut doc, &[label_id, sodium]).is_err());
        assert_eq!(doc, before);
    }
}

#[test]
fn charge_changes_accumulate() {
    let (mut doc, _, o, na) = methoxide("Na");
    doc.atom_mut(o).unwrap().charge = -1;
    doc.atom_mut(na).unwrap().charge = 1;
    assert_eq!(make_ionic(&mut doc, &[o, na]), Ok(1));
    assert_eq!(doc.atom(o).unwrap().charge, -2);
    assert_eq!(doc.atom(na).unwrap().charge, 2);
}
