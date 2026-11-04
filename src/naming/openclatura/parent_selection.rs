//! Parent preference criteria, in the Python registry's comparison order.
use super::{
    assembly_prefixes::substituent_sort_key,
    chains::{RingSystem, canonical_ranks},
    locants::{LocantKey, retained_locant_sort_key},
    molecule::{AtomSet, Molecule},
    namer::name_subgraph_with_upstream,
    perception::PerceivedGroup,
    ring_assembly::{RingAssembly, discover_ring_assemblies},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ParentSelection {
    pub paths: Vec<Vec<usize>>,
    pub is_ring: bool,
    pub is_bicycle: bool,
    pub is_spiro: bool,
    pub is_polycycle: bool,
    pub xyz: (usize, usize, usize),
    pub polycycle_descriptor: Option<String>,
    pub ring_assembly: Option<RingAssembly>,
}

impl ParentSelection {
    pub fn primary_path(&self) -> &[usize] {
        self.paths.first().map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn atom_set(&self) -> AtomSet {
        self.primary_path().iter().copied().collect()
    }
    pub fn requires_fixed_substituent_start(&self) -> bool {
        self.is_bicycle || self.is_spiro || self.is_polycycle || self.ring_assembly.is_some()
    }
}

type RingSeniority = (Vec<usize>, i64, i64, i64, Vec<i64>, usize);
type ChainSeniority = (i64, i64, Vec<i64>);
type AssemblyCitation = (Vec<LocantKey>, Vec<((String, String), LocantKey)>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ParentScore {
    anionic_centres: i64,
    contains_principal: i64,
    principal_count: i64,
    senior_elements: Vec<usize>,
    ring_parent: i64,
    ring_seniority: Option<RingSeniority>,
    chain_seniority: Option<ChainSeniority>,
    assembly_component: Option<RingSeniority>,
    multiple_bonds: i64,
    double_bonds: i64,
    assembly_junctions: Option<(Vec<LocantKey>, Vec<LocantKey>)>,
    split_acyl: usize,
    attached_prefixes: i64,
    assembly_substituent_locants: Option<Vec<LocantKey>>,
    assembly_substituent_citation: Option<AssemblyCitation>,
    path_tiebreak: Vec<usize>,
}

struct ParentGroups<'a> {
    principal: &'a [usize],
    // Prefix parents must stay rooted at their attachment, not at an anion.
    anionic: Option<&'a [PerceivedGroup]>,
}

/// P-72.7(a): skeletal anions and anionic suffixes each contribute their
/// negatively charged atoms once, even when perceived groups overlap.
fn anionic_centre_count(mol: &Molecule, parent: &AtomSet, groups: &[PerceivedGroup]) -> usize {
    parent
        .iter()
        .copied()
        .chain(
            groups
                .iter()
                .filter(|g| {
                    g.is_principal_candidate
                        && parent.contains(&g.attachment_carbon)
                        // A charge-separated neutral suffix (e.g. nitrile oxide)
                        // is not an anionic suffix.
                        && g.atom_ids().iter().map(|i| mol.atoms[i].charge).sum::<i32>() < 0
                })
                .flat_map(|g| g.atoms_involved.iter().copied()),
        )
        .filter(|i| mol.atoms[i].charge < 0)
        .collect::<AtomSet>()
        .len()
}

fn score(
    mol: &Molecule,
    path: &[usize],
    is_ring: bool,
    ring_count: usize,
    ring_kind: Option<usize>,
    groups: &ParentGroups<'_>,
    ranks: &BTreeMap<usize, usize>,
) -> ParentScore {
    let atoms: AtomSet = path.iter().copied().collect();
    let principal_count = groups
        .principal
        .iter()
        .filter(|i| atoms.contains(i))
        .count() as i64;
    // P-44.2.1 (a)-(c): nitrogen precedes every other ring heteroatom.
    let hetero_seniority: &[&str] = if ring_kind.is_some() {
        &[
            "N", "F", "Cl", "Br", "I", "O", "S", "Se", "Te", "P", "As", "Sb", "Bi", "Si", "Ge",
            "Sn", "Pb", "B", "Al", "Ga", "In", "Tl",
        ]
    } else {
        &["N", "O", "S", "P", "Si", "B"]
    };
    let symbols: Vec<&str> = path.iter().map(|i| mol.atoms[i].symbol.as_str()).collect();
    let senior_elements = if ring_kind.is_some() {
        Vec::new()
    } else {
        let elements = ["N", "P", "Si", "B", "O", "S", "C"];
        symbols
            .iter()
            .filter_map(|s| elements.iter().position(|e| e == s).map(|i| i + 1))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    let senior_hetero = symbols
        .iter()
        .filter_map(|s| hetero_seniority.iter().position(|e| e == s).map(|i| i + 1))
        .min()
        .into_iter()
        .collect::<Vec<_>>();
    let senior_hetero = if ring_kind.is_some() && senior_hetero.is_empty() {
        vec![usize::MAX]
    } else {
        senior_hetero
    };
    let hetero_count = symbols.iter().filter(|&&s| s != "C").count() as i64;
    let hetero_order: &[&str] = if ring_kind.is_some() {
        &[
            "F", "Cl", "Br", "I", "O", "S", "Se", "Te", "N", "P", "As", "Sb", "Bi", "Si", "Ge",
            "Sn", "Pb", "B", "Al", "Ga", "In", "Tl",
        ]
    } else {
        &["O", "S", "N", "P", "Si", "B"]
    };
    let hetero_counts = hetero_order
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
        anionic_centres: -groups
            .anionic
            .map_or(0, |g| anionic_centre_count(mol, &atoms, g) as i64),
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
                ring_kind.unwrap_or(0),
            )
        }),
        chain_seniority: (!is_ring).then(|| (-(path.len() as i64), -hetero_count, hetero_counts)),
        assembly_component: None,
        assembly_junctions: None,
        multiple_bonds: -multiple,
        double_bonds: -double,
        split_acyl,
        attached_prefixes: -attached,
        assembly_substituent_locants: None,
        assembly_substituent_citation: None,
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
    groups: &[PerceivedGroup],
) -> Option<ParentSelection> {
    parent_candidates(
        mol,
        chains,
        ring_systems,
        principal_carbons,
        Some(groups),
        &AtomSet::new(),
        true,
    )
    .into_iter()
    .min_by(|a, b| a.0.cmp(&b.0))
    .map(|(_, selection)| selection)
}

