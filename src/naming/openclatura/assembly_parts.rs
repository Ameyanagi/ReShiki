//! Structured inputs for the plain-name assembly path.
use super::molecule::AtomSet;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct RenderedSubstituentName {
    pub text: String,
    pub outer_parentheses_optional: bool,
}
#[derive(Clone, Debug, Default)]
pub struct SubstituentItem {
    pub name: String,
    pub locants: Vec<String>,
    pub atom_ids: AtomSet,
    pub bond_ids: AtomSet,
    pub charge_atom_ids: AtomSet,
    pub outer_parentheses_optional: bool,
}
impl SubstituentItem {
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
}
impl PrincipalGroupItem {
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
    pub is_double_attach: bool,
    pub is_triple_attach: bool,
    pub is_acyl_substituent: bool,
    pub attachment_locant: String,
    pub principal_citation_mode: Option<String>,
    pub retained_name: Option<String>,
    pub retained_absorbs_principal_group: bool,
    pub retained_substituent_name: Option<String>,
    pub retained_parent_metadata: Option<RetainedParentMetadata>,
    pub fusion_plan: Option<super::fusion::model::FusionParentPlan>,
    pub parent_bond_model: Option<super::fusion::model::ParentBondModel>,
    pub parent_bond_delta: Option<super::fusion::mancude::ParentBondDelta>,
    pub parent_derivative_state: Option<super::fusion::mancude::ParentDerivativeState>,
    pub front_modifiers: Vec<String>,
    pub front_modifier_locants: Vec<Option<String>>,
    pub principal_suffix_modifiers: Vec<SubstituentItem>,
    pub a_prefixes: Vec<SubstituentItem>,
    pub principal_group: Option<PrincipalGroupItem>,
    pub unsaturations: Vec<UnsaturationItem>,
    pub substituents: Vec<SubstituentItem>,
    pub stereo_features: Vec<(String, String)>,
    pub relative_stereo_prefixes: Vec<String>,
    pub indicated_hydrogens: Vec<String>,
    pub hydro_operations: Vec<HydroOperation>,
    pub parent_charges: Vec<ParentChargeItem>,
    pub parent_atom_ids: AtomSet,
    pub parent_bond_ids: AtomSet,
    pub parent_atom_ids_by_locant: BTreeMap<String, usize>,
    pub parent_atom_symbols_by_locant: BTreeMap<String, String>,
    pub parent_atom_charges_by_locant: BTreeMap<String, i32>,
    pub parent_atom_isotopes_by_locant: BTreeMap<String, Option<u16>>,
    pub parent_bond_orders_by_locants: BTreeMap<(String, String), usize>,
    pub parent_bond_ids_by_locants: BTreeMap<(String, String), usize>,
    pub locant_map_source: String,
    pub omit_redundant_locants: bool,
    pub elided_substituent_locants: BTreeSet<String>,
    pub elided_unsaturation_locants: BTreeSet<String>,
    pub elide_principal_group_locants: bool,
    pub is_spiro_component: bool,
}
impl Default for AssemblyParts {
    fn default() -> Self {
        Self::new(0)
    }
}
impl AssemblyParts {
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
            is_double_attach: false,
            is_triple_attach: false,
            is_acyl_substituent: false,
            attachment_locant: "1".into(),
            principal_citation_mode: None,
            retained_name: None,
            retained_absorbs_principal_group: false,
            retained_substituent_name: None,
            retained_parent_metadata: None,
            fusion_plan: None,
            parent_bond_model: None,
            parent_bond_delta: None,
            parent_derivative_state: None,
            front_modifiers: vec![],
            front_modifier_locants: vec![],
            principal_suffix_modifiers: vec![],
            a_prefixes: vec![],
            principal_group: None,
            unsaturations: vec![],
            substituents: vec![],
            stereo_features: vec![],
            relative_stereo_prefixes: vec![],
            indicated_hydrogens: vec![],
            hydro_operations: vec![],
            parent_charges: vec![],
            parent_atom_ids: AtomSet::new(),
            parent_bond_ids: AtomSet::new(),
            parent_atom_ids_by_locant: BTreeMap::new(),
            parent_atom_symbols_by_locant: BTreeMap::new(),
            parent_atom_charges_by_locant: BTreeMap::new(),
            parent_atom_isotopes_by_locant: BTreeMap::new(),
            parent_bond_orders_by_locants: BTreeMap::new(),
            parent_bond_ids_by_locants: BTreeMap::new(),
            locant_map_source: "generated".into(),
            omit_redundant_locants: true,
            elided_substituent_locants: BTreeSet::new(),
            elided_unsaturation_locants: BTreeSet::new(),
            elide_principal_group_locants: false,
            is_spiro_component: false,
        }
    }
}
