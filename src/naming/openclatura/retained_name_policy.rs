// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/retained_name_policy.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/retained_name_policy.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Adapted openclatura retained-parent spelling/hydrogenation policies with local serde/debug glue and regression tests.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Preferred retained spellings and explicitly declared hydrogenation policies.
use super::{formatting::format_multiplier, locants::retained_locant_sort_key as parse_locant};
use serde::{Deserialize, de::IgnoredAny};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedHydrogenationPolicy {
    pub base_parent: String,
    pub hydro_locants: Vec<String>,
    pub indicated_hydrogen_locants: Vec<String>,
}
// Provenance: impl RetainedHydrogenationPolicy | adapted
// Source: openclatura | src/openclatura/retained_name_policy.py::RetainedHydrogenationPolicy.relocated
// Source: openclatura | src/openclatura/retained_name_policy.py::render
// Source: openclatura | src/openclatura/retained_name_policy.py::RetainedHydrogenationPolicy.render
impl RetainedHydrogenationPolicy {
    pub fn relocated(&self, suffix_locants: &BTreeSet<String>) -> Self {
        let mut moved: Vec<_> = self
            .hydro_locants
            .iter()
            .filter(|l| suffix_locants.contains(*l))
            .cloned()
            .collect();
        if moved.is_empty() || moved.len() != self.indicated_hydrogen_locants.len() {
            return self.clone();
        }
        let mut hydro: Vec<_> = self
            .hydro_locants
            .iter()
            .chain(&self.indicated_hydrogen_locants)
            .filter(|l| !moved.contains(l))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        hydro.sort_by_key(|l| parse_locant(l));
        moved.sort_by_key(|l| parse_locant(l));
        Self {
            base_parent: self.base_parent.clone(),
            hydro_locants: hydro,
            indicated_hydrogen_locants: moved,
        }
    }
    pub(super) fn parent_name(&self) -> String {
        let mut parent = self.base_parent.as_str();
        if let Some((prefix, rest)) = parent.split_once('-')
            && prefix
                .split(',')
                .all(|l| l.ends_with('H') && l.chars().next().is_some_and(|c| c.is_ascii_digit()))
        {
            parent = rest;
        }
        if self.indicated_hydrogen_locants.is_empty() {
            parent.to_owned()
        } else {
            format!(
                "{}-{parent}",
                self.indicated_hydrogen_locants
                    .iter()
                    .map(|l| format!("{l}H"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
    pub(super) fn render_with_parent(&self, parent: &str) -> String {
        format!(
            "{}-{}{}{parent}",
            self.hydro_locants.join(","),
            format_multiplier("hydro", self.hydro_locants.len(), false),
            if parent.starts_with(|c: char| c.is_ascii_digit()) {
                "-"
            } else {
                ""
            },
        )
    }
    pub fn render(&self) -> String {
        self.render_with_parent(&self.parent_name())
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedParentNamePolicy {
    pub template_name: String,
    pub preferred_name: String,
    #[serde(default = "all_contexts")]
    pub preferred_contexts: Vec<String>,
    #[serde(default)]
    pub hydrogenation: Option<RetainedHydrogenationPolicy>,
    #[serde(default)]
    pub context_names: BTreeMap<String, String>,
    #[serde(rename = "reason")]
    _reason: IgnoredAny,
}

// Provenance: impl <RetainedParentNamePolicy as std::fmt::Debug> | original
// Source: ReShiki | src/naming/openclatura/retained_name_policy.rs::<RetainedParentNamePolicy as std::fmt::Debug>
impl std::fmt::Debug for RetainedParentNamePolicy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetainedParentNamePolicy")
            .field("template_name", &self.template_name)
            .field("preferred_name", &self.preferred_name)
            .field("preferred_contexts", &self.preferred_contexts)
            .field("hydrogenation", &self.hydrogenation)
            .field("context_names", &self.context_names)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedParentNamePolicyData {
    #[serde(rename = "bluebook_rule")]
    _bluebook_rule: IgnoredAny,
    parents: Vec<RetainedParentNamePolicy>,
}
// Provenance: fn all_contexts | adapted
// Source: openclatura | src/openclatura/retained_name_policy.py::RetainedParentNamePolicy.preferred_contexts
// Source: openclatura | src/openclatura/retained_name_policy.py::_policy_from_data
fn all_contexts() -> Vec<String> {
    vec!["all".into()]
}
impl RetainedParentNamePolicy {
    // Provenance: fn RetainedParentNamePolicy::output_name | adapted
    // Source: openclatura | src/openclatura/retained_name_policy.py::RetainedParentNamePolicy.output_name
    pub fn output_name(&self, context: &str) -> String {
        self.context_names.get(context).cloned().unwrap_or_else(|| {
            if self
                .preferred_contexts
                .iter()
                .any(|c| c == "all" || c == context)
            {
                self.preferred_name.clone()
            } else {
                self.template_name.clone()
            }
        })
    }
}
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained policy table"
)]
static POLICIES: LazyLock<Vec<RetainedParentNamePolicy>> = LazyLock::new(|| {
    let data: RetainedParentNamePolicyData =
        serde_json::from_str(include_str!("data/retained_parent_name_policy.json"))
            .expect("checked-in retained name policy data");
    let mut rows = data.parents;
    for row in &mut rows {
        if let Some(h) = &row.hydrogenation {
            row.preferred_name = h.render();
        }
    }
    rows
});
// Provenance: fn retained_parent_name_policies | adapted
// Source: openclatura | src/openclatura/retained_name_policy.py::retained_parent_name_policies
pub fn retained_parent_name_policies() -> &'static [RetainedParentNamePolicy] {
    &POLICIES
}
// Provenance: fn retained_parent_name_policy | adapted
// Source: openclatura | src/openclatura/retained_name_policy.py::RetainedParentNamePolicy
// Source: openclatura | src/openclatura/retained_name_policy.py::retained_parent_name_policy
pub fn retained_parent_name_policy(name: &str) -> Option<&'static RetainedParentNamePolicy> {
    POLICIES.iter().find(|p| p.template_name == name)
}
// Provenance: fn retained_parent_output_name | adapted
// Source: openclatura | src/openclatura/retained_name_policy.py::retained_parent_output_name
pub fn retained_parent_output_name(
    name: &str,
    context: &str,
    default_indicated_h: &[String],
    indicated_h: Option<&[String]>,
) -> String {
    let name =
        retained_parent_name_policy(name).map_or_else(|| name.into(), |p| p.output_name(context));
    render_retained_hydrogen_state(&name, default_indicated_h, indicated_h)
}
// Provenance: fn render_retained_hydrogen_state | adapted
// Source: openclatura | src/openclatura/retained_name_policy.py::render_retained_hydrogen_state
pub fn render_retained_hydrogen_state(
    name: &str,
    default_indicated_h: &[String],
    indicated_h: Option<&[String]>,
) -> String {
    let Some(h) = indicated_h.filter(|h| *h != default_indicated_h) else {
        return name.into();
    };
    let parent = if super::additive::name_indicated_hydrogen_locants(Some(name)).is_empty() {
        name
    } else {
        name.split_once('-').map_or(name, |(_, parent)| parent)
    };
    if h.is_empty() {
        parent.into()
    } else {
        format!(
            "{}-{parent}",
            h.iter()
                .map(|l| format!("{l}H"))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/retained_name_policy.rs::tests
mod tests {
    use crate::naming::verify_identity;
    use nazuchem_ir::{NameReason, NameStatus};

    #[test]
    // Provenance: fn tests::selected_hydrogen_replaces_the_source_parent_descriptor | adapted
    // Source: ReShiki | src/naming/openclatura/retained_name_policy.rs::tests::selected_hydrogen_replaces_the_source_parent_descriptor
    // Source: openclatura | tests/unit/test_analysis.py::test_stage10_fused_ion_template_registry_is_data_backed_and_gated
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn selected_hydrogen_replaces_the_source_parent_descriptor() {
        assert_eq!(
            super::render_retained_hydrogen_state("1H-indole", &[], Some(&["2".into()])),
            "2H-indole"
        );
        assert_eq!(
            super::render_retained_hydrogen_state("1H-indole", &["1".into()], Some(&[])),
            "indole"
        );
        assert_eq!(
            super::render_retained_hydrogen_state("1H-indole", &[], Some(&[])),
            "1H-indole"
        );
        assert_eq!(
            super::render_retained_hydrogen_state("1H-indole", &[], None),
            "1H-indole"
        );
    }

    #[test]
    // Provenance: fn tests::benzoheterocycle_locants_survive_substitution_fusion_and_spiro | adapted
    // Source: ReShiki | src/naming/openclatura/retained_name_policy.rs::tests::benzoheterocycle_locants_survive_substitution_fusion_and_spiro
    // Source: openclatura | tests/unit/test_analysis.py::test_stage3_retained_fused_pin_forms_and_indicated_hydrogens_are_validated
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn benzoheterocycle_locants_survive_substitution_fusion_and_spiro() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected, status) in [
            // The spelling policy must not fix H1: oxo reconstruction and
            // strict retained matches derive the hydrogen state from the graph.
            (
                "O=C1Nc2ccccc2N1",
                "1,3-dihydro-2H-1,3-benzimidazol-2-one",
                NameStatus::Preferred,
            ),
            (
                "O=C1N(C)c2ccccc2N1C",
                "1,3-dimethyl-1,3-dihydro-2H-1,3-benzimidazol-2-one",
                NameStatus::Preferred,
            ),
            (
                "c1ccc2n[nH]nc2c1",
                "2H-1,2,3-benzotriazole",
                NameStatus::Preferred,
            ),
            (
                "Cn1nc2ccccc2n1",
                "2-methyl-2H-1,2,3-benzotriazole",
                NameStatus::Preferred,
            ),
            (
                "O=C(O)Cn1nc2ccccc2n1",
                "(2H-1,2,3-benzotriazol-2-yl)acetic acid",
                NameStatus::Preferred,
            ),
            (
                "C1N=C2C=CC=CC2=N1",
                "2H-1,3-benzimidazole",
                NameStatus::Preferred,
            ),
            (
                "c1ccc2[nH]cnc2c1",
                "1H-1,3-benzimidazole",
                NameStatus::Preferred,
            ),
            (
                "c1ccc2[nH]nnc2c1",
                "1H-1,2,3-benzotriazole",
                NameStatus::Preferred,
            ),
            (
                "Cn1cnc2ccccc21",
                "1-methyl-1H-1,3-benzimidazole",
                NameStatus::Preferred,
            ),
            (
                "O=C(O)Cc1nc2ccccc2[nH]1",
                "(1H-1,3-benzimidazol-2-yl)acetic acid",
                NameStatus::Preferred,
            ),
            (
                "O=C(O)Cn1nnc2ccccc21",
                "1H-1,2,3-benzotriazol-1-ylacetic acid",
                NameStatus::Preferred,
            ),
            // Sulfonic acid uses the retained fallback outside the production
            // gate and must keep the source parent's explicit 1H citation.
            (
                "O=S(=O)(O)c1ccc2[nH]nnc2c1",
                "1H-1,2,3-benzotriazole-5-sulfonic acid",
                NameStatus::Preferred,
            ),
            (
                "c1nc2cc3occc3cc2[nH]1",
                "1H-furo[2,3-f][1,3]benzimidazole",
                NameStatus::Preferred,
            ),
            (
                "n1nc2cc3occc3cc2[nH]1",
                "1H-furo[2,3-f][1,2,3]benzotriazole",
                NameStatus::Preferred,
            ),
            (
                "C1=CC=CC=2C=CC=3N(C12)C1=C(N3)C=CC=C1",
                "[1,3]benzimidazolo[1,2-a]quinoline",
                NameStatus::Unverified(NameReason::OpsinFusionLayout),
            ),
            (
                "N1=NN=C2C1=CC1=CC=3C=CC=NC3C=C1C2",
                "4H-[1,2,3]benzotriazolo[5,6-g]quinoline",
                NameStatus::Unverified(NameReason::OpsinFusionLayout),
            ),
            (
                "c1ccc2c(c1)NC1(CCC1)N2",
                "1,3-dihydrospiro[[1,3]benzimidazole-2,1'-cyclobutane]",
                NameStatus::Preferred,
            ),
        ] {
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
            for candidate in std::iter::once(name.text.as_str()).chain(
                name.alternatives
                    .iter()
                    .map(|alternative| alternative.name.as_str()),
            ) {
                let parsed = parser.parse(candidate, &opsin::ParseOptions::strict());
                assert_eq!(
                    parsed.status,
                    opsin::Status::Success,
                    "{candidate}: {}",
                    parsed.message
                );
                let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
                verify_identity(smiles, &reconstructed)
                    .unwrap_or_else(|error| panic!("{smiles} -> {candidate}: {error}"));
            }
        }
    }

    #[test]
    // Provenance: fn tests::retained_parent_indicated_hydrogen_survives_replacement | adapted
    // Source: ReShiki | src/naming/openclatura/retained_name_policy.rs::tests::retained_parent_indicated_hydrogen_survives_replacement
    // Source: openclatura | tests/unit/test_analysis.py::test_stage3_retained_fused_pin_forms_and_indicated_hydrogens_are_validated
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn retained_parent_indicated_hydrogen_survives_replacement() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("O1COc2ccccc21", "2H-1,3-benzodioxole"),
            ("O=Cc1ccc2OCOc2c1", "2H-1,3-benzodioxole-5-carbaldehyde"),
            ("CC1(C)Oc2ccccc2O1", "2,2-dimethyl-2H-1,3-benzodioxole"),
            ("C=C1Oc2ccccc2O1", "2-methylidene-2H-1,3-benzodioxole"),
            ("C1OC=CO1", "2H-1,3-dioxole"),
            ("S1CSc2ccccc21", "2H-1,3-benzodithiole"),
            ("C=C1N=c2ccccc2=N1", "2-methylidene-2H-1,3-benzimidazole"),
            ("c1ccc2[nH]ccc2c1", "1H-indole"),
            ("Cn1ccc2ccccc21", "1-methyl-1H-indole"),
            ("c1ccc2[nH]cnc2c1", "1H-1,3-benzimidazole"),
            ("Cn1cnc2ccccc21", "1-methyl-1H-1,3-benzimidazole"),
        ] {
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            for candidate in std::iter::once(name.text.as_str()).chain(
                name.alternatives
                    .iter()
                    .map(|alternative| alternative.name.as_str()),
            ) {
                let parsed = parser.parse(candidate, &opsin::ParseOptions::strict());
                assert_eq!(
                    parsed.status,
                    opsin::Status::Success,
                    "{candidate}: {}",
                    parsed.message
                );
                let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
                verify_identity(smiles, &reconstructed)
                    .unwrap_or_else(|error| panic!("{smiles} -> {candidate}: {error}"));
            }
        }
        // #921: the ylidene site keeps its parent's 2H citation. Its status
        // follows the linear-phane rule and is pinned in the audit fixture.
        assert_eq!(
            super::super::name_smiles(
                r"C1=CC=C(C=C1)COC2=CC\3=C(C=C2)NN/C3=C\4/N=C5C=CC(=CC5=N4)NCCCN6C=CN=C6",
                &Default::default()
            )
            .unwrap()
            .text,
            "(2Z)-2-[5-(benzyloxy)-2,3-dihydro-1H-indazol-3-ylidene]-N-[3-(1H-imidazol-1-yl)propyl]-2H-1,3-benzimidazol-5-amine"
        );
    }

    #[test]
    fn recursive_benzo_policy_preserves_other_retained_substituents() {
        for (smiles, expected) in [
            (
                "CC1=CC(=CC=C1)OCCNC(=O)CN(C)C2CCC3=CC=CC=C23",
                "2-[(indan-1-yl)(methyl)amino]-N-[2-(3-methylphenoxy)ethyl]acetamide",
            ),
            (
                "C=CCN1CC(C2=CC=CC=C21)CCCNC(=O)CCl",
                "2-chloro-N-{3-[1-(prop-2-en-1-yl)indolin-3-yl]propyl}acetamide",
            ),
            (
                "CCC(C)C(=O)N1CCC2=C1C=CC(=C2)C3=CSC(=N3)NC(=O)C4=CC=CC=C4OCC",
                "2-ethoxy-N-{4-[1-(2-methylbutanoyl)indolin-5-yl]-1,3-thiazol-2-yl}benzamide",
            ),
            (
                "CC(C1CCC2=C1C(=C(C=C2)OC)OC)C(C)C(=O)N",
                "3-(6,7-dimethoxyindan-1-yl)-2-methylbutanamide",
            ),
        ] {
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
        }
    }
}
