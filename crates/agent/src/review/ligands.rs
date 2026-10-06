//! Bounded presentation edits for independently attached Cp/Cp* ligands.
use crate::{attachments::Kind, document::Document};

pub(super) struct Ligand {
    pub anchor: u64,
    pub ring: Vec<u64>,
    pub ids: Vec<u64>,
    pub label: &'static str,
}

pub(super) fn find(doc: &Document, anchor: u64) -> Option<Ligand> {
    let point = doc.atom(anchor)?;
    if point.attachment != Some(Kind::MultiCenter)
        || point.centroid.len() != 5
        || doc.abbreviation(anchor).is_some()
    {
        return None;
    }
    let ring = &point.centroid;
    if doc
        .atoms
        .iter()
        .any(|a| a.id != anchor && a.centroid.iter().any(|id| ring.contains(id)))
    {
        return None;
    }
    let mut methyls = vec![];
    for id in ring {
        let atom = doc.atom(*id)?;
        if atom.element != "C" || !atom.centroid.is_empty() || doc.abbreviation(*id).is_some() {
            return None;
        }
        let bonds: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| b.a == *id || b.b == *id)
            .collect();
        if bonds.iter().any(|b| {
            b.stereo.is_some()
                || !(matches!(b.display.as_str(), "plain" | "bold")
                    || b.projection && b.display == "wedge")
        }) {
            return None;
        }
        if bonds
            .iter()
            .filter(|b| ring.contains(&b.a) && ring.contains(&b.b) && matches!(b.order, 1 | 2 | 4))
            .count()
            != 2
        {
            return None;
        }
        let outside: Vec<_> = bonds
            .iter()
            .filter(|b| !ring.contains(&b.a) || !ring.contains(&b.b))
            .collect();
        if outside.len() > 1 {
            return None;
        }
        if let Some(bond) = outside.first() {
            let other = if bond.a == *id { bond.b } else { bond.a };
            let methyl = doc.atom(other)?;
            if bond.order != 1
                || methyl.element != "C"
                || !methyl.centroid.is_empty()
                || doc
                    .bonds
                    .iter()
                    .filter(|b| b.a == other || b.b == other)
                    .count()
                    != 1
                || doc.abbreviation(other).is_some()
            {
                return None;
            }
            methyls.push(other);
        }
    }
    let label = match methyls.len() {
        0 => "Cp",
        5 => "Cp*",
        _ => return None,
    };
    let mut ids = vec![anchor];
    ids.extend(ring);
    ids.extend(methyls);
    if doc
        .atoms
        .iter()
        .any(|a| a.id != anchor && a.centroid.iter().any(|id| ids.contains(id)))
    {
        return None;
    }
    Some(Ligand {
        anchor,
        ring: ring.clone(),
        ids,
        label,
    })
}

pub(super) fn all(doc: &Document) -> Vec<Ligand> {
    doc.atoms
        .iter()
        .filter(|a| a.attachment == Some(Kind::MultiCenter))
        .take(16)
        .filter_map(|a| find(doc, a.id))
        .collect()
}

pub(super) fn tilt(
    doc: &mut Document,
    ligand: &Ligand,
    angles: [f32; 3],
    depth_bonds: bool,
    show_charge: bool,
) -> Result<(), String> {
    if angles.iter().any(|a| !a.is_finite())
        || angles[0].abs() > 85.
        || angles[1].abs() > 85.
        || angles[2].abs() > 360.
    {
        return Err("Invalid ligand tilt angles".into());
    }
    let center = doc
        .atom(ligand.anchor)
        .ok_or("Missing ligand anchor")?
        .position;
    // Remove old depth emphasis, including the erroneous methyl emphasis in
    // older generated drafts, before rotating. The review's requested emphasis
    // is applied below; intermediate automatic styling must not reverse these
    // bonds and trip the independent chemical-data guard.
    for bond in &mut doc.bonds {
        if ligand.ids.contains(&bond.a) && ligand.ids.contains(&bond.b) && bond.projection {
            bond.projection = false;
            bond.display = "plain".into();
        }
    }
    crate::projection::tilt(doc, &ligand.ids, angles[0], true);
    crate::projection::tilt(doc, &ligand.ids, angles[1], false);
    crate::editing::transform_about(doc, &ligand.ids, center, 1., angles[2]);
    if depth_bonds {
        crate::projection::depth_bonds(doc, &ligand.ring);
    }
    for atom in doc.atoms.iter_mut().filter(|a| ligand.ring.contains(&a.id)) {
        atom.display.hide_charge = !show_charge;
    }
    Ok(())
}
