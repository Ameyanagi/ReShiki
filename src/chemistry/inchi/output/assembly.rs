use super::{Assembly, Error, Output, Topology, Warning, Work, at, at_mut, index, invalid};
use crate::chemistry::{
    ELEMENTS,
    electronic::Hybridization,
    graph::{self, Graph},
    kekulize::Direction,
    ranking::Metadata,
    stereo::{
        self,
        perception::{Properties, RingCache, State},
    },
};
use std::collections::{BTreeSet, HashSet, VecDeque};

/// Build the native atom/bond ordering, isotope H atoms and provisional cache.
/// This observable intermediate stage does not apply 0D stereo, cleanup,
/// sanitization or final stereo perception. Inputs are never changed.
pub fn topology(output: &Output) -> Result<Assembly, Error> {
    if output.atoms.len() > i16::MAX as usize || output.stereo.len() > i16::MAX as usize {
        return Err(Error::Limit("the signed 16-bit native count range"));
    }
    if output.message.len() > 16_000_000 || output.log.len() > 16_000_000 {
        return Err(Error::Limit("the native diagnostic size"));
    }
    let mut result = Assembly {
        state: None,
        warnings: Vec::new(),
        unspecified_bonds: Vec::new(),
    };
    if !matches!(output.status, 0 | 1) {
        return Ok(result);
    }
    let mut graph = Graph {
        atoms: Vec::new(),
        bonds: Vec::new(),
    };
    let mut isotopes = Vec::new();
    for (id, atom) in output.atoms.iter().enumerate() {
        if atom.element.len() >= 6 || atom.bonds.len() > 20 {
            return Err(invalid("Native atom storage bounds exceeded"));
        }
        let number = ELEMENTS
            .iter()
            .position(|e| e.symbol == atom.element)
            .ok_or_else(|| invalid(format!("Unknown element {}", atom.element)))?;
        let average = at(ELEMENTS, number)?.average;
        let shift = if atom.isotopic_mass == 0 {
            0
        } else {
            i32::from(atom.isotopic_mass) - 10_000
        };
        // Atom::setIsotope narrows to unsigned short. A zero shift leaves
        // isotope unset, even when the explicit native mass field is 10000.
        let isotope = if shift == 0 {
            0
        } else {
            (shift + (average + 0.5) as i32) as u16
        };
        let radical_electrons = match atom.radical {
            0 => 0,
            2 | 3 => (atom.radical - 1) as u8,
            value => {
                result
                    .warnings
                    .push(Warning::IgnoredRadical { atom: id, value });
                0
            }
        };
        graph.atoms.push(graph::Atom {
            atomic_number: u8::try_from(number).map_err(|_| invalid("Invalid element number"))?,
            isotope,
            charge: atom.charge,
            explicit_hydrogens: atom.hydrogens[0] as u8,
            no_implicit: true,
            aromatic: false,
            radical_electrons,
        });
        // Preserve the native else-if chain when multiple isotope slots exist.
        if let Some((slot, &count)) = atom
            .hydrogens
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, h)| **h != 0)
        {
            let count =
                usize::try_from(count).map_err(|_| Error::Limit("the isotope hydrogen count"))?;
            isotopes.push((slot as u16, id, count));
        }
    }
    let mut directions = Vec::new();
    let mut seen = HashSet::new();
    for (id, atom) in output.atoms.iter().enumerate() {
        for bond in &atom.bonds {
            let other = index(bond.neighbor, output.atoms.len())?;
            if !seen.insert((id.min(other), id.max(other))) {
                continue;
            }
            if !(0..=4).contains(&bond.kind) {
                result.warnings.push(Warning::IllegalBond {
                    atom: id,
                    value: bond.kind,
                });
                return Ok(result);
            }
            if bond.kind == 4 {
                result.warnings.push(Warning::AromaticBond {
                    atom: id,
                    neighbor: other,
                });
            }
            graph.bonds.push(graph::Bond {
                a: id,
                b: other,
                order: bond.kind as u8,
                aromatic: bond.kind == 4,
            });
            result.unspecified_bonds.push(bond.kind == 0);
            directions.push(match bond.stereo {
                1 | -6 => Direction::Wedge,
                6 | -1 => Direction::Hash,
                4 => Direction::Unknown,
                3 => Direction::EitherDouble,
                _ => Direction::None,
            });
        }
    }
    let extra = isotopes
        .iter()
        .try_fold(0usize, |n, (_, _, count)| n.checked_add(*count))
        .ok_or(Error::Limit("the isotope hydrogen count"))?;
    if graph.atoms.len().saturating_add(extra) > 100_000
        || graph.bonds.len().saturating_add(extra) > 300_000
    {
        return Err(Error::Limit("the reconstructed graph size"));
    }
    for (isotope, parent, count) in isotopes {
        for _ in 0..count {
            let id = graph.atoms.len();
            graph.atoms.push(graph::Atom {
                atomic_number: 1,
                isotope,
                ..Default::default()
            });
            graph.bonds.push(graph::Bond {
                a: id,
                b: parent,
                order: 1,
                aromatic: false,
            });
            directions.push(Direction::None);
            result.unspecified_bonds.push(false);
        }
    }
    let valences = graph.provisional_valences().map_err(Error::Chemistry)?;
    if valences.iter().any(|v| v.explicit_valence > 127) {
        return Err(Error::NativeCacheBoundary);
    }
    result.state = Some(State {
        metadata: Metadata::unspecified(&graph),
        properties: Properties::unspecified(&graph),
        conjugated: vec![false; graph.bonds.len()],
        hybridizations: vec![Hybridization::Unspecified; graph.atoms.len()],
        graph,
        directions,
        valences,
        rings: RingCache::default(),
    });
    Ok(result)
}

