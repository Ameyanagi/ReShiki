use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;
#[derive(Clone, Debug, Deserialize)]
pub struct Element {
    pub symbol: String,
    pub name: String,
    pub atomic_number: usize,
    pub standard_valence: usize,
    pub hw_stem: Option<String>,
    pub hw_priority: Option<usize>,
    #[serde(default)]
    pub mancude_forced_single: bool,
}
// The embedded element table and its row schema are immutable checked-in port data.
#[allow(clippy::expect_used)]
pub static ELEMENTS: LazyLock<BTreeMap<String, Element>> = LazyLock::new(|| {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../data/elements.json")).expect("element data");
    v["elements"]
        .as_array()
        .expect("checked-in element table contains an array")
        .iter()
        .map(|v| {
            let e: Element = serde_json::from_value(v.clone()).expect("element row");
            (e.symbol.clone(), e)
        })
        .collect()
});
pub fn get(symbol: &str) -> Option<&'static Element> {
    ELEMENTS.get(symbol)
}
pub fn is_known(symbol: &str) -> bool {
    ELEMENTS.contains_key(symbol)
}
