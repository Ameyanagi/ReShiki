//! Ordered suffix effects translated from OPSIN `SuffixApplier.java`.
//!
//! Source: OPSIN 2.9.0, commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and contributors; MIT (see the retained license).
//! Hydroxy conversion rules are deliberately handled in ComponentProcessor,
//! before this phase, exactly as in the source.

use std::sync::LazyLock;

use regex::Regex;

use crate::api::{OpsinWarning, ParsingError, WarningKind};
use crate::build_state::BuildState;
use crate::graph::{AtomId, Element, FragmentId, Graph};
use crate::isotope_specification_parser::parse_isotope_specification;
use crate::parse_tree::{Arena, NodeId};
use crate::suffix_rules::{SuffixRule, SuffixRuleType, SuffixRules};
use crate::xml_declarations::*;
use crate::{ambiguity, fragment_tools, valence};

static JAVA_DECIMAL_DIGIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\A\p{Nd}").expect("Java Character.isDigit decimal digit category")
});

pub struct SuffixApplier<'a> {
    state: &'a mut BuildState,
    suffix_rules: &'a SuffixRules,
}

impl<'a> SuffixApplier<'a> {
    pub fn new(state: &'a mut BuildState, suffix_rules: &'a SuffixRules) -> Self {
        Self {
            state,
            suffix_rules,
        }
    }

    pub fn is_group_type_with_specific_suffix_rules(&self, group_type: &str) -> bool {
        self.suffix_rules
            .is_group_type_with_specific_suffix_rules(group_type)
    }

    pub fn get_suffix_rule_tags(
        &self,
        suffix_type_to_use: &str,
        suffix_value: &str,
        subgroup_type: Option<&str>,
    ) -> Result<&[SuffixRule], ParsingError> {
        self.suffix_rules
            .rule_tags(suffix_type_to_use, suffix_value, subgroup_type)
            .map_err(|error| ParsingError(error.to_string()))
    }

