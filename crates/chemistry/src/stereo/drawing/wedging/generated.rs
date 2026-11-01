//! Presentation preferences for newly generated molecular drawings.
//!
//! Keep these conventions separate from the pinned RDKit bond chooser. A raw
//! `wedge_molecule` call continues to reproduce that chooser exactly.
use super::{Conformer, WedgeProperties, WedgeState, at, wedge_bond};
use crate::{
    electronic,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::{AtomMetadata, BondMetadata},
    rings,
    stereo::{Point3, drawing::from_directions, perception},
};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

fn at_mut<T>(items: &mut [T], index: usize) -> Result<&mut T, String> {
    items
        .get_mut(index)
        .ok_or_else(|| "Invalid generated stereo index".into())
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or("Generated ring stereo work limit exceeded")?;
        Ok(())
    }
}

struct Topology {
    adjacent: Vec<Vec<(usize, usize)>>,
    ring_adjacent: Vec<Vec<(usize, usize)>>,
    rings: Vec<Vec<usize>>,
    members: Vec<Vec<usize>>,
    sets: Vec<HashSet<usize>>,
}
impl Topology {
    fn new(graph: &Graph, work: &mut Work) -> Result<Self, String> {
        let found =
            rings::perceive(graph, rings::Options::default()).map_err(|error| error.to_string())?;
        let mut adjacent = vec![Vec::new(); graph.atoms.len()];
        let mut ring_adjacent = adjacent.clone();
        let mut ring_bonds = vec![false; graph.bonds.len()];
        for ring in &found.bonds {
            work.spend(ring.len())?;
            for &bond in ring {
                *ring_bonds
                    .get_mut(bond)
                    .ok_or("Missing generated ring bond")? = true;
            }
        }
        for (id, bond) in graph.bonds.iter().enumerate() {
            work.spend(1)?;
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                at_mut(&mut adjacent, a)?.push((b, id));
                if *at(&ring_bonds, id)? {
                    at_mut(&mut ring_adjacent, a)?.push((b, id));
                }
            }
        }
        let mut members = vec![Vec::new(); graph.atoms.len()];
        for (id, ring) in found.atoms.iter().enumerate() {
            work.spend(ring.len())?;
            for &atom in ring {
                at_mut(&mut members, atom)?.push(id);
            }
        }
        let sets = found
            .atoms
            .iter()
            .map(|ring| ring.iter().copied().collect())
            .collect();
        Ok(Self {
            adjacent,
            ring_adjacent,
            rings: found.atoms,
            members,
            sets,
        })
    }

    fn intersection(&self, a: usize, b: usize, work: &mut Work) -> Result<Vec<usize>, String> {
        let ring = at(&self.rings, a)?;
        let set = at(&self.sets, b)?;
        work.spend(ring.len())?;
        Ok(ring
            .iter()
            .copied()
            .filter(|atom| set.contains(atom))
            .collect())
    }

    /// A simple fusion shares precisely the junction bond. Bridged rings share
    /// a longer path, even if their bridgeheads have the same local degree.
    fn junction(&self, atom: usize, work: &mut Work) -> Result<Junction, String> {
        let adjacent = at(&self.ring_adjacent, atom)?;
        if adjacent.len() != 3 {
            return Ok(Junction::Other);
        }
        let mut fusion = None;
        let mut bridged = false;
        let members = at(&self.members, atom)?;
        for (i, &a) in members.iter().enumerate() {
            for &b in members.iter().skip(i + 1) {
                let shared = self.intersection(a, b, work)?;
                if shared.len() > 2 {
                    bridged = true;
                } else if shared.len() == 2 {
                    let other = shared.into_iter().find(|&item| item != atom);
                    if let Some(other) = other
                        && adjacent.iter().any(|&(a, _)| a == other)
                    {
                        fusion = Some(other);
                    }
                }
            }
        }
        // Cage corners (cubane) share a bond with each of three rings; only
        // an atom in exactly two rings is a simple fusion.
        Ok(match (fusion, bridged) {
            (_, true) => Junction::Bridgehead,
            (Some(other), false) if members.len() == 2 => Junction::Fusion(other),
            _ => Junction::Other,
        })
    }

    fn bridge_length(
        &self,
        atom: usize,
        neighbor: usize,
        work: &mut Work,
    ) -> Result<usize, String> {
        let (mut previous, mut current, mut length) = (atom, neighbor, 1usize);
        // Degree-two interiors have a unique continuation. A substituted
        // bridge atom can have an acyclic third bond without ending the bridge.
        while at(&self.ring_adjacent, current)?.len() == 2 {
            work.spend(1)?;
            let next = at(&self.ring_adjacent, current)?
                .iter()
                .find(|&&(next, _)| next != previous)
                .map(|&(next, _)| next)
                .ok_or("Missing generated bridge continuation")?;
            (previous, current) = (current, next);
            length += 1;
            if current == atom || length > self.ring_adjacent.len() {
                return Ok(usize::MAX);
            }
        }
        Ok(length)
    }

    fn spiro_pairs(&self, atom: usize, work: &mut Work) -> Result<Vec<[usize; 2]>, String> {
        let adjacent = at(&self.ring_adjacent, atom)?;
        if at(&self.adjacent, atom)?.len() != 4 || adjacent.len() != 4 {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        let members = at(&self.members, atom)?;
        for (i, &a) in members.iter().enumerate() {
            for &b in members.iter().skip(i + 1) {
                if self.intersection(a, b, work)? != [atom] {
                    continue;
                }
                let mut pairs = Vec::new();
                for ring in [a, b] {
                    let set = at(&self.sets, ring)?;
                    let bonds = adjacent
                        .iter()
                        .filter(|&&(neighbor, _)| set.contains(&neighbor))
                        .map(|&(_, bond)| bond)
                        .collect::<Vec<_>>();
                    if let [first, second] = bonds.as_slice() {
                        pairs.push([*first, *second]);
                    }
                }
                if let [first, second] = pairs.as_slice()
                    && first.iter().all(|bond| !second.contains(bond))
                {
                    result.extend(pairs);
                }
            }
        }
        result.sort_unstable();
        result.dedup();
        Ok(result)
    }
}

