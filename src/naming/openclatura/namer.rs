//! Recursive substituent naming, following the upstream plain-name pipeline.
use super::assembly_parts::{AssemblyParts, RenderedSubstituentName, SubstituentItem};
use super::assembly_prefixes::substituent_sort_key;
use super::chains::{find_all_carbon_paths, find_ring_systems, get_cyclic_atoms};
use super::component_group_rules::amide_nitrogen;
use super::component_namer::name_component;
use super::formatting::{
    format_center_ligands_by, is_complex_prefix, is_composite_prefix, strip_outer_parentheses,
};
use super::heteroatom_subgraphs::name_heteroatom_subgraph;
use super::locant_elision::apply_redundant_locant_elision;
use super::molecule::{Atom, AtomSet, Bond, Molecule, bond_ids_within, charged_atoms};
use super::nomenclature::RULES;
use super::parent_pipeline::{build_parent_assembly_plan, resolve_retained_parent};
use super::parent_selection::{ParentSelection, select_principal_parent};
use super::perception::{PerceivedGroup, perceive_groups};
use super::rules::{multipliers, stems};
use super::subgraph_tools::{
    add_indicated_hydrogens, add_parent_features, add_substituent, emit_bond_stereo,
    subgraph_component,
};
use std::collections::BTreeMap;

/// Name a branch from its attachment atom and an explicit parent boundary.
pub(super) fn name_subgraph_with_upstream(
    mol: &Molecule,
    start_idx: usize,
    exclude_atoms: &AtomSet,
    upstream_atom: Option<usize>,
) -> Result<String, String> {
    name_subgraph_rendered(mol, start_idx, exclude_atoms, upstream_atom).map(|n| n.text)
}

