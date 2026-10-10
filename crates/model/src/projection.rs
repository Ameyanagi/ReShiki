//! Editable orthographic projections and nonchemical ring-centre anchors.
use crate::document::{Document, Point};
use std::collections::HashSet;
pub mod growth;

pub fn validate(doc: &Document) -> Result<(), String> {
    for atom in &doc.atoms {
        if !atom.depth.is_finite() || atom.depth.abs() > 1_000_000. {
            return Err("Invalid projection depth".into());
        }
        if !atom.centroid.is_empty()
            && (atom.element != "*"
                || atom.centroid.len() < 2
                || atom.centroid.len() > 300
                || atom.centroid.iter().collect::<HashSet<_>>().len() != atom.centroid.len()
                || atom.centroid.iter().any(|id| {
                    doc.atom(*id).is_none_or(|a| {
                        a.id == atom.id || a.element == "*" || !a.centroid.is_empty()
                    })
                }))
        {
            return Err("A centroid needs at least two distinct real atoms".into());
        }
    }
    if doc.graphics.iter().any(|g| {
        g.depth
            .iter()
            .any(|z| !z.is_finite() || z.abs() > 1_000_000.)
    }) {
        return Err("Invalid shape projection depth".into());
    }
    Ok(())
}

pub fn add_centroid(doc: &mut Document, ids: &[u64]) -> Result<u64, String> {
    let mut members: Vec<_> = ids
        .iter()
        .copied()
        .filter(|id| {
            doc.atom(*id)
                .is_some_and(|a| a.centroid.is_empty() && a.element != "*")
        })
        .collect();
    members.sort_unstable();
    members.dedup();
    if !(2..=300).contains(&members.len()) {
        return Err("Select at least two atoms to add a centroid".into());
    }
    let id = doc.add_atom("*", Point::default());
    let atom = doc.atom_mut(id).ok_or("Missing centroid")?;
    atom.no_implicit = true;
    atom.centroid = members;
    sync_centroids(doc);
    Ok(id)
}

pub fn sync_centroids(doc: &mut Document) {
    let positions: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| !a.centroid.is_empty() && a.attachment.is_none())
        .filter_map(|a| {
            let points: Option<Vec<_>> = a.centroid.iter().map(|id| doc.atom(*id)).collect();
            let points = points?;
            let n = points.len() as f32;
            Some((
                a.id,
                Point::new(
                    points.iter().map(|a| a.position.x).sum::<f32>() / n,
                    points.iter().map(|a| a.position.y).sum::<f32>() / n,
                ),
                points.iter().map(|a| a.depth).sum::<f32>() / n,
            ))
        })
        .collect();
    for (id, position, depth) in positions {
        if let Some(a) = doc.atom_mut(id) {
            a.position = position;
            a.depth = depth;
        }
    }
}

pub fn prune_centroids(doc: &mut Document) {
    let present: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    let removed: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| !a.centroid.is_empty() && a.centroid.iter().any(|id| !present.contains(id)))
        .map(|a| a.id)
        .collect();
    doc.atoms.retain(|a| !removed.contains(&a.id));
    doc.bonds
        .retain(|b| !removed.contains(&b.a) && !removed.contains(&b.b));
    sync_centroids(doc);
}

/// Rotate the retained XYZ positions, preserving topology and stereo descriptors.
/// Labels stay upright. Positive depth faces the viewer.
pub fn tilt(doc: &mut Document, ids: &[u64], degrees: f32, around_x: bool) {
    if !degrees.is_finite() || degrees.abs() > 85. || degrees == 0. {
        return;
    }
    let ids = doc.expand_abbreviation_selection(ids);
    let selected: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && (a.centroid.is_empty() || a.attachment.is_some()))
        .collect();
    let center = if selected.is_empty() {
        crate::editing::center(doc, &ids)
    } else {
        Point::new(
            selected.iter().map(|a| a.position.x).sum::<f32>() / selected.len() as f32,
            selected.iter().map(|a| a.position.y).sum::<f32>() / selected.len() as f32,
        )
    };
    let center_z = if selected.is_empty() {
        0.
    } else {
        selected.iter().map(|a| a.depth).sum::<f32>() / selected.len() as f32
    };
    let (sin, cos) = degrees.to_radians().sin_cos();
    let rotate = |p: Point, z: f32| {
        let x = p.x - center.x;
        let y = p.y - center.y;
        let z = z - center_z;
        if around_x {
            (
                center.offset(x, y * cos - z * sin),
                center_z + y * sin + z * cos,
            )
        } else {
            (
                center.offset(x * cos + z * sin, y),
                center_z - x * sin + z * cos,
            )
        }
    };
    for a in &mut doc.atoms {
        if ids.contains(&a.id) && (a.centroid.is_empty() || a.attachment.is_some()) {
            (a.position, a.depth) = rotate(a.position, a.depth);
        }
    }
    for g in &mut doc.graphics {
        if !ids.contains(&g.id) {
            continue;
        }
        let [oz, xz, yz] = g.depth;
        let (o, zo) = rotate(g.origin, oz);
        let (x, zx) = rotate(g.origin.offset(g.axis_x.x, g.axis_x.y), oz + xz);
        let (y, zy) = rotate(g.origin.offset(g.axis_y.x, g.axis_y.y), oz + yz);
        g.origin = o;
        g.axis_x = Point::new(x.x - o.x, x.y - o.y);
        g.axis_y = Point::new(y.x - o.x, y.y - o.y);
        g.depth = [zo, zx - zo, zy - zo];
    }
    sync_centroids(doc);
    refresh_depth_bonds(doc, &ids);
    crate::arrow_anchors::reconcile(doc);
}

