//! Plain graph model and helpers ported from openclatura's `molecule.py`.
use super::graph_kernel::{Edge, normalize_edge};
use crate::naming::{NameReason, NameStatus};
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
    relative_ring_names: Cell<bool>,
    unknown_units: RefCell<BTreeSet<String>>,
    ring_assembly_junctions: RefCell<AtomSet>,
    relative_ring_alternative: Cell<bool>,
    endo_exo_names: Cell<bool>,
    endo_exo_alternative: RefCell<Option<AtomSet>>,
    benzoheterocycle_names: Cell<bool>,
    higher_order_fusion_alternative: Cell<bool>,
    benzo_fusion_alternative: Cell<bool>,
    acid_prefix_mode: Cell<bool>,
    isotope_parent: Cell<bool>,
}

impl Default for NamingBudget {
    fn default() -> Self {
        Self {
            remaining: Cell::new(NAMING_WORK_LIMIT),
            depth: Cell::new(0),
            status: Cell::new(NameStatus::Unverified(NameReason::UnauditedPath)),
            relative_ring_names: Cell::new(false),
            unknown_units: RefCell::default(),
            ring_assembly_junctions: RefCell::default(),
            relative_ring_alternative: Cell::new(false),
            endo_exo_names: Cell::new(false),
            endo_exo_alternative: RefCell::default(),
            benzoheterocycle_names: Cell::new(true),
            higher_order_fusion_alternative: Cell::new(false),
            benzo_fusion_alternative: Cell::new(false),
            acid_prefix_mode: Cell::new(false),
            isotope_parent: Cell::new(false),
        }
    }
}

pub(super) struct NamingDepth<'a>(&'a NamingBudget);

impl Drop for NamingDepth<'_> {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get() - 1);
    }
}

pub(super) struct AcidPrefixScope<'a> {
    active: &'a Cell<bool>,
    previous: bool,
}

