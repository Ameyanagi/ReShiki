//! Atomic edits of an explicitly defined tetrahedral configuration.
//!
//! Projection marks describe a view of the molecule. Inverting a Haworth or
//! Fischer centre therefore edits its visible substituents as well as winding.
use crate::{
    document::{Document, Point},
    haworth::interchange,
};
use std::collections::{HashMap, HashSet};

/// Invert one defined tetrahedral centre while retaining its drawing convention.
/// Missing atoms, unspecified centres, and unsafe branch edits leave `doc`
/// unchanged. Atom IDs and the chemical graph are retained.
pub fn invert_configuration(doc: &mut Document, id: u64) -> Result<(), String> {
    let atom = doc.atom(id).ok_or("The selected atom no longer exists")?;
    doc.validate()?;
    // Wedge-drawn centres keep their configuration in the marks alone.
    let mut stereo = atom.stereo.clone();
    if let Some(stereo) = &mut stereo {
        stereo.winding = match stereo.winding.as_str() {
            "cw" => "ccw",
            "ccw" => "cw",
            _ => return Err("The selected atom has no defined tetrahedral configuration".into()),
        }
        .into();
    }
    let mut work = doc.clone();
    let haworth = interchange::recognize(doc);
    if haworth.stereo.contains_key(&id) {
        invert_haworth(&mut work, id, &haworth)?;
    } else if crate::fischer::marked_centers(doc).contains(&id) {
        // Off-axis, the drawing has no Fischer reading to invert; reconciling
        // a later realignment would silently undo a winding-only inversion.
        let cross = crate::fischer::recognize_center(doc, id)
            .ok_or("Align the Fischer projection with the page axes before inverting it")?;
        if !crate::fischer::equivalent(
            doc.atom(id)
                .and_then(|atom| atom.stereo.as_ref())
                .ok_or("The selected atom has no defined tetrahedral configuration")?,
            &cross.reading(),
        ) {
            return Err("The Fischer appearance conflicts with its stored configuration".into());
        }
        invert_fischer(&mut work, &cross, &haworth)?;
    } else {
        // Narrow ends identify the centre whose configuration the marks carry.
        // Preserve filled/outline and short/long hatch families where possible.
        let mut marked = false;
        for bond in &mut work.bonds {
            if bond.a == id && !bond.projection && bond.order == 1 {
                marked |= invert_mark(bond);
            }
        }
        if !marked && stereo.is_none() {
            return Err("The selected atom has no defined tetrahedral configuration".into());
        }
    }
    work.atom_mut(id)
        .ok_or("The selected atom no longer exists")?
        .stereo = stereo;
    crate::atom_labels::clear_computed(&mut work);
    work.validate()?;
    if haworth.stereo.contains_key(&id) {
        if !interchange::recognize(&work).stereo.contains_key(&id) {
            return Err("Inversion would lose the Haworth drawing convention".into());
        }
        interchange::export_bonds(&work).map_err(String::from)?;
    }
    *doc = work;
    Ok(())
}

fn other(bond: &crate::document::Bond, id: u64) -> Option<u64> {
    if bond.a == id {
        Some(bond.b)
    } else if bond.b == id {
        Some(bond.a)
    } else {
        None
    }
}

fn invert_mark(bond: &mut crate::document::Bond) -> bool {
    let display = match bond.display.as_str() {
        "wedge" => "hash",
        "hash" => "wedge",
        "hollow_wedge" => "hashed",
        "hashed" => "hollow_wedge",
        "bold" => "hash",
        _ => return false,
    };
    bond.display = display.into();
    true
}

fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Point, factor: f32) -> Point {
    Point::new(a.x * factor, a.y * factor)
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Collect a branch after cutting its bond to the centre. Reconnecting to a
/// retained neighbor would move the backbone or ring and is not a subtree edit.
fn branch(
    doc: &Document,
    center: u64,
    start: u64,
    retained: &HashSet<u64>,
) -> Result<HashSet<u64>, String> {
    let mut adjacent: HashMap<u64, Vec<u64>> = HashMap::new();
    for bond in &doc.bonds {
        if bond.a == center || bond.b == center {
            continue;
        }
        adjacent.entry(bond.a).or_default().push(bond.b);
        adjacent.entry(bond.b).or_default().push(bond.a);
    }
    let mut seen = HashSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        if retained.contains(&id) {
            return Err("The substituent reconnects to the ring or backbone; inversion cannot move it safely".into());
        }
        if seen.insert(id) {
            stack.extend(adjacent.get(&id).into_iter().flatten().copied());
        }
    }
    Ok(seen)
}

fn check_reflected_subtrees(
    doc: &Document,
    moved: &HashSet<u64>,
    haworth: &interchange::Recognized,
) -> Result<(), String> {
    if !moved.is_disjoint(&haworth.atoms) {
        return Err(
            "The substituent contains another Haworth ring; inversion cannot reflect it safely"
                .into(),
        );
    }
    if !moved.is_disjoint(&crate::fischer::marked_centers(doc)) {
        return Err(
            "The substituent contains a Fischer projection; convert it to a wedge drawing before inversion"
                .into(),
        );
    }
    Ok(())
}

fn invert_branch_marks(doc: &mut Document, moved: &HashSet<u64>) {
    // Reflection reverses screen parity throughout the branch. Reversing its
    // ordinary depth marks preserves every other centre's configuration.
    for bond in &mut doc.bonds {
        if moved.contains(&bond.a) && !bond.projection && bond.order == 1 {
            invert_mark(bond);
        }
    }
}

