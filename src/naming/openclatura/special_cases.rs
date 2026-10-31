//! Whole-component shortcuts from Openclatura's `special_cases.py`.
//!
//! Matching is graph based; bindings and audit-only metadata are omitted from
//! this plain-name port.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::assembly_prefixes::substituent_sort_key;
use super::chains::{canonical_ranks, get_cyclic_atoms};
use super::formatting::{
    format_center_ligands_by, format_multiplier, oxy_prefix_from_branch, strip_outer_parentheses,
};
use super::molecule::{Atom, AtomSet, Bond, Molecule, component_atoms_until_blocked};
use super::namer::name_subgraph_with_upstream;
use super::nomenclature::RULES;
use super::perception::{PerceivedGroup, perceive_groups};
use super::rules::{elements, multipliers, retained, stems};
use super::subgraph_tools::subgraph_component;

fn set<const N: usize>(atoms: [usize; N]) -> AtomSet {
    atoms.into_iter().collect()
}
fn all_atoms(mol: &Molecule) -> AtomSet {
    mol.atoms.keys().copied().collect()
}
fn outside(mol: &Molecule, atoms: &AtomSet) -> AtomSet {
    all_atoms(mol).difference(atoms).copied().collect()
}
fn order(mol: &Molecule, u: usize, v: usize) -> usize {
    mol.get_bond(u, v).map_or(0, |b| b.order)
}
fn stereo_configured(mol: &Molecule, idx: usize) -> bool {
    mol.atoms[&idx].stereo.is_some() || mol.atoms[&idx].raw_stereo.is_some()
}
fn require_stereo_free_core(mol: &Molecule, atoms: &AtomSet) -> Result<(), String> {
    // Upstream divergence: manually spelled shortcut cores must not discard configured atom or bond stereo.
    if atoms.iter().any(|&idx| stereo_configured(mol, idx))
        || mol
            .bonds
            .values()
            .any(|bond| atoms.contains(&bond.u) && atoms.contains(&bond.v) && bond.stereo.is_some())
    {
        return Err("openclatura: shortcut cannot preserve configured stereochemistry".into());
    }
    Ok(())
}
fn center_ligands(names: &[String]) -> String {
    format_center_ligands_by(names, substituent_sort_key)
}
fn branch_name(
    mol: &Molecule,
    component: &AtomSet,
    root: usize,
    blocked: &AtomSet,
    upstream: usize,
) -> Result<String, String> {
    let mut exclude = outside(mol, component);
    exclude.extend(blocked);
    name_subgraph_with_upstream(mol, root, &exclude, Some(upstream))
}

pub fn single_atom_component_name(mol: &Molecule, component_atoms: &AtomSet) -> Option<String> {
    if component_atoms.len() != 1 {
        return None;
    }
    let atom = &mol.atoms[component_atoms.first()?];
    // Upstream divergence: a retained single-atom hydride does not cite atom stereo.
    if stereo_configured(mol, atom.idx) {
        return None;
    }
    if atom.charge > 0 && RULES.ions.single_atom_cations.contains(&atom.symbol) {
        return elements::get(&atom.symbol).map(|e| e.name.clone());
    }
    if atom.charge < 0
        && let Some(name) = RULES.ions.single_atom_anions.get(&atom.symbol)
    {
        return Some(name.clone());
    }
    let ion = match (atom.symbol.as_str(), atom.charge, atom.total_h_count) {
        ("N", 1, 4) => "ammonium",
        ("N", -1, 2) => "azanide",
        ("O", -1, 1) => "hydroxide",
        ("O", -2, 0) => "oxide",
        ("O", 1, 3) => "oxonium",
        ("S", -1, 1) => "sulfanide",
        ("S", -2, 0) => "sulfide",
        ("P", 1, 4) => "phosphanium",
        ("H", 1, 0) => "hydron",
        ("H", -1, 0) => "hydride",
        ("Se", -2, 0) => "selenide",
        _ => "",
    };
    if !ion.is_empty() {
        return Some(ion.into());
    }
    if atom.charge != 0 || atom.total_h_count != elements::get(&atom.symbol)?.standard_valence {
        return None;
    }
    let retained = match atom.symbol.as_str() {
        "N" => "ammonia",
        "O" => "water",
        "F" => "hydrogen fluoride",
        "Cl" => "hydrogen chloride",
        "Br" => "hydrogen bromide",
        "I" => "hydrogen iodide",
        _ => "",
    };
    if !retained.is_empty() {
        return Some(retained.into());
    }
    RULES
        .components
        .mononuclear_parent_hydrides
        .get(&atom.symbol)
        .cloned()
}

type SmallSignature = (Vec<(String, i32)>, Vec<(String, String, usize)>);
type SmallParentSpec<'a> = (
    &'a str,
    &'a [(&'a str, i32)],
    &'a [(&'a str, &'a str, usize)],
);
fn small_signature(mol: &Molecule, atoms: &AtomSet) -> SmallSignature {
    let mut atom_sig: Vec<_> = atoms
        .iter()
        .map(|i| (mol.atoms[i].symbol.clone(), mol.atoms[i].charge))
        .collect();
    atom_sig.sort();
    let mut bonds: Vec<_> = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| {
            let (a, z) = (&mol.atoms[&b.u].symbol, &mol.atoms[&b.v].symbol);
            (a.min(z).clone(), a.max(z).clone(), b.order)
        })
        .collect();
    bonds.sort();
    (atom_sig, bonds)
}

fn small_inorganic_parent_name(mol: &Molecule, atoms: &AtomSet) -> Option<String> {
    if !(2..=9).contains(&atoms.len())
        || atoms
            .iter()
            .any(|i| mol.atoms[i].is_carbon() && mol.atoms[i].total_h_count != 0)
    {
        return None;
    }
    // The upstream retained table is a multiset of charged atoms and bonds.
    let specs: &[SmallParentSpec<'_>] = &[
        ("hydroxylamine", &[("N", 0), ("O", 0)], &[("N", "O", 1)]),
        ("hydrogen peroxide", &[("O", 0), ("O", 0)], &[("O", "O", 1)]),
        (
            "phosphonic acid",
            &[("O", 0), ("O", 0), ("O", 0), ("P", 0)],
            &[("O", "P", 1), ("O", "P", 1), ("O", "P", 2)],
        ),
        (
            "phosphinic acid",
            &[("O", 0), ("O", 0), ("P", 0)],
            &[("O", "P", 1), ("O", "P", 2)],
        ),
        (
            "sulfuric diamide",
            &[("N", 0), ("N", 0), ("O", 0), ("O", 0), ("S", 0)],
            &[("N", "S", 1), ("N", "S", 1), ("O", "S", 2), ("O", "S", 2)],
        ),
        (
            "diphosphoric acid",
            &[
                ("O", 0),
                ("O", 0),
                ("O", 0),
                ("O", 0),
                ("O", 0),
                ("O", 0),
                ("O", 0),
                ("P", 0),
                ("P", 0),
            ],
            &[
                ("O", "P", 1),
                ("O", "P", 1),
                ("O", "P", 1),
                ("O", "P", 1),
                ("O", "P", 1),
                ("O", "P", 1),
                ("O", "P", 2),
                ("O", "P", 2),
            ],
        ),
        (
            "carbon dioxide",
            &[("C", 0), ("O", 0), ("O", 0)],
            &[("C", "O", 2), ("C", "O", 2)],
        ),
        (
            "carbon disulfide",
            &[("C", 0), ("S", 0), ("S", 0)],
            &[("C", "S", 2), ("C", "S", 2)],
        ),
        (
            "carbonyl sulfide",
            &[("C", 0), ("O", 0), ("S", 0)],
            &[("C", "O", 2), ("C", "S", 2)],
        ),
        ("carbon monoxide", &[("C", -1), ("O", 1)], &[("C", "O", 3)]),
        (
            "carbonyl dichloride",
            &[("C", 0), ("O", 0), ("Cl", 0), ("Cl", 0)],
            &[("C", "O", 2), ("C", "Cl", 1), ("C", "Cl", 1)],
        ),
        (
            "carbonyl difluoride",
            &[("C", 0), ("O", 0), ("F", 0), ("F", 0)],
            &[("C", "O", 2), ("C", "F", 1), ("C", "F", 1)],
        ),
        (
            "carbonyl dibromide",
            &[("C", 0), ("O", 0), ("Br", 0), ("Br", 0)],
            &[("C", "O", 2), ("C", "Br", 1), ("C", "Br", 1)],
        ),
        (
            "thiocyanic acid",
            &[("C", 0), ("N", 0), ("S", 0)],
            &[("C", "N", 3), ("C", "S", 1)],
        ),
        (
            "isocyanic acid",
            &[("C", 0), ("N", 0), ("O", 0)],
            &[("C", "N", 2), ("C", "O", 2)],
        ),
        (
            "isothiocyanic acid",
            &[("C", 0), ("N", 0), ("S", 0)],
            &[("C", "N", 2), ("C", "S", 2)],
        ),
        (
            "cyanate",
            &[("C", 0), ("N", 0), ("O", -1)],
            &[("C", "N", 3), ("C", "O", 1)],
        ),
        (
            "cyanate",
            &[("C", 0), ("N", -1), ("O", 0)],
            &[("C", "N", 2), ("C", "O", 2)],
        ),
        (
            "thiocyanate",
            &[("C", 0), ("N", 0), ("S", -1)],
            &[("C", "N", 3), ("C", "S", 1)],
        ),
        (
            "thiocyanate",
            &[("C", 0), ("N", -1), ("S", 0)],
            &[("C", "N", 2), ("C", "S", 2)],
        ),
        ("cyanide", &[("C", -1), ("N", 0)], &[("C", "N", 3)]),
        (
            "azide",
            &[("N", -1), ("N", 1), ("N", -1)],
            &[("N", "N", 2), ("N", "N", 2)],
        ),
        (
            "nitrate",
            &[("N", 1), ("O", 0), ("O", -1), ("O", -1)],
            &[("N", "O", 2), ("N", "O", 1), ("N", "O", 1)],
        ),
        (
            "nitrite",
            &[("N", 0), ("O", 0), ("O", -1)],
            &[("N", "O", 2), ("N", "O", 1)],
        ),
        ("peroxide", &[("O", -1), ("O", -1)], &[("O", "O", 1)]),
    ];
    let signature = small_signature(mol, atoms);
    for &(name, atom_spec, bond_spec) in specs {
        let mut a: Vec<_> = atom_spec.iter().map(|&(s, c)| (s.to_owned(), c)).collect();
        a.sort();
        let mut b: Vec<_> = bond_spec
            .iter()
            .map(|&(s, t, o)| (s.min(t).to_owned(), s.max(t).to_owned(), o))
            .collect();
        b.sort();
        if signature == (a, b) {
            return Some(name.to_owned());
        }
    }
    None
}

fn carbon_ligand_atoms(mol: &Molecule, atoms: &AtomSet, root: usize, central: usize) -> AtomSet {
    let mut found = AtomSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(idx) = queue.pop_front() {
        if found.contains(&idx) {
            continue;
        }
        if !atoms.contains(&idx) || !mol.atoms[&idx].is_carbon() {
            return AtomSet::new();
        }
        found.insert(idx);
        queue.extend(
            mol.get_neighbors(idx)
                .iter()
                .copied()
                .filter(|n| *n != central && atoms.contains(n)),
        );
    }
    found
}

fn carbon_path(
    mol: &Molecule,
    atoms: &AtomSet,
    start: usize,
    end: usize,
) -> Result<Vec<usize>, String> {
    let mut queue = VecDeque::from([(start, vec![start])]);
    let mut visited = AtomSet::new();
    while let Some((idx, path)) = queue.pop_front() {
        // Upstream divergence: shortcut path searches share the request's exhaustive-work budget.
        mol.spend_naming_work(path.len())?;
        if idx == end {
            return Ok(path);
        }
        if !visited.insert(idx) {
            continue;
        }
        let mut neighbors: Vec<_> = mol
            .get_neighbors(idx)
            .iter()
            .copied()
            .filter(|n| atoms.contains(n) && !path.contains(n))
            .collect();
        neighbors.sort();
        for n in neighbors {
            let mut p = path.clone();
            p.push(n);
            queue.push_back((n, p));
        }
    }
    Ok(Vec::new())
}

fn alkyl_ligand_name(
    mol: &Molecule,
    atoms: &AtomSet,
    root: usize,
    central: usize,
) -> Result<String, String> {
    let carbons = carbon_ligand_atoms(mol, atoms, root, central);
    if carbons.is_empty() {
        return Ok(String::new());
    }
    let mut degree_sum = 0;
    for &idx in &carbons {
        for &n in mol.get_neighbors(idx) {
            if atoms.contains(&n) && n != central {
                if order(mol, idx, n) != 1 || !mol.atoms[&n].is_carbon() {
                    return Ok(String::new());
                }
                degree_sum += 1;
            }
        }
    }
    if degree_sum / 2 != carbons.len() - 1 {
        return Ok(String::new());
    }
    // Upstream divergence: the direct alkyl spelling cannot represent ligand stereo or formal charge.
    require_stereo_free_core(mol, &carbons)?;
    if carbons.iter().any(|idx| mol.atoms[idx].charge != 0) {
        return Err("openclatura: shortcut cannot preserve charged alkyl ligand".into());
    }
    systematic_alkyl_name(mol, &carbons, root)
}

