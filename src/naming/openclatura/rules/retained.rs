//! Retained monocyclic and cage parents and graph-derived Hantzsch-Widman names.
use super::super::{
    molecule::{AtomSet, Molecule},
    nomenclature::section,
};
use super::{elements, elision, multipliers};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub struct RetainedMatch {
    pub name: String,
    pub locant_maps: Vec<BTreeMap<usize, String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonocycleSpec {
    pub name: String,
    pub size: usize,
    pub double_bonds: usize,
    #[serde(default)]
    pub symbols: BTreeMap<String, usize>,
    #[serde(default)]
    pub no_cumulated_double_bonds: bool,
    pub hetero_distance_multiset: Option<Vec<usize>>,
    pub hetero_gap_multiset: Option<Vec<usize>>,
    pub hetero_gap_cycle: Option<Vec<usize>>,
    pub chalcogen_nitrogen_distance_multiset: Option<Vec<usize>>,
}

#[allow(
    clippy::expect_used,
    reason = "Monocycle specs are immutable checked-in registry data."
)]
static SPECS: LazyLock<Vec<MonocycleSpec>> = LazyLock::new(|| {
    serde_json::from_value(section("retained_monocycle_specs").clone()).expect("monocycle specs")
});
#[allow(
    clippy::expect_used,
    reason = "Retained graph templates are immutable checked-in JSON data."
)]
static GRAPH_PARENTS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/retained_fused_graph_templates.json"))
        .expect("retained graph data")
});

fn monocycle_spec_for_name(name: &str) -> Option<MonocycleSpec> {
    SPECS.iter().find(|spec| spec.name == name).cloned()
}

/// Upstream memoizes generated specs by name for later hydro/indicated-H use.
/// Those callers also have the ring, so reproduce its spec directly and check
/// the name instead of making one molecule's result depend on earlier calls.
fn generated_spec_for_name(mol: &Molecule, path: &[usize], name: &str) -> Option<MonocycleSpec> {
    hw_spec_for_ring(mol, path).filter(|spec| spec.name == name)
}

pub fn get_retained_ring(mol: &Molecule, path: &[usize]) -> Option<RetainedMatch> {
    if !is_monocycle(mol, path) {
        return match_fused_templates(mol, path);
    }
    let double_bonds = count_double_bonds_in_ring(mol, path);
    let symbols: Vec<String> = path.iter().map(|i| mol.atoms[i].symbol.clone()).collect();
    let name = match_monocycle_spec(mol, path, double_bonds, &symbols, false)
        .or_else(|| match_monocycle_templates(mol, path, double_bonds))
        .or_else(|| {
            if symbols
                .iter()
                .any(|s| !matches!(s.as_str(), "C" | "N" | "O" | "S"))
            {
                generated_monocycle(mol, path, double_bonds)
            } else {
                match_monocycle_spec(mol, path, double_bonds, &symbols, true)
                    .or_else(|| generated_monocycle(mol, path, double_bonds))
            }
        })?;
    Some(RetainedMatch {
        name,
        locant_maps: Vec::new(),
    })
}

/// Retained parents selected before descriptor discovery by template capability.
pub fn get_pre_descriptor_retained_ring(mol: &Molecule, path: &[usize]) -> Option<RetainedMatch> {
    use super::super::retained_fused_templates::{
        MatchOptions, match_retained_graph_templates_with_options,
    };
    let ring = path.iter().copied().collect();
    let mut matches = match_retained_graph_templates_with_options(
        mol,
        &ring,
        MatchOptions {
            pre_descriptor_only: true,
            ..Default::default()
        },
    );
    if matches.is_empty() {
        matches = match_retained_graph_templates_with_options(
            mol,
            &ring,
            MatchOptions {
                allow_nonaromatic: true,
                pre_descriptor_only: true,
                family: Some("macrocycle"),
                ..Default::default()
            },
        );
    }
    let name = matches.first()?.template.name.clone();
    Some(RetainedMatch {
        locant_maps: matches
            .into_iter()
            .filter(|m| m.template.name == name)
            .map(|m| m.atom_to_locant)
            .collect(),
        name,
    })
}
fn match_fused_templates(mol: &Molecule, path: &[usize]) -> Option<RetainedMatch> {
    use super::super::retained_fused_templates::match_retained_graph_templates;
    let ring: AtomSet = path.iter().copied().collect();
    let mut matches = match_retained_graph_templates(mol, &ring, false);
    if matches.is_empty()
        && path.iter().all(|idx| {
            mol.atoms[idx].charge == 0
                && mol
                    .get_neighbors(*idx)
                    .iter()
                    .all(|n| ring.contains(n) || mol.bond_order(*idx, *n) <= 1)
        })
    {
        matches = match_retained_graph_templates(mol, &ring, true);
    }
    let name = matches.first()?.template.name.clone();
    Some(RetainedMatch {
        locant_maps: matches
            .into_iter()
            .filter(|m| m.template.name == name)
            .map(|m| m.atom_to_locant)
            .collect(),
        name,
    })
}

