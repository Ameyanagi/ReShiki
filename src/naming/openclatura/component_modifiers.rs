//! Ester alcohol fragments and principal-group N substituents.
use super::{
    assembly_parts::{AssemblyParts, SubstituentItem},
    assembly_prefixes::substituent_sort_key,
    component_group_rules::ester_or_peroxy_single_oxygen,
    formatting::strip_outer_parentheses,
    locants::parse_locant,
    molecule::{AtomSet, Molecule, bond_ids_within},
    namer::{name_subgraph_rendered, name_subgraph_with_upstream},
    perception::{LigandRole, PerceivedGroup},
    subgraph_tools::subgraph_component,
};
use std::collections::BTreeMap;

pub fn add_component_front_modifiers(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    exclude: &AtomSet,
    locants: &BTreeMap<usize, String>,
) -> Result<AtomSet, String> {
    let mut consumed = AtomSet::new();
    let Some(first) = groups.iter().find(|g| Some(g.key.as_str()) == key) else {
        return Ok(consumed);
    };
    if !first
        .resolved_rule
        .as_ref()
        .is_some_and(|r| r.has_capability("front_modifier"))
    {
        return Ok(consumed);
    }
    for group in groups.iter().filter(|g| Some(g.key.as_str()) == key) {
        let Some(linker) = ester_or_peroxy_single_oxygen(mol, group) else {
            continue;
        };
        let Some(&root) = mol
            .get_neighbors(linker)
            .iter()
            .find(|i| !group.atoms_involved.contains(i))
        else {
            continue;
        };
        let mut blocked = exclude.clone();
        blocked.insert(linker);
        let branch = name_subgraph_with_upstream(mol, root, &blocked, Some(linker))?;
        let mut modifier = strip_outer_parentheses(&branch);
        if let Some(d) = &group.descriptor {
            let linkers = d.ligands_with_role(LigandRole::CarbonLink);
            let doubles = d.ligands_with_role(LigandRole::DoubleBonded);
            if !linkers.is_empty()
                && !doubles.is_empty()
                && d.ligands.iter().any(|l| l.element != "O")
            {
                let element_locant = d
                    .ligands
                    .iter()
                    .filter(|l| l.role != LigandRole::DoubleBonded)
                    .map(|l| l.element.as_str())
                    .collect::<String>();
                if super::formatting::is_complex_prefix(&modifier) {
                    modifier = super::formatting::format_multiplier(&modifier, 1, true);
                }
                modifier = format!("{element_locant}-{modifier}");
            }
        }
        consumed.extend(subgraph_component(mol, root, &blocked));
        parts.front_modifiers.push(modifier);
        parts
            .front_modifier_locants
            .push(locants.get(&group.attachment_carbon).cloned());
    }
    Ok(consumed)
}

