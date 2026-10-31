//! Audited parent-hydride handoff for fusion, replacement and composition.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    assembly_parts::RetainedParentMetadata,
    fusion::{
        mancude::ParentDerivativeState,
        model::{FusionParentPlan, ParentBondModel},
        replacement_state::ReplacementFusionState,
        wrappers::BridgedFusionWrapperPlan,
    },
    molecule::AtomSet,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentHydrideKind {
    Retained,
    SystematicFusion,
    SkeletalReplacementFusion,
    BridgedFusion,
}

#[derive(Clone, Debug)]
pub struct RingParent {
    pub kind: ParentHydrideKind,
    pub atoms: AtomSet,
    pub parent_name: Option<String>,
    pub retained_locant_maps: Vec<BTreeMap<usize, String>>,
    pub hydride_metadata: Option<RetainedParentMetadata>,
    pub parent_bond_model: Option<ParentBondModel>,
    pub fusion_plan: Option<FusionParentPlan>,
    pub fusion_wrapper_plan: Option<Box<BridgedFusionWrapperPlan>>,
    pub skeletal_replacement_atom_ids: Vec<usize>,
    pub skeletal_replacement_audit_checks: Vec<String>,
    pub replacement_fusion_state: Option<ReplacementFusionState>,
}

impl RingParent {
    fn new(kind: ParentHydrideKind, atoms: AtomSet) -> Self {
        Self {
            kind,
            atoms,
            parent_name: None,
            retained_locant_maps: Vec::new(),
            hydride_metadata: None,
            parent_bond_model: None,
            fusion_plan: None,
            fusion_wrapper_plan: None,
            skeletal_replacement_atom_ids: Vec::new(),
            skeletal_replacement_audit_checks: Vec::new(),
            replacement_fusion_state: None,
        }
    }

    pub fn from_retained_locant_maps(
        atoms: AtomSet,
        maps: Vec<BTreeMap<usize, String>>,
        name: String,
        metadata: Option<RetainedParentMetadata>,
    ) -> Result<Self, String> {
        let mut parent = Self::new(ParentHydrideKind::Retained, atoms);
        parent.parent_name = Some(name);
        parent.retained_locant_maps = maps;
        parent.hydride_metadata = metadata;
        if !parent.audit_ok() {
            return Err(
                "openclatura: retained parent locant maps are not complete bijections".into(),
            );
        }
        Ok(parent)
    }

    pub fn from_fusion_plan(plan: FusionParentPlan) -> Result<Self, String> {
        if !plan.audit.confirmed() {
            return Err("openclatura: fusion parent requires a confirmed audit".into());
        }
        let atoms = plan
            .numbering
            .string_input_locant_maps()
            .first()
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        let mut parent = Self::new(ParentHydrideKind::SystematicFusion, atoms);
        parent.hydride_metadata = Some(RetainedParentMetadata {
            default_indicated_h: plan.indicated_hydrogens.clone(),
            indicated_hydrogen_count: plan.indicated_hydrogens.len(),
            mancude_double_bonds: plan.bond_model.maximum_non_cumulative_double_bonds,
            fusion_locants: plan
                .numbering
                .abstract_atom_to_locant
                .iter()
                .map(|(_, l)| l)
                .filter(|l| l.chars().any(|c| c.is_ascii_lowercase()))
                .cloned()
                .collect(),
            ..Default::default()
        });
        parent.fusion_plan = Some(plan);
        Ok(parent)
    }

    pub fn from_fusion_wrapper(plan: BridgedFusionWrapperPlan) -> Result<Self, String> {
        if !plan.audit_ok {
            return Err("openclatura: bridged fusion parent requires a confirmed audit".into());
        }
        let mut atoms = plan.parent.atom_ids().clone();
        atoms.extend(plan.bridges.iter().flat_map(|b| b.atom_ids.iter().copied()));
        let mut parent = Self::new(ParentHydrideKind::BridgedFusion, atoms);
        // Upstream divergence: wrapper composition preserves the parent hydride's
        // indicated-H metadata and the bridge's inherent saturation so later
        // suffix assembly uses the same citation without hydrogenating bridges.
        parent.hydride_metadata = plan.parent.hydride.hydride_metadata.clone();
        if let Some(metadata) = &mut parent.hydride_metadata {
            let locants = plan.atom_to_locant();
            metadata
                .inherent_saturated_locants
                .extend(plan.bridges.iter().flat_map(|bridge| {
                    bridge.atom_ids.iter().enumerate().filter_map(|(i, atom)| {
                        let before = i
                            .checked_sub(1)
                            .and_then(|j| bridge.internal_bond_orders.get(j));
                        let after = bridge.internal_bond_orders.get(i);
                        (!before.is_some_and(|order| *order == 2)
                            && !after.is_some_and(|order| *order == 2))
                        .then(|| locants[atom].clone())
                    })
                }));
        }
        parent.fusion_wrapper_plan = Some(Box::new(plan));
        Ok(parent)
    }

