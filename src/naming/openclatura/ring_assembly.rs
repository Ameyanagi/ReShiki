//! Whole cyclic-system assemblies and their fixed component numbering (P-28).
use std::collections::BTreeMap;

use super::{
    chains::{RingSystem, canonical_ranks},
    graph_kernel::{biconnected_edge_components, connected_components},
    locants::{LocantKey, retained_locant_sort_key},
    molecule::{AtomSet, Molecule},
    rules::{retained, stems},
};

#[derive(Clone, Debug)]
pub struct RingAssembly {
    pub name: String,
    pub unit_name: String,
    pub locant_maps: Vec<BTreeMap<usize, String>>,
    pub indicated_hydrogen_atoms: AtomSet,
    pub junction_bond_ids: AtomSet,
    pub ring_count: usize,
    pub(super) unit_atoms: AtomSet,
    pub(super) junction_key: (Vec<LocantKey>, Vec<LocantKey>),
}

struct Unit {
    name: String,
    maps: Vec<BTreeMap<usize, String>>,
    indicated: AtomSet,
    ring_count: usize,
}

/// A unit includes every cyclic atom connected by cyclic edges, including spiro
/// connections. A bridge between two units is never part of either unit.
pub(super) fn cyclic_units(mol: &Molecule) -> Vec<AtomSet> {
    let edges: super::graph_kernel::EdgeSet =
        biconnected_edge_components(mol, &mol.atoms.keys().copied().collect())
            .into_iter()
            .filter(|block| block.len() > 1)
            .flatten()
            .collect();
    let atoms = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
    connected_components(&atoms, &edges)
}

fn hydride(mol: &Molecule, atoms: &AtomSet) -> Molecule {
    let mut unit = mol.subgraph(atoms);
    for (&idx, atom) in &mut unit.atoms {
        // P-28.1: substituents are replaced by hydrogen, including =O by 2 H.
        atom.total_h_count += mol
            .get_neighbors(idx)
            .iter()
            .filter(|n| !atoms.contains(n))
            .map(|&n| mol.bond_order(idx, n))
            .sum::<usize>();
        atom.explicit_h_count = atom.total_h_count;
        atom.isotope = None;
        atom.stereo = None;
        atom.raw_stereo = None;
    }
    unit
}

/// Visit exact hydride graph isomorphisms until the visitor accepts one.
/// Aromatic bonds are compared independently of their Kekule localization.
fn visit_hydride_isomorphisms(
    left: &Molecule,
    right: &Molecule,
    mut visit: impl FnMut(&BTreeMap<usize, usize>) -> bool,
) -> bool {
    if left.atoms.len() != right.atoms.len() || left.bonds.len() != right.bonds.len() {
        return false;
    }
    let left_ranks = canonical_ranks(left, None);
    let right_ranks = canonical_ranks(right, None);
    let atom_key = |mol: &Molecule, idx: usize| {
        let atom = &mol.atoms[&idx];
        (
            atom.symbol.clone(),
            atom.charge,
            atom.is_aromatic,
            atom.total_h_count,
            mol.get_neighbors(idx).len(),
        )
    };
    let mut left_key: Vec<_> = left
        .atoms
        .keys()
        .map(|&i| (left_ranks[&i], atom_key(left, i)))
        .collect();
    let mut right_key: Vec<_> = right
        .atoms
        .keys()
        .map(|&i| (right_ranks[&i], atom_key(right, i)))
        .collect();
    left_key.sort();
    right_key.sort();
    if left_key != right_key {
        return false;
    }
    let edge = |mol: &Molecule, u, v| {
        mol.get_bond(u, v)
            .map(|bond| if bond.is_aromatic { 4 } else { bond.order })
    };
    fn matches(
        left: &Molecule,
        right: &Molecule,
        candidates: &BTreeMap<usize, Vec<usize>>,
        edge: &impl Fn(&Molecule, usize, usize) -> Option<usize>,
        mapping: &mut BTreeMap<usize, usize>,
        visit: &mut impl FnMut(&BTreeMap<usize, usize>) -> bool,
    ) -> bool {
        let Some((&u, options)) = candidates
            .iter()
            .filter(|(u, _)| !mapping.contains_key(u))
            .min_by_key(|(_, options)| {
                options
                    .iter()
                    .filter(|v| !mapping.values().any(|x| x == *v))
                    .count()
            })
        else {
            return visit(mapping);
        };
        for &v in options {
            if mapping.values().any(|&x| x == v)
                || mapping
                    .iter()
                    .any(|(&a, &b)| edge(left, u, a) != edge(right, v, b))
            {
                continue;
            }
            mapping.insert(u, v);
            if matches(left, right, candidates, edge, mapping, visit) {
                return true;
            }
            mapping.remove(&u);
        }
        false
    }
    let candidates = left
        .atoms
        .keys()
        .map(|&u| {
            (
                u,
                right
                    .atoms
                    .keys()
                    .copied()
                    .filter(|&v| {
                        left_ranks[&u] == right_ranks[&v] && atom_key(left, u) == atom_key(right, v)
                    })
                    .collect(),
            )
        })
        .collect();
    matches(
        left,
        right,
        &candidates,
        &edge,
        &mut BTreeMap::new(),
        &mut visit,
    )
}

