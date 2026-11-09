// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming/openclatura/spirobi.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! P-24.3: compose identical polycyclic hydrides before generic spiro assembly.
use super::{
    assembly_parts::{AssemblyParts, SubstituentItem, UnsaturationItem},
    assembly_prefixes::{format_replacement_prefixes, format_substituent_prefixes},
    assembly_utils::parse_locant,
    fusion::{
        mancude::{CompareOptions, compare_actual_parent_to_implied_parent},
        model::{FusionGraph, FusionGraphAtom, FusionGraphBond},
        numbering::{parent_bond_model, parent_pi_capable_atom_ids},
    },
    molecule::{AtomSet, Molecule},
    nomenclature::RULES,
    rules::{bonds, elements, elision, multipliers, stems},
};
use std::collections::{BTreeMap, BTreeSet};

type Locants = BTreeMap<usize, String>;
type LocantKey = (u8, usize, String);

pub(super) fn prime_locant(locant: &str) -> String {
    // P-14.3.1: primes precede a fusion letter, e.g. 3'a.
    let digits = locant.bytes().take_while(u8::is_ascii_digit).count();
    format!("{}'{}", &locant[..digits], &locant[digits..])
}

#[derive(PartialEq, Eq)]
pub(super) enum Component {
    VonBaeyer(String),
    Fused(String),
}
impl Component {
    fn name(&self) -> &str {
        match self {
            Self::VonBaeyer(name) | Self::Fused(name) => name,
        }
    }
}

pub(super) fn component(parts: &AssemblyParts) -> Result<Option<Component>, String> {
    if parts.is_spiro || parts.is_ring_assembly() {
        return Ok(None);
    }
    if parts
        .fusion_plan
        .as_ref()
        .is_some_and(|plan| !plan.audit.confirmed() || !plan.lambda_descriptors.is_empty())
    {
        return Ok(None);
    }
    if parts.fusion_plan.is_some() || parts.retained_name.is_some() {
        let edges: BTreeSet<_> = parts
            .parent_bond_orders_by_locants
            .keys()
            .map(|(a, b)| super::locants::canonical_locant_pair(a, b))
            .collect();
        if let Some(template) = super::retained_fused_templates::retained_graph_templates(false)
            .iter()
            .filter(|t| {
                !t.fusion_atoms.is_empty()
                    && t.atoms.len() == parts.parent_atom_ids.len()
                    && t.atoms.iter().all(|a| {
                        parts.parent_atom_symbols_by_locant.get(&a.locant) == Some(&a.symbol)
                            && parts.parent_atom_charges_by_locant.get(&a.locant) == Some(&a.charge)
                    })
                    && t.bonds
                        .iter()
                        .map(|b| super::locants::canonical_locant_pair(&b.locants.0, &b.locants.1))
                        .collect::<BTreeSet<_>>()
                        == edges
            })
            .max_by_key(|t| (t.mancude_double_bonds, t.priority))
        {
            return Ok(Some(Component::Fused(
                super::retained_name_policy::render_retained_hydrogen_state(
                    &template.output_name(),
                    &template.default_indicated_h,
                    Some(&[]),
                ),
            )));
        }
    }
    if let Some(plan) = &parts.fusion_plan {
        return super::fusion::descriptor::render_fusion_name(
            &plan.ast,
            super::fusion::registry::fusion_component_registry(),
        )
        .map(|name| Some(Component::Fused(name)));
    }
    if parts.retained_name.is_some() {
        return Ok(None);
    }
    let descriptor = if parts.is_bicycle {
        let (a, b, c) = parts.bicycle_xyz;
        super::ring_renderer::render_ring_descriptor("bicyclo", &[a, b, c])
    } else if super::ring_renderer::is_von_baeyer_descriptor(parts.polycycle_descriptor.as_deref())
    {
        parts.polycycle_descriptor.clone().unwrap_or_default()
    } else {
        return Ok(None);
    };
    Ok(Some(Component::VonBaeyer(format!(
        "{descriptor}{}ane",
        stems::stem_for(parts.parent_length)
    ))))
}

#[derive(Default)]
struct Hydrogen {
    indicated: Vec<String>,
    hydro: Vec<String>,
}