fn systematic_alkyl_name(mol: &Molecule, atoms: &AtomSet, root: usize) -> Result<String, String> {
    let _depth = mol.enter_naming()?;
    if atoms.len() == 1 {
        return Ok("methyl".into());
    }
    let atom_list: Vec<_> = atoms.iter().copied().collect();
    let mut choices = Vec::new();
    for (i, &left) in atom_list.iter().enumerate() {
        for &right in &atom_list[i..] {
            let path = carbon_path(mol, atoms, left, right)?;
            if !path.contains(&root) {
                continue;
            }
            for p in [path.clone(), path.into_iter().rev().collect()] {
                mol.spend_naming_work(p.len())?;
                let Some(root_offset) = p.iter().position(|i| *i == root) else {
                    continue;
                };
                let root_loc = root_offset + 1;
                let locs: Vec<_> = p
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        mol.get_neighbors(**a)
                            .iter()
                            .any(|n| atoms.contains(n) && !p.contains(n))
                    })
                    .map(|(i, _)| i + 1)
                    .collect();
                choices.push((std::cmp::Reverse(p.len()), root_loc, locs, p));
            }
        }
    }
    let Some((_, root_loc, _, path)) = choices.into_iter().min() else {
        return Ok(String::new());
    };
    let path_set: AtomSet = path.iter().copied().collect();
    let mut locants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (loc, &idx) in path.iter().enumerate() {
        for &n in mol.get_neighbors(idx) {
            if !atoms.contains(&n) || path_set.contains(&n) {
                continue;
            }
            let branch = carbon_ligand_atoms(mol, atoms, n, idx);
            let name = systematic_alkyl_name(mol, &branch, n)?;
            if name.is_empty() {
                return Ok(String::new());
            }
            locants.entry(name).or_default().push(loc + 1);
        }
    }
    let mut prefix = String::new();
    for (name, locs) in locants {
        prefix.push_str(&format!(
            "{}-{}",
            locs.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(","),
            format_multiplier(&name, locs.len(), false)
        ));
    }
    let stem = stems::stem_for(path.len());
    let parent = if root_loc == 1 {
        format!("{stem}yl")
    } else {
        format!("{stem}an-{root_loc}-yl")
    };
    Ok(format!("{prefix}{parent}"))
}

fn ligand_name(
    mol: &Molecule,
    atoms: &AtomSet,
    root: usize,
    central: usize,
    blocked: &AtomSet,
) -> Result<String, String> {
    let alkyl = alkyl_ligand_name(mol, atoms, root, central)?;
    if !alkyl.is_empty() {
        return Ok(alkyl);
    }
    Ok(strip_outer_parentheses(&branch_name(
        mol, atoms, root, blocked, central,
    )?))
}

#[derive(Clone, Debug)]
struct AzineSide {
    carbon: usize,
    nitrogen: usize,
    atoms: AtomSet,
}

/// The graph roles from `nitrogen_roles.azine_roles`: the two carbon sides
/// must partition the component after the C=N-N=C nitrogens are blocked.
fn azine_roles(mol: &Molecule, atoms: &AtomSet) -> Vec<[AzineSide; 2]> {
    let mut roles = Vec::new();
    for &n1 in atoms {
        if mol.atoms[&n1].symbol != "N" {
            continue;
        }
        for &n2 in mol.get_neighbors(n1) {
            if n2 <= n1
                || !atoms.contains(&n2)
                || mol.atoms[&n2].symbol != "N"
                || order(mol, n1, n2) != 1
            {
                continue;
            }
            let double_carbon = |nitrogen, blocked| {
                let candidates: Vec<_> = mol
                    .get_neighbors(nitrogen)
                    .iter()
                    .copied()
                    .filter(|n| {
                        *n != blocked && mol.atoms[n].is_carbon() && order(mol, nitrogen, *n) == 2
                    })
                    .collect();
                (candidates.len() == 1).then(|| candidates[0])
            };
            let (Some(c1), Some(c2)) = (double_carbon(n1, n2), double_carbon(n2, n1)) else {
                continue;
            };
            let core = set([n1, n2]);
            let side1 = component_atoms_until_blocked(mol, atoms, c1, &core);
            let side2 = component_atoms_until_blocked(mol, atoms, c2, &core);
            if side1.is_empty() || side2.is_empty() || !side1.is_disjoint(&side2) {
                continue;
            }
            if side1
                .iter()
                .chain(&side2)
                .chain(&core)
                .copied()
                .collect::<AtomSet>()
                != *atoms
            {
                continue;
            }
            roles.push([
                AzineSide {
                    carbon: c1,
                    nitrogen: n1,
                    atoms: side1,
                },
                AzineSide {
                    carbon: c2,
                    nitrogen: n2,
                    atoms: side2,
                },
            ]);
        }
    }
    roles
}

fn is_amidino_carbon(mol: &Molecule, carbon: usize, nitrogen: usize) -> bool {
    let others: Vec<_> = mol
        .get_neighbors(carbon)
        .iter()
        .copied()
        .filter(|n| *n != nitrogen)
        .collect();
    others.len() == 2
        && others
            .iter()
            .all(|n| mol.atoms[n].symbol == "N" && order(mol, carbon, *n) == 1)
}

fn hydrazone_stereo_prefix(mol: &Molecule, carbon: usize, nitrogen: usize, parent: &str) -> String {
    let Some(stereo) = mol
        .get_bond(carbon, nitrogen)
        .and_then(|b| b.stereo.as_deref())
        .filter(|stereo| matches!(*stereo, "E" | "Z"))
    else {
        return String::new();
    };
    let locant = parent
        .strip_suffix("-one")
        .and_then(|s| s.rsplit_once("an-"))
        .map(|(_, locant)| locant)
        .filter(|locant| !locant.is_empty() && locant.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("1");
    format!("({locant}{stereo})-")
}

/// Port of the ordered `simple_azine_parent_name` shortcut. In particular,
/// substituted amidino sides use their recursively named ylidenes before an
/// anhydride elsewhere in a ligand can choose the whole-component citation.
fn simple_azine_parent_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    for sides in azine_roles(mol, atoms) {
        require_stereo_free_core(mol, &set([sides[0].nitrogen, sides[1].nitrogen]))?;
        let parents = [
            carbonyl_equivalent_side_name(mol, &sides[0], false)?,
            carbonyl_equivalent_side_name(mol, &sides[1], false)?,
        ];
        let ylidenes = [
            carbonyl_equivalent_side_name(mol, &sides[0], true)?,
            carbonyl_equivalent_side_name(mol, &sides[1], true)?,
        ];
        if parents.iter().chain(&ylidenes).any(String::is_empty) {
            for parent_side in 0..2 {
                if parents[parent_side].is_empty() {
                    continue;
                }
                let other = &sides[1 - parent_side];
                let ylidene = strip_outer_parentheses(&branch_name(
                    mol,
                    atoms,
                    other.carbon,
                    &set([other.nitrogen]),
                    other.nitrogen,
                )?);
                if !ylidene.is_empty() && ylidene.ends_with("ylidene") {
                    let side = &sides[parent_side];
                    let stereo = hydrazone_stereo_prefix(
                        mol,
                        side.carbon,
                        side.nitrogen,
                        &parents[parent_side],
                    );
                    return Ok(Some(format!(
                        "{stereo}{} {ylidene}hydrazone",
                        parents[parent_side]
                    )));
                }
            }
            let amidino = sides
                .each_ref()
                .map(|side| is_amidino_carbon(mol, side.carbon, side.nitrogen));
            let aldehyde = sides.each_ref().map(|side| {
                mol.get_neighbors(side.carbon)
                    .iter()
                    .filter(|n| **n != side.nitrogen && mol.atoms[n].symbol != "H")
                    .count()
                    <= 1
            });
            let substituted_amidino = sides.iter().zip(amidino).any(|(side, amidino)| {
                amidino
                    && mol.get_neighbors(side.carbon).iter().any(|ligand| {
                        mol.atoms[ligand].symbol == "N"
                            && *ligand != side.nitrogen
                            && mol.get_neighbors(*ligand).iter().any(|other| {
                                *other != side.carbon && mol.atoms[other].symbol != "H"
                            })
                    })
            });
            if amidino.into_iter().filter(|yes| *yes).count() == 1
                && (substituted_amidino
                    || !aldehyde
                        .into_iter()
                        .zip(amidino)
                        .any(|(aldehyde, amidino)| aldehyde && !amidino))
            {
                let mut names = Vec::new();
                for side in &sides {
                    names.push(strip_outer_parentheses(&branch_name(
                        mol,
                        atoms,
                        side.carbon,
                        &set([side.nitrogen]),
                        side.nitrogen,
                    )?));
                }
                if names
                    .iter()
                    .all(|name| !name.is_empty() && name.ends_with("ylidene"))
                {
                    names.sort_by_key(|name| substituent_sort_key(name));
                    return Ok(Some(format!("1-({})-2-({})hydrazine", names[0], names[1])));
                }
            }
            continue;
        }
        let ranks = canonical_ranks(mol, None);
        let left_bond = mol.get_bond(sides[0].carbon, sides[0].nitrogen);
        let right_bond = mol.get_bond(sides[1].carbon, sides[1].nitrogen);
        if ranks[&sides[0].carbon] == ranks[&sides[1].carbon]
            && ranks[&sides[0].nitrogen] == ranks[&sides[1].nitrogen]
            && let (Some(left), Some(right)) = (left_bond, right_bond)
            && left.stereo == right.stereo
        {
            return Ok(Some(
                if let Some(stereo) = left.stereo.as_deref().filter(|s| matches!(*s, "E" | "Z")) {
                    format!("1,2-bis[({stereo})-{}]hydrazine", ylidenes[0])
                } else {
                    format!("1,2-bis({})hydrazine", ylidenes[0])
                },
            ));
        }
        let parent_side =
            usize::from((sides[1].atoms.len(), &parents[1]) > (sides[0].atoms.len(), &parents[0]));
        let side = &sides[parent_side];
        // Upstream divergence: an asymmetric hydrazone citation does not render the other C=N descriptor.
        if mol
            .get_bond(
                sides[1 - parent_side].carbon,
                sides[1 - parent_side].nitrogen,
            )
            .is_some_and(|bond| bond.stereo.is_some())
        {
            return Err("openclatura: shortcut cannot preserve configured stereochemistry".into());
        }
        let stereo =
            hydrazone_stereo_prefix(mol, side.carbon, side.nitrogen, &parents[parent_side]);
        return Ok(Some(format!(
            "{stereo}{} {}hydrazone",
            parents[parent_side],
            ylidenes[1 - parent_side]
        )));
    }
    Ok(None)
}

fn ordered_simple_ring(mol: &Molecule, atoms: &AtomSet) -> Vec<usize> {
    if atoms.len() < 3
        || atoms.iter().any(|i| {
            mol.get_neighbors(*i)
                .iter()
                .filter(|n| atoms.contains(n))
                .count()
                != 2
        })
    {
        return Vec::new();
    }
    let Some(&start) = atoms.first() else {
        return Vec::new();
    };
    let mut path = vec![start];
    let mut previous = None;
    loop {
        let current = *path.last().unwrap_or(&start);
        let next = mol
            .get_neighbors(current)
            .iter()
            .copied()
            .filter(|n| atoms.contains(n) && Some(*n) != previous)
            .min();
        let Some(next) = next else {
            return Vec::new();
        };
        if next == start {
            return if path.len() == atoms.len() {
                path
            } else {
                Vec::new()
            };
        }
        if path.contains(&next) {
            return Vec::new();
        }
        previous = Some(current);
        path.push(next);
    }
}

/// Retained biphenyl requires two complete benzene rings joined by one bridge.
fn biphenyl_parent_result(mol: &Molecule, atoms: &AtomSet) -> Option<String> {
    // Upstream divergence: the fixed name also requires neutral ring carbons.
    if atoms.len() != 12
        || atoms.iter().any(|idx| {
            let atom = &mol.atoms[idx];
            !atom.is_carbon() || atom.charge != 0 || atom.radical_electrons != 0
        })
    {
        return None;
    }
    let bridges: Vec<_> = mol
        .bonds
        .values()
        .filter(|bond| atoms.contains(&bond.u) && atoms.contains(&bond.v) && !bond.in_small_ring)
        .collect();
    if bridges.len() != 1 {
        return None;
    }
    let bridge = bridges[0];
    for (root, blocked) in [(bridge.u, bridge.v), (bridge.v, bridge.u)] {
        let ring = component_atoms_until_blocked(mol, atoms, root, &set([blocked]));
        if ring.len() != 6 {
            return None;
        }
        let path = ordered_simple_ring(mol, &ring);
        if retained::get_retained_ring(mol, &path)?.name != "benzene" {
            return None;
        }
    }
    Some("1,1'-biphenyl".into())
}

fn simple_cycle_unsaturation_locants(mol: &Molecule, path: &[usize]) -> Vec<usize> {
    path.iter()
        .enumerate()
        .filter_map(|(i, atom)| {
            (order(mol, *atom, path[(i + 1) % path.len()]) == 2).then_some(i + 1)
        })
        .collect()
}