/// Complete native graph assembly through 0D stereo and direction constraints.
/// InChI-specific cleanup, optional sanitization/H removal, and final perception
/// are later stages; this function deliberately exposes their original input.
pub fn assemble(output: &Output) -> Result<Assembly, Error> {
    let mut assembled = topology(output)?;
    let Some(state) = &mut assembled.state else {
        return Ok(assembled);
    };
    if output.stereo.is_empty() {
        return Ok(assembled);
    }
    let topology = Topology::new(&state.graph)?;
    let ranks = stereo::atom_priorities(&state.graph, &state.metadata).map_err(Error::Chemistry)?;
    for (properties, &rank) in state.properties.atoms.iter_mut().zip(&ranks) {
        properties.cip_rank = Some(rank);
    }
    let mut same = Vec::new();
    let mut opposite = Vec::new();
    let mut work = Work(20_000_000);
    for stereo in &output.stereo {
        if stereo.parity == 0 {
            continue;
        }
        match stereo.kind {
            0 => {}
            1 => {
                let [original_left, left, right, original_right] =
                    stereo.neighbors.map(|a| index(a, output.atoms.len()));
                let (original_left, left, right, original_right) =
                    (original_left?, left?, right?, original_right?);
                let Some(bond) = topology.bond(left, right)? else {
                    assembled.warnings.push(Warning::ExtendedDoubleBond);
                    continue;
                };
                let mut find = |id| -> Result<_, Error> {
                    let mut selected = None;
                    let mut extra = None;
                    for &(other, b) in at(&topology.edges, id)? {
                        work.spend(1)?;
                        if !matches!(at(&state.graph.bonds, b)?.order, 1 | 4) {
                            continue;
                        }
                        if selected.is_none_or(|(a, _)| ranks.get(other) > ranks.get(a)) {
                            if selected.is_some() {
                                extra = selected;
                            }
                            selected = Some((other, b));
                        } else {
                            extra = Some((other, b));
                        }
                    }
                    Ok((selected, extra))
                };
                let (a, extra_a) = find(left)?;
                let (b, extra_b) = find(right)?;
                let (Some((left_nbr, left_bond)), Some((right_nbr, right_bond))) = (a, b) else {
                    assembled.warnings.push(Warning::MissingStereoNeighbors);
                    continue;
                };
                let switch = (original_left == left_nbr) != (original_right == right_nbr);
                let parity = if switch && matches!(stereo.parity, 1 | 2) {
                    3 - stereo.parity
                } else {
                    stereo.parity
                };
                for (atom, selected, extra) in
                    [(left, left_bond, extra_a), (right, right_bond, extra_b)]
                {
                    if let Some((_, extra)) = extra {
                        let different_begin = (at(&state.graph.bonds, selected)?.a == atom)
                            != (at(&state.graph.bonds, extra)?.a == atom);
                        if different_begin {
                            same.push((selected, extra));
                        } else {
                            opposite.push((selected, extra));
                        }
                    }
                }
                let different_begin = (at(&state.graph.bonds, left_bond)?.a == left)
                    != (at(&state.graph.bonds, right_bond)?.a == right);
                if matches!(parity, 1 | 2) {
                    if (parity == 1) != different_begin {
                        same.push((left_bond, right_bond));
                    } else {
                        opposite.push((left_bond, right_bond));
                    }
                }
                let meta = at_mut(&mut state.metadata.bonds, bond)?;
                meta.stereo = match parity {
                    1 => 2,
                    2 => 3,
                    0 => 0,
                    _ => 1,
                };
                meta.stereo_atoms.extend([left_nbr, right_nbr]);
            }
            2 => {
                if matches!(stereo.parity, 3 | 4) {
                    continue;
                }
                let center = index(
                    stereo
                        .central_atom
                        .ok_or_else(|| invalid("Missing tetrahedral center"))?,
                    output.atoms.len(),
                )?;
                let neighbors = at(&topology.edges, center)?;
                let implicit = stereo.neighbors.first().copied() == stereo.central_atom;
                let mut odd = implicit && neighbors.len() == 3;
                let mut probe = neighbors.iter().map(|&(_, bond)| bond).collect::<Vec<_>>();
                let expected = stereo
                    .neighbors
                    .get(usize::from(implicit)..)
                    .ok_or_else(|| invalid("Missing tetrahedral neighbors"))?;
                if expected.len() != probe.len() {
                    return Err(Error::Chemistry("size mismatch".into()));
                }
                for (position, &other) in expected.iter().enumerate() {
                    let other = index(other, output.atoms.len())?;
                    let bond = topology
                        .bond(center, other)?
                        .ok_or_else(|| invalid("Missing tetrahedral bond"))?;
                    let offset = probe
                        .get(position..)
                        .and_then(|tail| tail.iter().position(|&b| b == bond))
                        .ok_or_else(|| Error::Chemistry("could not find probe element".into()))?;
                    if offset != 0 {
                        probe.swap(position, position + offset);
                        odd = !odd;
                    }
                }
                let mut tag = if stereo.parity == 1 { 2 } else { 1 };
                if odd {
                    tag = 3 - tag;
                }
                at_mut(&mut state.metadata.atoms, center)?.chiral_tag = tag;
            }
            3 => assembled.warnings.push(Warning::IgnoredAllene),
            kind => assembled.warnings.push(Warning::UnknownStereoType(kind)),
        }
    }
    if !assign_directions(&mut state.directions, &same, &opposite, &mut work)? {
        assembled.warnings.push(Warning::ConflictingBondDirections);
    }
    Ok(assembled)
}