pub(super) fn completed_graph(
    mol: &Molecule,
    atoms: &AtomSet,
    map: &Locants,
    junction: usize,
) -> FusionGraph {
    // P-24.3.2: discard component indicated H and make the completed system mancude.
    FusionGraph {
        atoms: atoms
            .iter()
            .map(|&id| {
                let atom = &mol.atoms[&id];
                FusionGraphAtom {
                    id,
                    symbol: atom.symbol.clone(),
                    formal_charge: atom.charge,
                    pi_capacity: 1,
                    forced_single: id == junction
                        || elements::get(&atom.symbol)
                            .is_some_and(|element| element.mancude_forced_single),
                    indicated_h_site: false,
                    saturated: false,
                }
            })
            .collect(),
        bonds: mol
            .bonds
            .values()
            .filter(|b| map.contains_key(&b.u) && map.contains_key(&b.v))
            .map(|b| FusionGraphBond {
                atoms: (b.u.min(b.v), b.u.max(b.v)),
                bond_class: "mancude".into(),
            })
            .collect(),
    }
}

fn fused_hydrogen(
    mol: &Molecule,
    parts: &AssemblyParts,
    map: &Locants,
    junction: usize,
) -> Result<Option<Hydrogen>, String> {
    let graph = completed_graph(mol, &parts.parent_atom_ids, map, junction);
    let model = parent_bond_model(&graph)?;
    let Some(delta) = compare_actual_parent_to_implied_parent(
        mol,
        &parts.parent_atom_ids,
        &model,
        &CompareOptions {
            atom_to_locant: Some(map),
            preserve_retained_parent_state: true,
            ..Default::default()
        },
    )?
    else {
        return Ok(None);
    };
    if !delta.compatible || !delta.additional_multiple_bond_ids.is_empty() {
        return Ok(None);
    }
    let occupied: AtomSet = delta
        .assignment
        .orders
        .iter()
        .filter(|(_, order)| *order == 2)
        .flat_map(|(edge, _)| [edge.0, edge.1])
        .collect();
    let capable = parent_pi_capable_atom_ids(&graph)?;
    let indicated: Vec<_> = capable
        .difference(&occupied)
        .filter(|&&id| id != junction)
        .copied()
        .collect();
    if indicated.iter().any(|id| {
        mol.atoms[id].symbol != "C" || mol.atoms[id].is_aromatic || mol.atoms[id].total_h_count == 0
    }) {
        return Ok(None);
    }
    Ok(Some(Hydrogen {
        indicated: indicated.iter().map(|id| map[id].clone()).collect(),
        hydro: delta
            .hydrogenated_edges
            .iter()
            .flat_map(|&(a, b)| [map[&a].clone(), map[&b].clone()])
            .collect(),
    }))
}

pub(super) fn project(
    mol: &Molecule,
    parts: &AssemblyParts,
    map: &Locants,
    von_baeyer: bool,
) -> Option<AssemblyParts> {
    let remap = |locant: &str| {
        parts
            .parent_atom_ids_by_locant
            .get(locant)
            .and_then(|id| map.get(id))
            .cloned()
    };
    let mut projected = AssemblyParts::new(parts.parent_length);
    projected.is_ring = true;
    projected.omit_redundant_locants = false;
    for item in parts.substituents.iter().filter(|s| s.spiro.is_none()) {
        let mut item = item.clone();
        item.locants = item
            .locants
            .iter()
            .map(|l| remap(l))
            .collect::<Option<_>>()?;
        projected.substituents.push(item);
    }
    if von_baeyer {
        for (&id, locant) in map {
            let atom = &mol.atoms[&id];
            if atom.symbol != "C" {
                let mut item = SubstituentItem::new(
                    elements::get(&atom.symbol)?.hw_stem.as_ref()?,
                    vec![locant.clone()],
                );
                item.atom_ids.insert(id);
                projected.a_prefixes.push(item);
            }
        }
        for bond in mol
            .bonds
            .values()
            .filter(|b| b.order > 1 && map.contains_key(&b.u) && map.contains_key(&b.v))
        {
            let (mut a, mut b) = (&map[&bond.u], &map[&bond.v]);
            if parse_locant(a) > parse_locant(b) {
                std::mem::swap(&mut a, &mut b);
            }
            let locant = if parse_locant(b).1 == parse_locant(a).1 + 1 {
                a.clone()
            } else {
                format!("{a}({b})")
            };
            let key = if bond.order == 2 { "double" } else { "triple" };
            if let Some(item) = projected
                .unsaturations
                .iter_mut()
                .find(|u| u.bond_key == key)
            {
                item.locants.push(locant);
            } else {
                projected.unsaturations.push(UnsaturationItem {
                    bond_key: key.into(),
                    locants: vec![locant],
                    ..Default::default()
                });
            }
        }
    }
    if let Some(group) = &parts.principal_group {
        let mut group = group.clone();
        group.locants = group
            .locants
            .iter()
            .map(|l| remap(l))
            .collect::<Option<_>>()?;
        projected.principal_group = Some(group);
    }
    Some(projected)
}

