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
        return original;
    }
    // A retry is optional: an unavailable template or exhausted layout budget
    // must not turn a previously successful import into a failure.
    if let Ok(candidate) = depict::compute_with_rank_properties(
        state,
        ranks,
        None,
        depict::Options {
            bond_length,
            use_ring_templates: true,
            ..Default::default()
        },
    ) {
        let mut positions = candidate.positions;
        exterior_terminals(state, &mut positions, &Default::default());
        if collisions(state, &positions, count, bond_length) < count {
            return positions;
        }
        if let Some(positions) = bridges::clear(state, &positions, bond_length) {
            return positions;
        }
    }
    if let Some(positions) = bridges::clear(state, &original, bond_length) {
        return positions;
    }
    original
}

#[cfg(test)]
mod tests;
