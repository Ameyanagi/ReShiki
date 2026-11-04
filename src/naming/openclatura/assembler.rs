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
    nomenclature::section,
};
use crate::naming::{NameReason, NameStatus};
use std::collections::{BTreeMap, BTreeSet};

/// Record departures only after promotions and final text selection have run.
pub(super) fn mark_assembled_name(mol: &super::molecule::Molecule, parts: &AssemblyParts) {
    if let Some(group) = &parts.principal_group {
        let rule = group
            .resolved_rule
            .as_ref()
            .or_else(|| super::nomenclature::RULES.functional_groups.get(&group.key));
        if rule.is_some_and(|rule| rule.families.contains("hydrazone")) {
            mol.mark_name(NameStatus::NotPreferred(NameReason::FunctionalClass));
        }
        // Formonitrile and acetonitrile oxide are retained (P-66.5.4.1);
        // substituted formonitrile oxide stays systematic (P-65.1.8).
        if !parts.is_ring
            && parts.parent_length <= 2
            && !parts.retained_absorbs_principal_group
            && rule.is_some_and(|rule| rule.suffix.as_deref() == Some("nitrile oxide"))
        {
            mol.mark_name(NameStatus::NotPreferred(NameReason::UnretainedParent));
        }
        if !parts.is_ring
            && parts.unsaturations.is_empty()
            && parts.a_prefixes.is_empty()
            && !parts.retained_absorbs_principal_group
        {
            if group.key == "aldehyde" && group.locants.len() == 1 {
                if parts.parent_length == 1
                    || parts.parent_length == 2 && parts.substituents.is_empty()
                {
                    mol.mark_name(NameStatus::NotPreferred(NameReason::UnretainedParent));
                } else if parts.parent_length == 2 {
                    mol.mark_name(NameStatus::Unverified(
                        NameReason::SubstitutedRetainedParent,
                    ));
                }
            }
            if parts.parent_length == 2 && group.key == "amide" && group.locants.len() == 2 {
                mol.mark_name(NameStatus::NotPreferred(NameReason::UnretainedParent));
            }
        }
    }
    // Formic acids, formyl halides and formamides substituted on carbon are
    // carbamic or carbonic acid derivatives (P-65.1.8.1).
    let formyl_parent = parts.retained_name.as_deref().is_some_and(|name| {
        matches!(name, "formic acid" | "formate" | "formamide") || name.starts_with("formyl ")
    });
    if formyl_parent
        && parts
            .substituents
            .iter()
            .any(|item| !item.locants.iter().any(|locant| locant.starts_with('N')))
    {
        mol.mark_name(NameStatus::NotPreferred(NameReason::UnretainedParent));
    }
    // An acyl group on a ring is named from the ring acid, cyclohexanecarbonyl
    // or 4-methylbenzoyl, not as a (ring-yl)carbonyl (P-65.1.7.4.2).
    if parts.is_acyl_substituent
        && parts.parent_length == 1
        && parts.retained_substituent_name.is_none()
        && let Some(&carbon) = parts.parent_atom_ids_by_locant.get("1")
    {
        let cyclic = super::chains::get_cyclic_atoms(mol, &Default::default());
        if parts.substituents.iter().any(|item| {
            item.atom_ids
                .iter()
                .any(|&atom| cyclic.contains(&atom) && mol.get_bond(carbon, atom).is_some())
        }) {
            mol.mark_name(NameStatus::NotPreferred(NameReason::AcylPrefix));
        }
    }
    super::parent_pipeline::mark_rendered_parent_status(mol, parts);
}

fn apply_data_postprocessing(name: &str) -> String {
    let mut name = name.to_owned();
    if let Some(rules) = section("postprocess_literal_replacements").as_array() {
        for rule in rules {
            if let (Some(from), Some(to)) = (rule["pattern"].as_str(), rule["replacement"].as_str())
            {
                name = name.replace(from, to);
            }
        }
    }
    name
}

