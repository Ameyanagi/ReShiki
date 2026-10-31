//! Shared, dependency-free records for numbered retained parent graphs.
use super::{assembly_parts::RetainedParentMetadata, molecule::AtomSet, rules::elements};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct RetainedGraphAtomTemplate {
    pub locant: String,
    pub symbol: String,
    pub charge: i32,
    pub aromatic: bool,
    pub fusion: bool,
    pub default_h: bool,
    pub saturated: bool,
    pub pi_capacity: Option<usize>,
    pub forced_single: bool,
    pub indicated_h_site: bool,
    pub required_stereo: bool,
}
impl RetainedGraphAtomTemplate {
    pub fn resolved_pi_capacity(&self) -> usize {
        if self.saturated || self.forced_single {
            return 0;
        }
        self.pi_capacity.unwrap_or_else(|| {
            usize::from(!elements::get(&self.symbol).is_some_and(|e| e.mancude_forced_single))
        })
    }
}
#[derive(Clone, Debug, Default)]
pub struct RetainedGraphBondTemplate {
    pub locants: (String, String),
    pub bond_class: String,
}
#[derive(Clone, Debug, Default)]
pub struct RetainedGraphTemplate {
    pub name: String,
    pub priority: i64,
    pub default_indicated_h: Vec<String>,
    pub locants: Vec<String>,
    pub atoms: Vec<RetainedGraphAtomTemplate>,
    pub bonds: Vec<RetainedGraphBondTemplate>,
    pub fusion_atoms: Vec<String>,
    pub family: String,
    pub aromatic_equivalence_policy: String,
    pub charge_policy: String,
    pub enforce_mancude_double_bonds: bool,
    pub enabled: bool,
    pub derivative_production_enabled: bool,
    pub implied_stereo: bool,
    pub mancude_double_bonds: Option<usize>,
    pub indicated_hydrogen_count_override: Option<usize>,
    pub pre_descriptor_selection: bool,
}
impl RetainedGraphTemplate {
    pub fn atom_by_locant(&self) -> BTreeMap<String, RetainedGraphAtomTemplate> {
        self.atoms
            .iter()
            .map(|a| (a.locant.clone(), a.clone()))
            .collect()
    }
    pub fn indicated_hydrogen_count(&self) -> usize {
        self.indicated_hydrogen_count_override.unwrap_or_else(|| {
            if !self.default_indicated_h.is_empty() {
                self.default_indicated_h.len()
            } else {
                self.atoms
                    .iter()
                    .filter(|a| {
                        !a.aromatic
                            && !a.fusion
                            && !elements::get(&a.symbol).is_some_and(|e| e.mancude_forced_single)
                    })
                    .count()
            }
        })
    }
    pub fn output_name(&self) -> String {
        super::retained_name_policy::retained_parent_output_name(
            &self.name,
            "unsubstituted_parent",
            &[],
            None,
        )
    }
    pub fn metadata(&self) -> RetainedParentMetadata {
        RetainedParentMetadata {
            default_indicated_h: self.default_indicated_h.clone(),
            fusion_locants: self.fusion_atoms.clone(),
            indicated_hydrogen_count: self.indicated_hydrogen_count(),
            mancude_double_bonds: self.mancude_double_bonds.unwrap_or(0),
            inherent_saturated_locants: self
                .atoms
                .iter()
                .filter(|a| a.saturated)
                .map(|a| a.locant.clone())
                .collect(),
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug)]
pub struct RetainedGraphTemplateMatch {
    pub template: RetainedGraphTemplate,
    pub atom_to_locant: BTreeMap<usize, String>,
    pub locant_to_atom: BTreeMap<String, usize>,
    pub matched_atoms: AtomSet,
    pub indicated_h: Vec<String>,
}
