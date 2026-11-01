//! Graph proofs and reconstruction audits from Python `polycycle_topology.py`.
//!
//! Descriptor numberings are admitted only after rebuilding their ring edges.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::graph_kernel::{EdgeSet, connected_components, normalize_edge};
use super::molecule::{AtomSet, Molecule};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonospiroProof {
    pub descriptor_numbers: [usize; 2],
    pub spiro_atom: usize,
    pub ring_components: [AtomSet; 2],
    pub numbering_paths: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinearDispiroProof {
    pub spiro_atoms: [usize; 2],
    pub has_direct_middle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BicycloProof {
    pub descriptor_numbers: [usize; 3],
    pub bridgeheads: [usize; 2],
    pub bridge_components: Vec<AtomSet>,
    pub numbering_paths: Vec<Vec<usize>>,
}

/// One reconstructed atom/locant map. Source identities remain attached to
/// locants throughout parent selection and assembly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingNumbering {
    pub kind: String,
    pub descriptor_numbers: Vec<usize>,
    pub path: Vec<usize>,
    pub atom_to_locant: BTreeMap<usize, usize>,
    pub locant_to_atom: BTreeMap<usize, usize>,
    pub audit_ok: bool,
    pub bridgehead_locants: Vec<usize>,
}

fn bridgehead_locants(kind: &str, descriptor_numbers: &[usize]) -> Vec<usize> {
    if kind == "bicyclo" && !descriptor_numbers.is_empty() {
        vec![1, descriptor_numbers[0] + 2]
    } else {
        Vec::new()
    }
}

pub fn build_ring_numbering(
    kind: &str,
    numbers: &[usize],
    path: &[usize],
    edges: &EdgeSet,
) -> RingNumbering {
    RingNumbering {
        kind: kind.into(),
        descriptor_numbers: numbers.to_vec(),
        path: path.to_vec(),
        atom_to_locant: path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i + 1))
            .collect(),
        locant_to_atom: path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (i + 1, atom))
            .collect(),
        audit_ok: audit_ring_numbering(kind, numbers, path, edges),
        bridgehead_locants: bridgehead_locants(kind, numbers),
    }
}

pub fn build_von_baeyer_numbering(
    descriptor: &str,
    path: &[usize],
    edges: &EdgeSet,
) -> RingNumbering {
    let parsed = parse_von_baeyer_descriptor(descriptor);
    let (kind, numbers) = parsed.map_or_else(
        || ("polycycle".into(), Vec::new()),
        |ParsedVonBaeyer {
             count,
             base,
             bridges,
         }| {
            (
                super::ring_renderer::von_baeyer_kind(count),
                base.into_iter()
                    .chain(bridges.into_iter().map(|(length, _)| length))
                    .collect(),
            )
        },
    );
    RingNumbering {
        kind,
        bridgehead_locants: bridgehead_locants("bicyclo", &numbers),
        descriptor_numbers: numbers,
        path: path.to_vec(),
        atom_to_locant: path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i + 1))
            .collect(),
        locant_to_atom: path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (i + 1, atom))
            .collect(),
        audit_ok: audit_von_baeyer_descriptor(descriptor, path, edges),
    }
}

pub fn edges_within_atoms(mol: &Molecule, atoms: &AtomSet) -> EdgeSet {
    mol.bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| normalize_edge(b.u, b.v))
        .collect()
}

pub fn internal_degrees(atoms: &AtomSet, edges: &EdgeSet) -> BTreeMap<usize, usize> {
    let mut degrees: BTreeMap<_, _> = atoms.iter().map(|&atom| (atom, 0)).collect();
    for &(u, v) in edges {
        if let Some(degree) = degrees.get_mut(&u) {
            *degree += 1;
        }
        if let Some(degree) = degrees.get_mut(&v) {
            *degree += 1;
        }
    }
    degrees
}

pub fn has_multiple_spiro_centers(atoms: &AtomSet, edges: &EdgeSet) -> bool {
    internal_degrees(atoms, edges)
        .values()
        .filter(|&&degree| degree >= 4)
        .count()
        >= 2
}

fn adjacent_atoms(atom: usize, edges: &EdgeSet) -> AtomSet {
    edges
        .iter()
        .filter_map(|&(u, v)| {
            if u == atom {
                Some(v)
            } else if v == atom {
                Some(u)
            } else {
                None
            }
        })
        .collect()
}

