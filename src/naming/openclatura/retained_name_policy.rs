//! Preferred retained spellings and explicitly declared hydrogenation policies.
use super::{formatting::format_multiplier, locants::retained_locant_sort_key as parse_locant};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

#[derive(Clone, Debug, Deserialize)]
pub struct RetainedHydrogenationPolicy {
    pub base_parent: String,
    #[serde(default)]
    pub hydro_locants: Vec<String>,
    #[serde(default)]
    pub indicated_hydrogen_locants: Vec<String>,
}
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
    pub fn render(&self) -> String {
        let mut parent = self.base_parent.as_str();
        if let Some((prefix, rest)) = parent.split_once('-')
            && prefix
                .split(',')
                .all(|l| l.ends_with('H') && l.chars().next().is_some_and(|c| c.is_ascii_digit()))
        {
            parent = rest;
        }
        let cited = if self.indicated_hydrogen_locants.is_empty() {
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
        };
        format!(
            "{}-{}-{cited}",
            self.hydro_locants.join(","),
            format_multiplier("hydro", self.hydro_locants.len(), false)
        )
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct RetainedParentNamePolicy {
    pub template_name: String,
    pub preferred_name: String,
    #[serde(default = "all_contexts")]
    pub preferred_contexts: Vec<String>,
    pub hydrogenation: Option<RetainedHydrogenationPolicy>,
    #[serde(default)]
    pub context_names: BTreeMap<String, String>,
}
fn all_contexts() -> Vec<String> {
    vec!["all".into()]
}
impl RetainedParentNamePolicy {
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
    let raw: serde_json::Value =
        serde_json::from_str(include_str!("data/retained_parent_name_policy.json"))
            .expect("retained name policy data");
    let mut rows: Vec<RetainedParentNamePolicy> =
        serde_json::from_value(raw["parents"].clone()).expect("retained name policy rows");
    for row in &mut rows {
        if let Some(h) = &row.hydrogenation {
            row.preferred_name = h.render();
        }
    }
    rows
});
pub fn retained_parent_name_policies() -> &'static [RetainedParentNamePolicy] {
    &POLICIES
}
pub fn retained_parent_name_policy(name: &str) -> Option<&'static RetainedParentNamePolicy> {
    POLICIES.iter().find(|p| p.template_name == name)
}
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
pub fn render_retained_hydrogen_state(
    name: &str,
    default_indicated_h: &[String],
    indicated_h: Option<&[String]>,
) -> String {
    let Some(h) = indicated_h.filter(|h| *h != default_indicated_h) else {
        return name.into();
    };
    let old = default_indicated_h
        .iter()
        .map(|l| format!("{l}H"))
        .collect::<Vec<_>>()
        .join(",");
    let parent = if old.is_empty() {
        name
    } else {
        name.strip_prefix(&format!("{old}-")).unwrap_or(name)
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
mod tests {
    use crate::naming::{NameReason, NameStatus, verify_identity};

    #[test]
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
                "2,3-dihydro-1H-spiro[[1,3]benzimidazole-2,1'-cyclobutane]",
                NameStatus::Preferred,
            ),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
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
    fn recursive_benzo_policy_preserves_other_retained_substituents() {
        for (smiles, expected) in [
            (
                "CC1=CC(=CC=C1)OCCNC(=O)CN(C)C2CCC3=CC=CC=C23",
                "2-[(indan-1-yl)(methyl)amino]-N-{2-[(3-methylphenyl)oxy]ethyl}acetamide",
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
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
        }
    }
}