    /// Apply the final suffix effects, including source-ordered deferred merges.
    /// The original parent atom list remains fixed even when suffix atoms merge.
    pub fn resolve_suffixes(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        suffixes: &[NodeId],
    ) -> Result<(), ParsingError> {
        let fragment = arena[group].fragment.ok_or_else(|| {
            ParsingError(
                "OPSIN Bug: Group was expected to have an associated fragment but it wasn't found"
                    .into(),
            )
        })?;
        let parent = self.state.fragment_manager.graph.fragment(fragment);
        let original_atoms = parent.atoms.clone();
        let group_type = parent.fragment_type.clone();
        let subgroup_type = parent.sub_type.clone();
        let fragment_token_text = self
            .state
            .fragment_manager
            .token_for_fragment(fragment)
            .map(|token| arena.value(token))
            .unwrap_or_default();
        let suffix_type = if self.is_group_type_with_specific_suffix_rules(&group_type) {
            group_type
        } else {
            STANDARDGROUP_TYPE_VAL.to_owned()
        };
        if let Some(associated) = self.state.xml_suffix_map.get_mut(&group) {
            associated.clear();
        }

        // LinkedHashMap order in the source: first occurrence of each value.
        let mut values: Vec<(String, Vec<NodeId>)> = Vec::new();
        for &suffix in suffixes {
            let value = arena[suffix]
                .attribute(VALUE_ATR)
                .ok_or_else(|| {
                    ParsingError("OPSIN Bug: Suffix did not have a value attribute".into())
                })?
                .to_owned();
            if let Some((_, matching)) = values.iter_mut().find(|(v, _)| *v == value) {
                matching.push(suffix);
            } else {
                values.push((value, vec![suffix]));
            }
            if let Some(suffix_fragment) = arena[suffix].fragment {
                let suffix_text = self
                    .state
                    .fragment_manager
                    .token_for_fragment(suffix_fragment)
                    .map(|token| arena.value(token))
                    .unwrap_or_default();
                if let Some(isotope) = arena
                    .next_sibling(suffix)
                    .filter(|&id| arena[id].name == ISOTOPESPECIFICATION_EL)
                {
                    if arena[isotope].attribute(TYPE_ATR) != Some(BOUGHTONSYSTEM_TYPE_VAL) {
                        return Err(ParsingError(
                            "Unexpected isotope specification after suffix".into(),
                        ));
                    }
                    self.apply_isotope_to_suffix(
                        arena,
                        suffix_fragment,
                        isotope,
                        false,
                        &suffix_text,
                    )?;
                }
                let mut isotope = arena.previous_sibling(suffix);
                while let Some(id) = isotope.filter(|&id| {
                    arena[id].name == ISOTOPESPECIFICATION_EL
                        && arena[id].attribute(TYPE_ATR) == Some(IUPACSYSTEM_TYPE_VAL)
                }) {
                    let previous = arena.previous_sibling(id);
                    self.apply_isotope_to_suffix(arena, suffix_fragment, id, true, &suffix_text)?;
                    isotope = previous;
                }
            }
        }

        let mut redetect_cycles = false;
        let mut to_merge = Vec::new();
        for (value, matching_suffixes) in values {
            let rules = self
                .get_suffix_rule_tags(&suffix_type, &value, Some(&subgroup_type))?
                .to_vec();
            let mut attachment_candidates: Option<Vec<AtomId>> = None;
            for (suffix_index, &suffix) in matching_suffixes.iter().enumerate() {
                let mut suffix_fragment = None;
                for rule in &rules {
                    match rule.kind {
                        SuffixRuleType::AddGroup => {
                            if suffix_fragment.is_some() {
                                return Err(ParsingError(format!(
                                    "OPSIN bug: Suffix may only have one addgroup rule: {}",
                                    arena.value(suffix)
                                )));
                            }
                            let sf = arena[suffix].fragment.ok_or_else(|| ParsingError(
                                "OPSIN Bug: Suffix was expected to have an associated fragment but it wasn't found".into()
                            ))?;
                            suffix_fragment = Some(sf);
                            let dummy = first_atom(&self.state.fragment_manager.graph, sf)?;
                            if self
                                .state
                                .fragment_manager
                                .graph
                                .atom(dummy)
                                .bonds
                                .is_empty()
                            {
                                return Err(ParsingError("OPSIN Bug: Dummy atom in suffix should have at least one bond to it".into()));
                            }
                            if arena[suffix].attribute(SUBTYPE_ATR) == Some(CYCLEFORMER_SUBTYPE_VAL)
                            {
                                self.process_cycle_forming_suffix(arena, sf, fragment, suffix)?;
                                redetect_cycles = true;
                            } else {
                                let order =
                                    self.state.fragment_manager.graph.incoming_valency(dummy);
                                let parent_atom = match self.get_frag_atom_to_use(
                                    arena,
                                    fragment,
                                    suffix,
                                    &suffix_type,
                                )? {
                                    Some(atom) => atom,
                                    None => {
                                        if attachment_candidates.is_none() {
                                            let required = matching_suffixes.len();
                                            let first_parent = original_atoms.first().copied().ok_or_else(|| {
                                                ParsingError("OPSIN Bug: List of atoms to add suffix to was empty".into())
                                            })?;
                                            let graph = &self.state.fragment_manager.graph;
                                            // Source passes the current fragment's atom list here,
                                            // while retaining original_atoms for the default atom.
                                            let mut candidates = fragment_tools::find_n_atoms_for_substitution(
                                                graph, &graph.fragment(fragment).atoms, Some(first_parent), required,
                                                order, true, false,
                                            ).map_err(as_parsing_error)?.ok_or_else(|| {
                                                ParsingError(format!("No suitable atom found to attach {value} suffix"))
                                            })?;
                                            if candidates.iter().any(|&atom| {
                                                fragment_tools::is_characteristic_atom(graph, atom)
                                            }) {
                                                return Err(ParsingError(
                                                    "No suitable atom found to attach suffix"
                                                        .into(),
                                                ));
                                            }
                                            if rule.attribute(SUFFIXRULES_KETONELOCANT_ATR)
                                                == Some("yes")
                                                && !graph.atom(first_parent).in_cycle
                                            {
                                                let pro_ketone =
                                                    self.get_pro_ketone_positions(&candidates);
                                                if pro_ketone.len() >= required {
                                                    candidates = pro_ketone;
                                                }
                                            }
                                            let standard_first = required == 1
                                                && matches!(
                                                    subgroup_type.as_str(),
                                                    ALKANESTEM_SUBTYPE_VAL | HETEROSTEM_SUBTYPE_VAL
                                                )
                                                && candidates[0] == first_atom(graph, fragment)?;
                                            if !standard_first
                                                && ambiguity::is_substitution_ambiguous(
                                                    graph,
                                                    &candidates,
                                                    required,
                                                )
                                                .map_err(as_parsing_error)?
                                            {
                                                self.add_ambiguity(format!(
                                                    "Addition of {value} suffix to: {}",
                                                    arena.value(group)
                                                ));
                                            }
                                            attachment_candidates = Some(candidates);
                                        }
                                        attachment_candidates.as_ref().unwrap()[suffix_index]
                                    }
                                };
                                for bond_id in
                                    self.state.fragment_manager.graph.atom(dummy).bonds.clone()
                                {
                                    let bond =
                                        self.state.fragment_manager.graph.bond(bond_id).clone();
                                    let suffix_atom = bond.other_atom(dummy).ok_or_else(|| {
                                        ParsingError(
                                            "OPSIN Bug: Bond did not involve suffix dummy atom"
                                                .into(),
                                        )
                                    })?;
                                    self.state
                                        .fragment_manager
                                        .create_bond(parent_atom, suffix_atom, bond.order)
                                        .map_err(as_parsing_error)?;
                                    self.state.fragment_manager.remove_bond(bond_id);
                                    if self
                                        .state
                                        .fragment_manager
                                        .graph
                                        .incoming_valency(parent_atom)
                                        > 2
                                        && matches!(value.as_str(), "aldehyde" | "al" | "aldoxime")
                                    {
                                        let aldehyde = if self
                                            .state
                                            .fragment_manager
                                            .graph
                                            .atom(suffix_atom)
                                            .locants
                                            .first()
                                            .map(String::as_str)
                                            == Some("X")
                                        {
                                            suffix_atom
                                        } else {
                                            parent_atom
                                        };
                                        self.state
                                            .fragment_manager
                                            .graph
                                            .atom_mut(aldehyde)
                                            .properties
                                            .is_aldehyde = true;
                                    }
                                }
                            }
                        }
                        SuffixRuleType::ChangeCharge => {
                            let charge = rule_number(rule, SUFFIXRULES_CHARGE_ATR)?;
                            let protons = rule_number(rule, SUFFIXRULES_PROTONS_ATR)?;
                            if arena[suffix].attribute(SUFFIXPREFIX_ATR).is_none() {
                                if let Some(atom) = self.get_frag_atom_to_use(
                                    arena,
                                    fragment,
                                    suffix,
                                    &suffix_type,
                                )? {
                                    add_charge_and_protons(
                                        &mut self.state.fragment_manager.graph,
                                        atom,
                                        charge,
                                        protons,
                                    );
                                } else {
                                    self.apply_unlocanted_charge_modification(
                                        &original_atoms,
                                        charge,
                                        protons,
                                        &fragment_token_text,
                                    )?;
                                }
                            } else {
                                let sf = suffix_fragment.ok_or_else(|| ParsingError(
                                    "OPSIN bug: ordering of elements in suffixRules.xml wrong; changeCharge found before addGroup".into()
                                ))?;
                                let atom = self.single_interfragment_suffix_atom(sf)?;
                                add_charge_and_protons(
                                    &mut self.state.fragment_manager.graph,
                                    atom,
                                    charge,
                                    protons,
                                );
                            }
                        }
                        SuffixRuleType::SetOutAtom => {
                            let out_valency = rule
                                .attribute(SUFFIXRULES_OUTVALENCY_ATR)
                                .map(|v| parse_integer(v, SUFFIXRULES_OUTVALENCY_ATR))
                                .transpose()?
                                .unwrap_or(1);
                            if arena[suffix].attribute(SUFFIXPREFIX_ATR).is_none() {
                                if !to_merge.is_empty() {
                                    self.merge_suffix_frags(fragment, &to_merge)?;
                                    to_merge.clear();
                                }
                                match self.get_frag_atom_to_use(
                                    arena,
                                    fragment,
                                    suffix,
                                    &suffix_type,
                                )? {
                                    Some(atom) => self.state.fragment_manager.graph.add_out_atom(
                                        fragment,
                                        atom,
                                        out_valency,
                                        true,
                                    ),
                                    None => {
                                        let atom = first_atom(
                                            &self.state.fragment_manager.graph,
                                            fragment,
                                        )?;
                                        self.state.fragment_manager.graph.add_out_atom(
                                            fragment,
                                            atom,
                                            out_valency,
                                            false,
                                        );
                                    }
                                }
                            } else {
                                let sf = suffix_fragment.ok_or_else(|| ParsingError(
                                    "OPSIN bug: ordering of elements in suffixRules.xml wrong; setOutAtom found before addGroup".into()
                                ))?;
                                let atom = self.single_interfragment_suffix_atom(sf)?;
                                self.state.fragment_manager.graph.add_out_atom(
                                    sf,
                                    atom,
                                    out_valency,
                                    true,
                                );
                            }
                        }
                        SuffixRuleType::SetAcidicElement => {
                            let element = rule
                                .attribute(SUFFIXRULES_ELEMENT_ATR)
                                .and_then(Element::from_symbol)
                                .ok_or_else(|| {
                                    ParsingError(
                                        "OPSIN Bug: Invalid acidic element in suffix rule".into(),
                                    )
                                })?;
                            let sf = suffix_fragment.ok_or_else(|| ParsingError(
                                "OPSIN Bug: setAcidicElement requires an associated suffix fragment".into()
                            ))?;
                            self.swap_elements_such_that_this_element_is_acidic(sf, element)?;
                        }
                        SuffixRuleType::AddSuffixPrefixIfNonePresentAndCyclic
                        | SuffixRuleType::AddFunctionalAtomsToHydroxyGroups
                        | SuffixRuleType::ChargeHydroxyGroups
                        | SuffixRuleType::RemoveTerminalOxygen
                        | SuffixRuleType::ConvertHydroxyGroupsToOutAtoms
                        | SuffixRuleType::ConvertHydroxyGroupsToPositiveCharge => {
                            // Already processed by ComponentProcessor.
                        }
                    }
                }
                if let Some(sf) = suffix_fragment {
                    to_merge.push(sf);
                    arena[suffix].fragment = None;
                }
            }
        }
        self.merge_suffix_frags(fragment, &to_merge)?;
        if redetect_cycles {
            self.state
                .fragment_manager
                .graph
                .assign_cycle_membership(fragment);
        }
        Ok(())
    }

