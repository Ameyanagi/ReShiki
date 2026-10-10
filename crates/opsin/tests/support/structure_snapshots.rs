use opsin::{
    Structure,
    graph::{AtomId, BondStereoValue, StereoGroupType, StereoReference},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn graph_value(structure: &Structure) -> Value {
    let graph = &structure.graph;
    let fragment = graph.fragment(structure.fragment);
    let indices: BTreeMap<AtomId, usize> = fragment
        .atoms
        .iter()
        .enumerate()
        .map(|(i, &atom)| (atom, i))
        .collect();
    let reference = |reference: Option<StereoReference>| -> Value {
        match reference {
            None => Value::Null,
            Some(StereoReference::Atom(atom)) => json!(indices[&atom]),
            Some(StereoReference::ImplicitHydrogen) => json!("hydrogen"),
            Some(StereoReference::DeoxyHydrogen) => json!("deoxyHydrogen"),
            Some(StereoReference::RingOpening) => json!("unresolvedRingOpening"),
        }
    };
    let atoms: Vec<Value> = fragment.atoms.iter().map(|&id| {
        let atom = graph.atom(id);
        let parity = atom.parity.as_ref().map(|parity| json!({
            "references": parity.atom_refs.map(&reference),
            "parity": parity.parity,
            "group_type": match parity.stereo_group.kind {
                StereoGroupType::Absolute => "Abs", StereoGroupType::Racemic => "Rac",
                StereoGroupType::Relative => "Rel", StereoGroupType::Unknown => "Unk",
            },
            "group_number": parity.stereo_group.number,
        }));
        json!({
            "element": atom.element.symbol(), "isotope": atom.isotope, "charge": atom.charge,
            "protons_added_or_removed": atom.protons_explicitly_added_or_removed,
            "implicit_hydrogen_allowed": atom.implicit_hydrogen_allowed,
            "lambda_valency": atom.lambda_convention_valency, "minimum_valency": atom.minimum_valency,
            "spare_valency": atom.spare_valency, "out_valency": atom.out_valency, "parity": parity,
        })
    }).collect();
    let bonds: Vec<Value> = fragment.bonds.iter().map(|&id| {
        let bond = graph.bond(id);
        let stereo = bond.stereo.as_ref().map(|stereo| json!({
            "references": stereo.atom_refs.map(|atom| indices[&atom]),
            "value": match stereo.value { BondStereoValue::Cis => "C", BondStereoValue::Trans => "T" },
        }));
        json!({"from":indices[&bond.from],"to":indices[&bond.to],"order":bond.order,"stereo":stereo})
    }).collect();
    let polymers: Vec<usize> = fragment
        .polymer_attachment_points
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|atom| indices[atom])
        .collect();
    json!({"atoms":atoms,"bonds":bonds,"polymer_attachment_points":polymers})
}

pub fn first_difference(expected: &Value, actual: &Value, path: &str) -> Option<String> {
    if expected == actual {
        return None;
    }
    match (expected, actual) {
        (Value::Array(e), Value::Array(a)) if e.len() == a.len() => {
            for (i, (e, a)) in e.iter().zip(a).enumerate() {
                if let Some(diff) = first_difference(e, a, &format!("{path}[{i}]")) {
                    return Some(diff);
                }
            }
        }
        (Value::Object(e), Value::Object(a)) if e.len() == a.len() => {
            for (key, value) in e {
                if let Some(other) = a.get(key) {
                    if let Some(diff) = first_difference(value, other, &format!("{path}.{key}")) {
                        return Some(diff);
                    }
                } else {
                    return Some(format!("{path}.{key}: missing"));
                }
            }
        }
        _ => {}
    }
    let abbreviated = |value: &Value| {
        let text = value.to_string();
        let mut chars = text.chars();
        let mut prefix: String = chars.by_ref().take(240).collect();
        if chars.next().is_some() {
            prefix.push_str("...");
        }
        prefix
    };
    Some(format!(
        "{path}: expected {}, actual {}",
        abbreviated(expected),
        abbreviated(actual)
    ))
}