fn simple_replacement_cycle_name(mol: &Molecule, path: &[usize]) -> String {
    let mut hetero = BTreeMap::<(usize, String), Vec<usize>>::new();
    for (offset, atom) in path.iter().enumerate() {
        if mol.atoms[atom].is_carbon() {
            continue;
        }
        let Some(element) = elements::get(&mol.atoms[atom].symbol) else {
            return String::new();
        };
        let Some(prefix) = element.hw_stem.as_ref().filter(|s| !s.is_empty()) else {
            return String::new();
        };
        hetero
            .entry((element.hw_priority.unwrap_or(999), prefix.clone()))
            .or_default()
            .push(offset + 1);
    }
    let mut prefix = String::new();
    for ((_, name), locants) in hetero {
        let multiplier = if locants.len() == 1 {
            String::new()
        } else {
            multipliers::basic(locants.len())
        };
        prefix.push_str(&format!(
            "{}-{multiplier}{name}",
            locants
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    let stem = stems::stem_for(path.len());
    let locants = simple_cycle_unsaturation_locants(mol, path);
    if locants.is_empty() {
        return format!("{prefix}cyclo{stem}an");
    }
    let multiplier = if locants.len() == 1 {
        String::new()
    } else {
        multipliers::basic(locants.len())
    };
    format!(
        "{prefix}cyclo{stem}a-{}-{multiplier}en",
        locants
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn retained_locant_sort_key(locant: &str) -> (usize, &str) {
    let digits: String = locant.chars().take_while(char::is_ascii_digit).collect();
    (digits.parse().unwrap_or(999), locant)
}

fn carbaldehyde_side_ring(mol: &Molecule, side: &AzineSide) -> Option<(AtomSet, usize)> {
    if !side.atoms.contains(&side.carbon) || !mol.atoms[&side.carbon].is_carbon() {
        return None;
    }
    let ring: AtomSet = side
        .atoms
        .difference(&set([side.carbon]))
        .copied()
        .collect();
    if ring.len() < 3 {
        return None;
    }
    let neighbors: Vec<_> = mol
        .get_neighbors(side.carbon)
        .iter()
        .copied()
        .filter(|n| ring.contains(n) && order(mol, side.carbon, *n) == 1)
        .collect();
    (neighbors.len() == 1).then(|| (ring, neighbors[0]))
}

fn carbonyl_equivalent_side_name(
    mol: &Molecule,
    side: &AzineSide,
    as_ylidene: bool,
) -> Result<String, String> {
    let name = carbonyl_equivalent_side_name_unchecked(mol, side, as_ylidene)?;
    if !name.is_empty() {
        require_stereo_free_core(mol, &side.atoms)?;
        // Upstream divergence: carbonyl-equivalent side citations do not render formal charges.
        if side.atoms.iter().any(|idx| mol.atoms[idx].charge != 0) {
            return Err("openclatura: shortcut cannot preserve charged carbonyl side".into());
        }
    }
    Ok(name)
}

fn carbonyl_equivalent_side_name_unchecked(
    mol: &Molecule,
    side: &AzineSide,
    as_ylidene: bool,
) -> Result<String, String> {
    if let Some((ring, attachment)) = carbaldehyde_side_ring(mol, side) {
        let path = ordered_simple_ring(mol, &ring);
        if !path.is_empty() {
            let homocyclic = path
                .iter()
                .all(|i| mol.atoms[i].symbol == mol.atoms[&path[0]].symbol);
            if let Some(parent) = retained::get_retained_ring(mol, &path) {
                let mut locant = parent
                    .locant_maps
                    .iter()
                    .filter_map(|map| map.get(&attachment))
                    .min_by_key(|s| retained_locant_sort_key(s))
                    .cloned()
                    .unwrap_or_else(|| {
                        if homocyclic {
                            "1".into()
                        } else {
                            String::new()
                        }
                    });
                if locant.is_empty() {
                    let name = strip_outer_parentheses(&branch_name(
                        mol,
                        &ring,
                        attachment,
                        &AtomSet::new(),
                        side.carbon,
                    )?);
                    if let Some((_, candidate)) =
                        name.strip_suffix("-yl").and_then(|s| s.rsplit_once('-'))
                        && candidate.as_bytes().first().is_some_and(u8::is_ascii_digit)
                        && {
                            let tail = candidate.trim_start_matches(|c: char| c.is_ascii_digit());
                            tail.is_empty()
                                || (tail.len() == 1 && tail.as_bytes()[0].is_ascii_lowercase())
                        }
                    {
                        locant = candidate.to_owned();
                    }
                }
                if !locant.is_empty() {
                    return Ok(if parent.name == "benzene" {
                        if as_ylidene {
                            "benzylidene"
                        } else {
                            "benzaldehyde"
                        }
                        .into()
                    } else if as_ylidene {
                        let stem = RULES
                            .retained
                            .substituent_stems
                            .get(&parent.name)
                            .map(|(stem, _)| stem.as_str())
                            .filter(|stem| !stem.is_empty())
                            .unwrap_or_else(|| parent.name.trim_end_matches('e'));
                        format!("{stem}-{locant}-ylmethylidene")
                    } else {
                        format!("{}-{locant}-carbaldehyde", parent.name)
                    });
                }
            }
            // A heterocycle uses its retained/Hantzsch-Widman citation rather
            // than numbering replacement prefixes from the attachment.
            if homocyclic {
                let offset = path.iter().position(|i| *i == attachment).unwrap_or(0);
                let forward: Vec<_> = path[offset..]
                    .iter()
                    .chain(&path[..offset])
                    .copied()
                    .collect();
                let reverse: Vec<_> = std::iter::once(forward[0])
                    .chain(forward[1..].iter().copied().rev())
                    .collect();
                let oriented = [forward, reverse]
                    .into_iter()
                    .min_by_key(|path| (simple_cycle_unsaturation_locants(mol, path), path.clone()))
                    .unwrap_or_default();
                let name = simple_replacement_cycle_name(mol, &oriented);
                if !name.is_empty() {
                    return Ok(if as_ylidene {
                        format!("{name}-1-ylmethylidene")
                    } else {
                        format!("{name}-1-carbaldehyde")
                    });
                }
            }
        }
    }
    if as_ylidene {
        let path = ordered_simple_ring(mol, &side.atoms);
        if !path.is_empty()
            && path.iter().all(|i| {
                mol.atoms[i].is_carbon()
                    || elements::get(&mol.atoms[i].symbol).is_some_and(|e| e.hw_stem.is_some())
            })
        {
            type RingChoice = ((Vec<usize>, usize, Vec<usize>), String);
            let mut choices: Vec<RingChoice> = Vec::new();
            let priority = path
                .iter()
                .filter(|i| !mol.atoms[i].is_carbon())
                .filter_map(|i| elements::get(&mol.atoms[i].symbol)?.hw_priority)
                .min();
            for offset in 0..path.len() {
                let forward: Vec<_> = path[offset..]
                    .iter()
                    .chain(&path[..offset])
                    .copied()
                    .collect();
                let reverse: Vec<_> = std::iter::once(forward[0])
                    .chain(forward[1..].iter().copied().rev())
                    .collect();
                for oriented in [forward, reverse] {
                    if let Some(priority) = priority
                        && (mol.atoms[&oriented[0]].is_carbon()
                            || elements::get(&mol.atoms[&oriented[0]].symbol)
                                .and_then(|e| e.hw_priority)
                                != Some(priority))
                    {
                        continue;
                    }
                    let locant = oriented.iter().position(|i| *i == side.carbon).unwrap_or(0) + 1;
                    let hetero = oriented
                        .iter()
                        .enumerate()
                        .filter_map(|(i, atom)| (!mol.atoms[atom].is_carbon()).then_some(i + 1))
                        .collect();
                    let name = simple_replacement_cycle_name(mol, &oriented);
                    if !name.is_empty() {
                        choices.push((
                            (hetero, locant, oriented),
                            format!("{name}-{locant}-ylidene"),
                        ));
                    }
                }
            }
            if let Some((_, name)) = choices.into_iter().min() {
                return Ok(name);
            }
        }
    }
    simple_carbonyl_side_name(mol, side, as_ylidene)
}

fn simple_carbonyl_side_name(
    mol: &Molecule,
    side: &AzineSide,
    as_ylidene: bool,
) -> Result<String, String> {
    if side.atoms.iter().any(|i| !mol.atoms[i].is_carbon()) {
        return Ok(String::new());
    }
    let edges: Vec<_> = side
        .atoms
        .iter()
        .flat_map(|a| {
            mol.get_neighbors(*a)
                .iter()
                .filter(move |b| side.atoms.contains(b) && a < b)
                .map(move |b| (*a, *b))
        })
        .collect();
    if edges.len() != side.atoms.len() - 1
        || edges
            .iter()
            .any(|(a, b)| !matches!(order(mol, *a, *b), 1..=3))
    {
        return Ok(String::new());
    }
    let ends: Vec<_> = side
        .atoms
        .iter()
        .copied()
        .filter(|i| {
            mol.get_neighbors(*i)
                .iter()
                .filter(|n| side.atoms.contains(n))
                .count()
                <= 1
        })
        .collect();
    let mut path = Vec::new();
    for &start in &ends {
        for &end in &ends {
            let candidate = carbon_path(mol, &side.atoms, start, end)?;
            if candidate.contains(&side.carbon) && candidate.len() > path.len() {
                path = candidate;
            }
        }
    }
    if path.len() != side.atoms.len() {
        return Ok(String::new());
    }
    // Upstream takes the first endpoint from a Python set, then retains that
    // path orientation for unsaturation while minimizing the carbonyl locant.
    // Only name sides whose spelling is independent of that incidental order.
    let bond_orders: Vec<_> = path
        .windows(2)
        .map(|pair| order(mol, pair[0], pair[1]))
        .collect();
    if !bond_orders.iter().eq(bond_orders.iter().rev()) {
        return Err("openclatura: azine unsaturation depends on upstream set ordering".into());
    }
    let locant = path.iter().position(|i| *i == side.carbon).unwrap_or(0) + 1;
    let locant = locant.min(path.len() - locant + 1);
    let stem = stems::stem_for(path.len());
    let mut parts = Vec::new();
    for (bond_order, ending) in [(2, "ene"), (3, "yne")] {
        let locants: Vec<_> = path
            .windows(2)
            .enumerate()
            .filter_map(|(i, pair)| (order(mol, pair[0], pair[1]) == bond_order).then_some(i + 1))
            .collect();
        if !locants.is_empty() {
            let multiplier = if locants.len() == 1 {
                String::new()
            } else {
                multipliers::basic(locants.len())
            };
            parts.push(format!(
                "{}-{multiplier}{ending}",
                locants
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
    }
    let unsaturation = if parts.is_empty() {
        "an".into()
    } else {
        format!("-{}", parts.join("-"))
    };
    Ok(if as_ylidene {
        if path.len() == 1 {
            "methylidene".into()
        } else {
            format!("{stem}{unsaturation}-{locant}-ylidene")
        }
    } else if path.len() == 1 {
        "formaldehyde".into()
    } else if locant == 1 {
        format!("{stem}{unsaturation}al")
    } else {
        format!("{stem}{unsaturation}-{locant}-one")
    })
}

fn sulfoxide_parent_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let sulfurs: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].symbol == "S")
        .collect();
    if sulfurs.len() != 1 {
        return Ok(None);
    }
    let central = sulfurs[0];
    let mut oxygens = Vec::new();
    let mut roots = Vec::new();
    for &n in mol.get_neighbors(central) {
        if !atoms.contains(&n) {
            return Ok(None);
        }
        if mol.atoms[&n].symbol == "O"
            && (order(mol, central, n) == 2 || mol.atoms[&n].charge == -1)
        {
            oxygens.push(n);
        } else if mol.atoms[&n].is_carbon() && order(mol, central, n) == 1 {
            roots.push(n);
        } else {
            return Ok(None);
        }
    }
    if oxygens.len() != 1 || roots.len() != 2 {
        return Ok(None);
    }
    let left = carbon_ligand_atoms(mol, atoms, roots[0], central);
    let right = carbon_ligand_atoms(mol, atoms, roots[1], central);
    if left.is_empty() || right.is_empty() || !left.is_disjoint(&right) {
        return Ok(None);
    }
    let ligand_results: Vec<_> = roots
        .iter()
        .map(|r| alkyl_ligand_name(mol, atoms, *r, central))
        .collect();
    if ligand_results
        .iter()
        .any(|result| result.as_ref().is_ok_and(String::is_empty))
    {
        return Ok(None);
    }
    let mut ligands: Vec<_> = ligand_results.into_iter().collect::<Result<_, _>>()?;
    // Upstream divergence: neutral sulfoxide class names require a neutral or charge-separated S=O group.
    let oxygen = oxygens[0];
    if !matches!(
        (
            mol.atoms[&central].charge,
            mol.atoms[&oxygen].charge,
            order(mol, central, oxygen)
        ),
        (0, 0, 2) | (1, -1, 1)
    ) {
        return Err("openclatura: shortcut cannot preserve sulfoxide formal charges".into());
    }
    require_stereo_free_core(mol, &oxygens.into_iter().collect())?;
    // Upstream divergence: an unassigned sulfoxide raw tag is not a rendered absolute descriptor.
    if mol.atoms[&central].raw_stereo.is_some() {
        return Err("openclatura: shortcut cannot preserve configured stereochemistry".into());
    }
    ligands.sort();
    let stereo = mol.atoms[&central]
        .stereo
        .as_ref()
        .map(|s| format!("({s})-"))
        .unwrap_or_default();
    Ok(Some(if ligands[0] == ligands[1] {
        format!("{stereo}di{} sulfoxide", ligands[0])
    } else {
        format!("{stereo}{} sulfoxide", ligands.join(" "))
    }))
}

fn has_principal_group(mol: &Molecule, atoms: &AtomSet) -> bool {
    perceive_groups(mol)
        .iter()
        .any(|g| g.is_principal_candidate && atoms.contains(&g.attachment_carbon))
}
fn competing_principal_group(mol: &Molecule, atoms: &AtomSet, central: usize) -> bool {
    perceive_groups(mol).iter().any(|g| {
        g.is_principal_candidate
            && atoms.contains(&g.attachment_carbon)
            && !g.atoms_involved.contains(&central)
            && g.attachment_carbon != central
    })
}

fn terminal_ligand_name(mol: &Molecule, idx: usize) -> String {
    // Upstream divergence: neutral terminal prefixes do not represent formal charge or atom stereo.
    if mol.atoms[&idx].charge != 0 || stereo_configured(mol, idx) {
        return String::new();
    }
    match mol.atoms[&idx].symbol.as_str() {
        "F" => "fluoro".into(),
        "Cl" => "chloro".into(),
        "Br" => "bromo".into(),
        "I" => "iodo".into(),
        "C" if mol.degree(idx) == 1 => "methyl".into(),
        _ => String::new(),
    }
}

fn central_ligand_name(
    mol: &Molecule,
    atoms: &AtomSet,
    central: usize,
    neighbor: usize,
) -> Result<String, String> {
    if order(mol, central, neighbor) != 1 {
        return Ok(String::new());
    }
    let name = terminal_ligand_name(mol, neighbor);
    if !name.is_empty() {
        return Ok(name);
    }
    if mol.atoms[&neighbor].symbol != "O" || mol.atoms[&neighbor].charge != 0 {
        return Ok(String::new());
    }
    let roots: Vec<_> = mol
        .get_neighbors(neighbor)
        .iter()
        .copied()
        .filter(|n| atoms.contains(n) && *n != central)
        .collect();
    if roots.len() != 1 {
        return Ok(String::new());
    }
    let alkyl = alkyl_ligand_name(mol, atoms, roots[0], neighbor)?;
    if alkyl.is_empty() {
        Ok(String::new())
    } else {
        Ok(oxy_prefix_from_branch(&alkyl))
    }
}

fn simple_central_parent_hydride_name(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<String>, String> {
    let centers: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| {
            RULES
                .components
                .mononuclear_parent_hydrides
                .contains_key(&mol.atoms[i].symbol)
                && !matches!(mol.atoms[i].symbol.as_str(), "O" | "F" | "Cl" | "Br" | "I")
                && mol.degree(*i) >= 2
        })
        .collect();
    if centers.len() != 1 {
        return Ok(None);
    }
    let c = centers[0];
    let a = &mol.atoms[&c];
    if stereo_configured(mol, c) {
        return Ok(None);
    }
    let senior = matches!(
        a.symbol.as_str(),
        "P" | "As" | "Sb" | "Bi" | "Si" | "Ge" | "Sn" | "Pb" | "B" | "Al" | "Ga" | "In" | "Tl"
    );
    let oxide = a.charge == 1
        && matches!(a.symbol.as_str(), "P" | "As" | "Sb")
        && mol.get_neighbors(c).iter().any(|n| {
            mol.atoms[n].symbol == "O" && mol.atoms[n].charge == -1 && mol.degree(*n) == 1
        });
    let onium = a.charge == 1
        && senior
        && !oxide
        && a.total_h_count == 0
        && mol
            .get_neighbors(c)
            .iter()
            .all(|n| mol.atoms[n].is_carbon());
    if a.charge != 0 && !oxide && !onium {
        return Ok(None);
    }
    let hydrocarbyl = senior && !has_principal_group(mol, atoms);
    let mut names = Vec::new();
    let mut oxide_oxygen = None;
    for &n in mol.get_neighbors(c) {
        if !atoms.contains(&n) {
            continue;
        }
        if hydrocarbyl
            && oxide_oxygen.is_none()
            && matches!(a.symbol.as_str(), "P" | "As" | "Sb")
            && a.total_h_count == 0
            && mol.atoms[&n].symbol == "O"
            && mol.degree(n) == 1
            && (order(mol, c, n) == 2 || (order(mol, c, n) == 1 && mol.atoms[&n].charge == -1))
        {
            oxide_oxygen = Some(n);
            continue;
        }
        let mut name = central_ligand_name(mol, atoms, c, n)?;
        if name.is_empty() && hydrocarbyl && mol.atoms[&n].is_carbon() && order(mol, c, n) == 1 {
            let ligand = component_atoms_until_blocked(mol, atoms, n, &set([c]));
            if !ligand.is_empty()
                && !ligand.iter().any(|i| mol.atoms[i].charge != 0)
                && !mol
                    .get_neighbors(c)
                    .iter()
                    .any(|other| *other != n && ligand.contains(other))
            {
                name = branch_name(mol, atoms, n, &set([c]), c)?;
            }
        }
        if name.is_empty() {
            return Ok(None);
        }
        names.push(name);
    }
    if names.is_empty() {
        return Ok(None);
    }
    let prefix = center_ligands(&names);
    let valence: usize = mol
        .get_neighbors(c)
        .iter()
        .map(|n| order(mol, c, *n))
        .sum::<usize>()
        + a.total_h_count.max(a.explicit_h_count);
    let lambda = if oxide_oxygen.is_some()
        || onium
        || elements::get(&a.symbol).is_some_and(|e| e.standard_valence == valence)
    {
        String::new()
    } else {
        format!("lambda{valence}-")
    };
    let mut parent = format!(
        "{lambda}{}",
        RULES.components.mononuclear_parent_hydrides[&a.symbol]
    );
    if oxide_oxygen.is_some() {
        parent.push_str(" oxide");
    } else if onium {
        if parent.ends_with('e') {
            parent.pop();
        }
        parent.push_str("ium");
    }
    Ok(Some(if !prefix.is_empty() && !lambda.is_empty() {
        format!("{prefix}-{parent}")
    } else {
        format!("{prefix}{parent}")
    }))
}

#[derive(Clone, Debug)]
struct OxoLigand {
    oxygen: usize,
    role: &'static str,
    attachment: Option<usize>,
}
#[derive(Clone, Debug)]
struct OxoRole {
    central: usize,
    ligands: Vec<OxoLigand>,
}
impl OxoRole {
    fn count(&self, role: &str) -> usize {
        self.ligands.iter().filter(|l| l.role == role).count()
    }
    fn core(&self) -> AtomSet {
        std::iter::once(self.central)
            .chain(self.ligands.iter().map(|l| l.oxygen))
            .collect()
    }
}
fn oxo_roles(mol: &Molecule, atoms: &AtomSet) -> Vec<OxoRole> {
    let mut roles = Vec::new();
    for &central in atoms {
        if mol.get_neighbors(central).is_empty()
            || mol
                .get_neighbors(central)
                .iter()
                .any(|n| !atoms.contains(n) || mol.atoms[n].symbol != "O")
        {
            continue;
        }
        let mut ligands = Vec::new();
        let mut valid = true;
        for &oxygen in mol.get_neighbors(central) {
            let normalized = matches!(mol.atoms[&central].symbol.as_str(), "Cl" | "Br" | "I")
                && order(mol, central, oxygen) == 1
                && mol.atoms[&oxygen].charge < 0
                && mol.degree(oxygen) == 1;
            let (role, attachment) = if order(mol, central, oxygen) == 2 || normalized {
                ("oxo", None)
            } else {
                let neighbors: Vec<_> = mol
                    .get_neighbors(oxygen)
                    .iter()
                    .copied()
                    .filter(|n| *n != central)
                    .collect();
                if neighbors.is_empty() {
                    (
                        if mol.atoms[&oxygen].charge < 0 {
                            "oxido"
                        } else {
                            "hydroxy"
                        },
                        None,
                    )
                } else if neighbors.len() == 1 && atoms.contains(&neighbors[0]) {
                    match mol.atoms[&neighbors[0]].symbol.as_str() {
                        "C" => ("alkoxy", Some(neighbors[0])),
                        "O" => ("peroxy", Some(neighbors[0])),
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                } else {
                    valid = false;
                    break;
                }
            };
            ligands.push(OxoLigand {
                oxygen,
                role,
                attachment,
            });
        }
        if valid {
            roles.push(OxoRole { central, ligands });
        }
    }
    roles
}
fn matching_oxo_spec<'a>(mol: &Molecule, role: &OxoRole) -> Option<&'a serde_json::Value>
where
    'static: 'a,
{
    let atom = &mol.atoms[&role.central];
    if atom.total_h_count != 0 {
        return None;
    }
    let single = role.ligands.iter().filter(|l| l.role != "oxo").count();
    let double = role.count("oxo");
    RULES
        .components
        .replacement_parent_oxoacid_specs
        .iter()
        .find(|spec| {
            spec["central"].as_str() == Some(atom.symbol.as_str())
                && spec["single_o"].as_u64() == Some(single as u64)
                && spec["double_o"].as_u64() == Some(double as u64)
                && spec
                    .get("charge")
                    .is_none_or(|c| c.as_i64() == Some(i64::from(atom.charge)))
        })
}
fn oxoacid_parent_name(mol: &Molecule, atoms: &AtomSet) -> Option<String> {
    let roles = oxo_roles(mol, atoms);
    if roles.len() != 1 {
        return None;
    }
    let r = &roles[0];
    if r.count("alkoxy") > 0 || r.count("peroxy") > 0 || r.core() != *atoms {
        return None;
    }
    let spec = matching_oxo_spec(mol, r)?;
    let field = if r.count("oxido") > 0
        && r.count("hydroxy") == 0
        && spec["ester_suffix"].as_str().is_some_and(|s| !s.is_empty())
    {
        "ester_suffix"
    } else {
        "name"
    };
    spec[field].as_str().map(str::to_owned)
}
fn modifier_phrase(names: &[String], safe: bool) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for name in names {
        *counts.entry(name.clone()).or_default() += 1;
    }
    let mut counted: Vec<_> = counts.into_iter().collect();
    counted.sort_by_key(|(name, _)| substituent_sort_key(name));
    counted
        .into_iter()
        .map(|(name, count)| {
            if count == 1 {
                name
            } else {
                format_multiplier(&name, count, safe)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn ester_modifier_name(
    mol: &Molecule,
    atoms: &AtomSet,
    root: usize,
    oxygen: usize,
    acid: &AtomSet,
) -> Result<String, String> {
    let name = branch_name(mol, atoms, root, acid, oxygen)?;
    if !name.is_empty() {
        return Ok(strip_outer_parentheses(&name));
    }
    alkyl_ligand_name(mol, atoms, root, oxygen)
}
fn oxoacid_ester_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let mut matches = Vec::new();
    for role in oxo_roles(mol, atoms) {
        if stereo_configured(mol, role.central) {
            continue;
        }
        let core = role.core();
        let central = &mol.atoms[&role.central];
        if role.count("peroxy") > 0 {
            // Verified charge-normalized halogen peroxy ester templates.
            if role.count("peroxy") != 1
                || role.count("oxo") != 2
                || role.ligands.len() != 3
                || central.charge != 2
            {
                continue;
            }
            let suffix = match central.symbol.as_str() {
                "Cl" => "peroxychlorate",
                "Br" => "peroxybromate",
                "I" => "peroxyiodate",
                _ => continue,
            };
            let Some(peroxy) = role.ligands.iter().find(|l| l.role == "peroxy") else {
                continue;
            };
            let Some(distal) = peroxy.attachment else {
                continue;
            };
            let roots: Vec<_> = mol
                .get_neighbors(distal)
                .iter()
                .copied()
                .filter(|n| atoms.contains(n) && *n != role.central && *n != peroxy.oxygen)
                .collect();
            if roots.len() != 1 {
                continue;
            }
            let mut acid = core.clone();
            acid.insert(distal);
            let modifier_atoms = component_atoms_until_blocked(mol, atoms, roots[0], &acid);
            if modifier_atoms.is_empty()
                || modifier_atoms.union(&acid).copied().collect::<AtomSet>() != *atoms
            {
                continue;
            }
            let modifier = ester_modifier_name(mol, atoms, roots[0], distal, &acid)?;
            if !modifier.is_empty() {
                require_stereo_free_core(mol, &acid)?;
                matches.push(format!("{modifier} {suffix}"));
            }
            continue;
        }
        let Some(spec) = matching_oxo_spec(mol, &role) else {
            continue;
        };
        let Some(base) = spec["ester_suffix"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let esters: Vec<_> = role.ligands.iter().filter(|l| l.role == "alkoxy").collect();
        if esters.is_empty() {
            continue;
        }
        let mut covered = AtomSet::new();
        let mut names = Vec::new();
        let mut valid = true;
        for ligand in esters {
            let Some(root) = ligand.attachment else {
                valid = false;
                break;
            };
            let ligand_atoms = component_atoms_until_blocked(mol, atoms, root, &core);
            if ligand_atoms.is_empty() || !ligand_atoms.is_disjoint(&covered) {
                valid = false;
                break;
            }
            let name = ester_modifier_name(mol, atoms, root, ligand.oxygen, &core)?;
            if name.is_empty() {
                valid = false;
                break;
            }
            names.push(name);
            covered.extend(ligand_atoms);
        }
        if !valid || covered.union(&core).copied().collect::<AtomSet>() != *atoms {
            continue;
        }
        require_stereo_free_core(mol, &core)?;
        let hydroxy = role.count("hydroxy");
        let suffix = if hydroxy == 0 || spec.get("charge").is_some() {
            base.to_owned()
        } else {
            let hydrogen = if hydroxy == 1 {
                "hydrogen".into()
            } else {
                format!("{}hydrogen", multipliers::basic(hydroxy))
            };
            format!("{hydrogen} {base}")
        };
        matches.push(format!("{} {suffix}", modifier_phrase(&names, false)));
    }
    Ok(if matches.len() == 1 {
        matches.pop()
    } else {
        None
    })
}

fn organophosphorus_acid_name(
    mol: &Molecule,
    atoms: &AtomSet,
    phosphinic: bool,
) -> Result<Option<String>, String> {
    let centers: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].symbol == "P")
        .collect();
    if centers.len() != 1 {
        return Ok(None);
    }
    let c = centers[0];
    let a = &mol.atoms[&c];
    if get_cyclic_atoms(mol, &AtomSet::new()).contains(&c)
        || stereo_configured(mol, c)
        || competing_principal_group(mol, atoms, c)
    {
        return Ok(None);
    }
    if phosphinic && a.charge != 0 || !phosphinic && a.total_h_count != 0 {
        return Ok(None);
    }
    let (mut double, mut hydroxy, mut esters, mut roots) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for &n in mol.get_neighbors(c) {
        if !atoms.contains(&n) {
            return Ok(None);
        }
        let atom = &mol.atoms[&n];
        let bond = order(mol, c, n);
        if atom.symbol == "O"
            && (bond == 2 || !phosphinic && bond == 1 && atom.charge == -1 && mol.degree(n) == 1)
        {
            double.push(n);
        } else if atom.symbol == "O" && bond == 1 && atom.charge == 0 && mol.degree(n) == 1 {
            hydroxy.push(n);
        } else if atom.symbol == "O" && bond == 1 && atom.charge == 0 && mol.degree(n) == 2 {
            esters.push(n);
        } else if atom.is_carbon() && bond == 1 {
            roots.push(n);
        } else {
            return Ok(None);
        }
    }
    if double.len() != 1 {
        return Ok(None);
    }
    if phosphinic {
        if !(1..=2).contains(&roots.len())
            || hydroxy.len() + esters.len() != 1
            || a.total_h_count.max(a.explicit_h_count) != 2 - roots.len()
        {
            return Ok(None);
        }
    } else if roots.len() != 1
        || hydroxy.len() + esters.len() != 2
        || a.charge != 0 && !(a.charge == 1 && double.iter().any(|o| mol.atoms[o].charge == -1))
    {
        return Ok(None);
    }
    let acid: AtomSet = std::iter::once(c)
        .chain(double)
        .chain(hydroxy.iter().copied())
        .chain(esters.iter().copied())
        .collect();
    require_stereo_free_core(mol, &acid)?;
    let mut ligands = Vec::new();
    for root in roots {
        let name = ligand_name(mol, atoms, root, c, &acid)?;
        if name.is_empty() {
            return Ok(None);
        }
        ligands.push(name);
    }
    // Phosphinic acids group their carbon ligands as a centre prefix. The
    // single phosphonic-acid ligand is cited directly by the upstream route.
    let ligand = if phosphinic {
        center_ligands(&ligands)
    } else {
        ligands.remove(0)
    };
    let mut modifiers = Vec::new();
    for oxygen in esters {
        let Some(root) = mol.get_neighbors(oxygen).iter().copied().find(|n| *n != c) else {
            return Ok(None);
        };
        let name = ester_modifier_name(mol, atoms, root, oxygen, &acid)?;
        if name.is_empty() {
            return Ok(None);
        }
        modifiers.push(name);
    }
    if modifiers.is_empty() {
        return Ok(Some(format!(
            "{ligand}{} acid",
            if phosphinic {
                "phosphinic"
            } else {
                "phosphonic"
            }
        )));
    }
    let mut phrase = modifier_phrase(&modifiers, !phosphinic);
    if !phosphinic && !hydroxy.is_empty() {
        phrase.push_str(if hydroxy.len() == 1 {
            " hydrogen"
        } else {
            " dihydrogen"
        });
    }
    Ok(Some(format!(
        "{phrase} {ligand}{}",
        if phosphinic {
            "phosphinate"
        } else {
            "phosphonate"
        }
    )))
}

fn organoboronic_acid_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let centers: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].symbol == "B")
        .collect();
    if centers.len() != 1 {
        return Ok(None);
    }
    let c = centers[0];
    let a = &mol.atoms[&c];
    if a.charge != 0
        || a.total_h_count != 0
        || get_cyclic_atoms(mol, &AtomSet::new()).contains(&c)
        || competing_principal_group(mol, atoms, c)
    {
        return Ok(None);
    }
    let (mut hydroxy, mut roots) = (Vec::new(), Vec::new());
    for &n in mol.get_neighbors(c) {
        if !atoms.contains(&n) || order(mol, c, n) != 1 {
            return Ok(None);
        }
        let a = &mol.atoms[&n];
        if a.symbol == "O" && a.charge == 0 && mol.degree(n) == 1 {
            hydroxy.push(n);
        } else if a.is_carbon() {
            roots.push(n);
        } else {
            return Ok(None);
        }
    }
    if !matches!((hydroxy.len(), roots.len()), (2, 1) | (1, 2)) {
        return Ok(None);
    }
    let acid: AtomSet = std::iter::once(c).chain(hydroxy).collect();
    require_stereo_free_core(mol, &acid)?;
    let mut names = Vec::new();
    for root in roots {
        let name = ligand_name(mol, atoms, root, c, &acid)?;
        if name.is_empty() {
            return Ok(None);
        }
        names.push(name);
    }
    Ok(Some(format!(
        "{}{} acid",
        center_ligands(&names),
        if names.len() == 1 {
            "boronic"
        } else {
            "borinic"
        }
    )))
}

fn connected_branches(mol: &Molecule, atoms: &AtomSet) -> Vec<AtomSet> {
    let mut branches = Vec::new();
    let mut seen = AtomSet::new();
    for &start in atoms {
        if seen.contains(&start) {
            continue;
        }
        let mut branch = set([start]);
        seen.insert(start);
        let mut stack = vec![start];
        while let Some(idx) = stack.pop() {
            for &n in mol.get_neighbors(idx) {
                if atoms.contains(&n) && seen.insert(n) {
                    branch.insert(n);
                    stack.push(n);
                }
            }
        }
        branches.push(branch);
    }
    branches
}
fn ordered_backbone(mol: &Molecule, atoms: &AtomSet) -> Option<Vec<usize>> {
    let degrees: BTreeMap<_, _> = atoms
        .iter()
        .map(|i| {
            (
                *i,
                mol.get_neighbors(*i)
                    .iter()
                    .filter(|n| atoms.contains(n))
                    .count(),
            )
        })
        .collect();
    if degrees.values().any(|d| *d > 2) {
        return None;
    }
    let ends: Vec<_> = degrees
        .iter()
        .filter(|(_, d)| **d <= 1)
        .map(|(i, _)| *i)
        .collect();
    if ends.len() != 2 {
        return None;
    }
    let mut chain = vec![*ends.iter().min()?];
    let mut previous = None;
    while chain.len() < atoms.len() {
        let current = *chain.last()?;
        let neighbors: Vec<_> = mol
            .get_neighbors(current)
            .iter()
            .copied()
            .filter(|n| atoms.contains(n) && Some(*n) != previous)
            .collect();
        if neighbors.len() != 1 || chain.contains(&neighbors[0]) {
            return None;
        }
        previous = Some(current);
        chain.push(neighbors[0]);
    }
    Some(chain)
}
fn chain_oxide_oxygens(
    mol: &Molecule,
    chain: &[usize],
    atoms: &AtomSet,
) -> Option<BTreeMap<usize, usize>> {
    let mut oxides = BTreeMap::new();
    for &i in chain {
        if mol.atoms[&i].charge == 0 {
            continue;
        }
        if mol.atoms[&i].charge != 1 {
            return None;
        }
        let oxygens: Vec<_> = mol
            .get_neighbors(i)
            .iter()
            .copied()
            .filter(|n| {
                atoms.contains(n)
                    && mol.atoms[n].symbol == "O"
                    && mol.atoms[n].charge == -1
                    && mol.degree(*n) == 1
                    && order(mol, i, *n) == 1
            })
            .collect();
        if oxygens.len() != 1 {
            return None;
        }
        oxides.insert(i, oxygens[0]);
    }
    (!oxides.is_empty()).then_some(oxides)
}
fn homonuclear_backbone(mol: &Molecule, atoms: &AtomSet) -> Option<(String, Vec<usize>)> {
    let mut candidates = Vec::new();
    let symbols: BTreeSet<_> = atoms.iter().map(|i| mol.atoms[i].symbol.clone()).collect();
    for symbol in symbols {
        if !RULES
            .components
            .mononuclear_parent_hydrides
            .contains_key(&symbol)
            || matches!(symbol.as_str(), "F" | "Cl" | "Br" | "I")
        {
            continue;
        }
        let backbone: AtomSet = atoms
            .iter()
            .copied()
            .filter(|i| mol.atoms[i].symbol == symbol)
            .collect();
        if backbone.len() < 2 {
            continue;
        }
        if let Some(chain) = ordered_backbone(mol, &backbone) {
            let rank = ["N", "P", "Si", "B", "O", "S", "C"]
                .iter()
                .position(|s| *s == symbol)
                .unwrap_or(8);
            candidates.push((rank, symbol, chain));
        }
    }
    if let Some((_, symbol, chain)) = candidates.into_iter().min() {
        return Some((symbol, chain));
    }
    // A lone formal oxide centre may lie within a connected same-element chain
    // even when other disconnected atoms of that element occur in its ligands.
    let centers: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| {
            RULES
                .components
                .mononuclear_parent_hydrides
                .contains_key(&mol.atoms[i].symbol)
                && !matches!(mol.atoms[i].symbol.as_str(), "O" | "F" | "Cl" | "Br" | "I")
                && mol.atoms[i].charge == 1
                && chain_oxide_oxygens(mol, &[*i], atoms).is_some()
        })
        .collect();
    if centers.len() != 1 {
        return None;
    }
    let c = centers[0];
    let symbol = mol.atoms[&c].symbol.clone();
    let mut connected = set([c]);
    let mut stack = vec![c];
    while let Some(i) = stack.pop() {
        for &n in mol.get_neighbors(i) {
            if atoms.contains(&n) && mol.atoms[&n].symbol == symbol && connected.insert(n) {
                stack.push(n);
            }
        }
    }
    if connected.len() < 2 {
        return None;
    }
    Some((symbol, ordered_backbone(mol, &connected)?))
}
fn same_element_parent_name(symbol: &str, length: usize, orders: &[usize]) -> Option<String> {
    let parent = RULES.components.mononuclear_parent_hydrides.get(symbol)?;
    if length < 2 || !parent.ends_with("ane") {
        return None;
    }
    if !orders.iter().any(|o| *o > 1)
        && let Some(retained) = RULES
            .components
            .retained_homonuclear_chain_names
            .get(&format!("{symbol}{length}"))
    {
        return Some(retained.clone());
    }
    let base = format!("{}{parent}", multipliers::basic(length));
    let unsaturated: Vec<_> = orders.iter().enumerate().filter(|(_, o)| **o > 1).collect();
    if unsaturated.is_empty() {
        return Some(base);
    }
    if unsaturated.len() != 1 {
        return None;
    }
    let stem = &base[..base.len() - 3];
    let ending = match unsaturated[0].1 {
        2 => "ene",
        3 => "yne",
        _ => return None,
    };
    Some(if length == 2 {
        format!("{stem}{ending}")
    } else {
        format!("{stem}-{}-{ending}", unsaturated[0].0 + 1)
    })
}
fn locanted_ligand_prefix(items: &[(usize, String)]) -> String {
    let mut counts: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (loc, name) in items {
        counts.entry(name.clone()).or_default().push(*loc);
    }
    let mut groups: Vec<_> = counts.into_iter().collect();
    groups.sort_by_key(|(name, _)| substituent_sort_key(name));
    groups
        .into_iter()
        .map(|(name, mut locs)| {
            locs.sort();
            format!(
                "{}-{}",
                locs.iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                format_multiplier(&name, locs.len(), false)
            )
        })
        .collect::<Vec<_>>()
        .join("-")
}
fn homonuclear_chain_parent_name(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<String>, String> {
    let Some((symbol, chain)) = homonuclear_backbone(mol, atoms) else {
        return Ok(None);
    };
    let chain_set: AtomSet = chain.iter().copied().collect();
    let oxide = chain_oxide_oxygens(mol, &chain, atoms);
    if chain.iter().any(|i| mol.atoms[i].charge != 0) && oxide.is_none() {
        return Ok(None);
    }
    let oxide = oxide.unwrap_or_default();
    let oxide_atoms: AtomSet = oxide.values().copied().collect();
    let ligand_atoms: AtomSet = atoms.difference(&oxide_atoms).copied().collect();
    if perceive_groups(mol).iter().any(|g| {
        g.is_principal_candidate
            && g.atoms_involved
                .intersection(&ligand_atoms)
                .any(|i| !chain_set.contains(i))
    }) {
        return Ok(None);
    }
    let mut ligands = Vec::new();
    let mut covered = chain_set.clone();
    for branch in connected_branches(mol, &ligand_atoms.difference(&chain_set).copied().collect()) {
        let mut links = Vec::new();
        for &i in &branch {
            for &n in mol.get_neighbors(i) {
                if chain_set.contains(&n) {
                    links.push((n, i));
                }
            }
        }
        links.sort();
        if links.len() != 1 {
            return Ok(None);
        }
        let (attachment, root) = links[0];
        let bond = order(mol, attachment, root);
        if !matches!(bond, 1 | 2) || (bond == 2 && chain.len() <= 2) {
            return Ok(None);
        }
        let mut name = if bond == 2 {
            String::new()
        } else {
            terminal_ligand_name(mol, root)
        };
        if name.is_empty() {
            name = branch_name(mol, &ligand_atoms, root, &chain_set, attachment)?;
        }
        if name.is_empty() || bond == 2 && !strip_outer_parentheses(&name).ends_with("ylidene") {
            return Ok(None);
        }
        ligands.push((attachment, name));
        covered.extend(branch);
    }
    if covered != ligand_atoms {
        return Ok(None);
    }
    let forward: Vec<_> = chain.windows(2).map(|p| order(mol, p[0], p[1])).collect();
    type Choice = (
        (usize, Vec<usize>, Vec<usize>, Vec<usize>),
        String,
        Vec<(usize, String)>,
        Vec<usize>,
    );
    let mut best: Option<Choice> = None;
    for (oriented, orders) in [
        (chain.clone(), forward.clone()),
        (
            chain.iter().copied().rev().collect(),
            forward.iter().copied().rev().collect(),
        ),
    ] {
        let Some(parent) = same_element_parent_name(&symbol, oriented.len(), &orders) else {
            continue;
        };
        let locants: BTreeMap<_, _> = oriented
            .iter()
            .enumerate()
            .map(|(offset, atom)| (*atom, offset + 1))
            .collect();
        let mut placed: Vec<_> = ligands
            .iter()
            .map(|(i, name)| (locants[i], name.clone()))
            .collect();
        placed.sort_by_key(|(loc, name)| (substituent_sort_key(name), *loc));
        let mut oxide_locs: Vec<_> = oxide.keys().map(|i| locants[i]).collect();
        oxide_locs.sort();
        let mut sorted_locs: Vec<_> = placed.iter().map(|(l, _)| *l).collect();
        sorted_locs.sort();
        let key = (
            orders.iter().position(|o| *o > 1).unwrap_or(orders.len()),
            oxide_locs,
            sorted_locs,
            placed.iter().map(|(l, _)| *l).collect(),
        );
        if best.as_ref().is_none_or(|b| key < b.0) {
            best = Some((key, parent, placed, oriented));
        }
    }
    let Some((_, parent, placed, oriented)) = best else {
        return Ok(None);
    };
    // Upstream divergence: homonuclear chain assembly renders bond descriptors but no backbone atom stereo.
    if chain_set
        .iter()
        .chain(&oxide_atoms)
        .any(|&idx| stereo_configured(mol, idx))
    {
        return Err("openclatura: shortcut cannot preserve configured stereochemistry".into());
    }
    let prefix = locanted_ligand_prefix(&placed);
    let mut name = format!("{prefix}{parent}");
    if !oxide.is_empty() {
        let locants: BTreeMap<_, _> = oriented
            .iter()
            .enumerate()
            .map(|(offset, atom)| (*atom, offset + 1))
            .collect();
        let mut locs: Vec<_> = oxide.keys().map(|i| locants[i]).collect();
        locs.sort();
        let mult = if locs.len() > 1 {
            multipliers::basic(locs.len())
        } else {
            String::new()
        };
        name.push_str(&format!(
            " {}-{mult}oxide",
            locs.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    let stereo: Vec<_> = oriented
        .windows(2)
        .enumerate()
        .filter_map(|(i, p)| {
            let b = mol.get_bond(p[0], p[1])?;
            let s = b.stereo.as_deref()?;
            (b.order == 2 && matches!(s, "E" | "Z")).then(|| format!("{}{s}", i + 1))
        })
        .collect();
    if !stereo.is_empty() {
        name = format!("({})-{name}", stereo.join(","));
    }
    Ok(Some(name))
}

fn nitrogen_ligands(
    mol: &Molecule,
    atoms: &AtomSet,
    nitrogen: usize,
    core: &AtomSet,
    carbon_only: bool,
) -> Result<Option<Vec<(AtomSet, String)>>, String> {
    let mut ligands = Vec::new();
    for &root in mol.get_neighbors(nitrogen) {
        if core.contains(&root) || !atoms.contains(&root) || mol.atoms[&root].symbol == "H" {
            continue;
        }
        if carbon_only && !mol.atoms[&root].is_carbon() {
            return Ok(None);
        }
        let branch = component_atoms_until_blocked(mol, atoms, root, core);
        if branch.is_empty() {
            return Ok(None);
        }
        if carbon_only
            && (branch.iter().any(|i| !mol.atoms[i].is_carbon())
                || branch.iter().any(|i| {
                    mol.get_neighbors(*i)
                        .iter()
                        .any(|n| branch.contains(n) && order(mol, *i, *n) != 1)
                }))
        {
            return Ok(None);
        }
        let mut name = strip_outer_parentheses(&branch_name(mol, atoms, root, core, nitrogen)?);
        if name.is_empty() {
            name = alkyl_ligand_name(mol, atoms, root, nitrogen)?;
        }
        if name.is_empty() {
            return Ok(None);
        }
        ligands.push((branch, name));
    }
    ligands.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(Some(ligands))
}
fn hydroxyurea_parent_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let centers: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| {
            mol.atoms[i].is_carbon()
                && mol
                    .get_neighbors(*i)
                    .iter()
                    .filter(|n| {
                        atoms.contains(n) && mol.atoms[n].symbol == "O" && order(mol, *i, **n) == 2
                    })
                    .count()
                    == 1
                && mol
                    .get_neighbors(*i)
                    .iter()
                    .filter(|n| {
                        atoms.contains(n) && mol.atoms[n].symbol == "N" && order(mol, *i, **n) == 1
                    })
                    .count()
                    == 2
        })
        .collect();
    if centers.len() != 1 {
        return Ok(None);
    }
    let c = centers[0];
    let Some(oxygen) = mol
        .get_neighbors(c)
        .iter()
        .copied()
        .find(|n| atoms.contains(n) && mol.atoms[n].symbol == "O")
    else {
        return Ok(None);
    };
    let nitrogens: Vec<_> = mol
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| atoms.contains(n) && mol.atoms[n].symbol == "N" && order(mol, c, *n) == 1)
        .collect();
    if nitrogens
        .iter()
        .any(|n| mol.get_bond(c, *n).is_some_and(|b| b.in_small_ring))
    {
        return Ok(None);
    }
    let mut hydroxy = Vec::new();
    for &n in &nitrogens {
        let oxygens: Vec<_> = mol
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|i| {
                atoms.contains(i) && *i != c && mol.atoms[i].symbol == "O" && order(mol, n, *i) == 1
            })
            .collect();
        if !oxygens.is_empty() {
            if oxygens.len() != 1
                || mol
                    .get_neighbors(oxygens[0])
                    .iter()
                    .any(|i| *i != n && atoms.contains(i))
            {
                return Ok(None);
            }
            hydroxy.push((n, oxygens[0]));
        }
    }
    if hydroxy.len() != 1 {
        return Ok(None);
    }
    let (n_prime, oh) = hydroxy[0];
    let Some(&n) = nitrogens.iter().find(|i| **i != n_prime) else {
        return Ok(None);
    };
    let core = set([c, oxygen, n, n_prime, oh]);
    require_stereo_free_core(mol, &core)?;
    let Some(first) = nitrogen_ligands(mol, atoms, n, &core, true)? else {
        return Ok(None);
    };
    let Some(second) = nitrogen_ligands(mol, atoms, n_prime, &core, true)? else {
        return Ok(None);
    };
    let mut represented = core;
    for (branch, _) in first.iter().chain(&second) {
        represented.extend(branch);
    }
    if represented != *atoms {
        return Ok(None);
    }
    let mut prefixes: Vec<_> = first
        .iter()
        .map(|(_, name)| format!("N-{}", format_multiplier(name, 1, false)))
        .collect();
    prefixes.extend(
        second
            .iter()
            .map(|(_, name)| format!("N'-{}", format_multiplier(name, 1, false))),
    );
    prefixes.push("N'-hydroxy".into());
    Ok(Some(format!("{}urea", prefixes.join("-"))))
}
fn sulfamic_acid_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let cyclic = get_cyclic_atoms(mol, &AtomSet::new());
    let mut centers = Vec::new();
    for &c in atoms {
        if mol.atoms[&c].symbol != "S" || mol.atoms[&c].charge != 0 {
            continue;
        }
        let neighbors: Vec<_> = mol
            .get_neighbors(c)
            .iter()
            .copied()
            .filter(|n| atoms.contains(n))
            .collect();
        let oxygens: Vec<_> = neighbors
            .iter()
            .copied()
            .filter(|n| mol.atoms[n].symbol == "O")
            .collect();
        let nitrogens: Vec<_> = neighbors
            .iter()
            .copied()
            .filter(|n| mol.atoms[n].symbol == "N")
            .collect();
        if neighbors.len() != 4
            || oxygens.len() != 3
            || nitrogens.len() != 1
            || cyclic.contains(&nitrogens[0])
        {
            continue;
        }
        let double: Vec<_> = oxygens
            .iter()
            .copied()
            .filter(|o| order(mol, c, *o) == 2)
            .collect();
        let single: Vec<_> = oxygens
            .iter()
            .copied()
            .filter(|o| !double.contains(o))
            .collect();
        if double.len() != 2
            || single.len() != 1
            || mol.atoms[&single[0]].charge != 0
            || mol.degree(single[0]) != 1
        {
            continue;
        }
        centers.push((c, nitrogens[0], oxygens));
    }
    if centers.len() != 1 {
        return Ok(None);
    }
    let Some((c, n, oxygens)) = centers.pop() else {
        return Ok(None);
    };
    if mol.atoms[&n].charge != 0 {
        return Ok(None);
    }
    let core: AtomSet = [c, n].into_iter().chain(oxygens).collect();
    require_stereo_free_core(mol, &core)?;
    let Some(ligands) = nitrogen_ligands(mol, atoms, n, &core, false)? else {
        return Ok(None);
    };
    let mut represented = core;
    for (branch, _) in &ligands {
        represented.extend(branch);
    }
    if represented != *atoms {
        return Ok(None);
    }
    let names: Vec<_> = ligands.into_iter().map(|(_, n)| n).collect();
    Ok(Some(format!("{}sulfamic acid", center_ligands(&names))))
}
fn azinic_acid_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let mut centers = Vec::new();
    for &n in atoms {
        if mol.atoms[&n].symbol != "N" || mol.atoms[&n].charge != 1 {
            continue;
        }
        let neighbors: Vec<_> = mol
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|i| atoms.contains(i))
            .collect();
        let oxygens: Vec<_> = neighbors
            .iter()
            .copied()
            .filter(|i| mol.atoms[i].symbol == "O")
            .collect();
        let others: Vec<_> = neighbors
            .iter()
            .copied()
            .filter(|i| !oxygens.contains(i))
            .collect();
        if oxygens.len() != 2 || others.len() != 1 {
            continue;
        }
        let oxido = oxygens
            .iter()
            .filter(|o| mol.atoms[o].charge == -1 && mol.degree(**o) == 1)
            .count();
        let hydroxy = oxygens
            .iter()
            .filter(|o| {
                mol.atoms[o].charge == 0 && mol.degree(**o) == 1 && mol.atoms[o].total_h_count > 0
            })
            .count();
        if oxido == 1 && hydroxy == 1 {
            centers.push((n, others[0], oxygens));
        }
    }
    if centers.len() != 1 {
        return Ok(None);
    }
    let Some((n, root, oxygens)) = centers.pop() else {
        return Ok(None);
    };
    let bond = order(mol, n, root);
    if !matches!(bond, 1 | 2) {
        return Ok(None);
    }
    let core: AtomSet = std::iter::once(n).chain(oxygens).collect();
    require_stereo_free_core(mol, &core)?;
    let name = strip_outer_parentheses(&branch_name(mol, atoms, root, &core, n)?);
    let branch = component_atoms_until_blocked(mol, atoms, root, &core);
    if name.is_empty()
        || branch.is_empty()
        || branch.union(&core).copied().collect::<AtomSet>() != *atoms
    {
        return Ok(None);
    }
    Ok(Some(if bond == 2 {
        format!("{}azinic acid", format_multiplier(&name, 1, true))
    } else {
        format!("N-{}azinic acid", format_multiplier(&name, 1, false))
    }))
}

