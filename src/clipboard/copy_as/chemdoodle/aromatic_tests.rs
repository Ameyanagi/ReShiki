use super::*;
use crate::{
    chemistry::{document::prepare, smiles},
    document::{Arrow, Point},
    editing::ELEMENTS,
    reactions::Reaction,
};

fn participant(doc: &mut Document, text: &str, offset: f32) -> Participant {
    let state = smiles::prepare(text).unwrap().state;
    let mut ids = Vec::new();
    for (i, atom) in state.graph.atoms.iter().enumerate() {
        let angle = i as f32 * std::f32::consts::TAU / state.graph.atoms.len() as f32;
        let id = doc.add_atom(
            ELEMENTS[usize::from(atom.atomic_number) - 1],
            Point::new(offset + angle.cos() * 42., angle.sin() * 42.),
        );
        let source = doc.atoms.last_mut().unwrap();
        source.charge = i32::from(atom.charge);
        source.isotope = u32::from(atom.isotope);
        source.explicit_h = u32::from(atom.explicit_hydrogens);
        source.no_implicit = atom.no_implicit;
        source.aromatic = atom.aromatic;
        ids.push(id);
    }
    for bond in &state.graph.bonds {
        doc.add_bond(ids[bond.a], ids[bond.b], bond.order, "plain");
    }
    Participant {
        atoms: ids,
        coefficient: 1,
    }
}

fn reaction(text: &str) -> Document {
    let mut doc = Document::default();
    let reactant = participant(&mut doc, text, 0.);
    let product = participant(&mut doc, "C1CCCCC1", 240.);
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        Point::new(90., 0.),
        Point::new(150., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(id);
    reaction.reactants.push(reactant);
    reaction.products.push(product);
    doc.reactions.push(reaction);
    doc
}

fn fragment(doc: &Document, participant: &Participant) -> Document {
    Document {
        atoms: doc
            .atoms
            .iter()
            .filter(|a| participant.atoms.contains(&a.id))
            .cloned()
            .collect(),
        bonds: doc
            .bonds
            .iter()
            .filter(|b| participant.atoms.contains(&b.a) && participant.atoms.contains(&b.b))
            .cloned()
            .collect(),
        ..Default::default()
    }
}

fn read_molecule(value: &Value) -> Document {
    let mut doc = Document::default();
    let mut ids = Vec::new();
    for atom in value["a"].as_array().unwrap() {
        let id = doc.add_atom(
            atom["l"].as_str().unwrap(),
            Point::new(
                atom["x"].as_f64().unwrap() as f32,
                atom["y"].as_f64().unwrap() as f32,
            ),
        );
        let source = doc.atoms.last_mut().unwrap();
        source.charge = atom["c"].as_i64().unwrap() as i32;
        source.isotope = atom["m"].as_u64().unwrap_or(0) as u32;
        source.explicit_h = atom["h"].as_u64().unwrap() as u32;
        source.no_implicit = true;
        ids.push(id);
    }
    for bond in value["b"].as_array().unwrap() {
        let order = bond["o"].as_u64().unwrap() as u8;
        assert!((1..=3).contains(&order));
        doc.add_bond(
            ids[bond["b"].as_u64().unwrap() as usize],
            ids[bond["e"].as_u64().unwrap() as usize],
            order,
            "plain",
        );
    }
    doc
}

fn identity(doc: &Document) -> String {
    smiles::write::write(&prepare(doc).unwrap().state, Default::default())
        .unwrap()
        .text
}

fn assert_roundtrip(doc: &Document, expected_hydrogens: u64) {
    let before = doc.clone();
    assert_eq!(reason(doc), None);
    let value: Value = serde_json::from_str(&write(doc).unwrap()).unwrap();
    assert_eq!(doc, &before);
    for (i, participant) in doc.reactions[0]
        .reactants
        .iter()
        .chain(&doc.reactions[0].products)
        .enumerate()
    {
        let source = fragment(doc, participant);
        let output = &value["m"][i];
        assert_eq!(identity(&read_molecule(output)), identity(&source));
        let prepared = prepare(&source).unwrap();
        for (((atom, source), chemical), valence) in output["a"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&source.atoms)
            .zip(&prepared.state.graph.atoms)
            .zip(&prepared.state.valences)
        {
            assert_eq!(atom["i"], json!(format!("a{}", source.id)));
            assert_eq!(
                atom["h"].as_u64().unwrap(),
                u64::from(chemical.explicit_hydrogens) + u64::from(valence.implicit_hydrogens)
            );
        }
    }
    let hydrogens = |i: usize| {
        value["m"][i]["a"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["h"].as_u64().unwrap())
            .sum::<u64>()
    };
    assert_eq!(hydrogens(0), expected_hydrogens);
    assert_eq!(hydrogens(1), 12);
    assert!(
        value["m"][1]["b"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["o"] == 1)
    );
}

#[test]
fn aromatic_and_alternating_benzene_copy_as_kekule_without_source_changes() {
    let aromatic = reaction("c1ccccc1");
    assert_roundtrip(&aromatic, 6);
    for (phase, flags) in [(0, false), (1, false), (0, true)] {
        let mut alternating = aromatic.clone();
        let ids = &alternating.reactions[0].reactants[0].atoms;
        for atom in &mut alternating.atoms {
            if ids.contains(&atom.id) {
                atom.aromatic = flags;
            }
        }
        for (i, bond) in alternating
            .bonds
            .iter_mut()
            .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
            .enumerate()
        {
            bond.order = if i % 2 == phase { 2 } else { 1 };
        }
        assert_roundtrip(&alternating, 6);
    }
}

#[test]
fn heteroaromatics_fused_rings_and_isotopes_keep_identity_and_hydrogens() {
    for (text, hydrogens) in [
        ("n1ccccc1", 5),
        ("c1ccc2ccccc2c1", 8),
        ("c1cc[nH]c1", 5),
        ("[13cH]1ccccc1", 6),
    ] {
        assert_roundtrip(&reaction(text), hydrogens);
    }
}

#[test]
fn failed_kekule_unknown_dummy_and_nonaromatic_normalization_keep_source() {
    let mut odd = reaction("C1CCCC1");
    let ids = &odd.reactions[0].reactants[0].atoms;
    for bond in &mut odd.bonds {
        if ids.contains(&bond.a) && ids.contains(&bond.b) {
            bond.order = 4;
        }
    }
    let mut unknown = reaction("CC");
    unknown.atoms[0].element = "Xx".into();
    let mut dummy = reaction("CC");
    dummy.atoms[0].element = "*".into();
    dummy.atoms[0].no_implicit = true;
    let mut nitro = reaction("CC");
    let nitrogen = nitro.add_atom("N", Point::new(-90., 0.));
    let oxygen = nitro.add_atom("O", Point::new(-118., 28.));
    let other = nitro.add_atom("O", Point::new(-118., -28.));
    nitro.add_bond(
        nitro.reactions[0].reactants[0].atoms[1],
        nitrogen,
        1,
        "plain",
    );
    nitro.add_bond(nitrogen, oxygen, 2, "plain");
    nitro.add_bond(nitrogen, other, 2, "plain");
    nitro.reactions[0].reactants[0]
        .atoms
        .extend([nitrogen, oxygen, other]);
    for (doc, message) in [
        (odd, "kek"),
        (unknown, "unknown element"),
        (dummy, "chemical normalization"),
        (nitro, "chemical normalization"),
    ] {
        let before = doc.clone();
        let error = write(&doc).unwrap_err();
        assert!(error.to_lowercase().contains(message), "{error}");
        assert_eq!(doc, before);
    }
}