fn same_hydride(left: &Molecule, right: &Molecule) -> bool {
    visit_hydride_isomorphisms(left, right, |_| true)
}

fn identical_junctions(mol: &Molecule, atoms: &[AtomSet]) -> Vec<(usize, usize, usize)> {
    let units: Vec<_> = atoms.iter().map(|a| hydride(mol, a)).collect();
    let owner: BTreeMap<_, _> = atoms
        .iter()
        .enumerate()
        .flat_map(|(i, a)| a.iter().map(move |&atom| (atom, i)))
        .collect();
    let mut identical = BTreeMap::new();
    mol.bonds
        .values()
        .filter_map(|bond| {
            let (&a, &b) = (owner.get(&bond.u)?, owner.get(&bond.v)?);
            if a == b {
                return None;
            }
            let pair = (a.min(b), a.max(b));
            let same = *identical
                .entry(pair)
                .or_insert_with(|| same_hydride(&units[a], &units[b]));
            same.then_some((a, b, bond.idx))
        })
        .collect()
}

/// Includes unsupported assemblies so successful renderings can account for
/// exactly the junctions for which a preferred assembly name was produced.
pub(super) fn assembly_junctions(mol: &Molecule) -> AtomSet {
    let atoms = cyclic_units(mol);
    let mut junctions: AtomSet = identical_junctions(mol, &atoms)
        .into_iter()
        .map(|(_, _, bond)| bond)
        .collect();
    let owner: BTreeMap<_, _> = atoms
        .iter()
        .enumerate()
        .flat_map(|(index, atoms)| atoms.iter().map(move |&atom| (atom, index)))
        .collect();
    let mut indicated = BTreeMap::new();
    for bond in mol.bonds.values() {
        if junctions.contains(&bond.idx) {
            continue;
        }
        let (Some(&left), Some(&right)) = (owner.get(&bond.u), owner.get(&bond.v)) else {
            continue;
        };
        if left == right {
            continue;
        }
        // P-28.2.3: relocating indicated H does not change the assembly's
        // parent hydride. Such pairs still need the unsupported-name status.
        let left = indicated
            .entry(left)
            .or_insert_with(|| indicated_hydride_key(&hydride(mol, &atoms[left])))
            .clone();
        let right = indicated
            .entry(right)
            .or_insert_with(|| indicated_hydride_key(&hydride(mol, &atoms[right])));
        if left
            .as_ref()
            .is_some_and(|left| Some(left) == right.as_ref())
        {
            junctions.insert(bond.idx);
        }
    }
    junctions
}

fn indicated_hydride_key(unit: &Molecule) -> Option<(String, usize, usize)> {
    let systems = super::chains::find_ring_systems(unit, &AtomSet::new()).ok()?;
    let path = systems.first()?.paths.first()?;
    let name = retained::get_retained_ring(unit, path)
        .filter(|found| {
            retained::cites_indicated_hydrogen(unit, path, Some(&found.name))
                || !super::additive::name_indicated_hydrogen_locants(Some(&found.name)).is_empty()
                || retained::parent_metadata(&found.name)
                    .is_some_and(|metadata| metadata.indicated_hydrogen_count > 0)
        })
        .map(|found| found.name)
        .or_else(|| {
            use super::retained_fused_templates::{
                MatchOptions, match_retained_graph_templates_with_options,
            };
            match_retained_graph_templates_with_options(
                unit,
                &unit.atoms.keys().copied().collect(),
                MatchOptions {
                    allow_nonaromatic: true,
                    allow_relocated_indicated_h: true,
                    ..Default::default()
                },
            )
            .into_iter()
            .find(|found| found.template.indicated_hydrogen_count() > 0)
            .map(|found| found.template.name)
        })?;
    let stem = name
        .split_once('-')
        .filter(|(prefix, _)| prefix.ends_with('H'))
        .map_or(name.as_str(), |(_, stem)| stem);
    Some((
        stem.into(),
        unit.atoms.values().map(|atom| atom.total_h_count).sum(),
        unit.bonds.values().filter(|bond| bond.order == 2).count(),
    ))
}

