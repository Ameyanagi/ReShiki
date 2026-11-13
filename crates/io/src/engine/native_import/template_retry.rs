//! Retry crowded bridged systems with native templates and bridge adjustments.
//! The exact native depiction API and clear imports retain their output.
use super::{State, depict};
use crate::chemistry::layout_refinement::{bridged, exterior_terminals};
use crate::chemistry::stereo::Point3;
use std::collections::{HashMap, HashSet};
mod bridges;

/// Count severe nonbonded atom-pair and atom-bond overlaps using a spatial grid.
/// Counts saturate once the result is known to be no better than the current layout.
fn collisions(state: &State, positions: &[Point3], stop: usize, bond_length: f64) -> usize {
    let clearance = 0.5 * bond_length;
    let bond_clearance = 0.3 * bond_length;
    let bonds: HashSet<_> = state
        .graph
        .bonds
        .iter()
        .map(|b| (b.a.min(b.b), b.a.max(b.b)))
        .collect();
    let mut cells = HashMap::<(i64, i64), Vec<(usize, &Point3)>>::new();
    let mut count = 0usize;
    for (id, point) in positions.iter().enumerate() {
        let cell = (
            (point.x / clearance).floor() as i64,
            (point.y / clearance).floor() as i64,
        );
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(nearby) = cells.get(&(cell.0 + dx, cell.1 + dy)) {
                    for &(other, previous) in nearby {
                        if !bonds.contains(&(other, id))
                            && (point.x - previous.x).hypot(point.y - previous.y) < clearance
                        {
                            count += 1;
                            if count >= stop {
                                return count;
                            }
                        }
                    }
                }
            }
        }
        cells.entry(cell).or_default().push((id, point));
    }
    // Atom-on-bond overlaps and the bridge repair (#735) are ReShiki
    // refinements that live RDKit comparisons stop before.
    if !crate::chemistry::document::refine() {
        return count;
    }
    for bond in &state.graph.bonds {
        let (Some(a), Some(b)) = (positions.get(bond.a), positions.get(bond.b)) else {
            continue;
        };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length2 = dx * dx + dy * dy;
        let x0 = ((a.x.min(b.x) - bond_clearance) / clearance).floor() as i64;
        let x1 = ((a.x.max(b.x) + bond_clearance) / clearance).floor() as i64;
        let y0 = ((a.y.min(b.y) - bond_clearance) / clearance).floor() as i64;
        let y1 = ((a.y.max(b.y) + bond_clearance) / clearance).floor() as i64;
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(nearby) = cells.get(&(x, y)) {
                    for &(id, point) in nearby {
                        if id == bond.a || id == bond.b {
                            continue;
                        }
                        let t =
                            (((point.x - a.x) * dx + (point.y - a.y) * dy) / length2).clamp(0., 1.);
                        if (point.x - a.x - t * dx).hypot(point.y - a.y - t * dy) < bond_clearance {
                            count += 1;
                            if count >= stop {
                                return count;
                            }
                        }
                    }
                }
            }
        }
    }
    count
}

/// Longest minus shortest bond, in bond lengths. Fused macrocycles such as
/// porphyrins embed without collisions but with stretched bonds (#1113).
fn spread(state: &State, positions: &[Point3], bond_length: f64) -> f64 {
    let (shortest, longest) = state
        .graph
        .bonds
        .iter()
        .filter_map(|bond| positions.get(bond.a).zip(positions.get(bond.b)))
        .map(|(a, b)| (b.x - a.x).hypot(b.y - a.y))
        .fold((f64::INFINITY, 0_f64), |(lo, hi), l| (lo.min(l), hi.max(l)));
    (longest - shortest) / bond_length
}

/// Bonds further apart than this are a stretched layout, not template rounding.
const STRETCHED: f64 = 0.1;

/// The templated layout, or None when a template is unavailable or the layout
/// budget is exhausted: a retry must not turn a successful import into a failure.
fn templated(
    state: &State,
    ranks: &[depict::ranks::AtomProperties],
    bond_length: f64,
) -> Option<Vec<Point3>> {
    let mut positions = depict::compute_with_rank_properties(
        state,
        ranks,
        None,
        depict::Options {
            bond_length,
            use_ring_templates: true,
            path_angle_expansion_extra_trials: crate::chemistry::document::refine(),
            preserve_spiro_rings: crate::chemistry::document::refine(),
            ..Default::default()
        },
    )
    .ok()?
    .positions;
    exterior_terminals(state, &mut positions, &Default::default());
    Some(positions)
}

pub(in crate::engine) fn improve(
    state: &State,
    ranks: &[depict::ranks::AtomProperties],
    original: Vec<Point3>,
    bond_length: f64,
) -> Vec<Point3> {
    if !bridged(state) {
        return original;
    }
    let count = collisions(state, &original, 4096, bond_length);
    if count == 0 {
        // A stretched macrocycle is kept unless its template draws every bond
        // at the same length without introducing a collision (#1113).
        if !crate::chemistry::document::refine()
            || spread(state, &original, bond_length) <= STRETCHED
        {
            return original;
        }
        return templated(state, ranks, bond_length)
            .filter(|positions| {
                spread(state, positions, bond_length) <= STRETCHED
                    && collisions(state, positions, 1, bond_length) == 0
            })
            .unwrap_or(original);
    }
    if let Some(positions) = templated(state, ranks, bond_length) {
        if collisions(state, &positions, count, bond_length) < count {
            return positions;
        }
        if !crate::chemistry::document::refine() {
            return original;
        }
        if let Some(positions) = bridges::clear(state, &positions, bond_length) {
            return positions;
        }
    }
    if !crate::chemistry::document::refine() {
        return original;
    }
    if let Some(positions) = bridges::clear(state, &original, bond_length) {
        return positions;
    }
    original
}

#[cfg(test)]
mod tests;
