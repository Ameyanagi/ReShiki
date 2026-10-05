//! Additional complete groups, built from the established endpoint replacement.
use crate::{chemistry::abbreviations::replace as replace_base, document::Document};
pub const LABELS: &[&str] = &["MgBr", "N3"];

pub fn replace(doc: &Document, selection: &[u64], label: &str) -> Result<Document, String> {
    let (base, elements, charges): (_, &[&str], &[i32]) = match label {
        "MgBr" => ("Et", &["Mg", "Br"], &[0, 0]),
        "N3" => ("nPr", &["N", "N", "N"], &[0, 1, -1]),
        _ => return Err("Unknown chemical group".into()),
    };
    let mut result = replace_base(doc, selection, base).map_err(|e| e.to_string())?;
    let group = result
        .abbreviations
        .iter_mut()
        .find(|g| g.members.iter().any(|id| selection.contains(id)))
        .ok_or("Missing replacement group")?;
    group.label = label.into();
    group.reverse_label = if label == "MgBr" { "BrMg" } else { "" }.into();
    let anchor = group.anchor;
    let members = group.members.clone();
    let mut chain = vec![anchor];
    while chain.len() < elements.len() {
        let last = *chain.last().ok_or("Missing group anchor")?;
        let next = result
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == last {
                    Some(b.b)
                } else if b.b == last {
                    Some(b.a)
                } else {
                    None
                }
            })
            .find(|id| members.contains(id) && !chain.contains(id))
            .ok_or("Invalid group chain")?;
        chain.push(next);
    }
    if members.len() != chain.len() {
        return Err("Unexpected group member".into());
    }
    for (i, ((&id, element), &charge)) in chain.iter().zip(elements).zip(charges).enumerate() {
        let atom = result.atom_mut(id).ok_or("Missing group atom")?;
        atom.element = (*element).into();
        atom.charge = charge;
        atom.explicit_h = 0;
        atom.label_h = 0;
        atom.no_implicit = label == "MgBr" || i > 0;
        atom.isotope = 0;
        atom.aromatic = false;
        atom.stereo = None;
        atom.cip_label = None;
    }
    for bond in &mut result.bonds {
        if members.contains(&bond.a) && members.contains(&bond.b) {
            bond.order = if label == "N3" { 2 } else { 1 };
        }
    }
    // Azide's central nitrogen is linear. Covalent MgBr also extends the
    // existing attachment direction instead of keeping a carbon zigzag.
    let first = result.atom(anchor).ok_or("Missing anchor")?.position;
    let second_id = *chain.get(1).ok_or("Missing second atom")?;
    let original_second = result
        .atom(second_id)
        .ok_or("Missing second atom")?
        .position;
    let outside = result
        .bonds
        .iter()
        .filter_map(|b| {
            if b.a == anchor {
                Some(b.b)
            } else if b.b == anchor {
                Some(b.a)
            } else {
                None
            }
        })
        .find(|id| !members.contains(id))
        .and_then(|id| result.atom(id))
        .map(|a| a.position);
    // Reserve space for the two-letter symbols and azide's charge labels.
    // The external bond and attachment position retain their original geometry.
    let delta = if label == "MgBr" {
        outside
            .map(|p| crate::document::Point::new(first.x - p.x, first.y - p.y))
            .unwrap_or(crate::document::Point::new(
                original_second.x - first.x,
                original_second.y - first.y,
            ))
    } else {
        crate::document::Point::new(original_second.x - first.x, original_second.y - first.y)
    };
    let second = first.offset(delta.x * 1.5, delta.y * 1.5);
    result
        .atom_mut(second_id)
        .ok_or("Missing second atom")?
        .position = second;
    if let Some(&third_id) = chain.get(2) {
        result
            .atom_mut(third_id)
            .ok_or("Missing third atom")?
            .position = second.offset(second.x - first.x, second.y - first.y);
    }
    crate::projection::sync_centroids(&mut result);
    result.validate()?;
    Ok(result)
}
