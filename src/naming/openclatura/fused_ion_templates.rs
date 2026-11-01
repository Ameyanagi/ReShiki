//! Graph-bound retained fused ion operations from the upstream spelling registry.
use super::{assembly_parts::AssemblyParts, molecule::AtomSet};
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
struct FusedIonTemplate {
    base_parent_name: String,
    #[serde(default)]
    base_parent_aliases: Vec<String>,
    allowed_charge_operation: String,
    #[serde(default)]
    allowed_locants: Vec<String>,
    suffix_or_prefix_form: String,
    opsin_grammar_name: String,
    #[serde(default)]
    production_ready: bool,
}

#[derive(Debug, Deserialize)]
struct FusedIonTemplateRegistry {
    templates: Vec<FusedIonTemplate>,
}
impl FusedIonTemplateRegistry {
    fn for_operation(
        &self,
        parent_name: &str,
        operation: &str,
        locants: &[String],
    ) -> Option<&FusedIonTemplate> {
        self.templates.iter().find(|template| {
            (template.base_parent_name == parent_name
                || template
                    .base_parent_aliases
                    .iter()
                    .any(|alias| alias == parent_name))
                && template.allowed_charge_operation == operation
                && template.allowed_locants == locants
        })
    }
}

#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in fused ion template table"
)]
static REGISTRY: LazyLock<FusedIonTemplateRegistry> = LazyLock::new(|| {
    let registry: FusedIonTemplateRegistry =
        serde_json::from_str(include_str!("data/fused_ion_templates.json"))
            .expect("fused ion template data");
    let snapshot: serde_json::Value =
        serde_json::from_str(include_str!("data/parser_grammar_snapshot.json"))
            .expect("parser grammar snapshot");
    let canonical = snapshot["charge_suffixes"]["canonical"]
        .as_array()
        .expect("canonical charge suffix vocabulary");
    assert!(
        registry.templates.iter().all(|template| {
            template.suffix_or_prefix_form == "oxide"
                || canonical
                    .iter()
                    .any(|suffix| suffix.as_str() == Some(&template.suffix_or_prefix_form))
        }),
        "fused ion template uses unsupported charge suffix vocabulary"
    );
    registry
});

#[derive(Clone, Debug)]
pub struct FusedIonOperationCandidate {
    locants: Vec<String>,
    represented_atom_ids: AtomSet,
    template: Option<&'static FusedIonTemplate>,
    consumed_substituent_names: Vec<String>,
}
impl FusedIonOperationCandidate {
    fn production_ready(&self) -> bool {
        self.template
            .is_some_and(|template| template.production_ready)
    }
    pub fn rendered_name(&self) -> Option<&str> {
        self.template
            .filter(|template| template.production_ready)
            .map(|template| template.opsin_grammar_name.as_str())
    }
}

pub fn select_fused_ion_operation(parts: &AssemblyParts) -> Option<FusedIonOperationCandidate> {
    fused_ion_operation_candidates(parts).find(FusedIonOperationCandidate::production_ready)
}

pub fn consume_fused_ion_operation(
    parts: &mut AssemblyParts,
    candidate: &FusedIonOperationCandidate,
) {
    if !candidate.production_ready() {
        return;
    }
    parts.substituents.retain(|item| {
        !(candidate.consumed_substituent_names.contains(&item.name)
            && item.locants == candidate.locants
            && item.atom_ids.is_subset(&candidate.represented_atom_ids))
    });
    parts
        .parent_charges
        .retain(|charge| !candidate.locants.contains(&charge.locant));
}

fn fused_ion_operation_candidates(
    parts: &AssemblyParts,
) -> impl Iterator<Item = FusedIonOperationCandidate> {
    [
        retained_fused_n_oxide_operation(parts),
        retained_fused_ring_n_charge_operation(parts),
    ]
    .into_iter()
    .flatten()
}

fn eligible_parent(parts: &AssemblyParts) -> Option<&str> {
    if parts.is_substituent
        || parts.principal_group.is_some()
        || !parts.a_prefixes.is_empty()
        || !parts.front_modifiers.is_empty()
        || !parts.unsaturations.is_empty()
        || !parts.hydro_operations.is_empty()
    {
        return None;
    }
    parts.retained_name.as_deref()
}

