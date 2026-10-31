//! OPSIN-compatible relative entry directions on an independently audited layout.
use super::super::molecule::AtomSet;
use super::{
    layout::{
        Budget, Point, opsin_occupied_row_orientation, preferred_intrinsic_layouts_with_budget,
        segments_intersect,
    },
    model::{FaceModel, FusedLayout},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::OnceLock,
};
#[derive(Clone, Debug, Deserialize)]
struct DirectionRow {
    relative: Vec<i64>,
    forbidden_ports: Vec<usize>,
    #[serde(default)]
    required_ports: Vec<usize>,
}
#[derive(Deserialize)]
struct DirectionData {
    directions: BTreeMap<usize, Vec<DirectionRow>>,
    steps: BTreeMap<i64, (i64, i64)>,
}
struct DirectionTables {
    directions: BTreeMap<usize, Vec<DirectionRow>>,
    steps: BTreeMap<i64, Point>,
}
#[allow(
    clippy::expect_used,
    reason = "the bundled immutable JSON table is checked by geometry tests"
)]
fn tables() -> &'static DirectionTables {
    static TABLES: OnceLock<DirectionTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let d: DirectionData =
            serde_json::from_str(include_str!("../data/fusion_entry_geometry.json"))
                .expect("entry direction tables");
        let directions = d.directions;
        let steps = d.steps;
        assert_eq!(steps.len(), 8);
        for (&n, rows) in &directions {
            assert!(n >= 3 && !rows.is_empty());
            for row in rows {
                assert_eq!(row.relative.len(), n - 1);
                assert!(row.relative.iter().all(|r| steps.contains_key(r)));
                assert!(
                    row.forbidden_ports
                        .iter()
                        .chain(&row.required_ports)
                        .all(|&p| p >= 1 && p < n)
                )
            }
        }
        DirectionTables { directions, steps }
    })
}
pub fn allowed_entry_directions(size: usize, occupied: &AtomSet) -> Vec<Vec<i64>> {
    let Some(options) = tables().directions.get(&size) else {
        return vec![];
    };
    if [5, 7].contains(&size) && occupied.len() == 1 {
        return vec![options[0].relative.clone()];
    }
    if size == 5 && occupied.len() == size {
        return options[..3].iter().map(|r| r.relative.clone()).collect();
    }
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    for row in options {
        if row.forbidden_ports.iter().any(|p| occupied.contains(p))
            || row.required_ports.iter().any(|p| !occupied.contains(p))
        {
            continue;
        }
        let signature = occupied
            .iter()
            .filter(|&&p| p != 0)
            .map(|&p| row.relative[p - 1])
            .collect::<Vec<_>>();
        if seen.insert(signature) {
            result.push(row.relative.clone())
        }
    }
    result
}
pub fn absolute_direction(relative: i64, previous: i64) -> i64 {
    let mut direction = (relative + previous + 4).rem_euclid(8) - 4;
    if direction.abs() == 2 {
        let (r, p) = (relative.abs(), previous.abs());
        if (r == 1 && p == 3) || (r == 3 && p == 1) {
            direction = direction.signum()
        } else if r == p && [1, 3].contains(&r) {
            direction = 3 * direction.signum()
        }
    }
    if direction == -4 { 4 } else { direction }
}
pub fn direction_steps() -> &'static BTreeMap<i64, Point> {
    &tables().steps
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryDirectionWitness {
    pub perimeter: Vec<usize>,
    pub start_face: usize,
    pub ring_positions: Vec<(usize, i64, i64)>,
    pub orientation_score: Vec<i64>,
    pub direction_conflicts: Vec<(usize, usize)>,
}
fn clockwise_boundary(cycle: &[usize], positions: &BTreeMap<usize, Point>) -> Option<Vec<usize>> {
    let area = (0..cycle.len())
        .map(|i| {
            let (a, b) = (
                positions.get(&cycle[i])?,
                positions.get(&cycle[(i + 1) % cycle.len()])?,
            );
            Some(a.0 as i128 * b.1 as i128 - b.0 as i128 * a.1 as i128)
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .sum::<i128>();
    if area == 0 {
        None
    } else if area < 0 {
        Some(cycle.to_vec())
    } else {
        Some(cycle.iter().rev().copied().collect())
    }
}
fn linear_face_chain(neighbors: &BTreeMap<usize, Vec<usize>>) -> bool {
    let ends = neighbors
        .iter()
        .filter(|(_, v)| v.len() == 1)
        .map(|(&f, _)| f)
        .collect::<Vec<_>>();
    if ends.len() != 2 || neighbors.values().any(|v| v.len() != 1 && v.len() != 2) {
        return false;
    }
    let mut seen = AtomSet::new();
    let mut pending = vec![ends[0]];
    while let Some(f) = pending.pop() {
        if seen.insert(f) {
            pending.extend(&neighbors[&f])
        }
    }
    seen.len() == neighbors.len()
}
#[cfg(test)]
pub fn entry_direction_geometry(
    model: &FaceModel,
    search_budget: usize,
) -> Result<Option<(FusedLayout, Vec<EntryDirectionWitness>)>, String> {
    entry_direction_geometry_with_options(model, search_budget, None, false)
}
struct EntrySearch<'a> {
    cycles: &'a BTreeMap<usize, Vec<usize>>,
    neighbors: &'a BTreeMap<usize, Vec<usize>>,
    ports: &'a BTreeMap<(usize, usize), usize>,
    budget: &'a mut Budget,
    tables: BTreeSet<Vec<((usize, usize), i64)>>,
    straight_only: bool,
}
impl EntrySearch<'_> {
    fn visit(
        &mut self,
        pending: Vec<(usize, i64, usize)>,
        visited: AtomSet,
        directions: BTreeMap<(usize, usize), i64>,
    ) -> Result<(), String> {
        self.budget.spend()?;
        if pending.is_empty() {
            if visited.len() == self.cycles.len() {
                self.tables.insert(directions.into_iter().collect());
            }
            return Ok(());
        }
        let (face, incoming, entry) = pending[0];
        if visited.contains(&face) {
            return self.visit(pending[1..].to_vec(), visited, directions);
        }
        let size = self.cycles[&face].len();
        let distances = self.neighbors[&face]
            .iter()
            .map(|&other| (other, (self.ports[&(face, other)] + size - entry) % size))
            .collect::<BTreeMap<_, _>>();
        for table in allowed_entry_directions(size, &distances.values().copied().collect()) {
            let outgoing = distances
                .iter()
                .map(|(&other, &distance)| {
                    (
                        other,
                        absolute_direction(
                            if distance == 0 {
                                4
                            } else {
                                table[distance - 1]
                            },
                            incoming,
                        ),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            if self.straight_only
                && self.neighbors[&face].len() == 2
                && outgoing
                    .iter()
                    .any(|(o, &value)| distances[o] != 0 && value != incoming)
            {
                continue;
            }
            let mut following = self.neighbors[&face]
                .iter()
                .filter(|o| !visited.contains(o))
                .map(|&other| (other, outgoing[&other], self.ports[&(other, face)]))
                .collect::<Vec<_>>();
            following.extend_from_slice(&pending[1..]);
            let mut visited = visited.clone();
            visited.insert(face);
            let mut directions = directions.clone();
            directions.extend(outgoing.into_iter().map(|(other, d)| ((face, other), d)));
            self.visit(following, visited, directions)?;
        }
        Ok(())
    }
}
#[derive(Clone)]
struct EntryGeometryCacheEntry {
    model: FaceModel,
    budget: usize,
    terminal_cycle: Option<Vec<usize>>,
    allow_distorted: bool,
    result: Option<(FusedLayout, Vec<EntryDirectionWitness>)>,
}
thread_local! {
    static ENTRY_GEOMETRY_CACHE: std::cell::RefCell<VecDeque<EntryGeometryCacheEntry>> = const { std::cell::RefCell::new(VecDeque::new()) };
}
pub fn entry_direction_geometry_with_options(
    model: &FaceModel,
    search_budget: usize,
    terminal_cycle: Option<&[usize]>,
    allow_distorted: bool,
) -> Result<Option<(FusedLayout, Vec<EntryDirectionWitness>)>, String> {
    if let Some(result) = ENTRY_GEOMETRY_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache.iter().position(|entry| {
            entry.model == *model
                && entry.budget == search_budget
                && entry.terminal_cycle.as_deref() == terminal_cycle
                && entry.allow_distorted == allow_distorted
        })?;
        let entry = cache.remove(index)?;
        let result = entry.result.clone();
        cache.push_back(entry);
        Some(result)
    }) {
        return Ok(result);
    }
    let result =
        entry_direction_geometry_uncached(model, search_budget, terminal_cycle, allow_distorted)?;
    ENTRY_GEOMETRY_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() == 128 {
            cache.pop_front();
        }
        cache.push_back(EntryGeometryCacheEntry {
            model: model.clone(),
            budget: search_budget,
            terminal_cycle: terminal_cycle.map(<[usize]>::to_vec),
            allow_distorted,
            result: result.clone(),
        });
    });
    Ok(result)
}
fn entry_direction_geometry_uncached(
    model: &FaceModel,
    search_budget: usize,
    terminal_cycle: Option<&[usize]>,
    allow_distorted: bool,
) -> Result<Option<(FusedLayout, Vec<EntryDirectionWitness>)>, String> {
    let tables = tables();
    if !model.faces.iter().any(|f| [5, 7].contains(&f.size))
        || model
            .faces
            .iter()
            .any(|f| !tables.directions.contains_key(&f.size))
    {
        return Ok(None);
    }
    let layouts = preferred_intrinsic_layouts_with_budget(model, search_budget)?;
    let Some(reference) = layouts.first() else {
        return Ok(None);
    };
    let positions = reference
        .atom_positions
        .iter()
        .map(|&(a, x, y)| (a, (x, y)))
        .collect::<BTreeMap<_, _>>();
    let mut cycles = BTreeMap::new();
    for face in &model.faces {
        let Some(mut cycle) = clockwise_boundary(&face.atom_cycle, &positions) else {
            return Ok(None);
        };
        cycle.reverse();
        cycles.insert(face.id, cycle);
    }
    let mut terminal_root = None;
    let mut reverse_winding = false;
    if let Some(terminal) = terminal_cycle {
        terminal_root = cycles
            .iter()
            .find(|(_, c)| {
                c.iter().copied().collect::<AtomSet>() == terminal.iter().copied().collect()
            })
            .map(|(&f, _)| f);
        let Some(root) = terminal_root else {
            return Ok(None);
        };
        let mut cycle = cycles[&root].clone();
        let Some(offset) = cycle.iter().position(|&a| a == terminal[0]) else {
            return Ok(None);
        };
        cycle.rotate_left(offset);
        if cycle != terminal {
            cycle.reverse();
            // The preceding rotation put terminal[0] first; reversal puts it last.
            let offset = cycle.len() - 1;
            cycle.rotate_left(offset);
            if cycle != terminal {
                return Ok(None);
            }
            for cycle in cycles.values_mut() {
                cycle.reverse()
            }
            reverse_winding = true
        }
    }
    let edges = cycles
        .iter()
        .map(|(&f, c)| {
            (
                f,
                (0..c.len())
                    .map(|i| {
                        let (a, b) = (c[i], c[(i + 1) % c.len()]);
                        (a.min(b), a.max(b))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut neighbors: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut ports = BTreeMap::new();
    for &(a, b, _) in &model.face_adjacency {
        let shared = edges[&a]
            .iter()
            .filter(|e| edges[&b].contains(e))
            .copied()
            .collect::<Vec<_>>();
        if shared.len() != 1 {
            return Ok(None);
        }
        let e = shared[0];
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
        #[allow(
            clippy::unwrap_used,
            reason = "the unique shared edge was selected from the intersection of both edge lists"
        )]
        let (left_port, right_port) = (
            edges[&a].iter().position(|&v| v == e).unwrap(),
            edges[&b].iter().position(|&v| v == e).unwrap(),
        );
        ports.insert((a, b), left_port);
        ports.insert((b, a), right_port);
    }
    if neighbors.keys().copied().collect::<AtomSet>() != cycles.keys().copied().collect() {
        return Ok(None);
    }
    let minimum = neighbors.values().map(Vec::len).min().unwrap_or(0);
    let mut terminals = cycles
        .keys()
        .copied()
        .filter(|f| neighbors[f].len() == minimum)
        .collect::<Vec<_>>();
    if let Some(root) = terminal_root {
        if !terminals.contains(&root) {
            return Ok(None);
        }
        terminals = vec![root]
    }
    let unique = terminals.len() == 1 && minimum == 1 && model.face_adjacency.len() >= cycles.len();
    if allow_distorted && !unique {
        return Ok(None);
    }
    if unique && !allow_distorted {
        return entry_direction_geometry_with_options(model, search_budget, terminal_cycle, true);
    }
    let linear = !allow_distorted
        && model.face_adjacency.len() + 1 == cycles.len()
        && linear_face_chain(&neighbors);
    let mut budget = Budget::new(search_budget);
    for straight_only in if linear {
        vec![true, false]
    } else {
        vec![false]
    } {
        let mut search = EntrySearch {
            cycles: &cycles,
            neighbors: &neighbors,
            ports: &ports,
            budget: &mut budget,
            tables: BTreeSet::new(),
            straight_only,
        };
        for &root in &terminals {
            let fused = neighbors[&root]
                .iter()
                .flat_map(|&other| {
                    let (a, b) = edges[&root][ports[&(root, other)]];
                    [a, b]
                })
                .collect::<AtomSet>();
            let mut entries = edges[&root]
                .iter()
                .enumerate()
                .filter(|(_, e)| !fused.contains(&e.0) && !fused.contains(&e.1))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if entries.is_empty() {
                entries = (0..edges[&root].len())
                    .filter(|i| {
                        !neighbors[&root]
                            .iter()
                            .any(|&other| ports[&(root, other)] == *i)
                    })
                    .collect()
            }
            for entry in entries {
                search.visit(vec![(root, 0, entry)], AtomSet::new(), BTreeMap::new())?
            }
        }
        let direction_tables = std::mem::take(&mut search.tables);
        drop(search);
        let Some(mut outer) = clockwise_boundary(&model.outer_boundary, &positions) else {
            return Ok(None);
        };
        if reverse_winding {
            outer.reverse()
        }
        let fusion = model
            .face_adjacency
            .iter()
            .flat_map(|&(a, b, _)| {
                let (a, b) = edges[&a][ports[&(a, b)]];
                [a, b]
            })
            .collect::<AtomSet>();
        let mut candidates = BTreeMap::new();
        let mut best_score = None;
        for items in direction_tables {
            let directions = items.iter().copied().collect::<BTreeMap<_, _>>();
            let distortion = items
                .iter()
                .filter(|((a, b), d)| absolute_direction(*d, 4) != directions[&(*b, *a)])
                .count() as i64
                / 2;
            for axis in directions.values().copied().collect::<BTreeSet<_>>() {
                budget.spend()?;
                let directed = directions
                    .iter()
                    .map(|(&edge, &d)| (edge, absolute_direction(d, -axis)))
                    .collect::<BTreeMap<_, _>>();
                let root = if allow_distorted {
                    terminals[0]
                } else {
                    // At least one five/seven-membered face was required above.
                    cycles.keys().copied().collect::<Vec<_>>()[0]
                };
                let mut centers = BTreeMap::from([(root, (0, 0))]);
                let mut queue = VecDeque::from([root]);
                let mut consistent = true;
                let mut conflicts = BTreeSet::new();
                while let Some(left) = queue.pop_front() {
                    for &right in &neighbors[&left] {
                        let (dx, dy) = tables.steps[&directed[&(left, right)]];
                        let point = (centers[&left].0 + dx, centers[&left].1 + dy);
                        if let Some(old) = centers.get(&right) {
                            if *old != point {
                                if allow_distorted {
                                    conflicts.insert((left.min(right), left.max(right)));
                                } else {
                                    consistent = false;
                                    break;
                                }
                            }
                        } else {
                            centers.insert(right, point);
                            queue.push_back(right)
                        }
                    }
                    if !consistent {
                        break;
                    }
                }
                if !consistent
                    || centers.len() != cycles.len()
                    || centers.values().collect::<BTreeSet<_>>().len() != cycles.len()
                {
                    continue;
                }
                let connections = model
                    .face_adjacency
                    .iter()
                    .map(|&(a, b, _)| (a, b))
                    .collect::<Vec<_>>();
                let mut crossing = false;
                for (i, &(a, b)) in connections.iter().enumerate() {
                    for &(c, d) in &connections[i + 1..] {
                        if a != c
                            && a != d
                            && b != c
                            && b != d
                            && segments_intersect(
                                centers[&a],
                                centers[&b],
                                centers[&c],
                                centers[&d],
                            )
                        {
                            crossing = true
                        }
                    }
                }
                if crossing {
                    continue;
                }
                let mut longest = 1;
                for &start in cycles.keys() {
                    let mut face = start;
                    let mut seen = AtomSet::from([face]);
                    loop {
                        let following = neighbors[&face]
                            .iter()
                            .filter(|&&o| directed[&(face, o)] == 0 && !seen.contains(&o))
                            .copied()
                            .collect::<Vec<_>>();
                        if following.len() != 1 {
                            break;
                        }
                        face = following[0];
                        seen.insert(face);
                    }
                    longest = longest.max(seen.len())
                }
                for (sx, sy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                    let oriented = centers
                        .iter()
                        .map(|(&f, &(x, y))| (f, (sx * x, sy * y)))
                        .collect::<BTreeMap<_, _>>();
                    let mut score = vec![distortion, -(longest as i64)];
                    score.extend(opsin_occupied_row_orientation(&oriented));
                    if best_score.as_ref().is_some_and(|b| &score > b) {
                        continue;
                    }
                    let boundary = if sx * sy == 1 {
                        outer.clone()
                    } else {
                        outer.iter().rev().copied().collect()
                    };
                    let eligible = cycles
                        .keys()
                        .filter(|f| {
                            cycles[f]
                                .iter()
                                .any(|a| boundary.contains(a) && !fusion.contains(a))
                        })
                        .copied()
                        .collect::<Vec<_>>();
                    let Some(top) = eligible
                        .into_iter()
                        .min_by_key(|f| (-oriented[f].1, -oriented[f].0))
                    else {
                        continue;
                    };
                    let starts = boundary
                        .iter()
                        .enumerate()
                        .filter(|&(i, a)| {
                            cycles[&top].contains(a)
                                && !fusion.contains(a)
                                && fusion
                                    .contains(&boundary[(i + boundary.len() - 1) % boundary.len()])
                        })
                        .map(|(_, a)| *a)
                        .collect::<Vec<_>>();
                    if starts.len() != 1 {
                        continue;
                    }
                    #[allow(
                        clippy::unwrap_used,
                        reason = "starts contains only atoms enumerated from this boundary"
                    )]
                    let offset = boundary.iter().position(|a| *a == starts[0]).unwrap();
                    let perimeter = boundary[offset..]
                        .iter()
                        .chain(&boundary[..offset])
                        .copied()
                        .collect::<Vec<_>>();
                    if best_score.as_ref().is_none_or(|b| &score < b) {
                        best_score = Some(score.clone());
                        candidates.clear()
                    }
                    candidates
                        .entry(perimeter.clone())
                        .or_insert_with(|| EntryDirectionWitness {
                            perimeter,
                            start_face: top,
                            ring_positions: oriented
                                .iter()
                                .map(|(&f, &(x, y))| (f, x, y))
                                .collect(),
                            orientation_score: score,
                            direction_conflicts: conflicts.iter().copied().collect(),
                        });
                }
            }
        }
        let count = cycles.len() as i64;
        if !straight_only
            || (!candidates.is_empty()
                && best_score == Some(vec![0, -count, -count, count, -2 * count]))
        {
            return Ok(if candidates.is_empty() {
                None
            } else {
                Some((reference.clone(), candidates.into_values().collect()))
            });
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_direction_degeneracy_and_nonorthogonal_composition() {
        assert_eq!(
            allowed_entry_directions(5, &AtomSet::from([0])),
            vec![vec![-2, 0, 1, 3]]
        );
        assert_eq!(allowed_entry_directions(5, &(0..5).collect()).len(), 3);
        assert_eq!(
            allowed_entry_directions(6, &AtomSet::from([0, 3])),
            vec![vec![-3, -1, 0, 1, 3]]
        );
        assert_eq!(absolute_direction(3, -1), 1);
        assert_eq!(absolute_direction(1, 1), 3);
        assert_eq!(absolute_direction(4, 0), 4);
        assert_eq!(direction_steps()[&-3], (-2, -2));
    }
    #[test]
    fn deep_pentagon_entry_chains_reach_the_exact_linear_score_bound() {
        use crate::naming::openclatura::{
            fusion::faces::{cycle_edges, select_bounded_face_model, typed_face_model},
            molecule::{Atom, Bond, Molecule},
        };
        for count in [4, 8, 16] {
            let mut mol = Molecule::new();
            let mut previous: Option<Vec<usize>> = None;
            let mut edges = BTreeSet::new();
            for _ in 0..count {
                let mut ring = vec![];
                for local in 0..5 {
                    let atom = if let Some(previous) =
                        previous.as_ref().filter(|_| [1, 2].contains(&local))
                    {
                        previous[3 + local - 1]
                    } else {
                        let id = mol.atoms.len();
                        let mut atom = Atom::new(id, if local == 0 { "O" } else { "C" });
                        atom.is_aromatic = true;
                        mol.add_atom(atom).unwrap();
                        id
                    };
                    ring.push(atom);
                }
                edges.extend(cycle_edges(&ring));
                previous = Some(ring);
            }
            for (id, (a, b)) in edges.into_iter().enumerate() {
                mol.add_bond(Bond::new(id, a, b, 1)).unwrap();
            }
            let atoms = mol.atoms.keys().copied().collect();
            let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
            let model = typed_face_model(&mol, &bounded);
            let (_, witnesses) = entry_direction_geometry(&model, 25000).unwrap().unwrap();
            assert!(witnesses.len() >= 2);
            let count = count as i64;
            for witness in witnesses {
                assert_eq!(
                    witness.orientation_score,
                    vec![0, -count, -count, count, -2 * count]
                );
            }
        }
    }
}
