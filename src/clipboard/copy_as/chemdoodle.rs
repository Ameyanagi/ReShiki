//! Bounded reaction handoff using the published ChemDoodle JSON format:
//! https://web.chemdoodle.com/docs/chemdoodle-json-format
//! This is a local text conversion, with no CAS endpoint or browser integration.
use crate::{document::Document, reactions::Participant};
use serde_json::{Value, json};
use std::collections::HashSet;

const STEREO: &str =
    "ChemDoodle reaction copy does not yet preserve stereochemistry; use RXN or reaction SMILES.";
const NORMALIZATION: &str =
    "ChemDoodle reaction copy would require chemical normalization; use RXN or reaction SMILES.";

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
        || arrow.cubic.is_some()
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
    if doc.bonds.iter().any(|b| !(1..=4).contains(&b.order)) {
        return Some(
            "ChemDoodle reaction copy supports ordinary single, double, triple and valid aromatic bonds only.",
        );
    }
    None
}

fn atomic_number(symbol: &str) -> Option<usize> {
    crate::editing::ELEMENTS
        .iter()
        .position(|&element| element == symbol)
        .map(|index| index + 1)
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
    if state.graph.atoms.iter().any(|a| a.radical_electrons != 0) {
        return Err("ChemDoodle reaction copy does not yet preserve inferred radicals; use RXN or reaction SMILES.".into());
    }
    // Aromatic perception may replace valid alternating bonds with order 4.
    // Keep atom identity, connectivity and every nonaromatic source order.
    if fragment.atoms.len() != state.graph.atoms.len()
        || fragment.bonds.len() != state.graph.bonds.len()
        || !prepared
            .ids
            .iter()
            .copied()
            .eq(fragment.atoms.iter().map(|a| a.id))
        || fragment.atoms.iter().zip(&state.graph.atoms).any(|(a, b)| {
            a.charge != i32::from(b.charge)
                || a.isotope != u32::from(b.isotope)
                || b.atomic_number == 0
                || atomic_number(&a.element) != Some(usize::from(b.atomic_number))
                || a.aromatic && !b.aromatic
                || !b.aromatic
                    && (a.explicit_h != u32::from(b.explicit_hydrogens)
                        || a.no_implicit != b.no_implicit)
        })
        || fragment.bonds.iter().zip(&state.graph.bonds).any(|(a, b)| {
            prepared.ids.get(b.a) != Some(&a.a)
                || prepared.ids.get(b.b) != Some(&a.b)
                || a.order != b.order && !(b.aromatic && matches!(a.order, 1 | 2 | 4))
        })
    {
        return Err(NORMALIZATION.into());
    }
    let kekule = crate::chemistry::document::kekule(&prepared).map_err(|e| e.to_string())?;
    let graph = &kekule.assignment.graph;
    if !kekule.success
        || graph.atoms.iter().any(|a| a.aromatic)
        || graph
            .bonds
            .iter()
            .any(|b| b.aromatic || !(1..=3).contains(&b.order))
    {
        return Err("ChemDoodle reaction copy could not assign valid Kekulé bonds; use RXN or reaction SMILES.".into());
    }
    if graph.atoms.len() != state.graph.atoms.len()
        || graph.bonds.len() != state.graph.bonds.len()
        || kekule.cache.len() != graph.atoms.len()
        || state.valences.len() != graph.atoms.len()
        || graph
            .atoms
            .iter()
            .zip(&kekule.cache)
            .zip(state.graph.atoms.iter().zip(&state.valences))
            .any(|((a, cache), (before, valence))| {
                u32::from(a.explicit_hydrogens) + cache.implicit_hydrogens
                    != u32::from(before.explicit_hydrogens) + valence.implicit_hydrogens
            })
    {
        return Err(NORMALIZATION.into());
    }
    let atoms: Vec<_> = fragment
        .atoms
        .iter()
        .zip(&graph.atoms)
        .zip(&kekule.cache)
        .map(|((source, atom), valence)| {
            let mut value = serde_json::Map::from_iter([
                ("i".into(), json!(format!("a{}", source.id))),
                ("l".into(), json!(source.element)),
                ("x".into(), json!(source.position.x)),
                ("y".into(), json!(source.position.y)),
                ("c".into(), json!(atom.charge)),
                // h is the count attached to this atom, not separate H vertices.
                (
                    "h".into(),
                    json!(u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens),
                ),
            ]);
            if atom.isotope != 0 {
                value.insert("m".into(), json!(atom.isotope));
            }
            Value::Object(value)
        })
        .collect();
    let bonds: Vec<_> = graph
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
    let [reaction] = doc.reactions.as_slice() else {
        return Err("Reaction formats need one complete defined reaction.".into());
    };
    let [arrow] = doc.arrows.as_slice() else {
        return Err("ChemDoodle reaction copy requires a single forward reaction arrow.".into());
    };
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
mod aromatic_tests;

#[cfg(test)]
mod tests;