pub fn bicyclo_proof(atoms: &AtomSet, edges: &EdgeSet) -> Option<BicycloProof> {
    let degrees = internal_degrees(atoms, edges);
    let bridgeheads: Vec<_> = degrees
        .iter()
        .filter_map(|(&atom, &degree)| (degree == 3).then_some(atom))
        .collect();
    let &[first, second] = bridgeheads.as_slice() else {
        return None;
    };
    let non_bridgehead = atoms
        .difference(&AtomSet::from([first, second]))
        .copied()
        .collect();
    let mut components = connected_components(&non_bridgehead, edges);
    for component in &components {
        let attachments: AtomSet = component
            .iter()
            .flat_map(|&atom| adjacent_atoms(atom, edges))
            .filter(|atom| bridgeheads.contains(atom))
            .collect();
        if attachments != AtomSet::from([first, second]) {
            return None;
        }
    }
    let direct = edges.contains(&normalize_edge(first, second));
    if components.len() + usize::from(direct) != 3 {
        return None;
    }
    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    let mut component_paths: Vec<_> = components
        .iter()
        .map(|component| component_path_between_centers(component, first, second, edges))
        .collect();
    if direct {
        component_paths.push(Vec::new());
    }
    let descriptor_numbers = [
        component_paths[0].len(),
        component_paths[1].len(),
        component_paths[2].len(),
    ];
    if descriptor_numbers.iter().sum::<usize>() + 2 != atoms.len() {
        return None;
    }
    let mut paths = Vec::new();
    for (start, end) in [(first, second), (second, first)] {
        let oriented: Vec<_> = component_paths
            .iter()
            .map(|path| {
                if start == first {
                    path.clone()
                } else {
                    reversed(path)
                }
            })
            .collect();
        for [a, b, c] in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            if [oriented[a].len(), oriented[b].len(), oriented[c].len()] != descriptor_numbers {
                continue;
            }
            let path: Vec<_> = std::iter::once(start)
                .chain(oriented[a].iter().copied())
                .chain(std::iter::once(end))
                .chain(oriented[b].iter().rev().copied())
                .chain(oriented[c].iter().copied())
                .collect();
            if path.iter().copied().collect::<AtomSet>() == *atoms {
                paths.push(path);
            }
        }
    }
    let numbering_paths = dedupe_paths(paths);
    (!numbering_paths.is_empty()).then_some(BicycloProof {
        descriptor_numbers,
        bridgeheads: [first, second],
        bridge_components: components,
        numbering_paths,
    })
}

pub fn monospiro_proof(atoms: &AtomSet, edges: &EdgeSet) -> Option<MonospiroProof> {
    let spiro_atoms: Vec<_> = internal_degrees(atoms, edges)
        .into_iter()
        .filter_map(|(atom, degree)| (degree >= 4).then_some(atom))
        .collect();
    let [spiro_atom] = spiro_atoms.as_slice() else {
        return None;
    };
    let non_spiro = atoms
        .difference(&AtomSet::from([*spiro_atom]))
        .copied()
        .collect();
    let mut components = connected_components(&non_spiro, edges);
    if components.len() != 2
        || components.iter().any(|component| {
            component
                .iter()
                .filter(|&&atom| adjacent_atoms(atom, edges).contains(spiro_atom))
                .count()
                != 2
        })
    {
        return None;
    }
    components.sort_by(|left, right| (left.len(), left).cmp(&(right.len(), right)));
    let descriptor_numbers = [components[0].len(), components[1].len()];
    if descriptor_numbers.iter().sum::<usize>() + 1 != atoms.len() {
        return None;
    }
    let mut starts = vec![(0, 1)];
    if descriptor_numbers[0] == descriptor_numbers[1] {
        starts.push((1, 0));
    }
    let mut paths = Vec::new();
    for (first, second) in starts {
        let first_path = component_path_between_attachments(&components[first], *spiro_atom, edges);
        let second_path =
            component_path_between_attachments(&components[second], *spiro_atom, edges);
        for oriented_first in [first_path.clone(), reversed(&first_path)] {
            for oriented_second in [second_path.clone(), reversed(&second_path)] {
                let mut path = oriented_first.clone();
                path.push(*spiro_atom);
                path.extend(oriented_second);
                paths.push(path);
            }
        }
    }
    Some(MonospiroProof {
        descriptor_numbers,
        spiro_atom: *spiro_atom,
        ring_components: [components[0].clone(), components[1].clone()],
        numbering_paths: dedupe_paths(paths),
    })
}