pub fn select_principal_parent_with_excluded(
    mol: &Molecule,
    chains: &[Vec<usize>],
    ring_systems: &[RingSystem],
    principal_carbons: &[usize],
    excluded: &AtomSet,
) -> Option<ParentSelection> {
    // `min_by_key` keeps the first tied item, as does Python's stable sort.
    parent_candidates(
        mol,
        chains,
        ring_systems,
        principal_carbons,
        None,
        excluded,
        true,
    )
    .into_iter()
    .min_by(|a, b| a.0.cmp(&b.0))
    .map(|(_, selection)| selection)
}

/// P-15.3 / P-51.3.1: multiplicative units must tie on parent seniority,
/// before substituent counts, locants and citation order select one parent.
pub(super) fn senior_parent_candidates(
    mol: &Molecule,
    chains: &[Vec<usize>],
    ring_systems: &[RingSystem],
    principal_carbons: &[usize],
    groups: &[PerceivedGroup],
    excluded: &AtomSet,
) -> Vec<ParentSelection> {
    let mut candidates = parent_candidates(
        mol,
        chains,
        ring_systems,
        principal_carbons,
        Some(groups),
        excluded,
        false,
    );
    for (score, _) in &mut candidates {
        score.attached_prefixes = 0;
        score.assembly_substituent_locants = None;
        score.assembly_substituent_citation = None;
    }
    let best = candidates.iter().map(|(score, _)| score).min().cloned();
    candidates
        .into_iter()
        .filter_map(|(score, selection)| (Some(&score) == best.as_ref()).then_some(selection))
        .collect()
}

