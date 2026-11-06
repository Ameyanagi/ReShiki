//! Parent numbering criteria ported from Python `numbering.py`.
use super::{
    assembly_prefixes::substituent_sort_key,
    locants::{
        LocantKey, get_atom_locants, get_bond_locants, parse_locant, retained_locant_sort_key,
    },
    molecule::{AtomSet, Molecule},
    rules::{elements, retained},
};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, Default)]
pub struct NumberingTopology {
    pub is_assembly: bool,
    pub is_bicycle: bool,
    pub is_spiro: bool,
    pub is_polycycle: bool,
}

/// Comparisons used only after every existing parent-numbering criterion ties.
#[derive(Default)]
pub struct NumberingModifiers {
    pub nitrogen_substituents: BTreeMap<usize, Vec<String>>,
    pub ester_alcohols: BTreeMap<usize, String>,
    pub acid_isotopes: Vec<(usize, String, u16)>,
}

#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
struct ModifierPreference {
    nitrogen_locants: Vec<usize>,
    nitrogen_citation: Vec<usize>,
    isotope_locants: Vec<Vec<LocantKey>>,
    alcohol_citation: Vec<LocantKey>,
}

impl NumberingModifiers {
    fn preference(&self, map: &BTreeMap<usize, String>) -> ModifierPreference {
        let mut nitrogens: Vec<_> = self
            .nitrogen_substituents
            .iter()
            .filter(|(carbon, _)| map.contains_key(carbon))
            .collect();
        nitrogens.sort_by_key(|(carbon, _)| parse_locant(&map[carbon]));
        let mut nitrogen_locants = Vec::new();
        let mut nitrogen_citation = Vec::new();
        for (index, (_, names)) in nitrogens.into_iter().enumerate() {
            for name in names {
                nitrogen_locants.push(index);
                nitrogen_citation.push((substituent_sort_key(name), index));
            }
        }
        nitrogen_locants.sort();
        nitrogen_citation.sort();
        let mut alcohol_citation: Vec<_> = self
            .ester_alcohols
            .iter()
            .filter_map(|(carbon, name)| {
                map.get(carbon)
                    .map(|locant| (substituent_sort_key(name), retained_locant_sort_key(locant)))
            })
            .collect();
        alcohol_citation.sort();
        let mut isotope_locants = Vec::new();
        let mut isotopes_by_priority: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (atom, symbol, mass) in &self.acid_isotopes {
            if let Some(locant) = map.get(atom) {
                let locant = retained_locant_sort_key(locant);
                isotope_locants.push(locant.clone());
                if let Some(element) = elements::get(symbol) {
                    isotopes_by_priority
                        .entry((element.atomic_number, *mass))
                        .or_default()
                        .push(locant);
                }
            }
        }
        isotope_locants.sort();
        // P-82.5.2: all modified positions, then higher atomic and mass number.
        let isotope_locants = std::iter::once(isotope_locants)
            .chain(isotopes_by_priority.into_values().rev().map(|mut locants| {
                locants.sort();
                locants
            }))
            .collect();
        ModifierPreference {
            nitrogen_locants,
            nitrogen_citation: nitrogen_citation
                .into_iter()
                .map(|(_, locant)| locant)
                .collect(),
            isotope_locants,
            alcohol_citation: alcohol_citation
                .into_iter()
                .map(|(_, locant)| locant)
                .collect(),
        }
    }

    fn path_preference(&self, path: &[usize]) -> ModifierPreference {
        let map = path
            .iter()
            .enumerate()
            .map(|(index, &carbon)| (carbon, (index + 1).to_string()))
            .collect();
        self.preference(&map)
    }
}
impl NumberingTopology {
    fn has_fixed_paths(self) -> bool {
        self.is_assembly || self.is_bicycle || self.is_spiro || self.is_polycycle
    }
}