fn linear_dispiro_proof(atoms: &AtomSet, edges: &EdgeSet) -> Option<LinearDispiroProof> {
    let degrees = internal_degrees(atoms, edges);
    let centers: Vec<_> = degrees
        .iter()
        .filter_map(|(&atom, &degree)| (degree >= 4).then_some(atom))
        .collect();
    let [first, second] = centers.as_slice() else {
        return None;
    };
    if degrees
        .iter()
        .any(|(atom, &degree)| degree > 2 && !centers.contains(atom))
    {
        return None;
    }
    let (mut terminal, middle) = dispiro_components(atoms, edges, [*first, *second]);
    let has_direct_middle = edges.contains(&normalize_edge(*first, *second));
    if terminal.len() != 2 || middle.len() + usize::from(has_direct_middle) != 2 {
        return None;
    }
    // The proof's component order is upstream's audit order. Rendering below
    // independently applies the terminal heteroatom and size preference.
    terminal.sort_by(|left, right| {
        right
            .atoms
            .len()
            .cmp(&left.atoms.len())
            .then_with(|| left.atoms.cmp(&right.atoms))
    });
    if terminal[0].spiro_atom == terminal[1].spiro_atom {
        return None;
    }
    let descriptor_numbers = [
        terminal[0].atoms.len(),
        if has_direct_middle {
            0
        } else {
            middle[0].len()
        },
        terminal[1].atoms.len(),
        if has_direct_middle {
            middle[0].len()
        } else {
            middle[1].len()
        },
    ];
    if descriptor_numbers.iter().sum::<usize>() + 2 != atoms.len() {
        return None;
    }
    Some(LinearDispiroProof {
        spiro_atoms: [*first, *second],
        has_direct_middle,
    })
}

struct TerminalComponent {
    atoms: AtomSet,
    spiro_atom: usize,
}

fn dispiro_components(
    atoms: &AtomSet,
    edges: &EdgeSet,
    centers: [usize; 2],
) -> (Vec<TerminalComponent>, Vec<AtomSet>) {
    let center_set = AtomSet::from(centers);
    let non_spiro = atoms.difference(&center_set).copied().collect();
    let mut terminal = Vec::new();
    let mut middle = Vec::new();
    for component in connected_components(&non_spiro, edges) {
        let attachments: AtomSet = component
            .iter()
            .flat_map(|&atom| adjacent_atoms(atom, edges))
            .filter(|atom| center_set.contains(atom))
            .collect();
        if attachments.len() == 1 {
            if let Some(&attachment) = attachments.first() {
                terminal.push(TerminalComponent {
                    atoms: component,
                    spiro_atom: attachment,
                });
            }
        } else if attachments == center_set {
            middle.push(component);
        }
    }
    (terminal, middle)
}

