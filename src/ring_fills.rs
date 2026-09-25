//! Ring interior paint follows chemical atom identities, never a detached shape.
use crate::{
    document::{Document, Point},
    graphics::{GraphicStyle, PathCommand},
    scene::Primitive,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingFill {
    /// Consecutive vertices of a closed molecular cycle.
    pub atoms: Vec<u64>,
    pub color: [u8; 3],
}

fn key(atoms: &[u64]) -> Vec<u64> {
    let mut result = atoms.to_vec();
    if let Some((first, _)) = result.iter().enumerate().min_by_key(|(_, id)| *id) {
        result.rotate_left(first);
        if result.get(1).zip(result.last()).is_some_and(|(a, b)| a > b)
            && let Some(tail) = result.get_mut(1..)
        {
            tail.reverse();
        }
    }
    result
}

impl RingFill {
    pub fn commands(&self, doc: &Document) -> Vec<PathCommand> {
        if !self.atoms.iter().all(|id| doc.atom_visible(*id)) {
            return vec![];
        }
        let points: Option<Vec<Point>> = self
            .atoms
            .iter()
            .map(|id| doc.atom(*id).map(|a| a.position))
            .collect();
        let Some(points) = points else {
            return vec![];
        };
        let mut commands: Vec<_> = points
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    PathCommand::Move(p)
                } else {
                    PathCommand::Line(p)
                }
            })
            .collect();
        commands.push(PathCommand::Close);
        commands
    }
    pub fn primitive(&self, doc: &Document) -> Option<Primitive> {
        let commands = self.commands(doc);
        (!commands.is_empty()).then_some(Primitive::Path {
            commands,
            style: GraphicStyle {
                stroke: self.color,
                fill: Some(self.color),
                width_pt: 0.,
                ..Default::default()
            },
            filled: true,
        })
    }
}

fn valid(fill: &RingFill, atoms: &HashSet<u64>, edges: &HashSet<(u64, u64)>) -> bool {
    (3..=64).contains(&fill.atoms.len())
        && fill.atoms.iter().collect::<HashSet<_>>().len() == fill.atoms.len()
        && fill.atoms.iter().all(|id| atoms.contains(id))
        && fill
            .atoms
            .iter()
            .zip(fill.atoms.iter().cycle().skip(1))
            .all(|(a, b)| edges.contains(&((*a).min(*b), (*a).max(*b))))
}
fn topology(doc: &Document) -> (HashSet<u64>, HashSet<(u64, u64)>) {
    (
        doc.atoms.iter().map(|a| a.id).collect(),
        doc.bonds
            .iter()
            .map(|b| (b.a.min(b.b), b.a.max(b.b)))
            .collect(),
    )
}
pub fn validate(doc: &Document) -> Result<(), String> {
    if doc.ring_fills.len() > 10_000 {
        return Err("Too many ring fills".into());
    }
    let (atoms, edges) = topology(doc);
    let mut seen = HashSet::new();
    for fill in &doc.ring_fills {
        if !valid(fill, &atoms, &edges) || !seen.insert(key(&fill.atoms)) {
            return Err("Ring fill needs a unique, closed cycle of 3–64 existing atoms".into());
        }
    }
    Ok(())
}
pub fn prune(doc: &mut Document) {
    let (atoms, edges) = topology(doc);
    let mut seen = HashSet::new();
    doc.ring_fills
        .retain(|fill| valid(fill, &atoms, &edges) && seen.insert(key(&fill.atoms)));
}
/// Minimal closed cycles entirely within the visible selection. Bounded BFS
/// avoids enumerating the exponentially many cycles in a dense graph.
pub fn selected_cycles(doc: &Document, selected: &[u64]) -> Vec<Vec<u64>> {
    let selected: HashSet<_> = selected.iter().copied().collect();
    let mut adjacent: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for b in &doc.bonds {
        if (1..=4).contains(&b.order)
            && selected.contains(&b.a)
            && selected.contains(&b.b)
            && doc.bond_visible(b.a, b.b)
        {
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
        }
    }
    for next in adjacent.values_mut() {
        next.sort_unstable();
    }
    let mut cycles = BTreeSet::new();
    let mut budget = 200_000_usize;
    for (&a, next) in &adjacent {
        for &b in next.iter().filter(|b| **b > a) {
            let mut previous = BTreeMap::from([(a, a)]);
            let mut queue = VecDeque::from([(a, 0)]);
            while let Some((current, depth)) = queue.pop_front() {
                if budget == 0 {
                    return cycles.into_iter().collect();
                }
                budget -= 1;
                if depth >= 63 {
                    continue;
                }
                for &neighbor in adjacent.get(&current).into_iter().flatten() {
                    if (current == a && neighbor == b) || previous.contains_key(&neighbor) {
                        continue;
                    }
                    previous.insert(neighbor, current);
                    queue.push_back((neighbor, depth + 1));
                    if neighbor == b {
                        break;
                    }
                }
                if previous.contains_key(&b) {
                    break;
                }
            }
            if !previous.contains_key(&b) {
                continue;
            }
            let mut ids = vec![b];
            let mut current = b;
            while current != a {
                let Some(parent) = previous.get(&current).copied() else {
                    break;
                };
                ids.push(parent);
                current = parent;
            }
            if current == a && ids.len() >= 3 {
                cycles.insert(key(&ids));
            }
        }
    }
    cycles.into_iter().collect()
}
/// Return the number of rings changed. None removes paint, not bonds.
pub fn apply(doc: &mut Document, selected: &[u64], color: Option<[u8; 3]>) -> usize {
    let cycles = selected_cycles(doc, selected);
    let mut changed = 0;
    for atoms in cycles {
        let index = doc
            .ring_fills
            .iter()
            .position(|fill| key(&fill.atoms) == atoms);
        match (index, color) {
            (Some(i), Some(color)) => {
                if let Some(fill) = doc.ring_fills.get_mut(i)
                    && fill.color != color
                {
                    fill.color = color;
                    changed += 1;
                }
            }
            (Some(i), None) => {
                doc.ring_fills.remove(i);
                changed += 1;
            }
            (None, Some(color)) => {
                doc.ring_fills.push(RingFill { atoms, color });
                changed += 1;
            }
            _ => (),
        }
    }
    changed
}
