//! Defined haptic abbreviations: their hidden rings remain real chemistry.
use crate::{abbreviations::Abbreviation, attachments::Kind, document::Document};

pub const LABELS: &[&str] = &["Cp", "Cp*"];

/// Cp is cyclopentadienyl (C5H5−), Cp* is pentamethylcyclopentadienyl
/// (C10H15−). The anchor connects to all five ring atoms, not one carbon.
/// Metal oxidation states are left as entered by the user.
pub fn replace(doc: &Document, id: u64, label: &str) -> Result<Document, String> {
    doc.validate()?;
    if !LABELS.contains(&label) {
        return Err("Choose Cp or Cp*".into());
    }
    let anchor = doc.atom(id).ok_or("Missing ligand endpoint")?;
    let remove = doc
        .abbreviation(id)
        .map(|g| g.members.clone())
        .unwrap_or_else(|| vec![id]);
    let boundary: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| remove.contains(&b.a) != remove.contains(&b.b))
        .collect();
    if boundary.len() > 1
        || boundary
            .iter()
            .any(|b| b.order != 1 || b.a != id && b.b != id)
    {
        return Err("Select an isolated atom or a single-bond endpoint for Cp/Cp*".into());
    }
    if doc
        .atoms
        .iter()
        .any(|a| !remove.contains(&a.id) && a.centroid.iter().any(|i| remove.contains(i)))
    {
        return Err("Expand or remove attachments to this group before replacing it".into());
    }
    doc.next_id()
        .checked_add(11)
        .ok_or("Object ID limit exceeded")?;
    if doc.atoms.len() > 99_989 {
        return Err("Atom limit exceeded".into());
    }
    let mut result = doc.clone();
    result
        .atoms
        .retain(|a| a.id == id || !remove.contains(&a.id));
    result
        .bonds
        .retain(|b| !(remove.contains(&b.a) && remove.contains(&b.b)));
    result
        .abbreviations
        .retain(|g| !g.members.iter().any(|i| remove.contains(i)));
    let mut ring = Vec::new();
    let mut members = vec![id];
    let methylated = label == "Cp*";
    let length = result.drawing_style.bond_length_world;
    let radius = length / (2. * (std::f32::consts::PI / 5.).sin());
    // Aim the metal contact between vertices/methyl groups in the expanded view.
    let start_angle = boundary
        .first()
        .and_then(|b| doc.atom(if b.a == id { b.b } else { b.a }))
        .map(|other| {
            (other.position.y - anchor.position.y).atan2(other.position.x - anchor.position.x)
                - std::f32::consts::PI / 5.
        })
        .unwrap_or(-std::f32::consts::FRAC_PI_2);
    for index in 0..5 {
        let angle = start_angle + index as f32 * std::f32::consts::TAU / 5.;
        let position = anchor
            .position
            .offset(radius * angle.cos(), radius * angle.sin());
        let carbon = result.add_atom("C", position);
        let atom = result.atom_mut(carbon).ok_or("Missing ligand carbon")?;
        atom.depth = anchor.depth;
        // Fix ligand H counts independently of the metal's coordination/valence.
        atom.explicit_h = u32::from(!methylated);
        atom.label_h = atom.explicit_h;
        atom.no_implicit = true;
        atom.charge = if index == 0 { -1 } else { 0 };
        ring.push(carbon);
        members.push(carbon);
        if methylated {
            let methyl = result.add_atom(
                "C",
                position.offset(length * angle.cos(), length * angle.sin()),
            );
            let atom = result.atom_mut(methyl).ok_or("Missing methyl carbon")?;
            atom.depth = anchor.depth;
            atom.explicit_h = 3;
            atom.label_h = 3;
            atom.no_implicit = true;
            result.add_bond(carbon, methyl, 1, "plain");
            members.push(methyl);
        }
    }
    for (index, (&a, &b)) in ring.iter().zip(ring.iter().cycle().skip(1)).enumerate() {
        result.add_bond(a, b, if index == 1 || index == 3 { 2 } else { 1 }, "plain");
    }
    let point = result.atom_mut(id).ok_or("Missing ligand anchor")?;
    point.element = "*".into();
    point.centroid = ring;
    point.attachment = Some(Kind::MultiCenter);
    point.charge = 0;
    point.isotope = 0;
    point.explicit_h = 0;
    point.label_h = 0;
    point.no_implicit = true;
    point.aromatic = false;
    point.radical_electrons = 0;
    point.stereo = None;
    point.cip_label = None;
    point.display.variable = None;
    point.marks.clear();
    for group in &mut result.groups {
        if group.members.iter().any(|i| remove.contains(i)) {
            group.members.retain(|i| !remove.contains(i));
            group.members.extend(&members);
        }
    }
    for reaction in &mut result.reactions {
        for role in [
            &mut reaction.reactants,
            &mut reaction.products,
            &mut reaction.agents,
        ] {
            for part in role {
                if part.atoms.iter().any(|i| remove.contains(i)) {
                    part.atoms.retain(|i| !remove.contains(i));
                    part.atoms.extend(&members);
                }
            }
        }
    }
    result.abbreviations.push(Abbreviation {
        label: label.into(),
        reverse_label: label.into(),
        anchor: id,
        members,
    });
    result.validate()?;
    Ok(result)
}
