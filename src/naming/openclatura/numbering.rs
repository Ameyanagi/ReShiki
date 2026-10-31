//! Parent numbering criteria ported from Python `numbering.py`.
use super::{
    locants::{get_atom_locants, get_bond_locants, parse_locant},
    molecule::{AtomSet, Molecule},
    rules::{elements, retained},
};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default)]
pub struct NumberingTopology {
    pub is_bicycle: bool,
    pub is_spiro: bool,
    pub is_polycycle: bool,
}
impl NumberingTopology {
    fn has_fixed_paths(self) -> bool {
        self.is_bicycle || self.is_spiro || self.is_polycycle
    }
}

#[derive(Clone, Debug)]
pub struct NumberingPreference {
    pub principal: Vec<usize>,
    pub hetero_by_priority: Vec<Vec<usize>>,
    pub indicated_hydrogen: Vec<usize>,
    pub hydro: Vec<usize>,
    pub unsaturation: Vec<usize>,
    pub substituent_and_unsaturation: Vec<usize>,
    pub substituent_citation: Vec<usize>,
    pub stereochemistry: Vec<usize>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Criterion {
    Flat(Vec<usize>),
    Nested(Vec<Vec<usize>>),
}
impl Criterion {
    fn is_empty(&self) -> bool {
        match self {
            Self::Flat(v) => v.is_empty(),
            Self::Nested(v) => v.is_empty(),
        }
    }
}

impl NumberingPreference {
    #[allow(
        clippy::panic,
        reason = "Criterion names come from the immutable checked-in numbering registry."
    )]
    fn criterion(&self, name: &str) -> Criterion {
        match name {
            "principal" => Criterion::Flat(self.principal.clone()),
            "hetero_by_priority" => Criterion::Nested(self.hetero_by_priority.clone()),
            "indicated_hydrogen" => Criterion::Flat(self.indicated_hydrogen.clone()),
            "hydro" => Criterion::Flat(self.hydro.clone()),
            "unsaturation" => Criterion::Flat(self.unsaturation.clone()),
            "substituent_and_unsaturation" => {
                Criterion::Flat(self.substituent_and_unsaturation.clone())
            }
            "substituent_citation" => Criterion::Flat(self.substituent_citation.clone()),
            "stereochemistry" => Criterion::Flat(self.stereochemistry.clone()),
            _ => panic!("unknown numbering criterion {name}"),
        }
    }
    #[allow(
        clippy::expect_used,
        reason = "The immutable checked-in registry supplies arrays of criterion names."
    )]
    fn compare(&self, other: &Self, is_ring: bool) -> Ordering {
        let criteria = super::nomenclature::section("numbering_criteria")
            [if is_ring { "ring" } else { "chain" }]
        .as_array()
        .expect("numbering criteria");
        for criterion in criteria {
            let name = criterion.as_str().expect("numbering criterion");
            let a = self.criterion(name);
            let b = other.criterion(name);
            // Python intentionally prefers a nonempty structural criterion to
            // an empty one, then uses ordinary tuple lexicographic ordering.
            let order = match (a.is_empty(), b.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => a.cmp(&b),
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    }
}

#[cfg(test)]
pub fn number_parent(
    mol: &Molecule,
    candidate_paths: &[Vec<usize>],
    principal_carbons: &AtomSet,
    substituent_mapping: &BTreeMap<usize, Vec<String>>,
    is_ring: bool,
    fixed_start: bool,
    retained_name: Option<&str>,
) -> Vec<usize> {
    number_parent_with_topology(
        mol,
        candidate_paths,
        principal_carbons,
        substituent_mapping,
        is_ring,
        fixed_start,
        retained_name,
        NumberingTopology::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn number_parent_with_topology(
    mol: &Molecule,
    candidate_paths: &[Vec<usize>],
    principal_carbons: &AtomSet,
    substituent_mapping: &BTreeMap<usize, Vec<String>>,
    is_ring: bool,
    fixed_start: bool,
    retained_name: Option<&str>,
    topology: NumberingTopology,
) -> Vec<usize> {
    let Some(path) = candidate_paths.first() else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    if topology.has_fixed_paths() || fixed_start {
        candidates.extend_from_slice(candidate_paths);
    } else if is_ring {
        for i in 0..path.len() {
            let rotated: Vec<_> = path[i..].iter().chain(&path[..i]).copied().collect();
            let reversed = rotated.iter().rev().copied().collect();
            candidates.push(rotated);
            candidates.push(reversed);
        }
    } else {
        candidates.push(path.clone());
        candidates.push(path.iter().rev().copied().collect());
    }
    let Some(first) = candidates.first() else {
        return Vec::new();
    };
    let mut best = first.clone();
    let mut best_score = numbering_preference(
        mol,
        first,
        principal_carbons,
        substituent_mapping,
        retained_name,
        topology,
    );
    for candidate in &candidates[1..] {
        let evaluation = numbering_preference(
            mol,
            candidate,
            principal_carbons,
            substituent_mapping,
            retained_name,
            topology,
        );
        if evaluation.compare(&best_score, is_ring) == Ordering::Less {
            best = candidate.clone();
            best_score = evaluation;
        }
    }
    best
}

#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors the shared Python parent-numbering pipeline interface."
)]
pub fn choose_parent_numbering(
    mol: &Molecule,
    candidate_paths: &[Vec<usize>],
    principal_atoms: &AtomSet,
    substituent_mapping: &BTreeMap<usize, Vec<String>>,
    locant_maps: &[BTreeMap<usize, String>],
    is_ring: bool,
    retained_name: Option<&str>,
    fixed_start: bool,
    topology: NumberingTopology,
) -> (Vec<usize>, Option<BTreeMap<usize, String>>) {
    if !locant_maps.is_empty() {
        type L = (u8, usize, String);
        type MapScore = (
            Vec<Vec<L>>,
            Vec<L>,
            Vec<L>,
            Vec<L>,
            Vec<L>,
            Vec<(String, L)>,
        );
        let evaluate = |map: &BTreeMap<usize, String>| -> MapScore {
            let value = |idx: &usize| parse_locant(&map[idx]);
            let mut principal: Vec<_> = principal_atoms
                .iter()
                .filter(|idx| map.contains_key(idx))
                .map(value)
                .collect();
            principal.sort();
            let mut hetero: BTreeMap<usize, Vec<L>> = BTreeMap::new();
            for (&idx, atom) in &mol.atoms {
                if map.contains_key(&idx) && atom.symbol != "C" {
                    hetero
                        .entry(
                            elements::get(&atom.symbol)
                                .and_then(|e| e.hw_priority)
                                .unwrap_or(99),
                        )
                        .or_default()
                        .push(value(&idx));
                }
            }
            let hetero = hetero
                .into_values()
                .map(|mut group| {
                    group.sort();
                    group
                })
                .collect();
            let mut subst: Vec<_> = substituent_mapping
                .keys()
                .filter(|idx| map.contains_key(idx))
                .map(value)
                .collect();
            subst.sort();
            let ring: AtomSet = map.keys().copied().collect();
            let mut hydro: Vec<_> = ring
                .iter()
                .filter(|&&idx| {
                    mol.atoms.contains_key(&idx)
                        && retained::is_saturated_ring_site(mol, idx, &ring)
                })
                .map(value)
                .collect();
            hydro.sort();
            let path: Vec<_> = ring
                .iter()
                .filter(|idx| mol.atoms.contains_key(idx))
                .copied()
                .collect();
            let mut indicated: Vec<_> =
                indicated_hydrogen_like_atoms(mol, &path, retained_name, false)
                    .iter()
                    .filter(|idx| map.contains_key(idx))
                    .map(value)
                    .collect();
            indicated.sort();
            let mut citation: Vec<_> = substituent_mapping
                .iter()
                .filter(|(idx, _)| map.contains_key(idx))
                .flat_map(|(idx, items)| {
                    items
                        .iter()
                        .map(move |item| (substituent_citation_name(item), value(idx)))
                })
                .collect();
            citation.sort();
            (hetero, indicated, principal, hydro, subst, citation)
        };
        if let Some(chosen) = locant_maps.iter().min_by_key(|map| evaluate(map)).cloned() {
            let mut path: Vec<_> = chosen.keys().copied().collect();
            path.sort_by_key(|idx| parse_locant(&chosen[idx]));
            return (path, Some(chosen));
        }
    }
    let path = number_parent_with_topology(
        mol,
        candidate_paths,
        principal_atoms,
        substituent_mapping,
        is_ring,
        fixed_start,
        retained_name,
        topology,
    );
    let map = topology.has_fixed_paths().then(|| {
        path.iter()
            .enumerate()
            .map(|(i, &idx)| (idx, (i + 1).to_string()))
            .collect()
    });
    (path, map)
}

fn numbering_preference(
    mol: &Molecule,
    path: &[usize],
    principal: &AtomSet,
    substituents: &BTreeMap<usize, Vec<String>>,
    retained_name: Option<&str>,
    topology: NumberingTopology,
) -> NumberingPreference {
    let principal = get_atom_locants(path, principal);
    let hetero_by_priority = heteroatom_locants_by_priority(mol, path, !topology.has_fixed_paths());
    let indicated_hydrogen = get_atom_locants(
        path,
        &indicated_hydrogen_like_atoms(mol, path, retained_name, false),
    );
    let hydro = mancude_hydro_split(mol, path, retained_name)
        .map(|(_, hydro)| get_atom_locants(path, &hydro))
        .unwrap_or_default();
    let substituent_locants = get_atom_locants(path, &substituents.keys().copied().collect());
    let mut unsaturation = if retained_name.is_some() {
        Vec::new()
    } else {
        let (d, t) = get_bond_locants(
            mol,
            path,
            topology.is_bicycle,
            topology.is_spiro,
            topology.is_polycycle,
        );
        d.into_iter().chain(t).collect::<Vec<_>>()
    };
    unsaturation.sort();
    let mut substituent_and_unsaturation: Vec<_> = substituent_locants
        .into_iter()
        .chain(unsaturation.iter().copied())
        .collect();
    substituent_and_unsaturation.sort();
    let substituent_citation = substituent_citation_locants(path, substituents);
    let stereochemistry = path
        .iter()
        .filter_map(|idx| {
            mol.atoms[idx]
                .stereo
                .as_ref()
                .map(|s| usize::from(s != "R"))
        })
        .collect();
    NumberingPreference {
        principal,
        hetero_by_priority,
        indicated_hydrogen,
        hydro,
        unsaturation,
        substituent_and_unsaturation,
        substituent_citation,
        stereochemistry,
    }
}

fn mancude_hydro_split(
    mol: &Molecule,
    path: &[usize],
    name: Option<&str>,
) -> Option<(AtomSet, AtomSet)> {
    let plan = retained::mancude_monocycle_hydro_plan(mol, path, name)?;
    let mut citable = plan.citable;
    citable.sort_by_key(|idx| path.iter().position(|i| i == idx));
    Some((
        citable[..plan.indicated].iter().copied().collect(),
        citable[plan.indicated..].iter().copied().collect(),
    ))
}

pub fn heteroatom_locants_by_priority(
    mol: &Molecule,
    path: &[usize],
    monocycle: bool,
) -> Vec<Vec<usize>> {
    let mut groups: BTreeMap<usize, AtomSet> = BTreeMap::new();
    for &idx in path {
        let atom = &mol.atoms[&idx];
        if atom.symbol != "C" {
            groups
                .entry(
                    elements::get(&atom.symbol)
                        .and_then(|e| e.hw_priority)
                        .unwrap_or(99),
                )
                .or_default()
                .insert(idx);
        }
    }
    let by_priority: Vec<_> = groups
        .values()
        .map(|group| get_atom_locants(path, group))
        .collect();
    if !monocycle {
        return by_priority;
    }
    let mut locants = BTreeMap::new();
    for (&priority, group) in &groups {
        let real: AtomSet = group
            .difference(&mol.substituted_symbols)
            .copied()
            .collect();
        for locant in get_atom_locants(path, &real) {
            locants.insert(locant, priority);
        }
    }
    let mut key = if let Some((_, &priority)) = locants.first_key_value() {
        vec![vec![priority], locants.keys().copied().collect()]
    } else {
        vec![Vec::new(), Vec::new()]
    };
    key.extend(by_priority);
    key
}

pub fn indicated_hydrogen_like_atoms(
    mol: &Molecule,
    path: &[usize],
    name: Option<&str>,
    include_all_ring_carbons: bool,
) -> AtomSet {
    if let Some((indicated, _)) = mancude_hydro_split(mol, path, name) {
        return indicated;
    }
    if (!retained::cites_indicated_hydrogen(mol, path, name) && !include_all_ring_carbons)
        || (name == Some("tetrazole") && path.iter().any(|idx| mol.atoms[idx].charge != 0))
    {
        return AtomSet::new();
    }
    let ring: AtomSet = path.iter().copied().collect();
    path.iter()
        .filter(|&&idx| {
            let atom = &mol.atoms[&idx];
            let bonds: Vec<_> = mol
                .get_neighbors(idx)
                .iter()
                .filter(|n| ring.contains(n))
                .filter_map(|&n| mol.get_bond(idx, n))
                .collect();
            if include_all_ring_carbons {
                bonds.len() >= 2
                    && atom.total_h_count > 0
                    && ((atom.is_aromatic && atom.symbol != "C")
                        || (atom.symbol == "C" && !atom.is_aromatic))
            } else {
                matches!(
                    atom.symbol.as_str(),
                    "C" | "N" | "P" | "As" | "Sb" | "Bi" | "B" | "Si" | "Ge" | "Sn" | "Pb"
                ) && bonds.iter().map(|b| b.order).sum::<usize>() == 2
            }
        })
        .copied()
        .collect()
}

pub fn substituent_citation_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect()
}
pub fn substituent_sort_key(name: &str) -> String {
    let mut normalized = name.to_lowercase();
    loop {
        normalized = normalized
            .trim_start_matches(['(', '[', '{', ')'])
            .to_owned();
        let Some(end) = normalized.find(['-', ')']) else {
            break;
        };
        let prefix = &normalized[..end];
        let locant = |part: &str| {
            let digits = part.chars().take_while(|c| c.is_ascii_digit()).count();
            (digits > 0 && part[digits..].chars().all(|c| c.is_ascii_lowercase())) || {
                let mut chars = part.chars();
                chars.next().is_some_and(|c| "nospmc".contains(c)) && chars.all(|c| c == '\'')
            }
        };
        let allowed = matches!(
            prefix,
            "sec" | "tert" | "t" | "s" | "d" | "l" | "m" | "o" | "p" | "alpha" | "beta" | "gamma"
        ) || (!prefix.is_empty() && prefix.chars().all(|c| "ezrs".contains(c)))
            || prefix.split(',').all(locant);
        if !allowed {
            break;
        }
        normalized = normalized[end..].trim_start_matches(['-', ')']).to_owned();
    }
    normalized
}