    fn merge_suffix_frags(
        &mut self,
        parent: FragmentId,
        suffix_fragments: &[FragmentId],
    ) -> Result<(), ParsingError> {
        for &suffix in suffix_fragments {
            let dummy = first_atom(&self.state.fragment_manager.graph, suffix)?;
            self.state
                .fragment_manager
                .remove_atom_and_associated_bonds(dummy);
            let locants: Vec<_> = self
                .state
                .fragment_manager
                .graph
                .fragment(suffix)
                .locants
                .keys()
                .cloned()
                .collect();
            for locant in locants {
                if starts_with_java_digit(&locant)
                    && self
                        .state
                        .fragment_manager
                        .graph
                        .atom_by_locant(parent, &locant)
                        .is_some()
                {
                    let atom = self
                        .state
                        .fragment_manager
                        .graph
                        .atom_by_locant(suffix, &locant)
                        .unwrap();
                    self.state
                        .fragment_manager
                        .graph
                        .atom_mut(atom)
                        .locants
                        .retain(|l| *l != locant);
                    self.state
                        .fragment_manager
                        .graph
                        .fragment_mut(suffix)
                        .locants
                        .remove(&locant);
                }
            }
            self.state
                .fragment_manager
                .incorporate_fragment(suffix, parent)
                .map_err(as_parsing_error)?;
        }
        Ok(())
    }