fn retained_fused_ring_n_charge_operation(
    parts: &AssemblyParts,
) -> Option<FusedIonOperationCandidate> {
    let parent = eligible_parent(parts)?;
    let [charge] = parts.parent_charges.as_slice() else {
        return None;
    };
    let locant = &charge.locant;
    if charge.symbol != "N"
        || parts
            .parent_atom_symbols_by_locant
            .get(locant)
            .map(String::as_str)
            != Some("N")
        || parts.parent_atom_charges_by_locant.get(locant) != Some(&charge.charge)
    {
        return None;
    }
    let operation = match charge.charge.cmp(&0) {
        std::cmp::Ordering::Greater => "ring_n_ium",
        std::cmp::Ordering::Less => "ring_n_deprotonation",
        std::cmp::Ordering::Equal => return None,
    };
    let locants = vec![locant.clone()];
    Some(FusedIonOperationCandidate {
        template: REGISTRY.for_operation(parent, operation, &locants),
        locants,
        represented_atom_ids: charge.atom_id.into_iter().collect(),
        consumed_substituent_names: Vec::new(),
    })
}

fn retained_fused_n_oxide_operation(parts: &AssemblyParts) -> Option<FusedIonOperationCandidate> {
    let parent = eligible_parent(parts)?;
    if !parts.indicated_hydrogens.is_empty() {
        return None;
    }
    let mut oxido = parts
        .substituents
        .iter()
        .filter(|item| item.name == "oxido" && item.locants.len() == 1);
    let item = oxido.next()?;
    if oxido.next().is_some() {
        return None;
    }
    let locant = &item.locants[0];
    let [charge] = parts.parent_charges.as_slice() else {
        return None;
    };
    if charge.symbol != "N"
        || charge.charge <= 0
        || &charge.locant != locant
        || parts
            .parent_atom_symbols_by_locant
            .get(locant)
            .map(String::as_str)
            != Some("N")
        || parts.parent_atom_charges_by_locant.get(locant) != Some(&1)
    {
        return None;
    }
    let mut atoms = item.atom_ids.clone();
    atoms.extend(charge.atom_id);
    Some(FusedIonOperationCandidate {
        template: REGISTRY.for_operation(parent, "ring_n_oxide", &item.locants),
        locants: item.locants.clone(),
        represented_atom_ids: atoms,
        consumed_substituent_names: vec!["oxido".into()],
    })
}

#[cfg(test)]
mod tests {
    use super::super::assembly_parts::{ParentChargeItem, SubstituentItem};
    use super::*;

    #[test]
    fn n_oxide_consumes_only_the_graph_bound_oxide_and_charge() {
        let mut parts = AssemblyParts::new(10);
        parts.retained_name = Some("quinoline".into());
        parts.parent_atom_ids_by_locant.insert("1".into(), 7);
        parts
            .parent_atom_symbols_by_locant
            .insert("1".into(), "N".into());
        parts.parent_atom_charges_by_locant.insert("1".into(), 1);
        parts.parent_charges.push(ParentChargeItem {
            locant: "1".into(),
            symbol: "N".into(),
            charge: 1,
            atom_id: Some(7),
        });
        parts.substituents.push(SubstituentItem {
            name: "oxido".into(),
            locants: vec!["1".into()],
            atom_ids: AtomSet::from([8]),
            bond_ids: AtomSet::from([9]),
            ..Default::default()
        });
        parts
            .substituents
            .push(SubstituentItem::new("methyl", vec!["3".into()]));
        let candidate = select_fused_ion_operation(&parts).expect("audited n-oxide template");
        assert_eq!(candidate.rendered_name(), Some("quinoline 1-oxide"));
        assert_eq!(candidate.represented_atom_ids, AtomSet::from([7, 8]));
        consume_fused_ion_operation(&mut parts, &candidate);
        assert!(parts.parent_charges.is_empty());
        assert_eq!(parts.substituents.len(), 1);
        assert_eq!(parts.substituents[0].name, "methyl");
    }

    #[test]
    fn unproved_ion_rows_and_wrong_numbering_do_not_render() {
        let mut parts = AssemblyParts::new(9);
        parts.retained_name = Some("1H-indole".into());
        parts
            .parent_atom_symbols_by_locant
            .insert("1".into(), "N".into());
        parts.parent_atom_charges_by_locant.insert("1".into(), -1);
        parts.parent_charges.push(ParentChargeItem {
            locant: "1".into(),
            symbol: "N".into(),
            charge: -1,
            ..Default::default()
        });
        assert_eq!(fused_ion_operation_candidates(&parts).count(), 1);
        assert!(select_fused_ion_operation(&parts).is_none());
        parts.retained_name = Some("isoquinoline".into());
        parts.parent_charges[0].charge = 1;
        parts.parent_atom_charges_by_locant.insert("1".into(), 1);
        assert!(select_fused_ion_operation(&parts).is_none());
    }
}
