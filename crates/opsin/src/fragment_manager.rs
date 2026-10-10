//! Ordered fragment registration and graph mutation from OPSIN FragmentManager.
use crate::{
    graph::{Atom, AtomId, BondId, Element, FragmentId, Graph, GraphError, StereoReference},
    parse_tree::{Arena, NodeId},
    smiles, valence,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct FragmentManager {
    pub graph: Graph,
    fragments: Vec<FragmentId>,
    inter_fragment_bonds: BTreeMap<FragmentId, Vec<BondId>>,
    pub fragment_tokens: BTreeMap<FragmentId, NodeId>,
}

impl FragmentManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fragments(&self) -> &[FragmentId] {
        &self.fragments
    }
    pub fn token_for_fragment(&self, fragment: FragmentId) -> Option<NodeId> {
        self.fragment_tokens.get(&fragment).copied()
    }
    pub fn register_fragment(&mut self, fragment: FragmentId) {
        if !self.inter_fragment_bonds.contains_key(&fragment) {
            self.fragments.push(fragment);
            self.inter_fragment_bonds.insert(fragment, Vec::new());
        }
    }
    pub fn build_smiles(
        &mut self,
        smiles: &str,
        fragment_type: &str,
        labels: &str,
    ) -> Result<FragmentId, GraphError> {
        let fragment = smiles::build_fragment(&mut self.graph, smiles, fragment_type, labels)?;
        self.register_fragment(fragment);
        Ok(fragment)
    }
    pub fn build_token_smiles(
        &mut self,
        smiles: &str,
        arena: &mut Arena,
        token: NodeId,
        labels: &str,
    ) -> Result<FragmentId, GraphError> {
        let fragment_type = arena[token].attribute("type").unwrap_or("");
        let fragment = self.build_smiles(smiles, fragment_type, labels)?;
        self.graph.fragment_mut(fragment).sub_type =
            arena[token].attribute("subType").unwrap_or("").into();
        self.graph.fragment_mut(fragment).token_attributes = arena[token]
            .attributes
            .iter()
            .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
            .collect();
        // The upstream builder records the token on the fragment; its caller
        // decides whether this is the token's primary fragment association.
        self.fragment_tokens.insert(fragment, token);
        Ok(fragment)
    }
    pub fn create_atom(&mut self, element: Element, fragment: FragmentId) -> AtomId {
        self.graph.add_atom(fragment, element)
    }
    pub fn create_bond(
        &mut self,
        from: AtomId,
        to: AtomId,
        order: u8,
    ) -> Result<BondId, GraphError> {
        let bond = self.graph.add_bond(from, to, order)?;
        if self.graph.atom(from).fragment != self.graph.atom(to).fragment {
            self.add_inter_fragment_bond(bond)?;
        }
        Ok(bond)
    }
    fn add_inter_fragment_bond(&mut self, bond: BondId) -> Result<(), GraphError> {
        for atom in [self.graph.bond(bond).from, self.graph.bond(bond).to] {
            let fragment = self.graph.atom(atom).fragment;
            let bonds = self
                .inter_fragment_bonds
                .get_mut(&fragment)
                .ok_or_else(|| error("Fragment not registered with this FragmentManager!"))?;
            if !bonds.contains(&bond) {
                bonds.push(bond);
            }
        }
        Ok(())
    }
    pub fn inter_fragment_bonds(&self, fragment: FragmentId) -> Result<&[BondId], GraphError> {
        self.inter_fragment_bonds
            .get(&fragment)
            .map(Vec::as_slice)
            .ok_or_else(|| error("Fragment not registered with this FragmentManager!"))
    }
    pub fn remove_bond(&mut self, bond: BondId) {
        for bonds in self.inter_fragment_bonds.values_mut() {
            bonds.retain(|&other| other != bond);
        }
        self.graph.remove_bond(bond);
    }
    pub fn incorporate_fragment(
        &mut self,
        child: FragmentId,
        parent: FragmentId,
    ) -> Result<(), GraphError> {
        let bonds = self
            .inter_fragment_bonds
            .get(&child)
            .cloned()
            .ok_or_else(|| error("Fragment not registered with this FragmentManager!"))?;
        if !self.inter_fragment_bonds.contains_key(&parent) {
            return Err(error("Fragment not registered with this FragmentManager!"));
        }
        self.graph.incorporate_fragment(child, parent)?;
        for bond in bonds {
            let endpoints = [self.graph.bond(bond).from, self.graph.bond(bond).to];
            if endpoints
                .iter()
                .all(|&atom| self.graph.atom(atom).fragment == parent)
            {
                if !self.graph.fragment(parent).bonds.contains(&bond) {
                    self.graph.fragment_mut(parent).bonds.push(bond);
                }
                self.inter_fragment_bonds
                    .get_mut(&parent)
                    .unwrap()
                    .retain(|&other| other != bond);
            } else {
                self.add_inter_fragment_bond(bond)?;
            }
        }
        self.inter_fragment_bonds.remove(&child);
        self.fragments.retain(|&fragment| fragment != child);
        Ok(())
    }
    pub fn incorporate_fragment_with_bond(
        &mut self,
        child: FragmentId,
        from: AtomId,
        parent: FragmentId,
        to: AtomId,
        order: u8,
    ) -> Result<(), GraphError> {
        if self.graph.atom(from).fragment != child {
            return Err(error(
                "OPSIN Bug: fromAtom was not associated with childFrag!",
            ));
        }
        if self.graph.atom(to).fragment != parent {
            return Err(error(
                "OPSIN Bug: toAtom was not associated with parentFrag!",
            ));
        }
        self.incorporate_fragment(child, parent)?;
        self.create_bond(from, to, order)?;
        Ok(())
    }
    /// Builds a shared final view without copying or deleting source fragments.
    pub fn unified_fragment(&mut self) -> FragmentId {
        let originals = self.fragments.clone();
        let unified = self.graph.add_fragment("");
        for fragment in originals {
            let source = self.graph.fragment(fragment).clone();
            for atom in source.atoms {
                self.graph.atom_mut(atom).fragment = unified;
                if !self.graph.fragment(unified).atoms.contains(&atom) {
                    self.graph.fragment_mut(unified).atoms.push(atom);
                }
                for locant in self.graph.atom(atom).locants.clone() {
                    self.graph
                        .fragment_mut(unified)
                        .locants
                        .insert(locant, atom);
                }
            }
            let bonds = source
                .bonds
                .into_iter()
                .chain(self.inter_fragment_bonds[&fragment].iter().copied());
            for bond in bonds {
                if !self.graph.fragment(unified).bonds.contains(&bond) {
                    self.graph.fragment_mut(unified).bonds.push(bond);
                }
            }
            self.graph
                .fragment_mut(unified)
                .out_atoms
                .extend(source.out_atoms);
            self.graph
                .fragment_mut(unified)
                .functional_atoms
                .extend(source.functional_atoms);
        }
        self.register_fragment(unified);
        unified
    }
    pub fn remove_fragment(&mut self, fragment: FragmentId) -> Result<(), GraphError> {
        let bonds = self
            .inter_fragment_bonds
            .remove(&fragment)
            .ok_or_else(|| error("Fragment not registered with this FragmentManager!"))?;
        for bond in bonds {
            for others in self.inter_fragment_bonds.values_mut() {
                others.retain(|&other| other != bond);
            }
        }
        self.fragments.retain(|&other| other != fragment);
        Ok(())
    }
    pub fn atom_by_id(&self, id: AtomId) -> Option<&Atom> {
        self.fragments
            .iter()
            .any(|&fragment| self.graph.fragment(fragment).atoms.contains(&id))
            .then(|| self.graph.atoms.get(id.0))
            .flatten()
            .filter(|atom| atom.active)
    }
    pub fn overall_charge(&self) -> i32 {
        self.fragments
            .iter()
            .flat_map(|&fragment| self.graph.fragment(fragment).atoms.iter())
            .map(|&atom| self.graph.atom(atom).charge)
            .sum()
    }
    pub fn check_valencies(&self) -> Result<(), GraphError> {
        for &fragment in &self.fragments {
            for &atom in &self.graph.fragment(fragment).atoms {
                if !valence::check_valency(&self.graph, atom) {
                    return Err(error(format!(
                        "Atom is in unphysical valency state! Element: {} valency: {}",
                        self.graph.atom(atom).element.symbol(),
                        self.graph.incoming_valency(atom)
                    )));
                }
            }
        }
        Ok(())
    }
    pub fn get_heteroatom(&mut self, smiles: &str) -> Result<AtomId, GraphError> {
        let fragment = smiles::build_fragment(&mut self.graph, smiles, "", "none")?;
        if self.graph.fragment(fragment).atoms.len() != 1 {
            return Err(error(
                "Heteroatom smiles described a fragment with multiple SMILES!",
            ));
        }
        Ok(self.graph.fragment(fragment).atoms[0])
    }
    pub fn replace_atom_with_smiles(
        &mut self,
        atom: AtomId,
        smiles: &str,
    ) -> Result<(), GraphError> {
        let heteroatom = self.get_heteroatom(smiles)?;
        self.replace_atom_with_atom(atom, heteroatom, false)
    }
    pub fn replace_atom_with_atom(
        &mut self,
        atom: AtomId,
        heteroatom: AtomId,
        assign_locant: bool,
    ) -> Result<(), GraphError> {
        let replacement = self.graph.atom(heteroatom).clone();
        let original = self.graph.atom(atom).clone();
        if replacement.charge != 0 {
            if original.charge == 0 {
                self.graph.atom_mut(atom).charge += replacement.charge;
                self.graph
                    .atom_mut(atom)
                    .protons_explicitly_added_or_removed +=
                    replacement.protons_explicitly_added_or_removed;
            } else if original.charge != replacement.charge {
                return Err(error(
                    "Charge conflict between replacement term and atom to be replaced",
                ));
            } else {
                self.graph
                    .atom_mut(atom)
                    .protons_explicitly_added_or_removed =
                    replacement.protons_explicitly_added_or_removed;
            }
        }
        self.graph.atom_mut(atom).element = replacement.element;
        self.remove_element_symbol_locants(atom);
        if assign_locant {
            let fragment = self.graph.atom(atom).fragment;
            let mut locant = replacement.element.symbol().to_owned();
            while self.graph.atom_by_locant(fragment, &locant).is_some() {
                locant.push('\'');
            }
            self.graph.add_locant(atom, locant);
        }
        Ok(())
    }
    fn remove_element_symbol_locants(&mut self, atom: AtomId) {
        let retained: Vec<_> = self
            .graph
            .atom(atom)
            .locants
            .iter()
            .filter(|locant| !is_element_symbol_locant(locant))
            .cloned()
            .collect();
        self.graph.clear_locants(atom);
        for locant in retained {
            self.graph.add_locant(atom, locant);
        }
    }
    pub fn remove_atom_and_associated_bonds(&mut self, atom: AtomId) {
        for bond in self.graph.atom(atom).bonds.clone() {
            self.remove_bond(bond);
        }
        self.graph.remove_atom_and_associated_bonds(atom);
    }

    /// FragmentTools removes terminal atoms through the upstream manager so
    /// its inter-fragment bond registry changes with the graph mutation.
    pub fn remove_terminal_atom(&mut self, atom: AtomId) -> Result<(), GraphError> {
        let result = crate::fragment_tools::remove_terminal_atom(&mut self.graph, atom);
        self.remove_deleted_inter_fragment_bonds();
        result
    }

    pub fn remove_terminal_oxygen(
        &mut self,
        atom: AtomId,
        desired_order: u8,
    ) -> Result<(), GraphError> {
        let result =
            crate::fragment_tools::remove_terminal_oxygen(&mut self.graph, atom, desired_order);
        self.remove_deleted_inter_fragment_bonds();
        result
    }

    fn remove_deleted_inter_fragment_bonds(&mut self) {
        for bonds in self.inter_fragment_bonds.values_mut() {
            bonds.retain(|&bond| self.graph.bond(bond).active);
        }
    }
    pub fn replace_atom_preserving_connectivity(
        &mut self,
        atom: AtomId,
        replacement: AtomId,
    ) -> Result<(), GraphError> {
        self.remove_element_symbol_locants(atom);
        let locants = self.graph.atom(atom).locants.clone();
        self.graph.clear_locants(atom);
        for locant in locants {
            self.graph.add_locant(replacement, locant);
        }
        for bond_id in self.graph.atom(atom).bonds.clone() {
            let bond = self.graph.bond(bond_id).clone();
            let neighbour = bond.other_atom(atom).expect("incident bond");
            if let Some(parity) = &mut self.graph.atom_mut(neighbour).parity {
                for reference in &mut parity.atom_refs {
                    if *reference == Some(StereoReference::Atom(atom)) {
                        *reference = Some(StereoReference::Atom(replacement));
                        break;
                    }
                }
            }
            if let Some(stereo) = &mut self.graph.bond_mut(bond_id).stereo {
                for reference in &mut stereo.atom_refs {
                    if *reference == atom {
                        *reference = replacement;
                        break;
                    }
                }
            }
            self.create_bond(replacement, neighbour, bond.order)?;
        }
        self.remove_atom_and_associated_bonds(atom);
        Ok(())
    }

    pub fn copy_fragment(&mut self, fragment: FragmentId) -> Result<FragmentId, GraphError> {
        self.copy_and_relabel_fragment(fragment, 0)
    }

    pub fn copy_and_relabel_fragment(
        &mut self,
        fragment: FragmentId,
        primes_to_add: usize,
    ) -> Result<FragmentId, GraphError> {
        let primes = u32::try_from(primes_to_add).map_err(|_| error("Too many locant primes"))?;
        let copy = self.graph.copy_and_relabel_fragment(fragment, primes)?;
        self.register_fragment(copy);
        Ok(copy)
    }

    pub fn clone_element(
        &mut self,
        arena: &mut Arena,
        element: NodeId,
        primes_to_add: usize,
        suffix_map: &mut BTreeMap<NodeId, Vec<FragmentId>>,
    ) -> Result<NodeId, GraphError> {
        let clone = arena.copy(element);
        let originals = arena.descendants_named(element, "group");
        let copies = arena.descendants_named(clone, "group");
        let mut fragment_mapping = BTreeMap::new();
        let mut original_fragment_order = Vec::new();
        for (original, copy) in originals.into_iter().zip(copies) {
            let fragment = arena[original]
                .fragment
                .ok_or_else(|| error("OPSIN bug: Cloned group has no fragment"))?;
            let new_fragment = self.copy_and_relabel_fragment(fragment, primes_to_add)?;
            if !fragment_mapping.contains_key(&fragment) {
                original_fragment_order.push(fragment);
            }
            fragment_mapping.insert(fragment, new_fragment);
            self.fragment_tokens.insert(new_fragment, copy);
            arena[copy].fragment = Some(new_fragment);
            self.graph.fragment_mut(new_fragment).token_attributes = arena[copy]
                .attributes
                .iter()
                .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
                .collect();
            let suffixes = suffix_map
                .get(&original)
                .cloned()
                .ok_or_else(|| error("OPSIN bug: Cloned group has no suffix mapping"))?;
            let mut new_suffixes = Vec::with_capacity(suffixes.len());
            for suffix in suffixes {
                new_suffixes.push(self.copy_fragment(suffix)?);
            }
            suffix_map.insert(copy, new_suffixes);
        }
        let mut bonds = Vec::new();
        for fragment in original_fragment_order {
            for &bond in self.inter_fragment_bonds(fragment)? {
                if !bonds.contains(&bond) {
                    bonds.push(bond);
                }
            }
        }
        for bond_id in bonds {
            let bond = self.graph.bond(bond_id).clone();
            let from_fragment = self.graph.atom(bond.from).fragment;
            let to_fragment = self.graph.atom(bond.to).fragment;
            let Some(&new_from_fragment) = fragment_mapping.get(&from_fragment) else {
                return Err(error(
                    "An element that was a clone contained a bond that went outside the scope of the cloning",
                ));
            };
            let Some(&new_to_fragment) = fragment_mapping.get(&to_fragment) else {
                return Err(error(
                    "An element that was a clone contained a bond that went outside the scope of the cloning",
                ));
            };
            let from_position = self
                .graph
                .fragment(from_fragment)
                .atoms
                .iter()
                .position(|&id| id == bond.from)
                .expect("fragment contains atom");
            let to_position = self
                .graph
                .fragment(to_fragment)
                .atoms
                .iter()
                .position(|&id| id == bond.to)
                .expect("fragment contains atom");
            let from = self.graph.fragment(new_from_fragment).atoms[from_position];
            let to = self.graph.fragment(new_to_fragment).atoms[to_position];
            self.create_bond(from, to, bond.order)?;
        }
        Ok(clone)
    }

    pub fn convert_spare_valencies_to_double_bonds(&mut self) -> Result<(), GraphError> {
        for fragment in self.fragments.clone() {
            crate::fragment_tools::convert_spare_valencies_to_double_bonds(
                &mut self.graph,
                fragment,
            )?;
        }
        Ok(())
    }

    pub fn make_hydrogens_explicit(&mut self) -> Result<(), GraphError> {
        for fragment in self.fragments.clone() {
            self.graph.make_hydrogens_explicit(fragment)?;
        }
        Ok(())
    }
}

fn is_element_symbol_locant(locant: &str) -> bool {
    let symbol = locant.trim_end_matches('\'');
    !symbol.is_empty()
        && symbol.len() <= 2
        && symbol.as_bytes()[0].is_ascii_uppercase()
        && symbol.as_bytes()[1..].iter().all(u8::is_ascii_lowercase)
}
fn error(message: impl Into<String>) -> GraphError {
    GraphError(message.into())
}
