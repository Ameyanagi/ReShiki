// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed; upstream-derived code with original nazuchem/ReShiki extensions.
//   openclatura 0.4.3, src/openclatura/retained_graph_model.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
//   openclatura 0.4.3, src/openclatura/retained_fused_templates.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
//   ReShiki, src/naming/openclatura/retained_graph_model.rs @ 5f559fd482e237307a589dfcb7ea333cee0a6e13.
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Adapted openclatura retained graph records and metadata, with original serde default-value glue.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Shared, dependency-free records for numbered retained parent graphs.
use super::{assembly_parts::RetainedParentMetadata, molecule::AtomSet, rules::elements};
use serde::Deserialize;
use std::collections::BTreeMap;

// Original nazuchem code (not in openclatura 0.4.3): fn carbon_symbol
fn carbon_symbol() -> String {
    "C".into()
}
// Original nazuchem code (not in openclatura 0.4.3): fn aromatic
fn aromatic() -> bool {
    true
}
// Original nazuchem code (not in openclatura 0.4.3): fn aromatic_bond_class
fn aromatic_bond_class() -> String {
    "aromatic".into()
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetainedGraphAtomTemplate {
    pub locant: String,
    #[serde(default = "carbon_symbol")]
    pub symbol: String,
    #[serde(default)]
    pub charge: i32,
    #[serde(default = "aromatic")]
    pub aromatic: bool,
    #[serde(default)]
    pub fusion: bool,
    #[serde(default)]
    pub default_h: bool,
    #[serde(default)]
    pub saturated: bool,
    #[serde(default)]
    pub pi_capacity: Option<usize>,
    #[serde(default)]
    pub forced_single: bool,
    #[serde(default)]
    pub indicated_h_site: bool,
    #[serde(default)]
    pub required_stereo: bool,
}
impl RetainedGraphAtomTemplate {
    // Adapted from src/openclatura/retained_graph_model.py, src/openclatura/retained_fused_templates.py (adapted; upstream function mapping not recorded; local fn RetainedGraphAtomTemplate::resolved_pi_capacity)
    pub fn resolved_pi_capacity(&self) -> usize {
        if self.saturated || self.forced_single {
            return 0;
        }
        self.pi_capacity.unwrap_or_else(|| {
            usize::from(!elements::get(&self.symbol).is_some_and(|e| e.mancude_forced_single))
        })
    }
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetainedGraphBondTemplate {
    pub locants: (String, String),
    #[serde(default = "aromatic_bond_class")]
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
    // Adapted from src/openclatura/retained_graph_model.py, src/openclatura/retained_fused_templates.py (adapted; upstream function mapping not recorded; local fn RetainedGraphTemplate::atom_by_locant)
    pub fn atom_by_locant(&self) -> BTreeMap<String, RetainedGraphAtomTemplate> {
        self.atoms
            .iter()
            .map(|a| (a.locant.clone(), a.clone()))
            .collect()
    }
    // Adapted from src/openclatura/retained_graph_model.py, src/openclatura/retained_fused_templates.py (adapted; upstream function mapping not recorded; local fn RetainedGraphTemplate::indicated_hydrogen_count)
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
    // Adapted from src/openclatura/retained_graph_model.py, src/openclatura/retained_fused_templates.py (adapted; upstream function mapping not recorded; local fn RetainedGraphTemplate::output_name)
    pub fn output_name(&self) -> String {
        super::retained_name_policy::retained_parent_output_name(
            &self.name,
            "unsubstituted_parent",
            &[],
            None,
        )
    }
    // Adapted from src/openclatura/retained_fused_templates.py::retained_parent_metadata (adapted; local fn RetainedGraphTemplate::metadata)
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
