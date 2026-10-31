//! Parent stems, unsaturation and principal/substituent tails.
use super::{
    assembly_charge::{
        append_charge_suffixes_to_terminal, has_ionic_retained_parent, has_retained_like_parent,
        inferred_ionic_retained_parent, parent_charge_suffix,
    },
    assembly_parts::AssemblyParts,
    assembly_prefixes::substituent_sort_key,
    assembly_utils::parse_locant,
    formatting::{is_complex_prefix, strip_outer_parentheses},
    nomenclature::RULES,
    principal_suffixes::render_principal_suffix,
    rules::{bonds, elision, multipliers, stems},
};

fn functional_ring_parent(name: &str, key: &str) -> Option<String> {
    if name != "benzene" {
        return None;
    }
    let base = match key {
        "alcohol" => "phenol",
        "amine" => "aniline",
        "ring_aldehyde" => "benzaldehyde",
        "ring_carboxylic_acid" => "benzoic acid",
        "ring_amide" => "benzamide",
        "ring_hydrazide" => "benzohydrazide",
        "ring_nitrile" => "benzonitrile",
        "ring_carboxylate" => "benzoate",
        "acyl" => "benzoyl",
        _ => "",
    };
    if !base.is_empty() {
        return Some(base.into());
    }
    if let Some(chain_key) = key.strip_prefix("ring_")
        && let Some(word) = halide_word(chain_key)
    {
        return Some(format!("benzoyl {word}"));
    }
    if RULES
        .functional_groups
        .keys_with_family("hydrazone")
        .contains(key)
    {
        let suffix = RULES.functional_groups.get(key)?.suffix.as_ref()?;
        let (base, derivative) = suffix.split_once(' ')?;
        let base_key = RULES
            .functional_groups
            .by_key
            .iter()
            .find(|(_, r)| r.suffix.as_deref() == Some(base))?
            .0;
        return functional_ring_parent(name, base_key).map(|s| format!("{s} {derivative}"));
    }
    None
}
fn halide_word(key: &str) -> Option<String> {
    let suffix = RULES.functional_groups.get(key)?.suffix.as_deref()?;
    suffix.strip_prefix("oyl ").map(str::to_owned)
}
fn retained_chain_name(length: usize, key: &str, count: usize) -> Option<String> {
    if let Some(name) =
        RULES
            .retained
            .chain_functional_parents
            .get(&(length, key.to_owned(), count))
    {
        return Some(name.clone());
    }
    let name = match (length, key, count) {
        (1, "carboxylic_acid", 1) => "formic acid",
        (2, "carboxylic_acid", 1) => "acetic acid",
        (1, "amide", 1) => "formamide",
        (2, "amide", 1) => "acetamide",
        (1, "hydrazide", 1) => "formohydrazide",
        (2, "hydrazide", 1) => "acetohydrazide",
        (1, "urea", 1) => "urea",
        (1, "guanidine", 1) => "guanidine",
        (1, "thiourea", 1) => "thiourea",
        (1, "nitrile", 1) => "hydrogen cyanide",
        (2, "nitrile", 1) => "acetonitrile",
        (3, "nitrile", 1) => "propionitrile",
        (4, "nitrile", 1) => "butyronitrile",
        (1, "ester" | "carboxylate", 1) => "formate",
        (2, "ester" | "carboxylate", 1) => "acetate",
        (2, "carboxylic_acid", 2) => "oxalic acid",
        (3, "carboxylic_acid", 2) => "malonic acid",
        (4, "carboxylic_acid", 2) => "succinic acid",
        (5, "carboxylic_acid", 2) => "glutaric acid",
        (6, "carboxylic_acid", 2) => "adipic acid",
        _ => "",
    };
    if !name.is_empty() {
        return Some(name.into());
    }
    if count == 1 && (length == 1 || length == 2) {
        if let Some(word) = halide_word(key) {
            return Some(format!(
                "{}yl {word}",
                if length == 1 { "form" } else { "acet" }
            ));
        }
        if let Some(suffix) = RULES
            .functional_groups
            .get(key)
            .and_then(|r| r.suffix.as_ref())
            && let Some(derivative) = suffix.strip_prefix("al ")
        {
            return Some(format!(
                "{} {derivative}",
                if length == 1 {
                    "formaldehyde"
                } else {
                    "acetaldehyde"
                }
            ));
        }
    }
    None
}
fn absorbed_chain_branch(
    parts: &AssemblyParts,
    key: &str,
    count: usize,
) -> Option<(String, usize)> {
    if parts.parent_length != 1 || count != 1 || parts.substituents.len() != 1 {
        return None;
    }
    let branch = &parts.substituents[0];
    if branch.locants != ["1"] {
        return None;
    }
    let direct = match (key, branch.name.as_str()) {
        ("nitrile", "hydroxy") => Some("cyanic acid"),
        ("carboxylic_acid", "amino") => Some("carbamic acid"),
        ("amide", "hydrazinyl") => Some("hydrazinecarboxamide"),
        ("thioamide", "hydrazinyl") => Some("hydrazinecarbothioamide"),
        ("ester", "hydrazinyl") => Some("hydrazinecarboxylate"),
        ("carboxylic_acid", "hydrazinyl") => Some("hydrazinecarboxylic acid"),
        ("ester" | "carboxylate", "amino") => Some("carbamate"),
        ("acid_chloride", "amino") => Some("carbamic chloride"),
        ("acid_fluoride", "amino") => Some("carbamic fluoride"),
        ("acid_bromide", "amino") => Some("carbamic bromide"),
        _ => None,
    };
    if let Some(n) = direct {
        return Some((n.into(), 0));
    }
    let text = strip_outer_parentheses(&branch.name);
    if text.starts_with("N-") || text.starts_with("N,") {
        return None;
    }
    let word = match key {
        "nitrile" => "cyanamide",
        "carboxylic_acid" => "carbamic acid",
        "ester" | "carboxylate" => "carbamate",
        "acid_chloride" => "carbamic chloride",
        "acid_fluoride" => "carbamic fluoride",
        "acid_bromide" => "carbamic bromide",
        _ => return None,
    };
    text.strip_suffix("amino")
        .map(|prefix| (format!("{prefix}{word}"), 0))
}
pub fn promote_benzene_retained_name(parts: &mut AssemblyParts) {
    if parts.is_ring
        && !parts.is_bicycle
        && !parts.is_spiro
        && parts.parent_length == 6
        && parts.unsaturations.len() == 1
        && parts.unsaturations[0].bond_key == "double"
        && parts.a_prefixes.is_empty()
    {
        let mut locs = parts.unsaturations[0].locants.clone();
        locs.sort_by_key(|s| parse_locant(s));
        if locs == ["1", "3", "5"] {
            parts.retained_name = Some("benzene".into());
            parts.unsaturations.clear();
        }
    }
}
pub fn promote_retained_functional_parent(parts: &mut AssemblyParts) -> Result<(), String> {
    let Some(group) = parts.principal_group.as_ref() else {
        return Ok(());
    };
    if parts.is_substituent || parts.retained_absorbs_principal_group {
        return Ok(());
    }
    let mut retained = if group.locants == ["1"] {
        functional_ring_parent(parts.retained_name.as_deref().unwrap_or(""), &group.key)
    } else {
        None
    };
    let mut absorbed = None;
    if retained.is_none() {
        if parts.is_ring
            || !parts.unsaturations.is_empty()
            || !parts.a_prefixes.is_empty()
            || parts.parent_charges.iter().any(|c| c.charge < 0)
        {
            return Ok(());
        }
        let mut locs = group.locants.clone();
        locs.sort_by_key(|s| s.parse::<usize>().unwrap_or(usize::MAX));
        let expected = if locs.len() == 1 {
            vec!["1".into()]
        } else {
            vec!["1".into(), parts.parent_length.to_string()]
        };
        if locs != expected {
            return Ok(());
        }
        if let Some((name, index)) = absorbed_chain_branch(parts, &group.key, locs.len()) {
            // Upstream divergence: unbracketed carbamate N substituents can be parsed as O substituents.
            if let Some(prefix) = name.strip_suffix("carbamate") {
                let grouped = super::assembly_utils::is_fully_enclosed(prefix)
                    || prefix
                        .strip_prefix("bis")
                        .is_some_and(super::assembly_utils::is_fully_enclosed);
                if !prefix.is_empty()
                    && !grouped
                    && (group.key == "carboxylate" || prefix.ends_with("peroxy"))
                {
                    return Err(
                        "openclatura: carbamate contraction needs explicit N attachment".into(),
                    );
                }
            }
            retained = Some(name);
            absorbed = Some(index);
        } else {
            retained = retained_chain_name(parts.parent_length, &group.key, locs.len());
        }
        if let Some(name) = retained.as_deref() {
            if !parts.substituents.is_empty()
                && [
                    "oxalic acid",
                    "malonic acid",
                    "succinic acid",
                    "glutaric acid",
                    "adipic acid",
                    "hydrogen cyanide",
                ]
                .contains(&name)
            {
                return Ok(());
            }
            if [
                "formaldehyde hydrazone",
                "acetaldehyde hydrazone",
                "formaldehyde amidinohydrazone",
                "acetaldehyde amidinohydrazone",
            ]
            .contains(&name)
                && parts
                    .substituents
                    .iter()
                    .flat_map(|s| &s.locants)
                    .any(|s| s.chars().all(|c| c.is_ascii_digit()))
            {
                return Ok(());
            }
        }
    }
    let Some(mut name) = retained else {
        return Ok(());
    };
    if let Some(mode) = &parts.principal_citation_mode
        && let Some(citation) = RULES
            .retained
            .principal_citations
            .get(&(mode.clone(), name.clone()))
    {
        name = citation.clone();
    }
    if name == "guanidine" {
        for (loc, _) in &mut parts.stereo_features {
            if loc == "1" {
                loc.clear();
            }
        }
    }
    parts.retained_name = Some(name);
    parts.retained_absorbs_principal_group = true;
    if let Some(index) = absorbed {
        parts.substituents.remove(index);
    }
    Ok(())
}
pub fn substituent_attachment_kind(parts: &AssemblyParts) -> &'static str {
    if parts.is_triple_attach {
        "triple"
    } else if parts.is_double_attach {
        "double"
    } else if parts.is_acyl_substituent {
        "acyl"
    } else {
        "single"
    }
}
pub fn promote_acyl_substituent_name(parts: &mut AssemblyParts) {
    if !parts.is_substituent
        || parts.is_acyl_substituent
        || parts.is_double_attach
        || parts.is_triple_attach
        || parts.is_ring
        || parts.retained_name.is_some()
        || parts.retained_substituent_name.is_some()
        || parts.principal_group.is_some()
        || !parts.a_prefixes.is_empty()
        || !parts.parent_charges.is_empty()
        || !parts.hydro_operations.is_empty()
        || parts.attachment_locant != "1"
    {
        return;
    }
    let carbonyls: Vec<_> = parts
        .substituents
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name == "oxo" && s.locants == ["1"])
        .map(|(i, _)| i)
        .collect();
    if carbonyls.len() != 1
        || parts
            .unsaturations
            .iter()
            .any(|u| u.locants.contains(&"1".to_owned()))
        || ((parts.parent_length == 1 || parts.parent_length == 2)
            && !parts.unsaturations.is_empty())
    {
        return;
    }
    parts.is_acyl_substituent = true;
    parts.substituents.remove(carbonyls[0]);
}
fn promote_carbamimidoyl(parts: &mut AssemblyParts) {
    if !parts.is_substituent
        || parts.retained_substituent_name.is_some()
        || parts.is_double_attach
        || parts.is_triple_attach
        || parts.is_ring
        || parts.retained_name.is_some()
        || parts.principal_group.is_some()
        || !parts.unsaturations.is_empty()
        || !parts.a_prefixes.is_empty()
        || !parts.parent_charges.is_empty()
        || parts.parent_length != 1
        || parts.attachment_locant != "1"
        || parts.substituents.len() != 2
        || substituent_attachment_kind(parts) != "single"
    {
        return;
    }
    let (mut amine, mut imine) = (None, None);
    for item in &parts.substituents {
        let text = strip_outer_parentheses(&item.name);
        if let Some(before) = text.strip_suffix("imino") {
            if imine.is_some() {
                return;
            }
            imine = Some(before.to_owned());
        } else if let Some(before) = text.strip_suffix("amino") {
            if amine.is_some() {
                return;
            }
            amine = Some(before.to_owned());
        } else {
            return;
        }
    }
    let (Some(amine), Some(imine)) = (amine, imine) else {
        return;
    };
    let simple = [
        "methyl", "ethyl", "propyl", "butyl", "phenyl", "benzyl", "hydroxy", "methoxy", "ethoxy",
        "cyano", "fluoro", "chloro", "amino",
    ];
    let mut prefixes = vec![];
    for (ligands, loc) in [(amine, "N"), (imine, "N'")] {
        if ligands.is_empty() {
            continue;
        }
        if ligands.contains('(') || ligands.starts_with("N-") || ligands.starts_with("N,") {
            return;
        }
        let count = multipliers::candidate_splits(&ligands)
            .into_iter()
            .find(|(_, s)| simple.contains(&s.as_str()))
            .map_or(1, |(n, _)| n);
        prefixes.push((
            ligands.clone(),
            format!("{}-{ligands}", vec![loc; count].join(",")),
        ));
    }
    prefixes.sort_by_key(|p| substituent_sort_key(&p.0));
    parts.retained_substituent_name = Some(format!(
        "{}carbamimidoyl",
        prefixes
            .iter()
            .map(|p| p.1.as_str())
            .collect::<Vec<_>>()
            .join("-")
    ));
    for (loc, _) in &mut parts.stereo_features {
        if loc == "1" {
            loc.clear();
        }
    }
    parts.substituents.clear();
}
pub fn promote_retained_substituent_name(parts: &mut AssemblyParts) {
    promote_carbamimidoyl(parts);
    if !parts.is_substituent
        || parts.retained_substituent_name.is_some()
        || parts.is_double_attach
        || parts.is_triple_attach
        || parts.is_ring
        || parts.retained_name.is_some()
        || parts.principal_group.is_some()
        || !parts.unsaturations.is_empty()
        || !parts.a_prefixes.is_empty()
        || !parts.parent_charges.is_empty()
        || parts.substituents.len() != 1
    {
        return;
    }
    let branch = &parts.substituents[0];
    if branch.locants.len() != 1 {
        return;
    }
    let kind = substituent_attachment_kind(parts);
    let mut retained = match (
        parts.parent_length,
        parts.attachment_locant.as_str(),
        branch.name.as_str(),
        branch.locants[0].as_str(),
        kind,
    ) {
        (1, "1", "phenyl", "1", "single") => Some("benzyl".to_owned()),
        (3, "2", "methyl", "2", "single") => Some("tert-butyl".to_owned()),
        (1, "1", "phenyl", "1", "acyl") => Some("benzoyl".to_owned()),
        _ => None,
    };
    if retained.is_none()
        && parts.parent_length == 1
        && parts.attachment_locant == "1"
        && kind == "acyl"
    {
        let name = strip_outer_parentheses(&branch.name);
        if !name.starts_with("N-") && !name.starts_with("N,") {
            for (ending, word) in [("amino", "carbamoyl"), ("sulfonamido", "sulfonylcarbamoyl")] {
                if let Some(prefix) = name.strip_suffix(ending) {
                    retained = Some(format!("{prefix}{word}"));
                    break;
                }
            }
        }
    }
    if let Some(name) = retained {
        parts.retained_substituent_name = Some(name);
        parts.substituents.remove(0);
    }
}
pub fn parent_stem_and_terminal(parts: &AssemblyParts) -> Result<(String, String), String> {
    if let Some(parent) = parts
        .parent_hydride
        .as_ref()
        .filter(|parent| parent.is_fusion_parent())
    {
        if parts.is_substituent
            && let Some(stem) = parent
                .hydride_metadata
                .as_ref()
                .and_then(|metadata| metadata.derivative_stem.as_ref())
        {
            return Ok((stem.clone(), String::new()));
        }
        let name = parent
            .base_name()
            .ok_or("openclatura: fusion parent has no audited spelling")?;
        return Ok(split_parent_terminal(name));
    }
    if parts.parent_hydride.is_none()
        && let Some(plan) = &parts.fusion_plan
    {
        return Ok(
            if let Some(stem) = plan.rendered_base_name.strip_suffix('e') {
                (stem.into(), "e".into())
            } else {
                (plan.rendered_base_name.clone(), String::new())
            },
        );
    }
    if has_ionic_retained_parent(parts)
        && let Some(name) = parts
            .retained_name
            .as_ref()
            .and_then(|name| RULES.charges.retained_ionic_n_parents.get(name))
    {
        return Ok((name.clone(), String::new()));
    }
    if let Some(name) = inferred_ionic_retained_parent(parts) {
        return Ok((name, String::new()));
    }
    if let Some(name) = &parts.retained_name {
        if parts.is_substituent
            && let Some(spec) = super::retained_specs::retained_parent_spec(Some(name))
            && let Some(stem) = spec.substituent_stem
        {
            return Ok((stem, spec.substituent_terminal.unwrap_or_default()));
        }
        return Ok(if let Some(stem) = name.strip_suffix('e') {
            (stem.into(), "e".into())
        } else {
            (name.clone(), String::new())
        });
    }
    if let Some(name) = parts
        .parent_hydride
        .as_ref()
        .and_then(|parent| parent.base_name())
    {
        return Ok(split_parent_terminal(name));
    }
    let stem = stems::stem_for(parts.parent_length);
    Ok((
        if parts.is_bicycle {
            let (x, y, z) = parts.bicycle_xyz;
            format!(
                "{}{stem}",
                super::ring_renderer::render_ring_descriptor("bicyclo", &[x, y, z])
            )
        } else if parts.is_spiro {
            let (x, y) = parts.spiro_xy;
            format!(
                "{}{stem}",
                super::ring_renderer::render_ring_descriptor("spiro", &[x, y])
            )
        } else if parts.is_polycycle {
            let descriptor = parts
                .polycycle_descriptor
                .as_deref()
                .ok_or("openclatura: polycyclic parent has no audited descriptor")?;
            format!("{descriptor}{stem}")
        } else if parts.is_ring {
            format!("cyclo{stem}")
        } else {
            stem
        },
        "e".into(),
    ))
}
fn split_parent_terminal(name: &str) -> (String, String) {
    if let Some(stem) = name.strip_suffix('e') {
        (stem.into(), "e".into())
    } else {
        (name.into(), String::new())
    }
}
pub fn format_unsaturations(
    parts: &AssemblyParts,
    stem: &str,
    omit_locants: bool,
) -> (String, String) {
    let mut stem = stem.to_owned();
    let mut unsats: Vec<_> = parts.unsaturations.iter().collect();
    unsats.sort_by_key(|u| {
        RULES
            .assembly
            .unsaturation_order
            .get(&u.bond_key)
            .copied()
            .unwrap_or(99)
    });
    let mut pieces = vec![];
    for (i, u) in unsats.iter().enumerate() {
        let count = u.locants.len().max(1);
        let infix = bonds::unsaturation_infix(&u.bond_key, count);
        let base = infix.strip_prefix('a').unwrap_or(&infix);
        if i == 0 && count > 1 && (!u.locants.is_empty() || !elision::is_vowel_start(base)) {
            stem.push('a');
        }
        if !u.locants.is_empty()
            && !omit_locants
            && !parts.elided_unsaturation_locants.contains(&u.bond_key)
        {
            let mut locs = u.locants.clone();
            locs.sort_by_key(|s| parse_locant(s));
            pieces.push(format!("-{}-{base}", locs.join(",")));
        } else {
            pieces.push(base.into());
        }
    }
    (stem, pieces.concat())
}
pub fn always_print_substituent_locant(parts: &AssemblyParts) -> bool {
    if parts.parent_length == 1 {
        return false;
    }
    if parts.is_bicycle || parts.is_spiro || parts.is_polycycle {
        return true;
    }
    if parts.is_ring
        && (!parts.a_prefixes.is_empty()
            || parts
                .retained_name
                .as_deref()
                .is_some_and(|s| s != "benzene"))
    {
        return true;
    }
    if super::retained_specs::retained_parent_spec(parts.retained_name.as_deref())
        .is_some_and(|spec| spec.attachment_policy.print_substituent_locant)
    {
        return true;
    }
    parent_stem_and_terminal(parts)
        .ok()
        .is_some_and(|(stem, _)| {
            RULES
                .assembly
                .ambiguous_connection_substituent_stems
                .iter()
                .any(|s| stem.ends_with(s))
        })
}
pub fn format_substituent_tail(
    parts: &AssemblyParts,
    stem: &str,
    _terminal: &str,
) -> (String, String, String, String) {
    let kind = substituent_attachment_kind(parts);
    let mut suffix = RULES.assembly.substituent_attachment_suffixes[kind].clone();
    if kind == "acyl" && parts.parent_length == 1 && !parts.substituents.is_empty() {
        suffix = RULES.assembly.substituent_attachment_suffixes["acyl_on_branch"].clone();
    }
    if parts.is_acyl_substituent {
        if suffix == RULES.assembly.substituent_attachment_suffixes["acyl_on_branch"] {
            return (String::new(), String::new(), suffix, String::new());
        }
        if parts.parent_length == 1 || parts.parent_length == 2 {
            return (
                (if parts.parent_length == 1 {
                    "form"
                } else {
                    "acet"
                })
                .into(),
                String::new(),
                "yl".into(),
                String::new(),
            );
        }
        let (stem, unsat) = if parts.unsaturations.is_empty() {
            (stem.to_owned(), "an".into())
        } else {
            format_unsaturations(parts, stem, false)
        };
        return (stem, unsat, suffix, String::new());
    }
    let always = parts.spiro_prefix_composition || always_print_substituent_locant(parts);
    let two = !parts.is_ring
        && parts.parent_length == 2
        && parts.attachment_locant == "1"
        && parts.unsaturations.len() == 1
        && parts.unsaturations[0].locants == ["1"];
    if !always && two {
        let (stem, unsat) = format_unsaturations(parts, stem, true);
        return (
            stem,
            unsat,
            append_charge_suffixes_to_terminal(parts, &suffix),
            String::new(),
        );
    }
    let terminal = if parts.retained_name.as_deref() == Some("benzene") {
        "yl".into()
    } else if (parts.attachment_locant != "1" || !parts.unsaturations.is_empty() || always)
        && parts.parent_length > 1
    {
        format!("-{}-{suffix}", parts.attachment_locant)
    } else {
        suffix
    };
    let (stem, unsat) = if !has_retained_like_parent(parts) && !parts.unsaturations.is_empty() {
        format_unsaturations(parts, stem, false)
    } else {
        (
            stem.to_owned(),
            if !has_retained_like_parent(parts)
                && parts.parent_length > 1
                && (parts.attachment_locant != "1" || always)
            {
                "an".into()
            } else {
                String::new()
            },
        )
    };
    (
        stem,
        unsat,
        append_charge_suffixes_to_terminal(parts, &terminal),
        String::new(),
    )
}
fn sole_group_redundant(parts: &AssemblyParts) -> bool {
    if parts
        .substituents
        .iter()
        .flat_map(|s| &s.locants)
        .any(|l| !l.starts_with(['N', 'O', 'S', 'P']))
        || !parts.a_prefixes.is_empty()
    {
        return false;
    }
    if parts.is_ring {
        return parts.unsaturations.is_empty() && parts.retained_name.as_deref() == Some("benzene");
    }
    !parts.is_substituent && parts.parent_length == 2 && parts.parent_charges.is_empty()
}
fn suffix_modifiers(parts: &AssemblyParts) -> String {
    let mut grouped = std::collections::BTreeMap::new();
    for m in &parts.principal_suffix_modifiers {
        *grouped
            .entry((m.name.clone(), m.locants.clone()))
            .or_insert(0_usize) += 1;
    }
    grouped
        .into_iter()
        .map(|((name, locs), count)| {
            let text = if is_complex_prefix(&name) && !name.starts_with('(') {
                format!("({name})")
            } else {
                name
            };
            let locs = (0..count).flat_map(|_| locs.clone()).collect::<Vec<_>>();
            let loc = if locs.is_empty() {
                String::new()
            } else {
                format!("{}-", locs.join(","))
            };
            format!(
                "{loc}{}{text}",
                if count > 1 {
                    multipliers::basic(count)
                } else {
                    String::new()
                }
            )
        })
        .collect()
}
pub fn format_principal_suffix(
    parts: &AssemblyParts,
    terminal: &str,
) -> Result<(String, String), String> {
    let Some(item) = &parts.principal_group else {
        return Ok((terminal.into(), String::new()));
    };
    let mut group = item
        .resolved_rule
        .as_ref()
        .or_else(|| RULES.functional_groups.get(&item.key))
        .ok_or_else(|| format!("Unsupported principal group {}", item.key))?
        .clone();
    group.suffix = group
        .suffix_for_citation(parts.principal_citation_mode.as_deref())
        .map(str::to_owned);
    // Upstream divergence: a selected group with no citation renderer disappears.
    if group.suffix.as_ref().is_none_or(String::is_empty) {
        return Err(format!("openclatura: no suffix renderer for {}", item.key));
    }
    let mut locs = item.locants.clone();
    locs.sort_by_key(|l| parse_locant(l));
    let mut omit = parts.parent_length == 1;
    if !omit
        && locs == ["1"]
        && (!group.suffix_with_locant
            || (parts.is_ring
                && parts.unsaturations.is_empty()
                && !parts.is_bicycle
                && !parts.is_spiro
                && !parts.is_polycycle
                && !parts.spiro_prefix_composition
                && parts.retained_name.is_none())
            || sole_group_redundant(parts))
    {
        omit = true;
    }
    let mut suffix = render_principal_suffix(&group, locs.len());
    if group.has_capability("element_locanted_suffix") && omit {
        suffix = format!("-{suffix}");
    }
    if !parts.principal_suffix_modifiers.is_empty()
        && group.families.contains("hydrazone")
        && suffix.ends_with("hydrazone")
    {
        suffix = format!(
            "{}{}hydrazone",
            &suffix[..suffix.len() - 9],
            suffix_modifiers(parts)
        );
    }
    let terminal = if elision::is_vowel_start(&suffix) {
        String::new()
    } else {
        terminal.into()
    };
    if parts.elide_principal_group_locants {
        omit = true;
    }
    Ok((
        terminal,
        if group.suffix_with_locant && !locs.is_empty() && !omit {
            format!("-{}-{suffix}", locs.join(","))
        } else {
            suffix
        },
    ))
}
fn redundant_unsaturation(parts: &AssemblyParts) -> bool {
    if parts.is_spiro_component
        || parts.unsaturations.len() != 1
        || parts.unsaturations[0].locants.len() != 1
        || !parts.a_prefixes.is_empty()
    {
        return false;
    }
    if !parts.is_ring {
        return parts.parent_length == 2
            && parts.principal_group.as_ref().is_none_or(|g| {
                [
                    "alcohol", "thiol", "amine", "ketone", "olate", "thiolate", "aminium",
                ]
                .contains(&g.key.as_str())
            });
    }
    !parts.is_bicycle
        && !parts.is_spiro
        && !parts.is_polycycle
        && parts.substituents.is_empty()
        && parts.principal_group.is_none()
        && parts.parent_charges.is_empty()
}
pub fn format_parent_tail(
    parts: &AssemblyParts,
    stem: &str,
    terminal: &str,
) -> Result<(String, String, String, String), String> {
    let (stem, unsat) = if !has_retained_like_parent(parts) {
        if parts.unsaturations.is_empty() {
            (stem.to_owned(), "an".into())
        } else {
            format_unsaturations(
                parts,
                stem,
                redundant_unsaturation(parts) && !parts.spiro_prefix_composition,
            )
        }
    } else {
        (stem.to_owned(), String::new())
    };
    let (mut terminal, mut suffix) = if parts.retained_absorbs_principal_group {
        (terminal.to_owned(), String::new())
    } else {
        format_principal_suffix(parts, terminal)?
    };
    let charge = parent_charge_suffix(parts);
    if !charge.is_empty() {
        terminal.clear();
        suffix = format!("{charge}{suffix}");
    }
    Ok((stem, unsat, terminal, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiro_composition_keeps_attachment_and_parent_unsaturation_locants() {
        use super::super::assembly_parts::UnsaturationItem;
        let mut parts = AssemblyParts::new(6);
        parts.is_ring = true;
        parts.is_substituent = true;
        parts.spiro_prefix_composition = true;
        assert_eq!(format_substituent_tail(&parts, "cyclohex", "e").2, "-1-yl");
        parts.is_substituent = false;
        parts.unsaturations.push(UnsaturationItem {
            bond_key: "double".into(),
            locants: vec!["1".into()],
            ..Default::default()
        });
        assert_eq!(
            format_parent_tail(&parts, "cyclohex", "e").unwrap().1,
            "-1-en"
        );
    }
}
