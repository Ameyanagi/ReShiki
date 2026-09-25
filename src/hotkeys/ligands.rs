//! Contextual pi-ligand construction uses real ring atoms and typed attachments.
use crate::{attachments::Kind, document::Document, editing};

pub(super) fn add(
    doc: &Document,
    id: u64,
    size: usize,
    length: f32,
) -> Result<(Document, u64), String> {
    doc.validate()?;
    if !matches!(size, 5 | 6) || !length.is_finite() || length <= 0. {
        return Err("Invalid ring size or bond length".into());
    }
    if doc.atoms.len() > 99_990 || doc.next_id().checked_add(10).is_none_or(|n| n == u64::MAX) {
        return Err("Drawing atom limit reached".into());
    }
    let atom = doc.atom(id).ok_or("Missing ligand target")?;
    let metal = matches!(
        atom.element.as_str(),
        "Li" | "Na"
            | "K"
            | "Rb"
            | "Cs"
            | "Be"
            | "Mg"
            | "Ca"
            | "Sr"
            | "Ba"
            | "Al"
            | "Sc"
            | "Ti"
            | "V"
            | "Cr"
            | "Mn"
            | "Fe"
            | "Co"
            | "Ni"
            | "Cu"
            | "Zn"
            | "Y"
            | "Zr"
            | "Nb"
            | "Mo"
            | "Tc"
            | "Ru"
            | "Rh"
            | "Pd"
            | "Ag"
            | "Cd"
            | "Hf"
            | "Ta"
            | "W"
            | "Re"
            | "Os"
            | "Ir"
            | "Pt"
            | "Au"
            | "Hg"
    );
    let neighbors: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            if b.a == id {
                doc.atom(b.b)
            } else if b.b == id {
                doc.atom(b.a)
            } else {
                None
            }
        })
        .map(|a| a.position)
        .collect();
    if !atom.centroid.is_empty()
        || doc.abbreviations.iter().any(|g| g.members.contains(&id))
        || !metal
            && (neighbors.len() > 1
                || atom.charge != 0
                || atom.isotope != 0
                || atom.stereo.is_some())
    {
        return Err("Choose a metal atom or an uncharged terminal atom for a pi ligand".into());
    }
    if !metal
        && doc
            .bonds
            .iter()
            .any(|b| (b.a == id || b.b == id) && b.order != 1)
    {
        return Err("Pi-ligand replacement needs a single-bond endpoint".into());
    }
    let angle = if neighbors.is_empty() {
        -std::f32::consts::FRAC_PI_6
    } else {
        editing::open_angle(atom.position, &neighbors)
    };
    let radius = length / (2. * (std::f32::consts::PI / size as f32).sin());
    let center = if metal {
        atom.position.offset(
            (length + radius * 0.5) * angle.cos(),
            (length + radius * 0.5) * angle.sin(),
        )
    } else {
        atom.position
    };
    let mut result = doc.clone();
    let anchor = if metal {
        result.add_atom("*", center)
    } else {
        id
    };
    let mut ring = vec![];
    for i in 0..size {
        let t = std::f32::consts::PI - std::f32::consts::PI / size as f32
            + i as f32 * std::f32::consts::TAU / size as f32;
        let p = center.offset(radius * t.cos(), radius * t.sin());
        let carbon = result.add_atom("C", p);
        let a = result.atom_mut(carbon).ok_or("Missing ring atom")?;
        a.aromatic = true;
        a.explicit_h = 1;
        a.label_h = 1;
        a.no_implicit = true;
        a.depth = atom.depth;
        if size == 5 && i == 0 {
            a.charge = -1;
            a.display.hide_charge = true;
        }
        ring.push(carbon);
    }
    for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        result.add_bond(a, b, 4, "plain");
    }
    // Build a regular ring in XYZ, then rotate it. Keeping depth is essential:
    // later tilts and the aromatic ellipse must use the same physical plane.
    crate::projection::tilt(&mut result, &ring, 60., false);
    editing::transform_about(&mut result, &ring, center, 1., angle.to_degrees());
    for carbon in &ring {
        let p = result.atom(*carbon).ok_or("Missing ring atom")?.position;
        if doc
            .atoms
            .iter()
            .filter(|a| a.id != id && a.centroid.is_empty())
            .any(|a| a.position.distance(p) < length * 0.4)
        {
            return Err(
                "There is not enough room for this ligand; move the target or nearby atoms first"
                    .into(),
            );
        }
    }
    // A thick near edge tapers into the far edges. These are perspective
    // styles on aromatic bonds, never tetrahedral stereochemical wedges.
    for bond in &mut result.bonds {
        if !ring.contains(&bond.a) || !ring.contains(&bond.b) {
            continue;
        }
        bond.projection = true;
    }
    crate::projection::refresh_depth_bonds(&mut result, &ring);
    let point = result.atom_mut(anchor).ok_or("Missing ring attachment")?;
    point.element = "*".into();
    point.position = center;
    point.depth = atom.depth;
    point.centroid = ring.clone();
    point.attachment = Some(Kind::MultiCenter);
    point.explicit_h = 0;
    point.label_h = 0;
    point.no_implicit = true;
    point.aromatic = false;
    point.display = Default::default();
    point.marks.clear();
    point.radical_electrons = 0;
    if metal {
        result.add_bond(id, anchor, 1, "plain");
    }
    for group in &mut result.groups {
        if group.members.contains(&id) {
            group.members.extend(ring.iter().copied());
            if metal {
                group.members.push(anchor);
            }
        }
    }
    for reaction in &mut result.reactions {
        for role in [
            &mut reaction.reactants,
            &mut reaction.products,
            &mut reaction.agents,
        ] {
            for participant in role {
                if participant.atoms.contains(&id) {
                    participant.atoms.extend(ring.iter().copied());
                    if metal {
                        participant.atoms.push(anchor);
                    }
                }
            }
        }
    }
    crate::projection::sync_centroids(&mut result);
    result.validate()?;
    Ok((result, if metal { id } else { anchor }))
}