#[derive(Clone, Debug)]
pub struct NumberingPreference {
    pub free_valence: Vec<usize>,
    pub skeletal_anions: Vec<usize>,
    pub principal: Vec<usize>,
    pub hetero_by_priority: Vec<Vec<usize>>,
    pub indicated_hydrogen: Vec<usize>,
    pub hydro: Vec<usize>,
    pub unsaturation: Vec<usize>,
    pub double_bonds: Vec<usize>,
    pub substituent_and_unsaturation: Vec<usize>,
    pub substituent_citation: Vec<usize>,
    pub stereochemistry: Vec<usize>,
}

#[derive(Clone, Copy)]
enum NumberingCriterion {
    FreeValence,
    SkeletalAnions,
    Principal,
    HeteroByPriority,
    IndicatedHydrogen,
    Hydro,
    Unsaturation,
    DoubleBonds,
    SubstituentAndUnsaturation,
    SubstituentCitation,
    Stereochemistry,
}

#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "The immutable checked-in registry supplies arrays of known criterion names."
)]
fn numbering_criteria(parent_kind: &str) -> Vec<NumberingCriterion> {
    let mut criteria: Vec<_> = super::nomenclature::section("numbering_criteria")[parent_kind]
        .as_array()
        .expect("numbering criteria")
        .iter()
        .map(
            |criterion| match criterion.as_str().expect("numbering criterion") {
                "principal" => NumberingCriterion::Principal,
                "hetero_by_priority" => NumberingCriterion::HeteroByPriority,
                "indicated_hydrogen" => NumberingCriterion::IndicatedHydrogen,
                "hydro" => NumberingCriterion::Hydro,
                "unsaturation" => NumberingCriterion::Unsaturation,
                "double_bonds" => NumberingCriterion::DoubleBonds,
                "substituent_and_unsaturation" => NumberingCriterion::SubstituentAndUnsaturation,
                "substituent_citation" => NumberingCriterion::SubstituentCitation,
                "stereochemistry" => NumberingCriterion::Stereochemistry,
                name => panic!("unknown numbering criterion {name}"),
            },
        )
        .collect();
    // P-72.6.3: substituent free valences precede anionic centres, which
    // precede characteristic suffixes (P-72.5.3, P-74.1.1). Fixed heteroatom
    // and indicated-hydrogen numbering keeps its existing priority.
    if let Some(principal) = criteria
        .iter()
        .position(|criterion| matches!(criterion, NumberingCriterion::Principal))
    {
        criteria.insert(principal, NumberingCriterion::SkeletalAnions);
        criteria.insert(principal, NumberingCriterion::FreeValence);
    }
    criteria
}

static CHAIN_CRITERIA: LazyLock<Vec<NumberingCriterion>> =
    LazyLock::new(|| numbering_criteria("chain"));
static RING_CRITERIA: LazyLock<Vec<NumberingCriterion>> =
    LazyLock::new(|| numbering_criteria("ring"));

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Criterion<'a> {
    Flat(&'a [usize]),
    Nested(&'a [Vec<usize>]),
}
impl Criterion<'_> {
    fn is_empty(&self) -> bool {
        match self {
            Self::Flat(v) => v.is_empty(),
            Self::Nested(v) => v.is_empty(),
        }
    }
}

