//! Optional small deformations of a nonaromatic bridge between two ring junctions.
use super::{Point3, State, collisions};

fn cross(p: &[Point3], a: usize, b: usize, c: usize) -> Option<f64> {
    let (a, b, c) = (p.get(a)?, p.get(b)?, p.get(c)?);
    Some((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x))
}

fn crossing(p: &[Point3], a: usize, b: usize, c: usize, d: usize) -> Option<bool> {
    Some(
        cross(p, a, b, c)? * cross(p, a, b, d)? < -1e-12
            && cross(p, c, d, a)? * cross(p, c, d, b)? < -1e-12,
    )
}

fn length(p: &[Point3], a: usize, b: usize) -> Option<f64> {
    let (a, b) = (p.get(a)?, p.get(b)?);
    Some((a.x - b.x).hypot(a.y - b.y))
}

/// Push a nonaromatic bridge away from the axis through its two junctions.
/// A trial is accepted only when it clears every severe overlap, keeps each
/// bond within 20% of its length, keeps every neighbour ray order (and so the
/// wedges' meaning) and introduces no bond crossing.
pub(super) fn clear(state: &State, positions: &[Point3], bond_length: f64) -> Option<Vec<Point3>> {
    let (n, m) = (state.graph.atoms.len(), state.graph.bonds.len());
    let bonds = &state.graph.bonds;
    // Each junction pair costs a traversal and each trial about n·m + m² checks;
    // bound the search for large cages.
    let trial = n * m + m * m;
    let mut budget = 1_000_000usize.checked_sub(trial)?;
    let mut adjacent = vec![Vec::new(); n];
    for bond in bonds {
        adjacent.get_mut(bond.a)?.push(bond.b);
        adjacent.get_mut(bond.b)?.push(bond.a);
    }
    let junctions: Vec<_> = (0..n)
        .filter(|&id| adjacent.get(id).is_some_and(|a| a.len() >= 3))
        .collect();
    let sign = |v: f64| if v.abs() < 1e-9 { 0. } else { v.signum() };
    for (index, &first) in junctions.iter().enumerate() {
        for &second in junctions.iter().take(index) {
            budget = budget.checked_sub(n + m)?;
            let mut seen = vec![false; n];
            let mut bridges = Vec::new();
            for start in 0..n {
                if start == first || start == second || *seen.get(start)? {
                    continue;
                }
                let (mut component, mut boundary, mut stack) = (vec![], 0, vec![start]);
                *seen.get_mut(start)? = true;
                while let Some(id) = stack.pop() {
                    component.push(id);
                    for &other in adjacent.get(id)? {
                        if other == first {
                            boundary |= 1;
                        } else if other == second {
                            boundary |= 2;
                        } else if !*seen.get(other)? {
                            *seen.get_mut(other)? = true;
                            stack.push(other);
                        }
                    }
                }
                if boundary == 3 {
                    bridges.push(component);
                }
            }
            // The junctions only form a cut when two components join both.
            if bridges.len() < 2 {
                continue;
            }
            let (a, b) = (*positions.get(first)?, *positions.get(second)?);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let length2 = dx * dx + dy * dy;
            for component in bridges {
                if component
                    .iter()
                    .any(|&id| state.graph.atoms.get(id).is_none_or(|atom| atom.aromatic))
                {
                    continue;
                }
                'trial: for factor in [1.1, 1.2, 1.4, 1.6] {
                    budget = budget.checked_sub(trial)?;
                    let mut candidate = positions.to_vec();
                    for &id in &component {
                        let p = *positions.get(id)?;
                        let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length2;
                        let (x, y) = (a.x + t * dx, a.y + t * dy);
                        let moved = candidate.get_mut(id)?;
                        moved.x = x + factor * (p.x - x);
                        moved.y = y + factor * (p.y - y);
                    }
                    if collisions(state, &candidate, 1, bond_length) != 0 {
                        continue;
                    }
                    for bond in bonds {
                        let ratio = length(&candidate, bond.a, bond.b)?
                            / length(positions, bond.a, bond.b)?;
                        if !(0.8..=1.2).contains(&ratio) {
                            continue 'trial;
                        }
                    }
                    for (id, neighbors) in adjacent.iter().enumerate() {
                        for (index, &a) in neighbors.iter().enumerate() {
                            for &b in neighbors.iter().take(index) {
                                if sign(cross(positions, id, a, b)?)
                                    != sign(cross(&candidate, id, a, b)?)
                                {
                                    continue 'trial;
                                }
                            }
                        }
                    }
                    for (index, p) in bonds.iter().enumerate() {
                        for q in bonds.iter().take(index) {
                            if [p.a, p.b].iter().all(|&id| id != q.a && id != q.b)
                                && crossing(&candidate, p.a, p.b, q.a, q.b)?
                                && !crossing(positions, p.a, p.b, q.a, q.b)?
                            {
                                continue 'trial;
                            }
                        }
                    }
                    return Some(candidate);
                }
            }
        }
    }
    None
}