fn is_monocycle(mol: &Molecule, path: &[usize]) -> bool {
    if path.len() < 3 {
        return false;
    }
    let ring: AtomSet = path.iter().copied().collect();
    ring.len() == path.len()
        && path.iter().enumerate().all(|(i, &idx)| {
            mol.atoms.contains_key(&idx)
                && mol.get_bond(idx, path[(i + 1) % path.len()]).is_some()
                && mol
                    .get_neighbors(idx)
                    .iter()
                    .filter(|n| ring.contains(n))
                    .count()
                    == 2
        })
}

pub fn count_double_bonds_in_ring(mol: &Molecule, path: &[usize]) -> usize {
    if path.is_empty() {
        return 0;
    }
    path.iter()
        .enumerate()
        .filter(|&(i, &idx)| mol.bond_order(idx, path[(i + 1) % path.len()]) == 2)
        .count()
}

fn no_cumulated_double_bonds(mol: &Molecule, path: &[usize]) -> bool {
    let ring: AtomSet = path.iter().copied().collect();
    path.iter().all(|&idx| {
        mol.get_neighbors(idx)
            .iter()
            .filter(|&&n| ring.contains(&n) && mol.bond_order(idx, n) == 2)
            .count()
            <= 1
    })
}

fn match_monocycle_spec(
    mol: &Molecule,
    path: &[usize],
    doubles: usize,
    symbols: &[String],
    hydro: bool,
) -> Option<String> {
    SPECS
        .iter()
        .find(|spec| {
            if spec.size != path.len() {
                return false;
            }
            if hydro {
                if spec.symbols.is_empty() || doubles == 0 || doubles >= spec.double_bonds {
                    return false;
                }
            } else if doubles != spec.double_bonds {
                return false;
            }
            if spec.no_cumulated_double_bonds && !no_cumulated_double_bonds(mol, path) {
                return false;
            }
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for symbol in symbols.iter().filter(|s| s.as_str() != "C") {
                *counts.entry(symbol.clone()).or_default() += 1;
            }
            if counts != spec.symbols {
                return false;
            }
            if let Some(expected) = &spec.hetero_distance_multiset
                && hetero_distances(mol, path) != sorted(expected)
            {
                return false;
            }
            if let Some(expected) = &spec.hetero_gap_multiset
                && sorted(&hetero_gaps(mol, path)) != sorted(expected)
            {
                return false;
            }
            if let Some(expected) = &spec.hetero_gap_cycle
                && hetero_gap_cycle(mol, path) != *expected
            {
                return false;
            }
            if let Some(expected) = &spec.chalcogen_nitrogen_distance_multiset
                && chalcogen_distances(mol, path) != sorted(expected)
            {
                return false;
            }
            !hydro || mancude_hydro_plan(mol, path, spec.double_bonds).is_some()
        })
        .map(|s| s.name.clone())
}

