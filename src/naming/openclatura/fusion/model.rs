//! Small graph values used by retained-parent bond-state proofs.
use std::collections::BTreeSet;

pub type Edge = (usize, usize);
pub type EdgeSet = BTreeSet<Edge>;
pub fn normalize_edge(u: usize, v: usize) -> Edge {
    (u.min(v), u.max(v))
}

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
    pub fn new(id: usize, symbol: impl Into<String>) -> Self {
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionGraph {
    pub atoms: Vec<FusionGraphAtom>,
    pub bonds: Vec<FusionGraphBond>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BondAssignment {
    pub orders: Vec<(Edge, usize)>,
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
    pub fn edges(&self) -> EdgeSet {
        self.required_single_bonds
            .iter()
            .chain(&self.required_double_bonds)
            .chain(&self.pi_eligible_edges)
            .copied()
            .collect()
    }
    pub fn pi_atoms(&self) -> BTreeSet<usize> {
        self.pi_eligible_edges
            .iter()
            .chain(&self.required_double_bonds)
            .flat_map(|&(u, v)| [u, v])
            .collect()
    }
}