fn substituent_citation_locants(
    path: &[usize],
    mapping: &BTreeMap<usize, Vec<String>>,
) -> Vec<usize> {
    let mut list = Vec::new();
    for (i, idx) in path.iter().enumerate() {
        if let Some(items) = mapping.get(idx) {
            for name in items {
                list.push((substituent_sort_key(name), i + 1));
            }
        }
    }
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list.into_iter().map(|(_, locant)| locant).collect()
}

#[cfg(test)]
mod tests {
    use super::super::{
        chains::{find_all_carbon_paths, find_ring_systems},
        graph_io::read_smiles,
    };
    use super::*;
    #[test]
    fn principal_group_numbering_precedes_unsaturation() {
        let mol = read_smiles("C=CCCCO").unwrap();
        let paths = find_all_carbon_paths(&mol, &AtomSet::new()).unwrap();
        let numbered = number_parent(
            &mol,
            &paths,
            &AtomSet::from([4]),
            &BTreeMap::new(),
            false,
            false,
            None,
        );
        assert_eq!(numbered[0], 4);
        assert_eq!(
            get_bond_locants(&mol, &numbered, false, false, false).0,
            vec![4]
        );
    }
    #[test]
    fn hw_numbering_prioritizes_senior_heteroatom_then_all_hetero_locants() {
        let mol = read_smiles("s1cnnc1").unwrap();
        let rings = find_ring_systems(&mol, &AtomSet::new());
        let numbered = number_parent(
            &mol,
            &rings[0].paths,
            &AtomSet::new(),
            &BTreeMap::new(),
            true,
            false,
            Some("1,3,4-thiadiazole"),
        );
        assert_eq!(mol.atoms[&numbered[0]].symbol, "S");
        assert_eq!(
            numbered
                .iter()
                .enumerate()
                .filter_map(|(i, idx)| (mol.atoms[idx].symbol == "N").then_some(i + 1))
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
    }
    #[test]
    fn alphabetic_substituent_citation_breaks_equal_locant_sets() {
        let mol = read_smiles("CCCCCCC").unwrap();
        let path = (0..7).collect::<Vec<_>>();
        let subs = BTreeMap::from([(1, vec!["methyl".into()]), (5, vec!["ethyl".into()])]);
        let numbered = number_parent(&mol, &[path], &AtomSet::new(), &subs, false, false, None);
        assert_eq!(numbered, (0..7).rev().collect::<Vec<_>>());
    }

    #[test]
    fn spiro_numbering_scores_only_the_topology_candidates() {
        let mol = read_smiles("C1CCC2(CC1)OCCO2").unwrap();
        let rings = find_ring_systems(&mol, &AtomSet::new());
        let system = &rings[0];
        let (path, map) = choose_parent_numbering(
            &mol,
            &system.paths,
            &AtomSet::new(),
            &BTreeMap::new(),
            &[],
            true,
            None,
            false,
            NumberingTopology {
                is_spiro: true,
                ..NumberingTopology::default()
            },
        );
        assert!(system.paths.contains(&path));
        assert_eq!(
            heteroatom_locants_by_priority(&mol, &path, false),
            vec![vec![1, 4]]
        );
        assert_eq!(map.as_ref().map(BTreeMap::len), Some(10));
    }
}
