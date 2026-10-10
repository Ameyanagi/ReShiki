//! OPSIN's ordered fragment assembly operations.
//! Port of `StructureBuildingMethods.java`, OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::{
    ambiguity,
    build_results::BuildResults,
    build_state::BuildState,
    cycle_detector, fragment_tools as ft,
    graph::{AtomId, BondId, Element, FragmentId, Graph, GraphError, OutAtom},
    isotope_specification_parser,
    parse_tree::{Arena, NodeId},
    valence,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type Result<T> = std::result::Result<T, GraphError>;
pub fn error(message: impl Into<String>) -> GraphError {
    GraphError(message.into())
}

pub struct Assembly<'a> {
    pub state: &'a mut BuildState,
    pub arena: &'a mut Arena,
}

impl Assembly<'_> {
    pub fn graph(&self) -> &Graph {
        self.state.graph()
    }
    pub fn graph_mut(&mut self) -> &mut Graph {
        self.state.graph_mut()
    }
    pub fn attr(&self, node: NodeId, key: &str) -> Option<String> {
        self.arena[node].attribute(key).map(str::to_owned)
    }
    pub fn number(&self, node: NodeId, key: &str) -> Result<usize> {
        self.attr(node, key)
            .ok_or_else(|| error(format!("Missing {key} on {}", self.arena[node].name)))?
            .parse()
            .map_err(|_| error(format!("Invalid {key} on {}", self.arena[node].name)))
    }
    pub fn fragment(&self, group: NodeId) -> Result<FragmentId> {
        self.arena[group]
            .fragment
            .ok_or_else(|| error("Group has no constructed fragment"))
    }
    pub fn token(&self, fragment: FragmentId) -> Result<NodeId> {
        self.state
            .fragment_manager
            .token_for_fragment(fragment)
            .ok_or_else(|| error("Fragment has no corresponding group token"))
    }
    pub fn atom_by_locant(&self, fragment: FragmentId, locant: &str) -> Result<AtomId> {
        self.graph()
            .atom_by_locant(fragment, locant)
            .ok_or_else(|| error(format!("No atom with locant {locant}")))
    }
    pub fn children_any(&self, node: NodeId, names: &[&str]) -> Vec<NodeId> {
        self.arena[node]
            .children
            .iter()
            .copied()
            .filter(|&id| names.contains(&self.arena[id].name.as_str()))
            .collect()
    }
    pub fn group(&self, node: NodeId) -> Result<NodeId> {
        if self.arena[node].name == "bracket" {
            self.rightmost_group(node)
        } else {
            self.arena
                .first_child_named(node, "group")
                .ok_or_else(|| error("substituent/root is missing its group"))
        }
    }
    pub fn rightmost_group(&self, node: NodeId) -> Result<NodeId> {
        if self.arena[node].name == "bracket" {
            let child = self
                .children_any(node, &["bracket", "substituent", "root"])
                .last()
                .copied()
                .ok_or_else(|| error("Bracket contains no substituent or root"))?;
            self.rightmost_group(child)
        } else {
            self.arena
                .children_named(node, "group")
                .last()
                .copied()
                .ok_or_else(|| error("substituent/root is missing its group"))
        }
    }
    pub fn next_group(&self, mut node: NodeId) -> Option<NodeId> {
        if self.arena[node].name == "group" {
            node = self.arena[node].parent?;
        }
        loop {
            let parent = self.arena[node].parent?;
            if self.arena[parent].name == "molecule" {
                return None;
            }
            if let Some(mut next) = self.arena.next_sibling(node) {
                while let Some(&child) = self.arena[next].children.first() {
                    next = child;
                }
                let scope = self.arena[next].parent?;
                if let Some(group) = self.arena.first_child_named(scope, "group") {
                    return Some(group);
                }
                node = next;
            } else {
                node = parent;
            }
        }
    }
    fn previous_group(&self, mut node: NodeId) -> Option<NodeId> {
        if self.arena[node].name == "group" {
            node = self.arena[node].parent?;
        }
        loop {
            let parent = self.arena[node].parent?;
            if self.arena[parent].name == "wordRule" {
                return None;
            }
            if let Some(mut previous) = self.arena.previous_sibling(node) {
                while let Some(&child) = self.arena[previous].children.last() {
                    previous = child;
                }
                let scope = self.arena[previous].parent?;
                if let Some(&group) = self.arena.children_named(scope, "group").last() {
                    return Some(group);
                }
                node = previous;
            } else {
                node = parent;
            }
        }
    }
    fn potentially_can_substitute(&self, node: NodeId) -> bool {
        let Some(parent) = self.arena[node].parent else {
            return false;
        };
        let Some(index) = self.arena.index_of(parent, node) else {
            return false;
        };
        self.arena[parent].children[index + 1..]
            .iter()
            .any(|&child| self.arena[child].name != "hyphen")
    }
    pub fn clone_element(&mut self, node: NodeId, primes: usize) -> Result<NodeId> {
        let state = &mut self.state;
        let copy = state.fragment_manager.clone_element(
            self.arena,
            node,
            primes,
            &mut state.xml_suffix_map,
        )?;
        for stereo in self
            .arena
            .descendants_named(copy, crate::xml_declarations::STEREOCHEMISTRY_EL)
        {
            if let Some(locant) = self.attr(stereo, "locant") {
                self.arena[stereo].set_attribute("locant", locant + &"'".repeat(primes));
            }
        }
        Ok(copy)
    }
    pub fn resolve_word_or_bracket(&mut self, word: NodeId) -> Result<()> {
        if self.arena[word].name == "wordRule" {
            return Ok(());
        }
        if !matches!(self.arena[word].name.as_str(), "word" | "bracket") {
            return Err(error("A word or bracket is the expected input"));
        }
        self.recursively_resolve_locanted_features(word)?;
        self.recursively_resolve_unlocanted_features(word)?;
        for node in self
            .arena
            .descendants_named_any(word, &["bracket", "substituent", "root"])
        {
            if self.attr(node, "multiplier").is_some() {
                return Err(error(format!(
                    "Structure building problem: multiplier on :{} was never used",
                    self.arena[node].name
                )));
            }
        }
        let groups = self.arena.descendants_named(word, "group");
        for &group in groups.iter().take(groups.len().saturating_sub(1)) {
            if self.attr(group, "resolved").is_none() {
                return Err(error(format!(
                    "Structure building problem: Bond was not made from :{} but one should have been",
                    self.arena.value(group)
                )));
            }
        }
        Ok(())
    }
    pub fn recursively_resolve_locanted_features(&mut self, node: NodeId) -> Result<()> {
        if !matches!(self.arena[node].name.as_str(), "word" | "bracket") {
            return Err(error("A word or bracket is the expected input"));
        }
        for child in self
            .children_any(node, &["bracket", "substituent", "root"])
            .into_iter()
            .rev()
        {
            if self.arena[child].name == "bracket" {
                self.recursively_resolve_locanted_features(child)?;
                if self.potentially_can_substitute(child) {
                    self.perform_additive_operations(child)?;
                    self.perform_locanted_substitutive_operations(child)?;
                }
            } else {
                self.resolve_root_or_substituent_locanted(child)?;
            }
        }
        Ok(())
    }
    pub fn recursively_resolve_unlocanted_features(&mut self, node: NodeId) -> Result<()> {
        if !matches!(self.arena[node].name.as_str(), "word" | "bracket") {
            return Err(error("A word or bracket is the expected input"));
        }
        for child in self
            .children_any(node, &["bracket", "substituent", "root"])
            .into_iter()
            .rev()
        {
            if self.arena[child].name == "bracket" {
                self.recursively_resolve_unlocanted_features(child)?;
                if self.potentially_can_substitute(child) {
                    self.perform_unlocanted_substitutive_operations(child)?;
                }
            } else {
                self.resolve_root_or_substituent_unlocanted(child)?;
            }
        }
        Ok(())
    }
    pub fn resolve_root_or_substituent_unlocanted(&mut self, node: NodeId) -> Result<()> {
        let can_substitute = self.potentially_can_substitute(node);
        self.resolve_unlocanted_features(node)?;
        if can_substitute {
            self.perform_unlocanted_substitutive_operations(node)?;
        }
        Ok(())
    }
    pub fn resolve_root_or_substituent_locanted(&mut self, node: NodeId) -> Result<()> {
        self.resolve_locanted_features(node)?;
        if self.potentially_can_substitute(node) {
            self.perform_additive_operations(node)?;
            self.perform_locanted_substitutive_operations(node)?;
        }
        Ok(())
    }
    pub fn find_alternative_groups(&self, start: NodeId) -> Result<(Vec<NodeId>, Vec<NodeId>)> {
        let parent = self.arena[start]
            .parent
            .ok_or_else(|| error("Substituent has no parent"))?;
        let mut stack = vec![(parent, false)];
        let mut groups = Vec::new();
        let mut unlikely = Vec::new();
        let mut first = true;
        while let Some((node, disfavoured)) = stack.pop() {
            if self.arena[node].name == "group" {
                if disfavoured {
                    unlikely.push(node);
                } else {
                    groups.push(node);
                }
                continue;
            }
            for child in self.children_any(node, &["bracket", "substituent", "root"]) {
                if first && self.arena.index_of(node, child) <= self.arena.index_of(node, start) {
                    continue;
                }
                if self.attr(child, "multiplier").is_some() {
                    continue;
                }
                if self.arena[child].name == "bracket" {
                    stack.push((
                        child,
                        disfavoured || self.attr(child, "type").as_deref() != Some("implicit"),
                    ));
                } else {
                    stack.push((
                        self.group(child)?,
                        disfavoured || self.attr(child, "locant").is_some(),
                    ));
                }
            }
            first = false;
        }
        Ok((groups, unlikely))
    }
    pub fn find_alternative_fragments(&self, start: NodeId) -> Result<Vec<FragmentId>> {
        find_alternative_fragments(self.arena, start)
    }
    fn children_ignoring_implicit_brackets(&self, node: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        for &child in &self.arena[node].children {
            if self.arena[child].name == "bracket"
                && self.attr(child, "type").as_deref() == Some("implicit")
                && self.attr(child, "locant").is_none()
            {
                result.extend(self.children_ignoring_implicit_brackets(child));
            } else {
                result.push(child);
            }
        }
        result
    }
    pub fn find_fragment_with_locant(
        &self,
        start: NodeId,
        locant: &str,
    ) -> Result<Option<FragmentId>> {
        let parent = self.arena[start]
            .parent
            .ok_or_else(|| error("Substituent has no parent"))?;
        let mut stack = VecDeque::from([parent]);
        let mut first = true;
        let mut mononuclear = None;
        while let Some(node) = stack.pop_back() {
            if matches!(self.arena[node].name.as_str(), "substituent" | "root") {
                let fragment = self.fragment(self.group(node)?)?;
                if mononuclear.is_some() && self.attr(node, "locant").is_some() {
                    return Ok(mononuclear);
                }
                if self.graph().atom_by_locant(fragment, locant).is_some() {
                    if locant == "1" && self.graph().fragment(fragment).atoms.len() == 1 {
                        if mononuclear.is_none() {
                            mononuclear = Some(fragment);
                        }
                    } else {
                        return Ok(Some(fragment));
                    }
                }
                continue;
            } else if mononuclear.is_some() {
                return Ok(mononuclear);
            }
            let mut bracketed = Vec::new();
            let mut preferred = None;
            for child in self.children_any(node, &["bracket", "substituent", "root"]) {
                if first && self.arena.index_of(node, child) <= self.arena.index_of(node, start) {
                    continue;
                }
                if self.attr(child, "multiplier").is_some() {
                    continue;
                }
                let flattened = if self.arena[child].name == "bracket"
                    && self.attr(child, "type").as_deref() == Some("implicit")
                    && self.attr(child, "locant").is_none()
                {
                    self.children_ignoring_implicit_brackets(child)
                } else {
                    vec![child]
                };
                for descendant in flattened {
                    if self.arena[descendant].name == "bracket" {
                        bracketed.push(descendant);
                    } else if first
                        && preferred.is_none()
                        && self.attr(descendant, "locant").is_none()
                        && ft::is_numeric_locant(locant)
                    {
                        preferred = Some(descendant);
                    } else {
                        stack.push_back(descendant);
                    }
                }
            }
            if let Some(preferred) = preferred {
                stack.push_back(preferred);
            }
            for bracket in bracketed.into_iter().rev() {
                stack.push_front(bracket);
            }
            first = false;
        }
        Ok(mononuclear)
    }
    pub fn bracketed_primed_locant(&self, node: NodeId, locant: &str) -> Option<String> {
        let primes = locant.chars().rev().take_while(|&c| c == '\'').count();
        if primes == 0 {
            return None;
        }
        let mut depth = 0;
        let mut parent = self.arena[node].parent;
        while let Some(id) = parent {
            if self.arena[id].name != "bracket" {
                break;
            }
            if self.attr(id, "type").as_deref() != Some("implicit") {
                depth += 1;
            }
            parent = self.arena[id].parent;
        }
        (primes == depth).then(|| locant[..locant.len() - primes].to_string())
    }
    fn find_atoms_for_substitution(
        &self,
        node: NodeId,
        count: usize,
        order: i32,
    ) -> Result<Option<Vec<AtomId>>> {
        let (groups, unlikely) = self.find_alternative_groups(node)?;
        for (parents, preserve) in [
            (&groups, true),
            (&groups, false),
            (&unlikely, true),
            (&unlikely, false),
        ] {
            let mut root_handled = false;
            for (index, &group) in parents.iter().enumerate() {
                let fragment = self.fragment(group)?;
                let mut atoms = self.graph().fragment(fragment).atoms.clone();
                if self.arena[group]
                    .parent
                    .is_some_and(|p| self.arena[p].name == "root")
                {
                    if root_handled {
                        continue;
                    }
                    root_handled = true;
                    for &other in &parents[index + 1..] {
                        if self.arena[other]
                            .parent
                            .is_some_and(|p| self.arena[p].name == "root")
                        {
                            atoms.extend(&self.graph().fragment(self.fragment(other)?).atoms);
                        }
                    }
                }
                if let Some(found) = ft::find_n_atoms_for_substitution(
                    self.graph(),
                    &atoms,
                    self.graph().fragment(fragment).default_in_atom,
                    count,
                    order,
                    true,
                    preserve,
                )? {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }
    fn combine_out_atoms(&mut self, fragment: FragmentId, group: NodeId) -> Result<()> {
        let outs = self.graph().fragment(fragment).out_atoms.clone();
        if outs.len() <= 1
            || self.attr(group, "subType").as_deref() == Some("epoxyLike")
            || matches!(
                self.arena.value(group).as_str(),
                "oxy" | "thio" | "seleno" | "telluro"
            )
        {
            return Ok(());
        }
        let atom = outs[0].atom;
        if outs.iter().any(|out| out.atom != atom) {
            return Err(error(format!(
                "Substitutive bond formation failure: Fragment expected to have one OutAtom but had: {}",
                outs.len()
            )));
        }
        let valency = outs.iter().map(|out| out.valency).sum();
        for index in (0..outs.len()).rev() {
            self.graph_mut().remove_out_atom(fragment, index);
        }
        self.graph_mut().add_out_atom(fragment, atom, valency, true);
        Ok(())
    }
    fn perform_locanted_substitutive_operations(&mut self, node: NodeId) -> Result<()> {
        let group = self.group(node)?;
        if self.attr(group, "resolved").is_some() {
            return Ok(());
        }
        let fragment = self.fragment(group)?;
        let Some(mut locant) = self.attr(node, "locant") else {
            return Ok(());
        };
        if self.graph().fragment(fragment).out_atoms.is_empty() {
            return Ok(());
        }
        self.combine_out_atoms(fragment, group)?;
        if self.attr(node, "multiplier").is_some() {
            return self.multiply_out_and_substitute(node);
        }
        let mut parent = self.find_fragment_with_locant(node, &locant)?;
        if parent.is_none()
            && let Some(modified) = self.bracketed_primed_locant(node, &locant)
        {
            parent = self.find_fragment_with_locant(node, &modified)?;
            if parent.is_some() {
                locant = modified;
            }
        }
        let parent = parent.ok_or_else(|| {
            error(format!(
                "Cannot find in scope fragment with atom with locant {locant}."
            ))
        })?;
        self.arena[group].set_attribute("resolved", "yes");
        let parent_group = self.token(parent)?;
        let mut target = self.atom_by_locant(parent, &locant)?;
        if self.attr(parent_group, "acceptsAdditiveBonds").is_some()
            && !self.graph().fragment(parent).out_atoms.is_empty()
            && self.attr(parent_group, "isAMultiRadical").is_some()
            && self.graph().atom(target).out_valency > 0
            && self.graph().fragment(fragment).out_atoms[0].valency == 1
            && self.graph().fragment(parent).atoms.first() == Some(&target)
        {
            return self.join_fragments_additively(fragment, parent);
        }
        if self.attr(group, "subType").as_deref() == Some("phospho")
            && self.graph().fragment(fragment).out_atoms[0].valency == 1
            && self.graph().atom(target).element != Element::O
            && let Some(oxygen) = self.graph().neighbours(target).into_iter().find(|&atom| {
                let a = self.graph().atom(atom);
                a.element == Element::O
                    && a.bonds.len() == 1
                    && self.graph().bond(a.bonds[0]).order == 1
                    && a.out_valency == 0
                    && a.charge == 0
            })
        {
            target = oxygen;
        }
        self.join_fragments_substitutively(fragment, target)
    }
    fn perform_unlocanted_substitutive_operations(&mut self, node: NodeId) -> Result<()> {
        let group = self.group(node)?;
        if self.attr(group, "resolved").is_some() {
            return Ok(());
        }
        let fragment = self.fragment(group)?;
        if self.graph().fragment(fragment).out_atoms.is_empty() {
            return Ok(());
        }
        if self.attr(node, "locant").is_some() {
            return Err(error(
                "Substituent has an unused outAtom and locant after locanted substitution",
            ));
        }
        self.combine_out_atoms(fragment, group)?;
        if self.attr(node, "multiplier").is_some() {
            return self.multiply_out_and_substitute(node);
        }
        if self.attr(group, "subType").as_deref() == Some("perhalogeno") {
            self.perform_perhalogeno_substitution(fragment, node)?;
        } else {
            let targets = self
                .substitution_targets(node, fragment, 1)?
                .ok_or_else(|| {
                    error("Unlocanted substitution failed: unable to find suitable atom to bond to")
                })?;
            if ambiguity::is_substitution_ambiguous(self.graph(), &targets, 1)? {
                self.state.add_is_ambiguous(format!(
                    "Connection of {} to {}",
                    self.arena.value(group),
                    self.arena
                        .value(self.token(self.graph().atom(targets[0]).fragment)?)
                ));
            }
            self.join_fragments_substitutively(fragment, targets[0])?;
        }
        self.arena[group].set_attribute("resolved", "yes");
        Ok(())
    }
    fn substitution_targets(
        &self,
        node: NodeId,
        fragment: FragmentId,
        count: usize,
    ) -> Result<Option<Vec<AtomId>>> {
        let group = self.token(fragment)?;
        if self.attr(group, "subType").as_deref() == Some("phospho")
            && self.graph().fragment(fragment).out_atoms[0].valency == 1
            && let Some(&parent) = self.find_alternative_fragments(node)?.first()
        {
            let hydroxy = ft::find_hydroxy_groups(self.graph(), parent)?;
            if hydroxy.len() >= count {
                return Ok(Some(hydroxy));
            }
        }
        self.find_atoms_for_substitution(
            node,
            count,
            self.graph().fragment(fragment).out_atoms[0].valency,
        )
    }
    fn perform_perhalogeno_substitution(
        &mut self,
        fragment: FragmentId,
        node: NodeId,
    ) -> Result<()> {
        let mut targets = Vec::new();
        for parent in self.find_alternative_fragments(node)? {
            ft::convert_spare_valencies_to_double_bonds(self.graph_mut(), parent)?;
            for atom in self.graph().fragment(parent).atoms.clone() {
                let count = ft::calculate_substitutable_hydrogen_atoms(self.graph(), atom);
                if count > 0 && ft::is_characteristic_atom(self.graph(), atom) {
                    continue;
                }
                targets.extend(std::iter::repeat_n(atom, count.max(0) as usize));
            }
        }
        if targets.is_empty() {
            return Err(error(
                "Failed to find any substitutable hydrogen for perhalogeno substitution",
            ));
        }
        let mut halogens = vec![fragment];
        for _ in 1..targets.len() {
            halogens.push(self.state.fragment_manager.copy_fragment(fragment)?);
        }
        for (&halogen, &target) in halogens.iter().zip(&targets) {
            let from = self.graph_mut().remove_out_atom(halogen, 0).atom;
            self.state.fragment_manager.create_bond(from, target, 1)?;
        }
        for &halogen in &halogens[1..] {
            self.state
                .fragment_manager
                .incorporate_fragment(halogen, fragment)?;
        }
        Ok(())
    }
    fn multiply_out_and_substitute(&mut self, node: NodeId) -> Result<()> {
        let multiplier = self.number(node, "multiplier")?;
        self.arena[node].remove_attribute("multiplier");
        let locants = self
            .attr(node, "locant")
            .map(|value| value.split(',').map(str::to_owned).collect::<Vec<_>>());
        if locants
            .as_ref()
            .is_some_and(|locants| locants.len() != multiplier)
        {
            return Err(error(
                "Mismatch between multiplier and substitution locants",
            ));
        }
        let parent = self.arena[node]
            .parent
            .ok_or_else(|| error("Multiplied substituent has no parent"))?;
        let index = self.arena.index_of(parent, node).unwrap();
        self.arena.detach(node);
        let multiplier_node = self
            .arena
            .first_child_named(node, "multiplier")
            .ok_or_else(|| error("Multiplier not found where multiplier expected"))?;
        let cutoff = self.arena.index_of(node, multiplier_node).unwrap();
        let mut prefix = Vec::new();
        // Detachment mutates this child list, so iterate an ordered snapshot.
        let preceding_children = self.arena[node].children[..cutoff].to_vec();
        for child in preceding_children.into_iter().rev() {
            self.arena.detach(child);
            prefix.push(child);
        }
        self.arena.detach(multiplier_node);
        let mut copies = Vec::new();
        for i in (0..multiplier).rev() {
            let copy = if i == 0 {
                node
            } else {
                self.clone_element(node, i)?
            };
            copies.push(copy);
            if let Some(locants) = &locants {
                self.arena.insert_child(parent, copy, index);
                self.arena[copy].set_attribute("locant", &locants[i]);
                self.perform_locanted_substitutive_operations(copy)?;
                self.arena.detach(copy);
            }
        }
        if locants.is_none() {
            let first = *copies
                .first()
                .ok_or_else(|| error("Multiplier must be positive"))?;
            self.arena.insert_child(parent, first, index);
            self.substitute_multiplied(&copies)?;
            self.arena.detach(first);
        }
        for copy in copies {
            self.arena.insert_child(parent, copy, index);
        }
        for child in prefix {
            self.arena.insert_child(node, child, 0);
        }
        Ok(())
    }
    fn substitute_multiplied(&mut self, copies: &[NodeId]) -> Result<()> {
        let node = copies[0];
        let group = self.group(node)?;
        let fragment = self.fragment(group)?;
        if self.graph().fragment(fragment).out_atoms.is_empty() {
            return Ok(());
        }
        if self.attr(node, "locant").is_some() {
            return Err(error("Unused locant after locanted substitution"));
        }
        if self.attr(group, "subType").as_deref() == Some("perhalogeno") {
            return Err(error("Perhalogeno groups cannot be multiplied"));
        }
        self.combine_out_atoms(fragment, group)?;
        let mut targets = self
            .substitution_targets(node, fragment, copies.len())?
            .ok_or_else(|| {
                error("Unlocanted substitution failed: unable to find suitable atoms")
            })?;
        if ambiguity::is_substitution_ambiguous(self.graph(), &targets, copies.len())? {
            self.state.add_is_ambiguous(format!(
                "Connection of {} to {}",
                self.arena.value(group),
                self.arena
                    .value(self.token(self.graph().atom(targets[0]).fragment)?)
            ));
            if let Some(preferred) =
                ambiguity::use_atom_environments_to_give_plausible_substitution(
                    self.graph(),
                    &targets,
                    copies.len(),
                )?
            {
                targets = preferred;
            }
        }
        for (&copy, &target) in copies.iter().zip(&targets) {
            let group = self.group(copy)?;
            let fragment = self.fragment(group)?;
            self.combine_out_atoms(fragment, group)?;
            self.join_fragments_substitutively(fragment, target)?;
            self.arena[group].set_attribute("resolved", "yes");
        }
        Ok(())
    }
    pub fn resolve_locanted_features(&mut self, node: NodeId) -> Result<()> {
        let groups = self.arena.children_named(node, "group");
        if groups.len() != 1 {
            return Err(error(
                "Each sub or root should only have one group element. This indicates a bug in OPSIN",
            ));
        }
        let group = groups[0];
        let fragment = self.fragment(group)?;
        let children = self.arena[node].children.clone();
        let mut unlocanted_deoxy = BTreeMap::new();
        let mut unlocanted_removal = BTreeMap::new();
        let mut dehydro = Vec::new();
        for prefix in children
            .iter()
            .copied()
            .rev()
            .filter(|&id| self.arena[id].name == "subtractivePrefix")
            .collect::<Vec<_>>()
        {
            let kind = self.attr(prefix, "type").unwrap_or_default();
            let locant = self.attr(prefix, "locant");
            let element = self
                .attr(prefix, "value")
                .and_then(|s| Element::from_symbol(&s));
            match kind.as_str() {
                "deoxy"=>{let element=element.ok_or_else(||error("Invalid subtractive element"))?;if let Some(locant)=locant{self.apply_subtractive_prefix(fragment,element,&locant)?;}else{*unlocanted_deoxy.entry(element).or_insert(0)+=1;}}
                "anhydro"=>self.apply_anhydro_prefix(fragment,prefix)?,
                "dehydro"=>dehydro.push(self.atom_by_locant(fragment,&locant.ok_or_else(||error("locants are assumed to be required for the use of dehydro to be unambiguous"))?)?),
                "heteratomRemoval"=>{let element=element.ok_or_else(||error("Invalid subtractive element"))?;if let Some(locant)=locant{self.apply_heteroatom_removal(fragment,element,&locant)?;}else{*unlocanted_removal.entry(element).or_insert(0)+=1;}}
                _=>return Err(error(format!("OPSIN bug: Unexpected subtractive prefix type: {kind}"))),
            }
            self.arena.detach(prefix);
        }
        for (element, count) in unlocanted_deoxy {
            self.apply_unlocanted_subtractive_prefixes(fragment, element, count)?;
        }
        for (element, count) in unlocanted_removal {
            self.apply_unlocanted_heteroatom_removal(fragment, element, count)?;
        }
        if !dehydro.is_empty() {
            if self.attr(group, "type").as_deref() == Some("carbohydrate")
                && dehydro.iter().collect::<BTreeSet<_>>().len() == dehydro.len()
            {
                for atom in dehydro {
                    let hydroxy = ft::find_hydroxy_like_terminal_atoms(
                        self.graph(),
                        &self.graph().neighbours(atom),
                        Element::O,
                    );
                    let oxygen = *hydroxy.first().ok_or_else(|| {
                        error("Dehydro atom did not have a hydroxy group to convert to a ketose")
                    })?;
                    let bond = self.graph().atom(oxygen).bonds[0];
                    self.graph_mut().bond_mut(bond).order = 2;
                }
            } else {
                let mut double = Vec::new();
                let mut triple = Vec::new();
                for atom in dehydro {
                    if self.graph().atom(atom).spare_valency {
                        triple.push(atom);
                    } else {
                        self.graph_mut().atom_mut(atom).spare_valency = true;
                        double.push(atom);
                    }
                }
                for atom in double {
                    if !self
                        .graph()
                        .neighbours(atom)
                        .iter()
                        .any(|&other| self.graph().atom(other).spare_valency)
                    {
                        return Err(error(
                            "Unexpected use of dehydro; two adjacent atoms were not unsaturated such as to form a double bond",
                        ));
                    }
                }
                self.add_dehydro_triple_bonds(triple)?;
            }
        }
        for hydrogen in children
            .iter()
            .copied()
            .rev()
            .filter(|&id| {
                matches!(
                    self.arena[id].name.as_str(),
                    "hydro" | "indicatedHydrogen" | "addedHydrogen"
                )
            })
            .collect::<Vec<_>>()
        {
            if let Some(locant) = self.attr(hydrogen, "locant") {
                let atom = self.atom_by_locant(fragment, &locant)?;
                if self.graph().atom(atom).spare_valency {
                    self.graph_mut().atom_mut(atom).spare_valency = false;
                } else {
                    let spiro_bug =
                        self.arena.value(group).starts_with("spiro")
                            && self.arena.children_named(node, "suffix").into_iter().any(
                                |suffix| self.attr(suffix, "locant").as_deref() == Some(&locant),
                            );
                    if !spiro_bug {
                        return Err(error(format!(
                            "hydrogen addition at locant: {locant} was requested, but this atom is not unsaturated"
                        )));
                    }
                }
                self.arena.detach(hydrogen);
            }
        }
        for unsaturator in children
            .iter()
            .copied()
            .rev()
            .filter(|&id| self.arena[id].name == "unsaturator")
            .collect::<Vec<_>>()
        {
            let order = self.number(unsaturator, "value")? as u8;
            if order <= 1 {
                self.arena.detach(unsaturator);
                continue;
            }
            if let Some(locant) = self.attr(unsaturator, "locant") {
                if let Some((base, compound)) = compound_locant(&locant) {
                    let atom = self.atom_by_locant(fragment, &base)?;
                    ft::unsaturate_to_locant(self.graph_mut(), atom, &compound, order, fragment)?;
                } else {
                    let atom = self.atom_by_locant(fragment, &locant)?;
                    ft::unsaturate(self.graph_mut(), atom, order, fragment)?;
                }
                self.arena.detach(unsaturator);
            }
        }
        for hetero in children
            .iter()
            .copied()
            .rev()
            .filter(|&id| self.arena[id].name == "heteroatom")
            .collect::<Vec<_>>()
        {
            if let Some(locant) = self.attr(hetero, "locant") {
                let replacement = self
                    .state
                    .fragment_manager
                    .get_heteroatom(&self.attr(hetero, "value").unwrap_or_default())?;
                let target = self.atom_by_locant(fragment, &locant)?;
                if self.graph().atom(replacement).element == self.graph().atom(target).element
                    && self.graph().atom(replacement).charge == self.graph().atom(target).charge
                {
                    return Err(error(format!(
                        "The replacement term {} was used on an atom that already is a {}",
                        self.arena.value(hetero),
                        self.graph().atom(replacement).element
                    )));
                }
                self.state
                    .fragment_manager
                    .replace_atom_with_atom(target, replacement, true)?;
                if self.attr(hetero, "lambda").is_some() {
                    let lambda = self.number(hetero, "lambda")? as i32;
                    self.graph_mut().atom_mut(target).lambda_convention_valency = Some(lambda);
                }
                self.arena.detach(hetero);
            }
        }
        self.apply_isotope_specifications(
            fragment,
            &children
                .iter()
                .copied()
                .filter(|&id| self.arena[id].name == "isotopeSpecification")
                .collect::<Vec<_>>(),
            true,
        )?;
        Ok(())
    }
    pub fn apply_subtractive_prefix(
        &mut self,
        fragment: FragmentId,
        element: Element,
        locant: &str,
    ) -> Result<()> {
        let adjacent = self.atom_by_locant(fragment, locant)?;
        let applicable = ft::find_hydroxy_like_terminal_atoms(
            self.graph(),
            &self.graph().neighbours(adjacent),
            element,
        );
        let atom=*applicable.first().ok_or_else(||error(format!("Unable to find terminal atom of type: {element} at locant {locant} for subtractive nomenclature")))?;
        self.remove_subtractive_atom(fragment, atom)
    }
    fn remove_subtractive_atom(&mut self, fragment: FragmentId, atom: AtomId) -> Result<()> {
        if ft::is_functional_atom(self.graph(), atom) {
            let adjacent = self.graph().neighbours(atom)[0];
            self.graph_mut()
                .fragment_mut(fragment)
                .functional_atoms
                .retain(|&a| a != atom);
            self.graph_mut()
                .fragment_mut(fragment)
                .functional_atoms
                .push(adjacent);
        }
        self.state.fragment_manager.remove_terminal_atom(atom)
    }
    pub fn apply_unlocanted_subtractive_prefixes(
        &mut self,
        fragment: FragmentId,
        element: Element,
        count: usize,
    ) -> Result<()> {
        let atoms = ft::find_hydroxy_like_terminal_atoms(
            self.graph(),
            &self.graph().fragment(fragment).atoms,
            element,
        );
        if atoms.len() < count || atoms.is_empty() {
            return Err(error(format!(
                "Unable to find terminal atom of type: {element} for subtractive nomenclature"
            )));
        }
        if ambiguity::is_substitution_ambiguous(self.graph(), &atoms, count)? {
            self.state
                .add_is_ambiguous("Group to remove with subtractive prefix");
        }
        for &atom in atoms.iter().take(count) {
            self.remove_subtractive_atom(fragment, atom)?;
        }
        Ok(())
    }
    fn apply_anhydro_prefix(&mut self, fragment: FragmentId, prefix: NodeId) -> Result<()> {
        let element = self
            .attr(prefix, "value")
            .and_then(|s| Element::from_symbol(&s))
            .ok_or_else(|| error("Invalid anhydro element"))?;
        let locants = self
            .attr(prefix, "locant")
            .ok_or_else(|| error("Two locants are required before an anhydro prefix"))?;
        let locants = locants.split(',').collect::<Vec<_>>();
        if locants.len() != 2 {
            return Err(error("Two locants are required before an anhydro prefix"));
        }
        let a = self.atom_by_locant(fragment, locants[0])?;
        let b = self.atom_by_locant(fragment, locants[1])?;
        let first = ft::find_hydroxy_like_terminal_atoms(
            self.graph(),
            &self.graph().neighbours(a),
            element,
        );
        let first = *first
            .first()
            .ok_or_else(|| error("Unable to find terminal atom for subtractive nomenclature"))?;
        self.state.fragment_manager.remove_terminal_atom(first)?;
        let second = ft::find_hydroxy_like_terminal_atoms(
            self.graph(),
            &self.graph().neighbours(b),
            element,
        );
        let second = *second
            .first()
            .ok_or_else(|| error("Unable to find terminal atom for subtractive nomenclature"))?;
        self.state.fragment_manager.create_bond(a, second, 1)?;
        Ok(())
    }
    fn add_dehydro_triple_bonds(&mut self, mut atoms: Vec<AtomId>) -> Result<()> {
        if atoms.iter().collect::<BTreeSet<_>>().len() != atoms.len() {
            return Err(error(
                "locants specified for dehydro specify the same atom too many times",
            ));
        }
        while let Some(&atom) = atoms.last() {
            let other=self.graph().neighbours(atom).into_iter().find(|other|atoms.contains(other)).ok_or_else(||error("dehydro indicated atom should form a triple bond but no adjacent atoms also had hydrogen removed!"))?;
            atoms.pop();
            atoms.retain(|&a| a != other);
            let bond = self.graph().bond_between(atom, other).unwrap();
            self.graph_mut().bond_mut(bond).order = 3;
            self.graph_mut().atom_mut(atom).spare_valency = false;
            self.graph_mut().atom_mut(other).spare_valency = false;
        }
        Ok(())
    }
    fn remove_element_locants(&mut self, atom: AtomId) {
        for locant in self.graph().atom(atom).locants.clone() {
            if ft::is_element_symbol_locant(&locant) {
                self.graph_mut().remove_locant(atom, &locant);
            }
        }
    }
    fn apply_heteroatom_removal(
        &mut self,
        fragment: FragmentId,
        element: Element,
        locant: &str,
    ) -> Result<()> {
        let atom = self.atom_by_locant(fragment, locant)?;
        if self.graph().atom(atom).element != element {
            return Err(error(format!(
                "Removal of {element} requested, but the atom at locant {locant} is not this element"
            )));
        }
        self.graph_mut().atom_mut(atom).element = Element::C;
        self.remove_element_locants(atom);
        Ok(())
    }
    fn apply_unlocanted_heteroatom_removal(
        &mut self,
        fragment: FragmentId,
        element: Element,
        count: usize,
    ) -> Result<()> {
        let atoms = self
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .copied()
            .filter(|&a| self.graph().atom(a).element == element)
            .collect::<Vec<_>>();
        if atoms.len() < count || atoms.is_empty() {
            return Err(error(format!(
                "Unable to find sufficient atoms of element: {element} for subtractive nomenclature"
            )));
        }
        if ambiguity::is_substitution_ambiguous(self.graph(), &atoms, count)? {
            self.state
                .add_is_ambiguous("Group to remove with subtractive prefix");
        }
        for &atom in atoms.iter().take(count) {
            self.graph_mut().atom_mut(atom).element = Element::C;
            self.remove_element_locants(atom);
        }
        Ok(())
    }
    pub fn resolve_unlocanted_features(&mut self, node: NodeId) -> Result<()> {
        let groups = self.arena.children_named(node, "group");
        if groups.len() != 1 {
            return Err(error(
                "Each sub or root should only have one group element. This indicates a bug in OPSIN",
            ));
        }
        let fragment = self.fragment(groups[0])?;
        let mut orders = Vec::new();
        let mut hetero = Vec::new();
        let mut hydrogen = Vec::new();
        let mut isotopes = Vec::new();
        for child in self.arena[node].children.clone() {
            match self.arena[child].name.as_str() {
                "unsaturator" => {
                    let order = self.number(child, "value")? as u8;
                    if order > 1 {
                        orders.push(order);
                    }
                    self.arena.detach(child);
                }
                "heteroatom" => {
                    hetero.push(child);
                    self.arena.detach(child);
                }
                "hydro" | "indicatedHydrogen" | "addedHydrogen" => {
                    hydrogen.push(child);
                    self.arena.detach(child);
                }
                "isotopeSpecification" => isotopes.push(child),
                _ => {}
            }
        }
        if !hydrogen.is_empty() {
            self.apply_unlocanted_hydro(fragment, &hydrogen)?;
        }
        for order in [3, 2] {
            let count = orders.iter().filter(|&&o| o == order).count();
            if count > 0 {
                self.unsaturate_bonds(fragment, order, count)?;
            }
        }
        if orders.iter().any(|&o| o != 2 && o != 3) {
            return Err(error("Unexpected unsaturation bond order"));
        }
        if !hetero.is_empty() {
            self.apply_unlocanted_heteroatoms(fragment, &hetero)?;
        }
        self.apply_isotope_specifications(fragment, &isotopes, false)?;
        for index in 0..self.graph().fragment(fragment).out_atoms.len() {
            let out = self.graph().fragment(fragment).out_atoms[index].clone();
            if !out.explicitly_set {
                let atom = self.find_atom_for_unlocanted_radical(fragment, &out)?;
                self.graph_mut().set_out_atom_target(fragment, index, atom);
                self.graph_mut()
                    .set_out_atom_explicit(fragment, index, true);
            }
        }
        Ok(())
    }
    fn apply_unlocanted_hydro(&mut self, fragment: FragmentId, hydrogen: &[NodeId]) -> Result<()> {
        let mut accepting = Vec::new();
        let mut implicitly_removed = BTreeSet::new();
        for atom in self.graph().fragment(fragment).atoms.clone() {
            if self.graph().atom(atom).atom_type == "suffix" {
                continue;
            }
            ft::ensure_spare_valency_consistent_with_valency(self.graph_mut(), atom, false)?;
            if self.graph().atom(atom).spare_valency {
                accepting.push(atom);
                ft::ensure_spare_valency_consistent_with_valency(self.graph_mut(), atom, true)?;
                if !self.graph().atom(atom).spare_valency {
                    implicitly_removed.insert(atom);
                }
            }
        }
        if hydrogen.iter().any(|&h| self.arena.value(h) == "perhydro") {
            if hydrogen.len() != 1 {
                return Err(error(
                    "Unexpected indication of hydrogen when perhydro makes such indication redundant",
                ));
            }
            for atom in accepting {
                self.graph_mut().atom_mut(atom).spare_valency = false;
            }
            return Ok(());
        }
        let (mut definite, mut other) = (Vec::new(), Vec::new());
        for atom in accepting {
            if !implicitly_removed.contains(&atom)
                && ft::intra_fragment_neighbours(self.graph(), atom)
                    .iter()
                    .any(|&n| self.graph().atom(n).spare_valency)
            {
                definite.push(atom);
            } else {
                other.push(atom);
            }
        }
        let mut prioritised = definite.clone();
        prioritised.append(&mut other);
        if hydrogen.len() > prioritised.len() {
            return Err(error(format!(
                "Cannot find atom to add hydrogen to ({} hydrogens requested but only {} positions that can be hydrogenated)",
                hydrogen.len(),
                prioritised.len()
            )));
        }
        if definite.len() as isize - hydrogen.len() as isize > 1
            && !(ambiguity::all_atoms_equivalent(self.graph(), &definite)?
                && (hydrogen.len() == 1 || hydrogen.len() + 1 == definite.len()))
        {
            self.state.add_is_ambiguous(format!(
                "Ambiguous choice of positions to add hydrogen to on {}",
                self.arena.value(self.token(fragment)?)
            ));
        }
        for atom in prioritised.into_iter().take(hydrogen.len()) {
            self.graph_mut().atom_mut(atom).spare_valency = false;
        }
        Ok(())
    }
    fn unsaturate_bonds(&mut self, fragment: FragmentId, order: u8, count: usize) -> Result<()> {
        let mut possible = find_bonds_to_unsaturate(self.graph(), fragment, order, false, &[]);
        let mut alternative = Vec::new();
        if possible.len() < count {
            possible = find_bonds_to_unsaturate(self.graph(), fragment, order, true, &[]);
        } else {
            alternative = find_bonds_to_unsaturate(self.graph(), fragment, order, false, &possible);
        }
        if possible.len() < count {
            return Err(error(format!(
                "Failed to find bond to change to a bond of order: {order}"
            )));
        }
        if possible.len() > count {
            let bond = self.graph().bond(possible[0]);
            let f = self.graph().fragment(fragment);
            let cyclic = count == 1
                && matches!(f.sub_type.as_str(), "alkaneStem" | "heteroStem")
                && self.graph().atom(bond.from).in_cycle
                && self.graph().atom(bond.to).in_cycle
                && (f.atoms.first() == Some(&bond.from) || f.atoms.first() == Some(&bond.to));
            if !cyclic && f.sub_type != "hantzschWidman" {
                let unambiguous = if alternative.len() >= count {
                    let mut all = possible.clone();
                    all.extend(alternative);
                    count == 1 && ambiguity::all_bonds_equivalent(self.graph(), &all)?
                } else {
                    (count == 1 || count + 1 == possible.len())
                        && ambiguity::all_bonds_equivalent(self.graph(), &possible)?
                };
                if !unambiguous {
                    self.state.add_is_ambiguous(format!(
                        "Unsaturation of bonds of {}",
                        self.arena.value(self.token(fragment)?)
                    ));
                }
            }
        }
        for &bond in possible.iter().take(count) {
            self.graph_mut().bond_mut(bond).order = order;
        }
        Ok(())
    }
    fn apply_unlocanted_heteroatoms(
        &mut self,
        fragment: FragmentId,
        hetero: &[NodeId],
    ) -> Result<()> {
        let mut descriptions = BTreeMap::new();
        for &node in hetero {
            *descriptions
                .entry((
                    self.attr(node, "value").unwrap_or_default(),
                    self.attr(node, "lambda"),
                ))
                .or_insert(0usize) += 1;
        }
        for ((smiles, lambda), count) in descriptions {
            let replacement = self.state.fragment_manager.get_heteroatom(&smiles)?;
            let el = self.graph().atom(replacement).element;
            let charge = self.graph().atom(replacement).charge;
            let candidates = self
                .graph()
                .fragment(fragment)
                .atoms
                .iter()
                .copied()
                .filter(|&atom| {
                    let a = self.graph().atom(atom);
                    a.atom_type != "suffix"
                        && !(el == a.element && charge == a.charge)
                        && (a.element == Element::C
                            || el == Element::C
                            || a.element == Element::O
                                && matches!(el, Element::S | Element::Se | Element::Te))
                        && valence::check_valency_available_for_replacement(
                            self.graph(),
                            atom,
                            replacement,
                        )
                })
                .collect::<Vec<_>>();
            if candidates.len() < count {
                return Err(error(
                    "Cannot find suitable atom for heteroatom replacement",
                ));
            }
            let f = self.graph().fragment(fragment);
            let cyclic = count == 1
                && matches!(f.sub_type.as_str(), "alkaneStem" | "heteroStem")
                && f.atoms
                    .first()
                    .is_some_and(|&a| self.graph().atom(a).in_cycle)
                && f.atoms.first() == candidates.first();
            if candidates.len() > count
                && !cyclic
                && !(ambiguity::all_atoms_equivalent(self.graph(), &candidates)?
                    && (count == 1 || count + 1 == candidates.len()))
            {
                self.state.add_is_ambiguous(format!(
                    "Heteroatom replacement on {}",
                    self.arena.value(self.token(fragment)?)
                ));
            }
            for &atom in candidates.iter().take(count) {
                self.state
                    .fragment_manager
                    .replace_atom_with_atom(atom, replacement, true)?;
                if let Some(lambda) = &lambda {
                    self.graph_mut().atom_mut(atom).lambda_convention_valency = Some(
                        lambda
                            .parse()
                            .map_err(|_| error("Invalid lambda convention valency"))?,
                    );
                }
            }
        }
        Ok(())
    }
    fn apply_isotope_specifications(
        &mut self,
        fragment: FragmentId,
        isotopes: &[NodeId],
        locanted: bool,
    ) -> Result<()> {
        for &node in isotopes.iter().rev() {
            let spec = isotope_specification_parser::parse_isotope_specification(self.arena, node)
                .map_err(|e| error(e.0))?;
            if spec.locants.is_some() != locanted {
                continue;
            }
            let targets = if let Some(locants) = spec.locants {
                let mut atoms = Vec::new();
                for locant in locants {
                    let atom = self.atom_by_locant(fragment, &locant)?;
                    if spec.element != Element::H && self.graph().atom(atom).element != spec.element
                    {
                        return Err(error(format!(
                            "The atom at locant: {locant} was not a {}",
                            spec.element
                        )));
                    }
                    atoms.push(atom);
                }
                atoms
            } else if spec.element == Element::H {
                let f = self.graph().fragment(fragment);
                let atoms=ft::find_n_atoms_for_substitution(self.graph(),&f.atoms,f.default_in_atom,spec.multiplier,1,true,false)?.ok_or_else(||error("Failed to find sufficient hydrogen atoms for unlocanted hydrogen isotope replacement"))?;
                if ambiguity::is_substitution_ambiguous(self.graph(), &atoms, spec.multiplier)?
                    && !self.cas_isotope_special_case(fragment, &atoms, spec.multiplier)
                {
                    self.state.add_is_ambiguous(format!(
                        "Position of hydrogen isotope on {}",
                        self.arena.value(self.token(fragment)?)
                    ));
                }
                atoms.into_iter().take(spec.multiplier).collect()
            } else {
                let atoms = self
                    .graph()
                    .fragment(fragment)
                    .atoms
                    .iter()
                    .copied()
                    .filter(|&a| self.graph().atom(a).element == spec.element)
                    .collect::<Vec<_>>();
                if atoms.len() < spec.multiplier {
                    return Err(error(format!(
                        "Failed to find sufficient atoms for {} isotope replacement",
                        spec.element
                    )));
                }
                if ambiguity::is_substitution_ambiguous(self.graph(), &atoms, spec.multiplier)? {
                    self.state.add_is_ambiguous(format!(
                        "Position of isotope on {}",
                        self.arena.value(self.token(fragment)?)
                    ));
                }
                atoms.into_iter().take(spec.multiplier).collect()
            };
            for atom in targets {
                if spec.element == Element::H {
                    let hydrogen = self
                        .state
                        .fragment_manager
                        .create_atom(Element::H, fragment);
                    self.graph_mut().atom_mut(hydrogen).isotope = Some(spec.isotope);
                    self.state.fragment_manager.create_bond(atom, hydrogen, 1)?;
                } else {
                    self.graph_mut().atom_mut(atom).isotope = Some(spec.isotope);
                }
            }
            self.arena.detach(node);
        }
        Ok(())
    }
    fn cas_isotope_special_case(
        &self,
        fragment: FragmentId,
        targets: &[AtomId],
        count: usize,
    ) -> bool {
        let atoms = &self.graph().fragment(fragment).atoms;
        let Some(&first) = atoms.first() else {
            return false;
        };
        if count != 1 || targets.first() != Some(&first) {
            return false;
        }
        if atoms.len() == 2 {
            return self.graph().atom(first).element == self.graph().atom(atoms[1]).element;
        }
        self.graph().atom(first).in_cycle
            && atoms.iter().skip(1).all(|&a| {
                self.graph().atom(a).element == self.graph().atom(first).element
                    && ft::intra_fragment_incoming_valency(self.graph(), a)
                        == ft::intra_fragment_incoming_valency(self.graph(), first)
                    && self.graph().atom(a).spare_valency == self.graph().atom(first).spare_valency
            })
    }
    pub fn find_atom_for_unlocanted_radical(
        &mut self,
        fragment: FragmentId,
        out: &OutAtom,
    ) -> Result<AtomId> {
        self.find_radical_position(fragment, out, true)
    }
    fn find_radical_position(
        &mut self,
        fragment: FragmentId,
        out: &OutAtom,
        take_out: bool,
    ) -> Result<AtomId> {
        let f = self.graph().fragment(fragment);
        let possible=ft::find_n_atoms_for_substitution(self.graph(),&f.atoms,Some(out.atom),1,out.valency,take_out,false)?.ok_or_else(||error("Failed to assign all unlocanted radicals to actual atoms without violating valency"))?;
        if !(matches!(f.sub_type.as_str(), "alkaneStem" | "heteroStem")
            && possible.first() == f.atoms.first())
            && ambiguity::is_substitution_ambiguous(self.graph(), &possible, 1)?
        {
            self.state.add_is_ambiguous(format!(
                "Positioning of radical on: {}",
                self.arena.value(self.token(fragment)?)
            ));
        }
        Ok(possible[0])
    }
    fn levels_to_word(&self, mut node: NodeId) -> Option<usize> {
        let mut levels = 0;
        while self.arena[node].name != "word" {
            node = self.arena[node].parent?;
            levels += 1;
        }
        Some(levels)
    }
    fn has_root_like_or_multiradical_group(&self, node: NodeId) -> Result<bool> {
        if self.attr(node, "inLocants").is_some() {
            return Ok(true);
        }
        for group in self.arena.descendants_named(node, "group") {
            let count = self.graph().fragment(self.fragment(group)?).out_atoms.len();
            if self.attr(group, "isAMultiRadical").is_some() {
                if count >= 1 {
                    return Ok(true);
                }
            } else if count == 0 && self.attr(group, "resolved").is_none() {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn additive_parent_eligible(
        &self,
        fragment: FragmentId,
        multiplier: Option<usize>,
    ) -> Result<bool> {
        let group = self.token(fragment)?;
        let count = self.graph().fragment(fragment).out_atoms.len();
        let enough = if let Some(n) = multiplier {
            count >= n || self.attr(group, "resolved").is_some() && count > n
        } else {
            count > 1 || self.attr(group, "resolved").is_some() && count >= 1
        };
        Ok(self.attr(group, "acceptsAdditiveBonds").is_some()
            && self.attr(group, "isAMultiRadical").is_some()
            && enough)
    }
    fn perform_additive_operations(&mut self, node: NodeId) -> Result<()> {
        if self.attr(node, "locant").is_some() {
            return Ok(());
        }
        let group = self.group(node)?;
        if self.attr(group, "resolved").is_some() {
            return Ok(());
        }
        let fragment = self.fragment(group)?;
        let count = self.graph().fragment(fragment).out_atoms.len();
        if count == 0 {
            return Ok(());
        }
        let multiplier = self
            .attr(node, "multiplier")
            .map(|_| self.number(node, "multiplier"))
            .transpose()?;
        if multiplier.is_none() {
            if let Some(next) = self.arena.next_sibling(node)
                && self.attr(next, "multiplier").is_some()
            {
                let n = self.number(next, "multiplier")?;
                if (count >= n
                    || count == 1
                        && self.graph().fragment(fragment).out_atoms[0].valency == n as i32)
                    && self.has_root_like_or_multiradical_group(next)?
                {
                    if count == 1 {
                        ft::split_out_atom_into_valency_one_out_atoms(
                            self.graph_mut(),
                            fragment,
                            0,
                        );
                    }
                    let results = BuildResults::from_node(
                        self.arena,
                        self.graph(),
                        self.arena[group]
                            .parent
                            .ok_or_else(|| error("Group has no parent"))?,
                    )?;
                    return self.perform_multiplicative_operations(results, next);
                }
            }
            if self.attr(group, "isAMultiRadical").is_some() {
                if let Some(next_fragment) = self.next_in_scope_multivalent_fragment(node)? {
                    let next_group = self.token(next_fragment)?;
                    let parent = self.arena[next_group]
                        .parent
                        .ok_or_else(|| error("Multiradical group has no parent"))?;
                    if self.state.current_word_rule.as_deref() != Some("polymer") {
                        if self.attr(next_group, "iminoLike").is_some() {
                            let adjacent = self
                                .next_group(node)
                                .map(|g| self.fragment(g))
                                .transpose()?;
                            if adjacent != Some(next_fragment)
                                && (self.potentially_can_substitute(parent)
                                    || self.arena[parent]
                                        .parent
                                        .is_some_and(|p| self.potentially_can_substitute(p)))
                            {
                                return Ok(());
                            }
                        }
                        if self.attr(group, "iminoLike").is_some()
                            && self.levels_to_word(group) > self.levels_to_word(next_group)
                        {
                            return Ok(());
                        }
                    }
                    if self.attr(parent, "multiplier").is_some() {
                        return Err(error(
                            "Attempted to form additive bond to a multiplied component",
                        ));
                    }
                    self.arena[group].set_attribute("resolved", "yes");
                    self.join_fragments_additively(fragment, next_fragment)?;
                }
                return Ok(());
            }
        }
        let siblings = self.find_alternative_fragments(node)?;
        let Some(&last) = siblings.last() else {
            return Ok(());
        };
        if self.additive_parent_eligible(last, multiplier)? {
            let target = self.graph().fragment(last).out_atoms[0].atom;
            if ft::calculate_substitutable_hydrogen_atoms(self.graph(), target) == 0 {
                self.arena[group].set_attribute("resolved", "yes");
                if multiplier.is_some() {
                    self.multiply_out_and_additively_bond(node, last)?;
                } else {
                    self.join_fragments_additively(fragment, last)?;
                }
            }
        }
        if self.attr(group, "resolved").is_none() && siblings.len() > 1 {
            for &parent in &siblings[..siblings.len() - 1] {
                if self.additive_parent_eligible(parent, multiplier)? {
                    let target = self.graph().fragment(parent).out_atoms[0].atom;
                    if ft::calculate_substitutable_hydrogen_atoms(self.graph(), target) == 0 {
                        self.arena[group].set_attribute("resolved", "yes");
                        if multiplier.is_some() {
                            self.multiply_out_and_additively_bond(node, parent)?;
                        } else {
                            self.join_fragments_additively(fragment, parent)?;
                        }
                    }
                    break;
                }
                if !ft::find_substitutable_atoms(
                    self.graph(),
                    parent,
                    self.graph().fragment(fragment).out_atoms[count - 1].valency,
                )?
                .is_empty()
                {
                    break;
                }
            }
        }
        Ok(())
    }
    fn next_in_scope_multivalent_fragment(&self, node: NodeId) -> Result<Option<FragmentId>> {
        if !matches!(self.arena[node].name.as_str(), "substituent" | "bracket") {
            return Err(error(
                "Input to this function should be a substituent or bracket",
            ));
        }
        let parent = self.arena[node]
            .parent
            .ok_or_else(|| error("substituent did not have a parent!"))?;
        let start = self.arena.index_of(parent, node).unwrap();
        for child in self.children_any(parent, &["substituent", "bracket", "root"]) {
            if self.arena.index_of(parent, child).unwrap() <= start
                || self.attr(child, "multiplier").is_some()
            {
                continue;
            }
            let descendants = if self.arena[child].name == "bracket" {
                self.arena
                    .descendants_named_any(child, &["substituent", "root"])
            } else {
                vec![child]
            };
            for descendant in descendants {
                let group = self.group(descendant)?;
                let fragment = self.fragment(group)?;
                if self.multivalent_group(group, fragment) {
                    return Ok(Some(fragment));
                }
            }
        }
        Ok(None)
    }
    fn multivalent_group(&self, group: NodeId, fragment: FragmentId) -> bool {
        let count = self.graph().fragment(fragment).out_atoms.len();
        self.attr(group, "isAMultiRadical").is_some()
            && (count >= 2 || count >= 1 && self.attr(group, "resolved").is_some())
    }
    fn first_multivalent_group(&self, bracket: NodeId) -> Result<Option<NodeId>> {
        if self.arena[bracket].name != "bracket" {
            return Err(error("Input to this function should be a bracket"));
        }
        for group in self.arena.descendants_named(bracket, "group") {
            if self.multivalent_group(group, self.fragment(group)?) {
                return Ok(Some(group));
            }
        }
        Ok(None)
    }
    fn multiply_out_and_additively_bond(&mut self, node: NodeId, parent: FragmentId) -> Result<()> {
        let n = self.number(node, "multiplier")?;
        self.arena[node].remove_attribute("multiplier");
        let mut clones = Vec::new();
        let mut prefixes = Vec::new();
        for i in (0..n).rev() {
            let copy = if i != 0 {
                let copy = self.clone_element(node, i)?;
                clones.push(copy);
                copy
            } else {
                let multiplier = self
                    .arena
                    .first_child_named(node, "multiplier")
                    .ok_or_else(|| error("Multiplier not found where multiplier expected"))?;
                let cutoff = self.arena.index_of(node, multiplier).unwrap();
                // Detachment mutates this child list, so iterate an ordered snapshot.
                let preceding_children = self.arena[node].children[..cutoff].to_vec();
                for child in preceding_children.into_iter().rev() {
                    self.arena.detach(child);
                    prefixes.push(child);
                }
                self.arena.detach(multiplier);
                node
            };
            let group = self.group(copy)?;
            let fragment = self.fragment(group)?;
            if self.graph().fragment(fragment).out_atoms.len() != 1 {
                return Err(error(
                    "Additive bond formation failure: Fragment expected to have one OutAtom",
                ));
            }
            self.join_fragments_additively(fragment, parent)?;
        }
        for copy in clones {
            self.arena.insert_after(node, copy);
        }
        for prefix in prefixes {
            self.arena.insert_child(node, prefix, 0);
        }
        Ok(())
    }
    fn perform_multiplicative_operations(
        &mut self,
        results: BuildResults,
        parent: NodeId,
    ) -> Result<()> {
        let n = self.number(parent, "multiplier")?;
        if n != results.out_atoms.len() {
            return Err(error(format!(
                "Multiplication bond formation failure: number of outAtoms disagree with multiplier(multiplier: {n}, outAtom count: {})",
                results.out_atoms.len()
            )));
        }
        self.arena[parent].remove_attribute("multiplier");
        let mut in_locants = self.attr(parent, "inLocants").map(|locants| {
            if locants == "default" {
                vec!["default".into(); n]
            } else {
                locants.split(',').map(str::to_owned).collect::<Vec<_>>()
            }
        });
        if in_locants
            .as_ref()
            .is_some_and(|locants| locants.len() != n)
        {
            return Err(error(
                "Mismatch between multiplier and number of inLocants in multiplicative nomenclature",
            ));
        }
        let mut clones = Vec::new();
        let mut next_results = BuildResults::new();
        for i in (0..n).rev() {
            let copy = if i == 0 {
                parent
            } else {
                let copy = self.clone_element(parent, i)?;
                clones.push(copy);
                copy
            };
            let group = if self.arena[copy].name == "bracket" {
                if let Some(group) = self.first_multivalent_group(copy)? {
                    group
                } else {
                    let groups = self.arena.descendants_named(copy, "group");
                    let locants=in_locants.as_ref().ok_or_else(||error("OPSIN Bug? in locants must be specified for a multiplied root in multiplicative nomenclature"))?;
                    if locants.first().is_some_and(|s| s == "default") {
                        *groups
                            .last()
                            .ok_or_else(|| error("Multiplied root has no groups"))?
                    } else {
                        let mut chosen = None;
                        for &group in groups.iter().rev() {
                            let f = self.fragment(group)?;
                            if locants
                                .iter()
                                .any(|locant| self.graph().atom_by_locant(f, locant).is_some())
                            {
                                chosen = Some(group);
                                break;
                            }
                        }
                        chosen.ok_or_else(||error("Locants for inAtoms on the root were either misassigned to the root or invalid"))?
                    }
                }
            } else {
                self.group(copy)?
            };
            let multiplied = self.fragment(group)?;
            let mut out = results.out_atom(self.graph(), i)?.clone();
            let radical = self.graph().atom(out.atom).fragment;
            let radical_group = self.token(radical)?;
            if self.attr(radical_group, "resolved").is_none() {
                self.resolve_unlocanted_features(
                    self.arena[radical_group]
                        .parent
                        .ok_or_else(|| error("Radical group has no parent"))?,
                )?;
                self.arena[radical_group].set_attribute("resolved", "yes");
                out = results.out_atom(self.graph(), i)?.clone();
            }
            let mut substitutively_bonded = false;
            if let Some(locants) = &mut in_locants {
                let right = self.group(copy)?;
                self.arena[right].set_attribute("resolved", "yes");
                if self.attr(group, "isAMultiRadical").is_some() {
                    if self.attr(parent, "inLocants").as_deref() != Some("default") {
                        return Err(error(
                            "inLocants should not be specified for a multiradical parent in multiplicative nomenclature",
                        ));
                    }
                } else {
                    let mut target = None;
                    for index in (0..locants.len()).rev() {
                        let locant = &locants[index];
                        if locant == "default" {
                            let candidates =
                                self.possible_atoms_for_multiplied_root(group, out.valency, i)?;
                            if candidates.is_empty() {
                                return Err(error(
                                    "No suitable atom found for multiplicative operation",
                                ));
                            }
                            if ambiguity::is_substitution_ambiguous(self.graph(), &candidates, 1)? {
                                self.state.add_is_ambiguous(format!(
                                    "Connection to multiplied group: {}",
                                    self.arena.value(group)
                                ));
                            }
                            target = Some(candidates[0]);
                            locants.remove(index);
                            break;
                        }
                        if let Some(atom) = self.graph().atom_by_locant(multiplied, locant) {
                            target = Some(atom);
                            locants.remove(index);
                            break;
                        }
                    }
                    let target=target.ok_or_else(||error("Locants for inAtoms on the root were either misassigned to the root or invalid"))?;
                    let from = if out.explicitly_set {
                        out.atom
                    } else {
                        self.find_atom_for_unlocanted_radical(radical, &out)?
                    };
                    self.remove_out_by_id(radical, &out)?;
                    self.state.fragment_manager.create_bond(
                        from,
                        target,
                        bond_order(out.valency)?,
                    )?;
                    substitutively_bonded = true;
                }
            }
            if !substitutively_bonded {
                self.join_fragments_additively(radical, multiplied)?;
            }
            if self.arena[copy].name == "bracket" {
                self.recursively_resolve_unlocanted_features(copy)?;
            }
            if in_locants.is_none() {
                next_results.merge(BuildResults::from_node(self.arena, self.graph(), copy)?);
            }
        }
        if next_results.fragments.len() == 1 {
            return Err(error(
                "Multiplicative nomenclature cannot yield only one temporary terminal fragment",
            ));
        }
        if next_results.fragments.len() >= 2 {
            let mut next = self.next_structural_sibling(parent);
            if next.is_none() {
                let outer = self.arena[parent]
                    .parent
                    .ok_or_else(|| error("Multiplied parent has no parent"))?;
                if self.arena[outer].name == "bracket" {
                    next = self.next_structural_sibling(outer);
                }
            }
            let next = next.ok_or_else(|| {
                error("Could not find suitable element to continue multiplicative nomenclature")
            })?;
            if self.attr(next, "multiplier").is_none() {
                return Err(error(
                    "Multiplier not found where multiplier was expected for successful multiplicative nomenclature",
                ));
            }
            self.perform_multiplicative_operations(next_results, next)?;
        }
        for clone in clones {
            self.arena.insert_after(parent, clone);
        }
        Ok(())
    }
    fn next_structural_sibling(&self, node: NodeId) -> Option<NodeId> {
        let mut next = self.arena.next_sibling(node);
        while let Some(id) = next {
            if matches!(
                self.arena[id].name.as_str(),
                "substituent" | "bracket" | "root"
            ) {
                return Some(id);
            }
            next = self.arena.next_sibling(id);
        }
        None
    }
    fn possible_atoms_for_multiplied_root(
        &self,
        group: NodeId,
        order: i32,
        primes: usize,
    ) -> Result<Vec<AtomId>> {
        let fragment = self.fragment(group)?;
        let f = self.graph().fragment(fragment);
        if self.attr(group, "usableAsAJoiner").as_deref() == Some("yes")
            && f.default_in_atom.is_none()
            && self
                .arena
                .previous_element(group, false)
                .is_some_and(|p| self.arena[p].name == "multiplier")
        {
            let primes = "'".repeat(primes);
            let mut length = 0;
            let mut previous = None;
            while let Some(next) = self
                .graph()
                .atom_by_locant(fragment, &format!("{}{primes}", length + 1))
            {
                if previous.is_some_and(|p| self.graph().bond_between(p, next).is_none()) {
                    break;
                }
                length += 1;
                previous = Some(next);
            }
            if length > 1 {
                let preferred = self.atom_by_locant(fragment, &format!("{length}{primes}"))?;
                return Ok(ft::find_n_atoms_for_substitution(
                    self.graph(),
                    &f.atoms,
                    Some(preferred),
                    1,
                    order,
                    true,
                    false,
                )?
                .unwrap_or_default());
            }
        }
        ft::find_substitutable_atoms(self.graph(), fragment, order)
    }
    fn remove_out_by_id(&mut self, fragment: FragmentId, out: &OutAtom) -> Result<()> {
        let position = self
            .graph()
            .fragment(fragment)
            .out_atoms
            .iter()
            .position(|other| other.id == out.id)
            .ok_or_else(|| error("Out atom already consumed"))?;
        self.graph_mut().remove_out_atom(fragment, position);
        Ok(())
    }
    pub fn join_fragments_additively(
        &mut self,
        fragment: FragmentId,
        parent: FragmentId,
    ) -> Result<()> {
        let group = self.token(fragment)?;
        let outs = self.graph().fragment(fragment).out_atoms.clone();
        let parent_outs = self.graph().fragment(parent).out_atoms.clone();
        if self.attr(group, "subType").as_deref() == Some("epoxyLike")
            && outs.iter().any(|out| out.locant.is_some())
        {
            return Err(error(format!(
                "Inappropriate use of {}",
                self.arena.value(group)
            )));
        }
        if outs.is_empty() || parent_outs.is_empty() {
            return Err(error(
                "Additive bond formation failure: Fragment expected to have at least one OutAtom but had none",
            ));
        }
        let mut in_index = 0;
        if parent_outs.len() > 1
            && parent_outs
                .iter()
                .any(|out| out.valency != parent_outs[0].valency)
        {
            let different = outs.iter().any(|out| out.valency != outs[0].valency);
            if different && outs.len() == 2 {
                if let Some(previous) = self.previous_group(group) {
                    let f = self.fragment(previous)?;
                    let previous_outs = &self.graph().fragment(f).out_atoms;
                    if previous_outs.len() > 1
                        && previous_outs
                            .iter()
                            .all(|out| out.valency == previous_outs[0].valency)
                        && previous_outs[0].valency == parent_outs[0].valency
                        && let Some(index) = parent_outs
                            .iter()
                            .position(|out| out.valency != previous_outs[0].valency)
                    {
                        in_index = index;
                    }
                }
            } else if let Some(index) = parent_outs
                .iter()
                .position(|out| out.valency == outs[0].valency)
            {
                in_index = index;
            }
        }
        let incoming = parent_outs[in_index].clone();
        let to = if incoming.explicitly_set {
            incoming.atom
        } else {
            self.find_atom_for_unlocanted_radical(
                self.graph().atom(incoming.atom).fragment,
                &incoming,
            )?
        };
        let order = incoming.valency;
        self.remove_out_by_id(parent, &incoming)?;
        let mut out_index = outs.iter().rposition(|out| out.valency == order);
        if out_index.is_none() {
            if outs.len() < order as usize {
                return Err(error(
                    "Additive bond formation failure: bond order disagreement",
                ));
            }
            let last = outs.last().unwrap().atom;
            let mut sum = 0;
            for index in (0..outs.len()).rev() {
                if outs[index].atom != last {
                    return Err(error(
                        "Additive bond formation failure: bond order disagreement",
                    ));
                }
                sum += outs[index].valency;
                if sum == order {
                    self.graph_mut().set_out_atom_valency(fragment, index, sum);
                    out_index = Some(index);
                    break;
                }
                self.graph_mut().remove_out_atom(fragment, index);
            }
        }
        let out_index = out_index
            .ok_or_else(|| error("Additive bond formation failure: bond order disagreement"))?;
        let out = self.graph().fragment(fragment).out_atoms[out_index].clone();
        let from = if out.explicitly_set {
            out.atom
        } else {
            self.find_atom_for_unlocanted_radical(self.graph().atom(out.atom).fragment, &out)?
        };
        self.remove_out_by_id(fragment, &out)?;
        self.state
            .fragment_manager
            .create_bond(from, to, bond_order(order)?)?;
        Ok(())
    }
    pub fn join_fragments_substitutively(
        &mut self,
        fragment: FragmentId,
        to: AtomId,
    ) -> Result<()> {
        let group = self.token(fragment)?;
        if self.attr(group, "subType").as_deref() == Some("epoxyLike") {
            self.form_epoxide(fragment, to)?;
            return Ok(());
        }
        if self.graph().fragment(fragment).out_atoms.len() != 1 {
            return Err(error(
                "Substitutive bond formation failure: Fragment expected to have one OutAtom",
            ));
        }
        if self.attr(group, "iminoLike").is_some()
            && self.graph().fragment(fragment).out_atoms[0].valency == 1
        {
            self.graph_mut().set_out_atom_valency(fragment, 0, 2);
        }
        let out = self.graph().fragment(fragment).out_atoms[0].clone();
        let from = if out.explicitly_set {
            out.atom
        } else {
            self.find_radical_position(fragment, &out, false)?
        };
        self.graph_mut().remove_out_atom(fragment, 0);
        if self.graph().atom(to).element.is_chalcogen() {
            let owner = self.graph().atom(to).fragment;
            self.graph_mut()
                .fragment_mut(owner)
                .functional_atoms
                .retain(|&atom| atom != to);
        }
        self.state
            .fragment_manager
            .create_bond(from, to, bond_order(out.valency)?)?;
        Ok(())
    }
    pub fn form_epoxide(&mut self, fragment: FragmentId, to: AtomId) -> Result<[AtomId; 2]> {
        let parent = self.graph().atom(to).fragment;
        let atoms = self.graph().fragment(parent).atoms.clone();
        if atoms.len() == 1 {
            return Err(error("Epoxides must be formed between two different atoms"));
        }
        if self.graph().fragment(fragment).out_atoms.len() != 2 {
            return Err(error("A bridging fragment must have two out atoms"));
        }
        let out1 = self.graph().fragment(fragment).out_atoms[0].clone();
        let first = if let Some(locant) = &out1.locant {
            self.atom_by_locant(parent, locant)?
        } else {
            to
        };
        self.graph_mut().remove_out_atom(fragment, 0);
        self.state
            .fragment_manager
            .create_bond(out1.atom, first, bond_order(out1.valency)?)?;
        let out2 = self.graph().fragment(fragment).out_atoms[0].clone();
        let second = if let Some(locant) = &out2.locant {
            self.atom_by_locant(parent, locant)?
        } else {
            let index = atoms
                .iter()
                .position(|&atom| atom == first)
                .ok_or_else(|| error("Bridge target absent from parent"))?;
            let preferred = if index + 1 >= atoms.len() {
                atoms[index - 1]
            } else {
                atoms[index + 1]
            };
            let mut possible = ft::find_n_atoms_for_substitution(
                self.graph(),
                &atoms,
                Some(preferred),
                1,
                1,
                true,
                false,
            )?
            .unwrap_or_default();
            possible.retain(|&a| a != first);
            if possible.is_empty() {
                return Err(error("Unable to find suitable atom to form bridge"));
            }
            if ambiguity::is_substitution_ambiguous(self.graph(), &possible, 1)? {
                self.state.add_is_ambiguous(format!(
                    "Addition of bridge to: {}",
                    self.arena.value(self.token(parent)?)
                ));
            }
            possible[0]
        };
        self.graph_mut().remove_out_atom(fragment, 0);
        if out1.atom == out2.atom && first == second {
            return Err(error("Epoxides must be formed between two different atoms"));
        }
        let order = if self.graph().atom(out2.atom).spare_valency
            && !self.graph().atom(second).spare_valency
        {
            2
        } else {
            out2.valency
        };
        self.state
            .fragment_manager
            .create_bond(out2.atom, second, bond_order(order)?)?;
        cycle_detector::assign_cycle_membership(self.graph_mut(), fragment);
        Ok([first, second])
    }
}

pub fn bond_order(order: i32) -> Result<u8> {
    u8::try_from(order)
        .ok()
        .filter(|order| (1..=3).contains(order))
        .ok_or_else(|| error("Assembly bond order must be 1, 2, or 3"))
}
fn compound_locant(locant: &str) -> Option<(String, String)> {
    let start = locant.find(['[', '(', '{'])?;
    let end = locant[start + 1..].find([']', ')', '}'])? + start + 1;
    let inner = &locant[start + 1..end];
    if !ft::is_numeric_locant(inner) {
        return None;
    }
    Some((
        format!("{}{}", &locant[..start], &locant[end + 1..]),
        inner.into(),
    ))
}
pub fn lambda_or_hw_or_max_valency(graph: &Graph, atom: AtomId) -> Option<i32> {
    let a = graph.atom(atom);
    if let Some(lambda) = a.lambda_convention_valency {
        Some(lambda + a.protons_explicitly_added_or_removed)
    } else if a.charge == 0 {
        valence::hw_valency(a.element)
    } else {
        valence::maximum_valency(a.element, a.charge)
    }
}
pub fn find_bonds_to_unsaturate(
    graph: &Graph,
    fragment: FragmentId,
    order: u8,
    adjacent: bool,
    ignore: &[BondId],
) -> Vec<BondId> {
    let mut chosen = Vec::new();
    'atoms: for &a in &graph.fragment(fragment).atoms {
        let atom = graph.atom(a);
        if atom.spare_valency || atom.atom_type == "suffix" || atom.properties.is_aldehyde {
            continue;
        }
        let mut incoming = 0;
        for &bond in &atom.bonds {
            let b = graph.bond(bond);
            if b.order != 1 && !adjacent {
                continue 'atoms;
            }
            if chosen.contains(&bond) {
                if !adjacent {
                    continue 'atoms;
                }
                incoming += i32::from(order);
            } else {
                incoming += i32::from(b.order);
            }
        }
        if lambda_or_hw_or_max_valency(graph, a)
            .is_some_and(|max| incoming + i32::from(order) - 1 + atom.out_valency > max)
        {
            continue;
        }
        'bonds: for &bond in &atom.bonds {
            let b = graph.bond(bond);
            if b.order != 1 || chosen.contains(&bond) || ignore.contains(&bond) {
                continue;
            }
            let other = b.other_atom(a).unwrap();
            if !graph.fragment(fragment).atoms.contains(&other) {
                continue;
            }
            let oa = graph.atom(other);
            if oa.spare_valency || oa.atom_type == "suffix" || oa.properties.is_aldehyde {
                continue;
            }
            let mut incoming = 0;
            for &ob in &oa.bonds {
                let b = graph.bond(ob);
                if b.order != 1 && !adjacent {
                    continue 'bonds;
                }
                if chosen.contains(&ob) {
                    if !adjacent {
                        continue 'bonds;
                    }
                    incoming += i32::from(order);
                } else {
                    incoming += i32::from(b.order);
                }
            }
            if lambda_or_hw_or_max_valency(graph, other)
                .is_some_and(|max| incoming + i32::from(order) - 1 + oa.out_valency > max)
            {
                continue;
            }
            chosen.push(bond);
            break;
        }
    }
    chosen
}
/// The exact alternative-group stack, shared with StereochemistryHandler.
pub fn find_alternative_fragments(arena: &Arena, start: NodeId) -> Result<Vec<FragmentId>> {
    let parent = arena[start]
        .parent
        .ok_or_else(|| error("Substituent has no parent"))?;
    let mut stack = vec![(parent, false)];
    let (mut groups, mut unlikely) = (Vec::new(), Vec::new());
    let mut first = true;
    while let Some((node, disfavoured)) = stack.pop() {
        if arena[node].name == "group" {
            if disfavoured {
                unlikely.push(node);
            } else {
                groups.push(node);
            }
            continue;
        }
        for &child in &arena[node].children {
            if !matches!(
                arena[child].name.as_str(),
                "bracket" | "substituent" | "root"
            ) {
                continue;
            }
            if first && arena.index_of(node, child) <= arena.index_of(node, start) {
                continue;
            }
            if arena[child].attribute("multiplier").is_some() {
                continue;
            }
            if arena[child].name == "bracket" {
                stack.push((
                    child,
                    disfavoured || arena[child].attribute("type") != Some("implicit"),
                ));
            } else {
                let group = arena
                    .first_child_named(child, "group")
                    .ok_or_else(|| error("substituent/root is missing its group"))?;
                stack.push((
                    group,
                    disfavoured || arena[child].attribute("locant").is_some(),
                ));
            }
        }
        first = false;
    }
    groups
        .into_iter()
        .chain(unlikely)
        .map(|node| {
            arena[node]
                .fragment
                .ok_or_else(|| error("Group has no constructed fragment"))
        })
        .collect()
}

pub fn resolve_locanted_features(
    state: &mut BuildState,
    arena: &mut Arena,
    node: NodeId,
) -> Result<()> {
    Assembly { state, arena }.resolve_locanted_features(node)
}
pub fn resolve_unlocanted_features(
    state: &mut BuildState,
    arena: &mut Arena,
    node: NodeId,
) -> Result<()> {
    Assembly { state, arena }.resolve_unlocanted_features(node)
}
pub fn resolve_word_or_bracket(
    state: &mut BuildState,
    arena: &mut Arena,
    node: NodeId,
) -> Result<()> {
    Assembly { state, arena }.resolve_word_or_bracket(node)
}
pub fn form_epoxide(
    state: &mut BuildState,
    arena: &mut Arena,
    fragment: FragmentId,
    atom: AtomId,
) -> Result<[AtomId; 2]> {
    Assembly { state, arena }.form_epoxide(fragment, atom)
}
pub fn apply_subtractive_prefix(
    state: &mut BuildState,
    fragment: FragmentId,
    element: Element,
    locant: &str,
) -> Result<()> {
    Assembly {
        state,
        arena: &mut Arena::default(),
    }
    .apply_subtractive_prefix(fragment, element, locant)
}