fn parent_candidates(
    mol: &Molecule,
    chains: &[Vec<usize>],
    ring_systems: &[RingSystem],
    principal_carbons: &[usize],
    groups: Option<&[PerceivedGroup]>,
    excluded: &AtomSet,
    atom_order_tiebreak: bool,
) -> Vec<(ParentScore, ParentSelection)> {
    let groups = ParentGroups {
        principal: principal_carbons,
        anionic: groups,
    };
    let ranks = canonical_ranks(mol, None);
    let assemblies = discover_ring_assemblies(mol, ring_systems);
    let compare_assembly = !assemblies.is_empty();
    let mut candidates: Vec<(ParentScore, ParentSelection)> = chains
        .iter()
        .filter(|p| !p.is_empty())
        .map(|path| {
            (
                score(
                    mol,
                    path,
                    false,
                    0,
                    compare_assembly.then_some(0),
                    &groups,
                    &ranks,
                ),
                ParentSelection {
                    paths: vec![path.clone()],
                    is_ring: false,
                    is_bicycle: false,
                    is_spiro: false,
                    is_polycycle: false,
                    xyz: (0, 0, 0),
                    polycycle_descriptor: None,
                    ring_assembly: None,
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
                        ring_count_for_system(mol, rs),
                        compare_assembly.then(|| ring_kind(rs)),
                        &groups,
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
                        ring_assembly: None,
                    },
                )
            }),
    );
    candidates.extend(assemblies.into_iter().map(|assembly| {
        let path: Vec<_> = assembly.locant_maps[0].keys().copied().collect();
        let mut parent_score = score(
            mol,
            &path,
            true,
            assembly.ring_count,
            Some(6),
            &groups,
            &ranks,
        );
        // P-44.2.2.2.7(a)-(g): after whole-assembly seniority ties, compare
        // one component: heteroatom presence, N, first heteroatom, rings,
        // atoms, heteroatom count, then counts in heteroatom seniority order.
        // Ring kind has already separated assemblies from other ring parents.
        let unit_path: Vec<_> = assembly.unit_atoms.iter().copied().collect();
        let unit_bonds = mol
            .bonds
            .values()
            .filter(|b| assembly.unit_atoms.contains(&b.u) && assembly.unit_atoms.contains(&b.v))
            .count();
        parent_score.assembly_component = score(
            mol,
            &unit_path,
            true,
            unit_bonds + 1 - unit_path.len(),
            Some(6),
            &ParentGroups {
                principal: &[],
                anionic: None,
            },
            &ranks,
        )
        .ring_seniority;
        parent_score.assembly_junctions = Some(assembly.junction_key.clone());
        let atoms: AtomSet = path.iter().copied().collect();
        parent_score.attached_prefixes = -(path
            .iter()
            .flat_map(|&atom| mol.get_neighbors(atom))
            .filter(|n| !atoms.contains(n) && !excluded.contains(n))
            .count() as i64);
        // P-44.1 precedes P-28.6: after principal-group coverage and ring
        // seniority, minimize junctions, then prefer more substituents and
        // their lowest locants.
        parent_score.assembly_substituent_locants = assembly
            .locant_maps
            .iter()
            .map(|map| {
                let mut locants: Vec<_> = path
                    .iter()
                    .flat_map(|&atom| {
                        mol.get_neighbors(atom)
                            .iter()
                            .filter(|n| !atoms.contains(n) && !excluded.contains(n))
                            .map(move |_| retained_locant_sort_key(&map[&atom]))
                    })
                    .collect();
                locants.sort();
                locants
            })
            .min();
        (
            parent_score,
            ParentSelection {
                paths: vec![path],
                is_ring: true,
                is_bicycle: false,
                is_spiro: false,
                is_polycycle: false,
                xyz: (0, 0, 0),
                polycycle_descriptor: None,
                ring_assembly: Some(assembly),
            },
        )
    }));
    if compare_assembly {
        prefer_ring_or_chain_class(&mut candidates, mol);
    }
    if !atom_order_tiebreak {
        for (score, _) in &mut candidates {
            score.path_tiebreak.clear();
        }
    }
    prefer_spiro_backbone_components(&mut candidates, ring_systems, mol);
    prefer_assembly_substituent_citation(&mut candidates, mol, excluded);
    candidates
}

