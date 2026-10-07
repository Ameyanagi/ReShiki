use super::*;
use crate::{
    document::{Arrow, Point},
    reactions::Reaction,
};

fn reaction() -> Document {
    let mut doc = Document::default();
    let mut participants = Vec::new();
    for (offset, order) in [(0., 1), (240., 2)] {
        let a = doc.add_atom("C", Point::new(offset, 0.));
        let b = doc.add_atom("C", Point::new(offset + 36., 20.));
        let c = doc.add_atom("O", Point::new(offset + 72., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, order, "plain");
        participants.push(Participant {
            atoms: vec![a, b, c],
            coefficient: 1,
        });
    }
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(120., 10.),
        Point::new(200., 10.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(participants.remove(0));
    reaction.products.push(participants.remove(0));
    doc.reactions.push(reaction);
    doc
}

#[test]
fn role_references_follow_explicit_membership_and_leave_source_unchanged() {
    let mut doc = reaction();
    let before = doc.clone();
    let value: Value = serde_json::from_str(&write(&doc).unwrap()).unwrap();
    assert_eq!(doc, before);
    assert_eq!(value["s"][0]["rs"], json!(["a1"]));
    assert_eq!(value["s"][0]["ps"], json!(["a4"]));
    assert_eq!(value["m"][0]["b"][1]["o"], 1);
    assert_eq!(value["m"][1]["b"][1]["o"], 2);
    assert_eq!(value["m"][0]["a"][2]["h"], 1);
    assert_eq!(value["m"][1]["a"][2]["h"], 0);
    let reaction = &mut doc.reactions[0];
    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
    let swapped: Value = serde_json::from_str(&write(&doc).unwrap()).unwrap();
    assert_eq!(swapped["s"][0]["rs"], json!(["a4"]));
    assert_eq!(swapped["s"][0]["ps"], json!(["a1"]));
}

#[test]
fn charges_isotopes_and_separate_deuterium_are_retained() {
    let mut doc = reaction();
    doc.atoms[2].charge = -1;
    doc.atoms[2].no_implicit = true;
    doc.atoms[3].isotope = 13;
    let deuterium = doc.add_atom("H", Point::new(240., -36.));
    doc.atoms.last_mut().unwrap().isotope = 2;
    doc.add_bond(4, deuterium, 1, "plain");
    doc.reactions[0].products[0].atoms.push(deuterium);
    let value: Value = serde_json::from_str(&write(&doc).unwrap()).unwrap();
    assert_eq!(value["m"][0]["a"][2]["c"], -1);
    assert_eq!(value["m"][0]["a"][2]["h"], 0);
    assert_eq!(value["m"][1]["a"][0]["m"], 13);
    assert_eq!(value["m"][1]["a"][0]["h"], 2);
    assert_eq!(value["m"][1]["a"][3]["l"], "H");
    assert_eq!(value["m"][1]["a"][3]["m"], 2);
    assert_eq!(value["m"][1]["a"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn unsupported_chemistry_has_no_prepared_payload() {
    use super::super::{CopyFormat, prepare_as};
    let source = reaction();
    let mut cases = Vec::new();
    let mut agents = source.clone();
    let agent = agents.add_atom("O", Point::new(160., -80.));
    agents.reactions[0].agents.push(Participant {
        atoms: vec![agent],
        coefficient: 1,
    });
    cases.push((agents, "reagent/catalyst"));
    let mut coefficient = source.clone();
    coefficient.reactions[0].reactants[0].coefficient = 2;
    cases.push((coefficient, "coefficients"));
    let mut mapped = source.clone();
    mapped.atoms[0].map_num = 1;
    cases.push((mapped, "atom mapping"));
    let mut wedge = source.clone();
    wedge.bonds[0].display = "wedge".into();
    cases.push((wedge, "stereochemistry"));
    let mut no_go = source.clone();
    no_go.arrows[0].kind = "no_go".into();
    cases.push((no_go, "forward arrow"));
    let mut aromatic = source.clone();
    aromatic.atoms[0].aromatic = true;
    cases.push((aromatic, "aromatic"));
    for (doc, message) in cases {
        let error = prepare_as(Default::default(), doc, CopyFormat::ChemDoodleReaction)
            .await
            .unwrap_err();
        assert!(error.contains(message), "{error}");
    }
    let prepared = prepare_as(Default::default(), source, CopyFormat::ChemDoodleReaction)
        .await
        .unwrap();
    assert!(prepared.text().unwrap().contains("\"synthetic\""));
    assert!(!prepared.notices.is_empty());
}

#[test]
fn stereo_inferred_from_plain_double_bond_geometry_is_rejected() {
    let mut doc = reaction();
    // The left participant is now trans-difluoroethene. There are no
    // stored stereo flags or wedges; preparation must still detect E/Z.
    doc.atoms[0].element = "F".into();
    doc.atoms[2].element = "C".into();
    doc.atoms[0].position = Point::new(0., 20.);
    doc.atoms[1].position = Point::new(36., 0.);
    doc.atoms[2].position = Point::new(72., 0.);
    doc.bonds[1].order = 2;
    let fluorine = doc.add_atom("F", Point::new(108., -20.));
    doc.add_bond(3, fluorine, 1, "plain");
    doc.reactions[0].reactants[0].atoms.push(fluorine);
    assert_eq!(reason(&doc), None);
    assert!(write(&doc).unwrap_err().contains("stereochemistry"));
}