fn phosphane_borane_zwitterion_name(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<String>, String> {
    let mut pairs = Vec::new();
    for &p in atoms {
        if mol.atoms[&p].symbol != "P" || mol.atoms[&p].charge <= 0 {
            continue;
        }
        for &b in mol.get_neighbors(p) {
            if atoms.contains(&b)
                && mol.atoms[&b].symbol == "B"
                && mol.atoms[&b].charge < 0
                && mol.atoms[&b].total_h_count == 3
                && mol
                    .get_neighbors(p)
                    .iter()
                    .filter(|n| atoms.contains(n) && **n != b)
                    .count()
                    == 3
            {
                pairs.push((p, b));
            }
        }
    }
    let Some(&(p, b)) = pairs.first() else {
        return Ok(None);
    };
    if stereo_configured(mol, p) {
        return Ok(None);
    }
    let roots: Vec<_> = mol
        .get_neighbors(p)
        .iter()
        .copied()
        .filter(|n| atoms.contains(n) && *n != b && mol.atoms[n].is_carbon())
        .collect();
    if roots.len() != 3 {
        return Ok(None);
    }
    let core = set([p, b]);
    require_stereo_free_core(mol, &core)?;
    let mut seen = AtomSet::new();
    let mut names = Vec::new();
    for root in roots {
        let branch = component_atoms_until_blocked(mol, atoms, root, &core);
        if branch.is_empty() || !branch.is_disjoint(&seen) {
            return Ok(None);
        }
        seen.extend(branch);
        let mut name = strip_outer_parentheses(&branch_name(mol, atoms, root, &core, p)?);
        if name.is_empty() {
            name = alkyl_ligand_name(mol, atoms, root, p)?;
        }
        if name.is_empty() {
            return Ok(None);
        }
        names.push(name);
    }
    if seen.union(&core).copied().collect::<AtomSet>() != *atoms {
        return Ok(None);
    }
    Ok(Some(format!(
        "({}phosphaniumyl)boranuide",
        center_ligands(&names)
    )))
}

fn sulfonium_ylide_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    let sulfurs: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].symbol == "S" && mol.atoms[i].charge > 0)
        .collect();
    let carbons: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].is_carbon() && mol.atoms[i].charge < 0)
        .collect();
    if sulfurs.len() != 1 || carbons.len() != 1 {
        return Ok(None);
    }
    let (s, c) = (sulfurs[0], carbons[0]);
    if stereo_configured(mol, s)
        || mol.atoms[&s].total_h_count != 0
        || mol.atoms[&s].explicit_h_count != 0
        || order(mol, s, c) != 1
    {
        return Ok(None);
    }
    let roots: Vec<_> = mol
        .get_neighbors(s)
        .iter()
        .copied()
        .filter(|n| *n != c && atoms.contains(n) && mol.atoms[n].is_carbon())
        .collect();
    if roots.len() != 2 {
        return Ok(None);
    }
    require_stereo_free_core(mol, &set([s, c]))?;
    let mut sulfur_atoms = AtomSet::new();
    let mut names = Vec::new();
    for root in roots {
        let branch = carbon_ligand_atoms(mol, atoms, root, s);
        if branch.is_empty() || !branch.is_disjoint(&sulfur_atoms) {
            return Ok(None);
        }
        sulfur_atoms.extend(branch);
        let name = alkyl_ligand_name(mol, atoms, root, s)?;
        if name.is_empty() {
            return Ok(None);
        }
        names.push(name);
    }
    let subs: Vec<_> = mol
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| *n != s && atoms.contains(n) && mol.atoms[n].symbol != "H")
        .collect();
    if subs.len() > 1 {
        return Ok(None);
    }
    let (mut sub_atoms, mut sub_name) = (AtomSet::new(), String::new());
    if let Some(&root) = subs.first() {
        let mut blocked = set([s, c]);
        blocked.extend(&sulfur_atoms);
        sub_atoms = component_atoms_until_blocked(mol, atoms, root, &blocked);
        if sub_atoms.is_empty() || !sub_atoms.is_disjoint(&sulfur_atoms) {
            return Ok(None);
        }
        sub_name = strip_outer_parentheses(&branch_name(mol, atoms, root, &blocked, c)?);
        if sub_name.is_empty() {
            sub_name = alkyl_ligand_name(mol, atoms, root, c)?;
        }
        if sub_name.is_empty() {
            return Ok(None);
        }
    }
    let mut represented = set([s, c]);
    represented.extend(sulfur_atoms);
    represented.extend(&sub_atoms);
    if represented != *atoms {
        return Ok(None);
    }
    let prefix = format!("{}sulfaniumyl", center_ligands(&names));
    let parent = if subs.is_empty() {
        "methanide".to_owned()
    } else {
        let degrees: Vec<_> = sub_atoms
            .iter()
            .map(|i| {
                mol.get_neighbors(*i)
                    .iter()
                    .filter(|n| sub_atoms.contains(n))
                    .count()
            })
            .collect();
        let linear = sub_atoms.iter().all(|i| mol.atoms[i].is_carbon())
            && degrees.iter().sum::<usize>() / 2 == sub_atoms.len() - 1
            && degrees.iter().all(|d| *d <= 2)
            && sub_atoms.iter().all(|i| {
                mol.get_neighbors(*i)
                    .iter()
                    .all(|n| !sub_atoms.contains(n) || order(mol, *i, *n) == 1)
            });
        if linear {
            return Ok(Some(format!(
                "1-({prefix}){}an-1-ide",
                stems::stem_for(sub_atoms.len() + 1)
            )));
        }
        if sub_name.ends_with("yl") {
            format!("{sub_name}methanide")
        } else {
            format!("({sub_name})methanide")
        }
    };
    Ok(Some(format!("({prefix}){parent}")))
}