fn invert_haworth(
    doc: &mut Document,
    id: u64,
    recognized: &interchange::Recognized,
) -> Result<(), String> {
    interchange::export_bonds(doc).map_err(String::from)?;
    // The ring component supplies its affine screen axes. Using these axes
    // retains up/down recognition after a rotation, scale, or shear.
    let mut ring = HashSet::from([id]);
    loop {
        let before = ring.len();
        for &index in &recognized.bonds {
            let bond = doc.bonds.get(index).ok_or("Missing Haworth ring bond")?;
            if ring.contains(&bond.a) || ring.contains(&bond.b) {
                ring.extend([bond.a, bond.b]);
            }
        }
        if ring.len() == before {
            break;
        }
    }
    let front = doc
        .bonds
        .iter()
        .find(|b| b.display == "bold" && ring.contains(&b.a) && ring.contains(&b.b))
        .ok_or("Missing Haworth front edge")?;
    let tip = doc
        .bonds
        .iter()
        .find(|b| b.display == "wedge" && b.b == front.a && ring.contains(&b.a))
        .ok_or("Missing Haworth perspective edge")?;
    let position = |id| {
        doc.atom(id)
            .map(|a| a.position)
            .ok_or("Missing Haworth atom")
    };
    let (p0, p1, p2) = (position(tip.a)?, position(front.a)?, position(front.b)?);
    let half = if ring.len() == 5 { 0.65 } else { 0.5 };
    let x = mul(sub(p1, p2), 1. / (2. * half));
    let y = mul(sub(sub(p0, p1), mul(x, 1. - half)), -2.);
    let determinant = cross(x, y);
    if !determinant.is_finite() || determinant.abs() < f32::EPSILON {
        return Err("The Haworth ring plane is degenerate".into());
    }
    let center = position(id)?;
    let branches: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| other(b, id))
        .filter(|other| !ring.contains(other))
        .collect();
    let mut moved = HashSet::new();
    for &root in &branches {
        let members = branch(doc, id, root, &ring)?;
        if !moved.is_disjoint(&members) {
            return Err("The substituents are connected; inversion cannot move them safely".into());
        }
        moved.extend(members);
    }
    check_reflected_subtrees(doc, &moved, recognized)?;
    for atom in &mut doc.atoms {
        if moved.contains(&atom.id) {
            let height = cross(x, sub(atom.position, center)) / determinant;
            let shift = mul(y, -2. * height);
            atom.position = atom.position.offset(shift.x, shift.y);
            atom.display.hydrogen_position = match atom.display.hydrogen_position {
                crate::atom_labels::HydrogenPosition::Above => {
                    crate::atom_labels::HydrogenPosition::Below
                }
                crate::atom_labels::HydrogenPosition::Below => {
                    crate::atom_labels::HydrogenPosition::Above
                }
                other => other,
            };
        }
    }
    invert_branch_marks(doc, &moved);
    // Keep inward OH labels facing the open side of the ring. The affine ring
    // x coordinate distinguishes the left corner and right front carbon.
    let ring_midpoint = p1.lerp(p2, 0.5);
    let ring_x = cross(sub(center, ring_midpoint), y) / determinant;
    let is_anomeric = doc.bonds.iter().any(|b| {
        other(b, id).is_some_and(|neighbor| {
            ring.contains(&neighbor) && doc.atom(neighbor).is_some_and(|atom| atom.element == "O")
        })
    });
    for root in branches {
        let atom = doc.atom_mut(root).ok_or("Missing Haworth substituent")?;
        if atom.element == "O" && atom.label_h != 0 {
            let above = cross(x, sub(atom.position, center)) / determinant < 0.;
            atom.display.hydrogen_position =
                if ring_x < -0.9 || ring_x > 0. && above && !is_anomeric {
                    crate::atom_labels::HydrogenPosition::Left
                } else {
                    crate::atom_labels::HydrogenPosition::Right
                };
        }
    }
    Ok(())
}

fn invert_fischer(
    doc: &mut Document,
    cross: &crate::fischer::Cross,
    haworth: &interchange::Recognized,
) -> Result<(), String> {
    let retained = HashSet::from([cross.up, cross.down]);
    let left = branch(doc, cross.center, cross.left, &retained)?;
    let right = branch(doc, cross.center, cross.right, &retained)?;
    if !left.is_disjoint(&right) {
        return Err(
            "The Fischer horizontal substituents are connected; inversion cannot swap them safely"
                .into(),
        );
    }
    let mut moved = left;
    moved.extend(right);
    check_reflected_subtrees(doc, &moved, haworth)?;
    let center = doc
        .atom(cross.center)
        .ok_or("Missing Fischer centre")?
        .position;
    for atom in &mut doc.atoms {
        if moved.contains(&atom.id) {
            atom.position.x = 2. * center.x - atom.position.x;
            atom.display.hydrogen_position = match atom.display.hydrogen_position {
                crate::atom_labels::HydrogenPosition::Left => {
                    crate::atom_labels::HydrogenPosition::Right
                }
                crate::atom_labels::HydrogenPosition::Right => {
                    crate::atom_labels::HydrogenPosition::Left
                }
                other => other,
            };
        }
    }
    invert_branch_marks(doc, &moved);
    for (root, side) in [
        (cross.left, crate::atom_labels::HydrogenPosition::Right),
        (cross.right, crate::atom_labels::HydrogenPosition::Left),
    ] {
        if let Some(atom) = doc.atom_mut(root)
            && atom.label_h != 0
        {
            atom.display.hydrogen_position = side;
        }
    }
    for group in &mut doc.abbreviations {
        if moved.contains(&group.anchor) {
            use crate::abbreviations::LabelAlignment;
            group.alignment = match group.alignment {
                LabelAlignment::Left => LabelAlignment::Right,
                LabelAlignment::Right => LabelAlignment::Left,
                other => other,
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
