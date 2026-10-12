//! Bounded variable-substituent placement and ink clearance.
use super::{
    Work,
    budget::charge_clearance,
    clearance::Footprint,
    geometry::{at, at_mut, finite, include},
    ink::{CAPSULE_SCALE, bond_paint_radius, highlight_radius},
};
use crate::document::{Document, Point};
use std::collections::HashMap;

/// Pose one detachable substituent before replacing its virtual handle bond.
/// The host stays fixed. A supporting half-plane separates its ring/backbone;
/// fixed exterior branches additionally require a prepaid clearance proof.
/// Shared/bridged or ambiguous placements retain the source instead of invoking
/// whole-component cleanup. All scans, storage and transforms are prepaid.
pub(super) fn pose_variable(
    doc: &mut Document,
    handle: u64,
    target: u64,
    root: u64,
    work: &mut Work,
) -> Result<(), String> {
    let references = doc
        .atoms
        .iter()
        .map(|a| {
            a.centroid
                .len()
                .saturating_add(a.stereo.as_ref().map_or(0, |s| s.neighbors.len()))
        })
        .chain(doc.bonds.iter().map(|b| b.stereo_atoms.len()))
        .fold(0usize, usize::saturating_add);
    work.charge(
        doc.atoms
            .len()
            .saturating_add(doc.bonds.len())
            .saturating_add(references)
            .saturating_mul(64),
    )?;
    let shared = || {
        "Variable attachment requires a detachable substituent with one anchor; the source drawing is retained".to_owned()
    };
    let exterior = || {
        "Variable attachment has no supported exterior placement; the source drawing is retained"
            .to_owned()
    };
    let indices: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let h = *indices.get(&handle).ok_or("Missing variable handle")?;
    let t = *indices.get(&target).ok_or("Missing attachment target")?;
    let r = *indices.get(&root).ok_or("Missing substituent root")?;
    if h == t || h == r || t == r {
        return Err(shared());
    }
    let mut adjacent = vec![Vec::new(); doc.atoms.len()];
    for b in &doc.bonds {
        let a = *indices.get(&b.a).ok_or("Missing variable bond endpoint")?;
        let z = *indices.get(&b.b).ok_or("Missing variable bond endpoint")?;
        if a != h && z != h {
            at_mut(&mut adjacent, a)?.push(z);
            at_mut(&mut adjacent, z)?.push(a);
        }
    }
    let component = |start| -> Result<Vec<bool>, String> {
        let mut found = vec![false; doc.atoms.len()];
        let mut pending = vec![start];
        *at_mut(&mut found, start)? = true;
        while let Some(i) = pending.pop() {
            for &j in at(&adjacent, i)? {
                if !*at(&found, j)? {
                    *at_mut(&mut found, j)? = true;
                    pending.push(j);
                }
            }
        }
        Ok(found)
    };
    let moving = component(r)?;
    for id in &at(&doc.atoms, h)?.centroid {
        let i = *indices.get(id).ok_or("Missing variable candidate")?;
        if *at(&moving, i)? {
            return Err(shared());
        }
    }
    if *at(&moving, t)? {
        return Err(shared());
    }
    // Another semantic handle or a derived centroid sharing the moving branch
    // would need a joint placement policy, not independent one-anchor poses.
    for (i, atom) in doc.atoms.iter().enumerate() {
        if i == h {
            continue;
        }
        if *at(&moving, i)? && (atom.attachment.is_some() || !atom.centroid.is_empty()) {
            return Err(shared());
        }
        for id in &atom.centroid {
            let j = *indices.get(id).ok_or("Missing centroid member")?;
            if *at(&moving, j)? {
                return Err(shared());
            }
        }
    }
    let moving_ids: Vec<_> = doc
        .atoms
        .iter()
        .zip(&moving)
        .filter(|(_, moved)| **moved)
        .map(|(a, _)| a.id)
        .collect();
    super::reject_affected_stereo(doc, &moving_ids)?;
    let delta = |a: Point, b: Point| {
        (
            f64::from(a.x) - f64::from(b.x),
            f64::from(a.y) - f64::from(b.y),
        )
    };
    let unit = |(x, y): (f64, f64)| -> Result<(f64, f64), String> {
        let length = x.hypot(y);
        if !length.is_finite() || length < 0.000001 {
            return Err(exterior());
        }
        Ok((x / length, y / length))
    };
    let center = at(&doc.atoms, t)?.position;
    let old_root = at(&doc.atoms, r)?.position;
    finite(center)?;
    finite(old_root)?;
    let authored_incoming = unit(delta(old_root, at(&doc.atoms, h)?.position))?;
    let lone_neighbor = match at(&adjacent, r)?.as_slice() {
        [neighbor] => Some(at(&doc.atoms, *neighbor)?),
        _ => None,
    };
    let incoming = if let Some(neighbor) = lone_neighbor
        && matches!(at(&doc.atoms, r)?.element.as_str(), "C" | "N" | "O" | "S")
        && doc.bonds.iter().any(|b| {
            b.order == 1
                && ((b.a == root && b.b == neighbor.id) || (b.b == root && b.a == neighbor.id))
        }) {
        // A virtual centroid is not a chemical bond direction. For an ordinary
        // two-coordinate root, use the conventional 120-degree drawing angle,
        // preserving the authored turn side by a proper rotation only. There
        // is no search, internal cleanup, reflection or bond-length change.
        let branch = unit(delta(neighbor.position, old_root))?;
        let turn = if authored_incoming.0 * branch.1 - authored_incoming.1 * branch.0 < 0. {
            -1.
        } else {
            1.
        };
        let sine = turn * (3_f64.sqrt() * 0.5);
        (
            branch.0 * 0.5 + branch.1 * sine,
            branch.1 * 0.5 - branch.0 * sine,
        )
    } else {
        authored_incoming
    };
    let outward = match at(&adjacent, t)?.as_slice() {
        [] => incoming,
        [a] => {
            let (x, y) = unit(delta(at(&doc.atoms, *a)?.position, center))?;
            (-x, -y)
        }
        [a, b] => {
            let first = unit(delta(at(&doc.atoms, *a)?.position, center))?;
            let second = unit(delta(at(&doc.atoms, *b)?.position, center))?;
            // A straight site has no strict inward certificate; refuse it
            // rather than accept a boundary/collinear contact as separated.
            unit((-first.0 - second.0, -first.1 - second.1))?
        }
        _ => return Err(exterior()),
    };
    let length = f64::from(doc.drawing_style.bond_length_world);
    if !length.is_finite() || length <= 0. {
        return Err(exterior());
    }
    let tolerance = (length * 0.00001).max(0.0001);
    let side = |position: Point| {
        let d = delta(position, center);
        let x = d.0 * outward.0;
        let y = d.1 * outward.1;
        // Outward's stored f64 coefficients define the separating line.
        // Bound subtraction, products and addition conservatively; a near-
        // boundary point is ambiguous, not admitted into a positive strip.
        let error = ((f64::from(position.x).abs() + f64::from(center.x).abs()) * outward.0.abs()
            + (f64::from(position.y).abs() + f64::from(center.y).abs()) * outward.1.abs()
            + x.abs()
            + y.abs())
            * (f64::EPSILON * 8.);
        (x + y, error)
    };
    let host = component(t)?;
    // Link/repeat depiction would later move this component again. There is
    // no simultaneous anchored pose policy: refuse rather than certify a pose
    // that a later whole-component depiction can invalidate.
    let mixed = |id: &u64| {
        indices
            .get(id)
            .is_some_and(|&i| host.get(i) == Some(&true) || moving.get(i) == Some(&true))
    };
    work.charge(
        doc.generic.links.len().saturating_add(
            doc.generic
                .repeats
                .iter()
                .map(|r| r.members.len())
                .fold(0usize, usize::saturating_add),
        ),
    )?;
    if doc.generic.links.iter().any(|link| mixed(&link.atom))
        || doc
            .generic
            .repeats
            .iter()
            .any(|repeat| repeat.members.iter().any(mixed))
    {
        return Err(shared());
    }
    let mut all_inward = true;
    for (i, (atom, fixed)) in doc.atoms.iter().zip(&host).enumerate() {
        if *fixed && i != t {
            finite(atom.position)?;
            let (distance, error) = side(atom.position);
            all_inward &= distance < -error;
        }
    }
    if !all_inward {
        // An adjacent ordinary substituent can lie on/outside the ring's
        // tangent without crossing the proposed branch. Peel only terminal
        // branches: the remaining ring/backbone must still be strictly inward.
        // An acyclic or concave/bridged ambiguous host gets no such exemption.
        let mut core = host.clone();
        let mut degrees: Vec<_> = adjacent.iter().map(Vec::len).collect();
        let mut leaves: Vec<_> = degrees
            .iter()
            .enumerate()
            .filter(|(i, degree)| core.get(*i) == Some(&true) && **degree < 2)
            .map(|(i, _)| i)
            .collect();
        while let Some(i) = leaves.pop() {
            if !*at(&core, i)? {
                continue;
            }
            *at_mut(&mut core, i)? = false;
            for &j in at(&adjacent, i)? {
                if *at(&core, j)? {
                    let degree = at_mut(&mut degrees, j)?;
                    *degree = degree.saturating_sub(1);
                    if *degree < 2 {
                        leaves.push(j);
                    }
                }
            }
        }
        if !*at(&core, t)? {
            return Err(exterior());
        }
        for (i, atom) in doc.atoms.iter().enumerate() {
            if i != t && *at(&core, i)? {
                let (distance, error) = side(atom.position);
                if distance >= -error {
                    return Err(exterior());
                }
            }
        }
        // Pay for all possible point/segment comparisons before constructing
        // destinations or examining any pair. The same aggregate Work is used.
        charge_clearance(doc, work)?;
    }
    let cosine = incoming.0 * outward.0 + incoming.1 * outward.1;
    let sine = incoming.0 * outward.1 - incoming.1 * outward.0;
    let new_root = (
        f64::from(center.x) + length * outward.0,
        f64::from(center.y) + length * outward.1,
    );
    let precision = || {
        "Variable attachment cannot preserve bond geometry at this coordinate precision; the source drawing is retained".to_owned()
    };
    let mut positions = vec![None; doc.atoms.len()];
    for (i, atom) in doc.atoms.iter().enumerate() {
        if !*at(&moving, i)? {
            continue;
        }
        let (x, y) = delta(atom.position, old_root);
        let ideal = (
            new_root.0 + cosine * x - sine * y,
            new_root.1 + sine * x + cosine * y,
        );
        let position = Point::new(ideal.0 as f32, ideal.1 as f32);
        finite(position)?;
        let rounding = (f64::from(position.x) - ideal.0).hypot(f64::from(position.y) - ideal.1);
        if rounding > tolerance {
            return Err(precision());
        }
        let (distance, error) = side(position);
        if distance <= error {
            return Err(exterior());
        }
        *at_mut(&mut positions, i)? = Some(position);
    }
    let installed_root = at(&positions, r)?.ok_or("Missing posed substituent root")?;
    let connecting = delta(installed_root, center);
    if (connecting.0.hypot(connecting.1) - length).abs() > tolerance {
        return Err(precision());
    }
    for bond in &doc.bonds {
        let a = *indices
            .get(&bond.a)
            .ok_or("Missing variable bond endpoint")?;
        let b = *indices
            .get(&bond.b)
            .ok_or("Missing variable bond endpoint")?;
        if *at(&moving, a)? && *at(&moving, b)? {
            let old = delta(at(&doc.atoms, a)?.position, at(&doc.atoms, b)?.position);
            let new = delta(
                at(&positions, a)?.ok_or("Missing posed branch endpoint")?,
                at(&positions, b)?.ok_or("Missing posed branch endpoint")?,
            );
            if (old.0.hypot(old.1) - new.0.hypot(new.1)).abs() > tolerance * 2. {
                return Err(precision());
            }
        }
    }
    if !all_inward {
        let radii: Vec<_> = doc
            .bonds
            .iter()
            .map(|b| variable_bond_radius(doc, b))
            .collect();
        let contact = doc
            .bonds
            .iter()
            .enumerate()
            .find(|(_, b)| (b.a == handle && b.b == root) || (b.b == handle && b.a == root))
            .ok_or("Missing variable connecting bond")?;
        let pad = *at(&radii, contact.0)?;
        let connector = Footprint::segment(center, installed_root, pad);
        for (i, atom) in doc.atoms.iter().enumerate() {
            if *at(&host, i)? && i != t && !connector.separated(Footprint::point(atom.position, 0.))
            {
                return Err(exterior());
            }
        }
        for (edge_index, edge) in doc.bonds.iter().enumerate() {
            let a = *indices.get(&edge.a).ok_or("Missing host endpoint")?;
            let b = *indices.get(&edge.b).ok_or("Missing host endpoint")?;
            if !*at(&host, a)? || !*at(&host, b)? {
                continue;
            }
            let fixed = Footprint::segment(
                at(&doc.atoms, a)?.position,
                at(&doc.atoms, b)?.position,
                *at(&radii, edge_index)?,
            );
            if a != t && b != t && !connector.separated(fixed) {
                return Err(exterior());
            }
            for (i, position) in positions.iter().enumerate() {
                if let Some(position) = position
                    && *at(&moving, i)?
                    && !Footprint::point(*position, 0.).separated(fixed)
                {
                    return Err(exterior());
                }
            }
            for (branch_index, branch) in doc.bonds.iter().enumerate() {
                let a = *indices.get(&branch.a).ok_or("Missing branch endpoint")?;
                let b = *indices.get(&branch.b).ok_or("Missing branch endpoint")?;
                if *at(&moving, a)?
                    && *at(&moving, b)?
                    && !Footprint::segment(
                        at(&positions, a)?.ok_or("Missing posed endpoint")?,
                        at(&positions, b)?.ok_or("Missing posed endpoint")?,
                        *at(&radii, branch_index)?,
                    )
                    .separated(fixed)
                {
                    return Err(exterior());
                }
            }
        }
    }
    // No fallible work remains once the first destination is installed.
    for (atom, position) in doc.atoms.iter_mut().zip(positions) {
        if let Some(position) = position {
            atom.position = position;
        }
    }
    Ok(())
}

