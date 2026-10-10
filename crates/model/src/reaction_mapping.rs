//! Chemical atom correspondence, independent of printed custom atom numbers.
//! Automatic proposals are detached, bounded, and require explicit acceptance.
use crate::{
    document::{Document, Point},
    reactions::{Reaction, Role},
};
use std::collections::{BTreeMap, HashSet};
mod search;
pub use search::{Budget, Proposal, Score, propose};

pub const MAX_MAP: u32 = i32::MAX as u32;

pub fn reaction(doc: &Document, arrow: u64) -> Result<&Reaction, String> {
    doc.reactions
        .iter()
        .find(|r| r.arrow == arrow)
        .ok_or_else(|| "Assign reactants and products first".into())
}
fn ids(reaction: &Reaction, role: Role) -> Vec<u64> {
    reaction
        .participants(role)
        .iter()
        .flat_map(|p| p.atoms.iter().copied())
        .collect()
}
fn compatible(doc: &Document, a: u64, b: u64) -> bool {
    doc.atom(a)
        .zip(doc.atom(b))
        .is_some_and(|(a, b)| a.element == b.element && a.isotope == b.isotope)
}
/// Duplicate map classes are legal imported data. Editing/automatic matching
/// creates the unique per-side labels required for ordinary atom correspondence.
fn unique(doc: &Document, atoms: &[u64]) -> Result<BTreeMap<u32, u64>, String> {
    let mut maps = BTreeMap::new();
    for id in atoms {
        let atom = doc
            .atom(*id)
            .ok_or("A reaction atom is no longer available")?;
        if atom.map_num != 0 && maps.insert(atom.map_num, *id).is_some() {
            return Err(format!(
                "Atom map {} occurs twice on one reaction side; edit these labels first",
                atom.map_num
            ));
        }
    }
    Ok(maps)
}
fn check_edit(doc: &Document, atom: u64, before: &Document) -> Result<(), String> {
    let mut member = false;
    for r in &doc.reactions {
        for role in Role::ALL {
            let side = ids(r, role);
            if side.contains(&atom) {
                let newly_mapped = doc.atom(atom).is_some_and(|a| {
                    a.map_num != 0 && before.atom(atom).is_none_or(|old| old.map_num != a.map_num)
                });
                if newly_mapped
                    && r.participants(role)
                        .iter()
                        .any(|p| p.coefficient != 1 && p.atoms.contains(&atom))
                {
                    return Err("Atom maps require one drawn molecule per participant; expand coefficients in the affected reaction first".into());
                }
                member = true;
                unique(doc, &side)?;
                if role != Role::Agent {
                    let maps = unique(doc, &side)?;
                    let opposite = ids(
                        r,
                        if role == Role::Reactant {
                            Role::Product
                        } else {
                            Role::Reactant
                        },
                    );
                    for other in opposite {
                        if let Some(a) = doc.atom(other)
                            && let Some(id) = maps.get(&a.map_num)
                            && !compatible(doc, *id, other)
                        {
                            return Err(
                                "Matching atom maps must have the same element and isotope".into(),
                            );
                        }
                    }
                }
            }
        }
    }
    if !member {
        let component = crate::reactions::molecules(doc, &[atom])
            .into_iter()
            .next()
            .unwrap_or_default();
        unique(doc, &component)?;
    }
    Ok(())
}
/// Return a validated replacement; errors leave the caller's document untouched.
pub fn set_map(doc: &Document, atom: u64, number: u32) -> Result<Document, String> {
    if number > MAX_MAP {
        return Err("Enter an atom map from 1 to 2147483647, or 0 to clear".into());
    }
    let mut candidate = doc.clone();
    candidate
        .atom_mut(atom)
        .ok_or("Select one atom to edit its map")?
        .map_num = number;
    if number != 0 {
        check_edit(&candidate, atom, doc)?;
    }
    candidate.validate()?;
    Ok(candidate)
}
/// Assign a selected reactant/product atom pair. Existing labels remain locked.
pub fn pair(doc: &Document, arrow: u64, selected: &[u64]) -> Result<Document, String> {
    let r = reaction(doc, arrow)?;
    let reactants = ids(r, Role::Reactant);
    let products = ids(r, Role::Product);
    let selected: Vec<_> = selected
        .iter()
        .copied()
        .filter(|id| doc.atom(*id).is_some())
        .collect();
    if selected.len() != 2 {
        return Err("Select one reactant atom and one product atom".into());
    }
    let a = selected
        .iter()
        .copied()
        .find(|id| reactants.contains(id))
        .ok_or("Select one reactant atom and one product atom")?;
    let b = selected
        .iter()
        .copied()
        .find(|id| products.contains(id))
        .ok_or("Select one reactant atom and one product atom")?;
    if !compatible(doc, a, b) {
        return Err("Pair atoms with the same element and isotope".into());
    }
    let a_map = doc.atom(a).map(|a| a.map_num).unwrap_or_default();
    let b_map = doc.atom(b).map(|a| a.map_num).unwrap_or_default();
    if a_map != 0 && b_map != 0 && a_map != b_map {
        return Err("These atoms have different maps; clear one before pairing".into());
    }
    let used: HashSet<_> = doc.atoms.iter().map(|a| a.map_num).collect();
    let number = if a_map != 0 {
        a_map
    } else if b_map != 0 {
        b_map
    } else {
        (1..=MAX_MAP)
            .find(|n| !used.contains(n))
            .ok_or("No atom map numbers are available")?
    };
    let mut candidate = doc.clone();
    candidate
        .atom_mut(a)
        .ok_or("The selected atom is no longer available")?
        .map_num = number;
    candidate
        .atom_mut(b)
        .ok_or("The selected atom is no longer available")?
        .map_num = number;
    check_edit(&candidate, a, doc)?;
    check_edit(&candidate, b, doc)?;
    candidate.validate()?;
    Ok(candidate)
}

