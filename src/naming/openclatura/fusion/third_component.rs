//! P-25.5 skeletal replacement after an audited cyclic component cover.
//!
//! The cyclic citation proves applicability only. The emitted parent is planned
//! independently on the corresponding carbon graph and its heteroatoms are
//! restored after topology, numbering, and (where applicable) hydrogen proof.
use super::super::{
    chains::canonical_ranks,
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
    polycycle_topology::edges_within_atoms,
    retained_fused_templates::{
        MatchOptions, match_retained_graph_templates_with_options, retained_parent_metadata,
    },
    ring_parent::RingParent,
};
use super::{
    config::fusion_nomenclature_config,
    cover::{audit_component_cover, component_scope},
    descriptor::{
        DescriptorOptions, build_fusion_name_ast, iter_fusion_name_asts, render_fusion_name,
    },
    faces::{cached_bounded_face_model, typed_face_model},
    layout::preferred_intrinsic_layouts,
    model::{FusionNameAst, FusionParentPlan},
    planner::{plan_fusion_parent, plan_numbered_candidate},
    registry::fusion_component_registry,
    replacement_state::{ReplacementFusionState, prove_replacement_state, replacement_state_scope},
    rules::fusion_ring_size_gate,
};
use std::collections::{BTreeMap, BTreeSet};

pub const THIRD_COMPONENT_TIER: &str = "p25.5-skeletal-replacement-v1";

/// Audited skeletal-replacement resolution of a cyclic component cover.
#[derive(Clone, Debug)]
pub struct ThirdComponentFusionPlan {
    pub parent: RingParent,
    pub prohibited_citation: FusionNameAst,
    pub replacement_atom_ids: Vec<usize>,
    pub cover_topology: String,
    pub ring_sizes: Vec<usize>,
    pub audit_checks: Vec<String>,
}

impl ThirdComponentFusionPlan {
    pub fn audit_ok(&self) -> bool {
        self.parent.audit_ok()
            && self.parent.is_skeletal_replacement_fusion()
            && self.prohibited_citation.plan_kind == "cyclic_component_cover"
            && self
                .prohibited_citation
                .citation_plan
                .as_ref()
                .is_some_and(|p| !p.cycle_closing_join_indices.is_empty())
            && matches!(self.cover_topology.as_str(), "unicyclic" | "cactus")
            && self.ring_sizes.windows(2).all(|sizes| sizes[0] <= sizes[1])
            && fusion_ring_size_gate(&self.ring_sizes)
            && !self.replacement_atom_ids.is_empty()
            && self.replacement_atom_ids == self.parent.skeletal_replacement_atom_ids
            && !self.audit_checks.is_empty()
            && self.audit_checks == self.parent.skeletal_replacement_audit_checks
    }
}