fn resolve_unit(mol: &Molecule, atoms: &AtomSet, system: &RingSystem) -> Option<Unit> {
    let unit = hydride(mol, atoms);
    if unit
        .atoms
        .values()
        .any(|a| a.charge != 0 || a.radical_electrons != 0)
    {
        return None;
    }
    let path = system.paths.first()?;
    let ring_count = unit.bonds.len() + 1 - unit.atoms.len();
    let found = retained::get_retained_ring(&unit, path);
    let (name, mut maps) = if let Some(found) = found {
        if retained::mancude_monocycle_hydro_plan(&unit, path, Some(&found.name)).is_some()
            || found.name.contains("hydro")
            || super::retained_name_policy::retained_parent_name_policy(&found.name)
                .is_some_and(|policy| policy.hydrogenation.is_some())
        {
            return None;
        }
        let metadata = retained::parent_metadata(&found.name);
        if metadata.as_ref().is_some_and(|metadata| {
            metadata.mancude_double_bonds > unit.bonds.values().filter(|b| b.order == 2).count()
        }) {
            return None;
        }
        (found.name, found.locant_maps)
    } else {
        if unit.atoms.values().any(|a| a.symbol != "C") || unit.bonds.values().any(|b| b.order != 1)
        {
            return None;
        }
        let stem = stems::stem_for(atoms.len());
        let name = if system.is_bicycle {
            format!("bicyclo[{}.{}.{}]{stem}ane", system.x, system.y, system.z)
        } else if system.is_spiro {
            format!("spiro[{}.{}]{stem}ane", system.x, system.y)
        } else if system.is_polycycle {
            format!("{}{stem}ane", system.polycycle_descriptor.as_deref()?)
        } else {
            format!("cyclo{stem}ane")
        };
        (name, Vec::new())
    };
    if maps.is_empty() {
        let paths = if system.is_bicycle || system.is_spiro || system.is_polycycle {
            system.paths.clone()
        } else if path.iter().any(|i| unit.atoms[i].symbol != "C") {
            retained::canonical_ring_orders(&unit, path)
        } else {
            (0..path.len())
                .flat_map(|offset| {
                    let forward: Vec<_> = path[offset..]
                        .iter()
                        .chain(&path[..offset])
                        .copied()
                        .collect();
                    [forward.clone(), forward.into_iter().rev().collect()]
                })
                .collect()
        };
        maps = paths
            .into_iter()
            .map(|p| {
                p.into_iter()
                    .enumerate()
                    .map(|(i, a)| (a, (i + 1).to_string()))
                    .collect()
            })
            .collect();
    }
    if maps.is_empty()
        || maps
            .iter()
            .any(|map| map.keys().copied().collect::<AtomSet>() != *atoms)
    {
        return None;
    }
    // P-28.2.1 requires the lowest junction locants within the traditional
    // parent numbering. Compose each valid numbering with every hydride graph
    // automorphism: this preserves that numbering (including P-24.2.2 spiro
    // descriptors), while recovering equivalent orientations omitted by a
    // topology planner's representative paths.
    let mut expanded = std::collections::BTreeSet::new();
    visit_hydride_isomorphisms(&unit, &unit, |automorphism| {
        for map in &maps {
            expanded.insert(
                map.iter()
                    .map(|(&atom, locant)| (automorphism[&atom], locant.clone()))
                    .collect(),
            );
        }
        false
    });
    maps = expanded.into_iter().collect();
    let mut indicated =
        super::numbering::indicated_hydrogen_like_atoms(&unit, path, Some(&name), false);
    if indicated.is_empty() {
        let mut hydrogen_locants = super::additive::name_indicated_hydrogen_locants(Some(&name));
        if let Some(metadata) = retained::parent_metadata(&name) {
            hydrogen_locants.extend(metadata.default_indicated_h);
        }
        indicated.extend(
            maps[0]
                .iter()
                .filter_map(|(&atom, locant)| hydrogen_locants.contains(locant).then_some(atom)),
        );
    }
    // The component name's indicated-H run belongs in front of the assembly.
    let name = name
        .split_once('-')
        .filter(|(prefix, _)| prefix.ends_with('H'))
        .map_or(name.clone(), |(_, stem)| stem.to_owned());
    Some(Unit {
        name,
        maps,
        indicated,
        ring_count,
    })
}