fn direct_amide_prefix(
    mol: &Molecule,
    group: &PerceivedGroup,
    component: &AtomSet,
) -> Result<Option<String>, String> {
    let Some(nitrogen) = amide_nitrogen(mol, group) else {
        return Ok(None);
    };
    let Some(base) = RULES.functional_groups.prefix_for(&group.key) else {
        return Ok(None);
    };
    let roots: Vec<_> = mol
        .get_neighbors(nitrogen)
        .iter()
        .copied()
        .filter(|n| {
            component.contains(n) && !group.atoms_involved.contains(n) && mol.atoms[n].symbol != "H"
        })
        .collect();
    if roots.is_empty() {
        return Ok(Some(base.into()));
    }
    let mut exclude: AtomSet = mol
        .atoms
        .keys()
        .copied()
        .filter(|n| !component.contains(n))
        .collect();
    exclude.extend(&group.atoms_involved);
    exclude.insert(nitrogen);
    let names = roots
        .iter()
        .map(|&n| {
            name_subgraph_with_upstream(mol, n, &exclude, Some(nitrogen))
                .map(|s| strip_outer_parentheses(&s))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(format!(
        "({}{base})",
        format_center_ligands_by(&names, substituent_sort_key)
    )))
}

fn direct_subgraph_prefix(
    mol: &Molecule,
    start: usize,
    component: &AtomSet,
) -> Result<Option<String>, String> {
    if mol.atoms[&start].is_carbon() {
        let terminal: Vec<_> = mol
            .get_neighbors(start)
            .iter()
            .copied()
            .filter(|n| {
                component.contains(n)
                    && mol.atoms[n].symbol == "S"
                    && mol.get_bond(start, *n).is_some_and(|b| b.order == 2)
                    && mol.get_neighbors(*n).iter().all(|n| *n == start)
            })
            .collect();
        if terminal.len() == 1 && component == &AtomSet::from([start, terminal[0]]) {
            mol.mark_name(crate::naming::NameStatus::NotPreferred(
                crate::naming::NameReason::ChalcogenPrefix,
            ));
            return Ok(Some("thioformyl".into()));
        }
        let phosphorus: Vec<_> = mol
            .get_neighbors(start)
            .iter()
            .copied()
            .filter(|n| {
                component.contains(n)
                    && mol.atoms[n].symbol == "P"
                    && mol.get_bond(start, *n).is_some_and(|b| b.order == 3)
            })
            .collect();
        if phosphorus.len() == 1 {
            let p = phosphorus[0];
            let oxo: Vec<_> = mol
                .get_neighbors(p)
                .iter()
                .copied()
                .filter(|n| {
                    *n != start
                        && component.contains(n)
                        && mol.atoms[n].symbol == "O"
                        && mol.get_bond(p, *n).is_some_and(|b| b.order == 2)
                        && mol.get_neighbors(*n).iter().all(|n| *n == p)
                })
                .collect();
            if component == &AtomSet::from([start, p]) {
                return Ok(Some("phosphanylidynemethyl".into()));
            }
            if oxo.len() == 1 && component == &AtomSet::from([start, p, oxo[0]]) {
                return Ok(Some("oxophosphanylidynemethyl".into()));
            }
        }
    }
    for group in perceive_groups(mol) {
        if !group.atoms_involved.contains(&start) || !group.atoms_involved.is_subset(component) {
            continue;
        }
        if matches!(
            group.key.as_str(),
            "hydrazide" | "ring_hydrazide" | "amidine" | "ring_amidine"
        ) {
            continue;
        }
        if RULES.prefixes.amide_like_groups.contains(&group.key)
            && let Some(name) = direct_amide_prefix(mol, &group, component)?
        {
            return Ok(Some(name));
        }
        if let Some(prefix) = RULES
            .functional_groups
            .direct_subgraph_prefix_for(&group.key)
        {
            super::functional_prefixes::mark_prefix_status(mol, &group.key);
            return Ok(Some(prefix.into()));
        }
    }
    Ok(None)
}

pub(super) fn select_subgraph_parent(
    mol: &Molecule,
    start: usize,
    component: &AtomSet,
    exclude: &AtomSet,
) -> Result<ParentSelection, String> {
    let cyclic = get_cyclic_atoms(mol, exclude);
    if cyclic.contains(&start) {
        let rings = find_ring_systems(mol, exclude)?
            .into_iter()
            .filter(|r| r.atoms.contains(&start))
            .collect::<Vec<_>>();
        return select_principal_parent(mol, &[], &rings, &[start])
            .ok_or_else(|| "openclatura: no recursive ring parent".into());
    }
    let paths = find_all_carbon_paths(mol, exclude)?
        .into_iter()
        .filter(|p| {
            p.contains(&start)
                && p.iter()
                    .all(|n| component.contains(n) && !cyclic.contains(n))
        })
        .collect::<Vec<_>>();
    select_principal_parent(mol, &paths, &[], &[start])
        .ok_or_else(|| "openclatura: no recursive chain parent".into())
}

fn collect_subgraph_substituents(
    mol: &Molecule,
    parent_path: &[usize],
    perceived: &[PerceivedGroup],
    exclude: &AtomSet,
) -> Result<BTreeMap<usize, Vec<SubstituentItem>>, String> {
    let parent: AtomSet = parent_path.iter().copied().collect();
    let mut handled = AtomSet::new();
    let mut mapping: BTreeMap<usize, Vec<SubstituentItem>> = BTreeMap::new();
    for group in perceived {
        if !group.atoms_involved.is_disjoint(exclude)
            || !parent.contains(&group.attachment_carbon)
            || group.is_principal_candidate
        {
            continue;
        }
        let rule = RULES.functional_groups.get(&group.key);
        let Some(name) = rule
            .filter(|r| r.role == "prefix")
            .and_then(|r| r.prefix.as_ref())
        else {
            continue;
        };
        let name = if group.variant.as_deref() == Some("substituted_heteroatom_imino") {
            let context = super::functional_prefixes::PrefixContext {
                mol,
                parent_path: &[],
                sub_exclude: exclude,
            };
            super::functional_prefixes::prefix_from_group(&context, group)?
        } else {
            super::functional_prefixes::mark_prefix_status(mol, &group.key);
            name.clone()
        };
        let mut item = SubstituentItem::new(name, Vec::new());
        item.atom_ids = group.atoms_involved.clone();
        let mut atoms = item.atom_ids.clone();
        atoms.insert(group.attachment_carbon);
        item.bond_ids = bond_ids_within(mol, &atoms);
        item.charge_atom_ids = charged_atoms(mol, &item.atom_ids);
        mapping
            .entry(group.attachment_carbon)
            .or_default()
            .push(item);
        handled.extend(&group.atoms_involved);
    }
    for &idx in parent_path {
        let neighbors: Vec<_> = mol
            .get_neighbors(idx)
            .iter()
            .copied()
            .filter(|n| !parent.contains(n) && !handled.contains(n) && !exclude.contains(n))
            .collect();
        if let Some((start, _)) =
            super::spiro_subgraph::find_spiro_side_pair(mol, idx, &neighbors, &parent, exclude)
        {
            let side =
                super::spiro_subgraph::spiro_side_component(mol, idx, start, &parent, exclude);
            let atoms: AtomSet = side.iter().copied().filter(|a| *a != idx).collect();
            let mut item = SubstituentItem::new("", Vec::new());
            item.atom_ids = atoms.clone();
            item.bond_ids = bond_ids_within(mol, &side);
            item.charge_atom_ids = charged_atoms(mol, &atoms);
            item.spiro = Some(Box::new(super::spiro_subgraph::spiro_subgraph_assembly(
                mol, idx, &side,
            )?));
            mapping.entry(idx).or_default().push(item);
            handled.extend(atoms);
        }
        for &n in mol.get_neighbors(idx) {
            if parent.contains(&n)
                || handled.contains(&n)
                || exclude.contains(&n)
                || mol.atoms[&n].symbol == "H"
            {
                continue;
            }
            let mut branch_exclude = exclude.clone();
            branch_exclude.extend(&parent);
            let atoms = subgraph_component(mol, n, &branch_exclude);
            super::spiro_subgraph::require_single_parent_attachment(mol, &atoms, &parent)?;
            let name = name_subgraph_rendered(mol, n, &branch_exclude, Some(idx))?;
            if name.text.is_empty() {
                return Err("openclatura: empty recursive substituent".into());
            }
            let mut attached = atoms.clone();
            attached.insert(idx);
            let mut item = SubstituentItem::new(name.text, Vec::new());
            item.atom_ids = atoms.clone();
            item.bond_ids = bond_ids_within(mol, &attached);
            item.charge_atom_ids = charged_atoms(mol, &atoms);
            item.outer_parentheses_optional = name.outer_parentheses_optional;
            mapping.entry(idx).or_default().push(item);
            handled.extend(atoms);
        }
    }
    Ok(mapping)
}

fn finalize_subgraph_name(mut name: String, parts: &AssemblyParts) -> String {
    if parts
        .principal_group
        .as_ref()
        .is_some_and(|g| matches!(g.key.as_str(), "nitrile" | "ring_nitrile"))
    {
        name = name.replace("nitrilo", "cyano");
    }
    if name == "phenyl" && parts.substituents.is_empty() {
        return name;
    }
    if ["yl", "ylidene", "ylidyne"]
        .iter()
        .any(|s| name.ends_with(s))
        && parts.substituents.is_empty()
        && parts.attachment_locant == "1"
        && (parts.unsaturations.is_empty()
            || !(is_complex_prefix(&name) || is_composite_prefix(&name)))
        && !["bicyclo", "spiro", "tricyclo"]
            .iter()
            .any(|s| name.starts_with(s))
    {
        return name;
    }
    format!("({name})")
}

pub(super) fn name_subgraph_rendered(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<RenderedSubstituentName, String> {
    let _depth = mol.enter_naming()?;
    let atom = mol
        .atoms
        .get(&start)
        .ok_or("openclatura: invalid substituent attachment atom")?;
    let component = subgraph_component(mol, start, exclude);
    if component.is_empty() {
        return Err("openclatura: excluded substituent attachment atom".into());
    }
    let cyclic = get_cyclic_atoms(mol, exclude);
    if !atom.is_carbon() && !cyclic.contains(&start) {
        let name = if let Some(name) = acylamino_amido_prefix(mol, start, exclude, upstream)? {
            Some(name)
        } else {
            name_heteroatom_subgraph(mol, start, exclude, upstream)?
        };
        if let Some(text) = name {
            return Ok(RenderedSubstituentName {
                text,
                outer_parentheses_optional: false,
            });
        }
    }
    if let Some(text) = direct_subgraph_prefix(mol, start, &component)? {
        return Ok(RenderedSubstituentName {
            text,
            outer_parentheses_optional: false,
        });
    }
    let sub_exclude: AtomSet = mol
        .atoms
        .keys()
        .copied()
        .filter(|n| !component.contains(n))
        .collect();
    let selection = select_subgraph_parent(mol, start, &component, &sub_exclude)?;
    let retained = resolve_retained_parent(mol, &selection);
    let (mut retained_name, mut maps) = retained
        .map(|r| (Some(r.name), r.locant_maps))
        .unwrap_or_default();
    let perceived = perceive_groups(mol);
    let mapping =
        collect_subgraph_substituents(mol, selection.primary_path(), &perceived, &sub_exclude)?;
    let retained_fused = super::retained_fused_production::production_retained_fused_parent(
        mol,
        selection.primary_path(),
        &component,
        &perceived,
        None,
        &mapping,
        Some(start),
    )?;
    let mut retained_metadata = None;
    if let Some(retained) = retained_fused {
        retained_name = Some(retained.name);
        maps = retained.locant_maps;
        retained_metadata = Some(retained.metadata);
    }
    let plan = build_parent_assembly_plan(
        mol,
        &selection,
        &[start],
        Some(start),
        upstream,
        &mapping,
        retained_name,
        &maps,
    )?;
    let mut parts = plan.parts;
    if let Some(metadata) = retained_metadata {
        parts.retained_parent_metadata = Some(metadata);
    }
    emit_bond_stereo(
        mol,
        &mut parts,
        &plan.numbered_path,
        &plan.locant_map,
        upstream,
    );
    add_indicated_hydrogens(mol, &mut parts, &plan.numbered_path, &plan.locant_map)?;
    for (&idx, items) in &mapping {
        for item in items {
            let mut item = item.clone();
            item.locants = vec![plan.locant_map[&idx].clone()];
            add_substituent(&mut parts, item);
        }
    }
    add_parent_features(mol, &mut parts, &plan.numbered_path, &plan.locant_map)?;
    apply_redundant_locant_elision(&mut parts);
    let raw = super::assembler::assemble_name_raw(&mut parts)?;
    let carbanion = simple_rooted_carbanion(mol, &parts, &plan.numbered_path, &plan.locant_map);
    let mut represented: AtomSet = parts
        .a_prefixes
        .iter()
        .flat_map(|p| p.charge_atom_ids.iter().copied())
        .collect();
    if carbanion.is_some() {
        represented.extend(
            plan.numbered_path
                .iter()
                .copied()
                .filter(|i| mol.atoms[i].charge == -1),
        );
    }
    let raw = carbanion.unwrap_or(raw);
    let text = finalize_subgraph_name(raw, &parts);
    let text = if parts
        .parent_hydride
        .as_ref()
        .is_some_and(|p| p.is_fusion_parent())
    {
        text
    } else {
        super::ionic_naming::apply_anionic_parent_names(
            &text,
            mol,
            &plan.numbered_path,
            &plan.locant_map,
            parts.retained_name.as_deref(),
            &represented,
        )?
    };
    let text = super::ionic_naming::apply_cationic_imino_parent_prefixes(
        &text,
        mol,
        &plan.numbered_path,
        &plan.locant_map,
    )?;
    let text = super::ionic_naming::apply_cationic_imino_names(&text, mol)?;
    let text = super::assembler::post_process_name(&text);
    super::assembler::mark_assembled_name(mol, &parts, &text);
    Ok(RenderedSubstituentName {
        text,
        outer_parentheses_optional: !parts.stereo_features.is_empty(),
    })
}

fn simple_rooted_carbanion(
    mol: &Molecule,
    parts: &AssemblyParts,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Option<String> {
    if !parts.substituents.is_empty()
        || parts.principal_group.is_some()
        || !parts.unsaturations.is_empty()
        || path.iter().any(|n| !mol.atoms[n].is_carbon())
    {
        return None;
    }
    let charged: Vec<_> = path.iter().filter(|n| mol.atoms[n].charge == -1).collect();
    if charged.len() != 1 || locants[charged[0]] != "1" {
        return None;
    }
    if path
        .windows(2)
        .any(|pair| mol.get_bond(pair[0], pair[1]).is_some_and(|b| b.order != 1))
    {
        return None;
    }
    let kind = match parts.attachment_order {
        3 => "triple",
        2 => "double",
        _ => "single",
    };
    let prefix = RULES
        .charges
        .heteroatom_charge_prefixes
        .get(&format!("C:-:{kind}"))?;
    Some(if path.len() == 1 {
        prefix.clone()
    } else {
        format!("{}yl{prefix}", stems::stem_for(path.len() - 1))
    })
}

fn acid_name_to_amido(acid: &str) -> Option<String> {
    if ["di", "tri", "tetra"].iter().any(|m| {
        ["oic acid", "carboxylic acid"]
            .iter()
            .any(|s| acid.ends_with(&format!("{m}{s}")))
    }) {
        return None;
    }
    for (suffix, replacement) in [
        ("carboxylic acid", "carboxamido"),
        ("sulfonic acid", "sulfonamido"),
        ("sulfinic acid", "sulfinamido"),
        ("oic acid", "amido"),
        ("ic acid", "amido"),
    ] {
        if let Some(stem) = acid.strip_suffix(suffix) {
            return Some(format!("{stem}{replacement}"));
        }
    }
    None
}

fn acylamino_amido_prefix(
    mol: &Molecule,
    nitrogen: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<Option<String>, String> {
    let atom = &mol.atoms[&nitrogen];
    if atom.symbol != "N" || atom.charge != 0 || atom.is_aromatic {
        return Ok(None);
    }
    let Some(upstream) = upstream else {
        return Ok(None);
    };
    if mol.bond_order(nitrogen, upstream) != 1 {
        return Ok(None);
    }
    let neighbors: Vec<_> = mol
        .get_neighbors(nitrogen)
        .iter()
        .copied()
        .filter(|n| !exclude.contains(n) && *n != upstream)
        .collect();
    let centers: Vec<_> = neighbors
        .iter()
        .copied()
        .filter(|&c| {
            if mol.bond_order(nitrogen, c) != 1 {
                return false;
            }
            let oxo: Vec<_> = mol
                .get_neighbors(c)
                .iter()
                .copied()
                .filter(|n| {
                    mol.atoms[n].symbol == "O"
                        && mol.degree(*n) == 1
                        && (mol.get_bond(c, *n).is_some_and(|bond| bond.order == 2)
                            || (mol.atoms[n].charge == -1 && mol.atoms[&c].charge > 0))
                })
                .collect();
            if !((mol.atoms[&c].is_carbon() && oxo.len() == 1)
                || (mol.atoms[&c].symbol == "S" && oxo.len() == 2))
            {
                return false;
            }
            let rest: Vec<_> = mol
                .get_neighbors(c)
                .iter()
                .filter(|n| **n != nitrogen && !oxo.contains(n))
                .collect();
            rest.len() <= 1 && rest.iter().all(|n| mol.atoms[n].is_carbon())
        })
        .collect();
    let &[center] = centers.as_slice() else {
        return Ok(None);
    };
    let mut blocked = exclude.clone();
    blocked.insert(nitrogen);
    let acid_atoms = subgraph_component(mol, center, &blocked);
    // Only a competing acid reachable without crossing an ether can take the suffix.
    // Upstream divergence: a sulfonic/sulfinic acid competes as well as a
    // carboxylic acid. Otherwise acid-name conversion can move the amido
    // attachment to a different sulfur centre and change connectivity.
    let mut seen = AtomSet::from([center]);
    let mut queue = vec![center];
    while let Some(a) = queue.pop() {
        for &n in mol.get_neighbors(a) {
            if !acid_atoms.contains(&n) || (mol.atoms[&n].symbol == "O" && mol.degree(n) > 1) {
                continue;
            }
            if seen.insert(n) {
                queue.push(n);
            }
        }
    }
    if seen.iter().any(|&c| {
        c != center
            && (mol.atoms[&c].is_carbon() || mol.atoms[&c].symbol == "S")
            && mol.get_neighbors(c).iter().any(|&n| {
                mol.atoms[&n].symbol == "O"
                    && mol.get_bond(c, n).is_some_and(|bond| bond.order == 2)
            })
            && mol.get_neighbors(c).iter().any(|&n| {
                mol.atoms[&n].symbol == "O"
                    && mol.degree(n) == 1
                    && mol.get_bond(c, n).is_some_and(|bond| bond.order == 1)
            })
    }) {
        return Ok(None);
    }
    let mut acid_mol = mol.subgraph(&acid_atoms);
    if acid_mol.atoms[&center].charge > 0
        && let Some((oxygen, bond_id)) = acid_mol.get_neighbors(center).iter().find_map(|&n| {
            let bond = acid_mol.get_bond(center, n)?;
            (acid_mol.atoms[&n].symbol == "O"
                && acid_mol.atoms[&n].charge == -1
                && acid_mol.degree(n) == 1
                && bond.order == 1)
                .then_some((n, bond.idx))
        })
    {
        let charge = acid_mol.atoms[&center].charge;
        acid_mol.set_atom_charge(oxygen, 0)?;
        acid_mol.set_bond_order(bond_id, 2)?;
        acid_mol.set_atom_charge(center, charge - 1)?;
    }
    let oxygen = mol.atoms.keys().next_back().copied().unwrap_or(0) + 1;
    let mut cap = Atom::new(oxygen, "O");
    cap.total_h_count = 1;
    acid_mol.add_atom(cap)?;
    let bond_id = mol.bonds.keys().next_back().copied().unwrap_or(0) + 1;
    acid_mol.add_bond(Bond::new(bond_id, center, oxygen, 1))?;
    let mut capped = acid_atoms.clone();
    capped.insert(oxygen);
    let status = mol.name_status();
    let acid_result = name_component(&acid_mol, &capped);
    // Upstream divergence: a speculative acid candidate cannot hide exhausted naming resources.
    mol.spend_naming_work(0)?;
    let Ok(acid_name) = acid_result else {
        mol.set_name_status(status);
        return Ok(None);
    };
    let Some(mut amido) = RULES
        .heteroatoms
        .retained_sulfonyl_group_acids
        .get(&acid_name)
        .map(|s| {
            if s == "tosyl" {
                mol.mark_name(crate::naming::NameStatus::NotPreferred(
                    crate::naming::NameReason::SulfonylPrefix,
                ));
            }
            format!("{s}amino")
        })
        .or_else(|| acid_name_to_amido(&acid_name))
    else {
        mol.set_name_status(status);
        return Ok(None);
    };
    let n_sub: Vec<_> = neighbors.into_iter().filter(|n| *n != center).collect();
    if !n_sub.is_empty() {
        let mut ex = exclude.clone();
        ex.insert(nitrogen);
        ex.extend(&acid_atoms);
        let mut counts = BTreeMap::<String, usize>::new();
        for n in n_sub {
            let name =
                strip_outer_parentheses(&name_subgraph_with_upstream(mol, n, &ex, Some(nitrogen))?);
            *counts.entry(name).or_default() += 1;
        }
        let mut prefixes = Vec::new();
        for (name, count) in counts {
            let display = if is_complex_prefix(&name) || is_composite_prefix(&name) {
                format!("({name})")
            } else {
                name.clone()
            };
            let mult = if count == 1 {
                String::new()
            } else if is_complex_prefix(&name) {
                multipliers::complex_(count)
            } else {
                multipliers::basic(count)
            };
            prefixes.push(format!("{}-{mult}{display}", vec!["N"; count].join(",")));
        }
        let prefix = prefixes.join("-");
        amido = format!(
            "{prefix}{}{amido}",
            if amido.starts_with(|c: char| c.is_ascii_digit()) {
                "-"
            } else {
                ""
            }
        );
    }
    Ok(Some(if is_complex_prefix(&amido) {
        format!("({amido})")
    } else {
        amido
    }))
}

#[cfg(test)]
mod tests {
    use super::name_subgraph_with_upstream;
    use crate::naming::openclatura::{graph_io::read_smiles, molecule::AtomSet};

    #[test]
    fn public_naming_bounds_recursion_on_a_two_mebibyte_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                use crate::naming::openclatura::name_smiles;

                assert_eq!(
                    name_smiles(&format!("{}C", "CO".repeat(100))).map(|name| name.text),
                    Err("openclatura: naming complexity limit exceeded".into())
                );
                assert!(name_smiles("COCOCOC").is_ok());
                assert_eq!(
                    name_smiles("CCO").map(|name| name.text),
                    Ok("ethanol".into())
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn recursive_substituent_names_preserve_attachment_and_boundary() -> Result<(), String> {
        let methyl = read_smiles("C")?;
        assert_eq!(
            name_subgraph_with_upstream(&methyl, 0, &AtomSet::new(), None)?,
            "methyl"
        );
        for (smiles, start, expected) in [
            ("CC(C)C", 1, "(propan-2-yl)"),
            ("CC=C", 1, "ethenyl"),
            ("CC#C", 1, "ethynyl"),
            ("Cc1ccccc1", 1, "phenyl"),
            ("CCc1ccccc1", 1, "benzyl"),
            ("CC#N", 1, "cyano"),
            ("CC(=O)O", 1, "carboxy"),
            ("CC(=O)N(C)C", 1, "(dimethylcarbamoyl)"),
            ("NC=S", 1, "thioformyl"),
            ("NC(=O)CCC(=O)O", 1, "(4-hydroxy-1,4-dioxobutyl)"),
            ("COC", 1, "methoxy"),
            ("CN(C)C", 1, "(dimethylamino)"),
        ] {
            let mol = read_smiles(smiles)?;
            assert_eq!(
                name_subgraph_with_upstream(&mol, start, &AtomSet::from([0]), Some(0))?,
                expected,
                "{smiles}"
            );
        }
        Ok(())
    }

    #[test]
    fn disjoint_ring_branches_keep_recursive_parent_and_attachment_names() {
        use crate::naming::openclatura::name_smiles;

        for (smiles, expected) in [
            ("C1=CC=NC(=C1)C2=CC=CC=N2", "2-(pyridin-2-yl)pyridine"),
            ("C1CCC(CC1)C2=CC=CC=C2", "cyclohexylbenzene"),
            (
                "C1=CC=C(C=C1)C2=CC=C(C=C2)C3=CC=CC=C3",
                "1,4-diphenylbenzene",
            ),
            (
                "C1=CC=C(C=C1)C2=CC(=CC=C2)C3=CC=CC=C3",
                "1,3-diphenylbenzene",
            ),
            (
                "CN(C)CCOC(C1=CC=CC=C1)C2=CC=CC=C2",
                "2-[(diphenylmethyl)oxy]-N,N-dimethylethan-1-amine",
            ),
            (
                "c1ccc(-c2cccc(Nc3cnccn3)n2)cc1",
                "N-(6-phenylpyridin-2-yl)pyrazin-2-amine",
            ),
        ] {
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn ring_branches_preserve_bond_and_atom_stereochemistry() {
        use crate::naming::openclatura::name_smiles;

        for (smiles, expected) in [
            (
                "C1=CC=C(C=C1)/C=C/C2=CC=CC=C2",
                "(1E)-2-phenylethenylbenzene",
            ),
            (
                "CN1CCC[C@H]1C2=CN=CC=C2",
                "3-[(2S)-1-methylpyrrolidin-2-yl]pyridine",
            ),
        ] {
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }
}
