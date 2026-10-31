//! Graph proofs and reconstruction audits from Python `polycycle_topology.py`.
//!
//! The input boundary admits cyclic blocks that are simple rings. Combining
//! those blocks at articulation atoms proves pure spiro topology without
//! searching fused or bridged cycles.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::graph_kernel::EdgeSet;
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
    pub descriptor_numbers: [usize; 4],
    pub spiro_atoms: [usize; 2],
    pub terminal_components: [AtomSet; 2],
    pub middle_components: Vec<AtomSet>,
    pub has_direct_middle: bool,
}

pub fn edges_within_atoms(mol: &Molecule, atoms: &AtomSet) -> EdgeSet {
    mol.bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| normalized_edge(b.u, b.v))
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

fn connected_components(atoms: &AtomSet, edges: &EdgeSet) -> Vec<AtomSet> {
    let mut components = Vec::new();
    let mut seen = AtomSet::new();
    for &start in atoms {
        if !seen.insert(start) {
            continue;
        }
        let mut component = AtomSet::new();
        let mut pending = VecDeque::from([start]);
        while let Some(current) = pending.pop_front() {
            component.insert(current);
            for neighbor in adjacent_atoms(current, edges) {
                if atoms.contains(&neighbor) && seen.insert(neighbor) {
                    pending.push_back(neighbor);
                }
            }
        }
        components.push(component);
    }
    components
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

pub fn linear_dispiro_proof(atoms: &AtomSet, edges: &EdgeSet) -> Option<LinearDispiroProof> {
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
    let has_direct_middle = edges.contains(&normalized_edge(*first, *second));
    if terminal.len() != 2 || middle.len() + usize::from(has_direct_middle) != 2 {
        return None;
    }
    // The proof's component order is upstream's audit order. Rendering below
    // independently applies the terminal heteroatom and size preference.
    terminal.sort_by(|left, right| {
        right
            .0
            .len()
            .cmp(&left.0.len())
            .then_with(|| left.0.cmp(&right.0))
    });
    if terminal[0].1 == terminal[1].1 {
        return None;
    }
    let descriptor_numbers = [
        terminal[0].0.len(),
        if has_direct_middle {
            0
        } else {
            middle[0].len()
        },
        terminal[1].0.len(),
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
        descriptor_numbers,
        spiro_atoms: [*first, *second],
        terminal_components: [terminal[0].0.clone(), terminal[1].0.clone()],
        middle_components: middle,
        has_direct_middle,
    })
}

type TerminalComponents = Vec<(AtomSet, usize)>;

fn dispiro_components(
    atoms: &AtomSet,
    edges: &EdgeSet,
    centers: [usize; 2],
) -> (TerminalComponents, Vec<AtomSet>) {
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
                terminal.push((component, attachment));
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
        let key = |component: &AtomSet| {
            (
                usize::from(!component.iter().any(|atom| mol.atoms[atom].symbol != "C")),
                component.len(),
                component.clone(),
            )
        };
        key(&left.0).cmp(&key(&right.0))
    });
    let (first_terminal, first_spiro) = &terminal[0];
    let (second_terminal, second_spiro) = &terminal[1];
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

fn normalized_edge(first: usize, second: usize) -> (usize, usize) {
    (first.min(second), first.max(second))
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
        if let (Some(&u), Some(&v)) = (
            path.get(first.wrapping_sub(1)),
            path.get(second.wrapping_sub(1)),
        ) {
            expected.insert(normalized_edge(u, v));
        }
    };
    let expected_count = match (kind, numbers) {
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
