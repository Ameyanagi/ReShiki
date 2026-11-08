// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/assembly_parts.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/name_operations.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/ring_parent.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/assembly_parts.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Adapted Openclatura assembly records with local anion, isotope and stereo state.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Structured inputs for the plain-name assembly path.
use super::molecule::AtomSet;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct RenderedSubstituentName {
    pub text: String,
    pub outer_parentheses_optional: bool,
    pub oxy_prefix: Option<String>,
}
// Provenance: impl RenderedSubstituentName | original
// Source: ReShiki | src/naming/openclatura/assembly_parts.rs::RenderedSubstituentName
impl RenderedSubstituentName {
    pub fn into_oxy_prefix(self) -> String {
        self.oxy_prefix
            .unwrap_or_else(|| super::formatting::oxy_prefix_from_branch(&self.text))
    }
}
#[derive(Clone, Debug, Default)]
pub struct SubstituentItem {
    pub name: String,
    pub locants: Vec<String>,
    pub atom_ids: AtomSet,
    pub bond_ids: AtomSet,
    pub charge_atom_ids: AtomSet,
    pub outer_parentheses_optional: bool,
    pub spiro: Option<Box<super::assembly_spiro::SpiroAssembly>>,
}
impl SubstituentItem {
    // Provenance: fn SubstituentItem::new | adapted
    // Source: openclatura | src/openclatura/assembly_parts.py::SubstituentItem
    // Source: openclatura | src/openclatura/name_operations.py::module scope (mapping uncertain)
    // Source: openclatura | src/openclatura/ring_parent.py::module scope (mapping uncertain)
    pub fn new(name: impl Into<String>, locants: Vec<String>) -> Self {
        Self {
            name: name.into(),
            locants,
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct UnsaturationItem {
    pub bond_key: String,
    pub locants: Vec<String>,
    pub atom_ids: AtomSet,
    pub bond_ids: AtomSet,
}
#[derive(Clone, Debug, Default)]
pub struct PrincipalGroupItem {
    pub key: String,
    pub locants: Vec<String>,
    pub atom_ids: AtomSet,
    pub bond_ids: AtomSet,
    pub charge_atom_ids: AtomSet,
    pub resolved_rule: Option<super::nomenclature::FunctionalGroupRule>,
    pub external_substitution_sites: bool,
}
impl PrincipalGroupItem {
    // Provenance: fn PrincipalGroupItem::new | adapted
    // Source: openclatura | src/openclatura/assembly_parts.py::PrincipalGroupItem
    // Source: openclatura | src/openclatura/name_operations.py::module scope (mapping uncertain)
    // Source: openclatura | src/openclatura/ring_parent.py::module scope (mapping uncertain)
    pub fn new(key: impl Into<String>, locants: Vec<String>) -> Self {
        Self {
            key: key.into(),
            locants,
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct ParentChargeItem {
    pub locant: String,
    pub symbol: String,
    pub charge: i32,
    pub atom_id: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnionSuffix {
    pub locants: Vec<String>,
    pub cite_locants: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RetainedAnionName {
    #[default]
    NotApplicable,
    Required {
        name: String,
        absorbed_substituent: Option<usize>,
    },
    Applied(String),
    SubstitutedAlkyl,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HydroOperation {
    pub key: String,
    pub locants: Vec<String>,
    pub operation_kind: String,
    pub atom_ids: Vec<usize>,
    pub bond_ids: Vec<usize>,
}
#[derive(Clone, Debug, Default)]
pub struct RetainedParentMetadata {
    pub indicated_hydrogen_count: usize,
    pub mancude_double_bonds: usize,
    pub default_indicated_h: Vec<String>,
    pub inherent_saturated_locants: Vec<String>,
    pub fusion_locants: Vec<String>,
    pub relocated_indicated_h: bool,
}
#[derive(Clone, Debug)]
pub struct AssemblyParts {
    pub parent_length: usize,
    pub is_ring: bool,
    pub is_bicycle: bool,
    pub is_spiro: bool,
    pub is_polycycle: bool,
    pub bicycle_xyz: (usize, usize, usize),
    pub spiro_xy: (usize, usize),
    pub polycycle_descriptor: Option<String>,
    pub is_substituent: bool,
    pub attachment_order: usize,
    pub is_acyl_substituent: bool,
    /// The chalcogen infix of a thio-, seleno- or telluroacyl substituent ("thio").
    pub acyl_infix: &'static str,
    pub attachment_locant: String,
    pub principal_citation_mode: Option<String>,
    pub retained_name: Option<String>,
    pub retained_absorbs_principal_group: bool,
    pub retained_anion_name: RetainedAnionName,
    pub retained_substituent_name: Option<String>,
    pub retained_parent_metadata: Option<RetainedParentMetadata>,
    pub fusion_plan: Option<super::fusion::model::FusionParentPlan>,
    pub parent_hydride: Option<super::ring_parent::RingParent>,
    pub parent_bond_model: Option<super::fusion::model::ParentBondModel>,
    pub parent_bond_delta: Option<super::fusion::mancude::ParentBondDelta>,
    pub parent_derivative_state: Option<super::fusion::mancude::ParentDerivativeState>,
    pub front_modifiers: Vec<String>,
    pub front_modifier_locants: Vec<Option<String>>,
    pub ester_site_exchange_proven: bool,
    pub principal_suffix_modifiers: Vec<SubstituentItem>,
    pub a_prefixes: Vec<SubstituentItem>,
    pub principal_group: Option<PrincipalGroupItem>,
    pub unsaturations: Vec<UnsaturationItem>,
    pub substituents: Vec<SubstituentItem>,
    pub stereo_features: Vec<(String, String)>,
    pub relative_stereo_prefixes: Vec<String>,
    pub endo_exo_descriptors: BTreeMap<String, &'static str>,
    pub indicated_hydrogens: Vec<String>,
    pub hydro_operations: Vec<HydroOperation>,
    pub parent_charges: Vec<ParentChargeItem>,
    pub anion_suffix: Option<AnionSuffix>,
    pub parent_atom_ids: AtomSet,
    pub parent_bond_ids: AtomSet,
    pub stereogenic_atom_ids: AtomSet,
    pub stereogenic_bond_ids: AtomSet,
    pub parent_atom_hydrogens_by_locant: BTreeMap<String, usize>,
    pub parent_external_bonds_by_locant: BTreeMap<String, Vec<(usize, usize, usize)>>,
    pub parent_atom_ids_by_locant: BTreeMap<String, usize>,
    /// Admissible parent numberings before local functional-group preferences.
    pub parent_numbering_maps: Vec<BTreeMap<usize, String>>,
    pub parent_atom_symbols_by_locant: BTreeMap<String, String>,
    pub parent_atom_charges_by_locant: BTreeMap<String, i32>,
    pub parent_atom_isotopes_by_locant: BTreeMap<String, Option<u16>>,
    pub parent_bond_orders_by_locants: BTreeMap<(String, String), usize>,
    pub parent_bond_ids_by_locants: BTreeMap<(String, String), usize>,
    pub proof_locant_map: bool,
    pub omit_redundant_locants: bool,
    /// P-82.6.1.1: positional isotopes require the constitutional locants too.
    pub isotope_explicit_locants: bool,
    pub elided_substituent_locants: BTreeSet<String>,
    pub elided_unsaturation_locants: BTreeSet<String>,
    pub elide_principal_group_locants: bool,
    pub is_spiro_component: bool,
    pub is_spiro_side_projection: bool,
    pub spiro_prefix_composition: bool,
    pub spiro_same_size_components: bool,
    /// The renderer cited locants on hydro prefixes of a fully hydrogenated parent.
    pub locanted_total_hydrogenation: bool,
}
impl Default for AssemblyParts {
    // Provenance: fn <AssemblyParts as Default>::default | adapted
    // Source: openclatura | src/openclatura/assembly_parts.py::AssemblyParts
    // Source: openclatura | src/openclatura/name_operations.py::module scope (mapping uncertain)
    // Source: openclatura | src/openclatura/ring_parent.py::module scope (mapping uncertain)
    fn default() -> Self {
        Self::new(0)
    }
}
impl AssemblyParts {
    // Provenance: fn AssemblyParts::is_ring_assembly | original
    // Source: ReShiki | src/naming/openclatura/assembly_parts.rs::AssemblyParts::is_ring_assembly
    pub fn is_ring_assembly(&self) -> bool {
        self.parent_hydride
            .as_ref()
            .is_some_and(|parent| parent.assembly.is_some())
    }

    // Provenance: fn AssemblyParts::new | adapted
    // Source: openclatura | src/openclatura/assembly_parts.py::AssemblyParts
    // Source: openclatura | src/openclatura/name_operations.py::module scope (mapping uncertain)
    // Source: openclatura | src/openclatura/ring_parent.py::module scope (mapping uncertain)
    pub fn new(parent_length: usize) -> Self {
        Self {
            parent_length,
            is_ring: false,
            is_bicycle: false,
            is_spiro: false,
            is_polycycle: false,
            bicycle_xyz: (0, 0, 0),
            spiro_xy: (0, 0),
            polycycle_descriptor: None,
            is_substituent: false,
            attachment_order: 1,
            is_acyl_substituent: false,
            acyl_infix: "",
            attachment_locant: "1".into(),
            principal_citation_mode: None,
            retained_name: None,
            retained_absorbs_principal_group: false,
            retained_anion_name: RetainedAnionName::NotApplicable,
            retained_substituent_name: None,
            retained_parent_metadata: None,
            fusion_plan: None,
            parent_hydride: None,
            parent_bond_model: None,
            parent_bond_delta: None,
            parent_derivative_state: None,
            front_modifiers: vec![],
            front_modifier_locants: vec![],
            ester_site_exchange_proven: false,
            principal_suffix_modifiers: vec![],
            a_prefixes: vec![],
            principal_group: None,
            unsaturations: vec![],
            substituents: vec![],
            stereo_features: vec![],
            relative_stereo_prefixes: vec![],
            endo_exo_descriptors: BTreeMap::new(),
            indicated_hydrogens: vec![],
            hydro_operations: vec![],
            parent_charges: vec![],
            anion_suffix: None,
            parent_atom_ids: AtomSet::new(),
            parent_bond_ids: AtomSet::new(),
            stereogenic_atom_ids: AtomSet::new(),
            stereogenic_bond_ids: AtomSet::new(),
            parent_atom_hydrogens_by_locant: BTreeMap::new(),
            parent_external_bonds_by_locant: BTreeMap::new(),
            parent_atom_ids_by_locant: BTreeMap::new(),
            parent_numbering_maps: vec![],
            parent_atom_symbols_by_locant: BTreeMap::new(),
            parent_atom_charges_by_locant: BTreeMap::new(),
            parent_atom_isotopes_by_locant: BTreeMap::new(),
            parent_bond_orders_by_locants: BTreeMap::new(),
            parent_bond_ids_by_locants: BTreeMap::new(),
            proof_locant_map: false,
            omit_redundant_locants: true,
            isotope_explicit_locants: false,
            elided_substituent_locants: BTreeSet::new(),
            elided_unsaturation_locants: BTreeSet::new(),
            elide_principal_group_locants: false,
            is_spiro_component: false,
            is_spiro_side_projection: false,
            spiro_prefix_composition: false,
            spiro_same_size_components: false,
            locanted_total_hydrogenation: false,
        }
    }

    // Provenance: fn AssemblyParts::is_simple_ring | adapted
    // Source: openclatura | src/openclatura/assembly_parts.py::AssemblyParts fields is_ring/is_bicycle/is_spiro/is_polycycle
    // Source: openclatura | src/openclatura/name_operations.py::module scope (mapping uncertain)
    // Source: openclatura | src/openclatura/ring_parent.py::module scope (mapping uncertain)
    pub fn is_simple_ring(&self) -> bool {
        self.is_ring && !self.is_bicycle && !self.is_spiro && !self.is_polycycle
    }

    // Provenance: fn AssemblyParts::stereochemical_locants | original
    // Source: ReShiki | src/naming/openclatura/assembly_parts.rs::AssemblyParts::stereochemical_locants
    pub(super) fn stereochemical_locants(&self, locants: &[String]) -> String {
        locants
            .iter()
            .map(|locant| self.stereochemical_locant(locant))
            .collect::<Vec<_>>()
            .join(",")
    }

    // Provenance: fn AssemblyParts::stereochemical_locant | original
    // Source: ReShiki | src/naming/openclatura/assembly_parts.rs::AssemblyParts::stereochemical_locant
    pub(super) fn stereochemical_locant(&self, locant: &str) -> String {
        self.endo_exo_descriptors.get(locant).map_or_else(
            || locant.to_owned(),
            |descriptor| format!("{locant}-{descriptor}"),
        )
    }
}