fn nitrogen_chalcogenide_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    use super::nomenclature::section;
    let mut matches = Vec::new();
    for &n in atoms {
        let a = &mol.atoms[&n];
        if a.symbol != "N" || a.charge <= 0 || a.is_aromatic {
            continue;
        }
        for &ch in mol.get_neighbors(n) {
            let e = &mol.atoms[&ch];
            if !atoms.contains(&ch)
                || e.charge >= 0
                || mol.degree(ch) != 1
                || order(mol, n, ch) != 1
                || section("chalcogen_elements")[&e.symbol]["nitrogen_chalcogenide_citation"]
                    .as_str()
                    != Some("functional_class")
            {
                continue;
            }
            let others: Vec<_> = mol
                .get_neighbors(n)
                .iter()
                .copied()
                .filter(|i| *i != ch)
                .collect();
            if others.is_empty() || others.iter().any(|i| !mol.atoms[i].is_carbon()) {
                continue;
            }
            let orders: Vec<_> = others.iter().map(|i| order(mol, n, *i)).collect();
            let imine = orders.iter().filter(|o| **o == 2).count() == 1
                && orders.iter().all(|o| matches!(o, 1 | 2));
            let amine = orders.iter().all(|o| *o == 1);
            if !imine && !amine {
                continue;
            }
            matches.push((n, ch, imine));
        }
    }
    if matches.len() != 1 {
        return Ok(None);
    }
    let (n, ch, imine) = matches[0];
    require_stereo_free_core(mol, &set([n, ch]))?;
    let reduced_atoms: AtomSet = atoms.iter().copied().filter(|i| *i != ch).collect();
    let mut reduced = mol.subgraph(&reduced_atoms);
    reduced.set_atom_charge(n, 0)?;
    let parent = super::component_namer::name_component(&reduced, &reduced_atoms)?;
    if parent.is_empty() {
        return Ok(None);
    }
    let Some(class) = section("chalcogenide_class_names")[&mol.atoms[&ch].symbol].as_str() else {
        return Ok(None);
    };
    Ok(Some(if imine {
        format!("{parent} N-{class}")
    } else {
        format!("{parent} {class}")
    }))
}