    fn apply_isotope_to_suffix(
        &mut self,
        arena: &mut Arena,
        fragment: FragmentId,
        isotope_node: NodeId,
        must_apply: bool,
        suffix_text: &str,
    ) -> Result<(), ParsingError> {
        let specification = parse_isotope_specification(arena, isotope_node)?;
        let element = specification.element;
        if specification.locants.is_some() && !must_apply {
            return Ok(());
        }
        if let Some(locants) = specification.locants {
            for locant in locants {
                let atom =
                    atom_by_locant_or_throw(&self.state.fragment_manager.graph, fragment, &locant)?;
                if element == Element::H {
                    self.add_isotopic_hydrogen(fragment, atom, specification.isotope)?;
                } else {
                    if self.state.fragment_manager.graph.atom(atom).element != element {
                        return Err(ParsingError(format!(
                            "The atom at locant: {locant} was not a {element}"
                        )));
                    }
                    self.state.fragment_manager.graph.atom_mut(atom).isotope =
                        Some(specification.isotope);
                }
            }
        } else {
            let graph = &self.state.fragment_manager.graph;
            let mut atoms = graph.fragment(fragment).atoms.clone();
            if atoms.is_empty() {
                return Err(ParsingError(
                    "OPSIN Bug: Suffix fragment did not have a dummy atom".into(),
                ));
            }
            atoms.remove(0);
            let candidates = if element == Element::H {
                fragment_tools::find_n_atoms_for_substitution(
                    graph,
                    &atoms,
                    None,
                    specification.multiplier,
                    1,
                    true,
                    false,
                )
                .map_err(as_parsing_error)?
            } else {
                let candidates: Vec<_> = atoms
                    .into_iter()
                    .filter(|&atom| graph.atom(atom).element == element)
                    .collect();
                (candidates.len() >= specification.multiplier).then_some(candidates)
            };
            let Some(candidates) = candidates else {
                if must_apply {
                    return Err(ParsingError(if element == Element::H {
                        "Failed to find sufficient hydrogen atoms for unlocanted hydrogen isotope replacement".into()
                    } else {
                        format!("Failed to find sufficient atoms for {element} isotope replacement")
                    }));
                }
                return Ok(());
            };
            if ambiguity::is_substitution_ambiguous(graph, &candidates, specification.multiplier)
                .map_err(as_parsing_error)?
            {
                let label = if element == Element::H {
                    "Position of hydrogen isotope on"
                } else {
                    "Position of isotope on"
                };
                self.add_ambiguity(format!("{label} {suffix_text}"));
            }
            for &atom in candidates.iter().take(specification.multiplier) {
                if element == Element::H {
                    self.add_isotopic_hydrogen(fragment, atom, specification.isotope)?;
                } else {
                    self.state.fragment_manager.graph.atom_mut(atom).isotope =
                        Some(specification.isotope);
                }
            }
        }
        arena.detach(isotope_node);
        Ok(())
    }

