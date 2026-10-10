//! OPSIN `FusedRingNumberer`, `SSSRFinder` and `Ring`, translated from
//! 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe, aa593, pm286 and OPSIN contributors; MIT (LICENSE).
//! Ring order, cyclic-list caching, shape enumeration and stable ties follow
//! the source, including its documented limitations for interior numbering.

use crate::{
    fragment_tools,
    graph::{AtomId, BondId, Element, FragmentId, Graph, GraphError},
};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

fn error(message: impl Into<String>) -> GraphError {
    GraphError(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ring {
    pub atoms: Vec<AtomId>,
    pub bonds: Vec<BondId>,
    pub neighbours: Vec<(BondId, usize)>,
    pub cyclic_atoms: Option<Vec<AtomId>>,
    pub cyclic_bonds: Option<Vec<BondId>>,
}
impl Ring {
    pub fn new(graph: &Graph, bonds: Vec<BondId>) -> Result<Self, GraphError> {
        if bonds.is_empty() {
            return Err(error("Bond list is empty"));
        }
        let mut atoms = Vec::new();
        for &bond in &bonds {
            let bond = graph.bond(bond);
            for atom in [bond.from, bond.to] {
                if !atoms.contains(&atom) {
                    atoms.push(atom);
                }
            }
        }
        if atoms.len() != bonds.len() {
            return Err(error("atomList and bondList different sizes. Ring(bond)"));
        }
        Ok(Self {
            atoms,
            bonds,
            neighbours: Vec::new(),
            cyclic_atoms: None,
            cyclic_bonds: None,
        })
    }
    pub fn size(&self) -> usize {
        self.atoms.len()
    }
    pub fn neighbour(&self, bond: BondId) -> Option<usize> {
        self.neighbours
            .iter()
            .find(|(b, _)| *b == bond)
            .map(|(_, ring)| *ring)
    }
    fn add_neighbour(&mut self, bond: BondId, ring: usize) {
        if let Some((_, existing)) = self.neighbours.iter_mut().find(|(b, _)| *b == bond) {
            *existing = ring;
        } else {
            self.neighbours.push((bond, ring));
        }
    }
    pub fn make_cyclic_lists(&mut self, graph: &Graph, start_bond: BondId, start_atom: AtomId) {
        if self.cyclic_bonds.is_some() {
            return;
        }
        let mut bonds = vec![start_bond];
        let mut atoms = vec![start_atom];
        let mut atom = start_atom;
        for _ in 0..self.size() - 1 {
            for &bond in &self.bonds {
                if bonds.contains(&bond) {
                    continue;
                }
                if let Some(next) = graph.bond(bond).other_atom(atom) {
                    bonds.push(bond);
                    atom = next;
                    atoms.push(atom);
                }
            }
        }
        self.cyclic_atoms = Some(atoms);
        self.cyclic_bonds = Some(bonds);
    }
    fn bond_index(&self, bond: BondId) -> Result<usize, GraphError> {
        self.cyclic_bonds.as_ref().and_then(|bonds| bonds.iter().position(|&id| id == bond)).ok_or_else(|| error("OPSIN bug: previous and current bond were not present in the cyclic bond list of the current ring"))
    }
}

fn symmetric_difference(first: &[BondId], second: &[BondId]) -> Vec<BondId> {
    first
        .iter()
        .filter(|id| !second.contains(id))
        .chain(second.iter().filter(|id| !first.contains(id)))
        .copied()
        .collect()
}

/// The pinned DFS and ring reduction, rather than another cycle-basis finder.
pub fn get_set_of_smallest_rings(
    graph: &Graph,
    fragment: FragmentId,
) -> Result<Vec<Ring>, GraphError> {
    fn expand(
        graph: &Graph,
        atom: AtomId,
        parent: Option<AtomId>,
        used: &mut HashSet<AtomId>,
        parents: &mut HashMap<AtomId, Option<AtomId>>,
        links: &mut Vec<BondId>,
    ) {
        used.insert(atom);
        parents.insert(atom, parent);
        for neighbour in graph.neighbours(atom) {
            if Some(neighbour) == parent {
                continue;
            }
            if used.contains(&neighbour) {
                let bond = graph
                    .bond_between(atom, neighbour)
                    .expect("Neighbour has no bond");
                if !links.contains(&bond) {
                    links.push(bond);
                }
            } else {
                expand(graph, neighbour, Some(atom), used, parents, links);
            }
        }
    }
    fn ancestors(
        graph: &Graph,
        mut atom: AtomId,
        parents: &HashMap<AtomId, Option<AtomId>>,
    ) -> Vec<BondId> {
        let mut bonds = Vec::new();
        while let Some(Some(parent)) = parents.get(&atom) {
            let bond = graph
                .bond_between(atom, *parent)
                .expect("DFS parent has no bond");
            if bonds.contains(&bond) {
                break;
            }
            bonds.push(bond);
            atom = *parent;
        }
        bonds
    }
    let Some(&root) = graph.fragment(fragment).atoms.first() else {
        return Err(error("Ring perception requires a nonempty fragment"));
    };
    let mut parents = HashMap::new();
    let mut links = Vec::new();
    expand(
        graph,
        root,
        None,
        &mut HashSet::new(),
        &mut parents,
        &mut links,
    );
    let mut rings = Vec::new();
    for bond in links {
        let edge = graph.bond(bond);
        let mut bonds = symmetric_difference(
            &ancestors(graph, edge.from, &parents),
            &ancestors(graph, edge.to, &parents),
        );
        bonds.push(bond);
        rings.push(Ring::new(graph, bonds)?);
    }
    if rings.len() > 1 {
        let mut change = true;
        while change {
            for index in 0..rings.len() {
                // Upstream assigns (rather than ORs) each reducer result to
                // change. Keep its termination and replacement ordering.
                change = false;
                let source = rings[index].bonds.clone();
                for (target, ring) in rings.iter_mut().enumerate() {
                    if target == index {
                        continue;
                    }
                    let bonds = symmetric_difference(&ring.bonds, &source);
                    if bonds.len() < ring.size() {
                        *ring = Ring::new(graph, bonds)?;
                        change = true;
                    }
                }
            }
        }
    }
    Ok(rings)
}

pub fn setup_adjacent_fused_ring_properties(rings: &mut [Ring]) {
    for index in 0..rings.len() {
        for bond in rings[index].bonds.clone() {
            for other in index + 1..rings.len() {
                if rings[other].bonds.contains(&bond) {
                    rings[other].add_neighbour(bond, index);
                    rings[index].add_neighbour(bond, other);
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionRingShape {
    EnterFromLeftHouse,
    EnterFromTopLeftHouse,
    EnterFromTopRightHouse,
    EnterFromRightHouse,
    EnterFromLeftSevenMembered,
    EnterFromTopSevenMembered,
    EnterFromRightSevenMembered,
    EnterFromBottomRightSevenMembered,
    EnterFromBottomLeftSevenMembered,
    Standard,
}
#[derive(Debug, Clone, Copy)]
struct RingShape {
    ring: usize,
    shape: FusionRingShape,
}
#[derive(Debug, Clone, Default)]
struct ConnectionTable {
    shapes: Vec<RingShape>,
    neighbours: Vec<usize>,
    directions: Vec<i32>,
    used: Vec<usize>,
}
type RingMap = Vec<Vec<Option<usize>>>;

pub fn opposite_direction(direction: i32) -> i32 {
    match direction.abs() {
        0 => 4,
        4 => 0,
        2 => -2 * direction.signum(),
        1 => -3 * direction.signum(),
        _ => -direction.signum(),
    }
}

pub fn absolute_direction(
    _shape: FusionRingShape,
    size: usize,
    relative: i32,
    previous: i32,
) -> i32 {
    let mut direction = if previous.abs() == 4 {
        if relative == 0 {
            4
        } else {
            relative - 4 * relative.signum()
        }
    } else {
        relative + previous
    };
    if direction.abs() > 4 {
        direction = -((8 - direction.abs()) * direction.signum());
    }
    if direction.abs() == 2 && (size.is_multiple_of(2) || size == 5 || size == 7) {
        if relative.abs() == 1 && previous.abs() == 3 || relative.abs() == 3 && previous.abs() == 1
        {
            direction = direction.signum();
        } else if relative.abs() == 1 && previous.abs() == 1
            || relative.abs() == 3 && previous.abs() == 3
        {
            direction = 3 * direction.signum();
        }
    }
    if direction == -4 { 4 } else { direction }
}

fn direction_from_distance(
    shape: FusionRingShape,
    size: usize,
    distance: usize,
) -> Result<i32, GraphError> {
    use FusionRingShape::*;
    let fixed: Option<&[i32]> = match (size, shape) {
        (3, _) => Some(&[-1, 1]),
        (4, _) => Some(&[-2, 0, 2]),
        (5, EnterFromLeftHouse) => Some(&[-2, 0, 1, 3]),
        (5, EnterFromTopLeftHouse | EnterFromTopRightHouse) => Some(&[-3, -1, 1, 3]),
        (5, EnterFromRightHouse) => Some(&[-3, -1, 0, 2]),
        (7, EnterFromLeftSevenMembered) => Some(&[-3, -1, 0, 1, 2, 3]),
        (7, EnterFromTopSevenMembered) => Some(&[-3, -2, -1, 1, 2, 3]),
        (7, EnterFromRightSevenMembered) => Some(&[-3, -2, -1, 0, 1, 3]),
        (7, EnterFromBottomRightSevenMembered) => Some(&[-3, -2, -1, 0, 1, 3]),
        (7, EnterFromBottomLeftSevenMembered) => Some(&[-3, -1, 0, 1, 2, 3]),
        (5 | 7, _) => {
            return Err(error(format!(
                "OPSIN Bug: Unrecognised fusion ring shape for {size} membered ring"
            )));
        }
        _ => None,
    };
    if let Some(values) = fixed {
        return distance
            .checked_sub(1)
            .and_then(|i| values.get(i))
            .copied()
            .ok_or_else(|| {
                error(format!(
                    "Impossible distance between bonds for a {size} membered ring"
                ))
            });
    }
    if size.is_multiple_of(2) {
        if distance == 1 {
            return Ok(-3);
        }
        if distance == size - 1 {
            return Ok(3);
        }
        let direction = distance as i32 - size as i32 / 2;
        return Ok(if direction.abs() > 2 && size >= 8 {
            -2 * direction.signum()
        } else {
            direction
        });
    }
    if distance == 1 {
        Ok(-3)
    } else if distance == size / 2 || distance == size / 2 + 1 {
        Ok(0)
    } else if distance == size - 1 {
        Ok(3)
    } else if distance < size / 2 {
        Ok(-2)
    } else if distance > size / 2 + 1 {
        Ok(2)
    } else {
        Err(error(
            "OPSIN Bug: Unable to determine direction between odd number of atoms ring and next ring",
        ))
    }
}

fn distance(ring: &Ring, first: BondId, second: BondId) -> Result<usize, GraphError> {
    Ok((ring.size() + ring.bond_index(second)? - ring.bond_index(first)?) % ring.size())
}
fn remove_degenerate_shapes(
    mut shapes: Vec<FusionRingShape>,
    distances: &[usize],
    size: usize,
) -> Result<Vec<FusionRingShape>, GraphError> {
    let mut distances = distances.to_vec();
    if let Some(index) = distances.iter().position(|&d| d == 0) {
        distances.remove(index);
    }
    for index in (0..shapes.len()).rev() {
        for previous in (0..index).rev() {
            let mut differs = false;
            for &distance in &distances {
                if direction_from_distance(shapes[index], size, distance)?
                    != direction_from_distance(shapes[previous], size, distance)?
                {
                    differs = true;
                    break;
                }
            }
            if !differs {
                shapes.remove(index);
                break;
            }
        }
    }
    Ok(shapes)
}
fn allowed_shapes(ring: &Ring, start: BondId) -> Result<Vec<FusionRingShape>, GraphError> {
    use FusionRingShape::*;
    let count = ring.neighbours.len();
    let mut shapes = Vec::new();
    if ring.size() == 5 {
        if count == 1 {
            shapes.push(EnterFromLeftHouse);
        } else if matches!(count, 2..=4) {
            let distances = ring
                .neighbours
                .iter()
                .map(|(bond, _)| distance(ring, start, *bond))
                .collect::<Result<Vec<_>, _>>()?;
            if !distances.contains(&1) {
                shapes.push(EnterFromLeftHouse);
            }
            if !distances.contains(&4) {
                shapes.push(EnterFromRightHouse);
            }
            if !distances.contains(&2) {
                shapes.push(EnterFromTopLeftHouse);
            } else if !distances.contains(&3) {
                shapes.push(EnterFromTopRightHouse);
            }
            shapes = remove_degenerate_shapes(shapes, &distances, 5)?;
        } else if count == 5 {
            shapes.extend([
                EnterFromLeftHouse,
                EnterFromRightHouse,
                EnterFromTopLeftHouse,
            ]);
        }
    } else if ring.size() == 7 {
        if count == 1 {
            shapes.push(EnterFromLeftSevenMembered);
        } else {
            let distances = ring
                .neighbours
                .iter()
                .map(|(bond, _)| distance(ring, start, *bond))
                .collect::<Result<Vec<_>, _>>()?;
            for (first, second, shape) in [
                (4, 6, EnterFromLeftSevenMembered),
                (1, 6, EnterFromTopSevenMembered),
                (1, 3, EnterFromRightSevenMembered),
                (2, 4, EnterFromBottomRightSevenMembered),
                (3, 5, EnterFromBottomLeftSevenMembered),
            ] {
                if !distances.contains(&first) && !distances.contains(&second) {
                    shapes.push(shape);
                }
            }
            shapes = remove_degenerate_shapes(shapes, &distances, 7)?;
        }
    } else {
        shapes.push(Standard);
    }
    Ok(shapes)
}

fn fusion_bond(rings: &[Ring], first: usize, second: usize) -> Result<BondId, GraphError> {
    rings[first]
        .bonds
        .iter()
        .copied()
        .find(|bond| rings[second].bonds.contains(bond))
        .ok_or_else(|| error("OPSIN bug: Fusion bond not found"))
}
fn atom_from_bond(ring: &Ring, bond: BondId) -> Result<AtomId, GraphError> {
    Ok(ring
        .cyclic_atoms
        .as_ref()
        .ok_or_else(|| error("The cyclic bond list should already have been generated"))?
        [(ring.bond_index(bond)? + ring.size() - 1) % ring.size()])
}

#[allow(clippy::too_many_arguments)]
fn build_connection_tables(
    graph: &Graph,
    rings: &mut [Ring],
    current: usize,
    previous: Option<usize>,
    previous_direction: i32,
    previous_bond: BondId,
    atom: AtomId,
    table: usize,
    tables: &mut Vec<ConnectionTable>,
    active: &mut Vec<usize>,
) -> Result<Vec<usize>, GraphError> {
    rings[current].make_cyclic_lists(graph, previous_bond, atom);
    let shapes = allowed_shapes(&rings[current], previous_bond)?;
    if shapes.is_empty() {
        return Err(error(
            "OPSIN limitation, unsupported ring size in fused ring numbering",
        ));
    }
    let mut generated = Vec::new();
    tables[table].used.push(current);
    for index in (0..shapes.len()).rev() {
        let current_table = if index == 0 {
            table
        } else {
            let copy = tables[table].clone();
            let id = tables.len();
            tables.push(copy);
            active.push(id);
            generated.push(id);
            id
        };
        let shape = RingShape {
            ring: current,
            shape: shapes[index],
        };
        let mut expand = vec![current_table];
        for (_, neighbour) in rings[current].neighbours.clone() {
            let bond = fusion_bond(rings, current, neighbour)?;
            let direction = if Some(neighbour) == previous {
                opposite_direction(previous_direction)
            } else {
                let distance = distance(&rings[current], previous_bond, bond)?;
                if distance == 0 {
                    return Err(error("OPSIN bug: Distance between bonds is equal to 0"));
                }
                absolute_direction(
                    shape.shape,
                    rings[current].size(),
                    direction_from_distance(shape.shape, rings[current].size(), distance)?,
                    previous_direction,
                )
            };
            for &id in &expand {
                tables[id].shapes.push(shape);
                tables[id].neighbours.push(neighbour);
                tables[id].directions.push(direction);
            }
            if !tables[current_table].used.contains(&neighbour) {
                let mut new_tables = Vec::new();
                for &id in &expand {
                    let atom = atom_from_bond(&rings[current], bond)?;
                    new_tables.extend(build_connection_tables(
                        graph,
                        rings,
                        neighbour,
                        Some(current),
                        direction,
                        bond,
                        atom,
                        id,
                        tables,
                        active,
                    )?);
                }
                expand.extend(&new_tables);
                generated.extend(new_tables);
            }
        }
    }
    Ok(generated)
}

fn starting_non_fused_bond(graph: &Graph, ring: &Ring) -> Option<BondId> {
    let mut bonds = ring.bonds.clone();
    for &(bond, _) in &ring.neighbours {
        let edge = graph.bond(bond);
        for atom in [edge.from, edge.to] {
            bonds.retain(|bond| !graph.atom(atom).bonds.contains(bond));
        }
    }
    bonds.first().copied().or_else(|| {
        ring.bonds
            .iter()
            .copied()
            .find(|&bond| ring.neighbour(bond).is_none())
    })
}

fn remove_distorted_tables(rings: &[Ring], tables: &[ConnectionTable], active: &mut Vec<usize>) {
    let counts: Vec<_> = active
        .iter()
        .map(|&id| {
            let table = &tables[id];
            let mut count = 0;
            for index in 0..table.shapes.len() {
                let first = table.shapes[index].ring;
                let second = table.neighbours[index];
                for other in index + 1..table.shapes.len() {
                    if table.shapes[other].ring == second
                        && table.neighbours[other] == first
                        && opposite_direction(table.directions[index]) != table.directions[other]
                    {
                        let _size = rings[second].size();
                        count += 1;
                    }
                }
            }
            count
        })
        .collect();
    let min = counts.iter().copied().min().unwrap_or(usize::MAX);
    let mut index = 0;
    active.retain(|_| {
        let keep = counts[index] <= min;
        index += 1;
        keep
    });
}

fn longest_chain_directions(
    tables: &[ConnectionTable],
    active: &[usize],
) -> Result<Vec<(usize, Vec<i32>)>, GraphError> {
    let mut result: Vec<(usize, Vec<i32>)> = Vec::new();
    let mut max_chain = 0;
    for &id in active {
        let table = &tables[id];
        result.push((id, Vec::new()));
        if table.shapes.len() != table.neighbours.len()
            || table.neighbours.len() != table.directions.len()
        {
            return Err(error(
                "OPSIN Bug: Sizes of arrays in fused ring numbering connection table are not equal",
            ));
        }
        for index in 0..table.shapes.len() {
            let mut neighbour = table.neighbours[index];
            let mut chain = 1;
            let direction = table.directions[index];
            for _ in 0..=table.used.len() {
                let first = table
                    .shapes
                    .iter()
                    .position(|shape| shape.ring == neighbour)
                    .ok_or_else(|| {
                        error("OPSIN bug: fused ring numbering: Ring missing from connection table")
                    })?;
                if let Some(next) = (first..table.shapes.len()).find(|&i| {
                    table.shapes[i].ring == neighbour && table.directions[i] == direction
                }) {
                    chain += 1;
                    neighbour = table.neighbours[next];
                    continue;
                }
                if chain >= max_chain {
                    let greater = chain > max_chain;
                    if greater {
                        for (_, directions) in &mut result {
                            directions.clear();
                        }
                    }
                    let directions = &mut result.last_mut().unwrap().1;
                    if greater
                        || !directions.contains(&direction)
                            && !directions.contains(&opposite_direction(direction))
                    {
                        directions.push(direction);
                    }
                    max_chain = chain;
                }
                break;
            }
            if max_chain > table.used.len() {
                return Err(error(
                    "OPSIN bug: fused ring layout contained a loop: more rings in a chain than there were rings!",
                ));
            }
        }
    }
    Ok(result)
}

fn delta_x(direction: i32) -> i32 {
    match direction.abs() {
        1 => 1,
        3 => -1,
        0 => 2,
        4 => -2,
        _ => 0,
    }
}
fn delta_y(direction: i32) -> i32 {
    if direction.abs() == 4 {
        0
    } else {
        direction.signum()
    }
}
fn generate_ring_map(
    table: &ConnectionTable,
    directions: &[i32],
) -> Result<Option<RingMap>, GraphError> {
    let mut taken = vec![table.shapes[0].ring];
    let mut coordinates = vec![(0i32, 0i32)];
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0, 0, 0, 0);
    for index in 0..table.used.len() - 1 {
        let current = *taken
            .get(index)
            .ok_or_else(|| error("OPSIN bug: Unexpected null ring in fused ring numbering"))?;
        let (x, y) = coordinates[index];
        let first = table
            .shapes
            .iter()
            .position(|shape| shape.ring == current)
            .ok_or_else(|| {
                error("OPSIN bug: fused ring numbering: Ring missing from connection table")
            })?;
        for (next, shape) in table.shapes.iter().enumerate().skip(first) {
            if shape.ring != current || taken.contains(&table.neighbours[next]) {
                continue;
            }
            let coordinate = (x + delta_x(directions[next]), y + delta_y(directions[next]));
            if taken.len() > table.used.len() {
                return Err(error("OPSIN Bug: Fused ring numbering bug"));
            }
            taken.push(table.neighbours[next]);
            coordinates.push(coordinate);
            min_x = min_x.min(coordinate.0);
            max_x = max_x.max(coordinate.0);
            min_y = min_y.min(coordinate.1);
            max_y = max_y.max(coordinate.1);
        }
    }
    if taken.len() != table.used.len() {
        return Err(error(
            "OPSIN Bug: Fused ring numbering bug, Coordinates have been calculated wrongly",
        ));
    }
    let mut map = vec![vec![None; (max_y - min_y + 1) as usize]; (max_x - min_x + 1) as usize];
    for (ring, (x, y)) in taken.into_iter().zip(coordinates) {
        let cell = &mut map[(x - min_x) as usize][(y - min_y) as usize];
        if cell.is_some() {
            return Ok(None);
        }
        *cell = Some(ring);
    }
    Ok(Some(map))
}

#[derive(Debug, Clone, Copy)]
struct Chain {
    length: usize,
    start_x: usize,
    y: usize,
}
fn maximum_horizontal_chains(map: &RingMap) -> Vec<Chain> {
    let mut chains = Vec::new();
    let mut maximum = 0;
    let width = map.len();
    // Maps are x-major; scan y in the first column's order while retaining
    // the source's x stride and its extra increment after each chain.
    for (y, _) in map[0].iter().enumerate() {
        let mut x = 0;
        while x < width {
            if map[x][y].is_some() {
                let mut length = 1;
                while x + 2 * length < width && map[x + 2 * length][y].is_some() {
                    length += 1;
                }
                if length > maximum {
                    chains.clear();
                    maximum = length;
                }
                if length >= maximum {
                    chains.push(Chain {
                        length,
                        start_x: x,
                        y,
                    });
                }
                x += 2 * length;
            }
            x += 1;
        }
    }
    chains
}
fn count_quadrants(map: &RingMap, mid_x: usize, chain_y: usize) -> [f64; 4] {
    let mut quadrants = [0.0; 4];
    for (x, column) in map.iter().enumerate() {
        for (y, ring) in column.iter().enumerate() {
            if ring.is_none() {
                continue;
            }
            if x == mid_x || y == chain_y {
                if x == mid_x && y > chain_y {
                    quadrants[0] += 0.5;
                    quadrants[1] += 0.5;
                } else if x == mid_x && y < chain_y {
                    quadrants[2] += 0.5;
                    quadrants[3] += 0.5;
                } else if x < mid_x && y == chain_y {
                    quadrants[1] += 0.5;
                    quadrants[2] += 0.5;
                } else if x > mid_x && y == chain_y {
                    quadrants[0] += 0.5;
                    quadrants[3] += 0.5;
                }
                if x == mid_x && y == chain_y {
                    for quadrant in &mut quadrants {
                        *quadrant += 0.25;
                    }
                }
            } else if x > mid_x && y > chain_y {
                quadrants[0] += 1.0;
            } else if x < mid_x && y > chain_y {
                quadrants[1] += 1.0;
            } else if x < mid_x && y < chain_y {
                quadrants[2] += 1.0;
            } else if x > mid_x && y < chain_y {
                quadrants[3] += 1.0;
            }
        }
    }
    quadrants
}
fn rules_bcd(quadrants: &[[f64; 4]]) -> Result<Vec<Vec<usize>>, GraphError> {
    if quadrants.is_empty() {
        return Err(error("OPSIN Bug: Fused ring numbering, no chains found?"));
    }
    let maximum = quadrants.iter().flatten().copied().fold(0.0, f64::max);
    let mut choices: Vec<Vec<usize>> = quadrants
        .iter()
        .map(|quadrants| (0..4).filter(|&i| quadrants[i] == maximum).collect())
        .collect();
    let mut minimum_opposite = f64::MAX;
    for (quadrants, choices) in quadrants.iter().zip(&choices) {
        for &choice in choices {
            minimum_opposite = minimum_opposite.min(quadrants[(choice + 2) % 4]);
        }
    }
    for (quadrants, choices) in quadrants.iter().zip(&mut choices) {
        choices.retain(|&i| quadrants[(i + 2) % 4] == minimum_opposite);
    }
    let mut maximum_above = 0.0f64;
    for (quadrants, choices) in quadrants.iter().zip(&choices) {
        for &choice in choices {
            maximum_above = maximum_above.max(quadrants[choice] + quadrants[choice ^ 1]);
        }
    }
    for (quadrants, choices) in quadrants.iter().zip(&mut choices) {
        choices.retain(|&i| quadrants[i] + quadrants[i ^ 1] == maximum_above);
    }
    Ok(choices)
}
fn transform_quadrant(map: &RingMap, quadrant: usize) -> RingMap {
    let width = map.len();
    let height = map[0].len();
    let mut transformed = vec![vec![None; height]; width];
    for (x, column) in map.iter().enumerate() {
        for (y, &ring) in column.iter().enumerate() {
            let new_x = if matches!(quadrant, 1 | 2) {
                width - x - 1
            } else {
                x
            };
            let new_y = if matches!(quadrant, 2 | 3) {
                height - y - 1
            } else {
                y
            };
            transformed[new_x][new_y] = ring;
        }
    }
    transformed
}
fn ring_position(map: &RingMap, ring: usize) -> Result<(usize, usize), GraphError> {
    for (x, column) in map.iter().enumerate() {
        if let Some(y) = column.iter().position(|&id| id == Some(ring)) {
            return Ok((x, y));
        }
    }
    Err(error(
        "OPSIN Bug: Ring not found in ringMap when performing fused ring numbering",
    ))
}
fn clockwise_ring(
    map: &RingMap,
    rings: &[Ring],
    ring: usize,
    visited: &[usize],
) -> Result<Option<usize>, GraphError> {
    let mut result = None;
    let (mut max_x, mut max_y) = (0, 0);
    for &(_, neighbour) in &rings[ring].neighbours {
        if visited.contains(&neighbour) {
            continue;
        }
        let (x, y) = ring_position(map, neighbour)?;
        if x > max_x || x == max_x && y > max_y {
            max_x = x;
            max_y = y;
            result = Some(neighbour);
        }
    }
    Ok(result)
}
fn upper_left_neighbour(map: &RingMap, rings: &[Ring], ring: usize) -> Result<usize, GraphError> {
    let mut result = None;
    let (mut min_x, mut max_y) = (usize::MAX, 0);
    for &(_, neighbour) in &rings[ring].neighbours {
        let (x, y) = ring_position(map, neighbour)?;
        if y > max_y || y == max_y && x < min_x {
            min_x = x;
            max_y = y;
            result = Some(neighbour);
        }
    }
    result.ok_or_else(|| error("OPSIN bug: Upper right ring has no neighbour"))
}
fn order_atoms(
    graph: &Graph,
    rings: &[Ring],
    map: &RingMap,
    inverse: bool,
    atom_count: usize,
) -> Result<Vec<AtomId>, GraphError> {
    let height = map[0].len();
    let mut upper_right = map
        .iter()
        .rev()
        .find_map(|column| column[height - 1])
        .ok_or_else(|| {
            error("OPSIN Bug: Upper right ring not found when performing fused ring numbering")
        })?;
    let mut visited = vec![upper_right];
    while rings[upper_right]
        .atoms
        .iter()
        .all(|&atom| graph.atom(atom).bonds.len() >= 3)
    {
        upper_right = clockwise_ring(map, rings, upper_right, &visited)?.ok_or_else(|| {
            error("OPSIN Bug: Unabled to find clockwise ring without fusion atoms")
        })?;
        visited.push(upper_right);
    }
    let previous = upper_left_neighbour(map, rings, upper_right)?;
    let mut previous_bond = fusion_bond(rings, upper_right, previous)?;
    let mut current = upper_right;
    let mut path = Vec::new();
    let mut count = 0;
    'path: while count <= atom_count {
        let ring = &rings[current];
        let size = ring.size();
        let mut start = ring.bond_index(previous_bond)?;
        let bonds = ring.cyclic_bonds.as_ref().ok_or_else(|| {
            error("OPSIN bug: cyclic bond set should have already been populated")
        })?;
        let atoms = ring.cyclic_atoms.as_ref().ok_or_else(|| {
            error("OPSIN bug: cyclic atom set should have already been populated")
        })?;
        let mut next_bond = None;
        for offset in 0..size {
            let index = if inverse {
                (start + size - offset - 1) % size
            } else {
                (start + offset + 1) % size
            };
            if ring.neighbour(bonds[index]).is_some() {
                next_bond = Some(bonds[index]);
                break;
            }
        }
        let next_bond = next_bond.ok_or_else(|| {
            error(
                "OPSIN Bug: None of the bonds from this ring were fused, but this is not possible ",
            )
        })?;
        let next = ring
            .neighbour(next_bond)
            .expect("Fusion bond has no neighbour");
        let mut end = ring.bond_index(next_bond)?;
        if !inverse {
            if (end + size - start) % size != 1 {
                start = (start + 1) % size;
                end = (end + size - 1) % size;
                if start > end {
                    end += size;
                }
                for index in start..=end {
                    let atom = atoms[index % size];
                    if path.contains(&atom) {
                        break 'path;
                    }
                    path.push(atom);
                }
            }
        } else if (start + size - end) % size != 1 {
            start = (start + size - 2) % size;
            if start < end {
                start += size;
            }
            for index in (end..=start).rev() {
                let atom = atoms[index % size];
                if path.contains(&atom) {
                    break 'path;
                }
                path.push(atom);
            }
        }
        previous_bond = next_bond;
        current = next;
        count += 1;
    }
    if count == atom_count {
        return Err(error(
            "OPSIN Bug: Fused ring numbering may have been stuck in an infinite loop while enumerating peripheral numbering",
        ));
    }
    Ok(path)
}
fn possible_periphery_orders(
    graph: &Graph,
    rings: &mut [Ring],
    atom_count: usize,
) -> Result<Vec<Vec<AtomId>>, GraphError> {
    let terminal = (0..rings.len())
        .min_by_key(|&i| rings[i].neighbours.len())
        .ok_or_else(|| error("OPSIN bug: Unable to find a terminal ring in fused ring system"))?;
    let start = starting_non_fused_bond(graph, &rings[terminal])
        .ok_or_else(|| error("OPSIN Bug: Non-fused bond from terminal ring not found"))?;
    let mut tables = vec![ConnectionTable::default()];
    let mut active = vec![0];
    build_connection_tables(
        graph,
        rings,
        terminal,
        None,
        0,
        start,
        graph.bond(start).from,
        0,
        &mut tables,
        &mut active,
    )?;
    remove_distorted_tables(rings, &tables, &mut active);
    let directions = longest_chain_directions(&tables, &active)?;
    let mut maps = Vec::new();
    for (id, horizontal_directions) in directions {
        let table = &tables[id];
        if table.shapes.is_empty()
            || table.shapes.len() != table.neighbours.len()
            || table.neighbours.len() != table.directions.len()
        {
            return Err(error(
                "OPSIN Bug: Sizes of arrays in fused ring numbering connection table are not equal",
            ));
        }
        for horizontal in horizontal_directions {
            let directions: Vec<_> = table
                .shapes
                .iter()
                .zip(&table.directions)
                .map(|(shape, &direction)| {
                    absolute_direction(
                        shape.shape,
                        rings[shape.ring].size(),
                        direction,
                        -horizontal,
                    )
                })
                .collect();
            if let Some(map) = generate_ring_map(table, &directions)? {
                maps.push(map);
            }
        }
    }
    if maps.is_empty() {
        return Err(error(
            "Fused ring systems with overlapping rings such as in helices cannot currently be numbered",
        ));
    }
    let mut quadrants = Vec::new();
    let mut corresponding_maps = Vec::new();
    for (index, map) in maps.iter().enumerate() {
        for chain in maximum_horizontal_chains(map) {
            quadrants.push(count_quadrants(
                map,
                chain.length + chain.start_x - 1,
                chain.y,
            ));
            corresponding_maps.push(index);
        }
    }
    let choices = rules_bcd(&quadrants)?;
    let mut paths = Vec::new();
    for (index, choices) in choices.iter().enumerate() {
        for &quadrant in choices {
            let map = transform_quadrant(&maps[corresponding_maps[index]], quadrant);
            paths.push(order_atoms(
                graph,
                rings,
                &map,
                matches!(quadrant, 0 | 2),
                atom_count,
            )?);
        }
    }
    Ok(paths)
}

fn heteroatom_priority(element: Element) -> i32 {
    use Element::*;
    match element {
        Hg => 2,
        Tl => 3,
        In => 4,
        Ga => 5,
        Al => 6,
        B => 7,
        Pb => 8,
        Sn => 9,
        Ge => 10,
        Si => 11,
        Bi => 12,
        Sb => 13,
        As => 14,
        P => 15,
        N => 16,
        Te => 17,
        Se => 18,
        S => 19,
        O => 20,
        I => 21,
        Br => 22,
        Cl => 23,
        F => 24,
        _ => 0,
    }
}
fn compare_sequences(graph: &Graph, first: &[AtomId], second: &[AtomId]) -> Ordering {
    if first.len() != second.len() {
        return Ordering::Equal;
    }
    let is_fusion_carbon =
        |id: AtomId| graph.atom(id).element == Element::C && graph.atom(id).bonds.len() >= 3;
    let non_fusion_first: Vec<_> = first
        .iter()
        .copied()
        .filter(|&id| !is_fusion_carbon(id))
        .collect();
    let non_fusion_second: Vec<_> = second
        .iter()
        .copied()
        .filter(|&id| !is_fusion_carbon(id))
        .collect();
    for (&first, &second) in non_fusion_first.iter().zip(&non_fusion_second) {
        let first_hetero = graph.atom(first).element != Element::C;
        let second_hetero = graph.atom(second).element != Element::C;
        if first_hetero != second_hetero {
            return second_hetero.cmp(&first_hetero);
        }
    }
    for (&first, &second) in non_fusion_first.iter().zip(&non_fusion_second) {
        let result = heteroatom_priority(graph.atom(second).element)
            .cmp(&heteroatom_priority(graph.atom(first).element));
        if result != Ordering::Equal {
            return result;
        }
    }
    for (&first, &second) in first.iter().zip(second) {
        let result = is_fusion_carbon(second).cmp(&is_fusion_carbon(first));
        if result != Ordering::Equal {
            return result;
        }
    }
    for (&first, &second) in first.iter().zip(second) {
        let result =
            (graph.atom(second).bonds.len() >= 3).cmp(&(graph.atom(first).bonds.len() >= 3));
        if result != Ordering::Equal {
            return result;
        }
    }
    Ordering::Equal
}

pub fn number_fused_ring(graph: &mut Graph, fragment: FragmentId) -> Result<(), GraphError> {
    let mut rings = get_set_of_smallest_rings(graph, fragment)?;
    if rings.len() < 2 {
        return Err(error(
            "Ring perception system found less than 2 rings within input fragment!",
        ));
    }
    let atoms = graph.fragment(fragment).atoms.clone();
    setup_adjacent_fused_ring_properties(&mut rings);
    for ring in &rings {
        if ring.size() <= 2 {
            return Err(error(format!("Invalid ring size: {}", ring.size())));
        }
    }
    if rings
        .iter()
        .any(|ring| ring.size() > 8 && ring.neighbours.len() > 2)
    {
        for atom in atoms {
            graph.clear_locants(atom);
        }
        return Ok(());
    }
    let mut paths = possible_periphery_orders(graph, &mut rings, atoms.len())?;
    if paths.is_empty() {
        for atom in atoms {
            graph.clear_locants(atom);
        }
        return Ok(());
    }
    for path in &mut paths {
        for &atom in &atoms {
            if !path.contains(&atom) {
                path.push(atom);
            }
        }
    }
    paths.sort_by(|first, second| compare_sequences(graph, first, second));
    fragment_tools::relabel_locants_as_fused_ring_system(graph, &paths[0]);
    graph.fragment_mut(fragment).atoms = paths.remove(0);
    Ok(())
}
