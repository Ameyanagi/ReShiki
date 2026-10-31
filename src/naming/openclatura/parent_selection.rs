//! Parent preference criteria, in the Python registry's comparison order.
use super::{
    chains::{RingSystem, canonical_ranks},
    molecule::{AtomSet, Molecule},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ParentSelection {
    pub paths: Vec<Vec<usize>>,
    pub is_ring: bool,
    pub is_bicycle: bool,
    pub is_spiro: bool,
    pub is_polycycle: bool,
    pub xyz: (usize, usize, usize),
    pub polycycle_descriptor: Option<String>,
    pub fixed_start_required: bool,
}

impl ParentSelection {
    pub fn primary_path(&self) -> &[usize] {
        self.paths.first().map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn atom_set(&self) -> AtomSet {
        self.primary_path().iter().copied().collect()
    }
    pub fn with_fixed_start(mut self, fixed: bool) -> Self {
        self.fixed_start_required = fixed;
        self
    }
    pub fn requires_fixed_substituent_start(&self) -> bool {
        self.fixed_start_required || self.is_bicycle || self.is_spiro || self.is_polycycle
    }
}

type RingSeniority = (Vec<usize>, i64, i64, i64, Vec<i64>);
type ChainSeniority = (i64, i64, Vec<i64>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ParentScore {
    contains_principal: i64,
    principal_count: i64,
    senior_elements: Vec<usize>,
    ring_parent: i64,
    ring_seniority: Option<RingSeniority>,
    chain_seniority: Option<ChainSeniority>,
    multiple_bonds: i64,
    double_bonds: i64,
    split_acyl: usize,
    attached_prefixes: i64,
    path_tiebreak: Vec<usize>,
}

fn score(
    mol: &Molecule,
    path: &[usize],
    is_ring: bool,
    ring_count: usize,
    principal: &[usize],
    ranks: &BTreeMap<usize, usize>,
) -> ParentScore {
    let atoms: AtomSet = path.iter().copied().collect();
    let principal_count = principal.iter().filter(|i| atoms.contains(i)).count() as i64;
    let element_seniority = ["N", "P", "Si", "B", "O", "S", "C"];
    let hetero_seniority = ["N", "O", "S", "P", "Si", "B"];
    let symbols: Vec<&str> = path.iter().map(|i| mol.atoms[i].symbol.as_str()).collect();
    let senior_elements = symbols
        .iter()
        .filter_map(|s| element_seniority.iter().position(|e| e == s).map(|i| i + 1))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let senior_hetero = symbols
        .iter()
        .filter_map(|s| hetero_seniority.iter().position(|e| e == s).map(|i| i + 1))
        .min()
        .into_iter()
        .collect();
    let hetero_count = symbols.iter().filter(|&&s| s != "C").count() as i64;
    let hetero_counts = ["O", "S", "N", "P", "Si", "B"]
        .iter()
        .map(|s| -(symbols.iter().filter(|&&x| x == *s).count() as i64))
        .collect::<Vec<_>>();
    let (mut multiple, mut double, mut split_acyl, mut attached) = (0, 0, 0, 0);
    for &idx in path {
        for &n in mol.get_neighbors(idx) {
            if !atoms.contains(&n) {
                attached += 1;
                continue;
            }
            if idx >= n {
                continue;
            }
            if let Some(b) = mol.get_bond(idx, n) {
                multiple += i64::from(b.order > 1);
                double += i64::from(b.order == 2);
            }
        }
        if mol.atoms[&idx].symbol != "C" {
            continue;
        }
        let has_oxo = mol.get_neighbors(idx).iter().any(|&n| {
            !atoms.contains(&n)
                && mol.atoms[&n].symbol == "O"
                && mol.get_bond(idx, n).is_some_and(|b| b.order == 2)
        });
        if has_oxo
            && mol.get_neighbors(idx).iter().any(|&n| {
                !atoms.contains(&n)
                    && matches!(mol.atoms[&n].symbol.as_str(), "N" | "O" | "S")
                    && mol.get_bond(idx, n).is_some_and(|b| b.order == 1)
            })
        {
            split_acyl += 1;
        }
    }
    ParentScore {
        contains_principal: -i64::from(principal_count > 0),
        principal_count: -principal_count,
        senior_elements,
        ring_parent: -i64::from(is_ring),
        ring_seniority: is_ring.then(|| {
            (
                senior_hetero,
                -(ring_count as i64),
                -(path.len() as i64),
                -hetero_count,
                hetero_counts.clone(),
            )
        }),
        chain_seniority: (!is_ring).then(|| (-(path.len() as i64), -hetero_count, hetero_counts)),
        multiple_bonds: -multiple,
        double_bonds: -double,
        split_acyl,
        attached_prefixes: -attached,
        path_tiebreak: {
            let mut key: Vec<_> = path.iter().map(|idx| ranks[idx]).collect();
            key.sort();
            key
        },
    }
}

pub fn select_principal_parent(
    mol: &Molecule,
    chains: &[Vec<usize>],
    ring_systems: &[RingSystem],
    principal_carbons: &[usize],
) -> Option<ParentSelection> {
    let ranks = canonical_ranks(mol, None);
    let mut candidates: Vec<(ParentScore, ParentSelection)> = chains
        .iter()
        .filter(|p| !p.is_empty())
        .map(|path| {
            (
                score(mol, path, false, 0, principal_carbons, &ranks),
                ParentSelection {
                    paths: vec![path.clone()],
                    is_ring: false,
                    is_bicycle: false,
                    is_spiro: false,
                    is_polycycle: false,
                    xyz: (0, 0, 0),
                    polycycle_descriptor: None,
                    fixed_start_required: false,
                },
            )
        })
        .collect();
    candidates.extend(
        ring_systems
            .iter()
            .filter(|rs| !rs.paths.is_empty())
            .map(|rs| {
                (
                    score(
                        mol,
                        &rs.paths[0],
                        true,
                        if rs.is_polycycle {
                            3
                        } else if rs.is_bicycle || rs.is_spiro {
                            2
                        } else {
                            1
                        },
                        principal_carbons,
                        &ranks,
                    ),
                    ParentSelection {
                        paths: rs.paths.clone(),
                        is_ring: true,
                        is_bicycle: rs.is_bicycle,
                        is_spiro: rs.is_spiro,
                        is_polycycle: rs.is_polycycle,
                        xyz: (rs.x, rs.y, if rs.is_bicycle { rs.z } else { 0 }),
                        polycycle_descriptor: rs.polycycle_descriptor.clone(),
                        fixed_start_required: false,
                    },
                )
            }),
    );
    // `min_by_key` keeps the first tied item, as does Python's stable sort.
    candidates
        .into_iter()
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, selection)| selection)
}

#[cfg(test)]
mod tests {
    use super::super::{
        chains::{find_all_carbon_paths, find_ring_systems},
        graph_io::read_smiles,
    };
    use super::*;
    #[test]
    fn ring_precedes_a_longer_chain_without_principal_groups() {
        let mol = read_smiles("CCCCCCCC1CCCCC1").unwrap();
        let excluded = AtomSet::new();
        let parent = select_principal_parent(
            &mol,
            &find_all_carbon_paths(&mol, &excluded).unwrap(),
            &find_ring_systems(&mol, &excluded),
            &[],
        )
        .unwrap();
        assert!(parent.is_ring);
        assert_eq!(parent.primary_path().len(), 6);
    }
    #[test]
    fn principal_group_coverage_precedes_ring_seniority() {
        let mol = read_smiles("OCCCC1CCCCC1").unwrap();
        let excluded = AtomSet::new();
        let parent = select_principal_parent(
            &mol,
            &find_all_carbon_paths(&mol, &excluded).unwrap(),
            &find_ring_systems(&mol, &excluded),
            &[1],
        )
        .unwrap();
        assert!(!parent.is_ring);
        assert!(parent.atom_set().contains(&1));
    }
}
