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