/// Emphasize single, double and aromatic outlines without rewriting stereo or order.
pub fn depth_bonds(doc: &mut Document, ids: &[u64]) {
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && a.centroid.is_empty())
        .collect();
    if atoms.is_empty() {
        return;
    }
    let mean = atoms.iter().map(|a| a.depth).sum::<f32>() / atoms.len() as f32;
    let depth: std::collections::HashMap<_, _> = atoms.iter().map(|a| (a.id, a.depth)).collect();
    for b in &mut doc.bonds {
        if matches!(b.order, 1 | 2 | 4)
            && matches!(b.display.as_str(), "plain" | "bold")
            && b.secondary_display.as_deref() != Some("dashed")
            && let (Some(a), Some(z)) = (depth.get(&b.a), depth.get(&b.b))
        {
            b.projection = true;
            b.display = if (a + z) / 2. > mean + 0.01 {
                "bold"
            } else {
                "plain"
            }
            .into();
            if b.order == 2 {
                // Use the established bold-double appearance: emphasize the
                // main rail while retaining a separate thin second rail.
                b.secondary_display = Some("plain".into());
            }
        }
    }
}

/// Recompute already-marked perspective outlines after a 3D rotation. Work
/// per connected outline so separate ligands have independent near/far planes.
/// Ordinary stereochemical wedges and unmarked bonds are never restyled.
pub fn refresh_depth_bonds(doc: &mut Document, ids: &[u64]) {
    use std::collections::HashMap;
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    let eligible = |b: &crate::document::Bond| {
        b.projection
            && !doc.depth_appearance.iter().any(|scope| {
                !scope.automatic && scope.atoms.contains(&b.a) && scope.atoms.contains(&b.b)
            })
            && matches!(b.order, 1 | 2 | 4)
            && matches!(b.display.as_str(), "plain" | "bold" | "wedge")
            && b.secondary_display.as_deref() != Some("dashed")
            && [b.a, b.b]
                .iter()
                .all(|id| doc.atom(*id).is_some_and(|a| a.centroid.is_empty()))
    };
    let edges: HashSet<_> = doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| eligible(b))
        .map(|(i, b)| {
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
            i
        })
        .collect();
    let mut near = HashMap::new();
    let mut visited = HashSet::new();
    for start in adjacent.keys() {
        if visited.contains(start) {
            continue;
        }
        let mut members = HashSet::new();
        let mut pending = vec![*start];
        while let Some(id) = pending.pop() {
            if members.insert(id) {
                pending.extend(adjacent.get(&id).into_iter().flatten().copied());
            }
        }
        visited.extend(members.iter().copied());
        if !ids.iter().any(|id| members.contains(id)) {
            continue;
        }
        let mut members: Vec<_> = members.into_iter().collect();
        members.sort_unstable();
        let depths: Vec<_> = members
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.id, a.depth)))
            .collect();
        if depths.is_empty() {
            continue;
        }
        let mean = depths.iter().map(|(_, z)| z).sum::<f32>() / depths.len() as f32;
        for (id, z) in depths {
            near.insert(id, z - mean);
        }
    }
    for (i, bond) in doc.bonds.iter_mut().enumerate() {
        if !edges.contains(&i) {
            continue;
        }
        let (Some(a), Some(b)) = (near.get(&bond.a), near.get(&bond.b)) else {
            continue;
        };
        if bond.order == 2 {
            // Double bonds have no tapered-wedge display. Their midpoint
            // determines foreground emphasis, as in the initial command.
            // Keep endpoint orientation, E/Z references and rail placement.
            bond.display = if (a + b) / 2. > 0.01 { "bold" } else { "plain" }.into();
            bond.secondary_display = Some("plain".into());
            continue;
        }
        let (a, b) = (*a > 0.01, *b > 0.01);
        bond.display = if a == b {
            // Stable orientation makes rotation followed by its inverse restore
            // the same document, including endpoints of non-stereo outlines.
            if bond.a > bond.b {
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
            if a { "bold" } else { "plain" }
        } else {
            if a {
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
            "wedge"
        }
        .into();
    }
}

#[cfg(test)]
mod tests;