fn decorated(locant: &str, unit: usize, count: usize) -> String {
    if count == 2 {
        let digits = locant.bytes().take_while(u8::is_ascii_digit).count();
        format!(
            "{}{}{}",
            &locant[..digits],
            if unit == 0 { "" } else { "'" },
            &locant[digits..]
        )
    } else {
        format!("{}^{locant}", unit + 1)
    }
}

fn path_assembly(
    mol: &Molecule,
    path: &[usize],
    units: &[Unit],
    edges: &[(usize, usize, usize)],
    component_bonds: &AtomSet,
) -> Option<RingAssembly> {
    let junction_atoms: Vec<_> = path
        .windows(2)
        .map(|pair| {
            let &(_, _, id) = edges.iter().find(|&&(a, b, _)| {
                (a == pair[0] && b == pair[1]) || (b == pair[0] && a == pair[1])
            })?;
            let bond = &mol.bonds[&id];
            Some(if units[pair[0]].maps[0].contains_key(&bond.u) {
                (bond.u, bond.v)
            } else {
                (bond.v, bond.u)
            })
        })
        .collect::<Option<_>>()?;
    let mut combinations = vec![BTreeMap::new()];
    for (position, &index) in path.iter().enumerate() {
        let attachments: Vec<_> = position
            .checked_sub(1)
            .map(|i| junction_atoms[i].1)
            .into_iter()
            .chain(junction_atoms.get(position).map(|p| p.0))
            .collect();
        let mut choices: Vec<_> = units[index]
            .maps
            .iter()
            .map(|map| {
                let citation: Vec<_> = attachments
                    .iter()
                    .map(|atom| retained_locant_sort_key(&map[atom]))
                    .collect();
                let mut set = citation.clone();
                set.sort();
                ((set, citation), map)
            })
            .collect();
        let best = choices
            .iter()
            .map(|(key, _)| key)
            .min()
            .cloned()
            .unwrap_or_default();
        choices.retain(|(key, _)| *key == best);
        // Numberings that agree on every atom a name can cite give the same
        // name, so keep one of each; symmetric units would otherwise multiply.
        let unit = &units[index].maps[0];
        let cited: Vec<_> = unit
            .keys()
            .copied()
            .filter(|&atom| {
                let a = &mol.atoms[&atom];
                a.stereo.is_some()
                    || a.raw_stereo.is_some()
                    || a.isotope.is_some()
                    || units[index].indicated.contains(&atom)
                    || mol.get_neighbors(atom).iter().any(|n| {
                        !unit.contains_key(n)
                            || mol.get_bond(atom, *n).is_some_and(|b| b.stereo.is_some())
                    })
            })
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        choices.retain(|(_, map)| seen.insert(cited.iter().map(|a| &map[a]).collect::<Vec<_>>()));
        combinations = combinations
            .into_iter()
            .flat_map(|partial| {
                choices.iter().map(move |(_, map)| {
                    let mut combined = partial.clone();
                    combined
                        .extend(map.iter().map(|(&atom, locant)| {
                            (atom, decorated(locant, position, path.len()))
                        }));
                    combined
                })
            })
            .collect();
    }
    combinations.sort();
    combinations.dedup();
    let map = &combinations[0];
    let citation: Vec<_> = junction_atoms
        .iter()
        .flat_map(|&(u, v)| [&map[&u], &map[&v]])
        .map(|l| retained_locant_sort_key(l))
        .collect();
    let mut set = citation.clone();
    set.sort();
    let prefix = junction_atoms
        .iter()
        .map(|&(u, v)| format!("{},{}", map[&u], map[&v]))
        .collect::<Vec<_>>()
        .join(":");
    let parent = &units[path[0]].name;
    let parent = if parent == "benzene" {
        "phenyl"
    } else {
        parent
    };
    let multiplier = ["", "", "bi", "ter", "quater", "quinque", "sexi"][path.len()];
    // P-28.2.1 and P-16.5: separate locants belonging to the multiplied
    // hydride from the assembly locants. Digits inside a descriptor's square
    // brackets are already enclosed and do not require this separation.
    let mut bracket_depth = 0_usize;
    let locanted_parent = parent.bytes().any(|byte| match byte {
        b'[' => {
            bracket_depth += 1;
            false
        }
        b']' => {
            bracket_depth -= 1;
            false
        }
        _ => bracket_depth == 0 && byte.is_ascii_digit(),
    });
    let parent = if locanted_parent
        || path.len() == 2
            && (parent.starts_with("cyclo")
                || parent.starts_with("bicyclo")
                || parent.contains("spiro[")
                || super::ring_renderer::is_von_baeyer_descriptor(Some(parent)))
    {
        format!("({parent})")
    } else {
        parent.to_owned()
    };
    let junction_set: AtomSet = junction_atoms.iter().flat_map(|&(u, v)| [u, v]).collect();
    let indicated_hydrogen_atoms = path
        .iter()
        .flat_map(|&index| units[index].indicated.iter().copied())
        .filter(|atom| {
            !(junction_set.contains(atom)
                && (matches!(
                    mol.atoms[atom].symbol.as_str(),
                    "N" | "P" | "As" | "Sb" | "Bi"
                ) || mol.atoms[atom].symbol == "C"
                    && mol
                        .get_neighbors(*atom)
                        .iter()
                        .filter(|n| map.contains_key(n))
                        .count()
                        == 4)
                && mol.atoms[atom].total_h_count == 0)
        })
        .collect();
    Some(RingAssembly {
        name: format!("{prefix}-{multiplier}{parent}"),
        unit_name: units[path[0]].name.clone(),
        locant_maps: combinations,
        indicated_hydrogen_atoms,
        junction_bond_ids: component_bonds.clone(),
        ring_count: path.iter().map(|&i| units[i].ring_count).sum(),
        unit_atoms: units[path[0]].maps[0].keys().copied().collect(),
        junction_key: (set, citation),
    })
}

