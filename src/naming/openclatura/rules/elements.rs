// SPDX-License-Identifier: MIT
// Origin: adapted from upstream code.
//   openclatura 0.4.3, src/openclatura/rules/elements.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
// Copyright (c) 2026 lamalab-org
// Changes: Adapted openclatura element schema/charge limits using a typed immutable Rust JSON loader.

use serde::{Deserialize, de::IgnoredAny};
use std::collections::BTreeMap;
use std::sync::LazyLock;
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub symbol: String,
    pub name: String,
    pub atomic_number: usize,
    pub standard_valence: usize,
    #[serde(deserialize_with = "Option::deserialize")]
    pub hw_stem: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub hw_priority: Option<usize>,
    pub mancude_forced_single: bool,
    #[serde(default)]
    pub mancude_bonding_limit: Option<usize>,
    #[serde(default)]
    pub mancude_charged_bonding_limits: Vec<(i32, usize)>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fusion_special_priority: Option<u64>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fusion_general_priority: Option<u64>,
    pub fusion_supported: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ElementData {
    #[serde(rename = "schema_version")]
    _schema_version: IgnoredAny,
    elements: Vec<Element>,
}
impl Element {
    pub fn mancude_limit_for_charge(&self, charge: i32) -> Result<Option<usize>, String> {
        if self.mancude_bonding_limit.is_none() || charge == 0 {
            return Ok(self.mancude_bonding_limit);
        }
        if let Some((_, limit)) = self
            .mancude_charged_bonding_limits
            .iter()
            .find(|(q, _)| *q == charge)
        {
            return Ok(Some(*limit));
        }
        Err(format!(
            "unsupported fixed-valence charge {charge:+} for {}",
            self.symbol
        ))
    }
}
// The embedded element table and its row schema are immutable checked-in port data.
#[allow(clippy::expect_used)]
pub static ELEMENTS: LazyLock<BTreeMap<String, Element>> = LazyLock::new(|| {
    let data: ElementData = serde_json::from_str(include_str!("../data/elements.json"))
        .expect("checked-in element data");
    data.elements
        .into_iter()
        .map(|element| (element.symbol.clone(), element))
        .collect()
});
pub fn get(symbol: &str) -> Option<&'static Element> {
    ELEMENTS.get(symbol)
}
pub fn is_known(symbol: &str) -> bool {
    ELEMENTS.contains_key(symbol)
}