/// Descriptor and paths from upstream `get_linear_dispiro_descriptor_and_paths`.
pub fn linear_dispiro_descriptor_and_paths(
    mol: &Molecule,
    atoms: &AtomSet,
    edges: &EdgeSet,
) -> Option<(String, Vec<Vec<usize>>)> {
    let proof = linear_dispiro_proof(atoms, edges)?;
    let (mut terminal, middle) = dispiro_components(atoms, edges, proof.spiro_atoms);
    terminal.sort_by(|left, right| {
        let [left, right] = [&left.atoms, &right.atoms].map(|component| {
            (
                usize::from(!component.iter().any(|atom| mol.atoms[atom].symbol != "C")),
                component.len(),
                component,
            )
        });
        left.cmp(&right)
    });
    let TerminalComponent {
        atoms: first_terminal,
        spiro_atom: first_spiro,
    } = &terminal[0];
    let TerminalComponent {
        atoms: second_terminal,
        spiro_atom: second_spiro,
    } = &terminal[1];
    if first_spiro == second_spiro {
        return None;
    }
    let descriptor_numbers = [
        first_terminal.len(),
        if proof.has_direct_middle {
            0
        } else {
            middle[0].len()
        },
        second_terminal.len(),
        if proof.has_direct_middle {
            middle[0].len()
        } else {
            middle[1].len()
        },
    ];
    let first_path = component_path_between_attachments(first_terminal, *first_spiro, edges);
    let second_path = component_path_between_attachments(second_terminal, *second_spiro, edges);
    let middle_path =
        component_path_between_centers(&middle[0], *first_spiro, *second_spiro, edges);
    let extra_middle_path = middle.get(1).map_or_else(Vec::new, |component| {
        component_path_between_centers(component, *first_spiro, *second_spiro, edges)
    });
    let mut paths = Vec::new();
    for oriented_first in [first_path.clone(), reversed(&first_path)] {
        for oriented_second in [second_path.clone(), reversed(&second_path)] {
            let mut path = oriented_first.clone();
            path.push(*first_spiro);
            if !proof.has_direct_middle {
                path.extend_from_slice(&middle_path);
            }
            path.push(*second_spiro);
            path.extend(oriented_second);
            path.extend(if proof.has_direct_middle {
                reversed(&middle_path)
            } else {
                reversed(&extra_middle_path)
            });
            if audit_ring_numbering("dispiro", &descriptor_numbers, &path, edges) {
                paths.push(path);
            }
        }
    }
    let paths = dedupe_paths(paths);
    if paths.is_empty() {
        return None;
    }
    Some((
        super::ring_renderer::render_ring_descriptor("dispiro", &descriptor_numbers),
        paths,
    ))
}

fn component_path_between_attachments(
    component: &AtomSet,
    center: usize,
    edges: &EdgeSet,
) -> Vec<usize> {
    let endpoints: Vec<_> = component
        .iter()
        .filter(|&&atom| adjacent_atoms(atom, edges).contains(&center))
        .copied()
        .collect();
    if endpoints.len() < 2 {
        return component.iter().copied().collect();
    }
    shortest_component_path(endpoints[0], endpoints[1], component, edges)
}

fn component_path_between_centers(
    component: &AtomSet,
    first: usize,
    second: usize,
    edges: &EdgeSet,
) -> Vec<usize> {
    let starts: Vec<_> = component
        .iter()
        .filter(|&&atom| adjacent_atoms(atom, edges).contains(&first))
        .copied()
        .collect();
    let ends: Vec<_> = component
        .iter()
        .filter(|&&atom| adjacent_atoms(atom, edges).contains(&second))
        .copied()
        .collect();
    for start in starts {
        for &end in &ends {
            let path = shortest_component_path(start, end, component, edges);
            if !path.is_empty() {
                return path;
            }
        }
    }
    component.iter().copied().collect()
}

fn shortest_component_path(
    start: usize,
    end: usize,
    component: &AtomSet,
    edges: &EdgeSet,
) -> Vec<usize> {
    let mut pending = VecDeque::from([(start, vec![start])]);
    let mut seen = AtomSet::from([start]);
    while let Some((current, path)) = pending.pop_front() {
        if current == end {
            return path;
        }
        for neighbor in adjacent_atoms(current, edges) {
            if component.contains(&neighbor) && seen.insert(neighbor) {
                let mut next_path = path.clone();
                next_path.push(neighbor);
                pending.push_back((neighbor, next_path));
            }
        }
    }
    vec![start]
}

fn reversed(path: &[usize]) -> Vec<usize> {
    path.iter().rev().copied().collect()
}