#[derive(Clone, Copy)]
enum Junction {
    Other,
    Fusion(usize),
    Bridgehead,
}

fn tetrahedral(tag: u8) -> bool {
    matches!(tag, 1 | 2)
}
fn wedged(direction: Direction) -> bool {
    matches!(direction, Direction::Wedge | Direction::Hash)
}
fn opposite(direction: Direction) -> Direction {
    if direction == Direction::Wedge {
        Direction::Hash
    } else {
        Direction::Wedge
    }
}

fn clear_center(state: &mut WedgeState, top: &Topology, atom: usize) -> Result<(), String> {
    for &(_, id) in at(&top.adjacent, atom)? {
        if at(&state.graph.bonds, id)?.a == atom && wedged(*at(&state.directions, id)?) {
            *at_mut(&mut state.directions, id)? = Direction::None;
        }
    }
    Ok(())
}

fn direct(
    state: &WedgeState,
    properties: &WedgeProperties,
    conformer: &Conformer,
    bond: usize,
    atom: usize,
) -> Result<WedgeState, String> {
    let mut result = wedge_bond(state, properties, conformer, bond, atom)?;
    let edge = at_mut(&mut result.graph.bonds, bond)?;
    if edge.a != atom {
        std::mem::swap(&mut edge.a, &mut edge.b);
    }
    Ok(result)
}

fn preserves_center(
    state: &WedgeState,
    conformer: &Conformer,
    atom: usize,
) -> Result<bool, String> {
    let perceived = from_directions(
        &state.graph,
        &crate::ranking::Metadata::unspecified(&state.graph),
        &state.directions,
        Some(&conformer.positions),
        true,
    )?;
    Ok(at(&perceived.metadata.atoms, atom)?.chiral_tag
        == at(&state.metadata.atoms, atom)?.chiral_tag)
}