fn peroxide_linkage_name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<String>, String> {
    use super::nomenclature::section;
    let is_chalcogen = |i: usize| {
        section("chalcogen_elements")
            .get(&mol.atoms[&i].symbol)
            .is_some()
    };
    let mut matches = Vec::new();
    for bond in mol.bonds.values() {
        let (l, r) = (bond.u, bond.v);
        if !atoms.contains(&l)
            || !atoms.contains(&r)
            || bond.order != 1
            || !is_chalcogen(l)
            || !is_chalcogen(r)
            || [l, r]
                .iter()
                .any(|i| mol.atoms[i].charge != 0 || mol.atoms[i].radical_electrons != 0)
        {
            continue;
        }
        let left: Vec<_> = mol
            .get_neighbors(l)
            .iter()
            .copied()
            .filter(|n| *n != r)
            .collect();
        let right: Vec<_> = mol
            .get_neighbors(r)
            .iter()
            .copied()
            .filter(|n| *n != l)
            .collect();
        if left.len() != 1
            || right.len() != 1
            || !mol.atoms[&left[0]].is_carbon()
            || !mol.atoms[&right[0]].is_carbon()
        {
            continue;
        }
        let blocked = set([l, r]);
        let branch = subgraph_component(mol, left[0], &blocked);
        if branch.contains(&right[0]) {
            continue;
        }
        if [(left[0], l), (right[0], r)]
            .iter()
            .any(|(attachment, linker)| {
                mol.get_neighbors(*attachment)
                    .iter()
                    .any(|n| n != linker && is_chalcogen(*n) && order(mol, *attachment, *n) == 2)
            })
        {
            continue;
        }
        matches.push((l, r, left[0], right[0]));
    }
    if matches.len() != 1 {
        return Ok(None);
    }
    let (l, r, la, ra) = matches[0];
    let (le, re) = (&mol.atoms[&l].symbol, &mol.atoms[&r].symbol);
    if le == re {
        return Ok(None);
    }
    // The O/S pair deliberately reuses the ordinary substituent pipeline.
    if [le.as_str(), re.as_str()]
        .iter()
        .all(|s| matches!(*s, "O" | "S"))
    {
        return Ok(None);
    }
    let mut unique = vec![le.as_str(), re.as_str()];
    unique.sort();
    unique.dedup();
    let Some(class) = section("chalcogen_peroxide_class_names")
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| {
            let mut e: Vec<_> = row["elements"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .collect();
            e.sort();
            e == unique
        })
        .and_then(|row| row["name"].as_str())
    else {
        return Ok(None);
    };
    let blocked = set([l, r]);
    require_stereo_free_core(mol, &blocked)?;
    let left = strip_outer_parentheses(&branch_name(mol, atoms, la, &blocked, l)?);
    let right = strip_outer_parentheses(&branch_name(mol, atoms, ra, &blocked, r)?);
    if left.is_empty() || right.is_empty() {
        return Ok(None);
    }
    let phrase = if left == right {
        format_multiplier(&left, 2, false)
    } else {
        let mut located = [format!("{le}-{left}"), format!("{re}-{right}")];
        located.sort_by_key(|s| substituent_sort_key(s));
        located.join(" ")
    };
    Ok(Some(format!("{phrase} {class}")))
}