fn nitrogen_rank(
    mol: &Molecule,
    n: usize,
    c: usize,
    exclude: &AtomSet,
) -> Result<(usize, String, isize), String> {
    let mut blocked = exclude.clone();
    blocked.insert(n);
    let names = mol
        .get_neighbors(n)
        .iter()
        .copied()
        .filter(|r| *r != c && mol.atoms[r].symbol != "H")
        .map(|r| {
            name_subgraph_with_upstream(mol, r, &blocked, Some(n)).map(|s| substituent_sort_key(&s))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if let Some(first) = names.iter().min() {
        (0, first.clone(), -(names.len() as isize))
    } else {
        (1, String::new(), 0)
    })
}

pub fn n_substituent_locant(
    key: &str,
    group_count: usize,
    n_count: usize,
    n_index: usize,
    global: usize,
    group_index: usize,
) -> String {
    if key == "hydrazine" {
        return if n_index == 0 { "N" } else { "N'" }.into();
    }
    let primes = if matches!(key, "amidine" | "ring_amidine") && group_count > 1 {
        n_index * group_count + group_index
    } else if super::nomenclature::RULES
        .functional_groups
        .get(key)
        .is_some_and(|r| r.families.contains("hydrazone"))
    {
        group_index
    } else if group_count == 1 && n_count == 1 {
        0
    } else {
        global
    };
    format!("N{}", "'".repeat(primes))
}

pub fn add_component_n_substituents(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
    exclude: &AtomSet,
) -> Result<AtomSet, String> {
    let mut consumed = AtomSet::new();
    let Some(key) = key else {
        return Ok(consumed);
    };
    let mut selected: Vec<_> = groups
        .iter()
        .filter(|g| g.key == key && path.contains(&g.attachment_carbon))
        .collect();
    selected.sort_by_key(|g| parse_locant(&locants[&g.attachment_carbon]));
    let mut global = 0;
    for (gi, group) in selected.iter().enumerate() {
        let carbon = group.attachment_carbon;
        let Some(rule) = &group.resolved_rule else {
            continue;
        };
        let mut nitrogens: Vec<_> = group
            .atoms_involved
            .iter()
            .copied()
            .filter(|n| mol.atoms[n].symbol == "N")
            .collect();
        nitrogens.sort_by_key(|&n| mol.get_bond(n, carbon).is_none());
        if rule.has_capability("urea") {
            let ranks = nitrogens
                .iter()
                .map(|&n| nitrogen_rank(mol, n, carbon, exclude).map(|r| (n, r)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            nitrogens.sort_by_key(|n| ranks[n].clone());
        }
        let core = group
            .atoms_involved
            .iter()
            .copied()
            .find(|c| {
                mol.atoms[c].is_carbon() && nitrogens.iter().any(|&n| mol.get_bond(*c, n).is_some())
            })
            .unwrap_or(carbon);
        let mut guanidine = BTreeMap::new();
        if matches!(key, "amidine" | "ring_amidine") {
            nitrogens.sort_by_key(|&n| mol.bond_order(n, core));
        } else if rule.has_capability("hydrazide") {
            let centers = group
                .descriptor
                .as_ref()
                .map(|d| d.centers.as_slice())
                .unwrap_or(std::slice::from_ref(&core));
            nitrogens.sort_by_key(|&n| !centers.iter().any(|&c| mol.get_bond(n, c).is_some()));
        } else if key == "guanidine" {
            let imino = nitrogens
                .iter()
                .copied()
                .find(|&n| mol.get_bond(n, core).is_some_and(|b| b.order == 2))
                .ok_or("openclatura: guanidine imino nitrogen missing")?;
            let mut amines: Vec<_> = nitrogens.iter().copied().filter(|&n| n != imino).collect();
            let ranks = amines
                .iter()
                .map(|&n| nitrogen_rank(mol, n, core, exclude).map(|r| (n, r)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            amines.sort_by_key(|n| ranks[n].clone());
            if amines.len() != 2 {
                return Err("openclatura: guanidine topology not ported".into());
            }
            guanidine.extend([
                (amines[0], "1".into()),
                (imino, "2".into()),
                (amines[1], "3".into()),
            ]);
        }
        let hydrazone = rule.families.contains("hydrazone");
        for (ni, &n) in nitrogens.iter().enumerate() {
            let roots: Vec<_> = mol
                .get_neighbors(n)
                .iter()
                .copied()
                .filter(|r| {
                    *r != carbon
                        && !group.atoms_involved.contains(r)
                        && mol.atoms[r].symbol != "H"
                        && !(hydrazone
                            && mol.atoms[r].is_carbon()
                            && mol.get_bond(n, *r).is_some_and(|b| b.order == 2))
                })
                .collect();
            let loc = guanidine.get(&n).cloned().unwrap_or_else(|| {
                n_substituent_locant(key, selected.len(), nitrogens.len(), ni, global, gi)
            });
            for root in roots {
                let mut blocked = exclude.clone();
                blocked.insert(n);
                let mut rendered = name_subgraph_rendered(mol, root, &blocked, Some(n))?;
                if mol.get_bond(n, root).is_some_and(|b| b.order == 2)
                    && mol.atoms[&root].symbol == "N"
                    && mol
                        .get_neighbors(root)
                        .iter()
                        .all(|&r| r == n || mol.atoms[&r].symbol == "H")
                {
                    rendered.text = "imino".into();
                }
                let atoms = subgraph_component(mol, root, &blocked);
                let mut bond_atoms = atoms.clone();
                bond_atoms.insert(n);
                let mut item = SubstituentItem::new(rendered.text, vec![loc.clone()]);
                item.outer_parentheses_optional = rendered.outer_parentheses_optional;
                item.bond_ids = bond_ids_within(mol, &bond_atoms);
                item.atom_ids = atoms.clone();
                item.charge_atom_ids = atoms
                    .iter()
                    .copied()
                    .filter(|i| mol.atoms[i].charge != 0)
                    .collect();
                consumed.extend(atoms);
                if hydrazone
                    && parts
                        .parent_atom_symbols_by_locant
                        .values()
                        .any(|s| s == "N")
                {
                    item.locants.clear();
                    parts.principal_suffix_modifiers.push(item);
                } else {
                    super::subgraph_tools::add_substituent(parts, item);
                }
            }
            global += 1;
        }
    }
    Ok(consumed)
}
