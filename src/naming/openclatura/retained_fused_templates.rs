// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/retained_fused_templates.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: openclatura | src/openclatura/retained_graph_model.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_retained_macrocycles.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_retained_fused_production.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_fusion_component_h_citation.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/roundtrip/test_fused_parents.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Openclatura retained graph registry/matcher and generated series adapted to typed Rust, with original topology-status and localized-pi safeguards.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Shared graph registry and exact, topology-indexed retained-parent matching.
use super::{
    assembly_parts::RetainedParentMetadata,
    locants::retained_locant_sort_key as parse_locant,
    molecule::{AtomSet, Molecule},
    retained_graph_model::{
        RetainedGraphAtomTemplate, RetainedGraphBondTemplate, RetainedGraphTemplate,
        RetainedGraphTemplateMatch,
    },
    rules::{elements, multipliers},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchOptions<'a> {
    pub include_disabled: bool,
    pub allow_nonaromatic: bool,
    pub allow_relocated_indicated_h: bool,
    pub pre_descriptor_only: bool,
    pub family: Option<&'a str>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TopologyKey {
    pub size: usize,
    pub symbols: BTreeMap<String, usize>,
    pub degrees: BTreeMap<usize, usize>,
    pub edge_degrees: BTreeMap<(usize, usize), usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedGraphData {
    #[serde(rename = "bluebook_rule")]
    _bluebook_rule: String,
    pub parents: Vec<RetainedGraphRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HydrocarbonGraphData {
    #[serde(rename = "bluebook_rule")]
    _bluebook_rule: String,
    #[serde(rename = "source")]
    _source: String,
    parents: Vec<RetainedGraphRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MacrocycleGraphData {
    #[serde(rename = "source")]
    _source: String,
    parents: Vec<RetainedGraphRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedGraphRow {
    pub name: String,
    pub pin: bool,
    pub priority: i64,
    #[serde(default)]
    pub attached_prefix: Option<String>,
    #[serde(default)]
    pub indicated_hydrogen_name: Option<String>,
    #[serde(default)]
    default_indicated_h: Option<Vec<String>>,
    pub template: RetainedGraphTemplateData,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedGraphTemplateData {
    pub atoms: Vec<RetainedGraphAtomTemplate>,
    pub bonds: Vec<RetainedGraphBondTemplate>,
    locants: Vec<String>,
    pub rings: Vec<Vec<String>>,
    fusion_atoms: Vec<String>,
    pub peripheral_atoms: Vec<String>,
    pub interior_atoms: Vec<String>,
    default_indicated_h: Vec<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub mancude_double_bonds: Option<usize>,
    pub enabled: bool,
    derivative_production_enabled: bool,
    #[serde(default)]
    family: Option<String>,
    #[serde(default = "neutral_kekule_equivalent")]
    aromatic_equivalence_policy: String,
    #[serde(default)]
    charge_policy: Option<String>,
    #[serde(default)]
    enforce_mancude_double_bonds: Option<bool>,
    #[serde(default)]
    implied_stereo: bool,
    #[serde(default)]
    indicated_hydrogen_count: Option<usize>,
    #[serde(default)]
    pre_descriptor_selection: bool,
}
// Provenance: fn neutral_kekule_equivalent | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::retained_graph_template_from_data
// Source: openclatura | src/openclatura/retained_graph_model.py::RetainedGraphTemplate.aromatic_equivalence_policy
fn neutral_kekule_equivalent() -> String {
    "neutral_kekule_equivalent".into()
}
impl RetainedGraphTemplateData {
    // Provenance: fn RetainedGraphTemplateData::set_family_defaults | adapted
    // Source: openclatura | src/openclatura/retained_fused_templates.py::_with_template_family
    fn set_family_defaults(&mut self, family: &str) {
        self.family.get_or_insert_with(|| family.into());
        self.charge_policy.get_or_insert_with(|| "exact".into());
        if family == "macrocycle" {
            self.enforce_mancude_double_bonds.get_or_insert(true);
        }
    }
}
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained graph registry"
)]
pub(super) static GRAPH_DATA: LazyLock<RetainedGraphData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("data/retained_fused_graph_templates.json"))
        .expect("checked-in retained graph table")
});
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained hydrocarbon graph registry"
)]
static HYDROCARBON_DATA: LazyLock<HydrocarbonGraphData> = LazyLock::new(|| {
    let mut data: HydrocarbonGraphData = serde_json::from_str(include_str!(
        "data/retained_fused_hydrocarbon_templates.json"
    ))
    .expect("checked-in retained hydrocarbon graph table");
    for row in &mut data.parents {
        row.template.set_family_defaults("fused");
    }
    data
});
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained macrocycle graph registry"
)]
static MACROCYCLE_DATA: LazyLock<MacrocycleGraphData> = LazyLock::new(|| {
    let mut data: MacrocycleGraphData =
        serde_json::from_str(include_str!("data/retained_macrocycle_templates.json"))
            .expect("checked-in retained macrocycle graph table");
    for row in &mut data.parents {
        row.template.set_family_defaults("macrocycle");
    }
    data
});
// Provenance: fn retained_graph_template_rows | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::inline in _declared_retained_graph_templates
pub(super) fn retained_graph_template_rows() -> impl Iterator<Item = &'static RetainedGraphRow> {
    GRAPH_DATA
        .parents
        .iter()
        .chain(&HYDROCARBON_DATA.parents)
        .chain(&MACROCYCLE_DATA.parents)
}
static ALL_TEMPLATES: LazyLock<Vec<RetainedGraphTemplate>> = LazyLock::new(|| {
    let mut templates: Vec<_> = retained_graph_template_rows()
        .map(retained_graph_template_from_data)
        .collect();
    for (family, minimum, maximum) in [("acenes", 4, 9), ("polyaphenes", 5, 10)] {
        for count in minimum..=maximum {
            let generated = generated_series_template(family, count);
            if !templates.iter().any(|t| t.name == generated.name) {
                templates.push(generated);
            }
        }
    }
    templates
});
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalizedPiData {
    #[serde(rename = "source")]
    _source: LocalizedPiSource,
    parents: BTreeMap<String, Vec<LocalizedPiBond>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalizedPiSource {
    #[serde(rename = "basis")]
    _basis: String,
    #[serde(rename = "note")]
    _note: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalizedPiBond {
    locants: (String, String),
    order: usize,
}
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained localized pi states"
)]
static LOCALIZED_PI: LazyLock<LocalizedPiData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("data/retained_fused_localized_pi.json"))
        .expect("checked-in retained localized pi states")
});
static ENABLED_TEMPLATES: LazyLock<Vec<RetainedGraphTemplate>> = LazyLock::new(|| {
    ALL_TEMPLATES
        .iter()
        .filter(|t| t.enabled)
        .cloned()
        .collect()
});
static TOPOLOGY_INDEX: LazyLock<BTreeMap<TopologyKey, Vec<usize>>> = LazyLock::new(|| {
    let mut index = BTreeMap::<_, Vec<_>>::new();
    for (i, t) in ALL_TEMPLATES.iter().enumerate() {
        index
            .entry(retained_graph_template_topology_key(t))
            .or_default()
            .push(i);
    }
    index
});
// Provenance: fn retained_graph_templates | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::retained_graph_templates
// Source: openclatura | src/openclatura/retained_fused_templates.py::_retained_graph_templates
pub fn retained_graph_templates(include_disabled: bool) -> &'static [RetainedGraphTemplate] {
    if include_disabled {
        &ALL_TEMPLATES
    } else {
        &ENABLED_TEMPLATES
    }
}
// Provenance: fn retained_parent_metadata | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::retained_parent_metadata
pub fn retained_parent_metadata(name: &str) -> Option<RetainedParentMetadata> {
    ALL_TEMPLATES
        .iter()
        .find(|t| t.name == name)
        .or_else(|| ALL_TEMPLATES.iter().find(|t| t.output_name() == name))
        .map(RetainedGraphTemplate::metadata)
}
// Provenance: fn retained_graph_template_from_data | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::retained_graph_template_from_data
fn retained_graph_template_from_data(row: &RetainedGraphRow) -> RetainedGraphTemplate {
    let data = &row.template;
    RetainedGraphTemplate {
        name: row.name.clone(),
        priority: row.priority,
        default_indicated_h: row
            .default_indicated_h
            .as_ref()
            .unwrap_or(&data.default_indicated_h)
            .clone(),
        locants: data.locants.clone(),
        atoms: data.atoms.clone(),
        bonds: data.bonds.clone(),
        fusion_atoms: data.fusion_atoms.clone(),
        family: data.family.as_deref().unwrap_or("fused").into(),
        aromatic_equivalence_policy: data.aromatic_equivalence_policy.clone(),
        charge_policy: data
            .charge_policy
            .as_deref()
            .unwrap_or_else(|| {
                if data.atoms.iter().all(|a| a.symbol == "C") {
                    "exact"
                } else {
                    "charge_layer"
                }
            })
            .into(),
        enforce_mancude_double_bonds: data.enforce_mancude_double_bonds.unwrap_or(false),
        enabled: data.enabled,
        derivative_production_enabled: data.derivative_production_enabled,
        implied_stereo: data.implied_stereo,
        mancude_double_bonds: data.mancude_double_bonds,
        indicated_hydrogen_count_override: data.indicated_hydrogen_count,
        pre_descriptor_selection: data.pre_descriptor_selection,
    }
}
// Provenance: fn molecule_graph_topology_key | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::molecule_graph_topology_key
pub(super) fn molecule_graph_topology_key(mol: &Molecule, atoms: &AtomSet) -> TopologyKey {
    let degrees: BTreeMap<_, _> = atoms
        .iter()
        .map(|&a| {
            (
                a,
                mol.get_neighbors(a)
                    .iter()
                    .filter(|n| atoms.contains(n))
                    .count(),
            )
        })
        .collect();
    let mut key = TopologyKey {
        size: atoms.len(),
        symbols: BTreeMap::new(),
        degrees: BTreeMap::new(),
        edge_degrees: BTreeMap::new(),
    };
    for a in atoms {
        *key.symbols.entry(mol.atoms[a].symbol.clone()).or_default() += 1;
        *key.degrees.entry(degrees[a]).or_default() += 1;
    }
    for b in mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
    {
        let (a, b) = (degrees[&b.u], degrees[&b.v]);
        *key.edge_degrees.entry((a.min(b), a.max(b))).or_default() += 1;
    }
    key
}
// Provenance: fn retained_graph_template_topology_key | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::retained_graph_template_topology_key
pub(super) fn retained_graph_template_topology_key(t: &RetainedGraphTemplate) -> TopologyKey {
    let mut degrees: BTreeMap<_, usize> = t.locants.iter().map(|l| (l, 0)).collect();
    for b in &t.bonds {
        *degrees.entry(&b.locants.0).or_default() += 1;
        *degrees.entry(&b.locants.1).or_default() += 1;
    }
    let mut key = TopologyKey {
        size: t.atoms.len(),
        symbols: BTreeMap::new(),
        degrees: BTreeMap::new(),
        edge_degrees: BTreeMap::new(),
    };
    for a in &t.atoms {
        *key.symbols.entry(a.symbol.clone()).or_default() += 1;
        *key.degrees.entry(degrees[&a.locant]).or_default() += 1;
    }
    for b in &t.bonds {
        let (a, b) = (degrees[&b.locants.0], degrees[&b.locants.1]);
        *key.edge_degrees.entry((a.min(b), a.max(b))).or_default() += 1;
    }
    key
}
// Provenance: fn match_retained_graph_templates | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::match_retained_graph_templates
pub fn match_retained_graph_templates(
    mol: &Molecule,
    atoms: &AtomSet,
    allow_nonaromatic: bool,
) -> Vec<RetainedGraphTemplateMatch> {
    match_retained_graph_templates_with_options(
        mol,
        atoms,
        MatchOptions {
            allow_nonaromatic,
            ..Default::default()
        },
    )
}
// Provenance: fn match_retained_graph_templates_with_options | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::match_retained_graph_templates
pub fn match_retained_graph_templates_with_options(
    mol: &Molecule,
    atoms: &AtomSet,
    options: MatchOptions<'_>,
) -> Vec<RetainedGraphTemplateMatch> {
    // Upstream divergence: retained parent names and hydro prefixes only
    // describe single/double ring bonds. Neutral Kekule topology matching
    // otherwise accepts triples and drops their extra unsaturation, including
    // on the pre-descriptor route that precedes the production gate.
    if mol
        .bonds
        .values()
        .any(|b| atoms.contains(&b.u) && atoms.contains(&b.v) && !matches!(b.order, 1 | 2))
    {
        return Vec::new();
    }
    if options.family.is_none() {
        for family in ["fused", "macrocycle"] {
            let m = match_retained_graph_templates_with_options(
                mol,
                atoms,
                MatchOptions {
                    family: Some(family),
                    ..options
                },
            );
            if !m.is_empty() {
                return m;
            }
        }
        return Vec::new();
    }
    let key = molecule_graph_topology_key(mol, atoms);
    let mut candidates: Vec<_> = TOPOLOGY_INDEX
        .get(&key)
        .into_iter()
        .flatten()
        .map(|&i| ALL_TEMPLATES[i].clone())
        .collect();
    if options.family == Some("fused")
        && key.symbols.len() == 1
        && key.symbols.get("C") == Some(&key.size)
        && key.size >= 18
        && (key.size - 2).is_multiple_of(4)
    {
        let count = (key.size - 2) / 4;
        if count <= 1000
            && key.edge_degrees.values().sum::<usize>() + 1 == key.size + count
            && key.degrees.keys().all(|d| matches!(d, 2 | 3))
        {
            for (family, eager) in [("acenes", 9), ("polyaphenes", 10)] {
                if count > eager {
                    let t = generated_series_template(family, count);
                    if retained_graph_template_topology_key(&t) == key
                        && !candidates.iter().any(|c| c.name == t.name)
                    {
                        candidates.push(t);
                    }
                }
            }
        }
    }
    let mut matches = Vec::new();
    for t in candidates.iter().filter(|t| {
        (options.include_disabled || t.enabled)
            && (!options.pre_descriptor_only || t.pre_descriptor_selection)
            && Some(t.family.as_str()) == options.family
    }) {
        if t.enforce_mancude_double_bonds
            && t.mancude_double_bonds.is_some_and(|n| {
                mol.bonds
                    .values()
                    .filter(|b| b.order == 2 && atoms.contains(&b.u) && atoms.contains(&b.v))
                    .count()
                    != n
            })
        {
            continue;
        }
        matches.extend(match_retained_graph_template_maps_with_options(
            mol,
            atoms,
            t,
            options.allow_nonaromatic,
            options.allow_relocated_indicated_h,
        ));
    }
    matches.sort_by_cached_key(|m| {
        (
            m.template.priority,
            m.indicated_h
                .iter()
                .map(|l| parse_locant(l))
                .collect::<Vec<_>>(),
            m.template
                .atoms
                .iter()
                .filter(|a| a.symbol != "C")
                .map(|a| parse_locant(&a.locant))
                .collect::<Vec<_>>(),
            m.template
                .fusion_atoms
                .iter()
                .map(|l| parse_locant(l))
                .collect::<Vec<_>>(),
            m.template.name.clone(),
            m.template
                .locants
                .iter()
                .map(|l| m.locant_to_atom[l])
                .collect::<Vec<_>>(),
        )
    });
    matches
}

