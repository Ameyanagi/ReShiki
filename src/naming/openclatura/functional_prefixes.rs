//! Characteristic groups cited as prefixes, in upstream registry order.
use super::{
    assembly_parts::SubstituentItem,
    assembly_prefixes::substituent_sort_key,
    component_group_rules::{amide_nitrogen, ester_or_peroxy_single_oxygen},
    formatting::{
        format_center_ligands_by, format_multiplier, oxy_prefix_from_branch,
        strip_outer_parentheses,
    },
    molecule::{AtomSet, Molecule, bond_ids_within},
    namer::name_subgraph_with_upstream,
    nomenclature::RULES,
    perception::{LigandRole, PerceivedGroup},
    subgraph_tools::subgraph_component,
};
use std::collections::BTreeMap;

pub struct PrefixContext<'a> {
    pub mol: &'a Molecule,
    pub parent_path: &'a [usize],
    pub sub_exclude: &'a AtomSet,
}

fn chalcogen_term(element: &str, field: &str) -> Result<&'static str, String> {
    super::nomenclature::section("chalcogen_elements")[element][field]
        .as_str()
        .ok_or_else(|| format!("openclatura: {element} {field} not ported"))
}

fn element_ligand(branch: &str, element: &str) -> Result<String, String> {
    // Upstream divergence: amidooxy contractions change the attachment of N-acyl aminooxy ligands.
    let plain = strip_outer_parentheses(branch);
    if plain.ends_with("amido") && !super::formatting::is_complex_prefix(&plain) {
        return Err("openclatura: amido chalcogen-ligand attachment is not supported".into());
    }
    Ok(super::formatting::format_element_substituent(
        "",
        branch,
        chalcogen_term(element, "substituent_suffix")?,
        false,
    ))
}

fn central_linkage_prefix(group: &PerceivedGroup) -> Option<&'static str> {
    let descriptor = group.descriptor.as_ref()?;
    let element = descriptor.central_element.as_deref()?;
    let count = descriptor.ligands_with_role(LigandRole::DoubleBonded).len();
    super::nomenclature::section("chalcogen_central_acid_origins")
        .as_array()?
        .iter()
        .find(|r| {
            r["element"].as_str() == Some(element)
                && r["double_bond_count"].as_u64() == Some(count as u64)
        })?["linkage_prefix"]
        .as_str()
}

fn descriptor_ester_prefix(
    context: &PrefixContext<'_>,
    group: &PerceivedGroup,
) -> Result<Option<String>, String> {
    let Some(d) = &group.descriptor else {
        return Ok(None);
    };
    let doubles = d.ligands_with_role(LigandRole::DoubleBonded);
    let linkers: Vec<_> = d
        .ligands
        .iter()
        .filter(|l| l.role != LigandRole::DoubleBonded)
        .collect();
    let Some(terminal) = linkers.last() else {
        return Ok(None);
    };
    let Some(root) = terminal.attachment_atom else {
        return Ok(None);
    };
    if doubles.len() != 1 && d.family != "central_acid" {
        return Ok(None);
    }
    let mut blocked = context.sub_exclude.clone();
    blocked.insert(terminal.atom);
    let branch = name_subgraph_with_upstream(context.mol, root, &blocked, Some(terminal.atom))?;
    let mut ligand = element_ligand(&branch, &terminal.element)?;
    for linker in linkers[..linkers.len() - 1].iter().rev() {
        ligand = element_ligand(&ligand, &linker.element)?;
    }
    let base = if d.family == "central_acid" {
        central_linkage_prefix(group).unwrap_or("")
    } else {
        chalcogen_term(&doubles[0].element, "acyl_center_prefix")?
    };
    Ok(Some(format!(
        "({}{base})",
        strip_outer_parentheses(&ligand)
    )))
}