impl NumberingPreference {
    fn criterion(&self, criterion: NumberingCriterion) -> Criterion<'_> {
        match criterion {
            NumberingCriterion::FreeValence => Criterion::Flat(&self.free_valence),
            NumberingCriterion::SkeletalAnions => Criterion::Flat(&self.skeletal_anions),
            NumberingCriterion::Principal => Criterion::Flat(&self.principal),
            NumberingCriterion::HeteroByPriority => Criterion::Nested(&self.hetero_by_priority),
            NumberingCriterion::IndicatedHydrogen => Criterion::Flat(&self.indicated_hydrogen),
            NumberingCriterion::Hydro => Criterion::Flat(&self.hydro),
            NumberingCriterion::Unsaturation => Criterion::Flat(&self.unsaturation),
            NumberingCriterion::DoubleBonds => Criterion::Flat(&self.double_bonds),
            NumberingCriterion::SubstituentAndUnsaturation => {
                Criterion::Flat(&self.substituent_and_unsaturation)
            }
            NumberingCriterion::SubstituentCitation => Criterion::Flat(&self.substituent_citation),
            NumberingCriterion::Stereochemistry => Criterion::Flat(&self.stereochemistry),
        }
    }
    fn compare(&self, other: &Self, is_ring: bool) -> Ordering {
        let criteria = if is_ring {
            &RING_CRITERIA
        } else {
            &CHAIN_CRITERIA
        };
        criteria
            .iter()
            .map(|&criterion| {
                let (a, b) = (self.criterion(criterion), other.criterion(criterion));
                // Python intentionally prefers a nonempty structural criterion to
                // an empty one, then uses ordinary tuple lexicographic ordering.
                match (a.is_empty(), b.is_empty()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    (false, false) => a.cmp(&b),
                }
            })
            .find(|order| order.is_ne())
            .unwrap_or(Ordering::Equal)
    }
}

#[allow(clippy::too_many_arguments)]
fn number_parent(
    mol: &Molecule,
    modifiers: &NumberingModifiers,
    candidate_paths: &[Vec<usize>],
    principal_carbons: &[usize],
    root: Option<usize>,
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
        root,
        substituent_mapping,
        retained_name,
        topology,
    );
    for candidate in &candidates[1..] {
        let evaluation = numbering_preference(
            mol,
            candidate,
            principal_carbons,
            root,
            substituent_mapping,
            retained_name,
            topology,
        );
        if evaluation.compare(&best_score, is_ring).then_with(|| {
            modifiers
                .path_preference(candidate)
                .cmp(&modifiers.path_preference(&best))
        }) == Ordering::Less
        {
            best = candidate.clone();
            best_score = evaluation;
        }
    }
    best
}

pub type ProvenHydrogenLocants = BTreeMap<Vec<(usize, String)>, (Vec<String>, Vec<String>)>;