fn prefer_assembly_substituent_citation(
    candidates: &mut [(ParentScore, ParentSelection)],
    mol: &Molecule,
    excluded: &AtomSet,
) {
    let Some((best, _)) = candidates.iter().min_by(|a, b| a.0.cmp(&b.0)) else {
        return;
    };
    let mut best = best.clone();
    best.path_tiebreak.clear();
    let tied: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, (score, selection))| {
            let mut score = score.clone();
            score.path_tiebreak.clear();
            (score == best && selection.ring_assembly.is_some()).then_some(index)
        })
        .collect();
    if tied.len() < 2 {
        return;
    }
    // P-28.6: only after attachment and collective substituent locants tie,
    // compare the locants of prefixes in alphanumerical citation order.
    let citations = tied
        .iter()
        .map(|&index| {
            let selection = &candidates[index].1;
            let atoms = selection.atom_set();
            let mut boundary = excluded.clone();
            boundary.extend(&atoms);
            let mut prefixes = Vec::new();
            for &atom in selection.primary_path() {
                for &neighbor in mol.get_neighbors(atom) {
                    if boundary.contains(&neighbor) {
                        continue;
                    }
                    let mut name = None;
                    // Comparing a discarded candidate must not mark the final name.
                    let _ = mol.candidate::<()>(|| {
                        name = Some(name_subgraph_with_upstream(
                            mol,
                            neighbor,
                            &boundary,
                            Some(atom),
                        )?);
                        Ok(None)
                    });
                    prefixes.push((atom, substituent_sort_key(&name?)));
                }
            }
            selection
                .ring_assembly
                .as_ref()?
                .locant_maps
                .iter()
                .filter(|map| {
                    let mut locants: Vec<_> = prefixes
                        .iter()
                        .map(|(atom, _)| retained_locant_sort_key(&map[atom]))
                        .collect();
                    locants.sort();
                    Some(&locants) == candidates[index].0.assembly_substituent_locants.as_ref()
                })
                .map(|map| {
                    let mut citation: Vec<_> = prefixes
                        .iter()
                        .map(|(atom, name)| (name.clone(), retained_locant_sort_key(&map[atom])))
                        .collect();
                    citation.sort();
                    (
                        citation.iter().map(|(_, locant)| locant.clone()).collect(),
                        citation,
                    )
                })
                .min()
        })
        .collect::<Option<Vec<_>>>();
    if let Some(citations) = citations {
        for (index, citation) in tied.into_iter().zip(citations) {
            candidates[index].0.assembly_substituent_citation = Some(citation);
        }
    }
}

fn ring_kind(system: &RingSystem) -> usize {
    // P-44.2.2.2: spiro, cyclic phane, fused, bridged fused,
    // nonfused bridged, linear phane, assembly.
    if system.is_spiro
        || system
            .polycycle_descriptor
            .as_deref()
            .is_some_and(|d| d.contains("spiro"))
    {
        0
    } else if system.is_bicycle && system.z > 0
        || system.is_polycycle
            && super::ring_renderer::is_von_baeyer_descriptor(
                system.polycycle_descriptor.as_deref(),
            )
    {
        4
    } else {
        2
    }
}

