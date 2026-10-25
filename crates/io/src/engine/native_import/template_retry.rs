//! Retry crowded bridged systems with native ring templates. The exact native
//! depiction API and every collision-free import retain their existing output.
use super::{State, depict};
use crate::chemistry::layout_refinement::{bridged, exterior_terminals};
use crate::chemistry::stereo::Point3;
use std::collections::{HashMap, HashSet};

/// Count severe nonbonded collisions using a bounded-neighborhood grid rather
/// than scanning every atom pair. Counts saturate once the result is known to
/// be no better than the current layout.
fn collisions(state: &State, positions: &[Point3], stop: usize) -> usize {
    const CLEARANCE: f64 = 0.75;
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
            (point.x / CLEARANCE).floor() as i64,
            (point.y / CLEARANCE).floor() as i64,
        );
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(nearby) = cells.get(&(cell.0 + dx, cell.1 + dy)) {
                    for &(other, previous) in nearby {
                        if !bonds.contains(&(other, id))
                            && (point.x - previous.x).hypot(point.y - previous.y) < CLEARANCE
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
    count
}

pub(super) fn improve(
    state: &State,
    ranks: &[depict::ranks::AtomProperties],
    original: Vec<Point3>,
) -> Vec<Point3> {
    if !bridged(state) {
        return original;
    }
    let count = collisions(state, &original, 4096);
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
            use_ring_templates: true,
            ..Default::default()
        },
    ) && collisions(state, &candidate.positions, count) < count
    {
        let mut positions = candidate.positions;
        exterior_terminals(state, &mut positions, &Default::default());
        return positions;
    }
    original
}

#[cfg(test)]
mod tests;