fn assign_directions(
    directions: &mut [Direction],
    same: &[(usize, usize)],
    opposite: &[(usize, usize)],
    work: &mut Work,
) -> Result<bool, Error> {
    let mut pending = BTreeSet::new();
    let mut rules = vec![Vec::new(); directions.len()];
    // Preserve native rule order while indexing the per-bond adjacency once.
    for (pairs, flip) in [(same, false), (opposite, true)] {
        for &(a, b) in pairs {
            pending.extend([a, b]);
            if a != b {
                at_mut(&mut rules, a)?.push((b, flip));
                at_mut(&mut rules, b)?.push((a, flip));
            }
        }
    }
    let mut queue = VecDeque::new();
    while !pending.is_empty() || !queue.is_empty() {
        work.spend(1)?;
        let Some((bond, direction)) = queue.pop_front() else {
            let &first = pending
                .first()
                .ok_or_else(|| invalid("Missing pending bond"))?;
            queue.push_back((first, Direction::Up));
            continue;
        };
        let current = at_mut(directions, bond)?;
        if *current != Direction::None {
            if *current != direction {
                return Ok(false);
            }
            // A pre-directed bond remains pending in native code. Reject the
            // otherwise nonterminating raw record at the bounded-work boundary.
            continue;
        }
        *current = direction;
        pending.remove(&bond);
        for &(other, flip) in at(&rules, bond)? {
            work.spend(1)?;
            let wanted = if flip {
                if direction == Direction::Up {
                    Direction::Down
                } else {
                    Direction::Up
                }
            } else {
                direction
            };
            let current = *at(directions, other)?;
            if current != Direction::None {
                if current != wanted {
                    return Ok(false);
                }
            } else {
                queue.push_back((other, wanted));
            }
        }
    }
    Ok(true)
}