pub fn post_process_name(name: &str) -> String {
    let name = apply_data_postprocessing(name)
        .replace("iminoamino", "diazenyl")
        .replace("aminoimino", "hydrazono");
    apply_data_postprocessing(&name)
}
struct IndicatedHydrogenRun<'a> {
    locants: Vec<&'a str>,
    remainder: &'a str,
}

fn indicated_run(text: &str) -> Option<IndicatedHydrogenRun<'_>> {
    let (run, remainder) = text.split_once('-')?;
    let locants = run
        .split(',')
        .map(|s| {
            let l = s.strip_suffix('H')?;
            let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
            (digits > 0 && l[digits..].chars().all(|c| c.is_ascii_lowercase())).then_some(l)
        })
        .collect::<Option<_>>()?;
    Some(IndicatedHydrogenRun { locants, remainder })
}

fn retained_parent_lambda_stem(parts: &AssemblyParts, stem: &str) -> (String, bool) {
    let enabled = parts.retained_name.as_ref().is_some_and(|name| {
        section("retained_parent_lambda_convention")["parent_names"]
            .as_array()
            .is_some_and(|parents| parents.iter().any(|parent| parent.as_str() == Some(name)))
    });
    if !enabled {
        return (stem.into(), false);
    }
    let mut descriptors = Vec::new();
    for (locant, symbol) in &parts.parent_atom_symbols_by_locant {
        if symbol == "C"
            || parts
                .parent_atom_charges_by_locant
                .get(locant)
                .copied()
                .unwrap_or(0)
                != 0
        {
            continue;
        }
        let Some(element) = super::rules::elements::get(symbol) else {
            continue;
        };
        let internal: usize = parts
            .parent_bond_orders_by_locants
            .iter()
            .filter(|((left, right), _)| left == locant || right == locant)
            .map(|(_, order)| *order)
            .sum();
        let external: usize = parts
            .parent_external_bonds_by_locant
            .get(locant)
            .into_iter()
            .flatten()
            .map(|(_, order, _)| *order)
            .sum();
        let bonding = internal
            + external
            + parts
                .parent_atom_hydrogens_by_locant
                .get(locant)
                .copied()
                .unwrap_or(0);
        if bonding > element.standard_valence {
            descriptors.push((locant, bonding));
        }
    }
    descriptors.sort_by_key(|(locant, bonding)| (parse_locant(locant), *bonding));
    if descriptors.is_empty() {
        return (stem.into(), false);
    }
    let lambda = descriptors
        .iter()
        .map(|(locant, bonding)| format!("{locant}lambda^{bonding}"))
        .collect::<Vec<_>>()
        .join(",");
    // P-14.1: indicated hydrogen precedes the lambda convention in the parent.
    let decorated = if let Some(run) = indicated_run(stem) {
        let boundary = stem.len() - run.remainder.len();
        format!("{}{lambda}-{}", &stem[..boundary], run.remainder)
    } else {
        format!("{lambda}-{stem}")
    };
    (decorated, true)
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
    let stem_cited = indicated_run(core).map_or_else(Vec::new, |run| run.locants);
    let citations: Vec<_> = parts
        .hydro_operations
        .iter()
        .enumerate()
        .filter(|(_, operation)| {
            operation.operation_kind == "indicated_hydrogen" && operation.key != "added_hydrogen"
        })
        .map(|(index, _)| index)
        .collect();
    let mut cited: Vec<_> = stem_cited
        .into_iter()
        .chain(citations.iter().flat_map(|&index| {
            parts.hydro_operations[index]
                .locants
                .iter()
                .map(String::as_str)
        }))
        .chain(parts.indicated_hydrogens.iter().map(String::as_str))
        .map(str::to_owned)
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
                && (["oxo", "sulfanylidene", "selanylidene", "imino"].contains(&item.name.as_str())
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
    for index in citations {
        let operation = &mut parts.hydro_operations[index];
        let locants = &operation.locants;
        if !locants.iter().any(|locant| surplus.contains(locant)) {
            continue;
        }
        if locants.iter().all(|locant| added.contains(locant)) {
            operation.key = "added_hydrogen".into();
        } else if locants.iter().all(|locant| hydro.contains(locant)) {
            operation.key = "additive_hydrogen".into();
            operation.operation_kind = "additive_hydrogen".into();
        } else {
            return core.into();
        }
        respelled.extend(locants.iter().cloned());
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
                    .parent_hydride
                    .as_ref()
                    .and_then(|parent| parent.base_name())
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
    let run = indicated_run(text)?;
    let remaining = run
        .locants
        .iter()
        .filter(|locant| !respelled.contains(**locant))
        .map(|locant| format!("{locant}H"))
        .collect::<Vec<_>>();
    Some(if remaining.is_empty() {
        run.remainder.into()
    } else {
        format!("{}-{}", remaining.join(","), run.remainder)
    })
}

fn add_indicated_hydrogen_prefix(parts: &mut AssemblyParts, core: &str) -> (String, bool) {
    let mut core = normalize_indicated_hydrogen_quota(parts, core);
    let mut cites_locants = indicated_run(&core).is_some();
    let mut additive: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.operation_kind == "additive_hydrogen")
        .flat_map(|o| o.locants.iter().cloned())
        .collect();
    let mut indicated: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.operation_kind == "indicated_hydrogen")
        .flat_map(|o| o.locants.iter().cloned())
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
        cites_locants = true;
        let added: BTreeSet<_> = parts
            .hydro_operations
            .iter()
            .filter(|o| o.key == "added_hydrogen")
            .flat_map(|o| o.locants.iter())
            .collect();
        if !indicated.iter().all(|l| added.contains(l))
            && let Some(run) = indicated_run(&core)
        {
            core = run.remainder.into();
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
        let cited = indicated_run(&core).map_or(0, |run| run.locants.len());
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
        if !parts.spiro_prefix_composition
            && (fully_hydrogenated_fusion
                || additive.len() + indicated.len().max(stated) == parts.parent_length)
        {
            return (format!("{hydro}{sep}{core}"), cites_locants);
        }
        core = format!("{}-{hydro}{sep}{core}", additive.join(","));
        cites_locants = true;
    }
    (core, cites_locants)
}
fn move_added_hydrogen(parts: &AssemblyParts, core: &str, suffix: &str) -> (String, String) {
    let added: BTreeSet<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.key == "added_hydrogen")
        .flat_map(|o| o.locants.iter().map(String::as_str))
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
    let Some(run) = indicated_run(&core[start..]) else {
        return (core.into(), suffix.into());
    };
    let (moving, kept): (Vec<_>, Vec<_>) =
        run.locants.into_iter().partition(|l| added.contains(*l));
    if moving.is_empty() {
        return (core.into(), suffix.into());
    }
    let head = &core[..start];
    let tail = run.remainder;
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
fn add_stereo(parts: &AssemblyParts, word: &str, cites_locants: bool) -> String {
    let mut features = vec![];
    for feature in &parts.stereo_features {
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    let unlocanted: BTreeSet<_> = features
        .iter()
        .filter(|(l, _)| l.is_empty())
        .map(|(_, d)| d.as_str())
        .collect();
    features.retain(|(l, d)| l != "1" || !unlocanted.contains(d.as_str()));
    features.sort_by_key(|(l, _)| {
        if l.is_empty() {
            (0, 0, String::new())
        } else {
            parse_locant(l)
        }
    });
    match features.as_slice() {
        [] => word.into(),
        [(_, descriptor)]
            if !cites_locants
                && super::locant_elision::single_stereo_locant_is_unambiguous(parts) =>
        {
            format!("({descriptor})-{word}")
        }
        _ => format!(
            "({})-{word}",
            features
                .iter()
                .map(|(l, d)| format!("{l}{d}"))
                .collect::<Vec<_>>()
                .join(",")
        ),
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
        let mut modifiers: Vec<_> = by_name.into_iter().collect();
        modifiers.sort_by_key(|(name, _)| front_modifier_sort_key(name));
        let mods = modifiers
            .into_iter()
            .map(|(name, mut locs)| {
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
    assemble_name_with_parent(parts).map(|(name, _)| name)
}

/// Keep the rendered parent boundary for isotope descriptor placement.
pub(super) fn assemble_name_with_parent(
    parts: &mut AssemblyParts,
) -> Result<(String, String), String> {
    super::assembly_charge::prepare_fusion_charge_assembly(parts)?;
    let fused_ion_candidate = select_fused_ion_operation(parts);
    if let Some(candidate) = &fused_ion_candidate {
        consume_fused_ion_operation(parts, candidate);
    }
    promote_acyl_substituent_name(parts);
    promote_retained_substituent_name(parts);
    promote_benzene_retained_name(parts);
    promote_retained_functional_parent(parts)?;
    let spiro_sides = super::assembly_spiro::split_spiro_substituents(parts)?;
    parts.spiro_prefix_composition = !spiro_sides.is_empty();
    let (a_prefix, mut cites_locants) = format_replacement_prefixes(parts);
    cites_locants |= !spiro_sides.is_empty();
    let core = if let Some(name) = fused_ion_candidate
        .as_ref()
        .and_then(|candidate| candidate.rendered_name())
    {
        cites_locants |= name
            .split('-')
            .any(|run| run.starts_with(|c: char| c.is_ascii_digit()));
        name.to_owned()
    } else if let Some(name) = &parts.retained_substituent_name {
        cites_locants |= name
            .split('-')
            .any(|run| run.starts_with(|c: char| c.is_ascii_digit()));
        name.clone()
    } else {
        let (stem, terminal) = parent_stem_and_terminal(parts)?;
        let (stem, lambda_cites_locants) = retained_parent_lambda_stem(parts, &stem);
        cites_locants |= lambda_cites_locants;
        let parent_terminal = if parts
            .parent_hydride
            .as_ref()
            .is_some_and(|p| p.uses_fusion_plan())
        {
            terminal.clone()
        } else {
            "e".into()
        };
        let stem = format!("{a_prefix}{stem}");
        let tail = if parts.is_substituent {
            format_substituent_tail(parts, &stem)
        } else {
            format_parent_tail(parts, &stem, &terminal)?
        };
        cites_locants |= tail.cites_locants;
        let spiro = super::assembly_spiro::format_spiro_core(
            &tail.stem,
            &tail.unsaturation,
            &tail.terminal,
            &spiro_sides,
            &tail.suffix,
            &parent_terminal,
        );
        let (core, hydrogen_cites_locants) = add_indicated_hydrogen_prefix(parts, &spiro.core);
        cites_locants |= hydrogen_cites_locants;
        let (core, suffix) = move_added_hydrogen(parts, &core, &spiro.suffix);
        format!("{core}{suffix}")
    };
    // Plan the core first: its citations keep optional substituent locants too.
    if cites_locants {
        parts.elided_substituent_locants.clear();
    }
    let (prefix, prefix_cites_locants) = format_substituent_prefixes(parts);
    cites_locants |= prefix_cites_locants;
    if prefix_cites_locants {
        parts.elided_substituent_locants.clear();
    }
    let charged_hyphen = !prefix.is_empty()
        && !positive_parent_n_charges(parts).is_empty()
        && parts.retained_name.is_some()
        && !parts.indicated_hydrogens.is_empty();
    let word = if !prefix.is_empty() && (needs_hyphen(&prefix, &core) || charged_hyphen) {
        format!("{prefix}-{core}")
    } else {
        format!("{prefix}{core}")
    };
    let mut word = add_stereo(parts, &word, cites_locants);
    let mut relative = vec![];
    for p in &parts.relative_stereo_prefixes {
        if !relative.contains(&p.as_str()) {
            relative.push(p.as_str());
        }
    }
    if !relative.is_empty() {
        word = format!("{}-{word}", relative.join("-"));
    }
    Ok((add_front_modifiers(parts, &word), core))
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
    fn stereo_locant_omission_preserves_configurations() {
        let canonical_inner_junction =
            crate::naming::canonical_smiles("C1CCC[C@H]2C3CCCCC3CCC12").unwrap();
        for (smiles, expected) in [
            ("C1CCC/C=C/CC1", "(E)-cyclooctene"),
            ("C1CCC/C=C\\CC1", "(Z)-cyclooctene"),
            ("C1CCCC/C=C/CC1", "(E)-cyclononene"),
            ("C1CCCC/C=C\\CC1", "(Z)-cyclononene"),
            ("C1CCCCC/C=C/CC1", "(E)-cyclodecene"),
            ("F[C@H](Cl)Br", "(R)-bromochlorofluoromethane"),
            ("F[C@@H](Cl)Br", "(S)-bromochlorofluoromethane"),
            ("F[C@H](Cl)I", "(R)-chlorofluoroiodomethane"),
            ("F[C@H](Cl)c1ccccc1", "[(R)-chlorofluoromethyl]benzene"),
            (
                "F[C@@H](Cl)c1ccc([C@H](F)Br)cc1",
                "1-[(R)-bromofluoromethyl]-4-[(S)-chlorofluoromethyl]benzene",
            ),
            (
                "O[C@H](F)c1ccc(Cl)cc1",
                "(R)-(4-chlorophenyl)fluoromethanol",
            ),
            ("O=C(O)C(F)Cl", "chlorofluoroacetic acid"),
            ("O=C(O)C(Cl)Cl", "dichloroacetic acid"),
            ("O=C(O)C(Cl)CCl", "2,3-dichloropropanoic acid"),
            ("O=C(N)CF", "2-fluoroacetamide"),
            ("O=C(NC)CF", "2-fluoro-N-methylacetamide"),
            ("O=C(O)[C@H](F)Cl", "(R)-chlorofluoroacetic acid"),
            ("O=C(O)[C@@H](F)Cl", "(S)-chlorofluoroacetic acid"),
            (
                "C1CCC[C@H]2C3CCCCC3CCC12",
                "(4aR)-tetradecahydrophenanthrene",
            ),
            (
                canonical_inner_junction.as_str(),
                "(4bR)-tetradecahydrophenanthrene",
            ),
            (
                "C1CCCC2C3CCCC[C@@H]3CCC12",
                "(8aR)-tetradecahydrophenanthrene",
            ),
            ("C/C=C/C", "(2E)-but-2-ene"),
            ("C/C=C\\C", "(2Z)-but-2-ene"),
            ("Cl/C=C/F", "(1E)-1-chloro-2-fluoroethene"),
            ("C[C@@H](O)CC", "(2R)-butan-2-ol"),
            ("O[C@H]1C[C@H](O)C1", "(1r,3r)-cyclobutane-1,3-diol"),
            ("C1C[C@H]2CC[C@@H]12", "(1s,4s)-bicyclo[2.2.0]hexane"),
            ("Cl/C1=C/CCCCCC1", "(1E)-1-chlorocyclooct-1-ene"),
            ("CC1CCC/C=C/CC1", "(1E)-5-methylcyclooct-1-ene"),
            (
                "O=C(O)[C@H](O)[C@H](O)[C@H](O)C(=O)O",
                "(2R,3r,4S)-2,3,4-trihydroxypentanedioic acid",
            ),
            ("C[C@H](O)c1ccccc1", "(1S)-1-phenylethan-1-ol"),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn constitutional_locant_omission_preserves_other_word_citations() {
        for (smiles, expected) in [
            ("CN1CCC(O)CC1", "1-methylpiperidin-4-ol"),
            (
                "COC(=O)C[C@@H](C1=CC=CC=C1Cl)NC(=O)C2CCN(CC2)C3=NS(=O)(=O)C4=CC=CC=C43",
                "methyl (3S)-3-(2-chlorophenyl)-3-[1-(1,1-dioxo-1,2-benzothiazol-3-yl)piperidine-4-carboxamido]propanoate",
            ),
            (
                "CC1=C(C=C(C=C1)NC(=O)C2CCCCN2C(=O)C3=CC=CS3)S(=O)(=O)N4CCCCCC4",
                "N-[3-(azepane-1-sulfonyl)-4-methylphenyl]-1-[(thiophen-2-yl)carbonyl]piperidine-2-carboxamide",
            ),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn acid_derived_amido_prefixes_keep_the_real_nitrogen_substitution_site() {
        for (smiles, expected) in [
            ("O=C(O)CNC(=O)Cc1ccccc1", "(2-phenylacetamido)acetic acid"),
            (
                "CC1([C@@H](N2[C@H](S1)[C@@H](C2=O)NC(=O)CC3=CC=CC=C3)C(=O)O)C",
                "(1R,4S,7R)-3,3-dimethyl-6-oxo-7-(2-phenylacetamido)-2-thia-5-azabicyclo[3.2.0]heptane-4-carboxylic acid",
            ),
            (
                "CN(C1=CC=C(C=C1)NC(=O)CNC(=O)C2=CSC=C2)C(=O)OC",
                "methyl methyl{4-[2-(thiophene-3-carboxamido)acetamido]phenyl}carbamate",
            ),
            (
                "CC1=CC=C(C=C1)N2C(=O)C3=C(N=C2SCC(=O)N/N=C\\C4=CC(=C(C=C4)OC(=O)C)OC)SC5=C3CCCC5",
                "2-methoxy-4-{(Z)-[(2-{[3-(4-methylphenyl)-4-oxo-5,6,7,8-tetrahydro[1]benzothieno[2,3-d]pyrimidin-2-yl]sulfanyl}acetamido)imino]methyl}phenyl acetate",
            ),
        ] {
            let input = crate::naming::canonical_smiles(smiles).unwrap();
            let name = super::super::name_smiles(&input).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                input,
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn n_substituted_amido_prefixes_use_complete_amide_ordering() {
        for (smiles, expected) in [
            (
                "OC(=O)CCN(C)C(=O)c1ccc(O)c(Cl)c1",
                "3-(3-chloro-4-hydroxy-N-methylbenzamido)propanoic acid",
            ),
            (
                "OC(=O)CCN(CC)C(=O)c1ccc(C)cc1",
                "3-(N-ethyl-4-methylbenzamido)propanoic acid",
            ),
            (
                "OC(=O)CCN(C)C(=O)c1ccc(C)cc1",
                "3-(N,4-dimethylbenzamido)propanoic acid",
            ),
            (
                "OC(=O)CCN(C(C)C)C(=O)c1ccc(C)cc1",
                "3-[4-methyl-N-(propan-2-yl)benzamido]propanoic acid",
            ),
            (
                "OC(=O)CCN(C(=O)c1ccccc1)c1ccccc1",
                "3-(N-phenylbenzamido)propanoic acid",
            ),
            (
                "CN(C(C)C(=O)O)S(=O)(=O)c1ccc(C)cc1",
                "2-(N,4-dimethylbenzene-1-sulfonamido)propanoic acid",
            ),
            (
                "O=C(O)CN(CCO)C(=O)c1cc(Cl)ccc1",
                "[3-chloro-N-(2-hydroxyethyl)benzamido]acetic acid",
            ),
            (
                "O=C(O)CN(CCO)C(=O)c1ccc(CCO)cc1",
                "[N,4-bis(2-hydroxyethyl)benzamido]acetic acid",
            ),
        ] {
            let input = crate::naming::canonical_smiles(smiles).unwrap();
            let name = super::super::name_smiles(&input).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                input,
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn competing_amide_suffixes_keep_the_attachment_nitrogen() {
        for (smiles, expected, status) in [
            (
                "O=C(O)CN(CCNC(=O)CCCC)C(C)=O",
                "{N-[2-(pentanamido)ethyl]acetamido}acetic acid",
                NameStatus::Preferred,
            ),
            (
                "O=C(O)CCN(CCNC(=O)CCCC)C=O",
                "3-{N-[2-(pentanamido)ethyl]formamido}propanoic acid",
                NameStatus::Preferred,
            ),
            (
                "O=C(O)CCN(CCNC(=O)c1ccccc1)C(C)=O",
                "3-[N-(2-benzamidoethyl)acetamido]propanoic acid",
                NameStatus::Preferred,
            ),
            (
                "OC(=O)CCN(CCNC(=O)CCCC)C(=O)c1ccc(C)cc1",
                "3-{4-methyl-N-[2-(pentanamido)ethyl]benzamido}propanoic acid",
                NameStatus::Preferred,
            ),
            (
                "O=C(O)CN(CCNC(=[Se])CCCC)S(=O)(=O)c1ccccc1",
                "(N-{2-[(pentaneselenoyl)amino]ethyl}benzenesulfonamido)acetic acid",
                NameStatus::NotPreferred(NameReason::AminoPrefix),
            ),
            (
                "O=C(O)CCN(C(C)=O)C(C)=O",
                "3-(diacetylamino)propanoic acid",
                NameStatus::NotPreferred(NameReason::AminoPrefix),
            ),
        ] {
            let input = crate::naming::canonical_smiles(smiles).unwrap();
            let name = super::super::name_smiles(&input).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                input,
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn hydrazone_descriptor_cites_a_locant_only_when_the_name_does() {
        for (smiles, expected) in [
            ("C/C=N/N=C", "(E)-ethanal methylidenehydrazone"),
            ("CC/C=N/N=C", "(E)-propanal methylidenehydrazone"),
            (
                "CC(C)/C=N/N=C",
                "formaldehyde (1E)-2-methylpropylidenehydrazone",
            ),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }

    #[test]
    fn substituted_monocycles_cite_principal_suffix_locant_one() {
        for (smiles, expected) in [
            ("O[C@H]1[C@H](C)CCCC1", "(1R,2R)-2-methylcyclohexan-1-ol"),
            ("O[C@H]1[C@@H](C)CCCC1", "(1R,2S)-2-methylcyclohexan-1-ol"),
            ("OC1C(C)CCCC1", "2-methylcyclohexan-1-ol"),
            ("OC1CCC(C)CC1", "4-methylcyclohexan-1-ol"),
            ("O=C1C(C)CCC1", "2-methylcyclopentan-1-one"),
            ("NC1C(C)CCCC1", "2-methylcyclohexan-1-amine"),
            ("OC(=O)C1C(C)CCCC1", "2-methylcyclohexane-1-carboxylic acid"),
            ("OC1C(Cl)CCC1", "2-chlorocyclopentan-1-ol"),
            ("SC1C(C)CCCC1", "2-methylcyclohexane-1-thiol"),
            ("OC1CCCCC1C(C)C", "2-(propan-2-yl)cyclohexan-1-ol"),
            ("CC1(O)CCCCC1", "1-methylcyclohexan-1-ol"),
            ("OC1CC1C", "2-methylcyclopropan-1-ol"),
            ("OC1C(C)CCCCC1", "2-methylcycloheptan-1-ol"),
            ("N#CC1C(C)CCCC1", "2-methylcyclohexane-1-carbonitrile"),
            ("O=CC1C(C)CCCC1", "2-methylcyclohexane-1-carbaldehyde"),
            (
                "OS(=O)(=O)C1C(C)CCCC1",
                "2-methylcyclohexane-1-sulfonic acid",
            ),
            ("CNC1C(C)CCCC1", "N,2-dimethylcyclohexan-1-amine"),
            ("[NH3+]C1C(C)CCCC1", "2-methylcyclohexan-1-aminium"),
            ("OC1CCCCC1", "cyclohexanol"),
            ("O=C1CCCC1", "cyclopentanone"),
            ("NC1CCCCC1", "cyclohexanamine"),
            ("[NH3+]C1CCCCC1", "cyclohexanaminium"),
            ("OC(=O)C1CCCCC1", "cyclohexanecarboxylic acid"),
            ("SC1CCCCC1", "cyclohexanethiol"),
            ("N#CC1CCCCC1", "cyclohexanecarbonitrile"),
            ("O=CC1CCCCC1", "cyclohexanecarbaldehyde"),
            ("OS(=O)(=O)C1CCCCC1", "cyclohexanesulfonic acid"),
            ("CNC1CCCCC1", "N-methylcyclohexanamine"),
            ("CN(C)C1CCCCC1", "N,N-dimethylcyclohexanamine"),
            ("CNC(=O)C1CCCCC1", "N-methylcyclohexanecarboxamide"),
            ("OC1C=CCCC1", "cyclohex-2-en-1-ol"),
            ("O=C1C=CCCC1", "cyclohex-2-en-1-one"),
            ("Oc1ccccc1C", "2-methylphenol"),
            ("Oc1ccccc1", "phenol"),
            ("OC(=O)c1ccccc1", "benzoic acid"),
            ("OC(=O)c1ccccc1C", "2-methylbenzoic acid"),
            ("Nc1ccccc1C", "2-methylaniline"),
            ("CCC(O)C", "butan-2-ol"),
            ("CCCC(=O)O", "butanoic acid"),
            ("OC1CCC(O)CC1", "cyclohexane-1,4-diol"),
            ("C1=CCCCC1", "cyclohexene"),
            ("CC1=CCCCC1", "1-methylcyclohex-1-ene"),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{smiles}: {}",
                name.text
            );
        }
    }

    #[test]
    fn systematic_fusion_suffixes_preserve_intrinsic_added_and_hydro_hydrogen() {
        for (smiles, expected) in [
            (
                "CCCC1=NN(C2=C1N=C(NC2=O)C3=C(C=CC(=C3)S(=O)(=O)N4CCN(CC4)C)OCC)C",
                "5-[2-ethoxy-5-(4-methylpiperazine-1-sulfonyl)phenyl]-1-methyl-3-propyl-6H-pyrazolo[4,3-d]pyrimidin-7(1H)-one",
            ),
            (
                "CCCCCN1C(=O)COC2=C1C=C(C(=C2)F)N3C(=O)C4=CC=CC=C4N=N3",
                "7-fluoro-6-(4-oxo-3,4-dihydro-1,2,3-benzotriazin-3-yl)-4-pentyl-1,4-benzoxazin-3(4H)-one",
            ),
            (
                "O=C1[CH-]NC2=C1C[NH2+]C2",
                "3-oxo-1,4,5,6-tetrahydropyrrolo[3,4-b]pyrrol-5-ium-2-ide",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles)
                    .map(|name| name.text)
                    .as_deref(),
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
                "N-(3-methanesulfonamidopropyl)-1-[(3-methylphenyl)methyl]-2-oxo-1,2-dihydropyridine-3-carboxamide",
            ),
            (
                "C1=CNC(=O)C(=C1)C(=O)NC2=NC(=CS2)CC(=O)NC3=CC=C(C=C3)C(=O)N",
                "N-(4-{2-[(4-carbamoylphenyl)amino]-2-oxoethyl}-1,3-thiazol-2-yl)-2-oxo-1,2-dihydropyridine-3-carboxamide",
            ),
            (
                "CC1=CC(=O)OC(=C1C(=O)NC2CC2)C",
                "N-cyclopropyl-4,6-dimethyl-2-oxopyran-5-carboxamide",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles)
                    .map(|name| name.text)
                    .as_deref(),
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