/// Apply generated-drawing conventions after `wedge_molecule` without changing
/// its chemical tags, coordinates, or RDKit-compatible bond-selection policy.
/// Bridgeheads without an acyclic substituent use their shortest bridge when
/// its first atom is not a stereocenter. A spiro center shows opposite wedges on
/// one ring, leaving the other ring plain. Degenerate projections are retained.
pub fn prefer_ring_wedges(
    input: &WedgeState,
    properties: &WedgeProperties,
    conformer: &Conformer,
) -> Result<WedgeState, String> {
    let mut work = Work(50_000_000);
    // Use the single-bond API's validation even when no candidate is found.
    input.graph.cached_valences(Some(&properties.valences))?;
    input.metadata.validate(&input.graph)?;
    let n = input.graph.atoms.len();
    if input.directions.len() != input.graph.bonds.len()
        || properties.attachment_points.len() != n
        || conformer.positions.len() != n
        || conformer.positions.iter().any(|point| !point.valid())
    {
        return Err("Invalid generated ring stereo annotations or coordinates".into());
    }
    if conformer.is_3d
        || !input
            .metadata
            .atoms
            .iter()
            .any(|a| tetrahedral(a.chiral_tag))
    {
        return Ok(input.clone());
    }
    let top = Topology::new(&input.graph, &mut work)?;
    let mut result = input.clone();
    for (atom, metadata) in input.metadata.atoms.iter().enumerate() {
        work.spend(1)?;
        if !tetrahedral(metadata.chiral_tag) {
            continue;
        }
        let adjacent = at(&top.adjacent, atom)?;
        let ring_adjacent = at(&top.ring_adjacent, atom)?;
        if adjacent.len() == 3
            && ring_adjacent.len() == 3
            && matches!(top.junction(atom, &mut work)?, Junction::Bridgehead)
        {
            let mut bridges = Vec::new();
            for &(neighbor, bond) in ring_adjacent {
                bridges.push((
                    top.bridge_length(atom, neighbor, &mut work)?,
                    bond,
                    neighbor,
                ));
            }
            bridges.sort_unstable();
            let shortest = at(&bridges, 0)?.0;
            if shortest > 1 && shortest != usize::MAX {
                for &(length, bond, neighbor) in &bridges {
                    if length != shortest {
                        break;
                    }
                    let edge = at(&result.graph.bonds, bond)?;
                    if at(&input.metadata.atoms, neighbor)?.chiral_tag != 0
                        || edge.order != 1
                        || (wedged(*at(&result.directions, bond)?) && edge.a != atom)
                    {
                        continue;
                    }
                    work.spend(n + input.graph.bonds.len())?;
                    let mut candidate = result.clone();
                    clear_center(&mut candidate, &top, atom)?;
                    candidate = direct(&candidate, properties, conformer, bond, atom)?;
                    if wedged(*at(&candidate.directions, bond)?)
                        && preserves_center(&candidate, conformer, atom)?
                    {
                        result = candidate;
                        break;
                    }
                }
            }
        }
        let mut pairs = top.spiro_pairs(atom, &mut work)?;
        // Preserve the raw chooser's ring when possible, then use graph order.
        let mut directed = Vec::new();
        let mut incoming = false;
        for &(_, bond) in adjacent {
            if wedged(*at(&result.directions, bond)?) {
                if at(&result.graph.bonds, bond)?.a == atom {
                    directed.push(bond);
                } else {
                    incoming = true;
                }
            }
        }
        pairs.sort_by_key(|pair| !pair.iter().any(|bond| directed.contains(bond)));
        if incoming {
            continue;
        }
        for [first, second] in pairs {
            if at(&result.graph.bonds, first)?.order != 1
                || at(&result.graph.bonds, second)?.order != 1
            {
                continue;
            }
            work.spend(n + input.graph.bonds.len())?;
            let mut candidate = result.clone();
            clear_center(&mut candidate, &top, atom)?;
            candidate = direct(&candidate, properties, conformer, first, atom)?;
            if !wedged(*at(&candidate.directions, first)?) {
                continue;
            }
            candidate = direct(&candidate, properties, conformer, second, atom)?;
            let direction = opposite(*at(&candidate.directions, first)?);
            *at_mut(&mut candidate.directions, second)? = direction;
            if preserves_center(&candidate, conformer, atom)? {
                result = candidate;
                break;
            }
        }
    }
    Ok(result)
}

/// Appended explicit hydrogens and their parent indices, in append order.
/// Existing atom/bond indices remain stable so callers can extend drawing IDs,
/// labels, and attachment masks without remapping the original molecule.
pub struct FusionHydrogens {
    pub state: perception::State,
    pub positions: Vec<Point3>,
    pub parents: Vec<usize>,
}

