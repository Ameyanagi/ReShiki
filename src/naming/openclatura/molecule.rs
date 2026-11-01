//! Plain graph model and helpers ported from openclatura's `molecule.py`.
use super::graph_kernel::{Edge, normalize_edge};
use crate::naming::NameStatus;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub type AtomSet = BTreeSet<usize>;

const NAMING_LIMIT_ERROR: &str = "openclatura: naming complexity limit exceeded";
const NAMING_WORK_LIMIT: usize = 500_000;

// Upstream divergence: one shared request budget bounds recursive naming and exhaustive path search.
#[derive(Debug)]
struct NamingBudget {
    remaining: Cell<usize>,
    depth: Cell<usize>,
    status: Cell<NameStatus>,
}

impl Default for NamingBudget {
    fn default() -> Self {
        Self {
            remaining: Cell::new(NAMING_WORK_LIMIT),
            depth: Cell::new(0),
            status: Cell::new(NameStatus::Preferred),
        }
    }
}

pub(super) struct NamingDepth<'a>(&'a NamingBudget);

impl Drop for NamingDepth<'_> {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get() - 1);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Atom {
    pub idx: usize,
    pub symbol: String,
    pub charge: i32,
    pub isotope: Option<u16>,
    pub stereo: Option<String>,
    pub raw_stereo: Option<String>,
    pub is_aromatic: bool,
    pub explicit_h_count: usize,
    pub total_h_count: usize,
    pub radical_electrons: usize,
}

impl Atom {
    pub fn new(idx: usize, symbol: impl Into<String>) -> Self {
        Self {
            idx,
            symbol: symbol.into(),
            ..Self::default()
        }
    }

    pub fn is_carbon(&self) -> bool {
        self.symbol == "C"
    }

    pub fn has_stereo(&self) -> bool {
        self.stereo.is_some() || self.raw_stereo.is_some()
    }

