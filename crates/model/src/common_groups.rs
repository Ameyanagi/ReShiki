//! Additional complete groups, built from the established endpoint replacement.
use crate::{
    chemistry::abbreviations::{ReplacementError as Error, replace_bundled as replace_base},
    document::Document,
};
pub const LABELS: &[&str] = &["MgBr", "N3"];

pub fn replace(doc: &Document, selection: &[u64], label: &str) -> Result<Document, Error> {
    let (base, elements, charges): (_, &[&str], &[i32]) = match label {
        "MgBr" => ("Et", &["Mg", "Br"], &[0, 0]),
        "N3" => ("nPr", &["N", "N", "N"], &[0, 1, -1]),
        _ => return Err(Error::Label),
    };
    let mut result = replace_base(doc, selection, base)?;
    let group = result
        .abbreviations
        .iter_mut()
        .find(|g| g.members.iter().any(|id| selection.contains(id)))
        .ok_or_else(|| Error::Definition("Missing replacement group".into()))?;
    group.label = label.into();
    group.reverse_label = if label == "MgBr" { "BrMg" } else { "" }.into();
    let anchor = group.anchor;
    let members = group.members.clone();
    let mut chain = vec![anchor];
    while chain.len() < elements.len() {
        let last = *chain
            .last()
            .ok_or_else(|| Error::Definition("Missing group anchor".into()))?;
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
            .ok_or_else(|| Error::Definition("Invalid group chain".into()))?;
        chain.push(next);
    }
    if members.len() != chain.len() {
        return Err(Error::Definition("Unexpected group member".into()));
    }
    for (i, ((&id, element), &charge)) in chain.iter().zip(elements).zip(charges).enumerate() {
        let atom = result
            .atom_mut(id)
            .ok_or_else(|| Error::Definition("Missing group atom".into()))?;
        atom.element = (*element).into();
        atom.charge = charge;
        atom.explicit_h = 0;
        atom.label_h = 0;
        atom.no_implicit = label == "MgBr" || i > 0;
        atom.isotope = 0;
        atom.aromatic = false;
        atom.stereo = None;
        atom.cip_label = None;
        atom.undefined_stereo = false;
    }
    for bond in &mut result.bonds {
        if members.contains(&bond.a) && members.contains(&bond.b) {
            bond.order = if label == "N3" { 2 } else { 1 };
        }
    }
    // Azide's central nitrogen is linear. Covalent MgBr also extends the
    // existing attachment direction instead of keeping a carbon zigzag.
    let first = result
        .atom(anchor)
        .ok_or_else(|| Error::Definition("Missing anchor".into()))?
        .position;
    let second_id = *chain
        .get(1)
        .ok_or_else(|| Error::Definition("Missing second atom".into()))?;
    let original_second = result
        .atom(second_id)
        .ok_or_else(|| Error::Definition("Missing second atom".into()))?
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
        .ok_or_else(|| Error::Definition("Missing second atom".into()))?
        .position = second;
    if let Some(&third_id) = chain.get(2) {
        result
            .atom_mut(third_id)
            .ok_or_else(|| Error::Definition("Missing third atom".into()))?
            .position = second.offset(second.x - first.x, second.y - first.y);
    }
    crate::projection::sync_centroids(&mut result);
    result.validate().map_err(Error::Definition)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chemistry::abbreviations::{self, AttachmentPolicy};

    #[test]
    fn additional_groups_preserve_typed_attachment_errors() -> Result<(), String> {
        let source = crate::rings::Preset::Benzene.document(42., false);
        let target = source.atoms.first().ok_or("Missing ring carbon")?.id;
        let refusal = abbreviations::replace(&source, &[target], "NO2")
            .unwrap_err()
            .to_string();
        for label in LABELS {
            for result in [
                replace(&source, &[target], label),
                abbreviations::replace(&source, &[target], label),
                abbreviations::replace_bundled(&source, &[target], label),
                abbreviations::replace_with_policy(
                    &source,
                    &[target],
                    label,
                    AttachmentPolicy::SharedAnchor,
                ),
            ] {
                let error = result.unwrap_err();
                assert!(matches!(error, Error::Attachment), "{label}: {error}");
                assert_eq!(error.to_string(), refusal);
                assert!(
                    !error
                        .to_string()
                        .contains("Invalid abbreviation definition")
                );
            }
        }
        Ok(())
    }
}