/// Expand undrawn hydrogens at tetrahedral fused-ring junctions for a newly
/// generated 2D drawing. Call before wedging, never on an existing drawing being
/// cleaned up. Bridgeheads and junctions with exocyclic substituents are excluded.
/// `bond_length` uses the same coordinate units as `positions`.
pub fn add_fusion_hydrogens(
    input: &perception::State,
    positions: &[Point3],
    bond_length: f64,
) -> Result<FusionHydrogens, String> {
    input.graph.cached_valences(Some(&input.valences))?;
    input.metadata.validate(&input.graph)?;
    let (n, e) = (input.graph.atoms.len(), input.graph.bonds.len());
    if positions.len() != n
        || positions.iter().any(|point| !point.valid())
        || !bond_length.is_finite()
        || !(1e-8..=1e90).contains(&bond_length)
        || input.directions.len() != e
        || input.conjugated.len() != e
        || input.hybridizations.len() != n
        || input.properties.atoms.len() != n
        || input.properties.bond_codes.len() != e
    {
        return Err("Invalid generated fusion-hydrogen annotations or coordinates".into());
    }
    let mut result = FusionHydrogens {
        state: input.clone(),
        positions: positions.to_vec(),
        parents: Vec::new(),
    };
    if !input
        .metadata
        .atoms
        .iter()
        .any(|a| tetrahedral(a.chiral_tag))
    {
        return Ok(result);
    }
    let mut work = Work(50_000_000);
    let top = Topology::new(&input.graph, &mut work)?;
    for (atom, original) in input.graph.atoms.iter().enumerate() {
        work.spend(1)?;
        let adjacent = at(&top.adjacent, atom)?;
        let ring_adjacent = at(&top.ring_adjacent, atom)?;
        if !tetrahedral(at(&input.metadata.atoms, atom)?.chiral_tag)
            || adjacent.len() != 3
            || ring_adjacent.len() != 3
            || u32::from(original.explicit_hydrogens)
                + at(&input.valences, atom)?.implicit_hydrogens
                != 1
        {
            continue;
        }
        let mut all_heavy_single = true;
        for &(neighbor, bond) in adjacent {
            all_heavy_single &= at(&input.graph.atoms, neighbor)?.atomic_number > 1
                && at(&input.graph.bonds, bond)?.order == 1;
        }
        if !all_heavy_single {
            continue;
        }
        let Junction::Fusion(shared) = top.junction(atom, &mut work)? else {
            continue;
        };
        let center = *at(positions, atom)?;
        let mut bisector = Point3::default();
        let mut valid = true;
        for &(neighbor, _) in ring_adjacent {
            if neighbor == shared {
                continue;
            }
            let point = at(positions, neighbor)?;
            let vector = Point3 {
                x: point.x - center.x,
                y: point.y - center.y,
                z: 0.0,
            };
            if let Ok(unit) = vector.unit() {
                bisector.x += unit.x;
                bisector.y += unit.y;
            } else {
                valid = false;
            }
        }
        if !valid {
            continue;
        }
        let shared = at(positions, shared)?;
        let away = Point3 {
            x: center.x - shared.x,
            y: center.y - shared.y,
            z: 0.0,
        };
        let Ok(mut bisector) = bisector.unit().or_else(|_| away.unit()) else {
            continue;
        };
        if bisector.dot(away) < 0.0 {
            bisector.x = -bisector.x;
            bisector.y = -bisector.y;
        }
        let hydrogen = result.state.graph.atoms.len();
        let position = Point3 {
            x: center.x + bond_length * bisector.x,
            y: center.y + bond_length * bisector.y,
            z: center.z,
        };
        if !position.valid() {
            return Err("Excessive generated hydrogen coordinate".into());
        }
        // The implicit/undrawn H occupies the final neighbor slot. Appending
        // its bond keeps tetrahedral winding unchanged, including [C@H] input.
        at_mut(&mut result.state.graph.atoms, atom)?.explicit_hydrogens = 0;
        result.state.graph.atoms.push(Atom {
            atomic_number: 1,
            no_implicit: true,
            ..Atom::default()
        });
        result.state.graph.bonds.push(Bond {
            a: atom,
            b: hydrogen,
            order: 1,
            aromatic: false,
        });
        result.state.metadata.atoms.push(AtomMetadata::default());
        result.state.metadata.bonds.push(BondMetadata::default());
        result.state.directions.push(Direction::None);
        result.state.properties.atoms.push(Default::default());
        result.state.properties.bond_codes.push(None);
        result.positions.push(position);
        result.parents.push(atom);
    }
    if !result.parents.is_empty() {
        result.state.valences = result.state.graph.provisional_valences()?;
        result.state.conjugated =
            electronic::conjugation_cached(&result.state.graph, Some(&result.state.valences))?;
        let tags = result
            .state
            .metadata
            .atoms
            .iter()
            .map(|a| a.chiral_tag)
            .collect::<Vec<_>>();
        result.state.hybridizations = electronic::hybridization_cached(
            &result.state.graph,
            &tags,
            &result.state.conjugated,
            Some(&result.state.valences),
        )?;
        result.state = perception::perceive(
            &result.state,
            perception::Options {
                clean: false,
                force: true,
                flag_possible: true,
            },
        )?;
    }
    Ok(result)
}