fn prefer_ring_or_chain_class(
    candidates: &mut Vec<(ParentScore, ParentSelection)>,
    mol: &Molecule,
) {
    let Some(principal) = candidates
        .iter()
        .map(|(s, _)| (s.anionic_centres, s.contains_principal, s.principal_count))
        .min()
    else {
        return;
    };
    // P-44.1.2 applies between rings and chains, never between two rings.
    let elements = [
        "N", "P", "As", "Sb", "Bi", "Si", "Ge", "Sn", "Pb", "B", "Al", "Ga", "In", "Tl", "O", "S",
        "Se", "Te", "C",
    ];
    let senior = |selection: &ParentSelection| {
        selection
            .primary_path()
            .iter()
            .filter_map(|atom| elements.iter().position(|&s| s == mol.atoms[atom].symbol))
            .min()
            .unwrap_or(usize::MAX)
    };
    let ring = candidates
        .iter()
        .filter(|(s, p)| {
            p.is_ring && (s.anionic_centres, s.contains_principal, s.principal_count) == principal
        })
        .map(|(_, p)| senior(p))
        .min();
    let chain = candidates
        .iter()
        .filter(|(s, p)| {
            !p.is_ring && (s.anionic_centres, s.contains_principal, s.principal_count) == principal
        })
        .map(|(_, p)| senior(p))
        .min();
    if let (Some(ring), Some(chain)) = (ring, chain) {
        candidates.retain(|(s, p)| {
            (s.anionic_centres, s.contains_principal, s.principal_count) != principal
                || if ring <= chain { p.is_ring } else { !p.is_ring }
        });
    }
}

fn ring_count_for_system(mol: &Molecule, system: &RingSystem) -> usize {
    if system.is_polycycle {
        let edges = system
            .atoms
            .iter()
            .map(|&atom| {
                mol.get_neighbors(atom)
                    .iter()
                    .filter(|&&other| atom < other && system.atoms.contains(&other))
                    .count()
            })
            .sum::<usize>();
        (edges + 1).saturating_sub(system.atoms.len()).max(3)
    } else if system.is_bicycle || system.is_spiro {
        2
    } else {
        1
    }
}

/// Spiro-connected components compete for the shared backbone before their
/// element seniority is compared. Principal-group coverage still comes first.
fn prefer_spiro_backbone_components(
    candidates: &mut Vec<(ParentScore, ParentSelection)>,
    systems: &[RingSystem],
    mol: &Molecule,
) {
    if systems.len() < 2 {
        return;
    }
    let shared: BTreeMap<Vec<usize>, (usize, usize)> = systems
        .iter()
        .enumerate()
        .flat_map(|(index, system)| {
            let count = systems
                .iter()
                .enumerate()
                .filter(|(other_index, other)| {
                    *other_index != index && !system.atoms.is_disjoint(&other.atoms)
                })
                .count();
            let ring_count = ring_count_for_system(mol, system);
            system
                .paths
                .iter()
                .map(move |path| (path.clone(), (count, ring_count)))
        })
        .collect();
    let Some(best_principal) = candidates
        .iter()
        .map(|(score, _)| {
            (
                score.anionic_centres,
                score.contains_principal,
                score.principal_count,
            )
        })
        .min()
    else {
        return;
    };
    type BackboneKey = (i64, i64, i64, Vec<usize>);
    let keys: BTreeMap<Vec<usize>, BackboneKey> = candidates
        .iter()
        .filter_map(|(score, selection)| {
            if !selection.is_ring
                || (
                    score.anionic_centres,
                    score.contains_principal,
                    score.principal_count,
                ) != best_principal
            {
                return None;
            }
            let path = selection.primary_path();
            let &(count, ring_count) = shared.get(path)?;
            (count > 0).then(|| {
                (
                    path.to_vec(),
                    (
                        -(count as i64),
                        -(ring_count as i64),
                        -(path.len() as i64),
                        score.path_tiebreak.clone(),
                    ),
                )
            })
        })
        .collect();
    if keys.len() < 2 {
        return;
    }
    let Some(best_key) = keys.values().min() else {
        return;
    };
    candidates.retain(|(_, selection)| {
        keys.get(selection.primary_path())
            .is_none_or(|key| key == best_key)
    });
}

#[cfg(test)]
mod tests {
    use super::super::{
        chains::{find_all_carbon_paths, find_ring_systems},
        graph_io::read_smiles,
    };
    use super::*;
    #[test]
    fn anionic_centres_precede_neutral_principal_groups() {
        for smiles in [
            "Oc1ccccc1-c1[n-]ccc1C",
            "OC(=O)c1ccccc1-c1[n-]ccc1",
            "Oc1ccc(cc1)[CH2-]",
            "OCCc1cc[n-]c1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let excluded = AtomSet::new();
            let groups = super::super::perception::perceive_groups(&mol);
            let principal: Vec<_> = groups.iter().map(|g| g.attachment_carbon).collect();
            let parent = select_principal_parent(
                &mol,
                &find_all_carbon_paths(&mol, &excluded).unwrap(),
                &find_ring_systems(&mol, &excluded).unwrap(),
                &principal,
                &groups,
            )
            .unwrap();
            assert_eq!(
                anionic_centre_count(&mol, &parent.atom_set(), &groups),
                1,
                "{smiles}"
            );
            assert!(
                parent.atom_set().iter().any(|i| mol.atoms[i].charge < 0),
                "{smiles}"
            );
        }
    }

