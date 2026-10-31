//! Recursive substituent naming, following the upstream plain-name pipeline.
use super::assembly_parts::{AssemblyParts, RenderedSubstituentName, SubstituentItem};
use super::assembly_prefixes::substituent_sort_key;
use super::chains::{find_all_carbon_paths, find_ring_systems, get_cyclic_atoms};
use super::formatting::{
    format_center_ligands_by, is_complex_prefix, is_composite_prefix, strip_outer_parentheses,
};
use super::group_atom_roles::amide_nitrogen;
use super::heteroatom_subgraphs::name_heteroatom_subgraph;
use super::locant_elision::apply_redundant_locant_elision;
use super::molecule::{Atom, AtomSet, Bond, Molecule, bond_ids_within, charged_atoms};
use super::nomenclature::RULES;
use super::parent_pipeline::build_parent_assembly_plan;
use super::parent_selection::{ParentSelection, select_principal_parent};
use super::perception::{PerceivedGroup, perceive_groups};
use super::rules::{multipliers, retained, stems};
use super::subgraph_tools::{
    add_indicated_hydrogens, add_parent_features, add_substituent, emit_bond_stereo,
    subgraph_component,
};
use std::collections::BTreeMap;

pub fn name_component(mol: &Molecule, component_atoms: &AtomSet) -> Result<String, String> {
    super::component_namer::name_component(mol, component_atoms)
}

/// Name a branch from its attachment atom and an explicit parent boundary.
pub fn name_subgraph_with_upstream(
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
            return Ok(Some(prefix.into()));
        }
    }
    Ok(None)
}

fn select_subgraph_parent(
    mol: &Molecule,
    start: usize,
    component: &AtomSet,
    exclude: &AtomSet,
) -> Result<ParentSelection, String> {
    let cyclic = get_cyclic_atoms(mol, exclude);
    if cyclic.contains(&start) {
        let rings = find_ring_systems(mol, exclude)
            .into_iter()
            .filter(|r| r.atoms.contains(&start))
            .collect::<Vec<_>>();
        return select_principal_parent(mol, &[], &rings, &[start])
            .ok_or_else(|| "openclatura: no recursive ring parent".into());
    }
    let paths = find_all_carbon_paths(mol, exclude)
        .into_iter()
        .filter(|p| {
            p.contains(&start)
                && p.iter()
                    .all(|n| component.contains(n) && !cyclic.contains(n))
        })
        .collect::<Vec<_>>();
    select_principal_parent(mol, &paths, &[], &[start])
        .map(|s| s.with_fixed_start(false))
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
            let name = name_subgraph_rendered(mol, n, &branch_exclude, Some(idx))?;
            if name.text.is_empty() {
                return Err("openclatura: empty recursive substituent".into());
            }
            let atoms = subgraph_component(mol, n, &branch_exclude);
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

pub fn name_subgraph_rendered(
    mol: &Molecule,
    start: usize,
    exclude: &AtomSet,
    upstream: Option<usize>,
) -> Result<RenderedSubstituentName, String> {
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
    let retained = if selection.is_ring {
        retained::get_retained_ring(mol, selection.primary_path())
    } else {
        None
    };
    let (retained_name, maps) = retained
        .map(|r| (Some(r.name), r.locant_maps))
        .unwrap_or_default();
    let perceived = perceive_groups(mol);
    let mapping =
        collect_subgraph_substituents(mol, selection.primary_path(), &perceived, &sub_exclude)?;
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
    emit_bond_stereo(
        mol,
        &mut parts,
        &plan.numbered_path,
        &plan.locant_map,
        upstream,
    );
    add_indicated_hydrogens(mol, &mut parts, &plan.numbered_path, &plan.locant_map);
    for (&idx, items) in &mapping {
        for item in items {
            let mut item = item.clone();
            item.locants = vec![plan.locant_map[&idx].clone()];
            add_substituent(&mut parts, item);
        }
    }
    add_parent_features(mol, &mut parts, &plan.numbered_path, &plan.locant_map);
    apply_redundant_locant_elision(&mut parts);
    let raw = super::assembler::assemble_name_raw(&mut parts)?;
    let raw =
        simple_rooted_carbanion(mol, &parts, &plan.numbered_path, &plan.locant_map).unwrap_or(raw);
    let text = finalize_subgraph_name(raw, &parts);
    let text = super::ionic_naming::apply_anionic_parent_names(
        &text,
        mol,
        &plan.numbered_path,
        &plan.locant_map,
        parts.retained_name.as_deref(),
    )?;
    let text = super::ionic_naming::apply_cationic_imino_parent_prefixes(
        &text,
        mol,
        &plan.numbered_path,
        &plan.locant_map,
    )?;
    let text = super::ionic_naming::apply_cationic_imino_names(&text, mol)?;
    let text = super::assembler::post_process_name(&text);
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
    let kind = if parts.is_triple_attach {
        "triple"
    } else if parts.is_double_attach {
        "double"
    } else {
        "single"
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
    if atom.symbol != "N"
        || atom.charge != 0
        || atom.is_aromatic
        || upstream.is_none()
        || upstream
            .and_then(|u| mol.get_bond(nitrogen, u))
            .is_none_or(|b| b.order != 1)
    {
        return Ok(None);
    }
    let neighbors: Vec<_> = mol
        .get_neighbors(nitrogen)
        .iter()
        .copied()
        .filter(|n| !exclude.contains(n) && Some(*n) != upstream)
        .collect();
    let centers: Vec<_> = neighbors
        .iter()
        .copied()
        .filter(|&c| {
            if mol.get_bond(nitrogen, c).is_none_or(|b| b.order != 1) {
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
    if centers.len() != 1 {
        return Ok(None);
    }
    let center = centers[0];
    let mut blocked = exclude.clone();
    blocked.insert(nitrogen);
    let acid_atoms = subgraph_component(mol, center, &blocked);
    // Only a competing acid reachable without crossing an ether can take the suffix.
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
            && mol.atoms[&c].is_carbon()
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
    let Ok(acid_name) = name_component(&acid_mol, &capped) else {
        return Ok(None);
    };
    let Some(mut amido) = RULES
        .heteroatoms
        .retained_sulfonyl_group_acids
        .get(&acid_name)
        .map(|s| format!("{s}amino"))
        .or_else(|| acid_name_to_amido(&acid_name))
    else {
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
}