    fn add_isotopic_hydrogen(
        &mut self,
        fragment: FragmentId,
        parent: AtomId,
        isotope: u32,
    ) -> Result<(), ParsingError> {
        let hydrogen = self
            .state
            .fragment_manager
            .create_atom(Element::H, fragment);
        self.state.fragment_manager.graph.atom_mut(hydrogen).isotope = Some(isotope);
        self.state
            .fragment_manager
            .create_bond(parent, hydrogen, 1)
            .map_err(as_parsing_error)?;
        Ok(())
    }

    fn get_pro_ketone_positions(&self, atoms: &[AtomId]) -> Vec<AtomId> {
        let graph = &self.state.fragment_manager.graph;
        atoms
            .iter()
            .copied()
            .filter(|&atom| {
                let bonds = &graph.atom(atom).bonds;
                bonds.len() == 2
                    && bonds.iter().all(|&bond| {
                        let bond = graph.bond(bond);
                        bond.order == 1
                            && bond
                                .other_atom(atom)
                                .is_some_and(|other| graph.atom(other).element == Element::C)
                    })
            })
            .collect()
    }

    fn process_cycle_forming_suffix(
        &mut self,
        arena: &Arena,
        suffix_fragment: FragmentId,
        parent: FragmentId,
        suffix: NodeId,
    ) -> Result<(), ParsingError> {
        let graph = &self.state.fragment_manager.graph;
        let dummy_atoms: Vec<_> = graph
            .fragment(suffix_fragment)
            .atoms
            .iter()
            .copied()
            .filter(|&atom| graph.atom(atom).element == Element::R)
            .collect();
        if dummy_atoms.len() != 2 {
            return Err(ParsingError(
                "OPSIN bug: Incorrect number of R atoms associated with cyclic suffix".into(),
            ));
        }
        if dummy_atoms
            .iter()
            .any(|&atom| graph.atom(atom).bonds.is_empty())
        {
            return Err(ParsingError(
                "OPSIN Bug: Dummy atoms in suffix should have at least one bond to them".into(),
            ));
        }
        let (parent1, mut parent2) = if let Some(locant) = arena[suffix].attribute(LOCANT_ATR) {
            let locants = split_java(locant);
            match locants.as_slice() {
                [a, b] => (
                    atom_by_locant_or_throw(graph, parent, a)?,
                    atom_by_locant_or_throw(graph, parent, b)?,
                ),
                [b] => (
                    atom_by_locant_or_throw(graph, parent, "1")?,
                    atom_by_locant_or_throw(graph, parent, b)?,
                ),
                _ => {
                    return Err(ParsingError(format!(
                        "Incorrect number of locants associated with cycle forming suffix, expected 2 found: {}",
                        locants.len()
                    )));
                }
            }
        } else if let Some(locant_id) = arena[suffix].attribute(LOCANTID_ATR) {
            let ids = split_java(locant_id);
            if ids.len() != 2 {
                return Err(ParsingError(
                    "OPSIN bug: Should be exactly 2 locants associated with a cyclic suffix".into(),
                ));
            }
            (
                atom_by_java_id_or_throw(graph, parent, ids[0])?,
                atom_by_java_id_or_throw(graph, parent, ids[1])?,
            )
        } else {
            let length = chain_length(graph, parent);
            if length > 1 && length == graph.fragment(parent).atoms.len() {
                (
                    atom_by_locant_or_throw(graph, parent, "1")?,
                    atom_by_locant_or_throw(graph, parent, &length.to_string())?,
                )
            } else {
                let hydroxy =
                    fragment_tools::find_hydroxy_groups(graph, parent).map_err(as_parsing_error)?;
                if hydroxy.len() == 1 && graph.atom_by_locant(parent, "1").is_some() {
                    (atom_by_locant_or_throw(graph, parent, "1")?, hydroxy[0])
                } else {
                    return Err(ParsingError(format!(
                        "cycle forming suffix: {} should be locanted!",
                        arena.value(suffix)
                    )));
                }
            }
        };
        if parent1 == parent2 {
            return Err(ParsingError(format!(
                "cycle forming suffix: {} attempted to form a cycle involving the same atom twice!",
                arena.value(suffix)
            )));
        }
        if graph.fragment(parent).fragment_type == CARBOHYDRATE_TYPE_VAL {
            self.state
                .fragment_manager
                .remove_terminal_oxygen(parent1, 2)
                .map_err(as_parsing_error)?;
            self.state
                .fragment_manager
                .remove_terminal_oxygen(parent1, 1)
                .map_err(as_parsing_error)?;
            let graph = &self.state.fragment_manager.graph;
            let hydroxy = fragment_tools::find_hydroxy_like_terminal_atoms(
                graph,
                &graph.neighbours(parent2),
                Element::O,
            );
            if hydroxy.len() != 1 {
                return Err(ParsingError("The second locant of a carbohydrate lactone should point to a carbon in the chain with a hydroxyl group".into()));
            }
            self.state
                .fragment_manager
                .remove_terminal_atom(hydroxy[0])
                .map_err(as_parsing_error)?;
        } else if graph.atom(parent2).element == Element::O {
            let neighbours = graph.neighbours(parent2);
            if neighbours.len() == 1 {
                let suffix_neighbours = graph.neighbours(dummy_atoms[1]);
                if suffix_neighbours.len() == 1
                    && graph.atom(suffix_neighbours[0]).element == Element::O
                {
                    self.state
                        .fragment_manager
                        .remove_atom_and_associated_bonds(parent2);
                    parent2 = neighbours[0];
                }
            }
        }
        self.make_bonds_to_suffix(parent1, dummy_atoms[0])?;
        self.make_bonds_to_suffix(parent2, dummy_atoms[1])?;
        self.state
            .fragment_manager
            .remove_atom_and_associated_bonds(dummy_atoms[1]);
        Ok(())
    }