fn anhydride_half_atoms(mol: &Molecule, start: usize, bridge: &AtomSet) -> AtomSet {
    subgraph_component(mol, start, bridge)
}
fn anhydride_half_name(mol: &Molecule, start: usize, bridge: &AtomSet) -> Result<String, String> {
    let mut atoms = anhydride_half_atoms(mol, start, bridge);
    let mut fragment = mol.subgraph(&atoms);
    let oh = mol.atoms.keys().max().copied().unwrap_or(0) + 100;
    let mut oxygen = Atom::new(oh, "O");
    oxygen.total_h_count = 1;
    fragment.add_atom(oxygen)?;
    let bond = fragment.bonds.keys().max().copied().unwrap_or(0) + 1;
    fragment.add_bond(Bond::new(bond, start, oh, 1))?;
    atoms.insert(oh);
    super::component_namer::name_component_with_citation(&fragment, &atoms, Some("anhydride_half"))
}
fn anhydride_class_name(mol: &Molecule, path: &[usize]) -> Option<String> {
    use super::nomenclature::section;
    let elements: Vec<_> = path[1..path.len() - 1]
        .iter()
        .map(|i| mol.atoms[i].symbol.as_str())
        .collect();
    let (template, replacement) = match elements.as_slice() {
        ["O"] => ("anhydride", String::new()),
        [element] => (
            "single_replacement_anhydride",
            section("chalcogen_elements")[*element]["replacement_infix"]
                .as_str()?
                .to_owned(),
        ),
        ["O", "O"] => ("peroxyanhydride", String::new()),
        [a, b] => {
            let replacements: Vec<_> = [a, b].into_iter().copied().filter(|e| *e != "O").collect();
            if replacements.len() == 1 {
                (
                    "single_replacement_peroxyanhydride",
                    section("chalcogen_elements")[replacements[0]]["replacement_infix"]
                        .as_str()?
                        .to_owned(),
                )
            } else if a == b {
                (
                    "double_replacement_peroxyanhydride",
                    section("chalcogen_elements")[*a]["replacement_infix"]
                        .as_str()?
                        .to_owned(),
                )
            } else {
                let mut infixes = Vec::new();
                for e in replacements {
                    infixes.push(section("chalcogen_elements")[e]["replacement_infix"].as_str()?);
                }
                infixes.sort();
                ("mixed_replacement_peroxyanhydride", infixes.concat())
            }
        }
        _ => return None,
    };
    Some(
        section("chalcogen_templates")[template]
            .as_str()?
            .replace("{replacement}", &replacement),
    )
}
pub fn try_name_anhydride_component(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    principal_key: Option<&str>,
) -> Result<Option<String>, String> {
    for group in groups {
        if Some(group.key.as_str()) != principal_key
            || !group
                .resolved_rule
                .as_ref()
                .is_some_and(|r| r.has_capability("anhydride"))
        {
            continue;
        }
        let Some(descriptor) = group.descriptor.as_ref() else {
            continue;
        };
        if descriptor.centers.len() != 2 || descriptor.linker_paths.is_empty() {
            continue;
        }
        let path = &descriptor.linker_paths[0];
        if !(3..=4).contains(&path.len()) {
            continue;
        }
        let bridge: AtomSet = path[1..path.len() - 1].iter().copied().collect();
        if bridge.is_empty() {
            continue;
        }
        let left = anhydride_half_name(mol, descriptor.centers[0], &bridge)?;
        let right = anhydride_half_name(mol, descriptor.centers[1], &bridge)?;
        let Some(class) = anhydride_class_name(mol, path) else {
            continue;
        };
        require_stereo_free_core(mol, &bridge)?;
        if left == right {
            return Ok(Some(format!("{left} {class}")));
        }
        let mut halves = [(left.clone(), left), (right.clone(), right)];
        if path.len() == 4 && mol.atoms[&path[1]].symbol != mol.atoms[&path[2]].symbol {
            halves[0].1 = format!("{}-{}", mol.atoms[&path[1]].symbol, halves[0].1);
            halves[1].1 = format!("{}-{}", mol.atoms[&path[2]].symbol, halves[1].1);
        }
        halves.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(Some(format!("{} {} {class}", halves[0].1, halves[1].1)));
    }
    Ok(None)
}