// Provenance: fn is_saturated_site | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_is_saturated_site
pub fn is_saturated_site(mol: &Molecule, atom: usize, ring: &AtomSet) -> bool {
    mol.get_neighbors(atom)
        .iter()
        .filter(|n| ring.contains(n))
        .all(|&n| mol.get_bond(atom, n).is_none_or(|b| b.order == 1))
}
// Provenance: fn is_retained_oxo_site | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_is_retained_oxo_site
pub fn is_retained_oxo_site(mol: &Molecule, atom: usize) -> bool {
    mol.atoms[&atom].symbol == "C"
        && mol.get_neighbors(atom).iter().any(|n| {
            mol.atoms[n].symbol == "O" && mol.get_bond(atom, *n).is_some_and(|b| b.order == 2)
        })
}
// Provenance: fn relocatable_atom_by_locant | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_relocatable_atom_by_locant
pub fn relocatable_atom_by_locant(
    t: &RetainedGraphTemplate,
) -> BTreeMap<String, RetainedGraphAtomTemplate> {
    let movable: BTreeSet<_> = t
        .atoms
        .iter()
        .filter(|a| {
            !a.aromatic
                && !a.fusion
                && a.symbol != "C"
                && !elements::get(&a.symbol).is_some_and(|e| e.mancude_forced_single)
        })
        .map(|a| a.symbol.clone())
        .collect();
    t.atoms
        .iter()
        .map(|a| {
            let mut a = a.clone();
            if t.default_indicated_h.contains(&a.locant) && a.symbol == "C" {
                a.saturated = false;
                a.default_h = false;
            } else if movable.contains(&a.symbol) && !a.fusion {
                a.aromatic = false;
                a.default_h = false;
            }
            (a.locant.clone(), a)
        })
        .collect()
}
// Provenance: fn atom_matches | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_atom_matches_template
fn atom_matches(
    mol: &Molecule,
    idx: usize,
    a: &RetainedGraphAtomTemplate,
    ring: &AtomSet,
    nonaromatic: bool,
    template: &RetainedGraphTemplate,
    fixed_hydrogenation: bool,
) -> bool {
    let atom = &mol.atoms[&idx];
    let charge = template.charge_policy.as_str();
    // P-72.2.2.1 removes a hydron from the indicated-H site of these
    // explicitly admitted retained hydrides. Keep the neutral graph and its
    // indicated-H locant; the anion operation supplies the charge suffix.
    let deprotonated_indicated_h = charge == "deprotonated_indicated_h"
        && template.default_indicated_h.contains(&a.locant)
        && a.symbol == "C"
        && a.charge == 0
        && a.saturated
        && !a.aromatic
        && atom.charge == -1
        && atom.radical_electrons == 0
        && mol
            .get_neighbors(idx)
            .iter()
            .map(|&neighbor| mol.bond_order(idx, neighbor))
            .sum::<usize>()
            + atom.total_h_count
            == 3;
    if atom.symbol != a.symbol
        || (matches!(charge, "exact" | "deprotonated_indicated_h")
            || (charge == "charge_layer" && a.charge != 0))
            && atom.charge != a.charge
            && !deprotonated_indicated_h
    {
        return false;
    }
    if a.aromatic
        && !atom.is_aromatic
        && is_saturated_site(mol, idx, ring)
        && !nonaromatic
        && !is_retained_oxo_site(mol, idx)
    {
        return false;
    }
    let doubles = mol
        .get_neighbors(idx)
        .iter()
        .filter(|n| ring.contains(n) && mol.get_bond(idx, **n).is_some_and(|b| b.order == 2))
        .count();
    if doubles > 1
        || a.default_h && atom.explicit_h_count + atom.total_h_count == 0
        || a.saturated && !is_saturated_site(mol, idx, ring)
    {
        return false;
    }
    !(fixed_hydrogenation && !a.aromatic && !a.saturated && a.symbol != "C" && doubles > 0)
}
/// Element-labelled graph maps for fusion status checks, independent of the
/// complete retained parent's bond orders, hydrogenation and charge state.
// Provenance: fn match_retained_graph_template_topology_maps | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_match_all_retained_fused_template
// Source: openclatura | src/openclatura/retained_fused_templates.py::_match_locants_backtracking
// Source: openclatura | src/openclatura/retained_fused_templates.py::_collect_locant_matches
pub(super) fn match_retained_graph_template_topology_maps(
    mol: &Molecule,
    atoms: &AtomSet,
    t: &RetainedGraphTemplate,
) -> Vec<BTreeMap<String, usize>> {
    if atoms.len() != t.atoms.len() {
        return Vec::new();
    }
    let index: BTreeMap<_, _> = t.locants.iter().enumerate().map(|(i, l)| (l, i)).collect();
    let mut neighbors = vec![BTreeSet::new(); t.locants.len()];
    for b in &t.bonds {
        let (i, j) = (index[&b.locants.0], index[&b.locants.1]);
        neighbors[i].insert(j);
        neighbors[j].insert(i);
    }
    let by_locant = t.atom_by_locant();
    let candidates: Vec<Vec<usize>> = t
        .locants
        .iter()
        .enumerate()
        .map(|(i, l)| {
            atoms
                .iter()
                .copied()
                .filter(|a| {
                    mol.atoms[a].symbol == by_locant[l].symbol
                        && mol
                            .get_neighbors(*a)
                            .iter()
                            .filter(|n| atoms.contains(n))
                            .count()
                            == neighbors[i].len()
                })
                .collect()
        })
        .collect();
    let mut topology = t.clone();
    topology.aromatic_equivalence_policy.clear();
    let mut assignments = Vec::new();
    collect_assignments(
        mol,
        &topology,
        &neighbors,
        &BTreeMap::new(),
        &candidates,
        &mut vec![None; t.locants.len()],
        &mut AtomSet::new(),
        &mut assignments,
    );
    assignments
        .into_iter()
        .map(|a| t.locants.iter().cloned().zip(a).collect())
        .collect()
}
// Provenance: fn match_retained_graph_template_maps_with_options | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::match_retained_graph_template_maps
pub(super) fn match_retained_graph_template_maps_with_options(
    mol: &Molecule,
    atoms: &AtomSet,
    t: &RetainedGraphTemplate,
    nonaromatic: bool,
    relocate: bool,
) -> Vec<RetainedGraphTemplateMatch> {
    if atoms.len() != t.atoms.len()
        || t.atoms.iter().all(|a| !a.aromatic)
            && atoms.iter().any(|&a| !is_saturated_site(mol, a, atoms))
    {
        return Vec::new();
    }
    // Upstream divergence: hydrogenating a localized retained parent can make
    // its remaining 4n pi cycle ambiguous again. Hydro locants identify the
    // saturated sites, but do not select either alternating state of that cycle.
    if LOCALIZED_PI.parents.contains_key(&t.name)
        && atoms.iter().any(|&a| is_saturated_site(mol, a, atoms))
        && has_localized_alternating_4n_cycle(mol, atoms)
    {
        return Vec::new();
    }
    let by_locant = if relocate {
        relocatable_atom_by_locant(t)
    } else {
        t.atom_by_locant()
    };
    let index: BTreeMap<_, _> = t
        .locants
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i))
        .collect();
    let mut neighbors = vec![BTreeSet::new(); t.locants.len()];
    let mut bond_classes = BTreeMap::new();
    for b in &t.bonds {
        let (Some(&i), Some(&j)) = (
            index.get(b.locants.0.as_str()),
            index.get(b.locants.1.as_str()),
        ) else {
            return Vec::new();
        };
        neighbors[i].insert(j);
        neighbors[j].insert(i);
        bond_classes.insert((i.min(j), i.max(j)), b.bond_class.clone());
    }
    let fixed = t.atoms.iter().any(|a| a.saturated);
    let candidates: Vec<Vec<usize>> = t
        .locants
        .iter()
        .enumerate()
        .map(|(i, l)| {
            atoms
                .iter()
                .copied()
                .filter(|&a| {
                    mol.get_neighbors(a)
                        .iter()
                        .filter(|n| atoms.contains(n))
                        .count()
                        == neighbors[i].len()
                        && atom_matches(mol, a, &by_locant[l], atoms, nonaromatic, t, fixed)
                })
                .collect()
        })
        .collect();
    if candidates.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let mut assignments = Vec::new();
    collect_assignments(
        mol,
        t,
        &neighbors,
        &bond_classes,
        &candidates,
        &mut vec![None; t.locants.len()],
        &mut AtomSet::new(),
        &mut assignments,
    );
    // Upstream divergence: neutral Kekule equivalence is not a proof for a
    // nonaromatic parent. A substituent or isotope can distinguish pi states
    // that its untyped graph automorphisms exchange. Without a declared bond
    // pattern for the emitted retained name, decline those ambiguous states.
    if t.aromatic_equivalence_policy == "neutral_kekule_equivalent"
        && t.atoms.iter().all(|a| a.symbol == "C" && a.aromatic)
        && atoms.iter().all(|a| !mol.atoms[a].is_aromatic)
        && atoms.iter().any(|a| {
            mol.atoms[a].isotope.is_some()
                || mol.get_neighbors(*a).iter().any(|n| !atoms.contains(n))
        })
        && t.mancude_double_bonds.is_some_and(|expected| {
            expected > 0
                && mol
                    .bonds
                    .values()
                    .filter(|b| b.order == 2 && atoms.contains(&b.u) && atoms.contains(&b.v))
                    .count()
                    == expected
        })
    {
        let pi_states: BTreeSet<Vec<usize>> = assignments
            .iter()
            .map(|assignment| {
                t.bonds
                    .iter()
                    .map(|b| {
                        mol.get_bond(
                            assignment[index[b.locants.0.as_str()]],
                            assignment[index[b.locants.1.as_str()]],
                        )
                        .map_or(0, |bond| bond.order)
                    })
                    .collect()
            })
            .collect();
        if pi_states.len() > 1 {
            return Vec::new();
        }
    }
    // Upstream divergence: graph automorphisms need not preserve the localized
    // pi state of a retained citation. Keep its fixed inter-ring/alternating
    // bonds while allowing ordinary Kekule alternatives in aromatic subgraphs.
    // Hydro locants instead identify the reduced parent state, checked above.
    if let Some(reference) = LOCALIZED_PI
        .parents
        .get(&t.name)
        .filter(|_| !atoms.iter().any(|&a| is_saturated_site(mol, a, atoms)))
    {
        assignments.retain(|assignment| {
            reference.iter().all(|bond| {
                let u = assignment[index[bond.locants.0.as_str()]];
                let v = assignment[index[bond.locants.1.as_str()]];
                mol.get_bond(u, v)
                    .is_some_and(|actual| actual.order == bond.order)
            })
        });
    }
    let mut order: Vec<_> = (0..t.locants.len()).collect();
    order.sort_by_key(|&i| {
        (
            std::cmp::Reverse(neighbors[i].len()),
            by_locant[&t.locants[i]].symbol == "C",
            t.locants[i].clone(),
        )
    });
    assignments.sort_by_key(|assignment| order.iter().map(|&i| assignment[i]).collect::<Vec<_>>());
    let derive = !t.default_indicated_h.is_empty() || t.indicated_hydrogen_count() == 0;
    assignments
        .into_iter()
        .filter_map(|assignment| {
            if t.implied_stereo
                && t.atoms.iter().filter(|a| a.required_stereo).any(|a| {
                    let x = &mol.atoms[&assignment[index[a.locant.as_str()]]];
                    x.stereo.is_none() && x.raw_stereo.is_none()
                })
            {
                return None;
            }
            let indicated = if relocate {
                relocated_indicated_h(mol, t, atoms, &assignment, false)?
            } else if !derive {
                relocated_indicated_h(mol, t, atoms, &assignment, true)
                    .unwrap_or_else(|| t.default_indicated_h.clone())
            } else {
                t.default_indicated_h.clone()
            };
            let locant_to_atom: BTreeMap<_, _> =
                t.locants.iter().cloned().zip(assignment).collect();
            Some(RetainedGraphTemplateMatch {
                template: t.clone(),
                atom_to_locant: locant_to_atom
                    .iter()
                    .map(|(l, &a)| (a, l.clone()))
                    .collect(),
                locant_to_atom,
                matched_atoms: atoms.clone(),
                indicated_h: indicated,
            })
        })
        .collect()
}
// Provenance: fn has_localized_alternating_4n_cycle | original
// Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::has_localized_alternating_4n_cycle
fn has_localized_alternating_4n_cycle(mol: &Molecule, atoms: &AtomSet) -> bool {
    // Provenance: fn has_localized_alternating_4n_cycle::search | original
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::has_localized_alternating_4n_cycle::search
    fn search(
        mol: &Molecule,
        atoms: &AtomSet,
        path: &mut Vec<usize>,
        next_order: usize,
        localized: bool,
        remaining: &mut usize,
    ) -> bool {
        if *remaining == 0 {
            return true;
        }
        *remaining -= 1;
        let start = path[0];
        let current = path[path.len() - 1];
        for &next in mol.get_neighbors(current) {
            if !atoms.contains(&next) || next < start {
                continue;
            }
            let Some(bond) = mol
                .get_bond(current, next)
                .filter(|b| b.order == next_order)
            else {
                continue;
            };
            let localized = localized || !bond.is_aromatic;
            if next == start && path.len() >= 4 && path.len().is_multiple_of(4) && localized {
                return true;
            } else if !path.contains(&next) {
                path.push(next);
                let found = search(mol, atoms, path, 3 - next_order, localized, remaining);
                path.pop();
                if found {
                    return true;
                }
            }
        }
        false
    }
    let mut remaining = 10_000;
    atoms
        .iter()
        .any(|&start| search(mol, atoms, &mut vec![start], 2, false, &mut remaining))
}