    fn get_frag_atom_to_use(
        &self,
        arena: &Arena,
        fragment: FragmentId,
        suffix: NodeId,
        suffix_type: &str,
    ) -> Result<Option<AtomId>, ParsingError> {
        let graph = &self.state.fragment_manager.graph;
        if let Some(locant) = arena[suffix].attribute(LOCANT_ATR) {
            return atom_by_locant_or_throw(graph, fragment, locant).map(Some);
        }
        if let Some(id) = arena[suffix].attribute(LOCANTID_ATR) {
            return atom_by_java_id_or_throw(graph, fragment, id).map(Some);
        }
        if let Some(id) = arena[suffix].attribute(DEFAULTLOCANTID_ATR) {
            return atom_by_java_id_or_throw(graph, fragment, id).map(Some);
        }
        if matches!(
            suffix_type,
            ACIDSTEM_TYPE_VAL | NONCARBOXYLICACID_TYPE_VAL | CHALCOGENACIDSTEM_TYPE_VAL
        ) {
            return first_atom(graph, fragment).map(Some);
        }
        Ok(None)
    }

    fn apply_unlocanted_charge_modification(
        &mut self,
        atoms: &[AtomId],
        charge_change: i32,
        proton_change: i32,
        token_text: &str,
    ) -> Result<(), ParsingError> {
        if atoms.is_empty() {
            return Err(ParsingError(
                "OPSIN Bug: List of atoms to add charge suffix to was empty".into(),
            ));
        }
        let graph = &self.state.fragment_manager.graph;
        let mut nitrogens = Vec::new();
        let mut heteroatoms = Vec::new();
        let mut carbons = Vec::new();
        let mut charged = Vec::new();
        for &atom in atoms {
            let a = graph.atom(atom);
            let Some(stable_valencies) =
                valence::possible_valencies(a.element, a.charge + charge_change)
            else {
                continue;
            };
            let default_valency = a
                .lambda_convention_valency
                .or_else(|| valence::default_valency(a.element))
                .ok_or_else(|| {
                    ParsingError(format!(
                        "OPSIN Bug: No default valency for {} when applying a charge suffix",
                        a.element
                    ))
                })?;
            let resulting_valency =
                default_valency + a.protons_explicitly_added_or_removed + proton_change;
            if !stable_valencies.contains(&resulting_valency) {
                continue;
            }
            if proton_change < 0 {
                let mut hydrogens = if a.implicit_hydrogen_allowed {
                    (graph.determine_valency(atom, true)
                        - graph.incoming_valency(atom)
                        - a.out_valency)
                        .max(0)
                } else {
                    0
                };
                if a.spare_valency
                    && !graph
                        .fragment(a.fragment)
                        .indicated_hydrogens
                        .contains(&atom)
                {
                    hydrogens -= 1;
                }
                if hydrogens < 1 {
                    continue;
                }
            }
            if a.charge != 0 {
                charged.push(atom);
            } else if a.element == Element::N {
                nitrogens.push(atom);
            } else if a.element != Element::C {
                heteroatoms.push(atom);
            } else {
                carbons.push(atom);
            }
        }
        let candidates = if !nitrogens.is_empty() {
            if graph.fragment(graph.atom(atoms[0]).fragment).fragment_type == AMINOACID_TYPE_VAL
                && nitrogens.contains(&atoms[0])
            {
                vec![atoms[0]]
            } else {
                nitrogens
            }
        } else if !heteroatoms.is_empty() {
            heteroatoms
        } else if !carbons.is_empty() {
            carbons
        } else if !charged.is_empty() {
            charged
        } else {
            atoms.to_vec()
        };
        let chosen = candidates[0];
        if !ambiguity::all_atoms_equivalent(graph, &candidates).map_err(as_parsing_error)? {
            self.add_ambiguity(format!("Addition of charge suffix to: {token_text}"));
        }
        add_charge_and_protons(
            &mut self.state.fragment_manager.graph,
            chosen,
            charge_change,
            proton_change,
        );
        Ok(())
    }

