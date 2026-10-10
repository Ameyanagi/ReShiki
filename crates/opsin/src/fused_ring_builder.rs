//! Full OPSIN `FusedRingBuilder` fusion-descriptor and graph rewrites.
//! 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (LICENSE).
use crate::{
    build_state::BuildState,
    fragment_tools, fused_ring_numberer,
    graph::{AtomId, Bond, BondId, Element, FragmentId, Graph, GraphError},
    parse_tree::{Arena, NodeId},
};
use std::collections::{BTreeMap, HashSet};

fn error(message: impl Into<String>) -> GraphError {
    GraphError(message.into())
}
fn primes(text: &str) -> i32 {
    text.chars().rev().take_while(|&ch| ch == '\'').count() as i32
}
fn split(text: &str, separator: char) -> Vec<String> {
    let mut parts: Vec<_> = text.split(separator).map(str::to_owned).collect();
    while parts.len() > 1 && parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

pub fn process_fused_rings(
    state: &mut BuildState,
    arena: &mut Arena,
    sub_or_root: NodeId,
) -> Result<(), GraphError> {
    let groups = arena.children_named(sub_or_root, "group");
    if groups.len() < 2 {
        return Ok(());
    }
    let mut fused = Vec::new();
    for index in (0..groups.len()).rev() {
        let group = groups[index];
        fused.insert(0, group);
        if index != 0 {
            let mut start = group;
            if matches!(arena.value(group).as_str(), "benz" | "benzo")
                && arena[group].attribute("subType") == Some("fusionRing")
                && let Some(previous) = arena.previous_sibling(group)
                && arena[previous].name == "locant"
            {
                start = previous;
            }
            let mut previous = arena.previous_sibling(start);
            while let Some(node) = previous {
                if !matches!(arena[node].name.as_str(), "multiplier" | "fusion") {
                    break;
                }
                previous = arena.previous_sibling(node);
            }
            if previous != Some(groups[index - 1]) {
                if fused.len() >= 2 {
                    Builder::new(state, arena, std::mem::take(&mut fused))?.build()?;
                }
                fused.clear();
            }
        }
    }
    if fused.len() >= 2 {
        Builder::new(state, arena, fused)?.build()?;
    }
    Ok(())
}

struct Builder<'a> {
    state: &'a mut BuildState,
    arena: &'a mut Arena,
    groups: Vec<NodeId>,
    last: NodeId,
    parent: FragmentId,
    scope: BTreeMap<i32, FragmentId>,
    replacements: BTreeMap<AtomId, AtomId>,
}
impl<'a> Builder<'a> {
    fn new(
        state: &'a mut BuildState,
        arena: &'a mut Arena,
        groups: Vec<NodeId>,
    ) -> Result<Self, GraphError> {
        let last = *groups
            .last()
            .ok_or_else(|| error("Empty fused ring system"))?;
        let parent = arena[last]
            .fragment
            .ok_or_else(|| error("Fused ring group has no fragment"))?;
        Ok(Self {
            state,
            arena,
            groups,
            last,
            parent,
            scope: BTreeMap::from([(0, parent)]),
            replacements: BTreeMap::new(),
        })
    }
    fn graph(&self) -> &Graph {
        self.state.graph()
    }
    fn graph_mut(&mut self) -> &mut Graph {
        self.state.graph_mut()
    }
    fn fragment(&self, group: NodeId) -> Result<FragmentId, GraphError> {
        self.arena[group]
            .fragment
            .ok_or_else(|| error("Fused ring group has no fragment"))
    }
    fn value(&self, node: NodeId) -> String {
        self.arena.value(node)
    }
    fn number(&self, node: NodeId, attr: &str) -> Result<usize, GraphError> {
        self.arena[node]
            .attribute(attr)
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                error(format!(
                    "OPSIN bug: Invalid {attr} on {}",
                    self.arena[node].name
                ))
            })
    }
    fn in_scope(&self, level: i32) -> Result<FragmentId, GraphError> {
        self.scope
            .get(&level)
            .copied()
            .ok_or_else(|| error("Fusion level refers to a ring not in scope"))
    }
    fn bracket_descriptor(&self, fusion: NodeId) -> Result<String, GraphError> {
        let value = self.value(fusion).to_ascii_lowercase();
        if value.len() < 2 {
            return Err(error("Malformed fusion bracket!"));
        }
        Ok(value[1..value.len() - 1].to_owned())
    }
    fn relabel(&mut self, fragment: FragmentId, level: i32) {
        if level > 0 {
            let atoms = self.graph().fragment(fragment).atoms.clone();
            fragment_tools::relabel_numeric_locants(
                self.graph_mut(),
                &atoms,
                &"'".repeat(level as usize),
            );
        }
    }
    fn remove_merged_atoms(&mut self) {
        for atom in self.replacements.keys().copied().collect::<Vec<_>>() {
            self.state
                .fragment_manager
                .remove_atom_and_associated_bonds(atom);
        }
        self.replacements.clear();
    }
    fn name_components(&self) -> Result<Vec<NodeId>, GraphError> {
        let mut components = Vec::new();
        let mut current = self.groups[0];
        while current != self.last {
            if matches!(self.arena[current].name.as_str(), "group" | "fusion") {
                components.push(current);
            }
            current = self
                .arena
                .next_sibling(current)
                .ok_or_else(|| error("OPSIN bug: Fused ring groups are not siblings"))?;
        }
        Ok(components)
    }
    fn build(&mut self) -> Result<(), GraphError> {
        self.process_numbering_and_irregularities()?;
        self.process_benzo_fusions()?;
        let components = self.name_components()?;
        let mut fragments = Vec::new();
        let mut parents = vec![self.parent];
        if !components.is_empty()
            && let Some(multiplier) = self
                .arena
                .previous_sibling(self.last)
                .filter(|&id| self.arena[id].name == "multiplier")
        {
            let count = self.number(multiplier, "value")?;
            self.arena.detach(multiplier);
            for _ in 1..count {
                let copy = self.state.fragment_manager.copy_fragment(self.parent)?;
                parents.push(copy);
                fragments.push(copy);
            }
        }
        let mut index = self.process_multi_parent(&parents, &components, &mut fragments)?;
        let mut level = (components.len() as i32 - 1 - index) / 2;
        while index >= 0 {
            let fusion = if self.arena[components[index as usize]].name == "fusion" {
                let fusion = components[index as usize];
                index -= 1;
                Some(fusion)
            } else {
                None
            };
            if index < 0 || self.arena[components[index as usize]].name != "group" {
                return Err(error(
                    "Group not found where group expected. This is probably a bug",
                ));
            }
            let group = components[index as usize];
            let next = self.fragment(group)?;
            let multiplier_element = self
                .arena
                .previous_sibling(group)
                .filter(|&id| self.arena[id].name == "multiplier");
            let mut multiplier = multiplier_element.map_or(Ok(1), |id| self.number(id, "value"))?;
            let descriptors = if let Some(fusion) = fusion {
                let descriptor = self.bracket_descriptor(fusion)?;
                if multiplier == 1 {
                    Some(vec![descriptor])
                } else if let Some(parts) = [';', ':', ',']
                    .into_iter()
                    .map(|separator| split(&descriptor, separator))
                    .find(|parts| parts.len() > 1)
                {
                    Some(parts)
                } else {
                    if index != 0 {
                        return Err(error(format!(
                            "Unexpected multiplier: {} or incorrect fusion descriptor: {descriptor}",
                            self.value(multiplier_element.expect("Multiplier missing"))
                        )));
                    }
                    multiplier = 1;
                    Some(vec![descriptor])
                }
            } else {
                None
            };
            if multiplier > 1 {
                self.arena
                    .detach(multiplier_element.expect("Multiplier missing"));
            }
            let mut fusion_components = vec![next];
            for copy in 1..multiplier {
                fusion_components.push(
                    self.state
                        .fragment_manager
                        .copy_and_relabel_fragment(next, copy)?,
                );
            }
            for (copy, &component) in fusion_components.iter().enumerate() {
                fragments.push(component);
                if let Some(descriptors) = &descriptors {
                    let descriptor = descriptors.get(copy).ok_or_else(|| {
                        error("Mismatch between multiplier and fusion descriptors")
                    })?;
                    if split(descriptor, ':').len() == 1 {
                        if split(descriptor, '-').len() == 1
                            && split(descriptor, ',').len() > 1
                            && fragment_tools::all_atoms_in_ring_are_identical(
                                self.graph(),
                                component,
                            )
                            && primes(&split(descriptor, ',')[0]) != level
                        {
                            let number_primes = primes(&split(descriptor, ',')[0]);
                            if number_primes + 1 != level {
                                if number_primes + 2 == level {
                                    level -= 1;
                                } else {
                                    return Err(error(format!(
                                        "Incorrect number of primes in fusion bracket: {descriptor}"
                                    )));
                                }
                            }
                            self.relabel(component, level);
                            let parent_locants = split(descriptor, ',');
                            let parent = self.in_scope(level)?;
                            let parent_atoms = self
                                .determine_atoms(parent, &parent_locants, None)?
                                .ok_or_else(|| error("Malformed fusion bracket!"))?;
                            let child_locants = self
                                .possible_numerical_locants(component, parent_atoms.len() - 1)
                                .ok_or_else(|| error("Unable to find implicit fusion locants"))?;
                            self.process_higher_order(
                                component,
                                parent,
                                &child_locants,
                                &parent_locants,
                            )?;
                        } else {
                            level = 0;
                            self.relabel(component, level);
                            let (numerical, letters) = numerical_and_letter_components(descriptor);
                            let (descriptor, number_primes) = if !letters.is_empty() {
                                (
                                    if numerical.is_empty() {
                                        letters.replace('\'', "")
                                    } else {
                                        format!("{numerical}-{}", letters.replace('\'', ""))
                                    },
                                    primes(&letters),
                                )
                            } else {
                                (descriptor.clone(), 0)
                            };
                            let parent = parents
                                .get(number_primes as usize)
                                .copied()
                                .ok_or_else(|| error("Unexpected prime in fusion descriptor"))?;
                            self.simple_fusion(Some(&descriptor), component, parent)?;
                        }
                    } else {
                        let number_primes = primes(&split(descriptor, ',')[0]) - copy as i32;
                        if number_primes != level {
                            if level == number_primes + 1 {
                                level -= 1;
                            } else {
                                return Err(error(format!(
                                    "Incorrect number of primes in fusion bracket: {descriptor}"
                                )));
                            }
                        }
                        self.relabel(component, level);
                        self.higher_fusion(descriptor, component, self.in_scope(level)?)?;
                    }
                } else {
                    self.relabel(component, level);
                    self.simple_fusion(None, component, self.in_scope(level)?)?;
                }
            }
            level += 1;
            if multiplier == 1 {
                self.scope.insert(level, fusion_components[0]);
            }
            index -= 1;
        }
        for fragment in fragments {
            self.state
                .fragment_manager
                .incorporate_fragment(fragment, self.parent)?;
        }
        self.remove_merged_atoms();
        let parent = self.parent;
        fused_ring_numberer::number_fused_ring(self.graph_mut(), parent)?;
        let mut name: String = components.iter().map(|&node| self.value(node)).collect();
        name.push_str(&self.value(self.last));
        self.arena[self.last].set_attribute("value", &name);
        self.arena[self.last].set_attribute("type", "ring");
        self.arena[self.last].set_value(name);
        for component in components {
            self.arena.detach(component);
        }
        Ok(())
    }
    fn process_multi_parent(
        &mut self,
        parents: &[FragmentId],
        components: &[NodeId],
        fragments: &mut Vec<FragmentId>,
    ) -> Result<i32, GraphError> {
        let mut index = components.len() as i32 - 1;
        let mut level = 0;
        if index >= 0 && parents.len() > 1 {
            let mut previous = parents.to_vec();
            while index >= 0 {
                if previous.len() == 1 {
                    self.scope.insert(level, previous[0]);
                    break;
                }
                if self.arena[components[index as usize]].name != "fusion" {
                    return Err(error(
                        "Fusion bracket not found where fusion bracket expected",
                    ));
                }
                let fusion = components[index as usize];
                index -= 1;
                if index < 0 || self.arena[components[index as usize]].name != "group" {
                    return Err(error(
                        "Group not found where group expected. This is probably a bug",
                    ));
                }
                let group = components[index as usize];
                let next = self.fragment(group)?;
                self.relabel(next, level);
                let multiplier = if let Some(node) = self
                    .arena
                    .previous_sibling(group)
                    .filter(|&id| self.arena[id].name == "multiplier")
                {
                    let count = self.number(node, "value")?;
                    self.arena.detach(node);
                    count
                } else {
                    1
                };
                let mut fusion_components = vec![next];
                for copy in 1..multiplier {
                    let fragment = self.state.fragment_manager.copy_fragment(next)?;
                    self.relabel(fragment, copy as i32);
                    fusion_components.push(fragment);
                }
                level += multiplier as i32;
                if multiplier > 1 && multiplier != previous.len() {
                    return Err(error(
                        "Mismatch between number of components and number of parents in fused ring system",
                    ));
                }
                let descriptor = self.bracket_descriptor(fusion)?;
                let descriptors = [';', ':', ',']
                    .into_iter()
                    .map(|separator| split(&descriptor, separator))
                    .find(|parts| parts.len() > 1)
                    .ok_or_else(|| error(format!("Invalid fusion descriptor: {descriptor}")))?;
                if descriptors.len() != previous.len() {
                    return Err(error(format!(
                        "Invalid fusion descriptor: {descriptor}(Number of locants disagrees with number of parents)"
                    )));
                }
                for (copy, descriptor) in descriptors.iter().enumerate() {
                    let component = if multiplier > 1 {
                        fusion_components[copy]
                    } else {
                        next
                    };
                    let parent = previous[copy];
                    if split(descriptor, ':').len() <= 1 {
                        let (numerical, letters) = numerical_and_letter_components(descriptor);
                        let descriptor = if !letters.is_empty() {
                            let number_primes = primes(&letters);
                            let descriptor = if numerical.is_empty() {
                                letters.replace('\'', "")
                            } else {
                                format!("{numerical}-{}", letters.replace('\'', ""))
                            };
                            if number_primes != copy as i32 {
                                return Err(error(format!(
                                    "Incorrect number of primes in fusion descriptor: {descriptor}"
                                )));
                            }
                            descriptor
                        } else {
                            descriptor.clone()
                        };
                        self.simple_fusion(Some(&descriptor), component, parent)?;
                    } else {
                        self.higher_fusion(descriptor, component, parent)?;
                    }
                }
                previous = fusion_components.clone();
                fragments.extend(fusion_components);
                index -= 1;
            }
            if previous.len() != 1 {
                return Err(error(
                    "Invalid fused ring system. Incomplete multiparent system",
                ));
            }
        }
        Ok(index)
    }
    fn adjacent_unsaturators(&self, group: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut next = self.arena.next_sibling(group);
        while let Some(node) = next {
            if self.arena[node].name != "unsaturator" {
                break;
            }
            result.push(node);
            next = self.arena.next_sibling(node);
        }
        result
    }
    fn process_numbering_and_irregularities(&mut self) -> Result<(), GraphError> {
        for group in self.groups.clone() {
            let ring = self.fragment(group)?;
            if self.arena[group].attribute("subType") == Some("alkaneStem") {
                self.aromatise_cyclic_alkane(group)?;
            }
            if self.arena[group].attribute("subType") == Some("hantzschWidman")
                && self.arena[group].attribute("addBond").is_some()
                && let Some(unsaturator) = self.adjacent_unsaturators(group).first().copied()
                && self.arena[unsaturator].attribute("locant").is_none()
                && self.arena[unsaturator].attribute("value") == Some("2")
            {
                self.arena.detach(unsaturator);
                let bonds = crate::structure_building_methods::find_bonds_to_unsaturate(
                    self.graph(),
                    ring,
                    2,
                    true,
                    &[],
                );
                let bond = bonds.first().copied().ok_or_else(|| {
                    error("Failed to find bond to unsaturate on partially saturated HW ring")
                })?;
                let (from, to) = (self.graph().bond(bond).from, self.graph().bond(bond).to);
                self.graph_mut().atom_mut(from).spare_valency = true;
                self.graph_mut().atom_mut(to).spare_valency = true;
            }
            if group == self.last {
                let atoms = self.graph().fragment(ring).atoms.clone();
                if atoms.iter().any(|&atom| !self.graph().atom(atom).in_cycle) {
                    return Err(error(format!(
                        "Inappropriate group used in fusion nomenclature. Only groups composed entirely of atoms in cycles may be used. i.e. not: {}",
                        self.value(group)
                    )));
                }
                if let Some(numbering) = self.arena[group]
                    .attribute("fusedRingNumbering")
                    .map(str::to_owned)
                {
                    for (index, locant) in numbering.split('/').enumerate() {
                        let atom = *atoms
                            .get(index)
                            .ok_or_else(|| error("Invalid fused ring numbering attribute"))?;
                        self.graph_mut().clear_locants(atom);
                        self.graph_mut().add_locant(atom, locant);
                    }
                } else {
                    self.sort_atoms_by_locant(ring);
                }
                for atom in atoms {
                    self.graph_mut().clear_locants(atom);
                }
            } else if self.arena[group].attribute("fusedRingNumbering").is_none() {
                self.sort_atoms_by_locant(ring);
            }
        }
        Ok(())
    }
    fn sort_atoms_by_locant(&mut self, fragment: FragmentId) {
        let mut atoms = self.graph().fragment(fragment).atoms.clone();
        atoms.sort_by(|&a, &b| fragment_tools::compare_atoms_by_locants(self.graph(), a, b));
        self.graph_mut().fragment_mut(fragment).atoms = atoms;
    }
    fn aromatise_cyclic_alkane(&mut self, group: NodeId) -> Result<(), GraphError> {
        let unsaturators = self.adjacent_unsaturators(group);
        let conjugate = match unsaturators.as_slice() {
            [] => true,
            [one] => {
                self.number(*one, "value")? == 2 && self.arena[*one].attribute("locant").is_none()
            }
            [one, two] => {
                self.number(*one, "value")? == 1
                    && self.number(*two, "value")? == 2
                    && self.arena[*two].attribute("locant").is_none()
            }
            _ => false,
        };
        if conjugate {
            for unsaturator in unsaturators {
                self.arena.detach(unsaturator);
            }
            let fragment = self.fragment(group)?;
            for atom in self.graph().fragment(fragment).atoms.clone() {
                self.graph_mut().atom_mut(atom).spare_valency = true;
            }
        }
        Ok(())
    }
    fn process_benzo_fusions(&mut self) -> Result<(), GraphError> {
        for index in (0..self.groups.len() - 1).rev() {
            let group = self.groups[index];
            if matches!(self.value(group).as_str(), "benz" | "benzo")
                && self
                    .arena
                    .next_sibling(group)
                    .is_some_and(|node| self.arena[node].name != "fusion")
            {
                let previous = self.arena.previous_sibling(group);
                if previous.is_none_or(|node| {
                    self.arena[node].name != "multiplier"
                        || self.arena[node].attribute("type") == Some("group")
                }) {
                    self.benzo_specific_fusion(group, self.groups[index + 1])?;
                    self.arena.detach(group);
                    self.groups.remove(index);
                }
            }
        }
        Ok(())
    }
    fn simple_fusion(
        &mut self,
        descriptor: Option<&str>,
        child: FragmentId,
        parent: FragmentId,
    ) -> Result<(), GraphError> {
        let (mut numerical, mut letters) = (None, None);
        if let Some(descriptor) = descriptor {
            let parts = split(descriptor, '-');
            if parts.len() == 2 {
                numerical = Some(split(&parts[0], ','));
                letters = Some(parts[1].chars().collect::<Vec<_>>());
            } else if parts[0].contains(',') {
                numerical = Some(split(&parts[0], ','));
            } else {
                letters = Some(parts[0].chars().collect::<Vec<_>>());
            }
        }
        let edge_length = if let Some(numbers) = &numerical {
            if numbers.len() <= 1 {
                return Err(error(
                    "At least two numerical locants must be provided to perform fusion!",
                ));
            }
            numbers.len() - 1
        } else {
            letters.as_ref().map_or(1, Vec::len)
        };
        if numerical.is_none() {
            numerical = self.possible_numerical_locants(child, edge_length);
        }
        if letters.is_none() {
            letters = self.possible_letter_locants(parent, edge_length);
        }
        let (Some(numerical), Some(letters)) = (numerical, letters) else {
            return Err(error(
                "Unable to find bond to form fused ring system. Some information for forming fused ring system was only supplyed implicitly",
            ));
        };
        let child_atoms = self
            .determine_atoms(child, &numerical, Some(letters.len() + 1))?
            .ok_or_else(|| error("Malformed fusion bracket!"))?;
        let peripheral = self.peripheral_atoms(parent)?;
        let start = letters
            .first()
            .copied()
            .ok_or_else(|| error("Malformed fusion bracket!"))? as usize
            - 'a' as usize;
        if start >= peripheral.len() {
            return Err(error("Malformed fusion bracket!"));
        }
        let parent_atoms = (0..=letters.len())
            .map(|step| peripheral[(start + step) % peripheral.len()])
            .collect();
        self.fuse_rings(child_atoms, parent_atoms)
    }
    fn possible_letter_locants(
        &self,
        fragment: FragmentId,
        edge_length: usize,
    ) -> Option<Vec<char>> {
        let atoms = &self.graph().fragment(fragment).atoms;
        let count = atoms.len();
        if count == 0 {
            return None;
        }
        let mut indices = Vec::new();
        for step in 0..=count {
            let index = (count - 1) - step % count;
            let atom = atoms[index];
            if self.graph().atom(atom).element == Element::C
                && self.graph().atom(atom).bonds.len() == 2
                && (indices.is_empty()
                    || self
                        .graph()
                        .neighbours(atom)
                        .contains(&atoms[(index + 1) % count]))
            {
                indices.push(index);
                if indices.len() == edge_length + 1 {
                    indices.reverse();
                    return Some(
                        indices[..edge_length]
                            .iter()
                            .map(|&i| char::from_u32('a' as u32 + i as u32).unwrap())
                            .collect(),
                    );
                }
            } else {
                indices.clear();
            }
        }
        None
    }
    fn possible_numerical_locants(
        &self,
        fragment: FragmentId,
        edge_length: usize,
    ) -> Option<Vec<String>> {
        let atoms = &self.graph().fragment(fragment).atoms;
        let count = atoms.len();
        if count == 0 {
            return None;
        }
        let mut locants = Vec::new();
        for step in 0..=count {
            let index = step % count;
            let atom = atoms[index];
            if self.graph().atom(atom).element == Element::C
                && self.graph().atom(atom).bonds.len() == 2
                && (locants.is_empty()
                    || self
                        .graph()
                        .neighbours(atom)
                        .contains(&atoms[(index + count - 1) % count]))
            {
                locants.push(self.graph().atom(atom).locants.first()?.clone());
                if locants.len() == edge_length + 1 {
                    return Some(locants);
                }
            } else {
                locants.clear();
            }
        }
        None
    }
    fn peripheral_atoms(&self, fragment: FragmentId) -> Result<Vec<AtomId>, GraphError> {
        let atoms = &self.graph().fragment(fragment).atoms;
        let first = *atoms
            .first()
            .ok_or_else(|| error("Fused ring has no atoms"))?;
        let last_index = self
            .graph()
            .neighbours(first)
            .into_iter()
            .filter_map(|atom| atoms.iter().position(|&a| a == atom))
            .filter(|&index| index != 1)
            .min()
            .ok_or_else(|| error("Unable to determine peripheral atoms"))?;
        Ok(atoms[..=last_index].to_vec())
    }
    fn higher_fusion(
        &mut self,
        descriptor: &str,
        child: FragmentId,
        parent: FragmentId,
    ) -> Result<(), GraphError> {
        let parts = split(descriptor, ':');
        if parts.len() != 2 {
            return Err(error(
                "Malformed fusion bracket: This is an OPSIN bug, check regexTokens.xml",
            ));
        }
        self.process_higher_order(
            child,
            parent,
            &split(&parts[0], ','),
            &split(&parts[1], ','),
        )
    }
    fn process_higher_order(
        &mut self,
        child: FragmentId,
        parent: FragmentId,
        child_locants: &[String],
        parent_locants: &[String],
    ) -> Result<(), GraphError> {
        let child_atoms = self
            .determine_atoms(child, child_locants, None)?
            .ok_or_else(|| error("Malformed fusion bracket!"))?;
        let parent_atoms = self
            .determine_atoms(parent, parent_locants, Some(child_atoms.len()))?
            .ok_or_else(|| error("Malformed fusion bracket!"))?;
        self.fuse_rings(child_atoms, parent_atoms)
    }
    fn atom_by_locant(&self, fragment: FragmentId, locant: &str) -> Result<AtomId, GraphError> {
        self.graph()
            .atom_by_locant(fragment, locant)
            .ok_or_else(|| error(format!("Could not find the atom with locant {locant}.")))
    }
    fn determine_atoms(
        &self,
        fragment: FragmentId,
        locants: &[String],
        expected: Option<usize>,
    ) -> Result<Option<Vec<AtomId>>, GraphError> {
        let atoms = self.peripheral_atoms(fragment)?;
        let first_locant = locants
            .first()
            .ok_or_else(|| error("Malformed fusion bracket!"))?;
        let last_locant = locants.last().unwrap();
        let first_atom = self.atom_by_locant(fragment, first_locant)?;
        let last_atom = self.atom_by_locant(fragment, last_locant)?;
        let first = atoms.iter().position(|&a| a == first_atom).ok_or_else(|| {
            error(format!(
                "{first_locant} refers to an atom that is not a peripheral atom!"
            ))
        })?;
        let last = atoms.iter().position(|&a| a == last_atom).ok_or_else(|| {
            error(format!(
                "{last_locant} refers to an atom that is not a peripheral atom!"
            ))
        })?;
        let intermediate = if locants.len() > 2 {
            locants[1..locants.len() - 1]
                .iter()
                .map(|locant| self.atom_by_locant(fragment, locant))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let walk = |ascending: bool| {
            let mut result = vec![atoms[first]];
            let mut index = first;
            while index != last {
                index = if ascending {
                    (index + 1) % atoms.len()
                } else {
                    (index + atoms.len() - 1) % atoms.len()
                };
                result.push(atoms[index]);
            }
            result
        };
        let ascending = walk(true);
        let mut selected = if expected.is_none_or(|n| n == ascending.len())
            && intermediate.iter().all(|atom| ascending.contains(atom))
        {
            Some(ascending)
        } else {
            None
        };
        if selected.is_none() || expected.is_none() {
            let descending = walk(false);
            if expected.is_none_or(|n| n == descending.len())
                && intermediate.iter().all(|atom| descending.contains(atom))
                && selected
                    .as_ref()
                    .is_none_or(|old| expected.is_some() || descending.len() < old.len())
            {
                selected = Some(descending);
            }
        }
        Ok(selected)
    }
    fn fuse_rings(
        &mut self,
        mut child_atoms: Vec<AtomId>,
        mut parent_atoms: Vec<AtomId>,
    ) -> Result<(), GraphError> {
        if parent_atoms.len() != child_atoms.len() {
            return Err(error(format!(
                "Problem with fusion descriptors: Parent atoms specified: {} Child atoms specified: {} These should have been identical!",
                parent_atoms.len(),
                child_atoms.len()
            )));
        }
        for index in (0..parent_atoms.len()).rev() {
            if let Some(&replacement) = self.replacements.get(&parent_atoms[index]) {
                parent_atoms[index] = replacement;
            }
            if let Some(&replacement) = self.replacements.get(&child_atoms[index]) {
                child_atoms[index] = replacement;
            }
        }
        for (&child, &parent) in child_atoms.iter().zip(&parent_atoms) {
            if self.graph().atom(child).spare_valency {
                self.graph_mut().atom_mut(parent).spare_valency = true;
            }
            if self.graph().atom(parent).element != self.graph().atom(child).element {
                return Err(error(
                    "Invalid fusion descriptor: Heteroatom placement is ambiguous as it is not present in both components of the fusion",
                ));
            }
            self.replacements.insert(child, parent);
        }
        let mut fusion_edges = HashSet::new();
        for pair in child_atoms.windows(2).chain(parent_atoms.windows(2)) {
            fusion_edges.insert(
                self.graph()
                    .bond_between(pair[0], pair[1])
                    .ok_or_else(|| error("Couldn't find specified bond"))?,
            );
        }
        let collect_bonds = |atoms: &[AtomId]| {
            let mut result = Vec::new();
            for &atom in atoms {
                for &bond in &self.graph().atom(atom).bonds {
                    if !fusion_edges.contains(&bond) && !result.contains(&bond) {
                        result.push(bond);
                    }
                }
            }
            result
        };
        let to_parent = collect_bonds(&child_atoms);
        let to_child = collect_bonds(&parent_atoms);
        for bond in to_parent {
            let bond = self.graph().bond(bond);
            let from = child_atoms
                .iter()
                .position(|&a| a == bond.from)
                .map_or(bond.from, |i| parent_atoms[i]);
            let to = child_atoms
                .iter()
                .position(|&a| a == bond.to)
                .map_or(bond.to, |i| parent_atoms[i]);
            self.state.fragment_manager.create_bond(from, to, 1)?;
        }
        // The upstream temporary bond is attached to just one child endpoint and
        // is deliberately absent from FragmentManager's bond sets.
        for bond in to_child {
            let bond = self.graph().bond(bond);
            let from = parent_atoms
                .iter()
                .position(|&a| a == bond.from)
                .map_or(bond.from, |i| child_atoms[i]);
            let to = parent_atoms
                .iter()
                .position(|&a| a == bond.to)
                .map_or(bond.to, |i| child_atoms[i]);
            if from == to {
                return Err(error("Bonds must be made between different atoms"));
            }
            let attach = if child_atoms.contains(&from) {
                from
            } else {
                to
            };
            // Atom.addBond uses Bond.equals: endpoint equality is unordered
            // and ignores order, even for this one-sided temporary bond.
            if self.graph().atom(attach).bonds.iter().any(|&id| {
                let existing = self.graph().bond(id);
                (existing.from == from && existing.to == to)
                    || (existing.from == to && existing.to == from)
            }) {
                return Err(error(
                    "Atom already has given bond (This is not allowed as this would give two bonds between the same atoms!)",
                ));
            }
            let id = BondId(self.graph().bonds.len());
            self.graph_mut().bonds.push(Bond {
                id,
                active: true,
                from,
                to,
                order: 1,
                stereo: None,
                smiles_direction: None,
            });
            self.graph_mut().atom_mut(attach).bonds.push(id);
        }
        Ok(())
    }
    fn benzo_specific_fusion(&mut self, benzo: NodeId, parent: NodeId) -> Result<(), GraphError> {
        let benzene = self.fragment(benzo)?;
        let parent_ring = self.fragment(parent)?;
        self.simple_fusion(None, benzene, parent_ring)?;
        self.state
            .fragment_manager
            .incorporate_fragment(benzene, parent_ring)?;
        self.remove_merged_atoms();
        fused_ring_numberer::number_fused_ring(self.graph_mut(), parent_ring)?;
        self.set_benzo_heteroatom_positioning(benzo, parent_ring)
    }
    fn set_benzo_heteroatom_positioning(
        &mut self,
        benzo: NodeId,
        fragment: FragmentId,
    ) -> Result<(), GraphError> {
        let Some(locant) = self
            .arena
            .previous_sibling(benzo)
            .filter(|&node| self.arena[node].name == "locant")
        else {
            return Ok(());
        };
        let locants = split(&self.value(locant), ',');
        if !locants
            .iter()
            .all(|locant| fragment_tools::is_numeric_locant(locant))
        {
            return Ok(());
        }
        let parent = self.arena[benzo]
            .parent
            .ok_or_else(|| error("Benzo group has no parent"))?;
        let unlocanted_suffixes = self
            .arena
            .children_named(parent, "suffix")
            .iter()
            .filter(|&&node| self.arena[node].attribute("locant").is_none())
            .count();
        if locants.len() == unlocanted_suffixes {
            return Ok(());
        }
        let heteroatoms = self
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .copied()
            .filter(|&atom| self.graph().atom(atom).element != Element::C)
            .collect::<Vec<_>>();
        if locants.len() == heteroatoms.len() {
            let potential_root = locants.len() == 1 && self.arena.previous_sibling(locant).is_none() && crate::component_processor::ComponentProcessor::check_locant_present_on_potential_root(self.state, self.arena, parent, &locants[0]).map_err(|err| error(err.to_string()))?;
            if !potential_root {
                let elements = heteroatoms
                    .iter()
                    .map(|&atom| self.graph().atom(atom).element)
                    .collect::<Vec<_>>();
                for atom in heteroatoms {
                    self.graph_mut().atom_mut(atom).element = Element::C;
                }
                for (locant, element) in locants.iter().zip(elements) {
                    let atom = self.atom_by_locant(fragment, locant)?;
                    self.graph_mut().atom_mut(atom).element = element;
                }
                self.arena.detach(locant);
            }
        } else if locants.len() > 1 {
            return Err(error(
                "Unable to assign all locants to benzo-fused ring or multiplier was mising",
            ));
        }
        Ok(())
    }
}

fn numerical_and_letter_components(descriptor: &str) -> (String, String) {
    let parts = split(descriptor, '-');
    if parts.len() == 2 {
        return (parts[0].clone(), parts[1].clone());
    }
    if parts[0].contains(',') {
        (parts[0].clone(), String::new())
    } else {
        (String::new(), parts[0].clone())
    }
}
