//! Recognize complete face-bound carbon rings in molecular coordinate files.
//! Coordinates do not establish formal charges or oxidation states.
#![allow(clippy::indexing_slicing)] // V3 always has exactly three coordinates.
use super::{Atom, V3, elements};
use std::collections::HashSet;

pub(super) struct Contact {
    pub metal: usize,
    pub ring: Vec<usize>,
}

pub(super) fn contacts(atoms: &[Atom], xyz: &[V3], bonds: &[(usize, usize)]) -> Vec<Contact> {
    if !atoms.iter().any(|a| elements::metal(&a.element))
        || atoms.iter().filter(|a| a.element == "C").count() < 5
    {
        return vec![];
    }
    let mut adjacent = vec![Vec::new(); atoms.len()];
    for &(a, b) in bonds {
        if let Some(list) = adjacent.get_mut(a) {
            list.push(b);
        }
        if let Some(list) = adjacent.get_mut(b) {
            list.push(a);
        }
    }
    let mut contacts = Vec::new();
    for (metal, _) in atoms
        .iter()
        .enumerate()
        .filter(|(_, a)| elements::metal(&a.element))
    {
        let carbons: HashSet<_> = adjacent
            .get(metal)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&i| atoms.get(i).is_some_and(|a| a.element == "C"))
            .collect();
        let mut visited = HashSet::new();
        for &start in &carbons {
            if !visited.insert(start) {
                continue;
            }
            let mut ring = vec![start];
            let mut pending = vec![start];
            while let Some(i) = pending.pop() {
                for &j in adjacent.get(i).into_iter().flatten() {
                    if carbons.contains(&j) && visited.insert(j) {
                        ring.push(j);
                        pending.push(j);
                    }
                }
            }
            // A complete chordless ring, not an open chain, a single sigma
            // contact, or a collection of nearby ligand carbon atoms.
            if !matches!(ring.len(), 5 | 6)
                || ring.iter().any(|i| {
                    adjacent
                        .get(*i)
                        .into_iter()
                        .flatten()
                        .filter(|j| ring.contains(j))
                        .count()
                        != 2
                })
            {
                continue;
            }
            // Traverse the cycle itself: the polygon normal cannot depend
            // on which vertices happen to arrive first from the HashSet.
            ring.sort_unstable();
            let mut ordered = vec![ring[0]];
            while ordered.len() < ring.len() {
                let current = *ordered.last().unwrap_or(&ring[0]);
                let next = adjacent
                    .get(current)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|j| ring.contains(j) && !ordered.contains(j))
                    .min();
                let Some(next) = next else { break };
                ordered.push(next);
            }
            if face_bound(metal, &ordered, xyz, bonds) {
                ring.sort_unstable();
                contacts.push(Contact { metal, ring });
            }
        }
    }
    contacts.sort_unstable_by(|a, b| (a.metal, &a.ring).cmp(&(b.metal, &b.ring)));
    contacts
}

fn face_bound(metal: usize, ring: &[usize], xyz: &[V3], bonds: &[(usize, usize)]) -> bool {
    let points: Option<Vec<_>> = ring.iter().map(|&i| xyz.get(i).copied()).collect();
    let (Some(points), Some(metal)) = (points, xyz.get(metal).copied()) else {
        return false;
    };
    let n = points.len() as f64;
    let center = points
        .iter()
        .fold([0.; 3], |sum, p| add(sum, scale(*p, 1. / n)));
    // Newell's polygon normal averages the whole physical ring instead of
    // selecting one of several slightly different three-atom planes.
    let normal = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .fold([0.; 3], |normal, (a, b)| {
            add(normal, cross(sub(*a, center), sub(*b, center)))
        });
    let length = dot(normal, normal).sqrt();
    if length < 1e-6 {
        return false;
    }
    let normal = scale(normal, 1. / length);
    let toward = sub(metal, center);
    let height = dot(toward, normal);
    // Equal short C–C distances and a planar ring distinguish a conjugated
    // face from a saturated/puckered ring. The metal must sit over its centre.
    if height.abs() < 0.7
        || dot(toward, toward) - height * height > 0.35_f64.powi(2)
        || points
            .iter()
            .any(|p| dot(sub(*p, center), normal).abs() > 0.12)
    {
        return false;
    }
    let mut shortest = f64::INFINITY;
    let mut longest = 0_f64;
    for p in &points {
        let d = dot(sub(*p, metal), sub(*p, metal)).sqrt();
        shortest = shortest.min(d);
        longest = longest.max(d);
    }
    longest - shortest <= 0.25
        && bonds
            .iter()
            .filter(|(a, b)| ring.contains(a) && ring.contains(b))
            .all(|&(a, b)| {
                xyz.get(a).zip(xyz.get(b)).is_some_and(|(a, b)| {
                    let v = sub(*a, *b);
                    (1.25..=1.48).contains(&dot(v, v).sqrt())
                })
            })
}

fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(a: V3, s: f64) -> V3 {
    a.map(|x| x * s)
}
fn dot(a: V3, b: V3) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