    pub fn element(&self) -> Option<&'static super::rules::elements::Element> {
        super::rules::elements::get(&self.symbol)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bond {
    pub idx: usize,
    pub u: usize,
    pub v: usize,
    pub order: usize,
    pub is_aromatic: bool,
    pub stereo: Option<String>,
    pub in_small_ring: bool,
}

impl Bond {
    pub fn new(idx: usize, u: usize, v: usize, order: usize) -> Self {
        Self {
            idx,
            u,
            v,
            order,
            is_aromatic: false,
            stereo: None,
            in_small_ring: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct Molecule {
    pub atoms: BTreeMap<usize, Atom>,
    pub bonds: BTreeMap<usize, Bond>,
    // Neighbor order is molecular bond insertion order, not numeric atom order.
    adjacency: BTreeMap<usize, Vec<usize>>,
    bond_lookup: BTreeMap<Edge, usize>,
    pub accurate_cip: BTreeMap<usize, String>,
    pub legacy_cip: BTreeMap<usize, String>,
    pub substituted_symbols: AtomSet,
    /// A plan is valid only for this molecular graph and its chemical state.
    pub(crate) fusion_plan_cache:
        RefCell<BTreeMap<AtomSet, Result<Option<super::fusion::model::FusionParentPlan>, String>>>,
    pub(crate) third_component_plan_cache: RefCell<
        BTreeMap<
            AtomSet,
            Result<Option<super::fusion::third_component::ThirdComponentFusionPlan>, String>,
        >,
    >,
    naming_budget: Rc<NamingBudget>,
}

impl Clone for Molecule {
    fn clone(&self) -> Self {
        Self {
            atoms: self.atoms.clone(),
            bonds: self.bonds.clone(),
            adjacency: self.adjacency.clone(),
            bond_lookup: self.bond_lookup.clone(),
            accurate_cip: self.accurate_cip.clone(),
            legacy_cip: self.legacy_cip.clone(),
            substituted_symbols: self.substituted_symbols.clone(),
            fusion_plan_cache: RefCell::default(),
            third_component_plan_cache: RefCell::default(),
            naming_budget: Rc::clone(&self.naming_budget),
        }
    }
}

impl Molecule {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn mark_name(&self, status: NameStatus) {
        let strength = |status| match status {
            NameStatus::Preferred => 0,
            NameStatus::Unverified(_) => 1,
            NameStatus::NotPreferred(_) => 2,
        };
        if strength(status) > strength(self.name_status()) {
            self.set_name_status(status);
        }
    }

    pub(super) fn name_status(&self) -> NameStatus {
        self.naming_budget.status.get()
    }

    pub(super) fn set_name_status(&self, status: NameStatus) {
        self.naming_budget.status.set(status);
    }

    /// Runs a naming attempt whose `None` or error the caller discards, so
    /// only an attempt that names something keeps its status marks.
    pub(super) fn candidate<T>(
        &self,
        name: impl FnOnce() -> Result<Option<T>, String>,
    ) -> Result<Option<T>, String> {
        let status = self.name_status();
        let result = name();
        if !matches!(result, Ok(Some(_))) {
            self.set_name_status(status);
        }
        result
    }

    pub(super) fn spend_naming_work(&self, amount: usize) -> Result<(), String> {
        let remaining = self.naming_budget.remaining.get().saturating_sub(amount);
        self.naming_budget.remaining.set(remaining);
        if remaining == 0 {
            return Err(NAMING_LIMIT_ERROR.into());
        }
        Ok(())
    }

    pub(super) fn enter_naming(&self) -> Result<NamingDepth<'_>, String> {
        if self.naming_budget.depth.get() >= 16 {
            self.naming_budget.remaining.set(0);
            return Err(NAMING_LIMIT_ERROR.into());
        }
        self.spend_naming_work(self.atoms.len())?;
        self.naming_budget
            .depth
            .set(self.naming_budget.depth.get() + 1);
        Ok(NamingDepth(&self.naming_budget))
    }

    pub fn add_atom(&mut self, atom: Atom) -> Result<(), String> {
        if !super::rules::elements::is_known(&atom.symbol) {
            return Err(format!("Unknown element symbol: {}", atom.symbol));
        }
        if self.atoms.contains_key(&atom.idx) {
            return Err(format!("Atom with idx {} already exists.", atom.idx));
        }
        self.adjacency.insert(atom.idx, Vec::new());
        self.fusion_plan_cache.get_mut().clear();
        self.third_component_plan_cache.get_mut().clear();
        self.atoms.insert(atom.idx, atom);
        Ok(())
    }

    pub fn add_bond(&mut self, bond: Bond) -> Result<(), String> {
        if !self.atoms.contains_key(&bond.u) || !self.atoms.contains_key(&bond.v) {
            return Err("Both atoms must exist".into());
        }
        if bond.u == bond.v {
            return Err("Cannot bond an atom to itself.".into());
        }
        if self
            .bond_lookup
            .contains_key(&normalize_edge(bond.u, bond.v))
        {
            return Err(format!(
                "Atoms {} and {} are already bonded.",
                bond.u, bond.v
            ));
        }
        if self.bonds.contains_key(&bond.idx) {
            return Err(format!("Bond with idx {} already exists.", bond.idx));
        }
        self.adjacency.entry(bond.u).or_default().push(bond.v);
        self.fusion_plan_cache.get_mut().clear();
        self.third_component_plan_cache.get_mut().clear();
        self.adjacency.entry(bond.v).or_default().push(bond.u);
        self.bond_lookup
            .insert(normalize_edge(bond.u, bond.v), bond.idx);
        self.bonds.insert(bond.idx, bond);
        Ok(())
    }

    pub fn get_neighbors(&self, atom_idx: usize) -> &[usize] {
        self.adjacency.get(&atom_idx).map_or(&[], Vec::as_slice)
    }

    pub fn get_bond(&self, u: usize, v: usize) -> Option<&Bond> {
        self.bond_lookup
            .get(&normalize_edge(u, v))
            .and_then(|id| self.bonds.get(id))
    }

    pub fn bond_order(&self, u: usize, v: usize) -> usize {
        self.get_bond(u, v).map_or(0, |bond| bond.order)
    }

    pub fn degree(&self, atom_idx: usize) -> usize {
        self.get_neighbors(atom_idx).len()
    }

    pub fn set_atom_charge(&mut self, atom_idx: usize, charge: i32) -> Result<(), String> {
        self.atoms
            .get_mut(&atom_idx)
            .ok_or_else(|| format!("Unknown atom index: {atom_idx}"))?
            .charge = charge;
        self.fusion_plan_cache.get_mut().clear();
        self.third_component_plan_cache.get_mut().clear();
        Ok(())
    }

    pub fn set_bond_order(&mut self, bond_idx: usize, order: usize) -> Result<(), String> {
        self.bonds
            .get_mut(&bond_idx)
            .ok_or_else(|| format!("Unknown bond index: {bond_idx}"))?
            .order = order;
        self.fusion_plan_cache.get_mut().clear();
        self.third_component_plan_cache.get_mut().clear();
        Ok(())
    }

    pub fn subgraph(&self, atom_ids: &AtomSet) -> Self {
        self.subgraph_with_symbols(atom_ids, &BTreeMap::new())
    }

    pub fn subgraph_with_symbols(
        &self,
        atom_ids: &AtomSet,
        symbols: &BTreeMap<usize, String>,
    ) -> Self {
        let mut fragment = Self::new();
        fragment.naming_budget = Rc::clone(&self.naming_budget);
        for idx in atom_ids {
            if let Some(original) = self.atoms.get(idx) {
                let mut atom = original.clone();
                if let Some(symbol) = symbols.get(idx) {
                    atom.symbol.clone_from(symbol);
                    if symbol != &original.symbol {
                        fragment.substituted_symbols.insert(*idx);
                    }
                }
                // Source subgraphs intentionally reset radicals; ordinary input
                // rejects radicals before graph construction.
                atom.radical_electrons = 0;
                fragment.adjacency.insert(
                    *idx,
                    self.get_neighbors(*idx)
                        .iter()
                        .copied()
                        .filter(|n| atom_ids.contains(n))
                        .collect(),
                );
                fragment.atoms.insert(*idx, atom);
            }
        }
        for bond in self.bonds.values() {
            if atom_ids.contains(&bond.u) && atom_ids.contains(&bond.v) {
                fragment
                    .bond_lookup
                    .insert(normalize_edge(bond.u, bond.v), bond.idx);
                fragment.bonds.insert(bond.idx, bond.clone());
            }
        }
        fragment.accurate_cip = self
            .accurate_cip
            .iter()
            .filter(|(idx, _)| fragment.atoms.contains_key(idx))
            .map(|(&idx, label)| (idx, label.clone()))
            .collect();
        fragment.legacy_cip = self
            .legacy_cip
            .iter()
            .filter(|(idx, _)| fragment.atoms.contains_key(idx))
            .map(|(&idx, label)| (idx, label.clone()))
            .collect();
        fragment
    }
}

pub fn bond_ids_within(mol: &Molecule, atom_ids: &AtomSet) -> BTreeSet<usize> {
    mol.bonds
        .values()
        .filter(|b| atom_ids.contains(&b.u) && atom_ids.contains(&b.v))
        .map(|b| b.idx)
        .collect()
}

pub fn component_atoms_until_blocked(
    mol: &Molecule,
    component_atoms: &AtomSet,
    root: usize,
    blocked: &AtomSet,
) -> AtomSet {
    let mut atoms = AtomSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(idx) = queue.pop_front() {
        if atoms.contains(&idx) {
            continue;
        }
        if !component_atoms.contains(&idx) || blocked.contains(&idx) {
            return AtomSet::new();
        }
        atoms.insert(idx);
        queue.extend(
            mol.get_neighbors(idx)
                .iter()
                .copied()
                .filter(|n| !blocked.contains(n) && component_atoms.contains(n)),
        );
    }
    atoms
}

pub fn charged_atoms(mol: &Molecule, atom_ids: &AtomSet) -> AtomSet {
    atom_ids
        .iter()
        .copied()
        .filter(|idx| mol.atoms.get(idx).is_some_and(|a| a.charge != 0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_is_shared_and_keeps_the_first_strongest_reason() {
        use crate::naming::NameReason;
        let mol = super::super::graph_io::read_smiles("CCO").unwrap();
        let fragment = mol.subgraph(&AtomSet::from([0, 1]));
        assert_eq!(mol.name_status(), NameStatus::Preferred);
        fragment.mark_name(NameStatus::Unverified(NameReason::UnauditedParent));
        mol.clone()
            .mark_name(NameStatus::Unverified(NameReason::AtomOrder));
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedParent)
        );
        mol.mark_name(NameStatus::NotPreferred(NameReason::AminoPrefix));
        fragment.mark_name(NameStatus::NotPreferred(NameReason::FunctionalClass));
        mol.mark_name(NameStatus::Preferred);
        assert_eq!(
            fragment.name_status(),
            NameStatus::NotPreferred(NameReason::AminoPrefix)
        );
        fragment.set_name_status(NameStatus::Preferred);
        assert_eq!(mol.name_status(), NameStatus::Preferred);
    }

    #[test]
    fn fusion_plans_do_not_survive_detached_or_chemically_changed_graphs() {
        let mut mol = Molecule::new();
        mol.add_atom(Atom::new(0, "C")).unwrap();
        mol.add_atom(Atom::new(1, "C")).unwrap();
        mol.add_bond(Bond::new(0, 0, 1, 1)).unwrap();
        let atoms = AtomSet::from([0, 1]);
        mol.fusion_plan_cache
            .borrow_mut()
            .insert(atoms.clone(), Ok(None));
        assert!(mol.clone().fusion_plan_cache.borrow().is_empty());
        assert!(mol.subgraph(&atoms).fusion_plan_cache.borrow().is_empty());
        mol.set_atom_charge(0, 1).unwrap();
        assert!(mol.fusion_plan_cache.borrow().is_empty());
        mol.fusion_plan_cache.borrow_mut().insert(atoms, Ok(None));
        mol.set_bond_order(0, 2).unwrap();
        assert!(mol.fusion_plan_cache.borrow().is_empty());
    }

    #[test]
    fn insertion_order_identity_and_induced_subgraph() {
        let mut mol = Molecule::new();
        for idx in [5, 2, 9, 3] {
            mol.add_atom(Atom::new(idx, "C")).unwrap();
        }
        mol.add_bond(Bond::new(4, 5, 9, 1)).unwrap();
        mol.add_bond(Bond::new(1, 5, 2, 2)).unwrap();
        mol.add_bond(Bond::new(8, 2, 3, 1)).unwrap();
        assert_eq!(mol.get_neighbors(5), &[9, 2]);
        assert_eq!(mol.get_bond(2, 5).unwrap().idx, 1);
        assert!(mol.add_bond(Bond::new(9, 2, 5, 1)).is_err());
        assert!(mol.add_bond(Bond::new(8, 9, 3, 1)).is_err());
        mol.accurate_cip.insert(2, "r".into());
        mol.legacy_cip.insert(2, "S".into());
        let ids = AtomSet::from([2, 5, 9]);
        let fragment = mol.subgraph(&ids);
        assert_eq!(fragment.get_neighbors(5), &[9, 2]);
        assert_eq!(
            fragment.bonds.keys().copied().collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(fragment.accurate_cip, mol.accurate_cip);
        assert_eq!(fragment.legacy_cip, mol.legacy_cip);
        assert_eq!(bond_ids_within(&mol, &ids), BTreeSet::from([1, 4]));
        assert_eq!(
            component_atoms_until_blocked(
                &mol,
                &AtomSet::from([2, 3, 5, 9]),
                5,
                &AtomSet::from([2])
            ),
            AtomSet::from([5, 9])
        );
    }
}