fn sorted(values: &[usize]) -> Vec<usize> {
    let mut values = values.to_vec();
    values.sort();
    values
}
fn hetero_positions(mol: &Molecule, path: &[usize]) -> Vec<usize> {
    path.iter()
        .enumerate()
        .filter_map(|(i, idx)| (mol.atoms[idx].symbol != "C").then_some(i))
        .collect()
}
fn hetero_distances(mol: &Molecule, path: &[usize]) -> Vec<usize> {
    let positions = hetero_positions(mol, path);
    let mut result = Vec::new();
    for (i, &left) in positions.iter().enumerate() {
        for &right in &positions[i + 1..] {
            let d = left.abs_diff(right);
            result.push(d.min(path.len() - d));
        }
    }
    result.sort();
    result
}
fn chalcogen_distances(mol: &Molecule, path: &[usize]) -> Vec<usize> {
    let mut result = Vec::new();
    for (i, _) in path
        .iter()
        .enumerate()
        .filter(|(_, idx)| matches!(mol.atoms[idx].symbol.as_str(), "O" | "S"))
    {
        for (j, _) in path
            .iter()
            .enumerate()
            .filter(|(_, idx)| mol.atoms[idx].symbol == "N")
        {
            let d = i.abs_diff(j);
            result.push(d.min(path.len() - d));
        }
    }
    result.sort();
    result
}
fn hetero_gaps(mol: &Molecule, path: &[usize]) -> Vec<usize> {
    let p = hetero_positions(mol, path);
    if p.len() < 3 {
        return Vec::new();
    }
    (0..p.len())
        .map(|i| {
            if i + 1 < p.len() {
                p[i + 1] - p[i]
            } else {
                path.len() - p[i] + p[0]
            }
        })
        .collect()
}
fn hetero_gap_cycle(mol: &Molecule, path: &[usize]) -> Vec<usize> {
    let gaps = hetero_gaps(mol, path);
    let reverse: Vec<_> = gaps.iter().rev().copied().collect();
    [&gaps, &reverse]
        .into_iter()
        .flat_map(|s| {
            (0..s.len()).map(|offset| {
                s[offset..]
                    .iter()
                    .chain(&s[..offset])
                    .copied()
                    .collect::<Vec<_>>()
            })
        })
        .min()
        .unwrap_or_default()
}

/// The source template registry also contains monocyclic HW parents. Retain its
/// preference order and its bond-count gate before generating an HW fallback.
fn match_monocycle_templates(mol: &Molecule, path: &[usize], doubles: usize) -> Option<String> {
    if !no_cumulated_double_bonds(mol, path) {
        return None;
    }
    let ring: AtomSet = path.iter().copied().collect();
    let mut rows: Vec<&Value> = GRAPH_PARENTS["parents"]
        .as_array()?
        .iter()
        .filter(|row| {
            let t = &row["template"];
            t["enabled"].as_bool().unwrap_or(true)
                && t["atoms"].as_array().is_some_and(|a| a.len() == path.len())
                && t["bonds"].as_array().is_some_and(|b| b.len() == path.len())
                && t["mancude_double_bonds"].as_u64().unwrap_or(0) as usize == doubles
        })
        .collect();
    rows.sort_by_key(|row| {
        (
            row["priority"].as_i64().unwrap_or(0),
            row["name"].as_str().unwrap_or(""),
        )
    });
    for row in rows {
        let t = &row["template"];
        let atoms = t["atoms"].as_array()?;
        if atoms.iter().all(|a| a["aromatic"].as_bool() == Some(false))
            && path
                .iter()
                .any(|&idx| !is_saturated_ring_site(mol, idx, &ring))
        {
            continue;
        }
        let mut numbered: Vec<_> = atoms.iter().collect();
        numbered.sort_by_key(|atom| {
            atom["locant"]
                .as_str()
                .unwrap_or("")
                .parse::<usize>()
                .unwrap_or(usize::MAX)
        });
        let symbols: Vec<&str> = numbered
            .iter()
            .map(|a| a["symbol"].as_str().unwrap_or("C"))
            .collect();
        if (0..path.len()).any(|offset| {
            [1_i64, -1].into_iter().any(|step| {
                (0..path.len()).all(|i| {
                    let p =
                        (offset as i64 + step * i as i64).rem_euclid(path.len() as i64) as usize;
                    mol.atoms[&path[p]].symbol == symbols[i]
                        && (!numbered[i]["saturated"].as_bool().unwrap_or(false)
                            || is_saturated_ring_site(mol, path[p], &ring))
                })
            })
        }) {
            return row["name"].as_str().map(str::to_owned);
        }
    }
    None
}