/// Every simple path through a tree of units, starting at `current`.
fn walk(
    current: usize,
    edges: &[(usize, usize, usize)],
    path: &mut Vec<usize>,
    paths: &mut Vec<Vec<usize>>,
) {
    paths.push(path.clone());
    for &(a, b, _) in edges {
        let next = if a == current {
            b
        } else if b == current {
            a
        } else {
            continue;
        };
        if !path.contains(&next) {
            path.push(next);
            walk(next, edges, path, paths);
            path.pop();
        }
    }
}

pub(super) fn discover_ring_assemblies(
    mol: &Molecule,
    systems: &[RingSystem],
) -> Vec<RingAssembly> {
    let atoms = cyclic_units(mol);
    let edges = identical_junctions(mol, &atoms);
    let mut unseen: AtomSet = edges.iter().flat_map(|&(a, b, _)| [a, b]).collect();
    let mut result = Vec::new();
    while let Some(&start) = unseen.first() {
        let mut component = AtomSet::new();
        let mut pending = vec![start];
        while let Some(index) = pending.pop() {
            if !component.insert(index) {
                continue;
            }
            unseen.remove(&index);
            pending.extend(edges.iter().filter_map(|&(a, b, _)| {
                if a == index {
                    Some(b)
                } else if b == index {
                    Some(a)
                } else {
                    None
                }
            }));
        }
        let component_edges: Vec<_> = edges
            .iter()
            .copied()
            .filter(|(a, b, _)| component.contains(a) && component.contains(b))
            .collect();
        let mut component_paths = Vec::new();
        for &start in &component {
            walk(
                start,
                &component_edges,
                &mut vec![start],
                &mut component_paths,
            );
        }
        // P-28.5, P-28.6: seven or more units in a row take a phane name; the
        // whole original component counts, so a heptaphane cannot become
        // partial terphenyls. Branches off a shorter assembly are substituents.
        if component_paths.iter().any(|p| p.len() > 6)
            || component_edges.len() + 1 != component.len()
            || component_edges
                .iter()
                .any(|(_, _, id)| mol.bonds[id].order != 1)
        {
            continue;
        }
        let mut unit_vec = Vec::new();
        let mut indices = BTreeMap::new();
        for &index in &component {
            let available = systems.iter().find(|s| s.atoms == atoms[index]);
            let Some(system) = available else {
                continue;
            };
            let Some(unit) = resolve_unit(mol, &atoms[index], system) else {
                continue;
            };
            indices.insert(index, unit_vec.len());
            unit_vec.push(unit);
        }
        if indices.len() < 2 {
            continue;
        }
        let available_edges: Vec<_> = component_edges
            .iter()
            .filter_map(|&(a, b, id)| Some((*indices.get(&a)?, *indices.get(&b)?, id)))
            .collect();
        // A failed hydride resolution is an unsupported assembly, rather than
        // permission to name a shorter supported fragment of the same parent.
        if component
            .iter()
            .filter(|&&i| systems.iter().any(|s| s.atoms == atoms[i]))
            .count()
            != indices.len()
        {
            continue;
        }
        let mut paths = Vec::new();
        for start in 0..unit_vec.len() {
            walk(start, &available_edges, &mut vec![start], &mut paths);
        }
        let bonds = component_edges.iter().map(|(_, _, id)| *id).collect();
        let candidates: Vec<_> = paths
            .into_iter()
            .filter(|p| p.len() >= 2)
            .filter_map(|p| path_assembly(mol, &p, &unit_vec, &available_edges, &bonds))
            .collect();
        let mut merged: BTreeMap<AtomSet, RingAssembly> = BTreeMap::new();
        for candidate in candidates {
            let key = candidate.locant_maps[0].keys().copied().collect();
            if let Some(previous) = merged.get_mut(&key) {
                match candidate.junction_key.cmp(&previous.junction_key) {
                    std::cmp::Ordering::Less => *previous = candidate,
                    std::cmp::Ordering::Equal => {
                        previous.locant_maps.extend(candidate.locant_maps);
                        previous.locant_maps.sort();
                        previous.locant_maps.dedup();
                    }
                    std::cmp::Ordering::Greater => {}
                }
            } else {
                merged.insert(key, candidate);
            }
        }
        result.extend(merged.into_values());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::{chains::find_ring_systems, graph_io::read_smiles};
    use super::*;

    fn assemblies(smiles: &str) -> Vec<RingAssembly> {
        let mol = read_smiles(smiles).unwrap();
        let systems = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        let mut candidates = discover_ring_assemblies(&mol, &systems);
        let longest = candidates.iter().map(|a| a.ring_count).max().unwrap_or(0);
        candidates.retain(|a| a.ring_count == longest);
        candidates
    }

    #[test]
    fn identical_whole_hydrides_supply_fixed_assembly_numberings() {
        for (smiles, name, size) in [
            ("Oc1ccccc1-c1ccccc1O", "1,1'-biphenyl", 12),
            ("c1ccc(nc1)-c1ccccn1", "2,2'-bipyridine", 12),
            ("c1ccc(nc1)-c1cccnc1", "2,3'-bipyridine", 12),
            ("c1ccc(s1)-c1cccs1", "2,2'-bithiophene", 10),
            ("C1CC1C1CC1", "1,1'-bi(cyclopropane)", 6),
            ("C1CCC(CC1)C1CCCCC1", "1,1'-bi(cyclohexane)", 12),
            (
                "Oc1ccc2ccccc2c1-c1c(O)ccc2ccccc12",
                "1,1'-binaphthalene",
                20,
            ),
            ("c1ccc2ccccc2c1-c1ccc2ccccc2c1", "1,2'-binaphthalene", 20),
        ] {
            let candidates = assemblies(smiles);
            assert_eq!(candidates.len(), 1, "{smiles}: {candidates:?}");
            let candidate = &candidates[0];
            assert_eq!(candidate.name, name, "{smiles}");
            assert_eq!(candidate.junction_bond_ids.len(), 1);
            assert!(candidate.locant_maps.iter().all(|map| {
                map.len() == size
                    && map
                        .values()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == size
            }));
            assert!(!candidate.locant_maps.is_empty());
        }
    }

    #[test]
    fn three_unit_junctions_use_composite_locants() {
        for (smiles, name) in [
            (
                "c1ccc(cc1)-c1ccc(cc1)-c1ccccc1",
                "1^1,2^1:2^4,3^1-terphenyl",
            ),
            ("c1ccccc1-c1cccc(-c2ccccc2)c1", "1^1,2^1:2^3,3^1-terphenyl"),
            ("C1CC1C1CC1C1CC1", "1^1,2^1:2^2,3^1-tercyclopropane"),
        ] {
            let candidates = assemblies(smiles);
            assert_eq!(candidates.len(), 1, "{smiles}: {candidates:?}");
            assert_eq!(candidates[0].name, name);
            assert!(
                candidates[0]
                    .locant_maps
                    .iter()
                    .all(|map| { map.values().all(|locant| locant.contains('^')) })
            );
        }
    }

    #[test]
    fn branched_assembly_keeps_every_longest_parent_atom_set() {
        let candidates = assemblies("c1ccccc1-c1cc(-c2ccccc2)cc(-c2ccccc2)c1");
        let longest: Vec<_> = candidates
            .into_iter()
            .filter(|a| a.ring_count == 3)
            .collect();
        assert_eq!(longest.len(), 3, "{longest:?}");
        for candidate in longest {
            assert_eq!(candidate.name, "1^1,2^1:2^3,3^1-terphenyl");
            assert_eq!(candidate.locant_maps[0].len(), 18);
            assert_eq!(candidate.ring_count, 3);
            assert_eq!(candidate.junction_bond_ids.len(), 3);
        }
    }

    #[test]
    fn pyrrole_indicated_hydrogen_is_recomputed_at_junctions() {
        let carbon_linked = assemblies("c1c[nH]c(c1)-c1ccc[nH]1");
        assert_eq!(carbon_linked.len(), 1, "{carbon_linked:?}");
        assert_eq!(carbon_linked[0].name, "2,2'-bipyrrole");
        assert_eq!(carbon_linked[0].indicated_hydrogen_atoms.len(), 2);
        let nitrogen_linked = assemblies("c1ccn(c1)-n1cccc1");
        assert_eq!(nitrogen_linked.len(), 1, "{nitrogen_linked:?}");
        assert_eq!(nitrogen_linked[0].name, "1,1'-bipyrrole");
        assert!(nitrogen_linked[0].indicated_hydrogen_atoms.is_empty());
    }

    #[test]
    fn nonidentical_and_unsupported_assemblies_do_not_become_candidates() {
        for smiles in [
            "C1CCC(CC1)c1ccccc1",
            "c1ccc(cc1)-c1ccncc1",
            "C1CCCC1=C1CCCC1",
            "C1=CCCCC1C1=CCCCC1",
            "C1(CCC2=CC=CC=C12)C1CCC2=CC=CC=C12",
        ] {
            assert!(assemblies(smiles).is_empty(), "{smiles}");
        }
        let mol = read_smiles("C1CCCC1=C1CCCC1").unwrap();
        assert_eq!(assembly_junctions(&mol).len(), 1);
    }

    #[test]
    fn relocated_indicated_hydrogen_keeps_unsupported_assembly_status() {
        use crate::naming::{NameReason, NameStatus};
        for (smiles, expected) in [
            (
                "C1C(=CC2=CC=CC=C12)C=1C2=CCC=C2C=CC1",
                "4-(1H-inden-2-yl)-2H-indene",
            ),
            (
                "N1C=CC(=CC=C1)C=1CC=NC=CC1",
                "4-(1H-azepin-4-yl)-3H-azepine",
            ),
        ] {
            let mol = read_smiles(smiles).unwrap();
            assert!(assemblies(smiles).is_empty(), "{smiles}");
            assert_eq!(assembly_junctions(&mol).len(), 1, "{smiles}");
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                NameStatus::NotPreferred(NameReason::RingAssembly),
                "{smiles}"
            );
        }
        for smiles in ["C1CCC(CC1)c1ccccc1", "c1ccc(cc1)-c1ccncc1"] {
            let mol = read_smiles(smiles).unwrap();
            assert!(assembly_junctions(&mol).is_empty(), "{smiles}");
        }
    }

    #[test]
    fn seven_unit_components_remain_unsupported_in_recursive_fragments() {
        let mol =
            read_smiles("c1ccccc1-c1ccc(-c2ccc(-c3ccc(-c4ccc(-c5ccc(-c6ccccc6)cc5)cc4)cc3)cc2)cc1")
                .unwrap();
        let all = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        assert_eq!(all.len(), 7);
        assert!(discover_ring_assemblies(&mol, &all).is_empty());
        let excluded = all[0].atoms.clone();
        let fragment = find_ring_systems(&mol, &excluded).unwrap();
        assert_eq!(fragment.len(), 6);
        assert!(discover_ring_assemblies(&mol, &fragment).is_empty());
        assert_eq!(assembly_junctions(&mol).len(), 6);
    }
}