/// Return a P-25.5.1 parent when ordinary pairwise fusion is impossible.
pub fn plan_third_component_fusion_parent(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<ThirdComponentFusionPlan>, String> {
    if let Some(cached) = mol.third_component_plan_cache.borrow().get(atoms) {
        return cached.clone();
    }
    let result = plan_uncached(mol, atoms);
    mol.third_component_plan_cache
        .borrow_mut()
        .insert(atoms.clone(), result.clone());
    result
}

fn plan_uncached(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<ThirdComponentFusionPlan>, String> {
    let replacements: Vec<_> = atoms
        .iter()
        .copied()
        .filter(|a| !mol.atoms[a].is_carbon())
        .collect();
    if replacements.is_empty()
        || atoms.iter().any(|a| mol.atoms[a].charge != 0)
        || replacements
            .iter()
            .any(|a| mol.atoms[a].element().is_none_or(|e| e.hw_stem.is_none()))
    {
        return Ok(None);
    }
    let Some((prohibited_citation, cover_topology, ring_sizes)) =
        prohibited_pairwise_citation(mol, atoms)?
    else {
        return Ok(None);
    };
    if !fusion_ring_size_gate(&ring_sizes) {
        return Ok(None);
    }
    let carbon = carbon_skeleton(mol, atoms);
    if !corresponding_carbon_graph_is_exact(mol, &carbon, atoms, &replacements) {
        return Ok(None);
    }
    let Some(mut parent) = carbon_parent(&carbon, atoms)? else {
        return Ok(None);
    };
    if !parent.audit_ok() {
        return Ok(None);
    }
    let mut replacement_state = None;
    if parent.fusion_plan.is_some() && replacement_state_scope(mol, atoms) {
        let Some((selected, state)) = select_replacement_state(mol, &carbon, atoms, &parent)?
        else {
            return Ok(None);
        };
        parent = selected;
        replacement_state = Some(state);
    }
    let maps = parent.proof_locant_maps();
    if maps.is_empty()
        || maps
            .iter()
            .any(|map| map.keys().copied().collect::<AtomSet>() != *atoms)
        || maps
            .iter()
            .any(|map| replacements.iter().any(|a| !map.contains_key(a)))
    {
        return Ok(None);
    }
    let mut audit_checks: Vec<_> = [
        "cyclic_component_cover_proved",
        "ordinary_pairwise_citation_not_emitted",
        "corresponding_carbon_graph_identity",
        "carbon_parent_audit",
        "complete_replacement_locants",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if let Some(state) = &replacement_state {
        audit_checks.extend(state.audit_checks.iter().cloned());
    }
    let mut parent =
        parent.into_skeletal_replacement_fusion(replacements.clone(), audit_checks.clone())?;
    if let Some(state) = replacement_state {
        let metadata = parent.hydride_metadata.get_or_insert_with(Default::default);
        metadata.default_indicated_h = state.indicated_hydrogens.clone();
        metadata.indicated_hydrogen_count = state.indicated_hydrogens.len();
        metadata.mancude_double_bonds = state.bond_model.maximum_non_cumulative_double_bonds;
        parent.replacement_fusion_state = Some(state);
    }
    let result = ThirdComponentFusionPlan {
        parent,
        prohibited_citation,
        replacement_atom_ids: replacements,
        cover_topology,
        ring_sizes,
        audit_checks,
    };
    // Upstream divergence: validate the complete Rust handoff against its
    // retained applicability certificate before putting it in the plan cache.
    if !result.audit_ok() {
        return Err(format!(
            "openclatura: {THIRD_COMPONENT_TIER} plan audit failed"
        ));
    }
    Ok(Some(result))
}

type ReplacementRank = (
    Vec<Vec<(usize, u8, String, usize)>>,
    Vec<(usize, u8, String, usize)>,
    Vec<usize>,
);

/// Equivalent carbon constructions are selected only after replacement proof.
fn select_replacement_state(
    mol: &Molecule,
    carbon: &Molecule,
    atoms: &AtomSet,
    first_parent: &RingParent,
) -> Result<Option<(RingParent, ReplacementFusionState)>, String> {
    let Some(first_plan) = first_parent.fusion_plan.as_ref() else {
        return Ok(None);
    };
    let Some(bounded) = cached_bounded_face_model(carbon, atoms)? else {
        return Ok(None);
    };
    let registry = fusion_component_registry();
    let faces = typed_face_model(carbon, &bounded);
    let matches = registry.match_face_model(carbon, &faces)?;
    let mut layouts = Some(preferred_intrinsic_layouts(
        &faces,
        fusion_nomenclature_config().search.layout_states,
    )?);
    let mut numbering_cache = BTreeMap::new();
    let first_core = carbon_plan_core(first_plan)?;
    let ranks = canonical_ranks(mol, None);
    let priorities: BTreeSet<_> = atoms
        .iter()
        .filter(|a| !mol.atoms[a].is_carbon())
        .filter_map(|a| mol.atoms[a].element().and_then(|e| e.hw_priority))
        .collect();
    let mut selected: Option<(ReplacementRank, RingParent, ReplacementFusionState)> = None;
    for candidate in
        iter_fusion_name_asts(carbon, &matches, registry, &DescriptorOptions::default())?
    {
        let ast = candidate?;
        // Different embeddings of the preferred construction need joint
        // proofs. Do not substitute a lower-priority carbon parent.
        if render_fusion_name(&ast, registry)? != first_core {
            break;
        }
        let candidate_plan = match plan_numbered_candidate(
            carbon,
            atoms,
            &ast,
            registry,
            &bounded,
            &faces,
            &mut layouts,
            &mut numbering_cache,
        ) {
            Ok(Some(plan)) => plan,
            Ok(None) => continue,
            // A rejected numbered candidate is upstream FusionUnsupported or
            // FusionAuditFailed. Explicit port limitations remain actionable.
            Err(error) if error.starts_with("openclatura:") && error.contains("not ported") => {
                return Err(error);
            }
            Err(_) => continue,
        };
        let variants = if candidate_plan.numbering_variants.is_empty() {
            std::slice::from_ref(&candidate_plan)
        } else {
            candidate_plan.numbering_variants.as_slice()
        };
        for plan in variants {
            let Some(state) = prove_replacement_state(mol, plan)? else {
                continue;
            };
            if !state.audit_ok() {
                continue;
            }
            let Some(mapping) = plan.numbering.string_input_locant_maps().into_iter().next() else {
                continue;
            };
            let mut ordered: Vec<_> = atoms.iter().copied().collect();
            ordered.sort_by_key(|a| locant_key(&mapping[a]));
            let rank = (
                priorities
                    .iter()
                    .map(|&priority| {
                        let mut locants: Vec<_> = atoms
                            .iter()
                            .filter(|a| {
                                !mol.atoms[a].is_carbon()
                                    && mol.atoms[a].element().and_then(|e| e.hw_priority)
                                        == Some(priority)
                            })
                            .map(|a| locant_key(&mapping[a]))
                            .collect();
                        locants.sort();
                        locants
                    })
                    .collect(),
                state
                    .indicated_hydrogens
                    .iter()
                    .map(|l| locant_key(l))
                    .collect(),
                ordered.iter().map(|a| ranks[a]).collect(),
            );
            if selected.as_ref().is_none_or(|best| rank < best.0) {
                selected = Some((rank, RingParent::from_fusion_plan(plan.clone())?, state));
            }
        }
    }
    Ok(selected.map(|(_, parent, state)| (parent, state)))
}

/// The plain-name plan retains intrinsic H separately from its carbon core.
fn carbon_plan_core(plan: &FusionParentPlan) -> Result<String, String> {
    if plan.indicated_hydrogens.is_empty() {
        return Ok(plan.rendered_base_name.clone());
    }
    let prefix = format!(
        "{}-",
        plan.indicated_hydrogens
            .iter()
            .map(|l| format!("{l}H"))
            .collect::<Vec<_>>()
            .join(",")
    );
    plan.rendered_base_name
        .strip_prefix(&prefix)
        .map(str::to_owned)
        .ok_or_else(|| "carbon parent indicated-hydrogen prefix disagrees with its plan".into())
}

fn corresponding_carbon_graph_is_exact(
    original: &Molecule,
    carbon: &Molecule,
    atoms: &AtomSet,
    replacements: &[usize],
) -> bool {
    let replacement: AtomSet = replacements.iter().copied().collect();
    if replacement
        != atoms
            .iter()
            .copied()
            .filter(|a| !original.atoms[a].is_carbon())
            .collect()
        || !carbon.atoms.keys().eq(original.atoms.keys())
        || !carbon.bonds.keys().eq(original.bonds.keys())
    {
        return false;
    }
    for &id in atoms {
        let c = &carbon.atoms[&id];
        let original_atom = &original.atoms[&id];
        if !c.is_carbon() || c.charge != 0 {
            return false;
        }
        let valence: usize = original
            .get_neighbors(id)
            .iter()
            .filter_map(|&n| original.get_bond(id, n))
            .map(|b| b.order)
            .sum();
        if c.explicit_h_count != 0 || c.total_h_count != 4usize.saturating_sub(valence) {
            return false;
        }
        let restored = if replacement.contains(&id) {
            &original_atom.symbol
        } else {
            &c.symbol
        };
        if restored != &original_atom.symbol {
            return false;
        }
    }
    original.bonds.iter().all(|(id, b)| {
        let c = &carbon.bonds[id];
        (c.u, c.v, c.order) == (b.u, b.v, b.order)
    })
}

fn prohibited_pairwise_citation(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<(FusionNameAst, String, Vec<usize>)>, String> {
    let bounded = match cached_bounded_face_model(mol, atoms) {
        Ok(Some(bounded)) => bounded,
        Ok(None) => return Ok(None),
        Err(error) if error.contains("search exceeded its budget of") => return Ok(None),
        Err(error) => return Err(error),
    };
    let registry = fusion_component_registry();
    let matches = registry.match_face_model(mol, &typed_face_model(mol, &bounded))?;
    let options = DescriptorOptions {
        cover_kinds: BTreeSet::from(["multiparent".into()]),
        join_kinds: ["ortho", "ortho_peri", "higher_order"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        // This AST is an applicability certificate and is never rendered.
        // Exact cover and interface budgets still apply to its search.
        enforce_interoperability_limits: false,
        ..Default::default()
    };
    let Ok(ast) = build_fusion_name_ast(mol, &matches, registry, &options) else {
        return Ok(None);
    };
    if ast.plan_kind != "cyclic_component_cover"
        || ast
            .citation_plan
            .as_ref()
            .is_none_or(|p| p.cycle_closing_join_indices.is_empty())
    {
        return Ok(None);
    }
    let scopes = ast
        .component_occurrences
        .iter()
        .map(|matched| {
            let map = matched.input_atom_by_locant();
            let spec = registry.spec_for_match(matched)?;
            component_scope(
                matched.occurrence_id,
                map.values().copied(),
                spec.bonds()
                    .iter()
                    .map(|b| (map[&b.locants.0], map[&b.locants.1])),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let cover = audit_component_cover(&scopes, atoms, &edges_within_atoms(mol, atoms))?;
    if !cover.ok || !matches!(cover.proof.topology.as_str(), "unicyclic" | "cactus") {
        return Ok(None);
    }
    let mut ring_sizes: Vec<_> = bounded.faces.iter().map(|f| f.atoms.len()).collect();
    ring_sizes.sort();
    Ok(Some((ast, cover.proof.topology, ring_sizes)))
}

fn carbon_skeleton(mol: &Molecule, atoms: &AtomSet) -> Molecule {
    let mut carbon = mol.clone();
    for &id in atoms {
        let hydrogens = 4usize.saturating_sub(
            mol.get_neighbors(id)
                .iter()
                .filter_map(|&n| mol.get_bond(id, n))
                .map(|b| b.order)
                .sum(),
        );
        if let Some(atom) = carbon.atoms.get_mut(&id) {
            atom.symbol = "C".into();
            atom.charge = 0;
            atom.isotope = None;
            atom.explicit_h_count = 0;
            atom.total_h_count = hydrogens;
        }
    }
    carbon
}

fn carbon_parent(carbon: &Molecule, atoms: &AtomSet) -> Result<Option<RingParent>, String> {
    let mut matches = match_retained_graph_templates_with_options(
        carbon,
        atoms,
        MatchOptions {
            family: Some("fused"),
            ..Default::default()
        },
    );
    if matches.is_empty() {
        matches = match_retained_graph_templates_with_options(
            carbon,
            atoms,
            MatchOptions {
                family: Some("fused"),
                allow_nonaromatic: true,
                pre_descriptor_only: true,
                ..Default::default()
            },
        );
    }
    if let Some(first) = matches.first() {
        let name = first.template.name.clone();
        let maps = matches
            .into_iter()
            .filter(|m| m.template.name == name)
            .map(|m| m.atom_to_locant)
            .collect();
        return RingParent::from_retained_locant_maps(
            atoms.clone(),
            maps,
            name.clone(),
            retained_parent_metadata(&name),
        )
        .map(Some);
    }
    let Some(plan) = plan_fusion_parent(carbon, atoms)? else {
        return Ok(None);
    };
    RingParent::from_fusion_plan(plan).map(Some)
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::*;

    #[test]
    fn carbonization_preserves_graph_and_adjusts_sigma_donor_hydrogens() {
        let mol = read_smiles("C1[C@@H]2C[C@@H]3C[C@H]1N[C@H](C2)S3").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let replacements: Vec<_> = mol
            .atoms
            .values()
            .filter(|a| !a.is_carbon())
            .map(|a| a.idx)
            .collect();
        let carbon = carbon_skeleton(&mol, &atoms);
        assert!(corresponding_carbon_graph_is_exact(
            &mol,
            &carbon,
            &atoms,
            &replacements
        ));
        for &id in &replacements {
            assert_eq!(carbon.atoms[&id].total_h_count, 2);
            assert!(carbon.atoms[&id].is_carbon());
        }
        let mut incorrect_hydrogens = carbon.clone();
        incorrect_hydrogens
            .atoms
            .get_mut(&replacements[0])
            .unwrap()
            .total_h_count = 1;
        assert!(!corresponding_carbon_graph_is_exact(
            &mol,
            &incorrect_hydrogens,
            &atoms,
            &replacements
        ));
        let mut incorrect_bond = carbon.clone();
        let bond_id = *incorrect_bond.bonds.keys().next().unwrap();
        incorrect_bond.set_bond_order(bond_id, 2).unwrap();
        assert!(!corresponding_carbon_graph_is_exact(
            &mol,
            &incorrect_bond,
            &atoms,
            &replacements
        ));
    }

    #[test]
    fn cyclic_certificate_is_never_an_emitted_fusion_citation() {
        for smiles in [
            "C1[C@@H]2C[C@@H]3C[C@H]1N[C@H](C2)S3",
            "N1=C2C3=C(N=NC3=N1)N=N2",
            "N1=C2C3=C(N=NC3=N1)C=C2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let (ast, topology, sizes) =
                prohibited_pairwise_citation(&mol, &atoms).unwrap().unwrap();
            assert_eq!(ast.plan_kind, "cyclic_component_cover", "{smiles}");
            assert!(
                !ast.citation_plan
                    .unwrap()
                    .cycle_closing_join_indices
                    .is_empty()
            );
            assert!(matches!(topology.as_str(), "unicyclic" | "cactus"));
            assert!(fusion_ring_size_gate(&sizes));
            assert!(sizes.windows(2).all(|w| w[0] <= w[1]));
        }
    }

    #[test]
    fn heteroadamantane_restores_heteroatoms_on_the_retained_carbon_parent() {
        let mol = read_smiles("C1[C@@H]2C[C@@H]3C[C@H]1N[C@H](C2)S3").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let plan = plan_third_component_fusion_parent(&mol, &atoms)
            .unwrap()
            .unwrap();
        assert_eq!(plan.parent.base_name(), Some("adamantane"));
        assert!(plan.parent.is_skeletal_replacement_fusion());
        assert!(plan.parent.audit_ok());
        assert_eq!(plan.replacement_atom_ids, vec![6, 9]);
        assert_eq!(plan.parent.proof_locant_maps().len(), 24);
        assert!(plan.parent.replacement_fusion_state.is_none());
        assert_eq!(plan.audit_checks.len(), 5);
    }

    #[test]
    fn cyclic_aza_covers_preserve_the_carbon_pi_decline() {
        for smiles in [
            "N1=C2C3=C(N=NC3=N1)N=N2",
            "N1=C2C3=C(N=NC3=N1)C=C2",
            "CC1=CC=2C3=C1N=NC3=NN2",
            "FC1=CC=2C3=C1N=NC3=NN2",
        ] {
            assert_eq!(
                super::super::super::name_smiles(smiles).unwrap_err(),
                "openclatura: localized nonaromatic fused pi state not ported"
            );
        }
    }

    #[test]
    fn noncyclic_or_carbon_parents_do_not_enter_skeletal_replacement() {
        for smiles in [
            "C1CC2CCC1C2",
            "C1C2CC3CC1CC(C2)C3",
            "C1OCC2COCC12",
            "C1=CCSC2SC=NC2=C1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert!(
                plan_third_component_fusion_parent(&mol, &atoms)
                    .unwrap()
                    .is_none(),
                "{smiles}"
            );
        }
    }
}
