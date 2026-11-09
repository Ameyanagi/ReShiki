// SPDX-License-Identifier: MIT
// Origin: port
// Source: openclatura | src/openclatura/fusion/composite_bridges.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org

//! Finite, verified two-fragment heteroatom/carbon bridge constructions.
use serde::{Deserialize, de::IgnoredAny};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

pub type ConstructionKey = (Vec<String>, Vec<usize>);
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompositeBridgeConstruction {
    pub heteroatom: String,
    pub hetero_prefix: String,
    pub carbon_prefix: String,
    pub carbon_bond_orders: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompositeBridgeData {
    schema_version: u64,
    #[serde(rename = "rule")]
    _rule: IgnoredAny,
    #[serde(rename = "verification")]
    _verification: IgnoredAny,
    localized_carbon_bridge_bond_orders: Vec<Vec<usize>>,
    constructions: Vec<CompositeBridgeConstruction>,
}
impl CompositeBridgeConstruction {
    pub fn symbols(&self) -> Vec<String> {
        std::iter::once(self.heteroatom.clone())
            .chain(std::iter::repeat_n(
                "C".into(),
                self.carbon_bond_orders.len() + 1,
            ))
            .collect()
    }
    pub fn internal_bond_orders(&self) -> Vec<usize> {
        std::iter::once(1)
            .chain(self.carbon_bond_orders.iter().copied())
            .collect()
    }
}
#[allow(clippy::expect_used)] // Immutable, checked-in table follows the shared table-loading convention.
static DATA: LazyLock<CompositeBridgeData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/fusion_composite_bridges.json"))
        .expect("validated composite bridge data")
});
pub fn localized_carbon_bridge_bond_orders() -> &'static BTreeSet<Vec<usize>> {
    static ORDERS: LazyLock<BTreeSet<Vec<usize>>> = LazyLock::new(|| {
        DATA.localized_carbon_bridge_bond_orders
            .iter()
            .map(|orders| {
                assert!(
                    !orders.is_empty()
                        && orders.contains(&2)
                        && orders.iter().all(|order| [1, 2].contains(order))
                );
                orders.clone()
            })
            .collect()
    });
    &ORDERS
}
#[allow(clippy::expect_used)] // Construction validation applies to immutable, checked-in data.
pub fn composite_bridge_constructions()
-> &'static BTreeMap<ConstructionKey, CompositeBridgeConstruction> {
    static CONSTRUCTIONS: LazyLock<BTreeMap<ConstructionKey, CompositeBridgeConstruction>> =
        LazyLock::new(|| {
            composite_bridge_constructions_from_data(&DATA)
                .expect("validated composite bridge constructions")
        });
    &CONSTRUCTIONS
}
fn composite_bridge_constructions_from_data(
    data: &CompositeBridgeData,
) -> Result<BTreeMap<ConstructionKey, CompositeBridgeConstruction>, String> {
    if data.schema_version != 1 {
        return Err("unsupported composite bridge construction schema version".into());
    }
    let mut indexed = BTreeMap::new();
    for construction in &data.constructions {
        for (field, value) in [
            ("heteroatom", &construction.heteroatom),
            ("hetero_prefix", &construction.hetero_prefix),
            ("carbon_prefix", &construction.carbon_prefix),
        ] {
            if value.trim().is_empty() {
                return Err(format!("composite bridge construction requires {field}"));
            }
        }
        if !["O", "S", "Se", "Te", "N", "P", "As", "Sb", "Bi"]
            .contains(&construction.heteroatom.as_str())
        {
            return Err("composite bridge must start with a supported heteroatom fragment".into());
        }
        if !construction
            .carbon_bond_orders
            .iter()
            .all(|order| [1, 2].contains(order))
        {
            return Err("composite carbon fragment requires single or double path bonds".into());
        }
        let key = (construction.symbols(), construction.internal_bond_orders());
        if indexed.insert(key, construction.clone()).is_some() {
            return Err("duplicate composite bridge construction graph".into());
        }
    }
    Ok(indexed)
}
