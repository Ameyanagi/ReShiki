//! Finite, verified two-fragment heteroatom/carbon bridge constructions.
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

pub type ConstructionKey = (Vec<String>, Vec<usize>);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeBridgeConstruction {
    pub heteroatom: String,
    pub hetero_prefix: String,
    pub carbon_prefix: String,
    pub carbon_bond_orders: Vec<usize>,
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
fn data() -> Value {
    serde_json::from_str(include_str!("../data/fusion_composite_bridges.json"))
        .expect("validated composite bridge data")
}
#[allow(clippy::expect_used)] // These shapes and integer values belong to the checked-in table.
pub fn localized_carbon_bridge_bond_orders() -> &'static BTreeSet<Vec<usize>> {
    static ORDERS: LazyLock<BTreeSet<Vec<usize>>> = LazyLock::new(|| {
        data()["localized_carbon_bridge_bond_orders"]
            .as_array()
            .expect("localized bridge rows")
            .iter()
            .map(|row| {
                let orders: Vec<_> = row
                    .as_array()
                    .expect("localized bridge path")
                    .iter()
                    .map(|order| order.as_u64().expect("integer bridge bond") as usize)
                    .collect();
                assert!(
                    !orders.is_empty()
                        && orders.contains(&2)
                        && orders.iter().all(|order| [1, 2].contains(order))
                );
                orders
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
            composite_bridge_constructions_from_data(&data())
                .expect("validated composite bridge constructions")
        });
    &CONSTRUCTIONS
}
pub fn composite_bridge_constructions_from_data(
    data: &Value,
) -> Result<BTreeMap<ConstructionKey, CompositeBridgeConstruction>, String> {
    if data["schema_version"].as_u64() != Some(1) {
        return Err("unsupported composite bridge construction schema version".into());
    }
    let rows = data["constructions"]
        .as_array()
        .ok_or("composite bridge constructions must be a list")?;
    let mut indexed = BTreeMap::new();
    for row in rows {
        if !row.is_object() {
            return Err("composite bridge construction must be a mapping".into());
        }
        let text = |field: &str| {
            row[field]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("composite bridge construction requires {field}"))
        };
        let heteroatom = text("heteroatom")?;
        if !["O", "S", "Se", "Te", "N", "P", "As", "Sb", "Bi"].contains(&heteroatom.as_str()) {
            return Err("composite bridge must start with a supported heteroatom fragment".into());
        }
        let orders = row["carbon_bond_orders"]
            .as_array()
            .ok_or("composite carbon fragment requires single or double path bonds")?;
        let mut carbon_bond_orders = Vec::new();
        for order in orders {
            let value = order
                .as_u64()
                .filter(|v| [1, 2].contains(v))
                .ok_or("composite carbon fragment requires single or double path bonds")?;
            carbon_bond_orders.push(value as usize);
        }
        let construction = CompositeBridgeConstruction {
            heteroatom,
            hetero_prefix: text("hetero_prefix")?,
            carbon_prefix: text("carbon_prefix")?,
            carbon_bond_orders,
        };
        let key = (construction.symbols(), construction.internal_bond_orders());
        if indexed.insert(key, construction).is_some() {
            return Err("duplicate composite bridge construction graph".into());
        }
    }
    Ok(indexed)
}