    #[test]
    fn two_anionic_suffix_centres_precede_one_skeletal_anion() {
        let mol = read_smiles("[O-]CC([O-])c1cc[n-]c1").unwrap();
        let excluded = AtomSet::new();
        let groups = super::super::perception::perceive_groups(&mol);
        let parent = select_principal_parent(
            &mol,
            &find_all_carbon_paths(&mol, &excluded).unwrap(),
            &find_ring_systems(&mol, &excluded).unwrap(),
            &[],
            &groups,
        )
        .unwrap();
        assert!(!parent.is_ring);
        assert_eq!(anionic_centre_count(&mol, &parent.atom_set(), &groups), 2);
    }

    #[test]
    fn perceived_anionic_suffix_atoms_are_counted_once() {
        for smiles in [
            "C[O-]",
            "C[S-]",
            "C[NH-]",
            "CC(=O)[O-]",
            "c1ccccc1C(=O)[O-]",
            "CS(=O)(=O)[O-]",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let mut groups = super::super::perception::perceive_groups(&mol);
            let parent = mol
                .atoms
                .values()
                .filter(|a| a.symbol == "C")
                .map(|a| a.idx)
                .collect();
            groups.extend(groups.clone());
            assert_eq!(anionic_centre_count(&mol, &parent, &groups), 1, "{smiles}");
            let all_atoms = mol.atoms.keys().copied().collect();
            assert_eq!(
                anionic_centre_count(&mol, &all_atoms, &groups),
                1,
                "{smiles}"
            );
        }
    }

    #[test]
    fn neutral_nitrile_oxide_does_not_add_an_anionic_suffix_centre() {
        let mol = read_smiles("CC#[N+][O-]").unwrap();
        let groups = super::super::perception::perceive_groups(&mol);
        assert!(groups.iter().any(|g| {
            g.is_principal_candidate
                && g.descriptor
                    .as_ref()
                    .is_some_and(|d| d.derivative == "zwitterion")
        }));
        let parent = mol
            .atoms
            .values()
            .filter(|a| a.is_carbon())
            .map(|a| a.idx)
            .collect();
        assert_eq!(anionic_centre_count(&mol, &parent, &groups), 0);
    }

    #[test]
    fn skeletal_anion_precedes_a_neutral_nitrile_oxide_parent() {
        let mol = read_smiles("[O-][N+]#CCCc1cc[n-]c1").unwrap();
        let excluded = AtomSet::new();
        let groups = super::super::perception::perceive_groups(&mol);
        let key = super::super::principal_groups::component_principal_key(&mol, &groups, false);
        let (principal, _) = super::super::principal_groups::partition_principal_and_prefix_groups(
            &groups,
            key.as_deref(),
        );
        let parent = select_principal_parent(
            &mol,
            &find_all_carbon_paths(&mol, &excluded).unwrap(),
            &find_ring_systems(&mol, &excluded).unwrap(),
            &principal,
            &groups,
        )
        .unwrap();
        assert!(parent.is_ring);
        assert!(parent.atom_set().iter().any(|i| mol.atoms[i].charge < 0));
        assert_eq!(anionic_centre_count(&mol, &parent.atom_set(), &groups), 1);
    }