/// Fit each complete product participant with a proper 2D rotation/translation.
/// A common lane offset retains the scheme's overall reactant/product separation.
/// No reflection, scaling, graph change, wedge flip, or stereo assignment occurs.
pub fn align(doc: &Document, arrow: u64) -> Result<Document, String> {
    doc.validate()?;
    let r = reaction(doc, arrow)?;
    let reactants = ids(r, Role::Reactant);
    let products = ids(r, Role::Product);
    let rm = unique(doc, &reactants)?;
    unique(doc, &products)?;
    for id in &products {
        if let Some(product) = doc.atom(*id)
            && let Some(other) = rm.get(&product.map_num)
            && !compatible(doc, *id, *other)
        {
            return Err("Matching atom maps must have the same element and isotope".into());
        }
    }
    let mean = |points: &[Point]| -> Point {
        let n = points.len().max(1) as f32;
        Point::new(
            points.iter().map(|p| p.x).sum::<f32>() / n,
            points.iter().map(|p| p.y).sum::<f32>() / n,
        )
    };
    let center = |atoms: &[u64]| {
        mean(
            &atoms
                .iter()
                .filter_map(|id| doc.atom(*id).map(|a| a.position))
                .collect::<Vec<_>>(),
        )
    };
    let rc = center(&reactants);
    let pc = center(&products);
    let lane = Point::new(pc.x - rc.x, pc.y - rc.y);
    let mut candidate = doc.clone();
    let mut matched = 0;
    for part in &r.products {
        let pairs: Vec<_> = part
            .atoms
            .iter()
            .filter_map(|id| {
                let product = doc.atom(*id)?;
                let reactant = doc.atom(*rm.get(&product.map_num)?)?;
                Some((product.position, reactant.position.offset(lane.x, lane.y)))
            })
            .collect();
        if pairs.is_empty() {
            continue;
        }
        matched += pairs.len();
        let from = mean(&pairs.iter().map(|(a, _)| *a).collect::<Vec<_>>());
        let to = mean(&pairs.iter().map(|(_, b)| *b).collect::<Vec<_>>());
        let (mut dot, mut cross) = (0_f64, 0_f64);
        for (a, b) in &pairs {
            let (ax, ay, bx, by) = (
                f64::from(a.x - from.x),
                f64::from(a.y - from.y),
                f64::from(b.x - to.x),
                f64::from(b.y - to.y),
            );
            dot += ax * bx + ay * by;
            cross += ax * by - ay * bx;
        }
        let angle = if dot.hypot(cross) > 1e-8 {
            cross.atan2(dot).to_degrees() as f32
        } else {
            0.
        };
        crate::editing::transform_about(&mut candidate, &part.atoms, from, 1., angle);
        for atom in candidate
            .atoms
            .iter_mut()
            .filter(|a| part.atoms.contains(&a.id))
        {
            atom.position = atom.position.offset(to.x - from.x, to.y - from.y);
        }
    }
    if matched == 0 {
        return Err("Assign matching reactant/product atom maps first".into());
    }
    candidate.validate()?;
    Ok(candidate)
}