impl Drop for AcidPrefixScope<'_> {
    fn drop(&mut self) {
        self.active.set(self.previous);
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
    pub stereogenic_atoms: AtomSet,
    pub stereogenic_bonds: AtomSet,
    /// Isotope parent naming retains source CIP labels after removing labels.
    pub(super) isotope_labels_stripped: bool,
    /// Functional centres whose capped fragments omit real substitution sites.
    pub projected_substitution_centers: AtomSet,
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
            stereogenic_atoms: self.stereogenic_atoms.clone(),
            stereogenic_bonds: self.stereogenic_bonds.clone(),
            isotope_labels_stripped: self.isotope_labels_stripped,
            projected_substitution_centers: self.projected_substitution_centers.clone(),
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

    /// Keep acid prefixes through carbon and heteroatom branch recursion,
    /// restoring the enclosing component's naming mode when the branch ends.
    pub(super) fn enter_acid_prefix_scope(&self) -> AcidPrefixScope<'_> {
        AcidPrefixScope {
            active: &self.naming_budget.acid_prefix_mode,
            previous: self.naming_budget.acid_prefix_mode.replace(true),
        }
    }

    pub(super) fn uses_acid_prefixes(&self) -> bool {
        self.naming_budget.acid_prefix_mode.get()
    }

    /// The isotope adapter names the stripped parent and needs a proven atom
    /// locant map, which only ordinary parent plans provide.
    pub(super) fn mark_isotope_parent(&self) {
        self.naming_budget.isotope_parent.set(true);
    }

    pub(super) fn is_isotope_parent(&self) -> bool {
        self.naming_budget.isotope_parent.get()
    }

    pub(super) fn mark_name(&self, status: NameStatus) {
        if status.severity() > self.name_status().severity() {
            self.set_name_status(status);
        }
    }

    pub(super) fn certify(&self) {
        if self.name_status() == NameStatus::Unverified(NameReason::UnauditedPath) {
            self.set_name_status(NameStatus::Preferred);
        }
    }

    pub(super) fn mark_relative_ring_alternative(&self) {
        self.naming_budget.relative_ring_alternative.set(true);
    }

    pub(super) fn has_relative_ring_alternative(&self) -> bool {
        self.naming_budget.relative_ring_alternative.get()
    }

    pub(super) fn use_relative_ring_names(&self) -> bool {
        self.naming_budget.relative_ring_names.get()
    }

    pub(super) fn set_relative_ring_names(&self, enabled: bool) {
        self.naming_budget.relative_ring_names.set(enabled);
    }

    pub(super) fn mark_endo_exo_alternative(&self, cage: &AtomSet) {
        *self.naming_budget.endo_exo_alternative.borrow_mut() = Some(cage.clone());
    }

    pub(super) fn has_endo_exo_alternative(&self) -> bool {
        // Fragment naming shares this budget. Check against the complete
        // source graph here, so an acid-side fragment cannot hide stereo on
        // the alcohol side of an ester (or any other external group).
        self.naming_budget
            .endo_exo_alternative
            .borrow()
            .as_ref()
            .is_some_and(|cage| {
                self.atoms
                    .values()
                    .all(|atom| !atom.has_stereo() || cage.contains(&atom.idx))
            })
    }

    pub(super) fn use_endo_exo_names(&self) -> bool {
        self.naming_budget.endo_exo_names.get()
    }

    pub(super) fn set_endo_exo_names(&self, enabled: bool) {
        self.naming_budget.endo_exo_names.set(enabled);
    }

    pub(super) fn mark_higher_order_fusion_alternative(&self) {
        self.naming_budget.higher_order_fusion_alternative.set(true);
    }

    pub(super) fn has_higher_order_fusion_alternative(&self) -> bool {
        self.naming_budget.higher_order_fusion_alternative.get()
    }

    pub(super) fn mark_benzo_fusion_alternative(&self) {
        self.naming_budget.benzo_fusion_alternative.set(true);
    }

    pub(super) fn has_benzo_fusion_alternative(&self) -> bool {
        self.naming_budget.benzo_fusion_alternative.get()
    }

    pub(super) fn use_benzoheterocycle_names(&self) -> bool {
        self.naming_budget.benzoheterocycle_names.get()
    }

    pub(super) fn set_benzoheterocycle_names(&self, enabled: bool) {
        self.naming_budget.benzoheterocycle_names.set(enabled);
    }

    pub(super) fn name_status(&self) -> NameStatus {
        self.naming_budget.status.get()
    }

    pub(super) fn record_unknown_unit(&self, locant: String) {
        self.naming_budget.unknown_units.borrow_mut().insert(locant);
    }

    pub(super) fn unknown_units(&self) -> Vec<String> {
        self.naming_budget
            .unknown_units
            .borrow()
            .iter()
            .cloned()
            .collect()
    }

    pub(super) fn record_ring_assembly(&self, junctions: &AtomSet) {
        self.naming_budget
            .ring_assembly_junctions
            .borrow_mut()
            .extend(junctions);
    }

    pub(super) fn ring_assembly_is_named(&self, junction: usize) -> bool {
        self.naming_budget
            .ring_assembly_junctions
            .borrow()
            .contains(&junction)
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
        let unknown_units = self.naming_budget.unknown_units.borrow().clone();
        let ring_assemblies = self.naming_budget.ring_assembly_junctions.borrow().clone();
        let higher_order_fusion = self.has_higher_order_fusion_alternative();
        let benzo_fusion = self.has_benzo_fusion_alternative();
        let relative_ring = self.has_relative_ring_alternative();
        let endo_exo = self.naming_budget.endo_exo_alternative.borrow().clone();
        let result = name();
        if !matches!(result, Ok(Some(_))) {
            self.set_name_status(status);
            *self.naming_budget.unknown_units.borrow_mut() = unknown_units;
            *self.naming_budget.ring_assembly_junctions.borrow_mut() = ring_assemblies;
            self.naming_budget
                .higher_order_fusion_alternative
                .set(higher_order_fusion);
            self.naming_budget
                .benzo_fusion_alternative
                .set(benzo_fusion);
            self.naming_budget
                .relative_ring_alternative
                .set(relative_ring);
            *self.naming_budget.endo_exo_alternative.borrow_mut() = endo_exo;
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
        fragment.isotope_labels_stripped = self.isotope_labels_stripped;
        fragment.projected_substitution_centers = self
            .projected_substitution_centers
            .intersection(atom_ids)
            .copied()
            .collect();
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
        // Naming projections retain source descriptors and their possible units,
        // including when a caller subsequently caps a functional group.
        fragment.stereogenic_atoms = self
            .stereogenic_atoms
            .iter()
            .copied()
            .filter(|idx| fragment.atoms.contains_key(idx))
            .collect();
        fragment.stereogenic_bonds = self
            .stereogenic_bonds
            .iter()
            .copied()
            .filter(|idx| fragment.bonds.contains_key(idx))
            .collect();
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
        let mol = super::super::graph_io::read_smiles("CCO").unwrap();
        let fragment = mol.subgraph(&AtomSet::from([0, 1]));
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedPath)
        );
        mol.mark_name(NameStatus::Preferred);
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedPath)
        );
        fragment.certify();
        assert_eq!(mol.name_status(), NameStatus::Preferred);
        fragment.mark_name(NameStatus::Unverified(NameReason::UnauditedParent));
        mol.clone()
            .mark_name(NameStatus::Unverified(NameReason::AtomOrder));
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedParent)
        );
        mol.certify();
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedParent)
        );
        mol.mark_name(NameStatus::NotPreferred(NameReason::AminoPrefix));
        fragment.mark_name(NameStatus::NotPreferred(NameReason::FunctionalClass));
        mol.mark_name(NameStatus::Preferred);
        mol.certify();
        assert_eq!(
            fragment.name_status(),
            NameStatus::NotPreferred(NameReason::AminoPrefix)
        );
        fragment.set_name_status(NameStatus::Preferred);
        assert_eq!(mol.name_status(), NameStatus::Preferred);
    }

    #[test]
    fn discarded_candidates_restore_stereo_alternatives() {
        let mol = super::super::graph_io::read_smiles("CCO").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let _: Option<()> = mol
            .candidate(|| {
                mol.mark_relative_ring_alternative();
                mol.mark_endo_exo_alternative(&atoms);
                Ok(None)
            })
            .unwrap();
        assert!(!mol.has_relative_ring_alternative());
        assert!(!mol.has_endo_exo_alternative());
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
        mol.stereogenic_atoms.extend([2, 3]);
        mol.stereogenic_bonds.extend([1, 8]);
        let ids = AtomSet::from([2, 5, 9]);
        let fragment = mol.subgraph(&ids);
        assert_eq!(fragment.get_neighbors(5), &[9, 2]);
        assert_eq!(
            fragment.bonds.keys().copied().collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(fragment.accurate_cip, mol.accurate_cip);
        assert_eq!(fragment.legacy_cip, mol.legacy_cip);
        assert_eq!(fragment.stereogenic_atoms, AtomSet::from([2]));
        assert_eq!(fragment.stereogenic_bonds, AtomSet::from([1]));
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

    #[test]
    fn extracted_acid_fragments_preserve_stereo_locant_omission() {
        for (smiles, expected) in [
            (
                "O=C(O)c1ccc([C@H](F)Cl)cc1",
                "4-[(R)-chlorofluoromethyl]benzoic acid",
            ),
            (
                "CC(=O)OC(=O)c1ccc([C@H](F)Cl)cc1",
                "acetic 4-[(R)-chlorofluoromethyl]benzoic anhydride",
            ),
            (
                "O=C(O)CNC(=O)c1ccc([C@H](F)Cl)cc1",
                "{4-[(R)-chlorofluoromethyl]benzamido}acetic acid",
            ),
            (
                "CC(=O)OC(=O)[C@H](F)Cl",
                "acetic (R)-chlorofluoroacetic anhydride",
            ),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            let request = crate::naming::local::parse_request(&name.text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{}",
                name.text
            );
        }
    }
}
