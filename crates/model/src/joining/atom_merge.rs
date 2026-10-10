//! Join three or more selected atoms without moving the surrounding fragments.
use crate::document::{Atom, Bond, Document, Point};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Retain the first selected atom at the selected positions' bounds center.
/// The input stays untouched on success and failure; hosts can commit once.
pub fn merge_atoms(doc: &Document, selected: &[u64]) -> Result<(Document, Vec<u64>), String> {
    merge_atoms_with_position(doc, selected, None)
}

/// Merge at a caller-supplied finite position, for example the center of the
/// selected displayed labels. Projection depth still uses the selected mean.
pub fn merge_atoms_at(
    doc: &Document,
    selected: &[u64],
    position: Point,
) -> Result<(Document, Vec<u64>), String> {
    if !position.x.is_finite() || !position.y.is_finite() {
        return Err("The atom merge position must be finite.".into());
    }
    merge_atoms_with_position(doc, selected, Some(position))
}

fn merge_atoms_with_position(
    doc: &Document,
    selected: &[u64],
    position: Option<Point>,
) -> Result<(Document, Vec<u64>), String> {
    doc.validate()?;
    let selected_set: HashSet<_> = selected.iter().copied().collect();
    if selected.len() < 3 || selected_set.len() != selected.len() {
        return Err("Select at least three distinct atoms to merge.".into());
    }
    let atoms: Vec<_> = selected
        .iter()
        .map(|id| doc.atom(*id).ok_or("Select only atoms to merge."))
        .collect::<Result<_, _>>()?;
    let first = atoms.first().copied().ok_or("Select atoms to merge.")?;
    for atom in &atoms {
        if doc
            .abbreviations
            .iter()
            .any(|group| group.members.contains(&atom.id))
        {
            return Err("Expand abbreviations before merging their atoms.".into());
        }
        if !atom.centroid.is_empty() || atom.attachment.is_some() {
            return Err("Select chemical atoms, rather than centroid or attachment points.".into());
        }
        if atom.stereo.is_some() {
            return Err(
                "Merging a stereocenter needs an explicit stereochemistry edit first.".into(),
            );
        }
        metadata_compatible(first, atom)?;
    }
    for bond in &doc.bonds {
        if (selected_set.contains(&bond.a) || selected_set.contains(&bond.b))
            && (bond.stereo.is_some()
                || bond.stereo_authoritative
                || !bond.projection
                    && matches!(
                        bond.display.as_str(),
                        "wedge" | "hollow_wedge" | "hash" | "wavy"
                    ))
        {
            return Err("Merging atoms on a stereochemical bond needs an explicit stereochemistry edit first.".into());
        }
    }
    let survivor = first.id;
    let mapped = |id| {
        if selected_set.contains(&id) {
            survivor
        } else {
            id
        }
    };
    // Compute the midpoint in f64 so finite coordinate bounds cannot overflow.
    let center = position.unwrap_or_else(|| {
        let mut lo = first.position;
        let mut hi = first.position;
        for atom in &atoms {
            lo.x = lo.x.min(atom.position.x);
            lo.y = lo.y.min(atom.position.y);
            hi.x = hi.x.max(atom.position.x);
            hi.y = hi.y.max(atom.position.y);
        }
        Point::new(
            ((f64::from(lo.x) + f64::from(hi.x)) / 2.) as f32,
            ((f64::from(lo.y) + f64::from(hi.y)) / 2.) as f32,
        )
    });
    // Projection depth retains its prior mean; it is independent of XY bounds.
    let n = atoms.len() as f64;
    let depth = (atoms.iter().map(|a| f64::from(a.depth)).sum::<f64>() / n) as f32;
    let mut result = doc.clone();
    result
        .atoms
        .retain(|a| !selected_set.contains(&a.id) || a.id == survivor);
    for atom in &mut result.atoms {
        if atom.id == survivor {
            atom.position = center;
            atom.depth = depth;
            atom.label_h = 0;
        }
        if let Some(stereo) = &mut atom.stereo {
            for id in &mut stereo.neighbors {
                *id = mapped(*id);
            }
            if stereo
                .neighbors
                .iter()
                .copied()
                .collect::<HashSet<_>>()
                .len()
                != stereo.neighbors.len()
            {
                return Err("Joining would collapse distinct stereocenter neighbors.".into());
            }
        }
        for id in &mut atom.centroid {
            *id = mapped(*id);
        }
        dedup(&mut atom.centroid);
        if !atom.centroid.is_empty() && atom.centroid.len() < 2 {
            return Err(
                "Joining would collapse a centroid or attachment target set to one atom.".into(),
            );
        }
    }
    let mut bonds: Vec<Bond> = Vec::new();
    let mut pairs = HashMap::new();
    for mut bond in result.bonds.drain(..) {
        bond.a = mapped(bond.a);
        bond.b = mapped(bond.b);
        if bond.a == bond.b {
            continue;
        }
        for id in &mut bond.stereo_atoms {
            *id = mapped(*id);
        }
        if bond.stereo.is_some()
            && (bond
                .stereo_atoms
                .iter()
                .copied()
                .collect::<HashSet<_>>()
                .len()
                != bond.stereo_atoms.len()
                || bond.stereo_atoms.contains(&bond.a)
                || bond.stereo_atoms.contains(&bond.b))
        {
            return Err("Joining would collapse distinct double-bond stereo references.".into());
        }
        let pair = (bond.a.min(bond.b), bond.a.max(bond.b));
        if let Some(index) = pairs.get(&pair).copied() {
            let existing = bonds.get(index).ok_or("Missing joined bond.")?;
            if !same_bond(existing, &bond) {
                return Err("Joining would combine bonds with conflicting order, direction, stereo or appearance. Edit those bonds first.".into());
            }
        } else {
            pairs.insert(pair, bonds.len());
            bonds.push(bond);
        }
    }
    result.bonds = bonds;
    for group in &mut result.groups {
        for id in &mut group.members {
            *id = mapped(*id);
        }
        dedup(&mut group.members);
    }
    for reaction in &mut result.reactions {
        for role in crate::reactions::Role::ALL {
            for participant in reaction.participants_mut(role) {
                for id in &mut participant.atoms {
                    *id = mapped(*id);
                }
                dedup(&mut participant.atoms);
            }
        }
    }
    for fill in &mut result.ring_fills {
        for id in &mut fill.atoms {
            *id = mapped(*id);
        }
    }
    crate::ring_fills::validate(&result)
        .map_err(|_| "Joining would collapse or duplicate a painted ring. Clear its fill first.")?;
    for scope in &mut result.depth_appearance {
        for id in &mut scope.atoms {
            *id = mapped(*id);
        }
        dedup(&mut scope.atoms);
        scope.weights = remap_weights(&scope.weights, &mapped)?;
        scope.overrides = remap_weights(&scope.overrides, &mapped)?;
    }
    crate::depth_appearance::validate(&result)
        .map_err(|_| "Joining would combine conflicting depth appearance scopes. Clear their depth appearance first.")?;
    crate::projection::sync_centroids(&mut result);
    result.reconcile_molecule_groups();
    crate::reactions::reconcile(&mut result)?;
    survivor_valence(&result, survivor)?;
    crate::atom_labels::clear_computed(&mut result);
    result.validate()?;
    Ok((result, vec![survivor]))
}