    pub fn into_skeletal_replacement_fusion(
        mut self,
        replacement_atom_ids: Vec<usize>,
        audit_checks: Vec<String>,
    ) -> Result<Self, String> {
        if !self.audit_ok()
            || replacement_atom_ids.is_empty()
            || audit_checks.is_empty()
            || replacement_atom_ids.iter().any(|a| !self.atoms.contains(a))
        {
            return Err("openclatura: skeletal replacement requires audited parent atoms".into());
        }
        self.kind = ParentHydrideKind::SkeletalReplacementFusion;
        self.skeletal_replacement_atom_ids = replacement_atom_ids;
        self.skeletal_replacement_audit_checks = audit_checks;
        Ok(self)
    }

    pub fn is_systematic_fusion(&self) -> bool {
        self.kind == ParentHydrideKind::SystematicFusion
    }
    pub fn is_skeletal_replacement_fusion(&self) -> bool {
        self.kind == ParentHydrideKind::SkeletalReplacementFusion
    }
    pub fn is_bridged_fusion(&self) -> bool {
        self.kind == ParentHydrideKind::BridgedFusion
    }
    pub fn is_fusion_parent(&self) -> bool {
        self.kind != ParentHydrideKind::Retained
    }
    pub fn uses_fusion_plan(&self) -> bool {
        self.fusion_plan.is_some()
            && matches!(
                self.kind,
                ParentHydrideKind::SystematicFusion | ParentHydrideKind::SkeletalReplacementFusion
            )
    }
    pub fn absorbs_skeletal_replacement(&self) -> bool {
        !self.is_skeletal_replacement_fusion()
    }

    pub fn audit_ok(&self) -> bool {
        if self.is_bridged_fusion() {
            return self
                .fusion_wrapper_plan
                .as_ref()
                .is_some_and(|p| p.audit_ok);
        }
        if self.is_systematic_fusion() {
            return self
                .fusion_plan
                .as_ref()
                .is_some_and(|p| p.audit.confirmed());
        }
        if self.is_skeletal_replacement_fusion()
            && (self.skeletal_replacement_atom_ids.is_empty()
                || self.skeletal_replacement_audit_checks.is_empty()
                || self
                    .skeletal_replacement_atom_ids
                    .iter()
                    .any(|a| !self.atoms.contains(a))
                || self
                    .replacement_fusion_state
                    .as_ref()
                    .is_some_and(|s| !s.audit_ok()))
        {
            return false;
        }
        self.retained_locant_maps.iter().all(|m| {
            m.keys().copied().collect::<AtomSet>() == self.atoms
                && m.values().collect::<BTreeSet<_>>().len() == self.atoms.len()
        })
    }

    pub fn base_name(&self) -> Option<&str> {
        if self.is_skeletal_replacement_fusion()
            && let Some(state) = &self.replacement_fusion_state
        {
            return Some(&state.rendered_name);
        }
        self.parent_name
            .as_deref()
            .or_else(|| {
                self.fusion_plan
                    .as_ref()
                    .map(|p| p.rendered_base_name.as_str())
            })
            .or_else(|| {
                self.fusion_wrapper_plan
                    .as_ref()
                    .map(|p| p.rendered_name.as_str())
            })
    }

    pub fn bond_model(&self) -> Option<&ParentBondModel> {
        self.replacement_fusion_state
            .as_ref()
            .map(|s| &s.bond_model)
            .or(self.parent_bond_model.as_ref())
            .or_else(|| {
                self.fusion_wrapper_plan
                    .as_ref()
                    .and_then(|p| p.parent.hydride.bond_model())
            })
            .or_else(|| self.fusion_plan.as_ref().map(|p| &p.bond_model))
    }

    pub fn derivative_state(&self) -> Option<&ParentDerivativeState> {
        self.replacement_fusion_state
            .as_ref()
            .map(|s| &s.derivative_state)
            .or_else(|| self.fusion_plan.as_ref().map(|p| &p.derivative_state))
            .or_else(|| {
                self.fusion_wrapper_plan
                    .as_ref()
                    .map(|p| &p.derivative_state)
            })
    }

    pub fn proof_locant_maps(&self) -> Vec<BTreeMap<usize, String>> {
        if let Some(plan) = &self.fusion_plan {
            return plan.numbering.string_input_locant_maps();
        }
        if let Some(plan) = &self.fusion_wrapper_plan {
            return vec![plan.atom_to_locant()];
        }
        self.retained_locant_maps.clone()
    }
}