pub fn hw_stem(size: usize, saturated: bool, symbols: &[String]) -> Option<&'static str> {
    let nitrogen = symbols.iter().any(|s| s == "N");
    Some(match size {
        3 => match (nitrogen, saturated) {
            (true, true) => "iridine",
            (true, false) => "irine",
            (false, true) => "irane",
            (false, false) => "irene",
        },
        4 => match (nitrogen, saturated) {
            (true, true) => "etidine",
            (false, true) => "etane",
            (_, false) => "ete",
        },
        5 => match (nitrogen, saturated) {
            (true, true) => "olidine",
            (false, true) => "olane",
            (_, false) => "ole",
        },
        6 => {
            let least = symbols
                .iter()
                .filter(|s| s.as_str() != "C")
                .max_by_key(|s| elements::get(s).and_then(|e| e.hw_priority).unwrap_or(0))?;
            match (least.as_str(), saturated) {
                ("O" | "S" | "Se" | "Te" | "Bi", true) => "ane",
                ("O" | "S" | "Se" | "Te" | "Bi", false) => "ine",
                ("N" | "Si" | "Ge" | "Sn" | "Pb", true) => "inane",
                ("N" | "Si" | "Ge" | "Sn" | "Pb", false) => "ine",
                ("B" | "F" | "Cl" | "Br" | "I" | "P" | "As" | "Sb", true) => "inane",
                ("B" | "F" | "Cl" | "Br" | "I" | "P" | "As" | "Sb", false) => "inine",
                _ => return None,
            }
        }
        7 => {
            if saturated {
                "epane"
            } else {
                "epine"
            }
        }
        8 => {
            if saturated {
                "ocane"
            } else {
                "ocine"
            }
        }
        9 => {
            if saturated {
                "onane"
            } else {
                "onine"
            }
        }
        10 => {
            if saturated {
                "ecane"
            } else {
                "ecine"
            }
        }
        _ => return None,
    })
}

pub fn hw_name(size: usize, saturated: bool, hetero: &[(usize, String)]) -> Option<String> {
    if hetero.is_empty() {
        return None;
    }
    let symbols: Vec<_> = hetero.iter().map(|(_, s)| s.clone()).collect();
    let stem = hw_stem(size, saturated, &symbols)?;
    let mut groups: BTreeMap<usize, Vec<(usize, String)>> = BTreeMap::new();
    for (locant, symbol) in hetero {
        let element = elements::get(symbol)?;
        groups
            .entry(element.hw_priority?)
            .or_default()
            .push((*locant, element.hw_stem.clone()?));
    }
    let mut locants = Vec::new();
    let mut prefixes = Vec::new();
    for group in groups.values_mut() {
        group.sort();
        locants.extend(group.iter().map(|(locant, _)| *locant));
        prefixes.push(if group.len() == 1 {
            group[0].1.clone()
        } else {
            elision::elide_terminal_a(&multipliers::basic(group.len()), &group[0].1)
        });
    }
    prefixes.push(stem.into());
    let mut parts = prefixes.into_iter();
    let mut parent = parts.next()?;
    for part in parts {
        parent = elision::elide_terminal_a(&parent, &part);
    }
    if hetero.len() == 1 && locants == [1] {
        Some(parent)
    } else {
        Some(format!(
            "{}-{parent}",
            locants
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ))
    }
}

