//! Optional readability repair for native bridged-template layouts.
//!
//! The native coordinate-map attachment rules can put a terminal substituent
//! inside the occupied sector of a bridged ring junction. Move only that leaf
//! into the exterior sector; ring coordinates and chemical state stay intact.
use super::{depict::geometry::Coordinates, stereo::Point3, stereo::perception::State};
use std::{
    collections::HashMap,
    f64::consts::{PI, TAU},
};

pub(crate) fn bridged(state: &State) -> bool {
    let mut remaining = 1_000_000usize;
    let mut shared = HashMap::<(usize, usize), usize>::new();
    let mut memberships = HashMap::<usize, Vec<usize>>::new();
    for (ring, atoms) in state.rings.atoms.iter().enumerate() {
        for &atom in atoms {
            let previous = memberships.entry(atom).or_default();
            for &other in previous.iter() {
                let Some(next) = remaining.checked_sub(1) else {
                    return false;
                };
                remaining = next;
                let count = shared.entry((other, ring)).or_default();
                *count += 1;
                if *count >= 3 {
                    return true;
                }
            }
            previous.push(ring);
        }
    }
    false
}

fn cross(a: Point3, b: Point3, c: Point3) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn crosses_other_bond(
    state: &State,
    points: &[Point3],
    leaf: usize,
    center: usize,
    end: Point3,
) -> bool {
    let Some(&start) = points.get(center) else {
        return true;
    };
    state.graph.bonds.iter().any(|bond| {
        if [bond.a, bond.b]
            .iter()
            .any(|&id| id == leaf || id == center)
        {
            return false;
        }
        let Some((&a, &b)) = points.get(bond.a).zip(points.get(bond.b)) else {
            return true;
        };
        let (ab_a, ab_b, cd_a, cd_b) = (
            cross(start, end, a),
            cross(start, end, b),
            cross(a, b, start),
            cross(a, b, end),
        );
        let on = |point: Point3, a: Point3, b: Point3| {
            point.x >= a.x.min(b.x) - 1e-9
                && point.x <= a.x.max(b.x) + 1e-9
                && point.y >= a.y.min(b.y) - 1e-9
                && point.y <= a.y.max(b.y) + 1e-9
        };
        ab_a * ab_b < -1e-12 && cd_a * cd_b < -1e-12
            || ab_a.abs() < 1e-12 && on(a, start, end)
            || ab_b.abs() < 1e-12 && on(b, start, end)
            || cd_a.abs() < 1e-12 && on(start, a, b)
            || cd_b.abs() < 1e-12 && on(end, a, b)
    })
}

fn adds_collision(
    points: &[Point3],
    leaf: usize,
    center: usize,
    old: Point3,
    new: Point3,
    clearance: f64,
) -> bool {
    points.iter().enumerate().any(|(id, point)| {
        id != leaf
            && id != center
            && (point.x - new.x).hypot(point.y - new.y) < clearance
            && (point.x - old.x).hypot(point.y - old.y) >= clearance
    })
}

fn bridged_junction(state: &State, rings: &[usize], remaining: &mut usize) -> bool {
    for (i, &first) in rings.iter().enumerate() {
        for &second in rings.iter().skip(i + 1) {
            let Some((a, b)) = state
                .rings
                .atoms
                .get(first)
                .zip(state.rings.atoms.get(second))
            else {
                return false;
            };
            let Some(next) = remaining.checked_sub(a.len().saturating_mul(b.len())) else {
                return false;
            };
            *remaining = next;
            if a.iter().filter(|id| b.contains(id)).take(3).count() == 3 {
                return true;
            }
        }
    }
    false
}

