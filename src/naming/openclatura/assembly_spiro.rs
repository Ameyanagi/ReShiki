//! Graph-numbered spiro components and their plain-name projection.
use super::{
    assembly_parent::{format_principal_suffix, parent_stem_and_terminal},
    assembly_parts::{AssemblyParts, HydroOperation, SubstituentItem, UnsaturationItem},
    assembly_prefixes::format_replacement_prefixes,
    assembly_utils::parse_locant,
    molecule::AtomSet,
    nomenclature::RULES,
    rules::{elements, elision, multipliers, retained::hw_name, stems},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct SpiroAssembly {
    pub parent_locant: String,
    pub side_locant: String,
    pub side_parent_name: String,
    pub side_prefixes: Vec<String>,
    pub side_suffixes: Vec<(String, String)>,
    pub side_stereo: Vec<(String, String)>,
    pub side_substituents: Vec<SubstituentItem>,
    pub side_parts: Option<Box<AssemblyParts>>,
    pub side_prime: String,
    pub cited_first: bool,
    pub continuation: Option<Box<SpiroAssembly>>,
}
impl SpiroAssembly {
    pub fn new(side_locant: impl Into<String>, side_parent_name: impl Into<String>) -> Self {
        Self {
            side_locant: side_locant.into(),
            side_parent_name: side_parent_name.into(),
            side_prime: "'".into(),
            ..Self::default()
        }
    }
}

pub fn prime_side_locant(locant: &str) -> String {
    let (base, annotation) = locant
        .find("lambda^")
        .map_or((locant, ""), |i| locant.split_at(i));
    format!(
        "{base}{}{annotation}",
        if !base.is_empty() && !base.ends_with('\'') {
            "'"
        } else {
            ""
        }
    )
}
fn is_systematic_fusion(parts: &AssemblyParts) -> bool {
    parts
        .parent_hydride
        .as_ref()
        .map_or(parts.fusion_plan.is_some(), |parent| {
            parent.is_systematic_fusion()
        })
}
fn side_replacement_prefixes(parts: &AssemblyParts) -> Vec<String> {
    let mut local = AssemblyParts::new(parts.parent_length);
    local.a_prefixes = parts
        .a_prefixes
        .iter()
        .cloned()
        .map(|mut p| {
            p.locants = p.locants.iter().map(|l| prime_side_locant(l)).collect();
            p
        })
        .collect();
    let mut replacement = format_replacement_prefixes(&local);
    if is_systematic_fusion(parts)
        && let Some(plan) = &parts.fusion_plan
        && !plan.lambda_descriptors.is_empty()
    {
        let descriptors = plan
            .lambda_descriptors
            .iter()
            .map(|d| {
                format!(
                    "{}lambda^{}",
                    prime_side_locant(&d.locant),
                    d.bonding_number
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        replacement = format!("{descriptors}-{replacement}");
    }
    if replacement.is_empty() {
        vec![]
    } else {
        vec![replacement]
    }
}

fn scope_spiro_oxo_hydrogens(parts: &mut AssemblyParts, junctions: &BTreeSet<String>) {
    if !is_systematic_fusion(parts) {
        return;
    }
    let Some(plan) = &parts.fusion_plan else {
        return;
    };
    let Some(group) = &parts.principal_group else {
        return;
    };
    if group.key != "ketone"
        || !plan.audit.confirmed()
        || !plan.derivative_state.bond_delta.compatible
    {
        return;
    }
    let operations: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| o.operation_kind == "additive_hydrogen")
        .collect();
    if operations
        .iter()
        .map(|o| o.locants.len())
        .sum::<usize>()
        .is_multiple_of(2)
    {
        return;
    }
    let mapping = &parts.parent_atom_ids_by_locant;
    if operations.iter().any(|o| {
        o.locants.len() != o.atom_ids.len()
            || o.locants
                .iter()
                .zip(&o.atom_ids)
                .any(|(l, a)| mapping.get(l) != Some(a))
    }) {
        return;
    }
    let oxo: AtomSet = plan
        .derivative_state
        .oxo_operations
        .iter()
        .map(|o| o.parent_atom_id)
        .collect();
    let suffix: AtomSet = group
        .locants
        .iter()
        .filter_map(|l| mapping.get(l))
        .filter(|a| oxo.contains(a))
        .copied()
        .collect();
    let assigned: BTreeMap<_, _> = plan
        .derivative_state
        .bond_delta
        .assignment
        .orders
        .iter()
        .copied()
        .collect();
    let mut eligible = AtomSet::new();
    for o in operations {
        for (locant, &atom) in o.locants.iter().zip(&o.atom_ids) {
            if junctions.contains(locant)
                || parts
                    .parent_atom_symbols_by_locant
                    .get(locant)
                    .map(String::as_str)
                    != Some("C")
                || parts
                    .parent_atom_charges_by_locant
                    .get(locant)
                    .copied()
                    .unwrap_or(0)
                    != 0
            {
                continue;
            }
            let incident: Vec<_> = parts
                .parent_bond_orders_by_locants
                .iter()
                .filter(|((a, b), _)| a == locant || b == locant)
                .collect();
            if incident.len() != 2 || incident.iter().any(|(_, o)| **o != 1) {
                continue;
            }
            for ((a, b), _) in incident {
                let (Some(&a), Some(&b)) = (mapping.get(a), mapping.get(b)) else {
                    return;
                };
                if (suffix.contains(&a) || suffix.contains(&b))
                    && assigned.get(&(a.min(b), a.max(b))) == Some(&2)
                {
                    eligible.insert(atom);
                }
            }
        }
    }
    if eligible.len().is_multiple_of(2) {
        return;
    }
    let mut projected = vec![];
    for operation in &parts.hydro_operations {
        if operation.operation_kind != "additive_hydrogen" {
            projected.push(operation.clone());
            continue;
        }
        for added in [false, true] {
            let sites: Vec<_> = operation
                .locants
                .iter()
                .zip(&operation.atom_ids)
                .filter(|(_, a)| eligible.contains(a) == added)
                .collect();
            if sites.is_empty() {
                continue;
            }
            let mut o = operation.clone();
            o.locants = sites.iter().map(|(l, _)| (*l).clone()).collect();
            o.atom_ids = sites.iter().map(|(_, a)| **a).collect();
            if added {
                o.key = "added_hydrogen".into();
                o.operation_kind = "indicated_hydrogen".into();
            }
            projected.push(o);
        }
    }
    parts.hydro_operations = projected;
    for operation in &parts.hydro_operations {
        if operation.key == "added_hydrogen" {
            for l in &operation.locants {
                if !parts.indicated_hydrogens.contains(l) {
                    parts.indicated_hydrogens.push(l.clone());
                }
            }
        }
    }
}

fn project_locanted_hw_component(parts: &mut AssemblyParts) {
    if parts.retained_name.is_none()
        || !parts.is_ring
        || parts.is_bicycle
        || parts.is_polycycle
        || parts.is_spiro
        || !parts.a_prefixes.is_empty()
        || !parts.parent_charges.is_empty()
        || parts
            .parent_atom_charges_by_locant
            .values()
            .any(|q| *q != 0)
    {
        return;
    }
    let size = parts.parent_length;
    let locants: Vec<_> = (1..=size).map(|i| i.to_string()).collect();
    let expected: BTreeSet<_> = locants.iter().cloned().collect();
    let mapping = &parts.parent_atom_ids_by_locant;
    let symbols = &parts.parent_atom_symbols_by_locant;
    if mapping.keys().cloned().collect::<BTreeSet<_>>() != expected
        || symbols.keys().cloned().collect::<BTreeSet<_>>() != expected
        || mapping.values().copied().collect::<AtomSet>() != parts.parent_atom_ids
        || mapping.len() != size
    {
        return;
    }
    let hetero: Vec<_> = locants
        .iter()
        .enumerate()
        .filter(|(_, l)| symbols[*l] != "C")
        .map(|(i, l)| (i + 1, symbols[l].clone()))
        .collect();
    if hetero.len() < 2
        || ![hw_name(size, false, &hetero), hw_name(size, true, &hetero)]
            .contains(&parts.retained_name)
    {
        return;
    }
    let edges: BTreeSet<_> = (0..size)
        .map(|i| {
            let a = locants[i].clone();
            let b = locants[(i + 1) % size].clone();
            if a < b { (a, b) } else { (b, a) }
        })
        .collect();
    let actual: BTreeSet<_> = parts
        .parent_bond_orders_by_locants
        .keys()
        .map(|(a, b)| {
            if a < b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            }
        })
        .collect();
    if edges != actual
        || parts
            .parent_bond_orders_by_locants
            .values()
            .any(|o| ![1, 2].contains(o))
        || !parts
            .parent_bond_orders_by_locants
            .values()
            .any(|o| *o == 2)
    {
        return;
    }
    parts.retained_name = None;
    parts.retained_parent_metadata = None;
    parts.parent_hydride = None;
    parts.parent_bond_delta = None;
    parts.hydro_operations.clear();
    parts.indicated_hydrogens.clear();
    parts.unsaturations.clear();
    for (edge, &order) in &parts.parent_bond_orders_by_locants {
        if order != 2 {
            continue;
        }
        let mut numbers: Vec<_> = [edge.0.as_str(), edge.1.as_str()]
            .iter()
            .filter_map(|s| s.parse::<usize>().ok())
            .collect();
        numbers.sort();
        if numbers.len() != 2 {
            return;
        }
        let locant = if numbers == [1, size] {
            size
        } else {
            numbers[0]
        };
        parts.unsaturations.push(UnsaturationItem {
            bond_key: "double".into(),
            locants: vec![locant.to_string()],
            atom_ids: AtomSet::from([mapping[&edge.0], mapping[&edge.1]]),
            bond_ids: parts
                .parent_bond_ids_by_locants
                .get(edge)
                .copied()
                .into_iter()
                .collect(),
        });
    }
    parts.a_prefixes = hetero
        .iter()
        .filter_map(|(l, s)| {
            elements::get(s)?.hw_stem.as_ref().map(|stem| {
                let mut item = SubstituentItem::new(stem, vec![l.to_string()]);
                item.atom_ids.insert(mapping[&l.to_string()]);
                item
            })
        })
        .collect();
    parts.elided_unsaturation_locants.clear();
}

type ParentRenderer<'a> = &'a mut dyn FnMut(&mut AssemblyParts) -> Result<String, String>;

/// Preserve the component-local proof before independently rendering its parent.
pub fn spiro_assembly_from_parts(
    parts: &AssemblyParts,
    junction_locant: &str,
    mut render_parent: Option<ParentRenderer<'_>>,
) -> Result<Option<SpiroAssembly>, String> {
    let mut parts = parts.clone();
    scope_spiro_oxo_hydrogens(&mut parts, &BTreeSet::from([junction_locant.into()]));
    project_locanted_hw_component(&mut parts);
    if render_parent.is_none()
        && (!parts.parent_charges.is_empty()
            || parts
                .parent_atom_charges_by_locant
                .values()
                .any(|q| *q != 0))
    {
        return Ok(None);
    }
    let nested: Vec<_> = parts
        .substituents
        .iter()
        .filter_map(|s| s.spiro.as_deref())
        .collect();
    let continuation = if nested.is_empty() {
        None
    } else {
        if nested.len() != 1 || nested[0].side_parts.is_none() || nested[0].continuation.is_some() {
            return Ok(None);
        }
        let side = nested[0];
        let Some(other) = side.side_parts.as_ref() else {
            return Ok(None);
        };
        let Some(&shared) = parts.parent_atom_ids_by_locant.get(&side.parent_locant) else {
            return Ok(None);
        };
        if side.parent_locant == junction_locant
            || other.parent_atom_ids_by_locant.get(&side.side_locant) != Some(&shared)
            || parts
                .parent_atom_ids
                .intersection(&other.parent_atom_ids)
                .copied()
                .collect::<AtomSet>()
                != AtomSet::from([shared])
            || other
                .parent_atom_ids_by_locant
                .values()
                .copied()
                .collect::<AtomSet>()
                != other.parent_atom_ids
            || other.parent_atom_ids_by_locant.len() != other.parent_atom_ids.len()
        {
            return Ok(None);
        }
        Some(Box::new(side.clone()))
    };
    if let Some(group) = &parts.principal_group
        && (!["ketone", "alcohol"].contains(&group.key.as_str())
            || !parts.principal_suffix_modifiers.is_empty()
            || parts
                .hydro_operations
                .iter()
                .any(|o| o.key == "added_hydrogen"))
    {
        return Ok(None);
    }
    let mut local = parts.clone();
    if is_systematic_fusion(&local)
        && let Some(plan) = &mut local.fusion_plan
    {
        let omit_h = local.principal_group.is_none()
            && !plan.indicated_hydrogens.is_empty()
            && local
                .hydro_operations
                .iter()
                .any(|o| o.operation_kind == "indicated_hydrogen");
        if omit_h {
            local.hydro_operations.push(HydroOperation {
                key: "indicated_hydrogen".into(),
                locants: plan.indicated_hydrogens.clone(),
                atom_ids: plan
                    .indicated_hydrogens
                    .iter()
                    .filter_map(|l| local.parent_atom_ids_by_locant.get(l).copied())
                    .collect(),
                operation_kind: "indicated_hydrogen".into(),
                ..Default::default()
            });
        }
        if !plan.lambda_descriptors.is_empty() || omit_h {
            let registry = super::fusion::registry::fusion_component_registry();
            let core = super::fusion::descriptor::render_fusion_name(&plan.ast, registry)?;
            // Plain-name plans keep the audited AST rather than trace token roles.
            // Its descriptor renderer is the same unmodified component citation.
            plan.rendered_base_name = if omit_h || plan.indicated_hydrogens.is_empty() {
                core
            } else {
                format!(
                    "{}-{core}",
                    plan.indicated_hydrogens
                        .iter()
                        .map(|l| format!("{l}H"))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            if let Some(parent) = &mut local.parent_hydride {
                parent.parent_name = Some(plan.rendered_base_name.clone());
            }
        }
    }
    local.substituents.clear();
    local.a_prefixes.clear();
    local.stereo_features.clear();
    local.is_spiro_component = true;
    local.omit_redundant_locants = false;
    local.elided_unsaturation_locants.clear();
    let (_, parent_terminal) = parent_stem_and_terminal(&local)?;
    let mut name = match render_parent.as_mut() {
        Some(render) => render(&mut local)?,
        None => super::assembler::assemble_name_raw(&mut local)?,
    };
    let mut suffixes = vec![];
    if let Some(group) = &local.principal_group
        && !local.retained_absorbs_principal_group
    {
        let (terminal, suffix) = format_principal_suffix(&local, &parent_terminal)?;
        let Some(base) = name.strip_suffix(&suffix).filter(|_| !suffix.is_empty()) else {
            return Err("openclatura: spiro side suffix must match shared parent assembly".into());
        };
        name = format!(
            "{base}{}",
            if terminal.is_empty() {
                parent_terminal.as_str()
            } else {
                ""
            }
        );
        let word = if group.key == "ketone" { "one" } else { "ol" };
        suffixes = group
            .locants
            .iter()
            .map(|l| (l.clone(), word.into()))
            .collect();
    }
    let substituents: Vec<_> = parts
        .substituents
        .iter()
        .filter(|s| s.spiro.is_none())
        .cloned()
        .map(|mut s| {
            s.locants = s.locants.iter().map(|l| prime_side_locant(l)).collect();
            s
        })
        .collect();
    let mut prefixes: Vec<_> = substituents
        .iter()
        .map(|s| format!("{}-{}", s.locants.join(","), s.name))
        .collect();
    prefixes.extend(side_replacement_prefixes(&parts));
    let mut result = SpiroAssembly::new(junction_locant, name);
    result.side_prefixes = prefixes;
    result.side_suffixes = suffixes;
    result.side_stereo = parts
        .stereo_features
        .iter()
        .filter(|(l, _)| l != junction_locant)
        .map(|(l, d)| (prime_side_locant(l), d.clone()))
        .collect();
    result.side_substituents = substituents;
    result.side_parts = Some(Box::new(parts));
    result.continuation = continuation;
    Ok(Some(result))
}

fn component_locant(text: &str, allow_prime: bool) -> bool {
    let digits = text.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0
        && text[digits..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || (allow_prime && c == '\''))
}
pub fn drop_junction_indicated_hydrogen(name: &str, locant: &str) -> String {
    let Some((head, tail)) = name.split_once('-') else {
        return name.into();
    };
    let Some(cited) = head
        .split(',')
        .map(|s| s.strip_suffix('H').filter(|s| component_locant(s, false)))
        .collect::<Option<Vec<_>>>()
    else {
        return name.into();
    };
    let remaining: Vec<_> = cited
        .iter()
        .filter(|l| **l != locant)
        .map(|l| format!("{l}H"))
        .collect();
    if remaining.len() == cited.len() {
        name.into()
    } else if remaining.is_empty() {
        tail.into()
    } else {
        format!("{}-{tail}", remaining.join(","))
    }
}
pub fn spiro_component_sort_key(name: &str) -> (String, Vec<usize>) {
    let mut stripped = name.trim();
    while (stripped.starts_with('(') || stripped.starts_with('['))
        && (stripped.ends_with(')') || stripped.ends_with(']'))
    {
        stripped = stripped[1..stripped.len() - 1].trim();
    }
    let mut locants = vec![];
    loop {
        if let Some((run, tail)) = stripped.split_once('-') {
            if run.split(',').all(|l| component_locant(l, true)) {
                locants.extend(run.split(',').filter_map(|l| {
                    l.chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<usize>()
                        .ok()
                }));
                stripped = tail;
                continue;
            }
            if run.split(',').all(|l| {
                l.strip_suffix('H')
                    .is_some_and(|l| component_locant(l, true))
            }) {
                stripped = tail;
                continue;
            }
        }
        let mut hydro = None;
        for prefix in [
            "", "di", "tri", "tetra", "penta", "hexa", "hepta", "octa", "nona", "deca",
        ] {
            if let Some(tail) = stripped.strip_prefix(&format!("{prefix}hydro")) {
                hydro = Some(tail.trim_start_matches('-'));
                break;
            }
        }
        if let Some(tail) = hydro {
            stripped = tail;
            continue;
        }
        break;
    }
    (stripped.to_lowercase(), locants)
}
fn spiro_side_is_cited_first(parts: &AssemblyParts, side: &SpiroAssembly) -> bool {
    if side.continuation.is_some()
        || parts.is_spiro_component
        || parts.is_spiro_side_projection
        || !parts.a_prefixes.is_empty()
        || side.side_prefixes.iter().any(|p| p.contains("lambda^"))
    {
        return false;
    }
    let Ok((stem, terminal)) = parent_stem_and_terminal(parts) else {
        return false;
    };
    let parent_name = format!(
        "{stem}{}",
        if !terminal.is_empty() && terminal != "e" {
            terminal.as_str()
        } else {
            "e"
        }
    );
    if parent_name.starts_with("spiro[") || parent_name.starts_with("dispiro[") {
        return false;
    }
    spiro_component_sort_key(&side.side_parent_name) < spiro_component_sort_key(&parent_name)
}
fn reprime_side(side: &SpiroAssembly, prime: &str) -> SpiroAssembly {
    let mut side = side.clone();
    side.side_prime = prime.into();
    side.side_prefixes = side
        .side_prefixes
        .iter()
        .map(|p| p.replace('\'', prime))
        .collect();
    side.side_stereo = side
        .side_stereo
        .iter()
        .map(|(l, d)| (format!("{}{prime}", l.trim_end_matches('\'')), d.clone()))
        .collect();
    for s in &mut side.side_substituents {
        s.locants = s
            .locants
            .iter()
            .map(|l| format!("{}{prime}", l.trim_end_matches('\'')))
            .collect();
    }
    side
}
fn prime_central_parts(parts: &mut AssemblyParts, keep_name_internals: bool) {
    let prime = |l: &str| {
        if !keep_name_internals || l.starts_with(|c: char| c.is_ascii_digit()) {
            prime_side_locant(l)
        } else {
            l.into()
        }
    };
    for s in &mut parts.substituents {
        s.locants = s.locants.iter().map(|l| prime(l)).collect();
    }
    for s in &mut parts.principal_suffix_modifiers {
        s.locants = s.locants.iter().map(|l| prime(l)).collect();
    }
    if !keep_name_internals {
        for s in &mut parts.a_prefixes {
            s.locants = s.locants.iter().map(|l| prime(l)).collect();
        }
    }
    if let Some(g) = &mut parts.principal_group {
        g.locants = g.locants.iter().map(|l| prime(l)).collect();
    }
    for o in &mut parts.hydro_operations {
        o.locants = o.locants.iter().map(|l| prime(l)).collect();
    }
    if !keep_name_internals {
        for o in &mut parts.unsaturations {
            o.locants = o.locants.iter().map(|l| prime(l)).collect();
        }
    }
    parts.indicated_hydrogens = parts.indicated_hydrogens.iter().map(|l| prime(l)).collect();
    parts.stereo_features = parts
        .stereo_features
        .iter()
        .map(|(l, d)| (prime(l), d.clone()))
        .collect();
    parts.attachment_locant = prime(&parts.attachment_locant);
    for q in &mut parts.parent_charges {
        q.locant = prime(&q.locant);
    }
    parts.front_modifier_locants = parts
        .front_modifier_locants
        .iter()
        .map(|l| l.as_ref().map(|l| prime(l)))
        .collect();
    parts.parent_atom_ids_by_locant = std::mem::take(&mut parts.parent_atom_ids_by_locant)
        .into_iter()
        .map(|(l, v)| (prime(&l), v))
        .collect();
    parts.parent_atom_symbols_by_locant = std::mem::take(&mut parts.parent_atom_symbols_by_locant)
        .into_iter()
        .map(|(l, v)| (prime(&l), v))
        .collect();
    parts.parent_atom_charges_by_locant = std::mem::take(&mut parts.parent_atom_charges_by_locant)
        .into_iter()
        .map(|(l, v)| (prime(&l), v))
        .collect();
    parts.parent_atom_isotopes_by_locant =
        std::mem::take(&mut parts.parent_atom_isotopes_by_locant)
            .into_iter()
            .map(|(l, v)| (prime(&l), v))
            .collect();
    parts.parent_bond_orders_by_locants = std::mem::take(&mut parts.parent_bond_orders_by_locants)
        .into_iter()
        .map(|((a, b), v)| ((prime(&a), prime(&b)), v))
        .collect();
    parts.parent_bond_ids_by_locants = std::mem::take(&mut parts.parent_bond_ids_by_locants)
        .into_iter()
        .map(|((a, b), v)| ((prime(&a), prime(&b)), v))
        .collect();
    parts.elided_substituent_locants = parts
        .elided_substituent_locants
        .iter()
        .map(|l| prime(l))
        .collect();
}
fn retire_parent_junction_hydrogen(parts: &mut AssemblyParts, junctions: &BTreeSet<String>) {
    parts.hydro_operations.retain(|o| {
        o.operation_kind != "indicated_hydrogen"
            || o.key == "added_hydrogen"
            || !o.locants.iter().all(|l| junctions.contains(l))
    });
    parts.indicated_hydrogens.retain(|l| !junctions.contains(l));
    let respell = |name: &str| {
        junctions.iter().fold(name.to_owned(), |name, l| {
            drop_junction_indicated_hydrogen(&name, l)
        })
    };
    parts.retained_name = parts.retained_name.as_deref().map(respell);
    if let Some(metadata) = &mut parts.retained_parent_metadata {
        metadata
            .default_indicated_h
            .retain(|l| !junctions.contains(l));
        metadata.indicated_hydrogen_count = metadata
            .indicated_hydrogen_count
            .min(metadata.default_indicated_h.len());
    }
    if let Some(plan) = &mut parts.fusion_plan {
        plan.rendered_base_name = respell(&plan.rendered_base_name);
    }
    if let Some(parent) = &mut parts.parent_hydride {
        parent.parent_name = parent.base_name().map(respell);
    }
}
fn deduplicate_shared_replacements(parts: &mut AssemblyParts, side: &mut SpiroAssembly) {
    let Some(other) = &mut side.side_parts else {
        return;
    };
    let Some(&atom) = parts.parent_atom_ids_by_locant.get(&side.parent_locant) else {
        return;
    };
    if other.parent_atom_ids_by_locant.get(&side.side_locant) != Some(&atom)
        || parts
            .parent_atom_ids
            .intersection(&other.parent_atom_ids)
            .copied()
            .collect::<AtomSet>()
            != AtomSet::from([atom])
    {
        return;
    }
    for component in [&*parts, &**other] {
        let mapping = &component.parent_atom_ids_by_locant;
        if mapping.values().copied().collect::<AtomSet>() != component.parent_atom_ids
            || mapping.len() != component.parent_atom_ids.len()
            || component
                .parent_bond_orders_by_locants
                .keys()
                .any(|(a, b)| !mapping.contains_key(a) || !mapping.contains_key(b))
        {
            return;
        }
    }
    if !parts.a_prefixes.iter().any(|s| s.atom_ids.contains(&atom))
        || !other.a_prefixes.iter().any(|s| s.atom_ids.contains(&atom))
    {
        return;
    }
    let Some(symbol) = parts.parent_atom_symbols_by_locant.get(&side.parent_locant) else {
        return;
    };
    if other.parent_atom_symbols_by_locant.get(&side.side_locant) != Some(symbol) {
        return;
    }
    let charge = parts
        .parent_atom_charges_by_locant
        .get(&side.parent_locant)
        .copied()
        .unwrap_or(0);
    let bonding: usize = [&*parts, &**other]
        .iter()
        .flat_map(|c| {
            c.parent_bond_orders_by_locants
                .iter()
                .map(move |((a, b), o)| (c, a, b, o))
        })
        .filter(|(c, a, b, _)| {
            c.parent_atom_ids_by_locant.get(*a) == Some(&atom)
                || c.parent_atom_ids_by_locant.get(*b) == Some(&atom)
        })
        .map(|(_, _, _, o)| *o)
        .sum();
    let hydrides = super::nomenclature::section("mononuclear_parent_hydrides");
    if charge == -1
        && other.parent_atom_charges_by_locant.get(&side.side_locant) == Some(&charge)
        && let Some(element) = elements::get(symbol)
        && let Some(hydride) = hydrides.get(symbol).and_then(|v| v.as_str())
        && bonding == element.standard_valence + 1
        && element
            .mancude_charged_bonding_limits
            .contains(&(charge, bonding))
    {
        let mut replacements = vec![];
        for item in &other.a_prefixes {
            let (shared, remaining): (Vec<_>, Vec<_>) = item
                .locants
                .iter()
                .cloned()
                .partition(|l| other.parent_atom_ids_by_locant.get(l) == Some(&atom));
            if !remaining.is_empty() {
                let mut kept = item.clone();
                kept.locants = remaining;
                kept.atom_ids.remove(&atom);
                kept.charge_atom_ids.remove(&atom);
                replacements.push(kept);
            }
            if !shared.is_empty() {
                let mut kept = item.clone();
                kept.name = format!("{}uida", hydride.strip_suffix('e').unwrap_or(hydride));
                kept.locants = shared;
                kept.atom_ids = AtomSet::from([atom]);
                kept.charge_atom_ids = AtomSet::from([atom]);
                replacements.push(kept);
            }
        }
        other.a_prefixes = replacements;
    }
    parts.a_prefixes = parts
        .a_prefixes
        .iter()
        .cloned()
        .filter_map(|mut s| {
            s.locants.retain(|l| {
                parts
                    .parent_atom_ids_by_locant
                    .get(l.split("lambda^").next().unwrap_or(l))
                    != Some(&atom)
            });
            s.atom_ids.remove(&atom);
            s.charge_atom_ids.remove(&atom);
            (!s.locants.is_empty()).then_some(s)
        })
        .collect();
}
const MULTIPLIER_PREFIXES: [&str; 5] = ["di", "tri", "tetra", "penta", "hexa"];
const WITHIN_NAME: [&str; 11] = [
    "en", "yn", "yl", "ylidene", "ylidyne", "ol", "one", "al", "amine", "oic", "carbo",
];
fn is_replacement_prefix(name: &str) -> bool {
    let stem = MULTIPLIER_PREFIXES
        .iter()
        .find_map(|m| name.strip_prefix(m))
        .unwrap_or(name);
    RULES.assembly.replacement_prefix_order.contains_key(stem)
}
fn split_side_prefix_run(text: &str) -> Vec<(String, String)> {
    let segments: Vec<_> = text.split('-').collect();
    let mut starts = vec![];
    let mut depth = 0_i32;
    for (i, segment) in segments.iter().enumerate() {
        let nested = depth > 0;
        depth += segment.matches('(').count() as i32 - segment.matches(')').count() as i32;
        if nested
            || depth > 0
            || !segment
                .split(',')
                .all(|s| s.ends_with('\'') && component_locant(s, true))
        {
            continue;
        }
        let following = segments.get(i + 1).copied().unwrap_or("");
        if WITHIN_NAME.iter().any(|p| following.starts_with(p)) {
            continue;
        }
        starts.push(i);
    }
    if starts.first() != Some(&0) {
        return vec![(String::new(), text.into())];
    }
    starts.push(segments.len());
    starts
        .windows(2)
        .map(|w| (segments[w[0]].into(), segments[w[0] + 1..w[1]].join("-")))
        .collect()
}
fn hoist_side_substituent_prefixes(
    parts: &mut AssemblyParts,
    mut side: SpiroAssembly,
) -> SpiroAssembly {
    if let Some(other) = &side.side_parts {
        parts
            .substituents
            .extend(side.side_substituents.iter().cloned());
        side.side_prefixes = side_replacement_prefixes(other)
            .into_iter()
            .map(|s| s.replace('\'', &side.side_prime))
            .collect();
    } else {
        let mut kept = vec![];
        for prefix in &side.side_prefixes {
            for (locants, name) in split_side_prefix_run(prefix) {
                if locants.is_empty() || is_replacement_prefix(&name) {
                    kept.push(if locants.is_empty() {
                        name
                    } else {
                        format!("{locants}-{name}")
                    });
                    continue;
                }
                let locants: Vec<_> = locants.split(',').map(str::to_owned).collect();
                let mut item = side
                    .side_substituents
                    .iter()
                    .find(|s| s.locants == locants && s.name == name)
                    .cloned()
                    .unwrap_or_else(|| SubstituentItem::new(&name, locants.clone()));
                let mut base = name.as_str();
                if locants.len() > 1 {
                    let basic = multipliers::basic(locants.len());
                    let complex = multipliers::complex_(locants.len());
                    base = base
                        .strip_prefix(&basic)
                        .or_else(|| base.strip_prefix(&complex))
                        .unwrap_or(base);
                }
                item.name = super::formatting::strip_outer_parentheses(base);
                parts.substituents.push(item);
            }
        }
        side.side_prefixes = kept;
    }
    side.continuation = side
        .continuation
        .map(|s| Box::new(hoist_side_substituent_prefixes(parts, *s)));
    side
}
fn normalize_spiro(mut side: SpiroAssembly) -> SpiroAssembly {
    if side.side_parts.is_none() {
        let extracted = extract_spiro_side_prefixes(&side.side_parent_name);
        side.side_prefixes.extend(extracted.prefixes);
        side.side_parent_name = extracted.parent_name;
        side.side_suffixes.extend(extracted.suffixes);
        side.side_stereo.extend(extracted.stereo);
    }
    side
}
pub fn split_spiro_substituents(parts: &mut AssemblyParts) -> Result<Vec<SpiroAssembly>, String> {
    let mut sides = vec![];
    let mut normal = vec![];
    for sub in std::mem::take(&mut parts.substituents) {
        if let Some(side) = sub.spiro {
            sides.push(normalize_spiro(*side));
            continue;
        }
        if let Some(raw) = sub.name.strip_prefix("[SPIRO]-")
            && let Some((locant, name)) = raw.split_once('-')
            && locant.chars().all(|c| c.is_ascii_digit())
        {
            let mut side = SpiroAssembly::new(locant, name);
            side.parent_locant = sub.locants.first().cloned().unwrap_or_default();
            sides.push(normalize_spiro(side));
        } else {
            normal.push(sub);
        }
    }
    if sides.len() > 2 || (sides.len() > 1 && sides.iter().any(|s| s.continuation.is_some())) {
        return Err("openclatura: spiro assembly requires a graph-numbered path of at most three components".into());
    }
    parts.substituents = normal;
    if !sides.is_empty() {
        let junctions = sides.iter().map(|s| s.parent_locant.clone()).collect();
        retire_parent_junction_hydrogen(parts, &junctions);
        for side in &mut sides {
            side.side_parent_name =
                drop_junction_indicated_hydrogen(&side.side_parent_name, &side.side_locant);
        }
        scope_spiro_oxo_hydrogens(parts, &junctions);
        project_locanted_hw_component(parts);
        for side in &mut sides {
            deduplicate_shared_replacements(parts, side);
        }
    }
    if sides.len() == 2 {
        sides.sort_by_key(|s| (parse_locant(&s.parent_locant), s.side_parent_name.clone()));
        prime_central_parts(parts, false);
        sides = vec![reprime_side(&sides[0], ""), reprime_side(&sides[1], "''")];
    } else if sides.len() == 1 && sides[0].continuation.is_some() {
        let side = &mut sides[0];
        side.continuation = side
            .continuation
            .as_ref()
            .map(|s| Box::new(reprime_side(s, "''")));
    } else if sides.len() == 1 && spiro_side_is_cited_first(parts, &sides[0]) {
        prime_central_parts(parts, true);
        sides[0] = reprime_side(&sides[0], "");
        sides[0].cited_first = true;
    }
    sides = sides
        .into_iter()
        .map(|s| hoist_side_substituent_prefixes(parts, s))
        .collect();
    for side in &sides {
        for side in std::iter::once(side).chain(side.continuation.as_deref()) {
            let junction = format!("{}{}", side.side_locant, side.side_prime);
            for feature in &side.side_stereo {
                if feature.0 != junction && !parts.stereo_features.contains(feature) {
                    parts.stereo_features.push(feature.clone());
                }
            }
        }
    }
    parts.spiro_prefix_composition = !sides.is_empty();
    Ok(sides)
}

fn parent_needs_parentheses(name: &str) -> bool {
    let bare = name.trim_start_matches(|c: char| c.is_ascii_digit() || c == ',' || c == '-');
    if ["bicyclo[", "tricyclo[", "tetracyclo["]
        .iter()
        .any(|p| name.starts_with(p) || bare.starts_with(p))
    {
        return false;
    }
    // A locanted replacement prefix on a polycycle is part of the component.
    if ["bicyclo[", "tricyclo[", "tetracyclo["].iter().any(|p| {
        name.find(p).is_some_and(|i| {
            i > 0
                && name[..i].contains('-')
                && name[..i]
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == ',' || c == '-')
        })
    }) {
        return false;
    }
    name.contains('-') || name.chars().any(|c| c.is_ascii_digit())
}
fn side_name(name: &str) -> String {
    if parent_needs_parentheses(name) {
        format!("({name})")
    } else {
        name.into()
    }
}
fn retained_n_info(parent: &str) -> Option<(usize, String)> {
    let ionic_by_neutral = &RULES.charges.retained_ionic_n_parents;
    let neutral = ionic_by_neutral.iter().find_map(|(neutral, ionic)| {
        (parent == ionic
            || parent == neutral
            || parent == format!("{}-1-ium", neutral.strip_suffix('e').unwrap_or(neutral)))
        .then_some(neutral)
    })?;
    let ionic = &ionic_by_neutral[neutral];
    let size = RULES
        .charges
        .saturated_n_ring_ionic_parents
        .iter()
        .find_map(|(size, name)| (name == ionic).then_some(*size))
        .unwrap_or(0);
    let explicit = if parent == ionic {
        format!("{}-1-ium", neutral.strip_suffix('e').unwrap_or(neutral))
    } else {
        parent.into()
    };
    Some((size, explicit))
}
fn side_locant(side: &SpiroAssembly, parent_name: &str) -> String {
    if side.side_parts.is_none()
        && side.side_locant == "1"
        && let Some((size, _)) = retained_n_info(parent_name)
        && size > 0
    {
        return (size / 2 + 1).to_string();
    }
    side.side_locant.clone()
}
fn prime_side_suffixes(side: &SpiroAssembly) -> Vec<(String, String)> {
    side.side_suffixes
        .iter()
        .map(|(l, s)| {
            (
                format!("{}{}", l.trim_end_matches('\''), side.side_prime),
                s.clone(),
            )
        })
        .collect()
}
fn format_side_suffixes(suffixes: &[(String, String)]) -> String {
    let mut grouped: Vec<(&str, Vec<&str>)> = vec![];
    for (l, s) in suffixes {
        if let Some((_, locants)) = grouped.iter_mut().find(|(word, _)| *word == s.as_str()) {
            locants.push(l);
        } else {
            grouped.push((s, vec![l]));
        }
    }
    grouped
        .into_iter()
        .map(|(s, mut locants)| {
            locants.sort_by_key(|l| parse_locant(l));
            format!(
                "-{}-{}{s}",
                locants.join(","),
                if locants.len() > 1 {
                    multipliers::basic(locants.len())
                } else {
                    String::new()
                }
            )
        })
        .collect()
}
fn suffix_run(text: &str, allow_multiplier: bool) -> Option<(Vec<String>, &str)> {
    let (locants, word) = text.strip_prefix('-')?.split_once('-')?;
    if !locants
        .chars()
        .all(|c| c.is_ascii_digit() || c == ',' || c == '\'')
    {
        return None;
    }
    let word = if allow_multiplier {
        ["di", "tri", "tetra"]
            .iter()
            .find_map(|m| word.strip_prefix(m))
            .unwrap_or(word)
    } else {
        word
    };
    ["ol", "one"]
        .contains(&word)
        .then(|| (locants.split(',').map(str::to_owned).collect(), word))
}
fn merge_terminal_and_side_suffixes(terminal: &str, side: &[(String, String)]) -> String {
    if side.is_empty() {
        return terminal.into();
    }
    if terminal.ends_with("yl") {
        let suffix = format_side_suffixes(side);
        return format!("{}{terminal}", suffix.strip_suffix('e').unwrap_or(&suffix));
    }
    let Some((mut locants, word)) = suffix_run(terminal, false) else {
        return format!("{terminal}{}", format_side_suffixes(side));
    };
    let (same, other): (Vec<_>, Vec<_>) = side.iter().cloned().partition(|(_, s)| s == word);
    if same.is_empty() {
        return format!("{terminal}{}", format_side_suffixes(side));
    }
    locants.extend(same.into_iter().map(|(l, _)| l));
    format!(
        "-{}-{}{word}{}",
        locants.join(","),
        if locants.len() > 1 {
            multipliers::basic(locants.len())
        } else {
            String::new()
        },
        format_side_suffixes(&other)
    )
}
fn merge_principal_and_side_suffixes(
    suffix: &str,
    side: Vec<(String, String)>,
) -> (String, Vec<(String, String)>) {
    let Some((mut locants, word)) = suffix_run(suffix, true) else {
        return (suffix.into(), side);
    };
    let (same, other): (Vec<_>, Vec<_>) = side.into_iter().partition(|(_, s)| s == word);
    if same.is_empty() {
        return (suffix.into(), other);
    }
    locants.extend(same.into_iter().map(|(l, _)| l));
    (
        format!(
            "-{}-{}{word}",
            locants.join(","),
            if locants.len() > 1 {
                multipliers::basic(locants.len())
            } else {
                String::new()
            }
        ),
        other,
    )
}
fn replacement_head(name: &str) -> Option<(&str, &str, &str)> {
    let (locants, rest) = name.split_once('-')?;
    if !locants.split(',').all(|l| component_locant(l, true)) {
        return None;
    }
    let mut candidates: Vec<_> = RULES
        .assembly
        .replacement_prefix_order
        .keys()
        .flat_map(|stem| {
            std::iter::once(stem.clone()).chain(
                MULTIPLIER_PREFIXES
                    .iter()
                    .map(move |m| format!("{m}{stem}")),
            )
        })
        .collect();
    candidates.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let word = candidates
        .iter()
        .find(|word| rest.starts_with(word.as_str()))?;
    Some((locants, &rest[..word.len()], &rest[word.len()..]))
}
pub fn unprime_inline_replacement_prefix(name: &str) -> String {
    if let Some((locants, word, tail)) = replacement_head(name)
        && locants.contains('\'')
    {
        format!("{}-{word}{tail}", locants.replace('\'', ""))
    } else {
        name.into()
    }
}
fn spiro_names_components(core: &str) -> bool {
    core.find("spiro[")
        .is_some_and(|i| core[i + 6..].split(']').next().unwrap_or("").contains('\''))
}
fn prime_replacement_prefixes(core: &str, prefixes: Vec<String>) -> Vec<String> {
    if !spiro_names_components(core) {
        return prefixes;
    }
    prefixes
        .into_iter()
        .map(|prefix| {
            if let Some((locants, word, tail)) = replacement_head(&prefix)
                && tail.is_empty()
                && locants.chars().all(|c| c.is_ascii_digit() || c == ',')
            {
                format!(
                    "{}-{word}",
                    locants
                        .split(',')
                        .map(|l| format!("{l}'"))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            } else {
                prefix
            }
        })
        .collect()
}
fn prime_inline_replacement_prefixes(core: &str) -> String {
    if let Some((locants, word, tail)) = replacement_head(core)
        && tail.starts_with("spiro[")
        && spiro_names_components(tail)
        && !locants.contains('\'')
    {
        format!(
            "{}-{word}{tail}",
            locants
                .split(',')
                .map(|l| format!("{l}'"))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        core.into()
    }
}
fn format_dispiro_core(core: &str, terminal: &str, sides: &[SpiroAssembly]) -> String {
    let mut ordered: Vec<_> = sides.iter().collect();
    ordered.sort_by_key(|s| (parse_locant(&s.parent_locant), s.side_parent_name.clone()));
    let (first, second) = (ordered[0], ordered[1]);
    let mut core = format!(
        "dispiro[{}-{},{}'-{}-{}',{}''-{}]",
        side_name(&first.side_parent_name),
        side_locant(first, &first.side_parent_name),
        first.parent_locant,
        side_name(&unprime_inline_replacement_prefix(core)),
        second.parent_locant,
        side_locant(second, &second.side_parent_name),
        side_name(&second.side_parent_name)
    );
    let prefixes: Vec<_> = first
        .side_prefixes
        .iter()
        .chain(&second.side_prefixes)
        .cloned()
        .collect();
    let suffixes: Vec<_> = prime_side_suffixes(first)
        .into_iter()
        .chain(prime_side_suffixes(second))
        .collect();
    if !terminal.is_empty() && terminal != "e" {
        if (terminal.contains("yl")
            || elision::is_vowel_start(
                terminal.trim_start_matches(|c: char| c == '-' || c == ',' || c.is_ascii_digit()),
            ))
            && core.ends_with('e')
        {
            core.pop();
        }
        core.push_str(&merge_terminal_and_side_suffixes(terminal, &suffixes));
    } else if !suffixes.is_empty() {
        core.push_str(&format_side_suffixes(&suffixes));
    }
    if !prefixes.is_empty() {
        // Upstream divergence: sides keep their final dispiro scope; re-priming moves replacements to the core.
        core = format!("{}{core}", prefixes.join("-"));
    }
    core
}
pub(super) struct SpiroCore {
    pub(super) core: String,
    pub(super) suffix: String,
}

pub(super) fn format_spiro_core(
    stem: &str,
    unsat: &str,
    terminal: &str,
    sides: &[SpiroAssembly],
    suffix: &str,
    parent_terminal: &str,
) -> SpiroCore {
    if sides.is_empty() {
        return SpiroCore {
            core: format!("{stem}{unsat}{terminal}"),
            suffix: suffix.into(),
        };
    }
    let mut core = format!(
        "{stem}{unsat}{}",
        if stem.ends_with("ium") {
            ""
        } else {
            parent_terminal
        }
    );
    if sides.len() == 2 && !core.starts_with("spiro[") {
        return SpiroCore {
            core: format_dispiro_core(&core, terminal, sides),
            suffix: suffix.into(),
        };
    }
    let mut prefixes = vec![];
    let mut suffixes = vec![];
    for side in sides {
        prefixes.extend(side.side_prefixes.iter().cloned());
        suffixes.extend(prime_side_suffixes(side));
        let extracted_parent = if side.side_parts.is_none() {
            let extracted = extract_spiro_side_prefixes(&side.side_parent_name);
            prefixes.extend(extracted.prefixes);
            suffixes.extend(
                extracted
                    .suffixes
                    .into_iter()
                    .map(|(l, s)| (format!("{}'", l.trim_end_matches('\'')), s)),
            );
            Some(extracted.parent_name)
        } else {
            None
        };
        let parent_name = extracted_parent
            .as_deref()
            .unwrap_or(&side.side_parent_name);
        if core.starts_with("spiro[") {
            continue;
        }
        let name = side_name(parent_name);
        if let Some(tail) = &side.continuation {
            prefixes.extend(tail.side_prefixes.iter().cloned());
            suffixes.extend(prime_side_suffixes(tail));
            core = format!(
                "dispiro[{}-{},{}'-{name}-{}',{}''-{}]",
                side_name(&core),
                side.parent_locant,
                side.side_locant,
                tail.parent_locant,
                tail.side_locant,
                side_name(&tail.side_parent_name)
            );
        } else if side.cited_first {
            core = format!(
                "spiro[{parent_name}-{},{}'-{core}]",
                side_locant(side, parent_name),
                side.parent_locant
            );
        } else {
            core = format!(
                "spiro[{core}-{},{}'-{name}]",
                side.parent_locant,
                side_locant(side, parent_name)
            );
        }
    }
    let mut suffix = suffix.to_owned();
    if !terminal.is_empty() && terminal != "e" {
        if (terminal.contains("yl")
            || elision::is_vowel_start(
                terminal.trim_start_matches(|c: char| c == '-' || c == ',' || c.is_ascii_digit()),
            ))
            && core.ends_with('e')
        {
            core.pop();
        }
        core.push_str(&merge_terminal_and_side_suffixes(terminal, &suffixes));
    } else if !suffixes.is_empty() {
        let (merged, other) = merge_principal_and_side_suffixes(&suffix, suffixes);
        suffix = merged;
        core.push_str(&format_side_suffixes(&other));
    }
    let side_first = sides.iter().any(|s| s.cited_first);
    if !prefixes.is_empty() {
        if !side_first {
            prefixes = prime_replacement_prefixes(&core, prefixes);
        }
        core = format!("{}{core}", prefixes.join("-"));
    }
    if !side_first {
        core = prime_inline_replacement_prefixes(&core);
    }
    SpiroCore { core, suffix }
}

struct ExtractedSide {
    prefixes: Vec<String>,
    parent_name: String,
    suffixes: Vec<(String, String)>,
    stereo: Vec<(String, String)>,
}

fn extract_spiro_side_prefixes(side_name: &str) -> ExtractedSide {
    let mut normalized = side_name.trim().to_owned();
    if normalized.starts_with('(') && normalized.ends_with(')') {
        normalized = normalized[1..normalized.len() - 1].into();
    }
    normalized = normalized.replace("-aziridine", "aziridine");
    let (mut normalized, suffixes) = extract_side_suffixes(&normalized);
    let mut aliases = vec![];
    for (size, ionic) in &RULES.charges.saturated_n_ring_ionic_parents {
        if let Some((neutral, _)) = RULES
            .charges
            .retained_ionic_n_parents
            .iter()
            .find(|(_, i)| *i == ionic)
        {
            aliases.push((
                format!("1-azacyclo{}ane", stems::stem_for(*size)),
                neutral.clone(),
            ));
        }
    }
    aliases.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
    aliases.extend([
        ("1-azacyclopropane".into(), "aziridine".into()),
        ("1-oxacyclopropane".into(), "oxirane".into()),
        ("1-thiacyclopropane".into(), "thiirane".into()),
    ]);
    let mut found = aliases.iter().find_map(|(alias, parent)| {
        normalized
            .strip_suffix(alias)
            .map(|p| (parent.clone(), p.trim_end_matches('-').to_owned()))
    });
    if found.is_none() {
        for marker in ["tetracyclo[", "tricyclo[", "bicyclo["] {
            if let Some(i) = normalized.rfind(marker)
                && i > 0
                && normalized.as_bytes()[i - 1] == b'-'
            {
                found = Some((
                    normalized[i..].into(),
                    normalized[..i].trim_end_matches('-').into(),
                ));
                break;
            }
        }
    }
    if found.is_none() {
        let mut parents: Vec<String> = RULES
            .charges
            .retained_ionic_n_parents
            .keys()
            .chain(RULES.charges.retained_ionic_n_parents.values())
            .cloned()
            .collect();
        parents.sort_by_key(|p| std::cmp::Reverse(p.len()));
        parents.extend(
            [
                "indoline",
                "indane",
                "1,3-benzoxazole",
                "1,3-benzothiazole",
                "benzimidazole",
                "benzene",
                "tetracyclo",
                "tricyclo",
                "bicyclo",
            ]
            .map(str::to_owned),
        );
        for size in RULES.charges.saturated_n_ring_ionic_parents.keys() {
            for stem in ["aza", "oxa", "thia"] {
                parents.push(format!("1-{stem}cyclo{}ane", stems::stem_for(*size)));
            }
        }
        parents.extend(["aziridine", "oxirane", "thiirane"].map(str::to_owned));
        parents.extend((3..9).map(|n| format!("cyclo{}ane", stems::stem_for(n))));
        found = parents.iter().find_map(|p| {
            normalized
                .strip_suffix(p)
                .map(|s| (p.clone(), s.trim_end_matches('-').into()))
        });
    }
    let Some((parent, mut prefix)) = found else {
        return ExtractedSide {
            prefixes: vec![],
            parent_name: normalized,
            suffixes,
            stereo: vec![],
        };
    };
    let mut stereo = vec![];
    if prefix.starts_with('(')
        && let Some(i) = prefix.find(')')
    {
        let body = &prefix[1..i];
        let parsed = body
            .split(',')
            .map(|term| {
                let descriptor = term.chars().last()?;
                let locant = &term[..term.len() - descriptor.len_utf8()];
                (component_locant(locant, true) && "RSEZ".contains(descriptor))
                    .then(|| (prime_side_locant(locant), descriptor.to_string()))
            })
            .collect::<Option<Vec<_>>>();
        if let Some(features) = parsed {
            stereo = features;
            prefix = prefix[i + 1..].trim_start_matches('-').into();
        }
    }
    normalized = retained_n_info(&parent).map_or(parent, |(_, name)| name);
    if prefix.is_empty() {
        return ExtractedSide {
            prefixes: vec![],
            parent_name: normalized,
            suffixes,
            stereo,
        };
    }
    let mut segments: Vec<String> = prefix.split('-').map(str::to_owned).collect();
    let mut depth = 0_i32;
    let mut remaining = segments.iter_mut().peekable();
    while let Some(segment) = remaining.next() {
        let nested = depth > 0;
        depth += segment.matches('(').count() as i32 - segment.matches(')').count() as i32;
        if nested || depth > 0 || !segment.chars().all(|c| c.is_ascii_digit() || c == ',') {
            continue;
        }
        let next = remaining.peek().map(|s| s.as_str()).unwrap_or("");
        if WITHIN_NAME.iter().any(|p| next.starts_with(p)) {
            continue;
        }
        *segment = segment
            .split(',')
            .map(|l| format!("{l}'"))
            .collect::<Vec<_>>()
            .join(",");
    }
    ExtractedSide {
        prefixes: vec![segments.join("-")],
        parent_name: normalized,
        suffixes,
        stereo,
    }
}
fn extract_side_suffixes(name: &str) -> (String, Vec<(String, String)>) {
    let Some((left, word)) = name.rsplit_once('-') else {
        return (name.into(), vec![]);
    };
    if !["ol", "one"].contains(&word) {
        return (name.into(), vec![]);
    }
    let Some((stem, locants)) = left.rsplit_once('-') else {
        return (name.into(), vec![]);
    };
    if !locants.chars().all(|c| c.is_ascii_digit() || c == ',')
        || !["an", "en", "yn"].iter().any(|s| stem.ends_with(s))
    {
        return (name.into(), vec![]);
    }
    (
        format!("{stem}e"),
        locants
            .split(',')
            .map(|l| (l.into(), word.into()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_order_ignores_hydrogenation_and_hydrogen_citations() {
        assert_eq!(
            spiro_component_sort_key("4a,5,6,7,8,8a-hexahydro-1H-quinoxaline"),
            spiro_component_sort_key("4a,5,6,7,8,8a-quinoxaline")
        );
        assert!(
            spiro_component_sort_key("1H-pyrrolo[3,2-b]pyridine")
                > spiro_component_sort_key("imidazolidine")
        );
    }
    #[test]
    fn component_junction_retires_only_its_own_hydrogen() {
        assert_eq!(
            drop_junction_indicated_hydrogen("9H-fluorene", "9"),
            "fluorene"
        );
        assert_eq!(
            drop_junction_indicated_hydrogen("1H,9H-fluorene", "9"),
            "1H-fluorene"
        );
        assert_eq!(prime_side_locant("2lambda^5"), "2'lambda^5");
    }
    #[test]
    fn suffixes_and_replacement_locants_share_completed_spiro_scope() {
        let mut side = SpiroAssembly::new("4", "cyclopentane");
        side.parent_locant = "3".into();
        side.side_suffixes = vec![("2".into(), "one".into())];
        assert_eq!(
            format_spiro_core("indolin", "", "e", &[side], "-2-one", "e").core,
            "spiro[indoline-3,4'-cyclopentane]"
        );
        let formatted = format_spiro_core(
            "indolin",
            "",
            "e",
            &[SpiroAssembly {
                side_suffixes: vec![("2".into(), "one".into())],
                parent_locant: "3".into(),
                ..SpiroAssembly::new("4", "cyclopentane")
            }],
            "-2-one",
            "e",
        );
        assert_eq!(formatted.suffix, "-2,2'-dione");
        assert!(formatted.core.ends_with(']'));
    }
    #[test]
    fn dispiro_replacements_keep_each_components_final_scope() {
        let first = SpiroAssembly {
            parent_locant: "1".into(),
            side_prime: String::new(),
            side_prefixes: vec!["1-sila".into()],
            ..SpiroAssembly::new("2", "cyclopropane")
        };
        let second = SpiroAssembly {
            parent_locant: "2".into(),
            side_prime: "''".into(),
            side_prefixes: vec!["1''-germa".into()],
            ..SpiroAssembly::new("2", "cyclobutane")
        };
        assert_eq!(
            format_dispiro_core("2,3-dihydro-1H-indene", "e", &[first, second]),
            "1-sila-1''-germadispiro[cyclopropane-2,1'-(2,3-dihydro-1H-indene)-2',2''-cyclobutane]"
        );
    }
    #[test]
    fn side_prefix_projection_respects_nested_locants() {
        let extracted =
            extract_spiro_side_prefixes("(2R)-2-(2,6-difluorophenyl)-3-methylcyclopentane");
        assert_eq!(extracted.parent_name, "cyclopentane");
        assert_eq!(extracted.prefixes, ["2'-(2,6-difluorophenyl)-3'-methyl"]);
        assert_eq!(extracted.stereo, [("2'".into(), "R".into())]);
    }
}