pub fn canonical_ring_orders(mol: &Molecule, path: &[usize]) -> Vec<Vec<usize>> {
    if !path.iter().any(|i| mol.atoms[i].symbol != "C") {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for offset in 0..path.len() {
        for step in [1_i64, -1] {
            let order: Vec<_> = (0..path.len())
                .map(|i| {
                    path[(offset as i64 + step * i as i64).rem_euclid(path.len() as i64) as usize]
                })
                .collect();
            let hetero: Vec<_> = order
                .iter()
                .enumerate()
                .filter(|(_, idx)| mol.atoms[idx].symbol != "C")
                .map(|(i, idx)| {
                    elements::get(&mol.atoms[idx].symbol)
                        .and_then(|e| e.hw_priority)
                        .map(|p| (i + 1, p))
                })
                .collect::<Option<_>>()
                .unwrap_or_default();
            let Some(&(_, first_priority)) = hetero.first() else {
                continue;
            };
            if Some(first_priority) != hetero.iter().map(|(_, p)| *p).min() {
                continue;
            }
            let locants: Vec<_> = hetero.iter().map(|(l, _)| *l).collect();
            let mut by_seniority: Vec<_> = hetero.into_iter().map(|(l, p)| (p, l)).collect();
            by_seniority.sort();
            candidates.push(((locants, by_seniority), order));
        }
    }
    let Some(best) = candidates.iter().map(|(key, _)| key).min().cloned() else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter_map(|(key, order)| (key == best && seen.insert(order.clone())).then_some(order))
        .collect()
}

pub fn mancude_bond_orders(symbols: &[String]) -> Vec<usize> {
    let n = symbols.len();
    if n == 0 {
        return Vec::new();
    }
    let mut orders = vec![1; n];
    let fixed = |s: &str| elements::get(s).is_some_and(|e| e.mancude_forced_single);
    let start = symbols.iter().position(|s| fixed(s)).unwrap_or(n - 1);
    let mut run = Vec::new();
    let pair = |orders: &mut Vec<usize>, run: &[usize]| {
        for &[a, b] in run.as_chunks::<2>().0 {
            orders[if (a + 1) % n == b { a } else { b }] = 2;
        }
    };
    for offset in 1..=n {
        let p = (start + offset) % n;
        if fixed(&symbols[p]) {
            pair(&mut orders, &run);
            run.clear();
        } else {
            run.push(p);
        }
    }
    pair(&mut orders, &run);
    orders
}

pub fn hw_spec_for_ring(mol: &Molecule, path: &[usize]) -> Option<MonocycleSpec> {
    if !(3..=10).contains(&path.len()) || path.iter().any(|&idx| needs_lambda(mol, idx)) {
        return None;
    }
    let orders = canonical_ring_orders(mol, path);
    let order = orders.first()?;
    let mut doubles = 0;
    for (i, &idx) in path.iter().enumerate() {
        match mol.get_bond(idx, path[(i + 1) % path.len()])?.order {
            1 => (),
            2 => doubles += 1,
            _ => return None,
        }
    }
    let hetero: Vec<_> = order
        .iter()
        .enumerate()
        .filter(|(_, idx)| mol.atoms[idx].symbol != "C")
        .map(|(i, idx)| (i + 1, mol.atoms[idx].symbol.clone()))
        .collect();
    let name = hw_name(path.len(), doubles == 0, &hetero)?;
    let ring_symbols: Vec<_> = order
        .iter()
        .map(|idx| mol.atoms[idx].symbol.clone())
        .collect();
    let mut symbols = BTreeMap::new();
    for (_, s) in &hetero {
        *symbols.entry(s.clone()).or_default() += 1;
    }
    Some(MonocycleSpec {
        name,
        size: path.len(),
        double_bonds: if doubles == 0 {
            0
        } else {
            mancude_bond_orders(&ring_symbols)
                .into_iter()
                .filter(|&n| n == 2)
                .count()
        },
        symbols,
        ..Default::default()
    })
}

fn needs_lambda(mol: &Molecule, idx: usize) -> bool {
    let atom = &mol.atoms[&idx];
    if atom.symbol == "C" || atom.charge != 0 {
        return false;
    }
    let Some(element) = elements::get(&atom.symbol) else {
        return true;
    };
    let mut bonding = atom.total_h_count;
    for &n in mol.get_neighbors(idx) {
        if let Some(b) = mol.get_bond(idx, n) {
            if b.order == 2 && mol.atoms[&n].symbol == "O" {
                continue;
            }
            bonding += b.order;
        }
    }
    bonding != element.standard_valence
}

fn generated_monocycle(mol: &Molecule, path: &[usize], doubles: usize) -> Option<String> {
    let spec = hw_spec_for_ring(mol, path)?;
    if doubles == spec.double_bonds
        || (doubles > 0
            && doubles < spec.double_bonds
            && mancude_hydro_plan(mol, path, spec.double_bonds).is_some())
    {
        Some(spec.name)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct HydroPlan {
    pub indicated: usize,
    pub added: usize,
    pub citable: Vec<usize>,
}

pub fn mancude_monocycle_hydro_plan(
    mol: &Molecule,
    path: &[usize],
    retained_name: Option<&str>,
) -> Option<HydroPlan> {
    let name = retained_name?;
    // rules.retained prefers a matching mancude registry spec before HW.
    let spec = monocycle_spec_for_name(name)
        .filter(|spec| spec.double_bonds > 0 && spec.size == path.len())
        .or_else(|| generated_spec_for_name(mol, path, name))?;
    mancude_hydro_plan(mol, path, spec.double_bonds)
}

pub fn mancude_hydro_plan(
    mol: &Molecule,
    path: &[usize],
    parent_double_bonds: usize,
) -> Option<HydroPlan> {
    if parent_double_bonds == 0 || count_double_bonds_in_ring(mol, path) >= parent_double_bonds {
        return None;
    }
    let ring: AtomSet = path.iter().copied().collect();
    let exocyclic: AtomSet = path
        .iter()
        .filter(|&&idx| {
            mol.get_neighbors(idx)
                .iter()
                .any(|&n| !ring.contains(&n) && mol.bond_order(idx, n) > 1)
        })
        .copied()
        .collect();
    let saturated: Vec<_> = path
        .iter()
        .filter(|&&idx| !exocyclic.contains(&idx) && is_saturated_ring_site(mol, idx, &ring))
        .copied()
        .collect();
    let fixed: AtomSet = saturated
        .iter()
        .filter(|&&idx| {
            elements::get(&mol.atoms[&idx].symbol).is_some_and(|e| e.mancude_forced_single)
                || (mol.atoms[&idx].total_h_count == 0
                    && mol
                        .get_neighbors(idx)
                        .iter()
                        .filter(|n| ring.contains(n))
                        .count()
                        > 2)
        })
        .copied()
        .collect();
    let citable: Vec<_> = saturated
        .into_iter()
        .filter(|idx| !fixed.contains(idx))
        .collect();
    let parent_saturated = path.len().saturating_sub(2 * parent_double_bonds);
    let indicated = parent_saturated
        .saturating_sub(fixed.len())
        .min(citable.len());
    let added = exocyclic.len().min(citable.len() - indicated);
    if !(citable.len() - indicated - added).is_multiple_of(2) {
        return None;
    }
    Some(HydroPlan {
        indicated,
        added,
        citable,
    })
}

pub fn is_saturated_ring_site(mol: &Molecule, idx: usize, ring: &AtomSet) -> bool {
    let bonds: Vec<_> = mol
        .get_neighbors(idx)
        .iter()
        .filter(|n| ring.contains(n))
        .filter_map(|&n| mol.get_bond(idx, n))
        .collect();
    !bonds.is_empty() && bonds.iter().all(|b| b.order == 1)
}

pub fn cites_indicated_hydrogen(mol: &Molecule, path: &[usize], name: Option<&str>) -> bool {
    let Some(name) = name else {
        return false;
    };
    // namer_config checks generated HW metadata before the retained registry.
    if let Some(spec) =
        generated_spec_for_name(mol, path, name).or_else(|| monocycle_spec_for_name(name))
    {
        let fixed: usize = spec
            .symbols
            .iter()
            .filter(|(s, _)| elements::get(s).is_some_and(|e| e.mancude_forced_single))
            .map(|(_, n)| *n)
            .sum();
        return spec.double_bonds > 0 && spec.size > 2 * spec.double_bonds + fixed;
    }
    GRAPH_PARENTS["parents"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["indicated_hydrogen_name"].as_str() == Some(name))
    })
}

pub fn parent_metadata(name: &str) -> Option<super::super::assembly_parts::RetainedParentMetadata> {
    super::super::retained_fused_templates::retained_parent_metadata(name)
}

#[cfg(test)]
mod tests {
    use super::super::super::{chains::find_ring_systems, graph_io::read_smiles};
    use super::*;
    fn retained_name(smiles: &str) -> Option<String> {
        let mol = read_smiles(smiles).unwrap();
        let ring = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        get_retained_ring(&mol, &ring[0].paths[0]).map(|r| r.name)
    }
    #[test]
    fn retained_and_hw_monocycles_have_source_spellings() {
        for (smiles, name) in [
            ("n1ccccc1", "pyridine"),
            ("C1CNCCO1", "morpholine"),
            ("C1CCNCCC1", "azepane"),
            ("O1COCC1", "1,3-dioxolane"),
            ("s1nncc1", "1,2,3-thiadiazole"),
            ("O1CBCC1", "1,3-oxaborolane"),
        ] {
            assert_eq!(retained_name(smiles).as_deref(), Some(name), "{smiles}");
        }
    }
    #[test]
    fn retained_cages_match_connectivity_and_all_numberings() {
        for (smiles, name, numberings) in [
            ("C1C2CC3CC1CC(C2)C3", "adamantane", 24),
            ("C12C3C4C1C5C2C3C45", "cubane", 48),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let path: Vec<_> = mol.atoms.keys().copied().collect();
            let retained = get_retained_ring(&mol, &path).unwrap();
            assert_eq!(retained.name, name);
            assert_eq!(retained.locant_maps.len(), numberings);
        }
        let mol = read_smiles("C1CC2CCC1CC2").unwrap();
        let path: Vec<_> = mol.atoms.keys().copied().collect();
        assert!(get_retained_ring(&mol, &path).is_none());
    }
    #[test]
    fn retained_cages_require_the_exact_neutral_template_charge() {
        for smiles in [
            "[C+]12C3C4C1C5C2C3C45",
            "[C-]12C3C4C1C5C2C3C45",
            "C1[C+]2CC3CC1CC(C2)C3",
            "C1[C-]2CC3CC1CC(C2)C3",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let path: Vec<_> = mol.atoms.keys().copied().collect();
            assert!(get_retained_ring(&mol, &path).is_none(), "{smiles}");
        }
    }
    #[test]
    fn partly_saturated_parent_reports_hydro_positions() {
        let mol = read_smiles("N1CCC=CC1").unwrap();
        let ring = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        let path = &ring[0].paths[0];
        assert_eq!(get_retained_ring(&mol, path).unwrap().name, "pyridine");
        let plan = mancude_monocycle_hydro_plan(&mol, path, Some("pyridine")).unwrap();
        assert_eq!((plan.indicated, plan.added, plan.citable.len()), (0, 0, 4));
    }
    #[test]
    fn generated_hydrogen_metadata_is_available_without_prior_recognition() {
        // Upstream HW specs for these rings are azepine (3 double bonds,
        // indicated H) and oxepine (3 double bonds, fixed saturated O).
        let mol = read_smiles("N1CCC=CC=C1").unwrap();
        let rings = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        let path = &rings[0].paths[0];
        let plan = mancude_monocycle_hydro_plan(&mol, path, Some("azepine")).unwrap();
        assert_eq!(
            (plan.indicated, plan.added, plan.citable),
            (1, 0, vec![0, 1, 2])
        );
        assert!(cites_indicated_hydrogen(&mol, path, Some("azepine")));
        assert!(mancude_monocycle_hydro_plan(&mol, path, Some("oxepine")).is_none());

        let oxygen = read_smiles("O1C=CC=CC=C1").unwrap();
        let oxygen_rings = find_ring_systems(&oxygen, &AtomSet::new()).unwrap();
        let oxygen_path = &oxygen_rings[0].paths[0];
        assert!(!cites_indicated_hydrogen(
            &oxygen,
            oxygen_path,
            Some("oxepine")
        ));
        assert!(!cites_indicated_hydrogen(
            &oxygen,
            oxygen_path,
            Some("azepine")
        ));

        assert_eq!(get_retained_ring(&mol, path).unwrap().name, "azepine");
        assert!(cites_indicated_hydrogen(&mol, path, Some("azepine")));
    }
    #[test]
    fn ring_triple_bonds_cannot_be_hw_saturated_parents() {
        assert!(retained_name("N1CC#CCCC1").is_none());
    }
}