#[derive(Clone, Debug)]
pub struct StructuralParentName {
    pub name: String,
    /// Only the homonuclear-chain route supplies the chain audit plan used by
    /// the upstream component-level suffix preference exception.
    pub is_chain: bool,
}

impl StructuralParentName {
    fn ordinary(name: String) -> Self {
        Self {
            name,
            is_chain: false,
        }
    }
}

pub fn blocking_suffix_defers_to_chain_parent(
    mol: &Molecule,
    atoms: &AtomSet,
    groups: &[PerceivedGroup],
    principal_key: Option<&str>,
    result: Option<&StructuralParentName>,
) -> bool {
    if !result.is_some_and(|result| result.is_chain) {
        return false;
    }
    let characteristic: AtomSet = groups
        .iter()
        .filter(|group| Some(group.key.as_str()) == principal_key)
        .filter(|group| {
            group
                .resolved_rule
                .as_ref()
                .is_some_and(|rule| rule.has_capability("blocks_structural_parent"))
        })
        .flat_map(|group| {
            group
                .descriptor
                .as_ref()
                .map_or_else(|| group.atom_ids(), |descriptor| descriptor.atom_ids())
        })
        .collect();
    atoms.iter().any(|idx| {
        !characteristic.contains(idx)
            && !mol.atoms[idx].is_carbon()
            && mol.atoms[idx].symbol != "H"
            && mol.degree(*idx) > 1
    })
}

#[cfg(test)]
pub fn structural_replacement_parent_name(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<String>, String> {
    Ok(structural_replacement_parent_result(mol, atoms)?.map(|result| result.name))
}

pub fn structural_replacement_parent_result(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<StructuralParentName>, String> {
    // Keep the Python shortcut precedence before ordinary chain and ring parents.
    if let Some(name) = small_inorganic_parent_name(mol, atoms) {
        require_stereo_free_core(mol, atoms)?;
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = biphenyl_parent_result(mol, atoms) {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = simple_azine_parent_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = phosphane_borane_zwitterion_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = sulfonium_ylide_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = nitrogen_chalcogenide_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = peroxide_linkage_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = hydroxyurea_parent_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = sulfamic_acid_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = azinic_acid_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = oxoacid_ester_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = oxoacid_parent_name(mol, atoms) {
        require_stereo_free_core(mol, atoms)?;
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = organophosphorus_acid_name(mol, atoms, true)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = organophosphorus_acid_name(mol, atoms, false)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = organoboronic_acid_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = sulfoxide_parent_name(mol, atoms)? {
        return Ok(Some(StructuralParentName::ordinary(name)));
    }
    if let Some(name) = homonuclear_chain_parent_name(mol, atoms)? {
        return Ok(Some(StructuralParentName {
            name,
            is_chain: true,
        }));
    }
    Ok(simple_central_parent_hydride_name(mol, atoms)?.map(StructuralParentName::ordinary))
}

#[cfg(test)]
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;

    fn special(smiles: &str) -> Option<String> {
        let mol = read_smiles(smiles).unwrap();
        structural_replacement_parent_name(&mol, &all_atoms(&mol)).unwrap()
    }

    #[test]
    fn single_atom_hydrides_require_complete_hydrogen_count() {
        let mut mol = Molecule::new();
        let atom = Atom::new(0, "Si");
        mol.add_atom(atom).unwrap();
        assert_eq!(single_atom_component_name(&mol, &set([0])), None);
        mol.atoms.get_mut(&0).unwrap().total_h_count = 4;
        assert_eq!(
            single_atom_component_name(&mol, &set([0])).as_deref(),
            Some("silane")
        );
    }

    #[test]
    fn graph_shortcuts_preserve_ligand_and_oxygen_roles() {
        assert_eq!(special("CS(C)=O").as_deref(), Some("dimethyl sulfoxide"));
        assert_eq!(
            special("CCS(C)=O").as_deref(),
            Some("ethyl methyl sulfoxide")
        );
        assert_eq!(
            special("CP(=O)(O)O").as_deref(),
            Some("methylphosphonic acid")
        );
        assert_eq!(
            special("CO[P](=O)(O)O").as_deref(),
            Some("methyl dihydrogen phosphate")
        );
        assert_eq!(special("CB(O)O").as_deref(), Some("methylboronic acid"));
    }

    #[test]
    fn biphenyl_shortcut_requires_two_complete_benzene_rings() {
        for smiles in ["c1ccccc1-c1ccccc1", "C1=CC=C(C=C1)C2=CC=CC=C2"] {
            assert_eq!(
                special(smiles).as_deref(),
                Some("1,1'-biphenyl"),
                "{smiles}"
            );
        }
        for smiles in [
            "c1ccccc1C1CCCCC1",
            "C1CCCCC1C1CCCCC1",
            "Cc1ccccc1-c1ccccc1",
            "c1ccccc1-c1ccccn1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            assert_eq!(
                biphenyl_parent_result(&mol, &all_atoms(&mol)),
                None,
                "{smiles}"
            );
        }
    }

    #[test]
    fn alkyl_shortcuts_decline_unrendered_ligand_stereo() {
        for smiles in [
            "C[C@H](CC)P(=O)(O)O",
            "C[C@@H](CC)P(=O)(O)O",
            "C[C@H](CC)B(O)O",
            "C[C@@H](CC)B(O)O",
            "C[C@H](CC)S(C)=O",
            "CCC[C@H](C)[S@@](C)=O",
        ] {
            assert_eq!(
                super::super::name_smiles(smiles).unwrap_err(),
                "openclatura: shortcut cannot preserve configured stereochemistry",
                "{smiles}"
            );
        }
        let left = super::super::name_smiles("C[S@](=O)CC").unwrap();
        let right = super::super::name_smiles("C[S@@](=O)CC").unwrap();
        assert_ne!(left, right);
        assert!(left.starts_with("(R)-") || left.starts_with("(S)-"));
        assert!(right.starts_with("(R)-") || right.starts_with("(S)-"));
    }

    #[test]
    fn manual_azine_shortcuts_decline_unrendered_atom_and_bond_stereo() {
        for smiles in [
            "C=NN=C[C@H]1CCOC1",
            "C/C=C/C=NN=C",
            "CCC/C=N/N=C/C",
            "C/C=N/N=CCC",
        ] {
            assert!(super::super::name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn charged_alkyl_ligands_are_not_rendered_as_neutral() {
        for smiles in ["C[CH-]P(=O)(O)O", "C[CH-]B(O)O", "C[CH-]S(C)=O"] {
            assert!(super::super::name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn public_naming_bounds_shortcut_alkyl_path_search() {
        use std::time::{Duration, Instant};

        let mut tree = "C".to_owned();
        for _ in 0..7 {
            tree = format!("C({tree}){tree}");
        }
        let started = Instant::now();
        assert_eq!(
            super::super::name_smiles(&format!("P(=O)(O)(O){tree}")),
            Err("openclatura: naming complexity limit exceeded".into())
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(
            super::super::name_smiles("CCC(CC)P(=O)(O)O").as_deref(),
            Ok("pentan-3-ylphosphonic acid")
        );
    }

    #[test]
    fn phosphorus_functional_parents_keep_their_own_ligand_grouping() {
        for (smiles, expected) in [
            ("CCC(CC)P(=O)(OC)OC", "dimethyl pentan-3-ylphosphonate"),
            ("CCC(CC)P(=O)(O)O", "pentan-3-ylphosphonic acid"),
            ("CCC(CC)[PH](=O)O", "(pentan-3-yl)phosphinic acid"),
        ] {
            assert_eq!(special(smiles).as_deref(), Some(expected), "{smiles}");
        }
    }

    #[test]
    fn azine_shortcut_uses_symmetric_carbonyl_and_ring_side_roles() {
        for (smiles, expected) in [
            ("C=N-N=C", "1,2-bis(methylidene)hydrazine"),
            ("CC(C)=NN=C(C)C", "1,2-bis(propan-2-ylidene)hydrazine"),
            ("CCC(C)=NN=CC", "butan-2-one ethan-1-ylidenehydrazone"),
            ("C/C=N/N=C/C", "1,2-bis[(E)-ethan-1-ylidene]hydrazine"),
            ("c1ccccc1C=NN=C", "benzaldehyde methylidenehydrazone"),
            (
                "c1ccncc1C=NN=C",
                "pyridine-3-carbaldehyde methylidenehydrazone",
            ),
            ("C1CCCCC1=NN=C", "formaldehyde cyclohexylidenehydrazone"),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let actual =
                super::super::component_namer::name_component(&mol, &all_atoms(&mol)).unwrap();
            assert_eq!(actual, expected, "{smiles}");
        }
    }

    #[test]
    fn azine_unsaturation_requires_order_independent_locants() {
        for smiles in [
            "CCCC=NN=CCC=C",
            "CCCC=NN=CCC#C",
            "C=CCC=NN=CCCC",
            "C#CCC=NN=CCCC",
        ] {
            assert_eq!(
                super::super::name_smiles(smiles).unwrap_err(),
                "openclatura: azine unsaturation depends on upstream set ordering",
                "{smiles}"
            );
        }
        for smiles in ["CCCC=NN=CC=CC", "CC=CC=NN=CCCC"] {
            assert_eq!(
                super::super::name_smiles(smiles).as_deref(),
                Ok("butanal but-2-ene-1-ylidenehydrazone"),
                "{smiles}"
            );
        }
    }

    #[test]
    fn substituted_amidino_azine_precedes_a_ligand_anhydride() {
        let smiles = "CC(=O)OC(=O)N(/C(N)=N/N=C=O)C(C)C";
        let mol = read_smiles(smiles).unwrap();
        let atoms = all_atoms(&mol);
        let expected = "1-((1E)-(((acetyloxy)carbonyl)(propan-2-yl)amino)(amino)methylidene)-2-(oxomethylidene)hydrazine";
        assert_eq!(
            simple_azine_parent_name(&mol, &atoms).unwrap().as_deref(),
            Some(expected)
        );
        assert_eq!(
            super::super::component_namer::name_component(&mol, &atoms).unwrap(),
            expected
        );
    }

    #[test]
    fn homonuclear_parent_keeps_chain_and_substituent_locants() {
        assert_eq!(special("CSSC").as_deref(), Some("1,2-dimethyldisulfane"));
        assert_eq!(special("NN").as_deref(), Some("hydrazine"));
        assert_eq!(
            special("Cl[SiH2][SiH2]F").as_deref(),
            Some("1-chloro-2-fluorodisilane")
        );
    }

    #[test]
    fn anhydride_halves_use_acid_citation_suffixes() {
        for (smiles, expected) in [
            ("CC(=O)OC(C)=O", "acetic anhydride"),
            ("CCC(=S)OC(=O)", "formic propanethioic anhydride"),
            ("CCC(=S)OSC(=S)CC", "propanethioic thioperoxyanhydride"),
            ("CC(=S)OC(C)=S", "ethanethioic anhydride"),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let actual =
                super::super::component_namer::name_component(&mol, &all_atoms(&mol)).unwrap();
            assert_eq!(actual, expected, "{smiles}");
        }
    }

    #[test]
    fn peroxol_suffix_defers_only_for_a_senior_heteroatom_chain() {
        for (smiles, expected, chain_wins) in [
            ("COO", "methaneperoxol", false),
            ("CS[SH]", "methanedithioperoxol", false),
            ("C[Se][SeH]", "methanediselenoperoxol", false),
            ("C[Te][TeH]", "methaneditelluroperoxol", false),
            ("SSCCOC", "1-(2-methoxyethyl)disulfane", true),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = all_atoms(&mol);
            let groups = super::super::principal_groups::component_groups(&mol, &atoms);
            let key = super::super::principal_groups::component_principal_key(&groups, false);
            let result = structural_replacement_parent_result(&mol, &atoms).unwrap();
            assert_eq!(
                blocking_suffix_defers_to_chain_parent(
                    &mol,
                    &atoms,
                    &groups,
                    key.as_deref(),
                    result.as_ref()
                ),
                chain_wins,
                "{smiles}"
            );
            let actual = super::super::component_namer::name_component(&mol, &atoms).unwrap();
            assert_eq!(actual, expected, "{smiles}");
        }
    }
}
