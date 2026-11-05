//! Parent stems, unsaturation and principal/substituent tails.
use super::{
    assembly_charge::{
        append_charge_suffixes_to_terminal, has_ionic_retained_parent, has_parent_anion_suffix,
        has_retained_like_parent, inferred_ionic_retained_parent, parent_anion_cites_locants,
        parent_charge_suffix,
    },
    assembly_parts::AssemblyParts,
    assembly_prefixes::substituent_sort_key,
    assembly_utils::parse_locant,
    formatting::{is_complex_prefix, phenylamino_spelling, strip_outer_parentheses},
    nomenclature::RULES,
    principal_suffixes::render_principal_suffix,
    rules::{bonds, elision, multipliers, stems},
    spelling,
};

fn functional_ring_parent(name: &str, key: &str) -> Option<String> {
    if name != "benzene" {
        return None;
    }
    let base = match key {
        "alcohol" => "phenol",
        "amine" => "aniline",
        "aminium" => "anilinium",
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
fn halide_word(key: &str) -> Option<&'static str> {
    let suffix = RULES.functional_groups.get(key)?.suffix.as_deref()?;
    suffix.strip_prefix("oyl ")
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
        (1, "nitrile", 1) => "formonitrile",
        (2, "nitrile", 1) => "acetonitrile",
        (1, "nitrile_O", 1) => "formonitrile oxide",
        (2, "nitrile_O", 1) => "acetonitrile oxide",
        (1, "aldehyde", 1) => "formaldehyde",
        (2, "aldehyde", 1) => "acetaldehyde",
        (1, "ester" | "carboxylate", 1) => "formate",
        (2, "ester" | "carboxylate", 1) => "acetate",
        (2, "carboxylic_acid", 2) => "oxalic acid",
        (2, "amide", 2) => "oxamide",
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
    let text = phenylamino_spelling(&text);
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
    // Unsubstituted anisole is the PIN (P-34.1.1.4).
    if parts.retained_name.as_deref() == Some("benzene")
        && !parts.is_substituent
        && parts.principal_group.is_none()
        && let [item] = parts.substituents.as_slice()
        && item.name == "methoxy"
        && item.locants.len() == 1
    {
        parts.retained_name = Some("anisole".into());
        parts.substituents.clear();
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
            // Substituted formyl parents follow P-65.1.8; substituted acetaldehyde
            // and N-substituted oxamide keep their systematic names here.
            if !parts.substituents.is_empty()
                && [
                    "oxalic acid",
                    "formonitrile",
                    "formonitrile oxide",
                    "formaldehyde",
                    "acetaldehyde",
                    "oxamide",
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
fn substituent_attachment_kind(parts: &AssemblyParts) -> &'static str {
    match parts.attachment_order {
        3 => "triple",
        2 => "double",
        _ if parts.is_acyl_substituent => "acyl",
        _ => "single",
    }
}
pub fn promote_acyl_substituent_name(parts: &mut AssemblyParts) {
    if !parts.is_substituent
        || parts.is_acyl_substituent
        || matches!(parts.attachment_order, 2 | 3)
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
        .filter_map(|(i, s)| {
            let infix = match s.name.as_str() {
                "oxo" => "",
                "sulfanylidene" => "thio",
                "selanylidene" => "seleno",
                "tellanylidene" => "telluro",
                _ => return None,
            };
            (s.locants == ["1"]).then_some((i, infix))
        })
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
    // P-65.1.5.1: thio-, seleno- and telluroacyl groups, as in ethanethioyl.
    let (index, infix) = carbonyls[0];
    parts.is_acyl_substituent = true;
    parts.acyl_infix = infix;
    parts.substituents.remove(index);
}
fn promote_carbamimidoyl(parts: &mut AssemblyParts) {
    if !parts.is_substituent
        || parts.retained_substituent_name.is_some()
        || matches!(parts.attachment_order, 2 | 3)
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
        let text = phenylamino_spelling(&text);
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
        let prefix = format!("{}-{ligands}", vec![loc; count].join(","));
        prefixes.push((ligands, prefix));
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
        || matches!(parts.attachment_order, 2 | 3)
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
        (1, "1", "phenyl", "1", "acyl") => Some(match parts.acyl_infix {
            "" => "benzoyl".to_owned(),
            infix => format!("benzenecarbo{infix}yl"),
        }),
        _ => None,
    };
    if retained.is_none()
        && parts.parent_length == 1
        && parts.attachment_locant == "1"
        && kind == "acyl"
    {
        let name = strip_outer_parentheses(&branch.name);
        let name = phenylamino_spelling(&name);
        if !name.starts_with("N-") && !name.starts_with("N,") {
            for (ending, word) in [("amino", "carbamo"), ("sulfonamido", "sulfonylcarbamo")] {
                if let Some(prefix) = name.strip_suffix(ending) {
                    retained = Some(format!("{prefix}{word}{}yl", parts.acyl_infix));
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
        && !has_parent_anion_suffix(parts)
        && let Some(name) = parts
            .retained_name
            .as_ref()
            .and_then(|name| RULES.charges.retained_ionic_n_parents.get(name))
    {
        return Ok((name.clone(), String::new()));
    }
    if let Some(name) = inferred_ionic_retained_parent(parts) {
        if has_parent_anion_suffix(parts)
            && let Some((neutral, _)) = RULES
                .charges
                .retained_ionic_n_parents
                .iter()
                .find(|(_, ionic)| **ionic == name)
        {
            return Ok(split_parent_terminal(neutral));
        }
        return Ok((name, String::new()));
    }
    if let Some(name) = &parts.retained_name {
        if parts.is_substituent
            && !has_parent_anion_suffix(parts)
            && let Some(spec) = super::retained_specs::retained_parent_spec(Some(name))
            && let Some(stem) = spec.substituent_stem
        {
            return Ok((stem, spec.substituent_terminal.unwrap_or_default()));
        }
        // Expanded valence belongs to the retained parent whether its oxo
        // ligands are cited as suffixes or prefixes.
        let mut descriptors = Vec::new();
        for (locant, symbol) in &parts.parent_atom_symbols_by_locant {
            let has_oxo = parts
                .principal_group
                .as_ref()
                .is_some_and(|g| g.key == "ketone" && g.locants.contains(locant))
                || parts
                    .substituents
                    .iter()
                    .any(|s| s.name == "oxo" && s.locants.contains(locant));
            if !has_oxo
                || symbol == "C"
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
            let bonding = parts
                .parent_atom_hydrogens_by_locant
                .get(locant)
                .copied()
                .unwrap_or(0)
                + parts
                    .parent_bond_orders_by_locants
                    .iter()
                    .filter(|((a, b), _)| a == locant || b == locant)
                    .map(|(_, order)| order)
                    .sum::<usize>()
                + parts
                    .parent_external_bonds_by_locant
                    .get(locant)
                    .into_iter()
                    .flatten()
                    .map(|(_, order, _)| order)
                    .sum::<usize>();
            if bonding > element.standard_valence {
                descriptors.push((locant, bonding));
            }
        }
        if descriptors.is_empty() {
            return Ok(split_parent_terminal(name));
        }
        descriptors.sort_by_key(|(locant, _)| parse_locant(locant));
        let (hydrogen, parent) = name
            .split_once('-')
            .filter(|(run, _)| run.split(',').all(|locant| locant.ends_with('H')))
            .map_or((None, name.as_str()), |(hydrogen, parent)| {
                (Some(hydrogen), parent)
            });
        let parent = if let Some((skeletal_locants, stem)) =
            parent.split_once('-').filter(|(run, _)| {
                run.split(',').all(|locant| {
                    parts
                        .parent_atom_symbols_by_locant
                        .get(locant)
                        .is_some_and(|symbol| symbol != "C")
                }) && descriptors
                    .iter()
                    .all(|(locant, _)| run.split(',').any(|cited| cited == locant.as_str()))
            }) {
            let skeletal_locants = skeletal_locants
                .split(',')
                .map(|locant| {
                    descriptors
                        .iter()
                        .find(|(expanded, _)| expanded.as_str() == locant)
                        .map_or_else(
                            || locant.to_owned(),
                            |(_, bonding)| format!("{locant}lambda^{bonding}"),
                        )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{skeletal_locants}-{stem}")
        } else {
            let lambda = descriptors
                .into_iter()
                .map(|(locant, bonding)| format!("{locant}lambda^{bonding}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{lambda}-{parent}")
        };
        let name = if let Some(hydrogen) = hydrogen {
            format!("{hydrogen}-{parent}")
        } else {
            parent
        };
        return Ok(split_parent_terminal(&name));
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
) -> (String, String, bool) {
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
    let mut cites_locants = false;
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
            cites_locants = true;
            let mut locs = u.locants.clone();
            locs.sort_by_key(|s| parse_locant(s));
            pieces.push(format!("-{}-{base}", locs.join(",")));
        } else {
            pieces.push(base.into());
        }
    }
    (stem, pieces.concat(), cites_locants)
}
pub fn always_print_substituent_locant(parts: &AssemblyParts) -> bool {
    if parts.isotope_explicit_locants {
        return true;
    }
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
        .is_some_and(|spec| spec.print_substituent_locant)
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
#[derive(Default)]
pub(super) struct ParentTail {
    pub(super) stem: String,
    pub(super) unsaturation: String,
    pub(super) terminal: String,
    pub(super) suffix: String,
    pub(super) cites_locants: bool,
}

pub(super) fn format_substituent_tail(parts: &AssemblyParts, stem: &str) -> ParentTail {
    let kind = substituent_attachment_kind(parts);
    let mut suffix = RULES.assembly.substituent_attachment_suffixes[kind].clone();
    if parts.is_ring_assembly() {
        // P-29.3.5: junction locants stay inside brackets; the free valence is outside.
        return ParentTail {
            stem: format!("[{}]", assembly_suffix_stem(parts, true)),
            terminal: format!("-{}-{suffix}", parts.attachment_locant),
            cites_locants: true,
            ..Default::default()
        };
    }
    if kind == "acyl" && parts.parent_length == 1 && !parts.substituents.is_empty() {
        suffix = RULES.assembly.substituent_attachment_suffixes["acyl_on_branch"].clone();
    }
    if parts.is_acyl_substituent {
        let infix = parts.acyl_infix;
        if suffix == RULES.assembly.substituent_attachment_suffixes["acyl_on_branch"] {
            return ParentTail {
                terminal: if infix.is_empty() {
                    suffix
                } else {
                    format!("carbono{infix}yl")
                },
                ..Default::default()
            };
        }
        if !infix.is_empty() {
            let (stem, unsaturation, cites_locants) = if parts.unsaturations.is_empty() {
                (stem.to_owned(), "ane".into(), false)
            } else {
                let (stem, unsaturation, cites_locants) = format_unsaturations(parts, stem, false);
                (stem, format!("{unsaturation}e"), cites_locants)
            };
            return ParentTail {
                stem,
                unsaturation,
                terminal: format!("{infix}yl"),
                suffix: String::new(),
                cites_locants,
            };
        }
        if parts.parent_length == 1 || parts.parent_length == 2 {
            return ParentTail {
                stem: (if parts.parent_length == 1 {
                    "form"
                } else {
                    "acet"
                })
                .into(),
                terminal: "yl".into(),
                ..Default::default()
            };
        }
        let (stem, unsaturation, cites_locants) = if parts.unsaturations.is_empty() {
            (stem.to_owned(), "an".into(), false)
        } else {
            format_unsaturations(parts, stem, false)
        };
        return ParentTail {
            stem,
            unsaturation,
            terminal: suffix,
            suffix: String::new(),
            cites_locants,
        };
    }
    if has_parent_anion_suffix(parts) {
        // Charged derivatives start from the full hydride, including methane's
        // `an` and the `e` that a multiplied anion suffix may retain.
        let (stem, unsaturation, unsaturation_cites_locants) = if has_retained_like_parent(parts) {
            (stem.to_owned(), String::new(), false)
        } else if parts.unsaturations.is_empty() {
            (stem.to_owned(), "an".into(), false)
        } else {
            format_unsaturations(parts, stem, false)
        };
        let cite_attachment = parts.parent_length > 1
            && (parent_anion_cites_locants(parts)
                || parts.attachment_locant != "1"
                || !parts.unsaturations.is_empty()
                || always_print_substituent_locant(parts));
        let attachment = if cite_attachment {
            format!(
                "-{}-{suffix}",
                parts.stereochemical_locant(&parts.attachment_locant)
            )
        } else {
            suffix
        };
        // The caller has already resolved this same parent successfully.
        let hydride_terminal =
            parent_stem_and_terminal(parts).map_or(String::new(), |(_, terminal)| terminal);
        let charged_terminal = spelling::join(&hydride_terminal, &parent_charge_suffix(parts));
        return ParentTail {
            stem,
            unsaturation,
            terminal: spelling::join(&charged_terminal, &attachment),
            cites_locants: unsaturation_cites_locants
                || cite_attachment
                || parent_anion_cites_locants(parts)
                || !super::assembly_charge::positive_parent_n_charges(parts).is_empty(),
            ..Default::default()
        };
    }
    let always = parts.spiro_prefix_composition || always_print_substituent_locant(parts);
    let two = !parts.is_ring
        && parts.parent_length == 2
        && parts.attachment_locant == "1"
        && parts.unsaturations.len() == 1
        && parts.unsaturations[0].locants == ["1"];
    if !always && two {
        let (stem, unsaturation, cites_locants) = format_unsaturations(parts, stem, true);
        let (terminal, charge_cites_locants) = append_charge_suffixes_to_terminal(parts, &suffix);
        return ParentTail {
            stem,
            unsaturation,
            terminal,
            suffix: String::new(),
            cites_locants: cites_locants || charge_cites_locants,
        };
    }
    let (terminal, attachment_cites_locants) = if parts.retained_name.as_deref() == Some("benzene")
    {
        ("yl".into(), false)
    } else if (parts.attachment_locant != "1" || !parts.unsaturations.is_empty() || always)
        && parts.parent_length > 1
    {
        (
            format!(
                "-{}-{suffix}",
                parts.stereochemical_locant(&parts.attachment_locant)
            ),
            true,
        )
    } else {
        (suffix, false)
    };
    let (stem, unsat, cites_locants) =
        if !has_retained_like_parent(parts) && !parts.unsaturations.is_empty() {
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
                false,
            )
        };
    let (terminal, charge_cites_locants) = append_charge_suffixes_to_terminal(parts, &terminal);
    ParentTail {
        stem,
        unsaturation: unsat,
        terminal,
        suffix: String::new(),
        cites_locants: cites_locants || attachment_cites_locants || charge_cites_locants,
    }
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
        // Upstream divergence: P-14.3.4.2(c) requires a monosubstituted ring.
        return parts.unsaturations.is_empty()
            && (parts.retained_name.as_deref() == Some("benzene")
                || (parts.is_simple_ring()
                    && !parts.spiro_prefix_composition
                    && parts.retained_name.is_none()));
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
) -> Result<(String, String, bool), String> {
    let Some(item) = &parts.principal_group else {
        return Ok((terminal.into(), String::new(), false));
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
        && !parts.is_ring_assembly()
        && !parts.isotope_explicit_locants
        && locs == ["1"]
        && (!group.suffix_with_locant || sole_group_redundant(parts))
    {
        omit = true;
    }
    let mut suffix = render_principal_suffix(&group, locs.len());
    let mut modifier_cites_locants = false;
    if group.has_capability("element_locanted_suffix") && omit {
        suffix = format!("-{suffix}");
    }
    if !parts.principal_suffix_modifiers.is_empty()
        && group.families.contains("hydrazone")
        && suffix.ends_with("hydrazone")
    {
        modifier_cites_locants = parts
            .principal_suffix_modifiers
            .iter()
            .any(|modifier| !modifier.locants.is_empty());
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
    let anion_cites_locants = parent_anion_cites_locants(parts);
    if anion_cites_locants {
        // Once the anion position is cited, preserve the functional suffix
        // locants too, including al and nitrile's normally implicit locants.
        omit = false;
    }
    let cites_locants =
        (group.suffix_with_locant || anion_cites_locants) && !locs.is_empty() && !omit;
    Ok((
        terminal,
        if cites_locants {
            format!("-{}-{suffix}", parts.stereochemical_locants(&locs))
        } else {
            suffix
        },
        cites_locants || modifier_cites_locants,
    ))
}
fn redundant_unsaturation(parts: &AssemblyParts) -> bool {
    if parts.isotope_explicit_locants {
        return false;
    }
    if parts.is_spiro_component
        || parts.unsaturations.len() != 1
        || parts.unsaturations[0].locants.len() != 1
        || !parts.a_prefixes.is_empty()
    {
        return false;
    }
    if !parts.is_ring {
        // No isomer can move the bond of ethene or ethyne (P-14.3.4.4: ethenesulfinylium).
        return parts.parent_length == 2;
    }
    !parts.is_bicycle
        && !parts.is_spiro
        && !parts.is_polycycle
        && parts.substituents.is_empty()
        && parts.principal_group.is_none()
        && parts.parent_charges.is_empty()
}
pub(super) fn format_parent_tail(
    parts: &AssemblyParts,
    stem: &str,
    terminal: &str,
) -> Result<ParentTail, String> {
    if parts.is_ring_assembly() {
        // P-28.2.1: an assembly name always cites its junction locants.
        let (terminal, suffix, _) = format_principal_suffix(parts, "e")?;
        let name = assembly_suffix_stem(parts, terminal.is_empty());
        return Ok(ParentTail {
            stem: if suffix.is_empty() {
                name
            } else {
                format!("[{name}]")
            },
            suffix,
            cites_locants: true,
            ..Default::default()
        });
    }
    let (stem, unsat, unsaturation_cites_locants) = if !has_retained_like_parent(parts) {
        if parts.unsaturations.is_empty() {
            (stem.to_owned(), "an".into(), false)
        } else {
            format_unsaturations(
                parts,
                stem,
                redundant_unsaturation(parts) && !parts.spiro_prefix_composition,
            )
        }
    } else {
        (stem.to_owned(), String::new(), false)
    };
    let (mut suffix_terminal, mut suffix, suffix_cites_locants) =
        if parts.retained_absorbs_principal_group {
            (terminal.to_owned(), String::new(), false)
        } else {
            format_principal_suffix(parts, terminal)?
        };
    let charge = parent_charge_suffix(parts);
    let charge_cites_locants = !charge.is_empty();
    if has_parent_anion_suffix(parts) {
        let charged_terminal = spelling::join(terminal, &charge);
        suffix_terminal = spelling::join(&charged_terminal, &suffix);
        // Keep the principal suffix separate for added-hydrogen placement.
        suffix_terminal.truncate(suffix_terminal.len() - suffix.len());
    } else if !charge.is_empty() {
        suffix = format!("{charge}{suffix}");
        suffix_terminal = spelling::join(terminal, &suffix);
        suffix_terminal.truncate(suffix_terminal.len() - suffix.len());
    }
    Ok(ParentTail {
        stem,
        unsaturation: unsat,
        terminal: suffix_terminal,
        suffix,
        cites_locants: unsaturation_cites_locants || suffix_cites_locants || charge_cites_locants,
    })
}

fn assembly_suffix_stem(parts: &AssemblyParts, elide: bool) -> String {
    let name = parts.retained_name.as_deref().unwrap_or_default();
    if elide {
        if let Some(stem) = name.strip_suffix("e)") {
            return format!("{stem})");
        }
        if let Some(stem) = name.strip_suffix('e') {
            return stem.into();
        }
    }
    name.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_parent(parts: &AssemblyParts) -> String {
        let (stem, terminal) = parent_stem_and_terminal(parts).unwrap();
        let tail = if parts.is_substituent {
            format_substituent_tail(parts, &stem)
        } else {
            format_parent_tail(parts, &stem, &terminal).unwrap()
        };
        format!(
            "{}{}{}{}",
            tail.stem, tail.unsaturation, tail.terminal, tail.suffix
        )
    }

    #[test]
    fn anion_composition_preserves_the_full_hydride_until_each_join() {
        use super::super::assembly_parts::{AnionSuffix, PrincipalGroupItem};
        let mut parts = AssemblyParts::new(6);
        parts.is_ring = true;
        parts.retained_name = Some("piperazine".into());
        parts.anion_suffix = Some(AnionSuffix {
            locants: vec!["1".into(), "4".into()],
            cite_locants: true,
        });
        assert_eq!(rendered_parent(&parts), "piperazine-1,4-diide");
        parts.principal_group = Some(PrincipalGroupItem::new("amine", vec!["2".into()]));
        assert_eq!(rendered_parent(&parts), "piperazine-1,4-diid-2-amine");

        let mut parts = AssemblyParts::new(4);
        parts.anion_suffix = Some(AnionSuffix {
            locants: vec!["2".into()],
            cite_locants: true,
        });
        parts.principal_group = Some(PrincipalGroupItem::new("aldehyde", vec!["1".into()]));
        assert_eq!(rendered_parent(&parts), "butan-2-id-1-al");
        parts.elide_principal_group_locants = true;
        assert_eq!(rendered_parent(&parts), "butan-2-id-1-al");
    }

    #[test]
    fn anion_substituents_keep_full_hydrides_and_elide_before_attachment() {
        use super::super::assembly_parts::AnionSuffix;
        let mut parts = AssemblyParts::new(3);
        parts.is_substituent = true;
        parts.attachment_locant = "2".into();
        parts.anion_suffix = Some(AnionSuffix {
            locants: vec!["2".into()],
            cite_locants: true,
        });
        assert_eq!(rendered_parent(&parts), "propan-2-id-2-yl");

        parts.parent_length = 1;
        parts.attachment_locant = "1".into();
        parts.anion_suffix = Some(AnionSuffix {
            locants: vec!["1".into()],
            cite_locants: false,
        });
        assert_eq!(rendered_parent(&parts), "methanidyl");

        parts.parent_length = 6;
        parts.is_ring = true;
        parts.retained_name = Some("benzene".into());
        assert_eq!(rendered_parent(&parts), "benzenidyl");
    }

    #[test]
    fn anion_charge_stacks_keep_route_order_and_cation_only_spelling() {
        use super::super::assembly_parts::{AnionSuffix, ParentChargeItem};
        let mut parts = AssemblyParts::new(5);
        parts.is_ring = true;
        parts.retained_name = Some("pyrazole".into());
        parts.parent_charges = vec![ParentChargeItem {
            locant: "2".into(),
            symbol: "N".into(),
            charge: 1,
            ..Default::default()
        }];
        parts.anion_suffix = Some(AnionSuffix {
            locants: vec!["1".into()],
            cite_locants: true,
        });
        assert_eq!(rendered_parent(&parts), "pyrazol-1-ide-2-ium");

        parts.parent_length = 6;
        parts.retained_name = Some("piperidine".into());
        parts.parent_charges[0].locant = "1".into();
        parts.anion_suffix.as_mut().unwrap().locants = vec!["4".into()];
        assert_eq!(rendered_parent(&parts), "piperidin-1-ium-4-ide");

        parts.anion_suffix = None;
        parts.retained_name = Some("piperazine".into());
        parts.parent_charges.push(ParentChargeItem {
            locant: "4".into(),
            symbol: "N".into(),
            charge: 1,
            ..Default::default()
        });
        assert_eq!(rendered_parent(&parts), "piperazine-1,4-diium");
    }

    #[test]
    fn cation_suffixes_keep_e_before_multipliers_and_elide_before_ium() {
        use super::super::assembly_parts::ParentChargeItem;
        for (parent, symbol, locants, expected) in [
            ("piperazine", "N", vec!["1", "4"], "piperazine-1,4-diium"),
            ("1,4-dioxane", "O", vec!["1", "4"], "1,4-dioxane-1,4-diium"),
            ("piperidine", "N", vec!["1"], "piperidin-1-ium"),
            ("morpholine", "N", vec!["4"], "morpholin-4-ium"),
        ] {
            let mut parts = AssemblyParts::new(6);
            parts.is_ring = true;
            parts.retained_name = Some(parent.into());
            parts.parent_charges = locants
                .into_iter()
                .map(|locant| ParentChargeItem {
                    locant: locant.into(),
                    symbol: symbol.into(),
                    charge: 1,
                    ..Default::default()
                })
                .collect();
            assert_eq!(rendered_parent(&parts), expected, "{parent}");
        }
    }

    #[test]
    fn anilinium_absorbs_one_aminium_group_and_keeps_substituent_locants() {
        use super::super::assembly_parts::{PrincipalGroupItem, SubstituentItem};
        let mut parts = AssemblyParts::new(6);
        parts.is_ring = true;
        parts.retained_name = Some("benzene".into());
        parts.principal_group = Some(PrincipalGroupItem::new("aminium", vec!["1".into()]));
        parts.substituents = vec![
            SubstituentItem::new("methyl", vec!["N".into(), "N".into(), "N".into()]),
            SubstituentItem::new("methyl", vec!["4".into()]),
        ];
        assert_eq!(
            super::super::assembler::assemble_name_raw(&mut parts).unwrap(),
            "N,N,N,4-tetramethylanilinium"
        );

        parts.substituents.clear();
        parts.retained_name = Some("benzene".into());
        parts.retained_absorbs_principal_group = false;
        parts
            .principal_group
            .as_mut()
            .unwrap()
            .locants
            .push("4".into());
        promote_retained_functional_parent(&mut parts).unwrap();
        assert!(!parts.retained_absorbs_principal_group);
        assert_eq!(rendered_parent(&parts), "benzene-1,4-bis(aminium)");
    }

    #[test]
    fn spiro_composition_keeps_attachment_and_parent_unsaturation_locants() {
        use super::super::assembly_parts::UnsaturationItem;
        let mut parts = AssemblyParts::new(6);
        parts.is_ring = true;
        parts.is_substituent = true;
        let tail = format_substituent_tail(&parts, "cyclohex");
        assert_eq!(tail.terminal, "yl");
        assert!(!tail.cites_locants);
        parts.spiro_prefix_composition = true;
        let tail = format_substituent_tail(&parts, "cyclohex");
        assert_eq!(tail.terminal, "-1-yl");
        assert!(tail.cites_locants);
        parts.is_substituent = false;
        parts.unsaturations.push(UnsaturationItem {
            bond_key: "double".into(),
            locants: vec!["1".into()],
            ..Default::default()
        });
        let tail = format_parent_tail(&parts, "cyclohex", "e").unwrap();
        assert_eq!(tail.unsaturation, "-1-en");
        assert!(tail.cites_locants);
        parts.spiro_prefix_composition = false;
        let tail = format_parent_tail(&parts, "cyclohex", "e").unwrap();
        assert_eq!(tail.unsaturation, "en");
        assert!(!tail.cites_locants);
    }
}
