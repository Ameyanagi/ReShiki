//! Checked-in fusion policy, search limits, and intrinsic ring shapes.
use serde::Deserialize;
use serde_json::Value;
use std::sync::LazyLock;
#[derive(Clone, Debug, Deserialize)]
pub struct FusionRuleConfig {
    pub planner_tier: String,
    pub minimum_ring_size: usize,
    pub minimum_ring_count: usize,
    pub support: FusionSupportConfig,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FusionSupportConfig {
    pub cover_kinds: Vec<String>,
    pub join_kinds: Vec<String>,
    pub multiparent_parents: bool,
    pub maximum_tree_component_occurrences: usize,
    pub maximum_multiparent_component_occurrences: usize,
    pub charged_parents: bool,
    pub nonstandard_valence: bool,
    pub interior_atoms: bool,
    pub maximum_indicated_hydrogens: Option<usize>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FusionSearchLimits {
    pub minimum_ring_size: usize,
    pub maximum_ring_size: usize,
    pub maximum_faces: usize,
    pub cycle_states: usize,
    pub face_model_states: usize,
    pub layout_states: usize,
    pub maximum_layouts: usize,
    pub maximum_component_occurrences: usize,
    pub maximum_component_selections: usize,
    pub component_selection_states: usize,
    pub locant_map_combinations: usize,
    pub mancude_states: usize,
    pub maximum_name_candidates: usize,
}
#[derive(Clone, Debug, Deserialize)]
pub struct RingShapeSpec {
    pub ring_size: usize,
    #[serde(rename = "id")]
    pub shape_id: String,
    pub vertices: Vec<(i64, i64)>,
    pub edge_directions: Vec<i64>,
    pub horizontal_axis_class: String,
    #[serde(default)]
    pub distortion_rank: usize,
    #[serde(default = "cartesian")]
    pub coordinate_system: String,
    pub directed_entry_port: Option<usize>,
    #[serde(default)]
    pub opposite_ports: Vec<usize>,
    #[serde(default)]
    pub entry_component_size: usize,
}
fn cartesian() -> String {
    "cartesian".into()
}
#[derive(Clone, Debug)]
pub struct FusionNomenclatureConfig {
    pub registry_version: String,
    pub graph_source: String,
    pub rules: FusionRuleConfig,
    pub search: FusionSearchLimits,
    pub ring_shapes: Vec<RingShapeSpec>,
    pub annulene_ring_sizes: Vec<usize>,
}
pub fn annulene_ring_sizes_from_data(data: &Value) -> Result<Vec<usize>, String> {
    let p = &data["annulene_series"];
    if p.is_null() {
        return Ok(Vec::new());
    }
    let min = p["minimum_ring_size"]
        .as_u64()
        .ok_or("missing annulene minimum")? as usize;
    let max = p["maximum_ring_size"]
        .as_u64()
        .ok_or("missing annulene maximum")? as usize;
    if min < 7 || min > max {
        return Err("invalid annulene interval".into());
    }
    Ok((min..=max).collect())
}
pub fn fusion_nomenclature_config_from_data(
    data: &Value,
) -> Result<FusionNomenclatureConfig, String> {
    if data["schema_version"] != 1 {
        return Err("unsupported fusion schema".into());
    }
    let mut rules = data["rules"].clone();
    rules["minimum_ring_size"] = rules["eligibility_gate"]["minimum_ring_size"].clone();
    rules["minimum_ring_count"] = rules["eligibility_gate"]["minimum_ring_count"].clone();
    let rules: FusionRuleConfig = serde_json::from_value(rules).map_err(|e| e.to_string())?;
    if rules.support.cover_kinds != ["tree"]
        || rules.support.join_kinds != ["ortho", "ortho_peri", "higher_order"]
    {
        return Err("unimplemented fusion support tier".into());
    }
    if rules.planner_tier.is_empty() {
        return Err("fusion planner tier must be named".into());
    }
    let search: FusionSearchLimits =
        serde_json::from_value(data["search_limits"].clone()).map_err(|e| e.to_string())?;
    if search.minimum_ring_size > search.maximum_ring_size || search.mancude_states == 0 {
        return Err("invalid fusion ring limits".into());
    }
    let ring_shapes: Vec<RingShapeSpec> =
        serde_json::from_value(data["ring_shapes"].clone()).map_err(|e| e.to_string())?;
    for s in &ring_shapes {
        if s.horizontal_axis_class.is_empty()
            || !(3..=8).contains(&s.ring_size)
            || s.vertices.len() != s.ring_size
            || s.edge_directions.len() != s.ring_size
            || s.vertices.get(..2) != Some(&[(0, 0), (4, 0)][..])
        {
            return Err("invalid intrinsic ring shape".into());
        }
    }
    Ok(FusionNomenclatureConfig {
        registry_version: data["registry_version"].as_str().unwrap_or_default().into(),
        graph_source: data["graph_source"].as_str().unwrap_or_default().into(),
        rules,
        search,
        ring_shapes,
        annulene_ring_sizes: annulene_ring_sizes_from_data(data)?,
    })
}
#[allow(clippy::expect_used)]
static CONFIG: LazyLock<FusionNomenclatureConfig> = LazyLock::new(|| {
    let v: Value = serde_json::from_str(include_str!("../data/fusion_components.json"))
        .expect("checked-in fusion policy JSON");
    fusion_nomenclature_config_from_data(&v).expect("checked-in fusion policy")
});
pub fn fusion_nomenclature_config() -> &'static FusionNomenclatureConfig {
    &CONFIG
}