    fn swap_elements_such_that_this_element_is_acidic(
        &mut self,
        fragment: FragmentId,
        element: Element,
    ) -> Result<(), ParsingError> {
        let graph = &mut self.state.fragment_manager.graph;
        for atom in graph.fragment(fragment).functional_atoms.clone() {
            let assignment = graph
                .atom(atom)
                .properties
                .ambiguous_element_assignment
                .clone();
            if let Some(other) = assignment
                .iter()
                .copied()
                .find(|&other| graph.atom(other).element == element)
            {
                if atom != other {
                    let locants = graph.atom(atom).locants.clone();
                    let other_locants = graph.atom(other).locants.clone();
                    graph.clear_locants(atom);
                    graph.clear_locants(other);
                    for locant in locants {
                        graph.add_locant(other, locant);
                    }
                    for locant in other_locants {
                        graph.add_locant(atom, locant);
                    }
                    let original_element = graph.atom(atom).element;
                    graph.atom_mut(atom).element = graph.atom(other).element;
                    graph.atom_mut(other).element = original_element;
                }
                // The source mutates this atom's Set instance. Original
                // replacements share it; copied fragments hold independent
                // sets even when their members are equal.
                graph.remove_ambiguous_element_assignment_members(atom, &[other, atom]);
                return Ok(());
            }
        }
        Err(ParsingError(format!(
            "Unable to find potential acidic atom with element: {element}"
        )))
    }

    fn make_bonds_to_suffix(&mut self, parent: AtomId, dummy: AtomId) -> Result<(), ParsingError> {
        for bond_id in self.state.fragment_manager.graph.atom(dummy).bonds.clone() {
            let bond = self.state.fragment_manager.graph.bond(bond_id).clone();
            let suffix_atom = bond
                .other_atom(dummy)
                .ok_or_else(|| ParsingError("OPSIN Bug: Invalid suffix dummy bond".into()))?;
            self.state
                .fragment_manager
                .create_bond(parent, suffix_atom, bond.order)
                .map_err(as_parsing_error)?;
            self.state.fragment_manager.remove_bond(bond_id);
        }
        Ok(())
    }

