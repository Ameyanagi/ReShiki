//! Stable OPSIN 2.9 construction enumeration for closed hexagonal ring maps.
//!
//! This parser compatibility proof preserves the source's bond, ring and row
//! enumeration. Exhaustion leaves the numbering ambiguity unresolved.
use super::super::molecule::AtomSet;
use super::{
    entry_geometry::{absolute_direction, allowed_entry_directions, direction_steps},
    layout::center_row_orientation_score,
    model::{FaceModel, FusionComponentSpec, FusionNameAst, normalize_edge},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Point = (i64, i64);
type Edge = (usize, usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionOrder {
    pub atom_order: Vec<usize>,
    /// Face and winding relative to the supplied audited reference positions.
    pub entries: Vec<(usize, i64)>,
}

pub fn ordered_hexagonal_construction(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    model: &FaceModel,
    positions: &BTreeMap<usize, Point>,
    search_budget: usize,
) -> Option<ConstructionOrder> {
    let citation = ast.citation_plan.as_ref()?;
    if citation.parent_occurrences().len() != 1
        || !citation.interparent_occurrences.is_empty()
        || !citation.cycle_closing_join_indices.is_empty()
        || !ast.multiplicative_groups.is_empty()
        || model.faces.iter().any(|face| face.size != 6)
        || specs
            .values()
            .any(|spec| spec.template.atoms.iter().any(|atom| atom.symbol != "C"))
        || specs.values().any(|spec| {
            spec.construction_order
                .as_ref()
                .is_none_or(|order| order.version != "2.9.0")
        })
    {
        return None;
    }
    let matches: BTreeMap<_, _> = ast
        .component_occurrences
        .iter()
        .map(|m| (m.occurrence_id, m))
        .collect();
    let mut bonds = Vec::<Edge>::new();
    let mut bond_keys = BTreeSet::new();
    let mut seen = AtomSet::new();
    let mut atom_order = Vec::new();
    for occurrence in citation
        .parent_occurrences()
        .into_iter()
        .chain(citation.render_order.iter().rev().copied())
    {
        let local = matches.get(&occurrence)?.input_atom_by_locant();
        let local_atoms: AtomSet = local.values().copied().collect();
        let shared: AtomSet = local_atoms.intersection(&seen).copied().collect();
        let construction = specs.get(&occurrence)?.construction_order.as_ref()?;
        let edges: Vec<_> = construction
            .directed_bond_locants
            .iter()
            .map(|(a, b)| Some((*local.get(a)?, *local.get(b)?)))
            .collect::<Option<_>>()?;
        for locant in &construction.atom_locants {
            let atom = *local.get(locant)?;
            if !seen.contains(&atom) {
                atom_order.push(atom);
            }
        }
        let joins: Vec<_> = ast
            .joins
            .iter()
            .filter(|join| join.attached_occurrence() == occurrence)
            .collect();
        let joined: AtomSet = joins
            .iter()
            .flat_map(|join| join.shared_input_atoms())
            .collect();
        if joins.len() > 1 || shared != joined {
            return None;
        }
        let mut ordered: Vec<_> = edges
            .iter()
            .copied()
            .filter(|(a, b)| !shared.contains(a) && !shared.contains(b))
            .collect();
        for join in joins {
            // Surviving child bonds precede descriptor-atom incident bonds.
            for atom in &join.interface.ordered_input_atoms {
                for &edge in &edges {
                    let shared_count = usize::from(shared.contains(&edge.0))
                        + usize::from(shared.contains(&edge.1));
                    if (edge.0 == *atom || edge.1 == *atom)
                        && shared_count == 1
                        && !ordered.contains(&edge)
                    {
                        ordered.push(edge);
                    }
                }
            }
        }
        for edge in ordered {
            if bond_keys.insert(normalize_edge(edge.0, edge.1)) {
                bonds.push(edge);
            }
        }
        seen.extend(local_atoms);
    }
    let expected_edges: BTreeSet<_> = model
        .faces
        .iter()
        .flat_map(|face| {
            face.atom_cycle
                .iter()
                .copied()
                .zip(face.atom_cycle.iter().copied().cycle().skip(1))
                .take(face.atom_cycle.len())
                .map(|(a, b)| normalize_edge(a, b))
        })
        .collect();
    if atom_order.iter().copied().collect::<AtomSet>() != positions.keys().copied().collect()
        || bond_keys != expected_edges
    {
        return None;
    }
    let mut adjacency: BTreeMap<_, Vec<_>> =
        atom_order.iter().map(|&atom| (atom, Vec::new())).collect();
    for (index, &(a, b)) in bonds.iter().enumerate() {
        adjacency.get_mut(&a)?.push((b, index));
        adjacency.get_mut(&b)?.push((a, index));
    }
    let first_atom = *atom_order.first()?;
    let mut used = AtomSet::from([first_atom]);
    let mut parents = BTreeMap::<usize, (usize, usize)>::new();
    let mut links = Vec::new();
    // Explicit iterator offsets preserve the recursive Java neighbor order.
    let mut stack = vec![(first_atom, None, 0usize)];
    while let Some((atom, parent, next)) = stack.last_mut() {
        let Some(&(other, edge)) = adjacency[atom].get(*next) else {
            stack.pop();
            continue;
        };
        *next += 1;
        if Some(other) == *parent {
            continue;
        }
        if used.contains(&other) {
            if !links.contains(&edge) {
                links.push(edge);
            }
        } else {
            parents.insert(other, (*atom, edge));
            used.insert(other);
            let parent = *atom;
            stack.push((other, Some(parent), 0));
        }
    }
    if used != atom_order.iter().copied().collect() {
        return None;
    }
    let ancestors = |mut atom: usize| {
        let mut result = Vec::new();
        while let Some(&(parent, edge)) = parents.get(&atom) {
            atom = parent;
            result.push(edge);
        }
        result
    };
    let mut rings: Vec<_> = links
        .into_iter()
        .map(|edge| {
            let mut ring = difference(&ancestors(bonds[edge].0), &ancestors(bonds[edge].1));
            ring.push(edge);
            ring
        })
        .collect();
    if rings.is_empty() {
        return None;
    }
    let mut remaining = search_budget;
    let mut change = true;
    while change {
        for index in 0..rings.len() {
            let ring = rings[index].clone();
            // SSSRFinder intentionally overwrites this flag for each ring and
            // reads replacements in place; OR-ing or snapshot iteration differs.
            change = false;
            for (target_index, target) in rings.iter_mut().enumerate() {
                spend(&mut remaining)?;
                if target_index == index {
                    continue;
                }
                let reduced = difference(target, &ring);
                if reduced.len() < target.len() {
                    *target = reduced;
                    change = true;
                }
            }
        }
    }
    let face_sets: BTreeMap<_, _> = model
        .faces
        .iter()
        .map(|face| {
            (
                face.atom_cycle.iter().copied().collect::<AtomSet>(),
                face.id,
            )
        })
        .collect();
    let face_ids: Vec<_> = rings
        .iter()
        .map(|ring| {
            let atoms: AtomSet = ring
                .iter()
                .flat_map(|&edge| [bonds[edge].0, bonds[edge].1])
                .collect();
            face_sets.get(&atoms).copied()
        })
        .collect::<Option<_>>()?;
    if face_ids.iter().copied().collect::<AtomSet>().len() != model.faces.len()
        || rings.iter().any(|ring| ring.len() != 6)
    {
        return None;
    }
    // Vec-backed dictionaries retain first insertion order when values change.
    let mut neighbors = vec![Vec::<(usize, usize)>::new(); rings.len()];
    for (index, ring) in rings.iter().enumerate() {
        for &edge in ring {
            for other in index + 1..rings.len() {
                if rings[other].contains(&edge) {
                    insert_neighbor(&mut neighbors[index], other, edge);
                    insert_neighbor(&mut neighbors[other], index, edge);
                    break;
                }
            }
        }
    }
    let root = (0..rings.len()).min_by_key(|&index| neighbors[index].len())?;
    let fusion_atoms: AtomSet = neighbors[root]
        .iter()
        .flat_map(|&(_, edge)| [bonds[edge].0, bonds[edge].1])
        .collect();
    let seed = rings[root]
        .iter()
        .copied()
        .find(|&edge| {
            !fusion_atoms.contains(&bonds[edge].0) && !fusion_atoms.contains(&bonds[edge].1)
        })
        .or_else(|| {
            rings[root]
                .iter()
                .copied()
                .find(|edge| !neighbors[root].iter().any(|(_, e)| e == edge))
        })?;
    let mut traversal = RingTraversal {
        rings: &rings,
        bonds: &bonds,
        neighbors: &neighbors,
        remaining: &mut remaining,
        cycles: BTreeMap::new(),
        visited: AtomSet::new(),
    };
    let entered = traversal.enter(root, seed, bonds[seed].0)?;
    let mut pending = vec![RingFrame {
        ring: root,
        previous: None,
        incoming: 0,
        edges: entered.0,
        relative: entered.1,
        next: 0,
    }];
    let mut directions = Vec::new();
    while let Some(frame) = pending.last_mut() {
        let Some(&(other, edge)) = neighbors[frame.ring].get(frame.next) else {
            pending.pop();
            continue;
        };
        frame.next += 1;
        let index = frame.edges.iter().position(|&value| value == edge)?;
        let direction = if Some(other) == frame.previous {
            absolute_direction(4, frame.incoming)
        } else if index > 0 {
            absolute_direction(frame.relative[index - 1], frame.incoming)
        } else {
            return None;
        };
        directions.push((frame.ring, other, direction));
        if !traversal.visited.contains(&other) {
            let atom = traversal.cycles[&frame.ring][(index + 5) % 6];
            let entered = traversal.enter(other, edge, atom)?;
            let previous = frame.ring;
            pending.push(RingFrame {
                ring: other,
                previous: Some(previous),
                incoming: direction,
                edges: entered.0,
                relative: entered.1,
                next: 0,
            });
        }
    }
    let root_cycle = &traversal.cycles[&root];
    let area: i128 = root_cycle
        .iter()
        .zip(root_cycle.iter().cycle().skip(1))
        .take(root_cycle.len())
        .map(|(a, b)| {
            i128::from(positions[a].0) * i128::from(positions[b].1)
                - i128::from(positions[b].0) * i128::from(positions[a].1)
        })
        .sum();
    let lookup: BTreeMap<_, _> = directions.iter().map(|&(a, b, d)| ((a, b), d)).collect();
    let mut axes = Vec::new();
    let mut longest = 0usize;
    for &(_, start, direction) in &directions {
        let (mut count, mut current) = (1, start);
        let mut exhausted = true;
        for _ in 0..rings.len() + 1 {
            let outgoing = directions
                .iter()
                .find(|&&(a, _, d)| a == current && d == direction);
            let Some(&(_, next, _)) = outgoing else {
                exhausted = false;
                break;
            };
            current = next;
            count += 1;
        }
        if exhausted {
            return None;
        }
        if count > longest {
            axes.clear();
            longest = count;
        }
        if count == longest
            && !axes.contains(&direction)
            && !axes.contains(&absolute_direction(direction, 4))
        {
            axes.push(direction);
        }
    }
    let steps = direction_steps();
    let mut options = Vec::<([i64; 3], usize, i64)>::new();
    for axis in axes {
        let mut centers = BTreeMap::from([(root, (0, 0))]);
        let mut queue = VecDeque::from([root]);
        while let Some(ring) = queue.pop_front() {
            for &(other, _) in &neighbors[ring] {
                let direction = absolute_direction(*lookup.get(&(ring, other))?, -axis);
                let &(dx, dy) = steps.get(&direction)?;
                let point = (centers[&ring].0 + dx, centers[&ring].1 + dy);
                if let Some(previous) = centers.get(&other) {
                    if *previous != point {
                        return None;
                    }
                } else {
                    centers.insert(other, point);
                    queue.push_back(other);
                }
            }
        }
        if centers.values().collect::<BTreeSet<_>>().len() != rings.len() {
            return None;
        }
        // Axis, occupied rows bottom-to-top and left-to-right, then quadrant.
        let cells: BTreeMap<_, _> = centers
            .iter()
            .map(|(&ring, &point)| (point, ring))
            .collect();
        let mut rows = Vec::new();
        for y in cells.keys().map(|p| p.1).collect::<BTreeSet<_>>() {
            let mut x = cells.keys().map(|p| p.0).min()?;
            let max_x = cells.keys().map(|p| p.0).max()?;
            while x <= max_x {
                let Some(&first) = cells.get(&(x, y)) else {
                    x += 2;
                    continue;
                };
                let mut row = vec![first];
                while let Some(&other) = cells.get(&(x + 4 * row.len() as i64, y)) {
                    row.push(other);
                }
                x += 4 * row.len() as i64 + 2;
                rows.push(row);
            }
        }
        let longest_row = rows.iter().map(Vec::len).max()?;
        for row in rows {
            if row.len() != longest_row {
                continue;
            }
            for (quadrant, (sx, sy)) in [(1, 1), (-1, 1), (-1, -1), (1, -1)].into_iter().enumerate()
            {
                let oriented: BTreeMap<_, _> = centers
                    .iter()
                    .map(|(&ring, &(x, y))| (ring, (sx * x, sy * y)))
                    .collect();
                let score = center_row_orientation_score(&oriented, &row);
                let top = *oriented
                    .keys()
                    .max_by_key(|ring| (oriented[ring].1, oriented[ring].0))?;
                if traversal.cycles[&top]
                    .iter()
                    .all(|atom| adjacency[atom].len() == 3)
                {
                    return None;
                }
                let winding = if area > 0 { 1 } else { -1 }
                    * if quadrant == 0 || quadrant == 2 {
                        -1
                    } else {
                        1
                    };
                options.push((score, face_ids[top], winding));
            }
        }
    }
    let best = options.iter().map(|row| row.0).min()?;
    Some(ConstructionOrder {
        atom_order,
        entries: options
            .into_iter()
            .filter(|row| row.0 == best)
            .map(|(_, face, winding)| (face, winding))
            .collect(),
    })
}

fn spend(remaining: &mut usize) -> Option<()> {
    *remaining = remaining.checked_sub(1)?;
    Some(())
}

fn difference(left: &[usize], right: &[usize]) -> Vec<usize> {
    left.iter()
        .filter(|edge| !right.contains(edge))
        .chain(right.iter().filter(|edge| !left.contains(edge)))
        .copied()
        .collect()
}

fn insert_neighbor(neighbors: &mut Vec<(usize, usize)>, ring: usize, edge: usize) {
    if let Some(existing) = neighbors.iter_mut().find(|(other, _)| *other == ring) {
        existing.1 = edge;
    } else {
        neighbors.push((ring, edge));
    }
}

struct RingFrame {
    ring: usize,
    previous: Option<usize>,
    incoming: i64,
    edges: Vec<usize>,
    relative: Vec<i64>,
    next: usize,
}

struct RingTraversal<'a> {
    rings: &'a [Vec<usize>],
    bonds: &'a [Edge],
    neighbors: &'a [Vec<(usize, usize)>],
    remaining: &'a mut usize,
    cycles: BTreeMap<usize, Vec<usize>>,
    visited: AtomSet,
}

impl RingTraversal<'_> {
    fn enter(
        &mut self,
        ring: usize,
        entry: usize,
        mut atom: usize,
    ) -> Option<(Vec<usize>, Vec<i64>)> {
        spend(self.remaining)?;
        self.visited.insert(ring);
        let mut cycle = vec![atom];
        let mut edges = vec![entry];
        while edges.len() < 6 {
            let edge = self.rings[ring].iter().copied().find(|edge| {
                !edges.contains(edge)
                    && (self.bonds[*edge].0 == atom || self.bonds[*edge].1 == atom)
            })?;
            let (a, b) = self.bonds[edge];
            atom = if a == atom { b } else { a };
            cycle.push(atom);
            edges.push(edge);
        }
        self.cycles.insert(ring, cycle);
        let occupied = self.neighbors[ring]
            .iter()
            .map(|(_, edge)| edges.iter().position(|value| value == edge))
            .collect::<Option<AtomSet>>()?;
        let choices = allowed_entry_directions(6, &occupied);
        if choices.len() == 1 {
            Some((edges, choices[0].clone()))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::retained_graph_model::RetainedGraphAtomTemplate;
    use super::super::model::{
        Face, FusionCitationNode, FusionCitationPlan, FusionComponentConstructionOrder,
        FusionComponentMatch,
    };
    use super::*;

    fn two_hexagons() -> (
        FusionNameAst,
        BTreeMap<usize, FusionComponentSpec>,
        FaceModel,
        BTreeMap<usize, Point>,
    ) {
        let cycles = [vec![0, 2, 3, 4, 5, 1], vec![1, 6, 7, 8, 9, 0]];
        let edges = [
            (0, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 1),
            (1, 0),
            (1, 6),
            (6, 7),
            (7, 8),
            (8, 9),
            (9, 0),
        ];
        let locants: Vec<_> = (0..10).map(|atom| (atom + 1).to_string()).collect();
        let mut spec = FusionComponentSpec::default();
        spec.template.atoms = locants
            .iter()
            .map(|locant| RetainedGraphAtomTemplate {
                locant: locant.clone(),
                symbol: "C".into(),
                ..Default::default()
            })
            .collect();
        spec.construction_order = Some(FusionComponentConstructionOrder {
            atom_locants: locants.clone(),
            directed_bond_locants: edges
                .into_iter()
                .map(|(a, b)| (locants[a].clone(), locants[b].clone()))
                .collect(),
            version: "2.9.0".into(),
            ..Default::default()
        });
        let ast = FusionNameAst {
            component_occurrences: vec![FusionComponentMatch {
                occurrence_id: 0,
                local_to_input_atom: locants
                    .into_iter()
                    .enumerate()
                    .map(|(atom, locant)| (locant, atom))
                    .collect(),
                ..Default::default()
            }],
            citation_plan: Some(FusionCitationPlan {
                roots: vec![FusionCitationNode {
                    occurrence_id: 0,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let model = FaceModel {
            faces: cycles
                .into_iter()
                .enumerate()
                .map(|(id, atom_cycle)| Face {
                    id,
                    atom_cycle,
                    size: 6,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let positions = [
            (0, (0, 2)),
            (1, (0, -2)),
            (2, (-2, 4)),
            (3, (-6, 4)),
            (4, (-8, 0)),
            (5, (-6, -4)),
            (6, (2, -4)),
            (7, (6, -4)),
            (8, (8, 0)),
            (9, (6, 4)),
        ]
        .into_iter()
        .collect();
        (ast, BTreeMap::from([(0, spec)]), model, positions)
    }

    #[test]
    fn exact_ring_enumeration_preserves_quadrants_and_budget_boundary() {
        let (ast, specs, model, positions) = two_hexagons();
        let order = ordered_hexagonal_construction(&ast, &specs, &model, &positions, 6).unwrap();
        assert_eq!(order.atom_order, (0..10).collect::<Vec<_>>());
        assert_eq!(order.entries, vec![(1, 1), (0, -1), (0, 1), (1, -1)]);
        assert!(ordered_hexagonal_construction(&ast, &specs, &model, &positions, 5).is_none());
    }

    #[test]
    fn construction_witness_requires_the_declared_parser_version() {
        let (ast, mut specs, model, positions) = two_hexagons();
        specs
            .get_mut(&0)
            .unwrap()
            .construction_order
            .as_mut()
            .unwrap()
            .version = "future".into();
        assert!(ordered_hexagonal_construction(&ast, &specs, &model, &positions, 100).is_none());
    }
}