fn central_chalcogen_linkage_prefix(
    context: &PrefixContext<'_>,
    group: &PerceivedGroup,
) -> Result<String, String> {
    let d = group
        .descriptor
        .as_ref()
        .ok_or("openclatura: missing central linkage descriptor")?;
    let bridge: Vec<_> = d
        .ligands
        .iter()
        .filter(|l| l.role != LigandRole::DoubleBonded)
        .collect();
    let Some(base) = central_linkage_prefix(group) else {
        return Ok(group.prefix().unwrap_or("").into());
    };
    if bridge.len() != 2 {
        return Ok(group.prefix().unwrap_or("").into());
    }
    let terminal = bridge[1];
    let terminal_text = if terminal.role == LigandRole::HydrogenBearing {
        let key = super::nomenclature::section("chalcogen_simple_group_keys")
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|r| {
                    r["family"].as_str() == Some("hydroxy")
                        && r["derivative"].as_str() == Some("alcohol")
                        && r["element"].as_str() == Some(terminal.element.as_str())
                })
            })
            .and_then(|r| r["key"].as_str());
        key.and_then(|k| RULES.functional_groups.cited_prefix_for(k))
            .unwrap_or_default()
    } else if matches!(
        terminal.role,
        LigandRole::CarbonLink | LigandRole::HeteroatomLink
    ) {
        if let Some(root) = terminal.attachment_atom {
            let mut blocked = context.sub_exclude.clone();
            blocked.insert(terminal.atom);
            element_ligand(
                &name_subgraph_with_upstream(context.mol, root, &blocked, Some(terminal.atom))?,
                &terminal.element,
            )?
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    if terminal_text.is_empty() {
        return Ok(group.prefix().unwrap_or("").into());
    }
    Ok(format!(
        "({}{base})",
        element_ligand(&terminal_text, &bridge[0].element)?
    ))
}

fn nitrogen_group_prefix(
    context: &PrefixContext<'_>,
    group: &PerceivedGroup,
) -> Result<String, String> {
    let mol = context.mol;
    let hydrazine = group.key == "hydrazine";
    let amidine = matches!(group.key.as_str(), "amidine" | "ring_amidine");
    let mut ns: Vec<_> = group
        .atoms_involved
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].symbol == "N")
        .collect();
    let core = group
        .atoms_involved
        .iter()
        .copied()
        .find(|i| mol.atoms[i].is_carbon() && ns.iter().any(|&n| mol.get_bond(*i, n).is_some()))
        .unwrap_or(group.attachment_carbon);
    ns.sort_by_key(|&n| {
        if amidine {
            mol.bond_order(n, core)
        } else {
            usize::from(mol.get_bond(n, core).is_none())
        }
    });
    let stereo = if amidine {
        ns.last()
            .and_then(|&n| mol.get_bond(n, core))
            .and_then(|b| b.stereo.as_ref())
            .map(|s| format!("({s})-"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let base = if hydrazine {
        "hydrazinyl"
    } else if amidine {
        "carbamimidoyl"
    } else {
        "hydrazinecarbonyl"
    };
    let mut names: Vec<(String, Vec<String>)> = Vec::new();
    for (index, &n) in ns.iter().take(2).enumerate() {
        let loc = if hydrazine || amidine {
            if index == 0 { "N" } else { "N'" }
        } else if index == 0 {
            "1"
        } else {
            "2"
        };
        for &root in mol.get_neighbors(n) {
            if root == core
                || group.atoms_involved.contains(&root)
                || mol.atoms[&root].symbol == "H"
            {
                continue;
            }
            let mut blocked = context.sub_exclude.clone();
            blocked.insert(n);
            let text = name_subgraph_with_upstream(mol, root, &blocked, Some(n))?;
            if let Some((_, locants)) = names.iter_mut().find(|(name, _)| name == &text) {
                locants.push(loc.into());
            } else {
                names.push((text, vec![loc.into()]));
            }
        }
    }
    if names.is_empty() {
        return Ok(if stereo.is_empty() {
            base.into()
        } else {
            format!("({stereo}{base})")
        });
    }
    if !hydrazine {
        names.sort_by_key(|(name, _)| substituent_sort_key(name));
    }
    let prefixes = names
        .into_iter()
        .map(|(name, locs)| {
            format!(
                "{}-{}",
                locs.join(","),
                format_multiplier(
                    &name,
                    locs.len(),
                    super::formatting::is_complex_prefix(&name)
                )
            )
        })
        .collect::<Vec<_>>()
        .join("-");
    let base = if !hydrazine && !amidine {
        "hydrazine-1-carbonyl"
    } else {
        base
    };
    Ok(format!("({stereo}{prefixes}{base})"))
}

pub fn ester_prefix_from_group(
    mol: &Molecule,
    group: &PerceivedGroup,
    exclude: &AtomSet,
    suffix: &str,
) -> Result<String, String> {
    let Some(oxygen) = ester_or_peroxy_single_oxygen(mol, group) else {
        return Ok(String::new());
    };
    let Some(&root) = mol
        .get_neighbors(oxygen)
        .iter()
        .find(|n| !group.atoms_involved.contains(n))
    else {
        return Ok(String::new());
    };
    let mut blocked = exclude.clone();
    blocked.insert(oxygen);
    let branch = name_subgraph_with_upstream(mol, root, &blocked, Some(oxygen))?;
    if RULES.prefixes.peroxy_ester_groups.contains(&group.key) {
        return Ok(format!(
            "({}peroxycarbonyl)",
            format_multiplier(&strip_outer_parentheses(&branch), 1, false)
        ));
    }
    Ok(format!("({}{suffix})", oxy_prefix_from_branch(&branch)))
}

pub fn amide_prefix_from_group(
    mol: &Molecule,
    group: &PerceivedGroup,
    exclude: &AtomSet,
) -> Result<String, String> {
    let Some(nitrogen) = amide_nitrogen(mol, group) else {
        return Ok(String::new());
    };
    let base = group
        .resolved_rule
        .as_ref()
        .and_then(|r| r.prefix.as_deref())
        .unwrap_or("");
    let mut blocked = exclude.clone();
    blocked.insert(nitrogen);
    let subs: Vec<_> = mol
        .get_neighbors(nitrogen)
        .iter()
        .filter(|n| !group.atoms_involved.contains(n) && mol.atoms[n].symbol != "H")
        .copied()
        .collect();
    if subs.is_empty() {
        return Ok(base.into());
    }
    let names = subs
        .into_iter()
        .map(|n| name_subgraph_with_upstream(mol, n, &blocked, Some(nitrogen)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!(
        "({}{base})",
        format_center_ligands_by(&names, substituent_sort_key)
    ))
}

pub fn prefix_from_group(
    context: &PrefixContext<'_>,
    group: &PerceivedGroup,
) -> Result<String, String> {
    let Some(rule) = group
        .resolved_rule
        .as_ref()
        .or_else(|| RULES.functional_groups.get(&group.key))
    else {
        return Ok(String::new());
    };
    let families = &rule.families;
    if RULES.functional_groups.get(&group.key).is_none()
        && let Some(d) = &group.descriptor
        && d.family == "central_acid"
    {
        if d.ligands
            .iter()
            .filter(|l| l.role != LigandRole::DoubleBonded)
            .count()
            == 2
        {
            return central_chalcogen_linkage_prefix(context, group);
        }
        if rule.has_capability("ester_like")
            && let Some(prefix) = descriptor_ester_prefix(context, group)?
        {
            return Ok(prefix);
        }
    }
    if families.contains("direct_prefix") {
        return Ok(RULES
            .functional_groups
            .cited_prefix_for(&group.key)
            .unwrap_or_default());
    }
    if matches!(
        group.key.as_str(),
        "hydrazide" | "ring_hydrazide" | "amidine" | "ring_amidine" | "hydrazine"
    ) {
        return nitrogen_group_prefix(context, group);
    }
    if group.key == "imino_prefix" || group.key == "iminium" {
        let base = if group.key == "iminium" {
            "iminio"
        } else {
            "imino"
        };
        let Some(n) = amide_nitrogen(context.mol, group) else {
            return Ok(base.into());
        };
        let roots: Vec<_> = context
            .mol
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|r| {
                *r != group.attachment_carbon
                    && !group.atoms_involved.contains(r)
                    && context.mol.atoms[r].symbol != "H"
            })
            .collect();
        if roots.is_empty() {
            return Ok(base.into());
        }
        let mut blocked = context.sub_exclude.clone();
        blocked.insert(n);
        let names = roots
            .into_iter()
            .map(|r| name_subgraph_with_upstream(context.mol, r, &blocked, Some(n)))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(format!(
            "({}{base})",
            format_center_ligands_by(&names, substituent_sort_key)
        ));
    }
    if families.contains("sulfonyl") {
        let name = ester_prefix_from_group(context.mol, group, context.sub_exclude, "sulfonyl")?;
        return Ok(if name.is_empty() {
            "sulfo".into()
        } else {
            name
        });
    }
    if families.contains("peroxy_acid") {
        // Upstream divergence: hydroperoxycarbonyl duplicates a center already in the parent.
        if group.descriptor.as_ref().is_none_or(|descriptor| {
            descriptor
                .centers
                .iter()
                .any(|center| context.parent_path.contains(center))
        }) {
            return Err("openclatura: peroxy-acid prefix overlaps its parent carbon".into());
        }
        return Ok("hydroperoxycarbonyl".into());
    }
    if rule.has_capability("acyl_leaving_group") {
        return Ok(rule
            .prefix
            .as_ref()
            .map(|s| format!("({s})"))
            .unwrap_or_default());
    }
    if families.contains("cyano_prefix") {
        return Ok("cyano".into());
    }
    if families.contains("carboxy_prefix") {
        return Ok("carboxy".into());
    }
    if families.contains("amide_like") || rule.has_capability("amide_like") {
        return amide_prefix_from_group(context.mol, group, context.sub_exclude);
    }
    if families.contains("ester_like") || rule.has_capability("ester_like") {
        if !RULES.prefixes.peroxy_ester_groups.contains(&group.key)
            && let Some(prefix) = descriptor_ester_prefix(context, group)?
        {
            return Ok(prefix);
        }
        return ester_prefix_from_group(context.mol, group, context.sub_exclude, "carbonyl");
    }
    Ok(if context.parent_path.contains(&group.attachment_carbon) {
        rule.prefix.clone().unwrap_or_default()
    } else {
        String::new()
    })
}

pub fn collect_component_prefix_substituents(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    path: &[usize],
    exclude: &AtomSet,
) -> Result<(BTreeMap<usize, Vec<SubstituentItem>>, AtomSet), String> {
    let parent: AtomSet = path.iter().copied().collect();
    let mut mapping: BTreeMap<usize, Vec<SubstituentItem>> = BTreeMap::new();
    let mut handled = AtomSet::new();
    let context = PrefixContext {
        mol,
        parent_path: path,
        sub_exclude: exclude,
    };
    for group in groups {
        if group
            .resolved_rule
            .as_ref()
            .is_some_and(|r| r.families.contains("prefix_skip"))
            || !parent.contains(&group.attachment_carbon)
        {
            continue;
        }
        let name = prefix_from_group(&context, group)?;
        if name.is_empty() {
            continue;
        }
        let mut atoms = group.atoms_involved.clone();
        for &idx in &group.atoms_involved {
            for &n in mol.get_neighbors(idx) {
                if n != group.attachment_carbon
                    && !group.atoms_involved.contains(&n)
                    && !parent.contains(&n)
                    && !exclude.contains(&n)
                    && mol.atoms[&n].symbol != "H"
                {
                    let mut blocked = exclude.clone();
                    blocked.insert(idx);
                    atoms.extend(subgraph_component(mol, n, &blocked));
                }
            }
        }
        let mut bond_atoms = atoms.clone();
        bond_atoms.insert(group.attachment_carbon);
        let mut item = SubstituentItem::new(name, vec![]);
        item.bond_ids = bond_ids_within(mol, &bond_atoms);
        item.charge_atom_ids = atoms
            .iter()
            .copied()
            .filter(|i| mol.atoms[i].charge != 0)
            .collect();
        item.atom_ids = atoms;
        mapping
            .entry(group.attachment_carbon)
            .or_default()
            .push(item);
        handled.extend(&group.atoms_involved);
    }
    Ok((mapping, handled))
}

#[cfg(test)]
mod tests {
    use super::super::name_smiles;

    #[test]
    fn peroxy_ester_prefix_encloses_only_complex_ligands() {
        for (smiles, expected) in [
            ("COOC(=O)CC(=O)O", "2-(methylperoxycarbonyl)acetic acid"),
            (
                "CC(C)OOC(=O)CC(=O)O",
                "2-[(propan-2-yl)peroxycarbonyl]acetic acid",
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
    fn peroxy_carbamate_keeps_competing_ester_prefix_route() {
        assert_eq!(
            name_smiles("COOC(=O)N1[C@H](C=O)CC[C@H]1C(=O)O")
                .map(|name| name.text)
                .as_deref(),
            Ok("(2S,5S)-5-formyl-1-[(methoxyoxy)carbonyl]pyrrolidine-2-carboxylic acid")
        );
    }
}