// Provenance: fn relocated_indicated_h | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_relocated_indicated_h
fn relocated_indicated_h(
    mol: &Molecule,
    t: &RetainedGraphTemplate,
    atoms: &AtomSet,
    assignment: &[usize],
    strict: bool,
) -> Option<Vec<String>> {
    let by = t.atom_by_locant();
    let saturated: Vec<_> = t
        .locants
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            !elements::get(&by[*l].symbol).is_some_and(|e| e.mancude_forced_single)
                && is_saturated_site(mol, assignment[*i], atoms)
        })
        .map(|(i, l)| (l.clone(), assignment[i]))
        .collect();
    let needed = t.indicated_hydrogen_count();
    if saturated.len() < needed {
        return None;
    }
    let mut movable: Vec<_> = saturated
        .iter()
        .filter(|(_, a)| !is_retained_oxo_site(mol, *a))
        .map(|(l, _)| l.clone())
        .collect();
    if movable.len() < needed {
        movable = saturated.into_iter().map(|(l, _)| l).collect();
    }
    if strict && movable.len() != needed {
        return None;
    }
    movable.sort_by_key(|l| parse_locant(l));
    movable.truncate(needed);
    Some(movable)
}
#[allow(clippy::too_many_arguments)]
// Provenance: fn collect_assignments | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_collect_locant_matches
fn collect_assignments(
    mol: &Molecule,
    t: &RetainedGraphTemplate,
    neighbors: &[BTreeSet<usize>],
    classes: &BTreeMap<(usize, usize), String>,
    candidates: &[Vec<usize>],
    assignment: &mut [Option<usize>],
    used: &mut AtomSet,
    matches: &mut Vec<Vec<usize>>,
) {
    if matches.len() >= 256 {
        return;
    }
    if used.len() == assignment.len() {
        matches.push(assignment.iter().flatten().copied().collect());
        return;
    }
    let remaining: Vec<_> = (0..assignment.len())
        .filter(|&i| assignment[i].is_none())
        .collect();
    let frontier: Vec<_> = remaining
        .iter()
        .copied()
        .filter(|&i| neighbors[i].iter().any(|&n| assignment[n].is_some()))
        .collect();
    let mut best = None;
    for &i in if frontier.is_empty() {
        &remaining
    } else {
        &frontier
    } {
        let options: Vec<_> = candidates[i]
            .iter()
            .copied()
            .filter(|a| {
                !used.contains(a)
                    && assignment.iter().enumerate().all(|(j, assigned)| {
                        assigned.is_none_or(|b| {
                            let expected = neighbors[i].contains(&j);
                            let bond = mol.get_bond(*a, b);
                            expected == bond.is_some()
                                && (!expected
                                    || t.aromatic_equivalence_policy != "exact"
                                    || bond.is_some_and(|b| {
                                        let class = classes.get(&(i.min(j), i.max(j)));
                                        b.order
                                            == if class.is_some_and(|s| s == "double") {
                                                2
                                            } else {
                                                1
                                            }
                                    }))
                        })
                    })
            })
            .collect();
        if options.is_empty() {
            return;
        }
        let key = (
            options.len(),
            std::cmp::Reverse(
                neighbors[i]
                    .iter()
                    .filter(|&&n| assignment[n].is_some())
                    .count(),
            ),
            std::cmp::Reverse(neighbors[i].len()),
            t.locants[i].clone(),
        );
        if best.as_ref().is_none_or(|(old, _, _)| key < *old) {
            best = Some((key, i, options));
        }
    }
    if let Some((_, i, options)) = best {
        for a in options {
            assignment[i] = Some(a);
            used.insert(a);
            collect_assignments(
                mol, t, neighbors, classes, candidates, assignment, used, matches,
            );
            used.remove(&a);
            assignment[i] = None;
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedSeriesData {
    #[serde(rename = "bluebook_rule")]
    _bluebook_rule: String,
    acenes: RetainedSeries,
    polyaphenes: RetainedSeries,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedSeries {
    member_overrides: BTreeMap<usize, RetainedSeriesMember>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedSeriesMember {
    priority: i64,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true")]
    derivative_production_enabled: bool,
}
// Provenance: fn default_true | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_acene_template_from_data
// Source: openclatura | src/openclatura/retained_fused_templates.py::_polyaphene_template_from_data
fn default_true() -> bool {
    true
}
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained series data"
)]
// Provenance: fn generated_series_template | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_acene_template_from_data
// Source: openclatura | src/openclatura/retained_fused_templates.py::_polyaphene_template_from_data
fn generated_series_template(family: &str, count: usize) -> RetainedGraphTemplate {
    static SERIES: LazyLock<RetainedSeriesData> = LazyLock::new(|| {
        serde_json::from_str(include_str!("data/retained_fused_series.json"))
            .expect("checked-in retained fused series")
    });
    let (locants, edges) = if family == "acenes" {
        higher_acene_locant_graph(count)
    } else {
        polyaphene_locant_graph(count)
    };
    let series = if family == "acenes" {
        &SERIES.acenes
    } else {
        &SERIES.polyaphenes
    };
    let overrides = series.member_overrides.get(&count);
    let name = format!(
        "{}{}",
        multipliers::basic(count),
        if family == "acenes" { "cene" } else { "phene" }
    );
    let fusion_atoms: Vec<_> = locants
        .iter()
        .filter(|l| !l.chars().all(|c| c.is_ascii_digit()))
        .cloned()
        .collect();
    RetainedGraphTemplate {
        name,
        priority: overrides.map_or(1000, |member| member.priority),
        atoms: locants
            .iter()
            .map(|l| RetainedGraphAtomTemplate {
                locant: l.clone(),
                symbol: "C".into(),
                aromatic: true,
                fusion: fusion_atoms.contains(l),
                ..Default::default()
            })
            .collect(),
        bonds: edges
            .iter()
            .map(|e| RetainedGraphBondTemplate {
                locants: e.clone(),
                bond_class: "aromatic".into(),
            })
            .collect(),
        fusion_atoms,
        locants,
        family: "fused".into(),
        aromatic_equivalence_policy: "neutral_kekule_equivalent".into(),
        charge_policy: "exact".into(),
        enabled: overrides.is_none_or(|member| member.enabled),
        derivative_production_enabled: overrides
            .is_none_or(|member| member.derivative_production_enabled),
        mancude_double_bonds: Some(2 * count + 1),
        pre_descriptor_selection: true,
        ..Default::default()
    }
}
// Provenance: fn higher_acene_locant_graph | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_higher_acene_locant_graph
fn higher_acene_locant_graph(count: usize) -> (Vec<String>, Vec<(String, String)>) {
    let left: Vec<_> = (4..count + 3).map(|n| format!("{n}a")).collect();
    let right_start = count + 6;
    let right: Vec<_> = (right_start..2 * count + 5)
        .map(|n| format!("{n}a"))
        .collect();
    let mut locants: Vec<_> = (1..=4).map(|n| n.to_string()).collect();
    locants.push(left[0].clone());
    locants.extend((5..count + 3).flat_map(|n| [n.to_string(), format!("{n}a")]));
    locants.extend((count + 3..=right_start).map(|n| n.to_string()));
    locants.push(right[0].clone());
    locants.extend((right_start + 1..2 * count + 5).flat_map(|n| [n.to_string(), format!("{n}a")]));
    let mut edges: Vec<_> = (0..locants.len())
        .map(|i| (locants[i].clone(), locants[(i + 1) % locants.len()].clone()))
        .collect();
    edges.extend(left.into_iter().rev().zip(right));
    (locants, edges)
}
// Provenance: fn polyaphene_locant_graph | adapted
// Source: openclatura | src/openclatura/retained_fused_templates.py::_polyaphene_locant_graph
fn polyaphene_locant_graph(count: usize) -> (Vec<String>, Vec<(String, String)>) {
    let maximum = 2 * count + 4;
    let omitted = (count + 7) / 2;
    let bay = 3 * count / 2 + 6;
    let first: Vec<_> = (4..count + 4).filter(|&n| n != omitted).collect();
    let second: Vec<_> = (count + 7..=maximum).collect();
    let mut locants = Vec::new();
    for n in 1..=maximum {
        locants.push(n.to_string());
        if first.contains(&n) || second.contains(&n) {
            locants.push(format!("{n}a"));
        }
        if n == bay {
            locants.push(format!("{n}b"));
        }
    }
    let mut edges: Vec<_> = (0..locants.len())
        .map(|i| (locants[i].clone(), locants[(i + 1) % locants.len()].clone()))
        .collect();
    let targets: Vec<_> = locants
        .iter()
        .filter(|l| **l == format!("{bay}b") || second.iter().any(|n| **l == format!("{n}a")))
        .cloned()
        .collect();
    edges.extend(
        first
            .into_iter()
            .rev()
            .map(|n| format!("{n}a"))
            .zip(targets),
    );
    (locants, edges)
}
#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;
    fn matches(smiles: &str, options: MatchOptions<'_>) -> Vec<RetainedGraphTemplateMatch> {
        let mol = read_smiles(smiles, &Default::default()).unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        match_retained_graph_templates_with_options(&mol, &atoms, options)
    }
    #[test]
    // Provenance: fn tests::registry_expands_all_declared_locanted_graphs | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::registry_expands_all_declared_locanted_graphs
    // Source: openclatura | tests/roundtrip/test_retained_macrocycles.py::test_macrocycle_match_exposes_all_numbering_orientations
    // Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py::test_polyaphene_series_generates_standard_locant_graphs
    // Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py::test_higher_acene_series_generates_standard_locant_graphs
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn registry_expands_all_declared_locanted_graphs() {
        let templates = retained_graph_templates(true);
        assert!(templates.len() > 250);
        for t in templates {
            assert_eq!(t.locants.len(), t.atoms.len(), "{}", t.name);
            let locants: BTreeSet<_> = t.locants.iter().collect();
            assert_eq!(locants.len(), t.locants.len(), "{}", t.name);
            assert_eq!(
                locants,
                t.atoms.iter().map(|a| &a.locant).collect(),
                "{}",
                t.name
            );
            assert!(
                t.bonds
                    .iter()
                    .all(|b| locants.contains(&b.locants.0) && locants.contains(&b.locants.1)),
                "{}",
                t.name
            );
            assert!(!t.bonds.is_empty(), "{}", t.name);
        }
        assert!(
            templates
                .iter()
                .any(|t| t.family == "macrocycle" && t.name == "porphyrin")
        );
        assert!(templates.iter().any(|t| t.name == "tetracene"));
        assert!(templates.iter().any(|t| t.name == "decaphene"));
    }
    #[test]
    // Provenance: fn tests::retained_graphs_accept_kekule_forms_and_all_numberings | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::retained_graphs_accept_kekule_forms_and_all_numberings
    // Source: openclatura | tests/unit/test_analysis.py::test_fused_and_polycyclic_retained_signatures_are_data_driven
    // Source: openclatura | tests/unit/test_analysis.py::test_retained_fused_graph_templates_match_core_numbering_when_enabled_for_audit
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn retained_graphs_accept_kekule_forms_and_all_numberings() {
        for (smiles, name, count) in [
            ("c1ccc2ccccc2c1", "naphthalene", 4),
            ("C1=CC=C2C=CC=CC2=C1", "naphthalene", 4),
            ("c1ccc2ncccc2c1", "quinoline", 1),
            ("c1ccc2[nH]ccc2c1", "1H-indole", 1),
            ("C1C2CC3CC1CC(C2)C3", "adamantane", 24),
            ("C12C3C4C1C5C2C3C45", "cubane", 48),
        ] {
            let rows = matches(smiles, MatchOptions::default());
            assert_eq!(
                rows.first().map(|m| m.template.name.as_str()),
                Some(name),
                "{smiles}"
            );
            assert_eq!(
                rows.iter().filter(|m| m.template.name == name).count(),
                count,
                "{smiles}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::xanthene_congener_templates_keep_heteroatom_and_methylene_locants | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::xanthene_congener_templates_keep_heteroatom_and_methylene_locants
    // Source: openclatura | tests/roundtrip/test_retained_fused_production.py::test_expected_retained_rings_are_registered
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn xanthene_congener_templates_keep_heteroatom_and_methylene_locants() {
        for (smiles, name, symbol) in [
            ("O1c2ccccc2Cc2ccccc21", "xanthene", "O"),
            ("S1c2ccccc2Cc2ccccc21", "9H-thioxanthene", "S"),
            ("[Se]1c2ccccc2Cc2ccccc21", "9H-selenoxanthene", "Se"),
            ("[Te]1c2ccccc2Cc2ccccc21", "9H-telluroxanthene", "Te"),
        ] {
            let mol = read_smiles(smiles, &Default::default()).unwrap();
            let rows = matches(smiles, MatchOptions::default());
            assert_eq!(rows.len(), 2, "{smiles}");
            for row in rows {
                assert_eq!(row.template.name, name, "{smiles}");
                assert_eq!(row.indicated_h, ["9"], "{smiles}");
                assert_eq!(row.template.metadata().inherent_saturated_locants, ["9"]);
                assert_eq!(mol.atoms[&row.locant_to_atom["10"]].symbol, symbol);
                let methylene = &mol.atoms[&row.locant_to_atom["9"]];
                assert_eq!(methylene.symbol, "C");
                assert_eq!(methylene.total_h_count, 2, "{smiles}");
            }
        }
    }

    #[test]
    fn added_phenanthroline_isomers_have_exact_retained_graphs() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected, nitrogen_locants) in [
            ("c1nccc2ccc3ccncc3c12", "2,9-phenanthroline", ["2", "9"]),
            ("c1cncc2ccc3ncccc3c12", "3,7-phenanthroline", ["3", "7"]),
            ("c1cncc2ccc3cnccc3c12", "3,8-phenanthroline", ["3", "8"]),
            ("c1ccnc2ccc3ncccc3c12", "4,7-phenanthroline", ["4", "7"]),
        ] {
            let rows = matches(smiles, MatchOptions::default());
            assert!(!rows.is_empty(), "{smiles}");
            for row in rows {
                assert_eq!(row.template.name, expected, "{smiles}");
                let actual: BTreeSet<_> = row
                    .template
                    .atoms
                    .iter()
                    .filter(|atom| atom.symbol == "N")
                    .map(|atom| atom.locant.as_str())
                    .collect();
                assert_eq!(actual, nitrogen_locants.into_iter().collect());
            }
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, crate::naming::NameStatus::Preferred);
            for candidate in std::iter::once(&name.text).chain(
                name.alternatives
                    .iter()
                    .map(|alternative| &alternative.name),
            ) {
                let parsed = parser.parse(candidate, &opsin::ParseOptions::strict());
                assert_eq!(
                    parsed.status,
                    opsin::Status::Success,
                    "{candidate}: {}",
                    parsed.message
                );
                assert!(
                    parsed.warnings.is_empty(),
                    "{candidate}: {:?}",
                    parsed.warnings
                );
                let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
                crate::naming::verify_identity(smiles, &reconstructed)
                    .unwrap_or_else(|error| panic!("{smiles} -> {candidate}: {error}"));
            }
        }
    }

    #[test]
    // Provenance: fn tests::indicated_h_carbanions_keep_the_retained_parent_and_locant | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::indicated_h_carbanions_keep_the_retained_parent_and_locant
    // Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py::test_requested_fused_parent_name_is_invariant_to_atom_order
    // Source: openclatura | tests/unit/test_fusion_component_h_citation.py::test_parent_citation_uses_declared_template_prefix_only
    // Source: openclatura | tests/roundtrip/test_fused_parents.py::test_fused_parent_suite_pin_claims_are_explicit_and_evidence_backed
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn indicated_h_carbanions_keep_the_retained_parent_and_locant() {
        for (smiles, parent, locant) in [
            ("[CH-]1C=Cc2ccccc21", "1H-indene", "1"),
            ("[CH-]1c2ccccc2-c2ccccc21", "9H-fluorene", "9"),
        ] {
            let mol = read_smiles(smiles, &Default::default()).unwrap();
            let charged = *mol
                .atoms
                .iter()
                .find(|(_, atom)| atom.charge == -1)
                .unwrap()
                .0;
            let rows = matches(smiles, MatchOptions::default());
            assert!(!rows.is_empty(), "{smiles}");
            for row in rows {
                assert_eq!(row.template.name, parent, "{smiles}");
                assert_eq!(row.atom_to_locant[&charged], locant, "{smiles}");
                assert_eq!(row.indicated_h, [locant], "{smiles}");
            }
        }
        for (smiles, expected) in [
            ("[CH-]1C=Cc2ccccc21", "1H-inden-1-ide"),
            (
                "C[C@H](O)[C-]1C=Cc2ccccc21",
                "1-[(1S)-1-hydroxyethyl]-1H-inden-1-ide",
            ),
            (
                "C[C@H](O)[C-]1c2ccccc2-c2ccccc21",
                "9-[(1S)-1-hydroxyethyl]-9H-fluoren-9-ide",
            ),
            ("[CH-]1c2ccccc2-c2ccccc21", "9H-fluoren-9-ide"),
            ("C1=Cc2ccccc2C1", "1H-indene"),
            ("c1ccc2c(c1)Cc1ccccc1-2", "9H-fluorene"),
        ] {
            for input in [
                smiles.to_owned(),
                crate::naming::canonical_smiles(smiles).unwrap(),
            ] {
                let name = super::super::name_smiles(&input, &Default::default()).unwrap();
                assert_eq!(name.text, expected, "{smiles} via {input}");
                assert_eq!(
                    name.status,
                    crate::naming::NameStatus::Preferred,
                    "{smiles} via {input}"
                );
            }
        }
    }

    #[test]
    fn indicated_h_carbanion_admission_does_not_relax_other_charge_states() {
        for smiles in [
            "[CH+]1C=Cc2ccccc21",
            "[C-]1C=Cc2ccccc21",
            "C1=[C-]Cc2ccccc21",
            "[CH+]1c2ccccc2-c2ccccc21",
        ] {
            assert!(
                matches(smiles, MatchOptions::default()).is_empty(),
                "{smiles}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::exact_charges_and_graph_connectivity_are_not_relaxed | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::exact_charges_and_graph_connectivity_are_not_relaxed
    // Source: openclatura | tests/unit/test_analysis.py::test_preferred_retained_output_names_preserve_exact_template_metadata
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn exact_charges_and_graph_connectivity_are_not_relaxed() {
        assert!(
            matches(
                "[C+]12C3C4C1C5C2C3C45",
                MatchOptions {
                    allow_nonaromatic: true,
                    ..Default::default()
                }
            )
            .is_empty()
        );
        assert!(matches("C1CC2CCC1CC2", MatchOptions::default()).is_empty());
        let rows = matches("c1ccc2CCCc2c1", MatchOptions::default());
        assert_eq!(
            rows.first().map(|m| m.template.name.as_str()),
            Some("indane")
        );
    }
    #[test]
    // Provenance: fn tests::retained_fused_names_do_not_erase_ring_triples_or_cumulated_doubles | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::retained_fused_names_do_not_erase_ring_triples_or_cumulated_doubles
    // Source: openclatura | tests/unit/test_analysis.py::test_general_retained_ring_recognizer_uses_templates_only_for_fused_systems
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn retained_fused_names_do_not_erase_ring_triples_or_cumulated_doubles() {
        for (smiles, legacy_name) in [
            ("C1=CC=C2C(=C1)C3=CC#CC=C3C2", None),
            ("C1=CC=C2C=CC#CC2=C1", None),
            ("C1=CC=C2NC#CC2=C1", None),
            (
                "C1=CC=C2C(=C1)C3=C=CC=CC3C2",
                Some("tricyclo[7.4.0.0^{2,7}]trideca-1(13),2,3,5,9,11-hexaene"),
            ),
            (
                "C1=CC=C2C=C=CCC2=C1",
                Some("bicyclo[4.4.0]deca-1(10),2,3,6,8-pentaene"),
            ),
        ] {
            for (allow_nonaromatic, allow_relocated_indicated_h) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                assert!(
                    matches(
                        smiles,
                        MatchOptions {
                            family: Some("fused"),
                            allow_nonaromatic,
                            allow_relocated_indicated_h,
                            ..Default::default()
                        }
                    )
                    .is_empty(),
                    "{smiles}"
                );
            }
            let name = super::super::name_smiles(smiles, &Default::default());
            if let Some(expected) = legacy_name {
                assert_eq!(name.unwrap().text, expected);
            } else {
                assert!(name.is_err(), "{smiles}");
            }
        }
        assert!(
            super::super::name_smiles("C1=CC=C2C(=C1)C3=CC#CC=C3C2(CCO)CCO", &Default::default())
                .is_err()
        );

        let mol = read_smiles("C#Cc1ccc2ccccc2c1", &Default::default()).unwrap();
        let ring = mol
            .atoms
            .iter()
            .filter(|(_, atom)| atom.is_aromatic)
            .map(|(&idx, _)| idx)
            .collect();
        assert_eq!(
            match_retained_graph_templates(&mol, &ring, false)
                .first()
                .map(|m| m.template.name.as_str()),
            Some("naphthalene")
        );
        assert_eq!(
            super::super::name_smiles("C#Cc1ccc2ccccc2c1", &Default::default())
                .map(|name| name.text)
                .as_deref(),
            Ok("2-ethynylnaphthalene")
        );
    }
    #[test]
    // Provenance: fn tests::decorated_nonaromatic_parent_requires_unambiguous_pi_state | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::decorated_nonaromatic_parent_requires_unambiguous_pi_state
    // Source: openclatura | tests/roundtrip/test_retained_fused_production.py::test_expected_retained_rings_are_registered
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn decorated_nonaromatic_parent_requires_unambiguous_pi_state() {
        let mol = read_smiles("ClC1=CC=CC2=CC=CC=CC2=C1", &Default::default()).unwrap();
        let ring: AtomSet = mol
            .atoms
            .iter()
            .filter(|(_, a)| a.symbol == "C")
            .map(|(&i, _)| i)
            .collect();
        assert!(match_retained_graph_templates(&mol, &ring, false).is_empty());
        assert_eq!(
            matches("C1=CC=CC2=CC=CC=CC2=C1", MatchOptions::default())
                .first()
                .map(|m| m.template.name.as_str()),
            Some("heptalene")
        );

        let aromatic = read_smiles("ClC1=CC=CC2=CC=CC=C12", &Default::default()).unwrap();
        let ring: AtomSet = aromatic
            .atoms
            .iter()
            .filter(|(_, a)| a.symbol == "C")
            .map(|(&i, _)| i)
            .collect();
        assert_eq!(
            match_retained_graph_templates(&aromatic, &ring, false)
                .first()
                .map(|m| m.template.name.as_str()),
            Some("naphthalene")
        );
    }
    #[test]
    // Provenance: fn tests::retained_localized_pi_state_constrains_all_numbering_maps | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::retained_localized_pi_state_constrains_all_numbering_maps
    // Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py::test_requested_fused_parent_name_is_invariant_to_atom_order
    // Source: openclatura | tests/roundtrip/test_retained_fused_production.py::test_expected_retained_rings_are_registered
    // Source: openclatura | tests/roundtrip/test_fused_parents.py::test_roundtrip
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn retained_localized_pi_state_constrains_all_numbering_maps() {
        for (smiles, expected) in [
            ("c1ccc2c(c1)-c1ccccc1-2", "biphenylene"),
            ("c1ccc2c(c1)-c1ccccc1-c1ccccc1-c1ccccc1-2", "tetraphenylene"),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles, &Default::default())
                    .map(|name| name.text)
                    .as_deref(),
                Ok(expected)
            );
        }
        for smiles in [
            "c1ccc2c(c1)=c1ccccc1=2",
            "Clc1cccc2c1=c1ccccc1=2",
            "c1ccc2c(c1)=c1ccccc1=c1ccccc1=c1ccccc1=2",
            "C1=CC2=C(CC1)C1=C2CCC=C1",
            "C1=CC2=c3ccccc3=C2CC1",
        ] {
            let mol = read_smiles(smiles, &Default::default()).unwrap();
            let ring = mol
                .atoms
                .iter()
                .filter(|(_, atom)| atom.symbol == "C")
                .map(|(&idx, _)| idx)
                .collect();
            for (allow_nonaromatic, allow_relocated_indicated_h) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                assert!(
                    match_retained_graph_templates_with_options(
                        &mol,
                        &ring,
                        MatchOptions {
                            family: Some("fused"),
                            allow_nonaromatic,
                            allow_relocated_indicated_h,
                            ..Default::default()
                        },
                    )
                    .is_empty(),
                    "{smiles}"
                );
            }
        }
        // A numbering that exchanges these localized bonds must not turn a
        // position-3 substituent into position 1 on the cited indacene parent.
        assert_eq!(
            super::super::name_smiles("ClC1=c2cc3c(cc2C=C1)=CC=C3", &Default::default())
                .map(|name| name.text)
                .as_deref(),
            Ok("3-chloros-indacene")
        );
    }

    #[test]
    // Provenance: fn tests::preferred_output_names_have_metadata_and_aliases_do_not | adapted
    // Source: ReShiki | src/naming/openclatura/retained_fused_templates.rs::tests::preferred_output_names_have_metadata_and_aliases_do_not
    // Source: openclatura | tests/roundtrip/test_fused_hydrocarbon_retained.py::test_hydrogenated_parent_uses_data_backed_preferred_name
    // Source: openclatura | tests/unit/test_analysis.py::test_no_bare_retained_alias_infers_a_canonical_hydride_tautomer
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn preferred_output_names_have_metadata_and_aliases_do_not() {
        assert!(retained_parent_metadata("2,3-dihydro-1H-indole").is_some());
        assert!(retained_parent_metadata("indoline").is_some());
        assert!(retained_parent_metadata("isoindole").is_none());
    }
}
