//! Chemical atom correspondence, independent of printed custom atom numbers.
//! Automatic proposals are detached, bounded, and require explicit acceptance.
use crate::{
    document::{Document, Point},
    reactions::{Reaction, Role},
};
use std::{
    cell::OnceCell,
    collections::{BTreeMap, HashSet},
};
mod search;
pub use search::{Budget, Proposal, Score, propose};

pub const MAX_MAP: u32 = i32::MAX as u32;
const COEFFICIENT_ERROR: &str = "Atom maps require one drawn molecule per participant; expand coefficients in the affected reaction first";

/// The error for a map number outside 0..=MAX_MAP (or not a number at all).
pub fn map_range_error() -> String {
    format!("Enter an atom map from 1 to {MAX_MAP}, or 0 to clear")
}

/// Set the drawing-wide visibility and clear every per-atom visibility override.
pub fn set_maps_visible(doc: &Document, show: bool) -> Document {
    let mut candidate = doc.clone();
    candidate.atom_labels.maps = show;
    for atom in &mut candidate.atoms {
        atom.display.mapping.show = None;
    }
    candidate
}

/// Restore automatic placement without changing map numbers or visibility.
pub fn reset_label_offsets(doc: &Document) -> Document {
    let mut candidate = doc.clone();
    for atom in &mut candidate.atoms {
        atom.display.mapping.offset = None;
    }
    candidate
}

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
fn same_isotope_element(a_element: &str, a_isotope: u32, b_element: &str, b_isotope: u32) -> bool {
    a_element == b_element && a_isotope == b_isotope
}
fn compatible(doc: &Document, a: u64, b: u64) -> bool {
    // Element/isotope equality is symmetric, so callers may reverse atom order.
    doc.atom(a)
        .zip(doc.atom(b))
        .is_some_and(|(a, b)| same_isotope_element(&a.element, a.isotope, &b.element, b.isotope))
}
fn cross_side_conflict(
    doc: &Document,
    maps: &BTreeMap<u32, u64>,
    other: &[u64],
) -> Option<(u32, u64, u64)> {
    other.iter().find_map(|other| {
        let atom = doc.atom(*other)?;
        let id = *maps.get(&atom.map_num)?;
        (!compatible(doc, id, *other)).then_some((atom.map_num, id, *other))
    })
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
fn validate_edited_atoms(doc: &Document, atoms: &[u64], before: &Document) -> Result<(), String> {
    struct Side {
        role: Role,
        atoms: Vec<u64>,
        maps: OnceCell<Result<BTreeMap<u32, u64>, String>>,
    }
    // Each reaction/role's IDs also serve as its opposite side's cached IDs.
    // Initialize uniqueness lazily to keep coefficient errors ahead of it.
    let scopes: Vec<_> = doc
        .reactions
        .iter()
        .map(|reaction| {
            let sides = Role::ALL.map(|role| Side {
                role,
                atoms: ids(reaction, role),
                maps: OnceCell::new(),
            });
            (reaction, sides)
        })
        .collect();
    for atom in atoms {
        let mut member = false;
        let newly_mapped = doc.atom(*atom).is_some_and(|a| {
            a.map_num != 0
                && before
                    .atom(*atom)
                    .is_none_or(|old| old.map_num != a.map_num)
        });
        for (reaction, sides) in &scopes {
            for side in sides {
                if side.atoms.contains(atom) {
                    if newly_mapped
                        && reaction
                            .participants(side.role)
                            .iter()
                            .any(|p| p.coefficient != 1 && p.atoms.contains(atom))
                    {
                        return Err(COEFFICIENT_ERROR.into());
                    }
                    member = true;
                    let maps = side
                        .maps
                        .get_or_init(|| unique(doc, &side.atoms))
                        .as_ref()
                        .map_err(Clone::clone)?;
                    if side.role != Role::Agent {
                        let opposite_role = if side.role == Role::Reactant {
                            Role::Product
                        } else {
                            Role::Reactant
                        };
                        if let Some(opposite) = sides.iter().find(|s| s.role == opposite_role)
                            && cross_side_conflict(doc, maps, &opposite.atoms).is_some()
                        {
                            return Err(
                                "Matching atom maps must have the same element and isotope".into(),
                            );
                        }
                    }
                }
            }
        }
        if !member {
            let component = crate::reactions::molecules(doc, &[*atom])
                .into_iter()
                .next()
                .unwrap_or_default();
            unique(doc, &component)?;
        }
    }
    Ok(())
}
fn check_edit(doc: &Document, atom: u64, before: &Document) -> Result<(), String> {
    validate_edited_atoms(doc, &[atom], before)
}
/// Return a validated replacement; errors leave the caller's document untouched.
pub fn set_map(doc: &Document, atom: u64, number: u32) -> Result<Document, String> {
    if number > MAX_MAP {
        return Err(map_range_error());
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
    validate_edited_atoms(&candidate, &[a, b], doc)?;
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
    if cross_side_conflict(doc, &rm, &products).is_some() {
        return Err("Matching atom maps must have the same element and isotope".into());
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
    reflow_separators(doc, &mut candidate, r);
    candidate.validate()?;
    Ok(candidate)
}

/// A product's extent along the reading axis, its extent across it and its
/// mean offset across it.
#[derive(Debug, Clone, Copy)]
struct Lane {
    lo: f32,
    hi: f32,
    across: (f32, f32),
    mean: f32,
}
/// The reading axis is the arrow's direction; a degenerate arrow reads along x.
fn reading_axis(doc: &Document, arrow: u64) -> Point {
    doc.arrows
        .iter()
        .find(|a| a.id == arrow)
        .map(|a| Point::new(a.end.x - a.start.x, a.end.y - a.start.y))
        .filter(|d| d.x.hypot(d.y) > 1e-3)
        .map_or(Point::new(1., 0.), |d| {
            let length = d.x.hypot(d.y);
            Point::new(d.x / length, d.y / length)
        })
}
/// (along, across) coordinates of a point on the reading axis.
fn project(axis: Point, p: Point) -> (f32, f32) {
    (p.x * axis.x + p.y * axis.y, p.y * axis.x - p.x * axis.y)
}
/// Product lanes in reading order.
fn lanes(doc: &Document, reaction: &Reaction, axis: Point) -> Vec<Lane> {
    let mut lanes: Vec<_> = reaction
        .products
        .iter()
        .filter_map(|part| {
            let points: Vec<_> = part
                .atoms
                .iter()
                .filter_map(|id| doc.atom(*id).map(|a| project(axis, a.position)))
                .collect();
            let range = |values: &mut dyn Iterator<Item = f32>| {
                values.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(v), hi.max(v))
                })
            };
            let positions = part
                .atoms
                .iter()
                .filter_map(|id| doc.atom(*id))
                .map(|atom| atom.position);
            let labels = crate::scene::participant_label_ink_boxes(doc, &part.atoms)
                .into_iter()
                .flat_map(|(lo, hi)| [lo, Point::new(lo.x, hi.y), hi, Point::new(hi.x, lo.y)]);
            let extents: Vec<_> = positions.chain(labels).map(|p| project(axis, p)).collect();
            let (lo, hi) = range(&mut extents.iter().map(|p| p.0));
            let across = range(&mut extents.iter().map(|p| p.1));
            let mean = points.iter().map(|p| p.1).sum::<f32>() / points.len().max(1) as f32;
            (!points.is_empty()).then_some(Lane {
                lo,
                hi,
                across,
                mean,
            })
        })
        .collect();
    lanes.sort_by(|a, b| (a.lo + a.hi).total_cmp(&(b.lo + b.hi)));
    lanes
}
/// Centre and size of a sign's visible `+`. A wide, aligned text frame or
/// surrounding whitespace can place it anywhere inside the frame.
fn glyph_box(a: &crate::document::Annotation) -> (Point, (f32, f32)) {
    let natural = crate::typography::TextFormat {
        width_pt: None,
        ..a.format.clone()
    };
    let width = |text: &str| crate::typography::layout(text, &natural).width;
    let layout = crate::typography::layout(&a.text, &a.format);
    let x = layout.fragments.first().map_or(0., |f| f.position.x);
    let lead = a.text.find('+').map_or("", |i| &a.text[..i]);
    let sign = width("+");
    let centre = a
        .position
        .offset(x + width(lead) + sign / 2., layout.height / 2.);
    (centre, (sign, layout.height))
}
/// Move each product-side `+` with the gap it separated, so the list still
/// reads `A + B` after alignment moves, rotates or reorders the participants.
/// Only separators move; participants keep their fitted positions.
fn reflow_separators(before: &Document, after: &mut Document, reaction: &Reaction) {
    let axis = reading_axis(before, reaction.arrow);
    // Each separator's id, centre (along, across) and size.
    let mut free: Vec<_> = reaction
        .annotations
        .iter()
        .filter_map(|id| before.annotations.iter().find(|a| a.id == *id))
        .filter(|a| a.text.trim() == "+")
        .map(|a| {
            let (centre, size) = glyph_box(a);
            (a.id, project(axis, centre), size)
        })
        .collect();
    let gap = |a: &Lane, b: &Lane| ((a.hi + b.lo) / 2., (a.mean + b.mean) / 2.);
    let old = lanes(before, reaction, axis);
    let new = lanes(after, reaction, axis);
    for (old, new) in old.windows(2).zip(new.windows(2)) {
        let ([a, b], [c, d]) = (old, new) else {
            continue;
        };
        // A product separator lies between the neighbours' centres along the
        // arrow and within their band across it; reagent signs lie outside.
        let (left, right) = ((a.lo + a.hi) / 2., (b.lo + b.hi) / 2.);
        let (low, high) = (a.across.0.min(b.across.0), a.across.1.max(b.across.1));
        let from = gap(a, b);
        let distance = |p: (f32, f32)| (p.0 - from.0).hypot(p.1 - from.1);
        let Some(index) = free
            .iter()
            .enumerate()
            .filter(|(_, (_, (t, n), (w, h)))| {
                let pad = w.max(*h);
                (left..=right).contains(t) && (low - pad..=high + pad).contains(n)
            })
            .min_by(|(_, (_, p, _)), (_, (_, q, _))| distance(*p).total_cmp(&distance(*q)))
            .map(|(index, _)| index)
        else {
            continue;
        };
        let (id, (along, _), (w, _)) = free.swap_remove(index);
        // Move with the gap, keeping the sign's offset from it, so text frame
        // width and alignment never shift the visible glyph. If that offset
        // leaves a narrowed gap, centre the glyph in it instead.
        let to = gap(c, d);
        let target = along - from.0 + to.0;
        let target = if (c.hi + w * 0.5..=d.lo - w * 0.5).contains(&target) {
            target
        } else {
            to.0
        };
        let (t, n) = (target - along, to.1 - from.1);
        if let Some(annotation) = after.annotations.iter_mut().find(|a| a.id == id) {
            annotation.position = annotation
                .position
                .offset(t * axis.x - n * axis.y, t * axis.y + n * axis.x);
        }
    }
}

#[cfg(test)]
mod tests;