pub fn dedupe_paths(paths: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn atom_at_locant(path: &[usize], locant: usize) -> Option<usize> {
    path.get(locant.wrapping_sub(1)).copied()
}

/// Rebuild a descriptor's edges from its proposed locants and require identity.
pub fn audit_ring_numbering(
    kind: &str,
    numbers: &[usize],
    path: &[usize],
    edges: &EdgeSet,
) -> bool {
    let path_atoms: AtomSet = path.iter().copied().collect();
    let edge_atoms: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
    if path_atoms.len() != path.len() || path_atoms != edge_atoms {
        return false;
    }
    let mut expected = EdgeSet::new();
    let mut edge = |first: usize, second: usize| {
        if let (Some(u), Some(v)) = (atom_at_locant(path, first), atom_at_locant(path, second)) {
            expected.insert(normalize_edge(u, v));
        }
    };
    let expected_count = match (kind, numbers) {
        ("bicyclo", &[first, second, third]) => {
            append_bicyclo_edges(&mut edge, [first, second, third]);
            first + second + third + 2
        }
        ("spiro", &[small, large]) => {
            let center = small + 1;
            append_ring_segment(&mut edge, 1, small, center);
            append_ring_segment(&mut edge, center + 1, large, center);
            small + large + 1
        }
        ("dispiro", &[first_terminal, first_middle, second_terminal, second_middle]) => {
            let first = first_terminal + 1;
            let second = first + first_middle + 1;
            append_ring_segment(&mut edge, 1, first_terminal, first);
            if first_middle == 0 {
                edge(first, second);
            } else {
                append_bridge(&mut edge, first, second, first + 1, first_middle);
            }
            append_ring_segment(&mut edge, second + 1, second_terminal, second);
            if second_middle != 0 {
                append_bridge(
                    &mut edge,
                    second,
                    first,
                    second + second_terminal + 1,
                    second_middle,
                );
            }
            numbers.iter().sum::<usize>() + 2
        }
        _ => return false,
    };
    path.len() == expected_count && expected == *edges
}

fn append_bicyclo_edges(edge: &mut impl FnMut(usize, usize), [first, second, third]: [usize; 3]) {
    let other = first + 2;
    for locant in 1..other {
        edge(locant, locant + 1);
    }
    if second == 0 {
        edge(other, 1);
    } else {
        append_bridge(edge, other, 1, other + 1, second);
    }
    if third == 0 {
        edge(1, other);
    } else {
        append_bridge(edge, 1, other, other + second + 1, third);
    }
}

struct ParsedVonBaeyer {
    count: usize,
    base: [usize; 3],
    bridges: Vec<(usize, [usize; 2])>,
}

fn parse_von_baeyer_descriptor(descriptor: &str) -> Option<ParsedVonBaeyer> {
    let count = super::ring_renderer::von_baeyer_cycle_count(Some(descriptor))?;
    if count < 3 {
        return None;
    }
    let body = descriptor.split_once('[')?.1.strip_suffix(']')?;
    let parts: Vec<_> = body.split('.').collect();
    if parts.len() < 4 {
        return None;
    }
    let base = [
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ];
    let mut bridges = Vec::new();
    for part in &parts[3..] {
        let (length, locants) = part.split_once("^{")?;
        let (first, second) = locants.strip_suffix('}')?.split_once(',')?;
        let first: usize = first.parse().ok()?;
        let second: usize = second.parse().ok()?;
        if first == second {
            return None;
        }
        bridges.push((length.parse().ok()?, [first.min(second), first.max(second)]));
    }
    Some(ParsedVonBaeyer {
        count,
        base,
        bridges,
    })
}

/// Rebuild every primary and secondary bridge from the descriptor's locants.
pub fn audit_von_baeyer_descriptor(descriptor: &str, path: &[usize], edges: &EdgeSet) -> bool {
    let Some(ParsedVonBaeyer {
        count,
        base,
        bridges,
    }) = parse_von_baeyer_descriptor(descriptor)
    else {
        return false;
    };
    let base_count = base.iter().sum::<usize>() + 2;
    if count != bridges.len() + 2
        || base_count + bridges.iter().map(|(length, _)| length).sum::<usize>() != path.len()
    {
        return false;
    }
    let path_atoms: AtomSet = path.iter().copied().collect();
    if path_atoms.len() != path.len()
        || path_atoms != edges.iter().flat_map(|&(u, v)| [u, v]).collect()
    {
        return false;
    }
    let mut expected = EdgeSet::new();
    let mut valid = true;
    let mut edge = |first: usize, second: usize| {
        if let (Some(u), Some(v)) = (atom_at_locant(path, first), atom_at_locant(path, second)) {
            expected.insert(normalize_edge(u, v));
        } else {
            valid = false;
        }
    };
    append_bicyclo_edges(&mut edge, base);
    let mut next = base_count + 1;
    for (length, [first, second]) in bridges {
        if length == 0 {
            edge(first, second);
        } else {
            if (next..next + length).contains(&first) || (next..next + length).contains(&second) {
                return false;
            }
            append_bridge(&mut edge, first, second, next, length);
            next += length;
        }
    }
    valid && next == path.len() + 1 && expected == *edges
}

/// Rebuild the numbered graph read by OPSIN, which numbers positive secondary
/// bridges from their higher attachment locant and orders them by attachments.
/// Upstream's own audit instead numbers from the lower attachment locant.
pub fn audit_von_baeyer_opsin_reading(descriptor: &str, path: &[usize], edges: &EdgeSet) -> bool {
    let Some(ParsedVonBaeyer {
        count,
        base,
        bridges,
    }) = parse_von_baeyer_descriptor(descriptor)
    else {
        return false;
    };
    let base_count = base.iter().sum::<usize>() + 2;
    if count != bridges.len() + 2
        || base_count + bridges.iter().map(|(length, _)| length).sum::<usize>() != path.len()
        || path.iter().copied().collect::<AtomSet>().len() != path.len()
        || path.iter().copied().collect::<AtomSet>()
            != edges.iter().flat_map(|&(u, v)| [u, v]).collect()
    {
        return false;
    }
    let mut expected = EdgeSet::new();
    let mut valid = true;
    let mut edge = |first: usize, second: usize| {
        if let (Some(u), Some(v)) = (atom_at_locant(path, first), atom_at_locant(path, second)) {
            expected.insert(normalize_edge(u, v));
        } else {
            valid = false;
        }
    };
    append_bicyclo_edges(&mut edge, base);
    for &(length, [first, second]) in &bridges {
        if length == 0 {
            edge(first, second);
        }
    }
    let mut secondary: Vec<_> = bridges
        .into_iter()
        .filter(|(length, _)| *length > 0)
        .collect();
    secondary.sort_by_key(|&(length, [low, high])| std::cmp::Reverse((high, low, length)));
    let mut next = base_count + 1;
    while !secondary.is_empty() {
        let previous = secondary.len();
        let mut dependent = Vec::new();
        for (length, [low, high]) in secondary {
            if high >= next {
                dependent.push((length, [low, high]));
                continue;
            }
            append_bridge(&mut edge, high, low, next, length);
            next += length;
        }
        if dependent.len() == previous {
            return false;
        }
        secondary = dependent;
    }
    valid && next == path.len() + 1 && expected == *edges
}

fn append_ring_segment(
    edge: &mut impl FnMut(usize, usize),
    start: usize,
    length: usize,
    center: usize,
) {
    if length == 0 {
        return;
    }
    edge(center, start);
    for locant in start..start + length - 1 {
        edge(locant, locant + 1);
    }
    edge(start + length - 1, center);
}

fn append_bridge(
    edge: &mut impl FnMut(usize, usize),
    first: usize,
    second: usize,
    start: usize,
    length: usize,
) {
    edge(first, start);
    for locant in start..start + length - 1 {
        edge(locant, locant + 1);
    }
    edge(start + length - 1, second);
}

#[cfg(test)]
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;

    #[test]
    fn bicyclo_proof_reconstructs_all_equal_bridge_orders() {
        let mol = read_smiles("C1CC2CCC1C2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let edges = edges_within_atoms(&mol, &atoms);
        let proof = bicyclo_proof(&atoms, &edges).unwrap();
        assert_eq!(proof.descriptor_numbers, [2, 2, 1]);
        assert_eq!(proof.bridgeheads, [2, 5]);
        assert_eq!(
            proof.numbering_paths,
            vec![
                vec![2, 1, 0, 5, 4, 3, 6],
                vec![2, 3, 4, 5, 0, 1, 6],
                vec![5, 0, 1, 2, 3, 4, 6],
                vec![5, 4, 3, 2, 1, 0, 6],
            ]
        );
        for path in &proof.numbering_paths {
            let numbering =
                build_ring_numbering("bicyclo", &proof.descriptor_numbers, path, &edges);
            assert!(numbering.audit_ok);
            assert_eq!(numbering.bridgehead_locants, [1, 4]);
            assert_eq!(numbering.atom_to_locant[&path[3]], 4);
        }
        assert!(!audit_ring_numbering(
            "bicyclo",
            &[3, 1, 1],
            &proof.numbering_paths[0],
            &edges
        ));
    }

    #[test]
    fn von_baeyer_audit_rejects_count_locant_and_coverage_errors() {
        let path: Vec<_> = (0..9).collect();
        // bicyclo[3.2.2] with a direct secondary bridge at locants 1,5.
        let edges = EdgeSet::from([
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 6),
            (0, 6),
            (0, 7),
            (7, 8),
            (4, 8),
            (0, 4),
        ]);
        assert!(audit_von_baeyer_descriptor(
            "tricyclo[3.2.2.0^{1,5}]",
            &path,
            &edges
        ));
        for descriptor in [
            "tetracyclo[3.2.2.0^{1,5}]",
            "tricyclo[3.2.2.0^{1,6}]",
            "tricyclo[3.2.2.0^{1,10}]",
            "tricyclo[3.2.2.1^{1,5}]",
        ] {
            assert!(
                !audit_von_baeyer_descriptor(descriptor, &path, &edges),
                "{descriptor}"
            );
        }
        let mut duplicate = path;
        duplicate[8] = 7;
        assert!(!audit_von_baeyer_descriptor(
            "tricyclo[3.2.2.0^{1,5}]",
            &duplicate,
            &edges
        ));
    }

    #[test]
    fn opsin_audit_detects_secondary_bridge_direction_and_order() {
        let mol = read_smiles("OCC1C=C2CC(Cl)C1CCC1CCCCCCC2CCCCC1").unwrap();
        let path = [
            11, 12, 13, 14, 15, 16, 17, 18, 4, 3, 2, 8, 9, 10, 23, 22, 21, 20, 19, 5, 6,
        ];
        let edges = edges_within_atoms(&mol, &path.iter().copied().collect());
        let descriptor = "tricyclo[6.6.5.2^{9,12}]";
        assert!(audit_von_baeyer_descriptor(descriptor, &path, &edges));
        assert!(!audit_von_baeyer_opsin_reading(descriptor, &path, &edges));
        let mut reversed = path;
        reversed[19..].reverse();
        assert!(audit_von_baeyer_opsin_reading(
            descriptor, &reversed, &edges
        ));

        // Even one-atom bridges change identity if their descriptor order
        // differs from OPSIN's descending attachment-locant order.
        let path: Vec<_> = (0..10).collect();
        let mut edges = EdgeSet::new();
        let mut edge = |a, b| {
            edges.insert(normalize_edge(a - 1, b - 1));
        };
        append_bicyclo_edges(&mut edge, [3, 2, 1]);
        append_bridge(&mut edge, 2, 5, 9, 1);
        append_bridge(&mut edge, 3, 6, 10, 1);
        let descriptor = "tetracyclo[3.2.1.1^{2,5}.1^{3,6}]";
        assert!(audit_von_baeyer_descriptor(descriptor, &path, &edges));
        assert!(!audit_von_baeyer_opsin_reading(descriptor, &path, &edges));
        let mut reordered = path;
        reordered.swap(8, 9);
        assert!(audit_von_baeyer_opsin_reading(
            descriptor, &reordered, &edges
        ));
    }

    #[test]
    fn opsin_audit_resolves_secondary_bridge_dependencies() {
        let path: Vec<_> = (0..10).collect();
        let mut edges = EdgeSet::new();
        let mut edge = |a, b| {
            edges.insert(normalize_edge(a - 1, b - 1));
        };
        append_bicyclo_edges(&mut edge, [3, 2, 1]);
        append_bridge(&mut edge, 5, 2, 9, 1);
        append_bridge(&mut edge, 9, 3, 10, 1);
        assert!(audit_von_baeyer_opsin_reading(
            "tetracyclo[3.2.1.1^{2,5}.1^{3,9}]",
            &path,
            &edges,
        ));
        assert!(!audit_von_baeyer_opsin_reading(
            "tetracyclo[3.2.1.1^{2,10}.1^{3,9}]",
            &path,
            &edges,
        ));
    }

    #[test]
    fn monospiro_proof_matches_python_paths_and_reconstructs_every_orientation() {
        let mol = read_smiles("C1CCC2(CC1)CCCC2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let edges = edges_within_atoms(&mol, &atoms);
        let proof = monospiro_proof(&atoms, &edges).unwrap();
        assert_eq!(proof.descriptor_numbers, [4, 5]);
        assert_eq!(proof.spiro_atom, 3);
        // Installed Python 0.4.3, source atom IDs and offered path order.
        assert_eq!(
            proof.numbering_paths,
            vec![
                vec![6, 7, 8, 9, 3, 2, 1, 0, 5, 4],
                vec![6, 7, 8, 9, 3, 4, 5, 0, 1, 2],
                vec![9, 8, 7, 6, 3, 2, 1, 0, 5, 4],
                vec![9, 8, 7, 6, 3, 4, 5, 0, 1, 2],
            ]
        );
        for path in &proof.numbering_paths {
            assert!(audit_ring_numbering(
                "spiro",
                &proof.descriptor_numbers,
                path,
                &edges
            ));
        }
        let mut duplicate = proof.numbering_paths[0].clone();
        duplicate[0] = duplicate[1];
        assert!(!audit_ring_numbering("spiro", &[4, 5], &duplicate, &edges));
        assert!(!audit_ring_numbering(
            "spiro",
            &[3, 6],
            &proof.numbering_paths[0],
            &edges
        ));
    }

    #[test]
    fn equal_spiro_rings_offer_both_ring_orders_before_heteroatom_numbering() {
        let mol = read_smiles("C1CC2(C1)COC2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let proof = monospiro_proof(&atoms, &edges_within_atoms(&mol, &atoms)).unwrap();
        assert_eq!(proof.descriptor_numbers, [3, 3]);
        assert_eq!(
            proof.numbering_paths,
            vec![
                vec![1, 0, 3, 2, 4, 5, 6],
                vec![1, 0, 3, 2, 6, 5, 4],
                vec![3, 0, 1, 2, 4, 5, 6],
                vec![3, 0, 1, 2, 6, 5, 4],
                vec![4, 5, 6, 2, 1, 0, 3],
                vec![4, 5, 6, 2, 3, 0, 1],
                vec![6, 5, 4, 2, 1, 0, 3],
                vec![6, 5, 4, 2, 3, 0, 1],
            ]
        );
    }

    #[test]
    fn linear_dispiro_direct_middle_and_two_middle_paths_match_python() {
        for (smiles, descriptor, expected) in [
            (
                "C1CC12CC21CC1",
                "dispiro[2.0.2.1]",
                vec![
                    vec![0, 1, 2, 4, 5, 6, 3],
                    vec![0, 1, 2, 4, 6, 5, 3],
                    vec![1, 0, 2, 4, 5, 6, 3],
                    vec![1, 0, 2, 4, 6, 5, 3],
                ],
            ),
            (
                "C1CC2(C1)CCC1(CC2)CC1",
                "dispiro[2.2.3.2]",
                vec![
                    vec![9, 10, 6, 5, 4, 2, 1, 0, 3, 8, 7],
                    vec![9, 10, 6, 5, 4, 2, 3, 0, 1, 8, 7],
                    vec![10, 9, 6, 5, 4, 2, 1, 0, 3, 8, 7],
                    vec![10, 9, 6, 5, 4, 2, 3, 0, 1, 8, 7],
                ],
            ),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let edges = edges_within_atoms(&mol, &atoms);
            let (actual, paths) =
                linear_dispiro_descriptor_and_paths(&mol, &atoms, &edges).unwrap();
            assert_eq!(actual, descriptor);
            assert_eq!(paths, expected);
        }
    }

    #[test]
    fn terminal_heteroatom_preference_precedes_terminal_ring_size() {
        let mol = read_smiles("C1=CC12CCC1(CNCOCN1)CSC2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let edges = edges_within_atoms(&mol, &atoms);
        let (descriptor, paths) =
            linear_dispiro_descriptor_and_paths(&mol, &atoms, &edges).unwrap();
        assert_eq!(descriptor, "dispiro[6.2.2.3]");
        assert_eq!(
            paths[0],
            vec![6, 7, 8, 9, 10, 11, 5, 4, 3, 2, 0, 1, 14, 13, 12]
        );
        assert_eq!(paths.len(), 4);
        assert!(paths.iter().all(|path| audit_ring_numbering(
            "dispiro",
            &[6, 2, 2, 3],
            path,
            &edges
        )));
    }
}