pub(super) fn variable_ink_clearance(
    source: &Document,
    doc: &Document,
    boxes: &[Option<(Point, Point)>],
) -> Result<(), String> {
    let collision = || {
        "Variable attachment cannot clear the preserved bonds and atom labels; the source drawing is retained".to_owned()
    };
    let indices: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, atom)| {
            let mut bounds = *at(boxes, i)?;
            if crate::highlights::atom_color(doc, atom).is_some() {
                let radius = highlight_radius(&doc.drawing_style);
                let (left, right, rx, ry) = if let Some((lo, hi)) = bounds {
                    let ry = (hi.y - lo.y + radius) * 0.5;
                    let y = (lo.y + hi.y) * 0.5;
                    (Point::new(lo.x, y), Point::new(hi.x, y), ry * 0.5, ry)
                } else {
                    (atom.position, atom.position, radius, radius)
                };
                include(
                    &mut bounds,
                    left.offset(-rx * CAPSULE_SCALE, -ry * CAPSULE_SCALE),
                    right.offset(rx * CAPSULE_SCALE, ry * CAPSULE_SCALE),
                );
            }
            if let Some((lo, hi)) = bounds {
                finite(lo)?;
                finite(hi)?;
            }
            Ok(bounds.map_or_else(
                || Footprint::point(atom.position, 0.),
                |bounds| Footprint::rectangle(bounds, 0.),
            ))
        })
        .collect::<Result<_, String>>()?;
    let mut adjacent = vec![Vec::new(); doc.atoms.len()];
    let mut edges = Vec::with_capacity(doc.bonds.len());
    for bond in &doc.bonds {
        let a = *indices.get(&bond.a).ok_or("Missing generated endpoint")?;
        let b = *indices.get(&bond.b).ok_or("Missing generated endpoint")?;
        at_mut(&mut adjacent, a)?.push(b);
        at_mut(&mut adjacent, b)?.push(a);
        edges.push((
            a,
            b,
            Footprint::segment(
                at(&doc.atoms, a)?.position,
                at(&doc.atoms, b)?.position,
                variable_bond_radius(doc, bond),
            ),
        ));
    }
    for variable in source.atoms.iter().filter(|a| a.is_variable_attachment()) {
        let contact = source
            .bonds
            .iter()
            .find(|b| b.a == variable.id || b.b == variable.id)
            .ok_or("Missing source variable bond")?;
        let root = if contact.a == variable.id {
            contact.b
        } else {
            contact.a
        };
        let root = *indices
            .get(&root)
            .ok_or("Variable root identity changed during expansion")?;
        let mut targets = at(&adjacent, root)?.iter().copied().filter(|&i| {
            doc.atoms
                .get(i)
                .is_some_and(|a| variable.centroid.contains(&a.id))
        });
        let target = targets.next().ok_or("Missing generated variable contact")?;
        if targets.next().is_some() {
            return Err(collision());
        }
        let component = |start| -> Result<Vec<bool>, String> {
            let mut found = vec![false; doc.atoms.len()];
            let mut pending = vec![start];
            *at_mut(&mut found, start)? = true;
            while let Some(i) = pending.pop() {
                for &j in at(&adjacent, i)? {
                    if (i == root && j == target) || (i == target && j == root) {
                        continue;
                    }
                    if !*at(&found, j)? {
                        *at_mut(&mut found, j)? = true;
                        pending.push(j);
                    }
                }
            }
            Ok(found)
        };
        let moving = component(root)?;
        let host = component(target)?;
        if *at(&moving, target)? {
            return Err(collision());
        }
        // Custom off-site marks/indicators are not clipped by a new bond like
        // the atom's chemical label. No supported junction policy exists.
        let junction = at(&doc.atoms, target)?;
        if !junction.marks.is_empty()
            || junction.display.number.is_some()
            || (junction.map_num != 0
                && junction
                    .display
                    .mapping
                    .show
                    .unwrap_or(doc.atom_labels.maps))
        {
            return Err(collision());
        }
        for (i, moved) in moving.iter().enumerate() {
            if !moved {
                continue;
            }
            for (j, fixed) in host.iter().enumerate() {
                if *fixed && !at(&atoms, i)?.separated(*at(&atoms, j)?) {
                    return Err(collision());
                }
            }
        }
        for &(a, b, branch) in &edges {
            let connector = (a == root && b == target) || (b == root && a == target);
            if !connector && !(*at(&moving, a)? && *at(&moving, b)?) {
                continue;
            }
            if connector {
                // The host half-plane cannot separate the connector from a
                // folded part of its own substituent. Check every nonroot ink
                // footprint and nonincident internal bond before publishing.
                // These linear scans fit the already prepaid full pair bound.
                for (i, moved) in moving.iter().enumerate() {
                    if *moved && i != root && !branch.separated(*at(&atoms, i)?) {
                        return Err(collision());
                    }
                }
                for &(c, d, internal) in &edges {
                    if *at(&moving, c)?
                        && *at(&moving, d)?
                        && c != root
                        && d != root
                        && !branch.separated(internal)
                    {
                        return Err(collision());
                    }
                }
            }
            for (i, fixed) in host.iter().enumerate() {
                // The chemical label is clipped at its own bond junction.
                if *fixed && !(connector && i == target) && !branch.separated(*at(&atoms, i)?) {
                    return Err(collision());
                }
            }
            for &(c, d, fixed) in &edges {
                if *at(&host, c)?
                    && *at(&host, d)?
                    && !(connector && (c == target || d == target))
                    && !branch.separated(fixed)
                {
                    return Err(collision());
                }
            }
        }
        for (i, moved) in moving.iter().enumerate() {
            if !moved {
                continue;
            }
            for &(a, b, fixed) in &edges {
                if *at(&host, a)? && *at(&host, b)? && !at(&atoms, i)?.separated(fixed) {
                    return Err(collision());
                }
            }
        }
    }
    Ok(())
}

fn variable_bond_radius(doc: &Document, bond: &crate::document::Bond) -> f64 {
    let style = &doc.drawing_style;
    let ordinary = |b: &crate::document::Bond| {
        matches!(b.order, 1..=4 | 7)
            && b.display == "plain"
            && b.secondary_display.as_deref().is_none_or(|s| s == "plain")
            && b.highlight.is_none()
    };
    if !ordinary(bond)
        || doc.bonds.iter().any(|b| {
            (b.a == bond.a || b.a == bond.b || b.b == bond.a || b.b == bond.b) && !ordinary(b)
        })
    {
        return f64::from(bond_paint_radius(style));
    }
    // Plain joined outlines clamp miter distance to four half line-widths.
    // Double/triple/aromatic rails lie within one spacing plus half a line.
    // Broader/styled/highlighted incident bonds use the global bound above.
    let rail = if bond.order == 1 {
        0.
    } else {
        style.bond_length_world * style.bond_spacing_ratio
    };
    f64::from((rail + style.line_width() * 0.5).max(style.line_width() * 2.))
}

#[cfg(test)]
mod tests;