    fn single_interfragment_suffix_atom(
        &self,
        suffix_fragment: FragmentId,
    ) -> Result<AtomId, ParsingError> {
        let bonds = self
            .state
            .fragment_manager
            .inter_fragment_bonds(suffix_fragment)
            .map_err(as_parsing_error)?;
        if bonds.len() != 1 {
            return Err(ParsingError(
                "OPSIN bug: Wrong number of bonds between suffix and group".into(),
            ));
        }
        let bond = self
            .state
            .fragment_manager
            .graph
            .bond(*bonds.iter().next().unwrap());
        Ok(
            if self.state.fragment_manager.graph.atom(bond.from).fragment == suffix_fragment {
                bond.from
            } else {
                bond.to
            },
        )
    }

    fn add_ambiguity(&mut self, message: String) {
        self.state.warnings.push(OpsinWarning {
            kind: WarningKind::AppearsAmbiguous,
            message,
        });
    }
}

fn as_parsing_error(error: impl std::fmt::Display) -> ParsingError {
    ParsingError(error.to_string())
}

fn first_atom(graph: &Graph, fragment: FragmentId) -> Result<AtomId, ParsingError> {
    graph
        .fragment(fragment)
        .atoms
        .first()
        .copied()
        .ok_or_else(|| ParsingError("OPSIN Bug: Fragment has no atoms".into()))
}

fn atom_by_locant_or_throw(
    graph: &Graph,
    fragment: FragmentId,
    locant: &str,
) -> Result<AtomId, ParsingError> {
    graph
        .atom_by_locant(fragment, locant)
        .ok_or_else(|| ParsingError(format!("Could not find the atom with locant {locant}.")))
}

fn atom_by_java_id_or_throw(
    graph: &Graph,
    fragment: FragmentId,
    value: &str,
) -> Result<AtomId, ParsingError> {
    let id = parse_integer(value, LOCANTID_ATR)?;
    // OPSIN's global atom allocator is one-based; arena atom IDs are zero-based.
    let atom = id
        .checked_sub(1)
        .and_then(|id| usize::try_from(id).ok())
        .map(AtomId)
        .filter(|atom| graph.fragment(fragment).atoms.contains(atom));
    atom.ok_or_else(|| ParsingError(format!("Couldn't find atom with id {id}.")))
}

fn add_charge_and_protons(graph: &mut Graph, atom: AtomId, charge: i32, protons: i32) {
    let atom = graph.atom_mut(atom);
    atom.charge += charge;
    atom.protons_explicitly_added_or_removed += protons;
}

fn chain_length(graph: &Graph, fragment: FragmentId) -> usize {
    let mut length = 0;
    let mut previous = None;
    while let Some(atom) = graph.atom_by_locant(fragment, &(length + 1).to_string()) {
        if previous.is_some_and(|previous| graph.bond_between(previous, atom).is_none()) {
            break;
        }
        length += 1;
        previous = Some(atom);
    }
    length
}

fn rule_number(rule: &SuffixRule, attribute: &str) -> Result<i32, ParsingError> {
    let value = rule.attribute(attribute).ok_or_else(|| {
        ParsingError(format!(
            "OPSIN Bug: Missing {attribute} attribute in suffix rule"
        ))
    })?;
    parse_integer(value, attribute)
}

fn parse_integer(value: &str, attribute: &str) -> Result<i32, ParsingError> {
    value.parse().map_err(|_| {
        ParsingError(format!(
            "OPSIN Bug: Invalid integer {value} in {attribute} attribute"
        ))
    })
}

fn split_java(value: &str) -> Vec<&str> {
    let mut values: Vec<_> = value.split(',').collect();
    if !value.is_empty() {
        while values.last() == Some(&"") {
            values.pop();
        }
    }
    values
}

fn starts_with_java_digit(value: &str) -> bool {
    // Java calls Character.isDigit(charAt(0)): supplementary code points are
    // encountered as a high surrogate, which is not itself a decimal digit.
    value.chars().next().is_some_and(|ch| ch as u32 <= 0xffff) && JAVA_DECIMAL_DIGIT.is_match(value)
}