fn metadata_compatible(first: &Atom, atom: &Atom) -> Result<(), String> {
    for (conflict, label) in [
        (first.charge != atom.charge, "charges"),
        (first.isotope != atom.isotope, "isotopes"),
        (
            first.radical_electrons != atom.radical_electrons,
            "radicals",
        ),
        (
            first.explicit_h != atom.explicit_h || first.no_implicit != atom.no_implicit,
            "fixed hydrogens",
        ),
        (first.aromatic != atom.aromatic, "aromatic states"),
        (first.map_num != atom.map_num, "atom maps"),
        (first.marks != atom.marks, "atom marks"),
    ] {
        if conflict {
            return Err(format!(
                "Selected atoms have conflicting {label}. Edit them before joining; the first atom's label and color are retained."
            ));
        }
    }
    Ok(())
}

fn dedup(ids: &mut Vec<u64>) {
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(*id));
}

fn same_bond(a: &Bond, b: &Bond) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    a.cip_label = None;
    b.cip_label = None;
    if a.a != b.a {
        if a.order == 5 || matches!(a.display.as_str(), "wedge" | "hollow_wedge" | "hash") {
            return false;
        }
        b.reverse();
    }
    a == b
}

fn remap_weights(
    weights: &BTreeMap<u64, f32>,
    mapped: &impl Fn(u64) -> u64,
) -> Result<BTreeMap<u64, f32>, String> {
    let mut result = BTreeMap::new();
    for (id, weight) in weights {
        if result
            .insert(mapped(*id), *weight)
            .is_some_and(|previous| previous != *weight)
        {
            return Err(
                "Selected atoms have conflicting depth paint. Clear their depth appearance first."
                    .into(),
            );
        }
    }
    Ok(result)
}

/// Check only the surviving atom's local valence; unrelated chemistry or
/// nonchemical attachment nodes must not prevent this local drawing edit.
fn survivor_valence(doc: &Document, survivor: u64) -> Result<(), String> {
    use crate::chemistry::{
        ELEMENTS,
        graph::{Atom, Bond, Graph},
    };
    let atom = doc.atom(survivor).ok_or("Missing joined atom.")?;
    let number = ELEMENTS
        .iter()
        .position(|e| e.symbol == atom.element)
        .ok_or_else(|| format!("Unknown element {} at the joined atom.", atom.element))?;
    let mut graph = Graph {
        atoms: vec![Atom {
            atomic_number: u8::try_from(number).map_err(|_| "Invalid joined element.")?,
            isotope: u16::try_from(atom.isotope)
                .map_err(|_| "Joined isotope exceeds its supported range.")?,
            charge: i8::try_from(atom.charge)
                .map_err(|_| "Joined charge exceeds its supported range.")?,
            explicit_hydrogens: u8::try_from(atom.explicit_h)
                .map_err(|_| "Joined hydrogen count exceeds its supported range.")?,
            no_implicit: atom.no_implicit,
            aromatic: atom.aromatic,
            radical_electrons: atom.radical_electrons,
        }],
        bonds: vec![],
    };
    for bond in doc
        .bonds
        .iter()
        .filter(|b| b.a == survivor || b.b == survivor)
    {
        let index = graph.atoms.len();
        graph.atoms.push(Atom::default());
        let (a, b) = if bond.a == survivor {
            (0, index)
        } else {
            (index, 0)
        };
        graph.bonds.push(Bond {
            a,
            b,
            order: bond.order,
            aromatic: bond.order == 4,
        });
    }
    graph
        .valences()
        .map_err(|error| format!("Cannot join these atoms: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
