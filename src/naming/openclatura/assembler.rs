//! Plain-name assembly, without trace, audit or token-span construction.
use super::{
    assembly_charge::positive_parent_n_charges,
    assembly_parent::{
        format_parent_tail, format_substituent_tail, parent_stem_and_terminal,
        promote_acyl_substituent_name, promote_benzene_retained_name,
        promote_retained_functional_parent, promote_retained_substituent_name,
    },
    assembly_parts::{AssemblyParts, HydroOperation},
    assembly_prefixes::{format_replacement_prefixes, format_substituent_prefixes},
    assembly_utils::{needs_hyphen, parse_locant},
    formatting::format_multiplier,
    fused_ion_templates::{consume_fused_ion_operation, select_fused_ion_operation},
    name_postprocessing::apply_data_postprocessing,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn post_process_name(name: &str) -> String {
    let name = apply_data_postprocessing(name)
        .replace("iminoamino", "diazenyl")
        .replace("aminoimino", "hydrazono");
    apply_data_postprocessing(&name)
}
fn indicated_run(text: &str) -> Option<(Vec<String>, usize)> {
    let (run, _) = text.split_once('-')?;
    let locs: Option<Vec<_>> = run
        .split(',')
        .map(|s| {
            let l = s.strip_suffix('H')?;
            let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits == 0 || !l[digits..].chars().all(|c| c.is_ascii_lowercase()) {
                None
            } else {
                Some(l.to_owned())
            }
        })
        .collect();
    Some((locs?, run.len() + 1))
}
/// Saturation beyond the retained hydride's quota is added H or a hydro prefix.
fn normalize_indicated_hydrogen_quota(parts: &mut AssemblyParts, core: &str) -> String {
    let Some(quota) = super::additive::parent_indicated_hydrogen_quota(parts) else {
        return core.into();
    };
    if core.contains("lambda")
        || parts
            .retained_name
            .as_ref()
            .is_some_and(|name| name.contains("lambda"))
    {
        return core.into();
    }
    let stem_cited = indicated_run(core).map_or_else(Vec::new, |(locants, _)| locants);
    let citations: Vec<_> = parts
        .hydro_operations
        .iter()
        .enumerate()
        .filter(|(_, operation)| {
            operation.operation_kind == "indicated_hydrogen" && operation.key != "added_hydrogen"
        })
        .map(|(index, operation)| (index, operation.locants.clone()))
        .collect();
    let mut cited: Vec<_> = stem_cited
        .into_iter()
        .chain(
            citations
                .iter()
                .flat_map(|(_, locants)| locants.iter().cloned()),
        )
        .chain(parts.indicated_hydrogens.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    cited.sort_by_key(|locant| parse_locant(locant));
    if cited.len() <= quota {
        return core.into();
    }
    let surplus: BTreeSet<_> = cited.into_iter().skip(quota).collect();
    let mut bonded: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (left, right) in parts.parent_bond_ids_by_locants.keys() {
        bonded.entry(left).or_default().insert(right);
        bonded.entry(right).or_default().insert(left);
    }
    let suffix_locants: BTreeSet<_> = parts
        .principal_group
        .iter()
        .flat_map(|group| group.locants.iter().map(String::as_str))
        .collect();
    let added: BTreeSet<_> = surplus
        .iter()
        .filter(|locant| {
            parts.parent_charges.is_empty()
                && bonded
                    .get(locant.as_str())
                    .is_some_and(|neighbors| !neighbors.is_disjoint(&suffix_locants))
        })
        .cloned()
        .collect();
    let mut hydro: BTreeSet<_> = surplus.difference(&added).cloned().collect();
    let claimed: BTreeSet<_> = parts
        .hydro_operations
        .iter()
        .filter(|operation| {
            operation.operation_kind == "additive_hydrogen" || operation.key == "added_hydrogen"
        })
        .flat_map(|operation| operation.locants.iter().cloned())
        .collect();
    if !surplus.is_disjoint(&claimed) {
        return core.into();
    }
    let prefix_locants: BTreeSet<_> = parts
        .substituents
        .iter()
        .filter(|item| {
            parts.parent_charges.is_empty()
                && (["oxo", "thioxo", "sulfanylidene", "selanylidene", "imino"]
                    .contains(&item.name.as_str())
                    || item.name.ends_with("ylidene"))
        })
        .flat_map(|item| item.locants.iter())
        .filter(|locant| !claimed.contains(*locant))
        .map(String::as_str)
        .collect();
    let adjacent: Vec<_> = hydro
        .iter()
        .filter_map(|site| bonded.get(site.as_str()))
        .flat_map(|neighbors| {
            neighbors
                .intersection(&prefix_locants)
                .map(|locant| (*locant).to_owned())
        })
        .collect();
    hydro.extend(adjacent);
    let existing_hydro: usize = parts
        .hydro_operations
        .iter()
        .filter(|operation| operation.operation_kind == "additive_hydrogen")
        .map(|operation| operation.locants.len())
        .sum();
    if !(existing_hydro + hydro.len()).is_multiple_of(2) {
        return core.into();
    }
    let mut respelled = BTreeSet::new();
    for (index, locants) in citations {
        if !locants.iter().any(|locant| surplus.contains(locant)) {
            continue;
        }
        let operation = &mut parts.hydro_operations[index];
        if locants.iter().all(|locant| added.contains(locant)) {
            operation.key = "added_hydrogen".into();
        } else if locants.iter().all(|locant| hydro.contains(locant)) {
            operation.key = "additive_hydrogen".into();
            operation.operation_kind = "additive_hydrogen".into();
        } else {
            return core.into();
        }
        respelled.extend(locants);
    }
    for (locants, kind) in [(&added, "added_hydrogen"), (&hydro, "additive_hydrogen")] {
        let mut pending: Vec<_> = locants.difference(&respelled).cloned().collect();
        if !pending.is_empty() {
            pending.sort_by_key(|locant| parse_locant(locant));
            parts.hydro_operations.push(HydroOperation {
                key: kind.into(),
                locants: pending,
                operation_kind: kind.into(),
                ..Default::default()
            });
        }
    }
    parts
        .indicated_hydrogens
        .retain(|locant| !hydro.contains(locant));
    if (!hydro.is_empty() || !added.is_empty())
        && parts
            .retained_name
            .as_deref()
            .or_else(|| {
                parts
                    .fusion_plan
                    .as_ref()
                    .map(|plan| plan.rendered_base_name.as_str())
            })
            .is_some_and(|name| indicated_run(name).is_some())
        && let Some(metadata) = &mut parts.retained_parent_metadata
    {
        metadata
            .default_indicated_h
            .retain(|locant| !hydro.contains(locant) && !added.contains(locant));
        metadata.indicated_hydrogen_count = metadata
            .indicated_hydrogen_count
            .min(metadata.default_indicated_h.len());
    }
    respell_indicated_hydrogen(core, &hydro).unwrap_or_else(|| core.into())
}

fn respell_indicated_hydrogen(text: &str, respelled: &BTreeSet<String>) -> Option<String> {
    let (locants, end) = indicated_run(text)?;
    let remaining = locants
        .iter()
        .filter(|locant| !respelled.contains(*locant))
        .map(|locant| format!("{locant}H"))
        .collect::<Vec<_>>();
    Some(if remaining.is_empty() {
        text[end..].into()
    } else {
        format!("{}-{}", remaining.join(","), &text[end..])
    })
}

fn add_indicated_hydrogen_prefix(parts: &mut AssemblyParts, core: &str) -> String {
    let mut core = normalize_indicated_hydrogen_quota(parts, core);
    let mut additive: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.operation_kind == "additive_hydrogen")
        .flat_map(|o| o.locants.clone())
        .collect();
    let mut indicated: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.operation_kind == "indicated_hydrogen")
        .flat_map(|o| o.locants.clone())
        .collect();
    if indicated.is_empty() {
        indicated = parts.indicated_hydrogens.clone();
    }
    let cationic: BTreeSet<_> = positive_parent_n_charges(parts)
        .iter()
        .map(|c| c.locant.as_str())
        .collect();
    indicated.retain(|l| !cationic.contains(l.as_str()));
    indicated.sort_by_key(|l| parse_locant(l));
    indicated.dedup();
    additive.sort_by_key(|l| parse_locant(l));
    additive.dedup();
    if !indicated.is_empty() {
        let added: BTreeSet<_> = parts
            .hydro_operations
            .iter()
            .filter(|o| o.key == "added_hydrogen")
            .flat_map(|o| o.locants.iter())
            .collect();
        if !indicated.iter().all(|l| added.contains(l))
            && let Some((_, end)) = indicated_run(&core)
        {
            core = core[end..].into();
        }
        core = format!(
            "{}-{core}",
            indicated
                .iter()
                .map(|l| format!("{l}H"))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    if !additive.is_empty() {
        let sep = if core.starts_with(|c: char| c.is_ascii_digit()) {
            "-"
        } else {
            ""
        };
        let hydro = format_multiplier("hydro", additive.len(), false);
        let mut stated = parts
            .retained_parent_metadata
            .as_ref()
            .map_or(0, |m| m.indicated_hydrogen_count);
        let cited = indicated_run(&core).map_or(0, |(ls, _)| ls.len());
        if cited > 0 && cited < stated {
            stated = cited;
        }
        let fully_hydrogenated_fusion = parts.fusion_plan.is_some()
            && parts.parent_bond_delta.as_ref().is_some_and(|delta| {
                delta.compatible
                    && delta.implied_multiple_bond_ids.is_empty()
                    && delta.additional_multiple_bond_ids.is_empty()
                    && delta
                        .hydrogenated_edges
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>()
                        == delta
                            .assignment
                            .orders
                            .iter()
                            .filter(|(_, order)| *order == 2)
                            .map(|(edge, _)| *edge)
                            .collect()
            });
        if fully_hydrogenated_fusion
            || additive.len() + indicated.len().max(stated) == parts.parent_length
        {
            return format!("{hydro}{sep}{core}");
        }
        core = format!("{}-{hydro}{sep}{core}", additive.join(","));
    }
    core
}
fn move_added_hydrogen(parts: &AssemblyParts, core: &str, suffix: &str) -> (String, String) {
    let added: BTreeSet<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.key == "added_hydrogen")
        .flat_map(|o| o.locants.iter())
        .collect();
    if added.is_empty() {
        return (core.into(), suffix.into());
    }
    let Some(tail) = suffix.strip_prefix('-') else {
        return (core.into(), suffix.into());
    };
    let Some((suffix_locs, rest)) = tail.split_once('-') else {
        return (core.into(), suffix.into());
    };
    if !suffix_locs
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == ',')
    {
        return (core.into(), suffix.into());
    }
    let start = if indicated_run(core).is_some() {
        0
    } else if let Some(i) = core.find("hydro-") {
        i + 6
    } else {
        return (core.into(), suffix.into());
    };
    let Some((cited, end)) = indicated_run(&core[start..]) else {
        return (core.into(), suffix.into());
    };
    let (moving, kept): (Vec<_>, Vec<_>) = cited.into_iter().partition(|l| added.contains(l));
    if moving.is_empty() {
        return (core.into(), suffix.into());
    }
    let head = &core[..start];
    let tail = &core[start + end..];
    let new_core = if !kept.is_empty() {
        format!(
            "{head}{}-{tail}",
            kept.iter()
                .map(|l| format!("{l}H"))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        let head = head.trim_end_matches('-');
        let sep = if !head.is_empty() && tail.starts_with(|c: char| c.is_ascii_digit()) {
            "-"
        } else {
            ""
        };
        format!("{head}{sep}{tail}")
    };
    (
        new_core,
        format!(
            "-{suffix_locs}({})-{rest}",
            moving
                .iter()
                .map(|l| format!("{l}H"))
                .collect::<Vec<_>>()
                .join(",")
        ),
    )
}
fn add_stereo(parts: &AssemblyParts, word: &str) -> String {
    let mut features = vec![];
    for feature in &parts.stereo_features {
        if !features.contains(feature) {
            features.push(feature.clone());
        }
    }
    let unlocanted: BTreeSet<_> = features
        .iter()
        .filter(|(l, _)| l.is_empty())
        .map(|(_, d)| d.clone())
        .collect();
    features.retain(|(l, d)| l != "1" || !unlocanted.contains(d));
    features.sort_by_key(|(l, _)| {
        if l.is_empty() {
            (0, 0, String::new())
        } else {
            parse_locant(l)
        }
    });
    if features.is_empty() {
        word.into()
    } else {
        format!(
            "({})-{word}",
            features
                .iter()
                .map(|(l, d)| format!("{l}{d}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
fn front_modifier_sort_key(name: &str) -> String {
    name.strip_prefix("tert-")
        .or_else(|| name.strip_prefix("sec-"))
        .unwrap_or(name)
        .trim_start_matches(['(', '['])
        .to_lowercase()
}
fn add_front_modifiers(parts: &AssemblyParts, word: &str) -> String {
    if parts.front_modifiers.is_empty() {
        return word.into();
    }
    let have = parts.front_modifier_locants.len() == parts.front_modifiers.len()
        && parts.front_modifier_locants.iter().all(Option::is_some);
    let distinct = parts.front_modifiers.iter().collect::<BTreeSet<_>>().len();
    if have && distinct > 1 {
        let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (m, l) in parts
            .front_modifiers
            .iter()
            .zip(&parts.front_modifier_locants)
            .filter_map(|(m, l)| l.as_deref().map(|l| (m, l)))
        {
            by_name.entry(m).or_default().push(l);
        }
        let mut names: Vec<_> = by_name.keys().copied().collect();
        names.sort_by_key(|n| front_modifier_sort_key(n));
        let mods = names
            .into_iter()
            .map(|name| {
                let mut locs = by_name[name].clone();
                locs.sort_by_key(|l| (l.len(), *l));
                format!(
                    "{}-{}",
                    locs.join(","),
                    format_multiplier(name, locs.len(), false)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        return format!("{mods} {word}");
    }
    let mut counts = BTreeMap::new();
    for m in &parts.front_modifiers {
        *counts.entry(m).or_insert(0_usize) += 1;
    }
    let mods = counts
        .into_iter()
        .map(|(m, c)| {
            if c > 1 {
                format_multiplier(m, c, true)
            } else {
                m.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{mods} {word}")
}
pub fn assemble_name_raw(parts: &mut AssemblyParts) -> Result<String, String> {
    super::assembly_charge::prepare_fusion_charge_assembly(parts)?;
    let fused_ion_candidate = select_fused_ion_operation(parts);
    if let Some(candidate) = &fused_ion_candidate {
        consume_fused_ion_operation(parts, candidate);
    }
    promote_acyl_substituent_name(parts);
    promote_retained_substituent_name(parts);
    promote_benzene_retained_name(parts);
    promote_retained_functional_parent(parts)?;
    let prefix = format_substituent_prefixes(parts);
    let a_prefix = format_replacement_prefixes(parts);
    let core = if let Some(name) = fused_ion_candidate
        .as_ref()
        .and_then(|candidate| candidate.rendered_name())
    {
        name.to_owned()
    } else if let Some(name) = &parts.retained_substituent_name {
        name.clone()
    } else {
        let (stem, terminal) = parent_stem_and_terminal(parts)?;
        let stem = format!("{a_prefix}{stem}");
        let (stem, unsat, terminal, suffix) = if parts.is_substituent {
            format_substituent_tail(parts, &stem, &terminal)
        } else {
            format_parent_tail(parts, &stem, &terminal)?
        };
        let core = add_indicated_hydrogen_prefix(parts, &format!("{stem}{unsat}{terminal}"));
        let (core, suffix) = move_added_hydrogen(parts, &core, &suffix);
        format!("{core}{suffix}")
    };
    let charged_hyphen = !prefix.is_empty()
        && !positive_parent_n_charges(parts).is_empty()
        && parts.retained_name.is_some()
        && !parts.indicated_hydrogens.is_empty();
    let word = if !prefix.is_empty() && (needs_hyphen(&prefix, &core) || charged_hyphen) {
        format!("{prefix}-{core}")
    } else {
        format!("{prefix}{core}")
    };
    let mut word = add_stereo(parts, &word);
    let mut relative = vec![];
    for p in &parts.relative_stereo_prefixes {
        if !relative.contains(p) {
            relative.push(p.clone());
        }
    }
    if !relative.is_empty() {
        word = format!("{}-{word}", relative.join("-"));
    }
    Ok(add_front_modifiers(parts, &word))
}
#[cfg(test)]
pub fn assemble_name(parts: &mut AssemblyParts) -> Result<String, String> {
    assemble_name_raw(parts).map(|n| post_process_name(&n))
}

#[cfg(test)]
mod tests {
    use super::super::{
        assembly_parts::PrincipalGroupItem,
        nomenclature::{rule_from_value, section},
    };
    use super::*;

    #[test]
    fn systematic_fusion_suffixes_preserve_intrinsic_added_and_hydro_hydrogen() {
        for (smiles, expected) in [
            (
                "CCCC1=NN(C2=C1N=C(NC2=O)C3=C(C=CC(=C3)S(=O)(=O)N4CCN(CC4)C)OCC)C",
                "5-(2-ethoxy-5-(4-methylpiperazin-1-ylsulfonyl)phenyl)-1-methyl-3-propyl-6H-pyrazolo[4,3-d]pyrimidin-7(1H)-one",
            ),
            (
                "CCCCCN1C(=O)COC2=C1C=C(C(=C2)F)N3C(=O)C4=CC=CC=C4N=N3",
                "7-fluoro-6-(4-oxo-3,4-dihydrobenzo[d][1,2,3]triazin-3-yl)-4-pentyl-1,4-benzoxazin-3(4H)-one",
            ),
            (
                "O=C1[CH-]NC2=C1C[NH2+]C2",
                "3-oxo-1,4,5,6-tetrahydropyrrolo[3,4-b]pyrrol-5-ium-2-ide",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn oxo_prefixes_cite_retained_ring_hydrogenation() {
        for (smiles, expected) in [
            (
                "CC1=CC(=CC=C1)CN2C=CC=C(C2=O)C(=O)NCCCNS(=O)(=O)C",
                "N-(3-methanesulfonamidopropyl)-1-((3-methylphenyl)methyl)-2-oxo-1,2-dihydropyridine-3-carboxamide",
            ),
            (
                "C1=CNC(=O)C(=C1)C(=O)NC2=NC(=CS2)CC(=O)NC3=CC=C(C=C3)C(=O)N",
                "N-(4-(2-((4-carbamoylphenyl)amino)-2-oxoethyl)-1,3-thiazol-2-yl)-2-oxo-1,2-dihydropyridine-3-carboxamide",
            ),
            (
                "CC1=CC(=O)OC(=C1C(=O)NC2CC2)C",
                "N-cyclopropyl-4,6-dimethyl-2-oxopyran-5-carboxamide",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn selected_principal_citation_requires_a_suffix_renderer() {
        let mut parts = AssemblyParts::new(2);
        let mut group = PrincipalGroupItem::new("unrendered", vec!["1".into()]);
        group.resolved_rule = Some(rule_from_value("unrendered", &serde_json::json!({})));
        parts.principal_group = Some(group);
        assert!(assemble_name(&mut parts).is_err());
    }

    #[test]
    fn element_locanted_dynamic_suffix_keeps_separator_and_parent_vowel() {
        for (first, terminal, expected) in [
            ("Se", "O", "methane-SeO-selenoperoxol"),
            ("O", "Se", "methane-OSe-selenoperoxol"),
            ("O", "O", "methaneperoxol"),
        ] {
            let policy = section("chalcogen_peroxol_rules")
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["first"] == first && r["terminal"] == terminal)
                .unwrap();
            let mut rule = rule_from_value("dynamic_peroxol", policy);
            rule.suffix_with_locant = true;
            let mut parts = AssemblyParts::new(1);
            let mut group = PrincipalGroupItem::new("dynamic_peroxol", vec!["1".into()]);
            group.resolved_rule = Some(rule);
            parts.principal_group = Some(group);
            assert_eq!(assemble_name(&mut parts).unwrap(), expected);
        }
    }
}