/// A side projected as a prefix still contributes to the system's principal suffix.
pub(super) fn promote_suffixes(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    groups: &[super::perception::PerceivedGroup],
    maps: &[&Locants],
) {
    for group in groups {
        let Some(locant) = maps
            .iter()
            .find_map(|map| map.get(&group.attachment_carbon))
        else {
            continue;
        };
        if !parts
            .substituents
            .iter()
            .any(|item| item.atom_ids == group.atoms_involved)
        {
            continue;
        }
        parts
            .substituents
            .retain(|item| item.atom_ids != group.atoms_involved);
        let principal = parts.principal_group.get_or_insert_with(|| {
            super::assembly_parts::PrincipalGroupItem {
                key: group.key.clone(),
                resolved_rule: group.resolved_rule.clone(),
                ..Default::default()
            }
        });
        principal.locants.push(locant.clone());
        principal.atom_ids.extend(group.atom_ids());
    }
    if let Some(principal) = &mut parts.principal_group {
        principal.bond_ids = super::molecule::bond_ids_within(mol, &principal.atom_ids);
    }
}

#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
struct UnsaturationRank {
    compound_count: usize,
    primary: Vec<LocantKey>,
    all: Vec<LocantKey>,
}

fn unsaturation_ranks(mol: &Molecule, maps: [&Locants; 2]) -> [UnsaturationRank; 2] {
    let mut ranks: [UnsaturationRank; 2] = Default::default();
    for map in maps {
        for bond in mol
            .bonds
            .values()
            .filter(|b| b.order > 1 && map.contains_key(&b.u) && map.contains_key(&b.v))
        {
            let (mut a, mut b) = (parse_locant(&map[&bond.u]), parse_locant(&map[&bond.v]));
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            // P-31.1.4.2: count compound locants, compare primary locants,
            // then all cited locants, including the parenthesized endpoints.
            let compound = b.1 != a.1 + 1;
            // P-31.1.4.3: multiple bonds together, then double bonds alone.
            for rank in ranks.iter_mut().take(if bond.order == 2 { 2 } else { 1 }) {
                rank.compound_count += usize::from(compound);
                rank.primary.push(a.clone());
                rank.all.push(a.clone());
                if compound {
                    rank.all.push(b.clone());
                }
            }
        }
    }
    for rank in &mut ranks {
        rank.primary.sort();
        rank.all.sort();
    }
    ranks
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    junctions: Vec<LocantKey>,
    hetero: Vec<LocantKey>,
    hetero_priority: Vec<Vec<LocantKey>>,
    hydrogen: Vec<LocantKey>,
    suffix: Vec<LocantKey>,
    hydro: Vec<LocantKey>,
    unsaturation: UnsaturationRank,
    double: UnsaturationRank,
    substituents: Vec<LocantKey>,
    citation: Vec<Vec<LocantKey>>,
}
fn keys(locants: impl IntoIterator<Item = String>) -> Vec<LocantKey> {
    let mut keys: Vec<_> = locants.into_iter().map(|l| parse_locant(&l)).collect();
    keys.sort();
    keys
}
fn rank(
    parts: &AssemblyParts,
    junctions: &[String],
    hydrogen: &Hydrogen,
    [unsaturation, double]: [UnsaturationRank; 2],
) -> Rank {
    let mut replacements: BTreeMap<_, Vec<String>> = BTreeMap::new();
    for item in &parts.a_prefixes {
        replacements
            .entry(
                RULES
                    .assembly
                    .replacement_prefix_order
                    .get(&item.name)
                    .copied()
                    .unwrap_or(99),
            )
            .or_default()
            .extend(item.locants.clone());
    }
    let mut substituents: BTreeMap<_, Vec<String>> = BTreeMap::new();
    for item in &parts.substituents {
        substituents
            .entry(super::assembly_prefixes::substituent_sort_key(&item.name))
            .or_default()
            .extend(item.locants.clone());
    }
    Rank {
        junctions: junctions.iter().map(|l| parse_locant(l)).collect(),
        hetero: keys(parts.a_prefixes.iter().flat_map(|s| s.locants.clone())),
        hetero_priority: replacements.into_values().map(keys).collect(),
        hydrogen: keys(hydrogen.indicated.clone()),
        suffix: keys(parts.principal_group.iter().flat_map(|s| s.locants.clone())),
        hydro: keys(hydrogen.hydro.clone()),
        unsaturation,
        double,
        substituents: keys(parts.substituents.iter().flat_map(|s| s.locants.clone())),
        citation: substituents.into_values().map(keys).collect(),
    }
}