/// Leave fixed atoms, ring geometry, ordinary layouts and already exterior
/// substituents untouched. Every accepted move keeps its bond length and adds
/// neither a severe nonbonded collision nor a proper bond crossing.
pub(crate) fn exterior_terminals(state: &State, points: &mut [Point3], fixed: &Coordinates) {
    if points.len() != state.graph.atoms.len() || !bridged(state) {
        return;
    }
    let mut adjacent = vec![Vec::new(); points.len()];
    for bond in &state.graph.bonds {
        let Some(a) = adjacent.get_mut(bond.a) else {
            return;
        };
        a.push((bond.b, bond.order));
        let Some(b) = adjacent.get_mut(bond.b) else {
            return;
        };
        b.push((bond.a, bond.order));
    }
    let mut rings = vec![Vec::new(); points.len()];
    for (index, ring) in state.rings.atoms.iter().enumerate() {
        for &id in ring {
            let Some(count) = rings.get_mut(id) else {
                return;
            };
            count.push(index);
        }
    }
    let mut remaining = 1_000_000usize;
    for (leaf, atom) in state.graph.atoms.iter().enumerate() {
        if atom.atomic_number != 6
            || atom.charge != 0
            || atom.radical_electrons != 0
            || atom.aromatic
            || fixed.contains_key(&leaf)
        {
            continue;
        }
        let Some(&[(center, 1)]) = adjacent.get(leaf).map(Vec::as_slice) else {
            continue;
        };
        let Some(neighbors) = adjacent.get(center) else {
            continue;
        };
        let Some(center_rings) = rings.get(center) else {
            return;
        };
        if neighbors.len() != 4 || center_rings.len() < 2 {
            continue;
        }
        let Some(&origin) = points.get(center) else {
            return;
        };
        let Some(&old) = points.get(leaf) else {
            return;
        };
        let mut angles = Vec::with_capacity(3);
        for &(id, order) in neighbors {
            if id == leaf {
                continue;
            }
            if order != 1 || rings.get(id).is_none_or(Vec::is_empty) {
                break;
            }
            let Some(point) = points.get(id) else {
                return;
            };
            angles.push(
                (point.y - origin.y)
                    .atan2(point.x - origin.x)
                    .rem_euclid(TAU),
            );
        }
        if angles.len() != 3 {
            continue;
        }
        angles.sort_by(f64::total_cmp);
        let Some((&first, &last)) = angles.first().zip(angles.last()) else {
            continue;
        };
        let mut gap = (last, first + TAU - last);
        for pair in angles.windows(2) {
            if let [a, b] = pair
                && b - a > gap.1
            {
                gap = (*a, b - a);
            }
        }
        let old_angle = (old.y - origin.y).atan2(old.x - origin.x).rem_euclid(TAU);
        if gap.1 <= PI + 1e-6 || (old_angle - gap.0).rem_euclid(TAU) < gap.1 {
            continue;
        }
        if !bridged_junction(state, center_rings, &mut remaining) {
            continue;
        }
        let length = (old.x - origin.x).hypot(old.y - origin.y);
        if !length.is_finite() || length < 1e-6 {
            continue;
        }
        let cost = points
            .len()
            .saturating_add(state.graph.bonds.len())
            .saturating_mul(2);
        let Some(next) = remaining.checked_sub(cost) else {
            break;
        };
        remaining = next;
        let angle = gap.0 + gap.1 * 0.5;
        let new = Point3 {
            x: origin.x + length * angle.cos(),
            y: origin.y + length * angle.sin(),
            z: old.z,
        };
        if !adds_collision(points, leaf, center, old, new, length * 0.5)
            && !crosses_other_bond(state, points, leaf, center, new)
            && let Some(point) = points.get_mut(leaf)
        {
            *point = new;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chemistry::{depict, smiles};
    const ARTEMISININ: &str = "C[C@@H]1CC[C@H]2[C@H](C(=O)O[C@H]3[C@@]24[C@H]1CC[C@](O3)(OO4)C)C";

    fn layout(text: &str) -> anyhow::Result<(State, Vec<Point3>)> {
        let state = smiles::read(text)?.prepared.state;
        let positions = depict::compute(
            &state,
            &vec![None; state.graph.atoms.len()],
            None,
            depict::Options {
                use_ring_templates: true,
                ..Default::default()
            },
        )?
        .positions;
        Ok((state, positions))
    }

    #[test]
    fn exterior_repair_changes_only_the_leaf_and_respects_fixed_atoms() -> anyhow::Result<()> {
        let (state, old) = layout(ARTEMISININ)?;
        let mut repaired = old.clone();
        exterior_terminals(&state, &mut repaired, &Coordinates::new());
        assert_ne!(repaired[18], old[18]);
        for (id, (a, b)) in old.iter().zip(&repaired).enumerate() {
            if id != 18 {
                assert_eq!(a, b);
            }
        }
        let length =
            |points: &[Point3]| (points[18].x - points[14].x).hypot(points[18].y - points[14].y);
        assert!((length(&old) - length(&repaired)).abs() < 1e-12);
        let mut fixed = Coordinates::new();
        fixed.insert(
            18,
            depict::geometry::Point {
                x: old[18].x,
                y: old[18].y,
            },
        );
        let mut held = old.clone();
        exterior_terminals(&state, &mut held, &fixed);
        assert_eq!(held, old);
        exterior_terminals(&state, &mut repaired, &Coordinates::new());
        let twice = repaired.clone();
        exterior_terminals(&state, &mut repaired, &Coordinates::new());
        assert_eq!(repaired, twice);
        Ok(())
    }

    #[test]
    fn occupied_exterior_or_new_crossing_does_not_publish_a_move() -> anyhow::Result<()> {
        for mode in ["collision", "crossing", "touching", "collinear"] {
            let (state, mut points) = layout(&format!(
                "{ARTEMISININ}.{}",
                if mode == "collision" { "C" } else { "CC" }
            ))?;
            let mut target = points.clone();
            exterior_terminals(&state, &mut target, &Coordinates::new());
            assert_ne!(target[18], points[18]);
            if mode != "collision" {
                let origin = points[14];
                let end = target[18];
                let (dx, dy) = (end.x - origin.x, end.y - origin.y);
                let middle = Point3 {
                    x: (origin.x + end.x) * 0.5,
                    y: (origin.y + end.y) * 0.5,
                    z: 0.,
                };
                points[20] = Point3 {
                    x: middle.x - dy,
                    y: middle.y + dx,
                    z: 0.,
                };
                points[21] = Point3 {
                    x: middle.x + dy,
                    y: middle.y - dx,
                    z: 0.,
                };
                if mode == "touching" {
                    points[20] = Point3 {
                        x: origin.x + dx * 0.25,
                        y: origin.y + dy * 0.25,
                        z: 0.,
                    };
                    points[21] = Point3 {
                        x: points[20].x - dy,
                        y: points[20].y + dx,
                        z: 0.,
                    };
                } else if mode == "collinear" {
                    points[20] = Point3 {
                        x: origin.x - dx,
                        y: origin.y - dy,
                        z: 0.,
                    };
                    points[21] = Point3 {
                        x: origin.x + 2. * dx,
                        y: origin.y + 2. * dy,
                        z: 0.,
                    };
                }
                assert!(!adds_collision(
                    &points, 18, 14, points[18], target[18], 0.75
                ));
                assert!(crosses_other_bond(&state, &points, 18, 14, target[18]));
            } else {
                points[20] = target[18];
            }
            let original = points.clone();
            exterior_terminals(&state, &mut points, &Coordinates::new());
            assert_eq!(points, original, "Unsafe exterior: {mode}");
        }
        Ok(())
    }

    #[test]
    fn a_disconnected_fused_junction_is_not_reclassified_as_bridged() -> anyhow::Result<()> {
        let (state, mut points) = layout(&format!("{ARTEMISININ}.CC12CCCCC1CCCC2"))?;
        assert!(bridged(&state));
        let junction = 21;
        let rings = state
            .rings
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(id, ring)| ring.contains(&junction).then_some(id))
            .collect::<Vec<_>>();
        assert!(rings.len() >= 2);
        assert!(!bridged_junction(&state, &rings, &mut 1_000_000));
        let origin = Point3 {
            x: 30.,
            y: 30.,
            z: 0.,
        };
        points[junction] = origin;
        let neighbors = state
            .graph
            .bonds
            .iter()
            .filter_map(|bond| {
                if bond.a == junction {
                    Some(bond.b)
                } else if bond.b == junction {
                    Some(bond.a)
                } else {
                    None
                }
            })
            .filter(|&id| id != 20)
            .collect::<Vec<_>>();
        assert_eq!(neighbors.len(), 3);
        for (index, neighbor) in neighbors.iter().enumerate() {
            let angle = index as f64 * PI / 4.;
            points[*neighbor] = Point3 {
                x: origin.x + 1.5 * angle.cos(),
                y: origin.y + 1.5 * angle.sin(),
                z: 0.,
            };
        }
        points[20] = Point3 {
            x: 31.,
            y: 31.,
            z: 0.,
        };
        let old = points[20];
        exterior_terminals(&state, &mut points, &Coordinates::new());
        assert_eq!(points[20], old);
        Ok(())
    }
}
