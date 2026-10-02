//! Bounded reaction handoff using the published ChemDoodle JSON format:
//! https://web.chemdoodle.com/docs/chemdoodle-json-format
//! This is a local text conversion, with no CAS endpoint or browser integration.
use crate::{document::Document, reactions::Participant};
use serde_json::{Value, json};
use std::collections::HashSet;

const STEREO: &str =
    "ChemDoodle reaction copy does not yet preserve stereochemistry; use RXN or reaction SMILES.";
const AROMATIC: &str =
    "ChemDoodle reaction copy does not yet preserve aromaticity; use RXN or reaction SMILES.";

/// Cheap source checks for the menu. Prepared chemistry is checked again below.
pub(super) fn reason(doc: &Document) -> Option<&'static str> {
    let reaction = doc.reactions.first()?;
    if doc.atoms.len() > 10_000 || doc.bonds.len() > 30_000 {
        return Some("ChemDoodle reaction copy is limited to 10,000 atoms and 30,000 bonds.");
    }
    if !reaction.agents.is_empty() {
        return Some(
            "ChemDoodle reaction copy does not yet preserve reagent/catalyst roles; use RXN or reaction SMILES.",
        );
    }
    if reaction
        .reactants
        .iter()
        .chain(&reaction.products)
        .any(|p| p.coefficient != 1)
    {
        return Some(
            "ChemDoodle reaction copy requires participant coefficients of one; use RXN or reaction SMILES.",
        );
    }
    let [arrow] = doc.arrows.as_slice() else {
        return Some("ChemDoodle reaction copy requires a single forward reaction arrow.");
    };
    let style = arrow.appearance();
    if arrow.kind != "forward"
        || arrow.control.is_some()
        || style.head != crate::arrows::Head::Full
        || style.tail != crate::arrows::Head::None
        || style.no_go != crate::arrows::NoGo::None
        || style.dipole
    {
        return Some(
            "ChemDoodle reaction copy requires a forward arrow without reverse, no-reaction or electron-flow semantics.",
        );
    }
    if !doc.abbreviations.is_empty() {
        return Some("Expand abbreviations before ChemDoodle reaction copy.");
    }
    if doc.atoms.iter().any(|a| a.map_num != 0) {
        return Some(
            "ChemDoodle reaction copy does not yet preserve atom mapping; use RXN or reaction SMILES.",
        );
    }
    if doc
        .atoms
        .iter()
        .any(|a| a.stereo.is_some() || a.depth != 0.)
        || doc.bonds.iter().any(|b| {
            b.stereo.is_some() || !b.stereo_atoms.is_empty() || b.projection || b.display != "plain"
        })
    {
        return Some(STEREO);
    }
    if doc.atoms.iter().any(|a| a.aromatic) || doc.bonds.iter().any(|b| b.order == 4) {
        return Some(AROMATIC);
    }
    if doc.atoms.iter().any(|a| a.radical_electrons != 0) {
        return Some(
            "ChemDoodle reaction copy does not yet preserve radicals; use RXN or reaction SMILES.",
        );
    }
    if doc
        .atoms
        .iter()
        .any(|a| !a.centroid.is_empty() || a.attachment.is_some())
    {
        return Some(
            "ChemDoodle reaction copy does not preserve attachment or centroid atoms; use the native drawing.",
        );
    }
    if doc.bonds.iter().any(|b| !(1..=3).contains(&b.order)) {
        return Some(
            "ChemDoodle reaction copy supports ordinary single, double and triple bonds only.",
        );
    }
    None
}

fn molecule(doc: &Document, participant: &Participant) -> Result<(Value, String), String> {
    let ids: HashSet<_> = participant.atoms.iter().copied().collect();
    let fragment = Document {
        atoms: doc
            .atoms
            .iter()
            .filter(|a| ids.contains(&a.id))
            .cloned()
            .collect(),
        bonds: doc
            .bonds
            .iter()
            .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
            .cloned()
            .collect(),
        ..Default::default()
    };
    if crate::reactions::molecules(&fragment, &participant.atoms).len() != 1 {
        return Err("ChemDoodle reaction copy requires one connected molecule per participant; grouped salts need RXN or reaction SMILES.".into());
    }
    let prepared = crate::chemistry::document::prepare(&fragment).map_err(|e| e.to_string())?;
    let state = &prepared.state;
    if state.metadata.atoms.iter().any(|a| a.chiral_tag != 0)
        || state
            .metadata
            .bonds
            .iter()
            .any(|b| b.stereo != 0 || b.unknown_stereo)
        || !state.metadata.groups.is_empty()
    {
        return Err(STEREO.into());
    }
    if state.graph.atoms.iter().any(|a| a.aromatic) || state.graph.bonds.iter().any(|b| b.aromatic)
    {
        return Err(AROMATIC.into());
    }
    if state.graph.atoms.iter().any(|a| a.radical_electrons != 0) {
        return Err("ChemDoodle reaction copy does not yet preserve inferred radicals; use RXN or reaction SMILES.".into());
    }
    // Do not silently pass through a sanitization that changes the source graph.
    if fragment.atoms.iter().zip(&state.graph.atoms).any(|(a, b)| {
        a.charge != i32::from(b.charge) || a.isotope != u32::from(b.isotope) || b.atomic_number == 0
    }) || fragment
        .bonds
        .iter()
        .zip(&state.graph.bonds)
        .any(|(a, b)| a.order != b.order)
    {
        return Err("ChemDoodle reaction copy would require chemical normalization; use RXN or reaction SMILES.".into());
    }
    let atoms: Vec<_> = fragment
        .atoms
        .iter()
        .zip(&state.graph.atoms)
        .zip(&state.valences)
        .map(|((source, atom), valence)| {
            let mut value = json!({
                "i": format!("a{}", source.id), "l": source.element,
                "x": source.position.x, "y": source.position.y,
                "c": atom.charge,
                // h is the count attached to this atom, not separate H vertices.
                "h": u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens
            });
            if atom.isotope != 0 {
                value["m"] = json!(atom.isotope);
            }
            value
        })
        .collect();
    let bonds: Vec<_> = state
        .graph
        .bonds
        .iter()
        .map(|bond| json!({"b": bond.a, "e": bond.b, "o": bond.order}))
        .collect();
    let anchor = fragment.atoms.first().ok_or("Empty reaction participant")?;
    Ok((json!({"a": atoms, "b": bonds}), format!("a{}", anchor.id)))
}

pub(super) fn write(doc: &Document) -> Result<String, String> {
    doc.validate()?;
    if let Some(reason) = super::reaction_reason(doc).or_else(|| reason(doc)) {
        return Err(reason.into());
    }
    let reaction = &doc.reactions[0];
    let arrow = &doc.arrows[0];
    let mut molecules = Vec::new();
    let mut reactants = Vec::new();
    let mut products = Vec::new();
    for (participants, anchors) in [
        (&reaction.reactants, &mut reactants),
        (&reaction.products, &mut products),
    ] {
        for participant in participants {
            let (molecule, anchor) = molecule(doc, participant)?;
            molecules.push(molecule);
            anchors.push(anchor);
        }
    }
    serde_json::to_string(&json!({
        "m": molecules,
        "s": [{"t": "Line", "i": format!("s{}", arrow.id),
            "x1": arrow.start.x, "y1": arrow.start.y,
            "x2": arrow.end.x, "y2": arrow.end.y,
            "a": "synthetic", "rs": reactants, "ps": products}]
    }))
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
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
        cases.push((aromatic, "aromaticity"));
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
}