    #[test]
    fn ring_precedes_a_longer_chain_without_principal_groups() {
        let mol = read_smiles("CCCCCCCC1CCCCC1").unwrap();
        let excluded = AtomSet::new();
        let parent = select_principal_parent(
            &mol,
            &find_all_carbon_paths(&mol, &excluded).unwrap(),
            &find_ring_systems(&mol, &excluded).unwrap(),
            &[],
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
            &find_ring_systems(&mol, &excluded).unwrap(),
            &[1],
            &[],
        )
        .unwrap();
        assert!(!parent.is_ring);
        assert!(parent.atom_set().contains(&1));
    }

    #[test]
    fn principal_group_coverage_precedes_branched_assembly_junctions() {
        let mol = read_smiles("c1(-c2ccccc2)c(-c2ccccc2)cc(-c2ccc(O)cc2)cc1").unwrap();
        let excluded = AtomSet::new();
        let oxygen = mol.atoms.values().find(|a| a.symbol == "O").unwrap().idx;
        let principal = mol.get_neighbors(oxygen)[0];
        let parent = select_principal_parent(
            &mol,
            &find_all_carbon_paths(&mol, &excluded).unwrap(),
            &find_ring_systems(&mol, &excluded).unwrap(),
            &[principal],
            &[],
        )
        .unwrap();
        assert!(parent.atom_set().contains(&principal));
        assert_eq!(
            parent.ring_assembly.unwrap().name,
            "1^1,2^1:2^3,3^1-terphenyl"
        );
    }

    #[test]
    fn branched_assemblies_use_alphanumerical_prefix_citation_to_break_ties() {
        for smiles in [
            "c1(-c2ccc(Br)cc2)cc(-c2ccc(Cl)cc2)cc(-c2ccc(F)cc2)c1",
            "c1(-c2ccc(F)cc2)cc(-c2ccc(Cl)cc2)cc(-c2ccc(Br)cc2)c1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let excluded = AtomSet::new();
            let parent = select_principal_parent(
                &mol,
                &[],
                &find_ring_systems(&mol, &excluded).unwrap(),
                &[],
                &[],
            )
            .unwrap();
            let atoms = parent.atom_set();
            assert_eq!(
                parent.ring_assembly.unwrap().name,
                "1^1,2^1:2^3,3^1-terphenyl"
            );
            for (symbol, in_parent) in [("Br", true), ("Cl", false), ("F", true)] {
                let halogen = mol.atoms.values().find(|a| a.symbol == symbol).unwrap();
                assert_eq!(
                    atoms.contains(&mol.get_neighbors(halogen.idx)[0]),
                    in_parent,
                    "{smiles}: {symbol}"
                );
            }
        }
    }

    #[test]
    fn fused_parent_precedes_an_assembly_after_equal_ring_and_atom_totals() {
        let mol = read_smiles("c1ccc2C=CC=CC=Cc2c1-c1ccccc1-c1ccccc1").unwrap();
        let excluded = AtomSet::new();
        let systems = find_ring_systems(&mol, &excluded).unwrap();
        let assembly = discover_ring_assemblies(&mol, &systems)
            .into_iter()
            .max_by_key(|a| a.ring_count)
            .unwrap();
        assert_eq!(assembly.ring_count, 2);
        assert_eq!(assembly.locant_maps[0].len(), 12);
        let parent = select_principal_parent(&mol, &[], &systems, &[], &[]).unwrap();
        assert!(parent.ring_assembly.is_none());
        assert_eq!(parent.primary_path().len(), 12);
    }

    #[test]
    fn assembly_seniority_uses_all_units_for_ring_and_atom_totals() {
        for (smiles, rings, atoms) in [
            (
                "c1ccc2ccccc2c1-c1c(Cc3ccccc3-c3ccccc3-c3ccccc3)ccc2ccccc12",
                4,
                20,
            ),
            ("C1CCCCCC1C1CCCCCC1Cc1ccccc1-c1ccccc1", 2, 14),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let excluded = AtomSet::new();
            let parent = select_principal_parent(
                &mol,
                &[],
                &find_ring_systems(&mol, &excluded).unwrap(),
                &[],
                &[],
            )
            .unwrap();
            assert_eq!(parent.primary_path().len(), atoms, "{smiles}");
            assert_eq!(parent.ring_assembly.unwrap().ring_count, rings, "{smiles}");
        }
    }
}