pub(super) struct SpirobiName {
    pub name: String,
    pub parent: String,
    pub audited: bool,
}

pub(super) fn compose(
    mol: &Molecule,
    parts: &AssemblyParts,
) -> Result<Option<SpirobiName>, String> {
    let sides: Vec<_> = parts
        .substituents
        .iter()
        .filter_map(|s| s.spiro.as_deref())
        .collect();
    let [side] = sides.as_slice() else {
        return Ok(None);
    };
    let Some(other) = side.side_parts.as_deref() else {
        return Ok(None);
    };
    if side.continuation.is_some()
        || other.substituents.iter().any(|s| s.spiro.is_some())
        || parts.is_substituent
        || parts.isotope_explicit_locants
    {
        return Ok(None);
    }
    let (Some(identity), Some(other_identity)) = (component(parts)?, component(other)?) else {
        return Ok(None);
    };
    if identity != other_identity {
        return Ok(None);
    }
    let shared: Vec<_> = parts
        .parent_atom_ids
        .intersection(&other.parent_atom_ids)
        .copied()
        .collect();
    let [junction] = shared.as_slice() else {
        return Ok(None);
    };
    let junction = *junction;
    let von_baeyer = matches!(identity, Component::VonBaeyer(_));
    for c in [parts, other] {
        if c.parent_atom_ids_by_locant
            .values()
            .copied()
            .collect::<AtomSet>()
            != c.parent_atom_ids
            || c.parent_atom_ids_by_locant.len() != c.parent_atom_ids.len()
            || c.parent_numbering_maps.is_empty()
            || c.parent_numbering_maps
                .iter()
                .any(|map| map.keys().copied().collect::<AtomSet>() != c.parent_atom_ids)
            || !c.parent_charges.is_empty()
            || c.principal_group.as_ref().is_some_and(|group| {
                !von_baeyer || !["alcohol", "ketone"].contains(&group.key.as_str())
            })
            || !c.principal_suffix_modifiers.is_empty()
            || !c.stereo_features.is_empty()
            || !c.endo_exo_descriptors.is_empty()
            || !c.front_modifiers.is_empty()
            || !c.relative_stereo_prefixes.is_empty()
            || c.parent_atom_ids.iter().any(|id| {
                let atom = &mol.atoms[id];
                let bonding = atom.total_h_count
                    + mol
                        .get_neighbors(*id)
                        .iter()
                        .map(|&other| mol.bond_order(*id, other))
                        .sum::<usize>();
                atom.charge != 0
                    || elements::get(&atom.symbol).is_some_and(|e| bonding > e.standard_valence)
                    || (!von_baeyer && !["C", "O", "S"].contains(&mol.atoms[id].symbol.as_str()))
            })
        {
            return Ok(None);
        }
    }
    if mol.atoms[&junction].symbol != "C" || mol.get_neighbors(junction).len() != 4 {
        return Ok(None);
    }
    let principal_key = parts
        .principal_group
        .as_ref()
        .or(other.principal_group.as_ref())
        .map(|group| group.key.as_str());
    let groups: Vec<_> = if let Some(key) = principal_key {
        super::perception::perceive_groups(mol)
            .into_iter()
            .filter(|group| group.key == key)
            .collect()
    } else {
        vec![]
    };
    let mut best = None;
    // P-24.3.3–4 and P-31.1.5.2.1: compare both prime assignments jointly.
    for (first, second) in [(parts, other), (other, parts)] {
        for first_map in &first.parent_numbering_maps {
            for second_map in &second.parent_numbering_maps {
                let primed: Locants = second_map
                    .iter()
                    .map(|(&id, l)| (id, prime_locant(l)))
                    .collect();
                let junctions = [first_map[&junction].clone(), primed[&junction].clone()];
                let (Some(mut projected), Some(mut second_parts)) = (
                    project(mol, first, first_map, von_baeyer),
                    project(mol, second, &primed, von_baeyer),
                ) else {
                    continue;
                };
                let mut hydrogen = Hydrogen::default();
                if !von_baeyer {
                    let (Some(first_h), Some(second_h)) = (
                        fused_hydrogen(mol, first, first_map, junction)?,
                        fused_hydrogen(mol, second, &primed, junction)?,
                    ) else {
                        continue;
                    };
                    hydrogen.indicated.extend(first_h.indicated);
                    hydrogen.indicated.extend(second_h.indicated);
                    hydrogen.hydro.extend(first_h.hydro);
                    hydrogen.hydro.extend(second_h.hydro);
                }
                projected
                    .substituents
                    .append(&mut second_parts.substituents);
                projected.a_prefixes.append(&mut second_parts.a_prefixes);
                for unsaturation in second_parts.unsaturations {
                    if let Some(item) = projected
                        .unsaturations
                        .iter_mut()
                        .find(|u| u.bond_key == unsaturation.bond_key)
                    {
                        item.locants.extend(unsaturation.locants);
                    } else {
                        projected.unsaturations.push(unsaturation);
                    }
                }
                if let Some(group) = second_parts.principal_group {
                    if let Some(first_group) = &mut projected.principal_group {
                        if first_group.key != group.key {
                            continue;
                        }
                        first_group.locants.extend(group.locants);
                        first_group.atom_ids.extend(group.atom_ids);
                    } else {
                        projected.principal_group = Some(group);
                    }
                }
                promote_suffixes(mol, &mut projected, &groups, &[first_map, &primed]);
                let unsaturation = if von_baeyer {
                    unsaturation_ranks(mol, [first_map, &primed])
                } else {
                    Default::default()
                };
                let score = rank(&projected, &junctions, &hydrogen, unsaturation);
                if best.as_ref().is_none_or(|(old, _, _, _)| score < *old) {
                    best = Some((score, projected, junctions, hydrogen));
                }
            }
        }
    }
    let Some((_, projected, junctions, mut hydrogen)) = best else {
        return Ok(None);
    };
    let mut unit = super::assembly_spiro::spiro_component(identity.name());
    let mut ending = String::new();
    if !projected.unsaturations.is_empty() {
        let (_, unsaturation, _) =
            super::assembly_parent::format_unsaturations(&projected, "", false);
        // P-31.1.5.2.1: elision follows citation order, not bond insertion order.
        let elide = projected
            .unsaturations
            .iter()
            .min_by_key(|u| {
                RULES
                    .assembly
                    .unsaturation_order
                    .get(&u.bond_key)
                    .copied()
                    .unwrap_or(99)
            })
            .is_some_and(|u| {
                let infix = bonds::unsaturation_infix(&u.bond_key, u.locants.len().max(1));
                elision::is_vowel_start(infix.strip_prefix('a').unwrap_or(&infix))
            });
        if elide && unit.ends_with('e') {
            unit.pop();
        }
        ending = format!("{unsaturation}e");
    }
    if projected.principal_group.is_some() {
        let (terminal, suffix, _) =
            super::assembly_parent::format_principal_suffix(&projected, "e")?;
        if terminal.is_empty() {
            if ending.is_empty() && unit.ends_with('e') {
                unit.pop();
            }
            if ending.ends_with('e') {
                ending.pop();
            }
        }
        ending.push_str(&suffix);
    }
    let mut core = format!("{},{}-spirobi[{unit}]{ending}", junctions[0], junctions[1]);
    hydrogen.indicated.sort_by_key(|l| parse_locant(l));
    if !hydrogen.indicated.is_empty() {
        core = format!(
            "{}-{core}",
            hydrogen
                .indicated
                .iter()
                .map(|l| format!("{l}H"))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    let replacement = format_replacement_prefixes(&projected).0;
    if !replacement.is_empty() {
        core = format!("{replacement}-{core}");
    }
    hydrogen.hydro.sort_by_key(|l| parse_locant(l));
    if !hydrogen.hydro.is_empty() {
        core = format!(
            "{}-{}hydro-{core}",
            hydrogen.hydro.join(","),
            multipliers::basic(hydrogen.hydro.len())
        );
    }
    let prefix = format_substituent_prefixes(&projected).0;
    let word = if prefix.is_empty() {
        core.clone()
    } else {
        format!("{prefix}-{core}")
    };
    Ok(Some(SpirobiName {
        name: word,
        parent: core,
        audited: hydrogen.hydro.is_empty() && projected.principal_group.is_none(),
    }))
}