#[allow(clippy::too_many_arguments)]
pub fn choose_parent_numbering(
    mol: &Molecule,
    modifiers: &NumberingModifiers,
    candidate_paths: &[Vec<usize>],
    principal_atoms: &[usize],
    root: Option<usize>,
    substituent_mapping: &BTreeMap<usize, Vec<String>>,
    locant_maps: &[BTreeMap<usize, String>],
    is_ring: bool,
    retained_name: Option<&str>,
    fixed_start: bool,
    topology: NumberingTopology,
    proven_hydrogen_locants: Option<&ProvenHydrogenLocants>,
) -> (Vec<usize>, Option<BTreeMap<usize, String>>) {
    if !locant_maps.is_empty() {
        type MapScore = (
            Vec<Vec<LocantKey>>,
            Vec<LocantKey>,
            Vec<LocantKey>,
            Vec<LocantKey>,
            Vec<LocantKey>,
            Vec<LocantKey>,
            Vec<LocantKey>,
            Vec<(String, LocantKey)>,
            Vec<(LocantKey, usize)>,
            Vec<usize>,
            ModifierPreference,
        );
        let evaluate = |map: &BTreeMap<usize, String>| -> MapScore {
            let value = |idx: &usize| retained_locant_sort_key(&map[idx]);
            let free_valence = root
                .iter()
                .filter(|idx| map.contains_key(idx))
                .map(value)
                .collect();
            let mut skeletal_anions: Vec<_> = map
                .keys()
                .filter(|idx| mol.atoms.get(idx).is_some_and(|atom| atom.charge < 0))
                .map(value)
                .collect();
            skeletal_anions.sort();
            let mut principal: Vec<_> = principal_atoms
                .iter()
                .filter(|idx| map.contains_key(idx))
                .map(value)
                .collect();
            principal.sort();
            let mut hetero: BTreeMap<usize, Vec<LocantKey>> = BTreeMap::new();
            for (&idx, atom) in &mol.atoms {
                if !topology.is_assembly && map.contains_key(&idx) && atom.symbol != "C" {
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
            let mut path: Vec<_> = ring
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
            if let Some(proven) = proven_hydrogen_locants
                && let Some((indicated_locants, hydro_locants)) = proven.get(
                    &map.iter()
                        .map(|(&idx, locant)| (idx, locant.clone()))
                        .collect::<Vec<_>>(),
                )
            {
                indicated = indicated_locants
                    .iter()
                    .map(|l| retained_locant_sort_key(l))
                    .collect();
                indicated.sort();
                hydro = hydro_locants
                    .iter()
                    .map(|l| retained_locant_sort_key(l))
                    .collect();
                hydro.sort();
            }
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
            let mut stereo: Vec<_> = map
                .keys()
                .filter(|_| topology.is_assembly)
                .filter_map(|idx| {
                    mol.atoms[idx]
                        .stereo
                        .as_ref()
                        // P-31.1.4.3.4: R before S, r before s.
                        .map(|s| (value(idx), usize::from(!matches!(s.as_str(), "R" | "r"))))
                })
                .collect();
            stereo.sort();
            path.sort_by_key(value);
            // Other retained/fused maps do not score atom stereo. Axial
            // descriptors are scored on every map.
            (
                hetero,
                indicated,
                free_valence,
                skeletal_anions,
                principal,
                hydro,
                subst,
                citation,
                stereo,
                stereochemistry_preference(mol, &path, false),
                modifiers.preference(map),
            )
        };
        if let Some(chosen) = locant_maps.iter().min_by_key(|map| evaluate(map)).cloned() {
            let mut path: Vec<_> = chosen.keys().copied().collect();
            path.sort_by_key(|idx| retained_locant_sort_key(&chosen[idx]));
            return (path, Some(chosen));
        }
    }
    let path = number_parent(
        mol,
        modifiers,
        candidate_paths,
        principal_atoms,
        root,
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
    principal: &[usize],
    root: Option<usize>,
    substituents: &BTreeMap<usize, Vec<String>>,
    retained_name: Option<&str>,
    topology: NumberingTopology,
) -> NumberingPreference {
    let free_valence = get_atom_locants(path, &root.into_iter().collect());
    let skeletal_anions = get_atom_locants(
        path,
        &path
            .iter()
            .filter(|idx| mol.atoms.get(idx).is_some_and(|atom| atom.charge < 0))
            .copied()
            .collect(),
    );
    // A parent atom can carry several external principal groups.
    let mut principal: Vec<_> = principal
        .iter()
        .filter_map(|atom| path.iter().position(|candidate| candidate == atom))
        .map(|position| position + 1)
        .collect();
    principal.sort_unstable();
    let hetero_by_priority = heteroatom_locants_by_priority(mol, path, !topology.has_fixed_paths());
    let indicated_hydrogen = get_atom_locants(
        path,
        &indicated_hydrogen_like_atoms(mol, path, retained_name, false),
    );
    let hydro = mancude_hydro_split(mol, path, retained_name)
        .map(|(_, hydro)| get_atom_locants(path, &hydro))
        .unwrap_or_default();
    let substituent_locants = get_atom_locants(path, &substituents.keys().copied().collect());
    // P-31.1.1.1: low locants to multiple bonds as a set, then to double bonds.
    let (mut double_bonds, triple_bonds) = if retained_name.is_some() {
        Default::default()
    } else {
        get_bond_locants(
            mol,
            path,
            topology.is_bicycle,
            topology.is_spiro,
            topology.is_polycycle,
        )
    };
    let mut unsaturation: Vec<_> = double_bonds.iter().chain(&triple_bonds).copied().collect();
    unsaturation.sort();
    double_bonds.sort();
    let mut substituent_and_unsaturation: Vec<_> = substituent_locants
        .into_iter()
        .chain(unsaturation.iter().copied())
        .collect();
    substituent_and_unsaturation.sort();
    let substituent_citation = substituent_citation_locants(path, substituents);
    let stereochemistry = stereochemistry_preference(mol, path, true);
    NumberingPreference {
        free_valence,
        skeletal_anions,
        principal,
        hetero_by_priority,
        indicated_hydrogen,
        hydro,
        unsaturation,
        double_bonds,
        substituent_and_unsaturation,
        substituent_citation,
        stereochemistry,
    }
}

/// P-31.1.4.3.4: after structural numbering ties, prefer R to S and M to P
/// at the first differing locant. An internal axis is cited once, at its lower
/// locant; an axis leaving this parent is cited at the parent endpoint.
fn stereochemistry_preference(
    mol: &Molecule,
    path: &[usize],
    include_atom_descriptors: bool,
) -> Vec<usize> {
    let positions: BTreeMap<_, _> = path
        .iter()
        .enumerate()
        .map(|(position, &atom)| (atom, position))
        .collect();
    let mut descriptors = Vec::new();
    for (position, &idx) in path.iter().enumerate() {
        if include_atom_descriptors && let Some(stereo) = mol.atoms[&idx].stereo.as_ref() {
            descriptors.push(usize::from(stereo != "R"));
        }
        for &neighbor in mol.get_neighbors(idx) {
            if positions
                .get(&neighbor)
                .is_some_and(|&other| other < position)
            {
                continue;
            }
            if let Some(stereo @ ("M" | "P")) = mol
                .get_bond(idx, neighbor)
                .and_then(|bond| bond.stereo.as_deref())
            {
                descriptors.push(usize::from(stereo != "M"));
            }
        }
    }
    descriptors
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
        || (name == Some("tetrazole") && path.iter().any(|idx| mol.atoms[idx].charge > 0))
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
    fn late_nitrogen_numbering_preserves_multiplicity_before_citation() {
        let mol = read_smiles("CCC").unwrap();
        let modifiers = NumberingModifiers {
            nitrogen_substituents: BTreeMap::from([
                (0, vec!["ethyl".into()]),
                (2, vec!["methyl".into(), "methyl".into()]),
            ]),
            ..NumberingModifiers::default()
        };
        let path = number_parent(
            &mol,
            &modifiers,
            &[vec![0, 1, 2]],
            &[0, 2],
            None,
            &BTreeMap::new(),
            false,
            false,
            None,
            NumberingTopology::default(),
        );
        assert_eq!(path, vec![2, 1, 0]);

        let modifiers = NumberingModifiers {
            nitrogen_substituents: BTreeMap::from([
                (0, vec!["methyl".into()]),
                (2, vec!["ethyl".into()]),
            ]),
            ..NumberingModifiers::default()
        };
        assert!(modifiers.path_preference(&[2, 1, 0]) < modifiers.path_preference(&[0, 1, 2]));
    }

    #[test]
    fn existing_carbon_numbering_precedes_late_nitrogen_and_alcohol_citation() {
        let mol = read_smiles("CCCCC").unwrap();
        let modifiers = NumberingModifiers {
            nitrogen_substituents: BTreeMap::from([
                (0, vec!["methyl".into()]),
                (4, vec!["ethyl".into(), "ethyl".into()]),
            ]),
            ester_alcohols: BTreeMap::from([(0, "methyl".into()), (4, "ethyl".into())]),
            ..NumberingModifiers::default()
        };
        for maps in [
            Vec::new(),
            vec![
                BTreeMap::from([
                    (0, "1".into()),
                    (1, "2".into()),
                    (2, "3".into()),
                    (3, "4".into()),
                    (4, "5".into()),
                ]),
                BTreeMap::from([
                    (4, "1".into()),
                    (3, "2".into()),
                    (2, "3".into()),
                    (1, "4".into()),
                    (0, "5".into()),
                ]),
            ],
        ] {
            let (path, _) = choose_parent_numbering(
                &mol,
                &modifiers,
                &[vec![4, 3, 2, 1, 0]],
                &[0, 4],
                None,
                &BTreeMap::from([(1, vec!["methyl".into()])]),
                &maps,
                false,
                None,
                false,
                NumberingTopology::default(),
                None,
            );
            assert_eq!(path, vec![0, 1, 2, 3, 4]);
        }
    }

    #[test]
    fn ordinary_and_retained_numbering_prefer_m_to_p() {
        let mut mol = read_smiles("c1(-c2ccccc2)ccc(-c2ccccc2)cc1").unwrap();
        for bond in mol.bonds.values_mut() {
            bond.stereo = match (bond.u, bond.v) {
                (0, 1) => Some("M".into()),
                (9, 10) => Some("P".into()),
                _ => None,
            };
        }
        let preferred = vec![0, 7, 8, 9, 16, 17];
        let other = vec![9, 8, 7, 0, 17, 16];
        let substituents = BTreeMap::from([(0, vec!["phenyl".into()]), (9, vec!["phenyl".into()])]);
        let numbered = number_parent(
            &mol,
            &NumberingModifiers::default(),
            &[other.clone(), preferred.clone()],
            &[],
            None,
            &substituents,
            true,
            true,
            Some("benzene"),
            NumberingTopology::default(),
        );
        assert_eq!(numbered, preferred);
        let maps: Vec<BTreeMap<_, _>> = [other, preferred.clone()]
            .iter()
            .map(|path| {
                path.iter()
                    .enumerate()
                    .map(|(i, &a)| (a, (i + 1).to_string()))
                    .collect()
            })
            .collect();
        let (numbered, map) = choose_parent_numbering(
            &mol,
            &NumberingModifiers::default(),
            &[],
            &[],
            None,
            &substituents,
            &maps,
            true,
            Some("benzene"),
            false,
            NumberingTopology::default(),
            None,
        );
        assert_eq!(numbered, preferred);
        assert_eq!(map, Some(maps[1].clone()));
        assert_eq!(stereochemistry_preference(&mol, &[0, 1], false), vec![0]);
    }

    #[test]
    fn double_bonds_take_low_locants_only_after_multiple_bonds_tie() {
        for (smiles, principal, expected) in [
            ("C#CCC=C", vec![], vec![4, 3, 2, 1, 0]),
            ("C#CCC=C", vec![0], vec![0, 1, 2, 3, 4]),
            ("C#CCC=CC", vec![], vec![0, 1, 2, 3, 4, 5]),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let path: Vec<_> = (0..mol.atoms.len()).collect();
            let numbered = number_parent(
                &mol,
                &NumberingModifiers::default(),
                &[path],
                &principal,
                None,
                &BTreeMap::new(),
                false,
                false,
                None,
                NumberingTopology::default(),
            );
            assert_eq!(numbered, expected, "{smiles}");
        }
    }

    #[test]
    fn repeated_principal_groups_choose_the_lowest_locant_sequence() {
        let mol = read_smiles("CC").unwrap();
        for path in [vec![0, 1], vec![1, 0]] {
            let numbered = number_parent(
                &mol,
                &NumberingModifiers::default(),
                &[path],
                &[0, 1, 1],
                None,
                &BTreeMap::new(),
                false,
                false,
                None,
                NumberingTopology::default(),
            );
            assert_eq!(numbered, [1, 0]);
        }
    }

    #[test]
    fn principal_group_numbering_precedes_unsaturation() {
        let mol = read_smiles("C=CCCCO").unwrap();
        let paths = find_all_carbon_paths(&mol, &AtomSet::new()).unwrap();
        let numbered = number_parent(
            &mol,
            &NumberingModifiers::default(),
            &paths,
            &[4],
            None,
            &BTreeMap::new(),
            false,
            false,
            None,
            NumberingTopology::default(),
        );
        assert_eq!(numbered[0], 4);
        assert_eq!(
            get_bond_locants(&mol, &numbered, false, false, false).0,
            vec![4]
        );
    }

    #[test]
    fn skeletal_anions_precede_characteristic_suffix_locants() {
        for (smiles, path, principal, expected, is_ring) in [
            (
                "CC(C)[CH-]C(=O)C(C)C",
                vec![0, 1, 3, 4, 6, 8],
                vec![4],
                vec![0, 1, 3, 4, 6, 8],
                false,
            ),
            (
                "[NH3+]CC[CH-]C#N",
                vec![1, 2, 3],
                vec![1],
                vec![3, 2, 1],
                false,
            ),
            (
                "[C-]#CCCC(=O)[O-]",
                vec![0, 1, 2, 3, 4],
                vec![4],
                vec![0, 1, 2, 3, 4],
                false,
            ),
            (
                "[CH-]1CCC(CC1)S(=O)(=O)[O-]",
                vec![0, 1, 2, 3, 4, 5],
                vec![3],
                vec![0, 1, 2, 3, 4, 5],
                true,
            ),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let (numbered, _) = choose_parent_numbering(
                &mol,
                &NumberingModifiers::default(),
                &[path],
                &principal,
                None,
                &BTreeMap::new(),
                &[],
                is_ring,
                None,
                false,
                NumberingTopology::default(),
                None,
            );
            assert_eq!(numbered, expected, "{smiles}");
        }
    }

    #[test]
    fn skeletal_anions_precede_unsaturation_and_prefixes() {
        for (smiles, path, substituents, expected) in [
            (
                "OC1=CC(=O)C=C[CH-]1",
                vec![1, 2, 3, 5, 6, 7],
                BTreeMap::from([(1, vec!["hydroxy".into()]), (3, vec!["oxo".into()])]),
                vec![7, 1, 2, 3, 5, 6],
            ),
            (
                "O=C1C=CC=C[CH-]1",
                vec![1, 2, 3, 4, 5, 6],
                BTreeMap::from([(1, vec!["oxo".into()])]),
                vec![6, 5, 4, 3, 2, 1],
            ),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let (numbered, _) = choose_parent_numbering(
                &mol,
                &NumberingModifiers::default(),
                &[path],
                &[],
                None,
                &substituents,
                &[],
                true,
                None,
                false,
                NumberingTopology::default(),
                None,
            );
            assert_eq!(numbered, expected, "{smiles}");
        }
    }

    #[test]
    fn retained_maps_assign_skeletal_anions_low_locants_before_suffixes() {
        let mol = read_smiles("[CH-]1CCC(=O)CC1").unwrap();
        let paths = [vec![3, 2, 1, 0, 6, 5], vec![0, 1, 2, 3, 5, 6]];
        let maps: Vec<BTreeMap<_, _>> = paths
            .iter()
            .map(|path| {
                path.iter()
                    .enumerate()
                    .map(|(i, &atom)| (atom, (i + 1).to_string()))
                    .collect()
            })
            .collect();
        let (numbered, map) = choose_parent_numbering(
            &mol,
            &NumberingModifiers::default(),
            &[],
            &[3],
            None,
            &BTreeMap::new(),
            &maps,
            true,
            None,
            false,
            NumberingTopology::default(),
            None,
        );
        assert_eq!(numbered, paths[1]);
        assert_eq!(map, Some(maps[1].clone()));
    }

    #[test]
    fn free_valence_precedes_skeletal_anions_in_both_numbering_paths() {
        for (smiles, root, rooted, anionic, is_ring, retained) in [
            ("CC[CH2-]", 0, vec![0, 1, 2], vec![2, 1, 0], false, None),
            (
                "Cc1c[c-]c[nH]1",
                1,
                vec![5, 1, 2, 3, 4],
                vec![5, 4, 3, 2, 1],
                true,
                Some("pyrrole"),
            ),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let paths = [anionic.clone(), rooted.clone()];
            let maps: Vec<BTreeMap<_, _>> = paths
                .iter()
                .map(|path| {
                    path.iter()
                        .enumerate()
                        .map(|(i, &atom)| (atom, (i + 1).to_string()))
                        .collect()
                })
                .collect();
            for locant_maps in [&[][..], maps.as_slice()] {
                for (free_valence, principal, expected) in
                    [(Some(root), vec![], &rooted), (None, vec![root], &anionic)]
                {
                    let (numbered, _) = choose_parent_numbering(
                        &mol,
                        &NumberingModifiers::default(),
                        &paths,
                        &principal,
                        free_valence,
                        &BTreeMap::new(),
                        locant_maps,
                        is_ring,
                        retained,
                        false,
                        NumberingTopology::default(),
                        None,
                    );
                    assert_eq!(&numbered, expected, "{smiles}: root {free_valence:?}");
                }
            }
        }
    }

    #[test]
    fn skeletal_anions_respect_fixed_heteroatom_numbering() {
        let mol = read_smiles("N1CCC[CH-]C1").unwrap();
        let preferred = vec![0, 5, 4, 3, 2, 1];
        let ordinary = number_parent(
            &mol,
            &NumberingModifiers::default(),
            &[vec![0, 1, 2, 3, 4, 5]],
            &[],
            None,
            &BTreeMap::new(),
            true,
            false,
            None,
            NumberingTopology::default(),
        );
        assert_eq!(ordinary, preferred);
        let maps: Vec<BTreeMap<_, _>> = [vec![4, 5, 0, 1, 2, 3], preferred.clone()]
            .iter()
            .map(|path| {
                path.iter()
                    .enumerate()
                    .map(|(i, &atom)| (atom, (i + 1).to_string()))
                    .collect()
            })
            .collect();
        let (numbered, map) = choose_parent_numbering(
            &mol,
            &NumberingModifiers::default(),
            &[],
            &[],
            None,
            &BTreeMap::new(),
            &maps,
            true,
            None,
            false,
            NumberingTopology::default(),
            None,
        );
        assert_eq!(numbered, preferred);
        assert_eq!(map, Some(maps[1].clone()));
    }

    #[test]
    fn hw_numbering_prioritizes_senior_heteroatom_then_all_hetero_locants() {
        let mol = read_smiles("s1cnnc1").unwrap();
        let rings = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        let numbered = number_parent(
            &mol,
            &NumberingModifiers::default(),
            &rings[0].paths,
            &[],
            None,
            &BTreeMap::new(),
            true,
            false,
            Some("1,3,4-thiadiazole"),
            NumberingTopology::default(),
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
        let numbered = number_parent(
            &mol,
            &NumberingModifiers::default(),
            &[path],
            &[],
            None,
            &subs,
            false,
            false,
            None,
            NumberingTopology::default(),
        );
        assert_eq!(numbered, (0..7).rev().collect::<Vec<_>>());
    }

    #[test]
    fn spiro_numbering_scores_only_the_topology_candidates() {
        let mol = read_smiles("C1CCC2(CC1)OCCO2").unwrap();
        let rings = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        let system = &rings[0];
        let (path, map) = choose_parent_numbering(
            &mol,
            &NumberingModifiers::default(),
            &system.paths,
            &[],
            None,
            &BTreeMap::new(),
            &[],
            true,
            None,
            false,
            NumberingTopology {
                is_spiro: true,
                ..NumberingTopology::default()
            },
            None,
        );
        assert!(system.paths.contains(&path));
        assert_eq!(
            heteroatom_locants_by_priority(&mol, &path, false),
            vec![vec![1, 4]]
        );
        assert_eq!(map.as_ref().map(BTreeMap::len), Some(10));
    }
}
