// SPDX-License-Identifier: MIT
// Origin: adapted from upstream code.
//   openclatura 0.4.3, src/openclatura/fusion/config.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
// Copyright (c) 2026 lamalab-org
// Changes: Openclatura policy/search/ring-shape schema adapted to typed serde configuration.

//! Checked-in fusion policy, search limits, and intrinsic ring shapes.
use serde::Deserialize;
use std::sync::LazyLock;
#[derive(Clone, Debug)]
pub struct FusionRuleConfig {
    pub planner_tier: String,
    pub minimum_ring_size: usize,
    pub minimum_ring_count: usize,
    pub support: FusionSupportConfig,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionSupportConfig {
    pub cover_kinds: Vec<String>,
    pub join_kinds: Vec<String>,
    pub multiparent_parents: bool,
    pub maximum_tree_component_occurrences: usize,
    pub maximum_multiparent_component_occurrences: usize,
    pub charged_parents: bool,
    pub nonstandard_valence: bool,
    pub interior_atoms: bool,
    #[serde(deserialize_with = "Option::deserialize")]
    pub maximum_indicated_hydrogens: Option<usize>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusionComponentData {
    schema_version: usize,
    pub registry_version: String,
    graph_source: String,
    pub retained_component_status: RetainedComponentStatusData,
    rules: FusionRuleData,
    search_limits: FusionSearchLimits,
    annulene_series: AnnuleneSeries,
    ring_shapes: Vec<RingShapeSpec>,
    pub retained_component_generators: Vec<RetainedComponentGenerator>,
    pub generated_components: Vec<GeneratedComponent>,
    pub systematic_component_generators: Vec<SystematicComponentGenerator>,
    pub components: Vec<FusionComponentRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FusionRuleData {
    planner_tier: String,
    support: FusionSupportConfig,
    eligibility_gate: FusionEligibilityGate,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FusionEligibilityGate {
    minimum_ring_size: usize,
    minimum_ring_count: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnuleneSeries {
    minimum_ring_size: usize,
    maximum_ring_size: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedComponentStatusData {
    #[serde(rename = "rule")]
    _rule: String,
    pub components: Vec<RetainedComponentStatus>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedComponentStatus {
    pub template_name: String,
    pub horizontal_ring_count: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GeneratedComponent {
    pub key: String,
    pub generator: String,
    pub ring_size: usize,
    pub bond_class: String,
    pub pin_component: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedComponentGenerator {
    pub family: String,
    pub allow_as_parent: bool,
    pub allow_as_peri_parent: bool,
    pub allow_as_attached: bool,
    #[serde(rename = "rule")]
    _rule: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SystematicComponentGenerator {
    pub generator: String,
    pub allow_as_parent: bool,
    pub allow_as_attached: bool,
    #[serde(rename = "rule")]
    _rule: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusionComponentRow {
    pub key: String,
    #[serde(default)]
    pub parent_name: Option<String>,
    #[serde(default)]
    pub attached_prefix: Option<String>,
    #[serde(default)]
    pub preferred_fusion_prefix: Option<String>,
    #[serde(default)]
    pub template_names: Option<Vec<String>>,
    pub allow_as_parent: bool,
    pub allow_as_attached: bool,
    #[serde(default)]
    pub allow_as_peri_parent: bool,
    #[serde(default)]
    pub parent_hydride_only: bool,
    #[serde(default)]
    pub accepted_general_prefixes: Vec<String>,
    #[serde(default)]
    pub horizontal_ring_count: Option<usize>,
    #[serde(default = "basic")]
    pub multiplicative_prefix_style: String,
    #[serde(default)]
    pub omit_attached_locants: bool,
    #[serde(default)]
    pub omit_ortho_peri_attached_locants: bool,
    #[serde(rename = "rule")]
    pub _rule: String,
}
fn basic() -> String {
    "basic".into()
}
pub(super) fn annulene_ring_sizes_from_data(
    data: &FusionComponentData,
) -> Result<Vec<usize>, String> {
    let p = &data.annulene_series;
    let min = p.minimum_ring_size;
    let max = p.maximum_ring_size;
    if min < 7 || min > max {
        return Err("invalid annulene interval".into());
    }
    Ok((min..=max).collect())
}
fn fusion_nomenclature_config_from_data(
    data: &FusionComponentData,
) -> Result<FusionNomenclatureConfig, String> {
    if data.schema_version != 1 {
        return Err("unsupported fusion schema".into());
    }
    let rules = FusionRuleConfig {
        planner_tier: data.rules.planner_tier.clone(),
        minimum_ring_size: data.rules.eligibility_gate.minimum_ring_size,
        minimum_ring_count: data.rules.eligibility_gate.minimum_ring_count,
        support: data.rules.support.clone(),
    };
    if rules.support.cover_kinds != ["tree"]
        || rules.support.join_kinds != ["ortho", "ortho_peri", "higher_order"]
    {
        return Err("unimplemented fusion support tier".into());
    }
    if rules.planner_tier.is_empty() {
        return Err("fusion planner tier must be named".into());
    }
    let search = data.search_limits.clone();
    if search.minimum_ring_size > search.maximum_ring_size || search.mancude_states == 0 {
        return Err("invalid fusion ring limits".into());
    }
    let ring_shapes = data.ring_shapes.clone();
    if ring_shapes.iter().any(|s| {
        s.horizontal_axis_class.is_empty()
            || !(3..=8).contains(&s.ring_size)
            || s.vertices.len() != s.ring_size
            || s.edge_directions.len() != s.ring_size
            || s.vertices.get(..2) != Some(&[(0, 0), (4, 0)][..])
    }) {
        return Err("invalid intrinsic ring shape".into());
    }
    Ok(FusionNomenclatureConfig {
        registry_version: data.registry_version.clone(),
        graph_source: data.graph_source.clone(),
        rules,
        search,
        ring_shapes,
        annulene_ring_sizes: annulene_ring_sizes_from_data(data)?,
    })
}
#[allow(clippy::expect_used)]
pub(super) static FUSION_DATA: LazyLock<FusionComponentData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/fusion_components.json"))
        .expect("checked-in fusion policy JSON")
});
#[allow(clippy::expect_used)]
static CONFIG: LazyLock<FusionNomenclatureConfig> = LazyLock::new(|| {
    fusion_nomenclature_config_from_data(&FUSION_DATA).expect("checked-in fusion policy")
});
pub fn fusion_nomenclature_config() -> &'static FusionNomenclatureConfig {
    &CONFIG
}
