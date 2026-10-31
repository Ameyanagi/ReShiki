//! Small graph values used by retained-parent bond-state proofs.
pub use super::super::graph_kernel::{Edge, EdgeSet, normalize_edge};
use super::super::molecule::AtomSet;
use super::super::retained_graph_model::RetainedGraphTemplate;
use super::mancude::ParentDerivativeState;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionGraphAtom {
    pub id: usize,
    pub symbol: String,
    pub formal_charge: i32,
    pub pi_capacity: usize,
    pub forced_single: bool,
    pub indicated_h_site: bool,
    pub saturated: bool,
}
impl FusionGraphAtom {
    pub(super) fn new(id: usize, symbol: impl Into<String>) -> Self {
        Self {
            id,
            symbol: symbol.into(),
            formal_charge: 0,
            pi_capacity: 1,
            forced_single: false,
            indicated_h_site: false,
            saturated: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionGraphBond {
    pub atoms: Edge,
    pub bond_class: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionGraph {
    pub atoms: Vec<FusionGraphAtom>,
    pub bonds: Vec<FusionGraphBond>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BondAssignment {
    pub orders: Vec<(Edge, usize)>,
}
impl BondAssignment {
    pub(super) fn paired_atoms(&self) -> AtomSet {
        self.orders
            .iter()
            .filter(|(_, order)| *order == 2)
            .flat_map(|(edge, _)| [edge.0, edge.1])
            .collect()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParentBondModel {
    pub allowed_kekule_assignments: Vec<BondAssignment>,
    pub required_single_bonds: EdgeSet,
    pub pi_eligible_edges: EdgeSet,
    pub maximum_non_cumulative_double_bonds: usize,
    pub required_double_bonds: EdgeSet,
}
impl ParentBondModel {
    pub(super) fn edges(&self) -> EdgeSet {
        self.required_single_bonds
            .iter()
            .chain(&self.required_double_bonds)
            .chain(&self.pi_eligible_edges)
            .copied()
            .collect()
    }
    pub(super) fn pi_atoms(&self) -> BTreeSet<usize> {
        self.pi_eligible_edges
            .iter()
            .chain(&self.required_double_bonds)
            .flat_map(|&(u, v)| [u, v])
            .collect()
    }
}

// Fusion planning records retain Python sequence order explicitly in Vec.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComparisonKey {
    Int(i64),
    Str(String),
    Tuple(Vec<ComparisonKey>),
}
impl Default for ComparisonKey {
    fn default() -> Self {
        Self::Tuple(Vec::new())
    }
}
impl From<i32> for ComparisonKey {
    fn from(v: i32) -> Self {
        Self::Int(v as i64)
    }
}
impl From<i64> for ComparisonKey {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<usize> for ComparisonKey {
    fn from(v: usize) -> Self {
        Self::Int(v as i64)
    }
}
impl From<String> for ComparisonKey {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}
impl From<&str> for ComparisonKey {
    fn from(v: &str) -> Self {
        Self::Str(v.into())
    }
}
impl From<Vec<i64>> for ComparisonKey {
    fn from(v: Vec<i64>) -> Self {
        Self::Tuple(v.into_iter().map(Self::Int).collect())
    }
}
impl ComparisonKey {
    pub(super) fn tuple(values: impl IntoIterator<Item = ComparisonKey>) -> Self {
        Self::Tuple(values.into_iter().collect())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuditStatus {
    Confirmed,
    #[default]
    Abstain,
    Mismatch,
    Error,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum FusionJoinKind {
    #[default]
    Ortho,
    OrthoPeri,
    HigherOrder,
}
impl FusionJoinKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Ortho => "ortho",
            Self::OrthoPeri => "ortho_peri",
            Self::HigherOrder => "higher_order",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComponentLocant {
    pub component_id: usize,
    pub text: String,
    pub prime_depth: usize,
}
impl std::fmt::Display for ComponentLocant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.text, "'".repeat(self.prime_depth))
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct FusionSide {
    pub component_id: usize,
    pub letter: String,
    pub prime_depth: usize,
}
impl std::fmt::Display for FusionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.letter, "'".repeat(self.prime_depth))
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderedFusionInterface {
    pub kind: FusionJoinKind,
    pub attached_occurrence: usize,
    pub host_occurrence: usize,
    pub attached_path: Vec<ComponentLocant>,
    pub host_path: Vec<ComponentLocant>,
    pub cited_attached_locants: Vec<ComponentLocant>,
    pub host_sides: Vec<FusionSide>,
    pub host_locants: Vec<ComponentLocant>,
    pub ordered_input_atoms: Vec<usize>,
    pub ordered_input_edges: Vec<Edge>,
    pub ordered_input_bonds: Vec<usize>,
}
impl OrderedFusionInterface {
    pub(super) fn shared_input_atoms(&self) -> BTreeSet<usize> {
        self.ordered_input_atoms.iter().copied().collect()
    }
    pub(super) fn shared_input_bonds(&self) -> BTreeSet<usize> {
        self.ordered_input_bonds.iter().copied().collect()
    }
}
#[derive(Clone, Debug, Default)]
pub struct FusionComponentConstructionOrder {
    pub atom_locants: Vec<String>,
    pub directed_bond_locants: Vec<(String, String)>,
    pub source: String,
    pub version: String,
}
#[derive(Clone, Debug, Default)]
pub struct FusionComponentSpec {
    pub key: String,
    pub parent_name: String,
    pub attached_prefix: String,
    pub template: RetainedGraphTemplate,
    pub usable_as_parent: bool,
    pub usable_as_attached: bool,
    pub accepted_general_prefixes: Vec<String>,
    pub horizontal_ring_count: usize,
    pub multiplicative_prefix_style: String,
    pub usable_as_peri_parent: bool,
    pub construction_order: Option<FusionComponentConstructionOrder>,
    // Fusion topology stays beside the existing shared retained-parent graph.
    pub rings: Vec<Vec<String>>,
    pub peripheral_order: Vec<String>,
    pub interior_atoms: Vec<String>,
    pub pin_component: bool,
}
impl FusionComponentSpec {
    pub(super) fn locants(&self) -> &[String] {
        &self.template.locants
    }
    pub(super) fn atoms(&self) -> &[super::super::retained_graph_model::RetainedGraphAtomTemplate] {
        &self.template.atoms
    }
    pub(super) fn bonds(&self) -> &[super::super::retained_graph_model::RetainedGraphBondTemplate] {
        &self.template.bonds
    }
    pub(super) fn ring_sizes(&self) -> Vec<usize> {
        self.rings.iter().map(Vec::len).collect()
    }
    pub(super) fn fusion_carbon_locants(&self) -> Vec<String> {
        let atoms = self.template.atom_by_locant();
        self.template
            .fusion_atoms
            .iter()
            .filter(|l| atoms.get(*l).is_some_and(|a| a.symbol == "C"))
            .cloned()
            .collect()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionComponentMatch {
    pub occurrence_id: usize,
    pub spec_key: String,
    pub covered_face_ids: BTreeSet<usize>,
    pub local_to_input_atom: Vec<(String, usize)>,
    pub local_to_skeleton_atom: Vec<(String, usize)>,
    pub topology_key: ComparisonKey,
    pub template_name: String,
}
impl FusionComponentMatch {
    pub(super) fn input_atom_by_locant(&self) -> BTreeMap<String, usize> {
        self.local_to_input_atom.iter().cloned().collect()
    }
    pub(super) fn skeleton_atom_by_locant(&self) -> BTreeMap<String, usize> {
        self.local_to_skeleton_atom.iter().cloned().collect()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionJoin {
    pub order: usize,
    pub interface: OrderedFusionInterface,
}
impl FusionJoin {
    pub(super) fn attached_occurrence(&self) -> usize {
        self.interface.attached_occurrence
    }
    pub(super) fn host_occurrence(&self) -> usize {
        self.interface.host_occurrence
    }
    pub(super) fn kind(&self) -> FusionJoinKind {
        self.interface.kind
    }
    pub(super) fn attached_locants(&self) -> &[ComponentLocant] {
        &self.interface.cited_attached_locants
    }
    pub(super) fn host_sides(&self) -> &[FusionSide] {
        &self.interface.host_sides
    }
    pub(super) fn host_locants(&self) -> &[ComponentLocant] {
        &self.interface.host_locants
    }
    pub(super) fn shared_input_atoms(&self) -> BTreeSet<usize> {
        self.interface.shared_input_atoms()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionDescriptor {
    pub attached_locants: Vec<ComponentLocant>,
    pub parent_sides: Vec<FusionSide>,
    pub parent_locants: Vec<ComponentLocant>,
    pub kind: FusionJoinKind,
}
impl FusionDescriptor {
    pub(super) fn from_interface(i: &OrderedFusionInterface) -> Self {
        Self {
            attached_locants: i.cited_attached_locants.clone(),
            parent_sides: i.host_sides.clone(),
            parent_locants: i.host_locants.clone(),
            kind: i.kind,
        }
    }
    pub(super) fn render(&self) -> String {
        let a = self
            .attached_locants
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        if !self.parent_sides.is_empty() {
            format!(
                "[{a}-{}]",
                self.parent_sides
                    .iter()
                    .map(ToString::to_string)
                    .collect::<String>()
            )
        } else {
            format!(
                "[{a}:{}]",
                self.parent_locants
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionCitationNode {
    pub occurrence_id: usize,
    pub children: Vec<FusionCitationNode>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionCitationPlan {
    pub roots: Vec<FusionCitationNode>,
    pub primary_join_indices: Vec<usize>,
    pub interparent_join_indices: Vec<usize>,
    pub cycle_closing_join_indices: Vec<usize>,
    pub interparent_occurrences: Vec<usize>,
    pub render_order: Vec<usize>,
}
impl FusionCitationPlan {
    pub(super) fn parent_occurrences(&self) -> Vec<usize> {
        self.roots.iter().map(|r| r.occurrence_id).collect()
    }
    pub(super) fn from_tree(root: FusionCitationNode, joins: &[FusionJoin]) -> Self {
        fn post(n: &FusionCitationNode, out: &mut Vec<usize>) {
            for c in &n.children {
                post(c, out);
                out.push(c.occurrence_id)
            }
        }
        let mut render_order = Vec::new();
        post(&root, &mut render_order);
        Self {
            roots: vec![root],
            primary_join_indices: (0..joins.len()).collect(),
            render_order,
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionMultiplicityGroup {
    pub occurrence_ids: Vec<usize>,
    pub multiplier: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ParentLocationKey {
    pub incomplete_system: usize,
    pub maximum_attachment_order: usize,
    pub attachment_count_by_order: Vec<i64>,
    pub multiplicative_grouping_score: Vec<i64>,
    pub interparent_seniority: ComparisonKey,
    pub attached_component_preference: ComparisonKey,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionNameAst {
    pub plan_kind: String,
    pub parent_occurrences: Vec<usize>,
    pub component_occurrences: Vec<FusionComponentMatch>,
    pub joins: Vec<FusionJoin>,
    pub citation_tree: Option<FusionCitationNode>,
    pub multiplicative_groups: Vec<FusionMultiplicityGroup>,
    pub descriptors: Vec<FusionDescriptor>,
    pub citation_plan: Option<FusionCitationPlan>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Face {
    pub id: usize,
    pub atom_cycle: Vec<usize>,
    pub edge_cycle: Vec<usize>,
    pub size: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FaceModel {
    pub faces: Vec<Face>,
    pub edge_to_faces: Vec<(usize, Vec<usize>)>,
    pub perimeter_edges: BTreeSet<usize>,
    pub fusion_edges: BTreeSet<usize>,
    pub outer_boundary: Vec<usize>,
    pub face_adjacency: Vec<(usize, usize, usize)>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutKind {
    #[default]
    Intrinsic,
    OpsinConstruction,
    OpsinEntry,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusedLayout {
    pub face_positions: Vec<(usize, i64, i64)>,
    pub orientation: i64,
    pub atom_positions: Vec<(usize, i64, i64)>,
    pub face_shapes: Vec<(usize, String)>,
    pub orientation_score: Vec<i64>,
    pub audit_evidence: Vec<String>,
    pub component_entry_edge: Option<Edge>,
    pub kind: LayoutKind,
    pub construction_atom_order: Option<Vec<usize>>,
    pub construction_numbering_priority: Option<ComparisonKey>,
    pub entry_perimeter: Option<Vec<usize>>,
    pub entry_face_id: Option<usize>,
    pub entry_ring_positions: Vec<(usize, i64, i64)>,
    pub entry_direction_conflicts: Vec<(usize, usize)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RejectedNumbering {
    pub orientation_score: ComparisonKey,
    pub reason: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionNumberingProof {
    pub selected_face_model: FaceModel,
    pub selected_layout: FusedLayout,
    pub orientation_score: ComparisonKey,
    pub abstract_atom_to_locant: Vec<(usize, String)>,
    pub input_locant_maps: Vec<Vec<(usize, String)>>,
    pub rejected_numberings: Vec<RejectedNumbering>,
}
impl FusionNumberingProof {
    pub(in super::super) fn string_input_locant_maps(&self) -> Vec<BTreeMap<usize, String>> {
        self.input_locant_maps
            .iter()
            .map(|m| m.iter().cloned().collect())
            .collect()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionAuditResult {
    pub status: AuditStatus,
    pub paired_external_pi_consumption: bool,
    pub errors: Vec<String>,
}
impl FusionAuditResult {
    pub(in super::super) fn confirmed(&self) -> bool {
        self.status == AuditStatus::Confirmed
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FusionChargeOperationKind {
    #[default]
    HeteroatomCationization,
    Deprotonation,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FusionChargeOperation {
    pub atom_id: usize,
    pub locant: String,
    pub symbol: String,
    pub base_charge: i32,
    pub observed_charge: i32,
    pub operation_kind: FusionChargeOperationKind,
}
#[derive(Clone, Debug, Default)]
pub struct FusionParentPlan {
    pub ast: FusionNameAst,
    pub rendered_base_name: String,
    pub abstract_parent_graph: FusionGraph,
    pub numbering: FusionNumberingProof,
    pub bond_model: ParentBondModel,
    pub indicated_hydrogens: Vec<String>,
    pub audit: FusionAuditResult,
    pub derivative_state: ParentDerivativeState,
    pub charge_operations: Vec<FusionChargeOperation>,
    pub lambda_descriptors: Vec<super::valence::FusionLambdaDescriptor>,
    pub numbering_variants: Vec<FusionParentPlan>,
}
