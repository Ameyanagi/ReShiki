//! OPSIN word-rule assembly and finalization.
//! Port of `StructureBuilder.java`, OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
use crate::{
    build_results::BuildResults,
    build_state::BuildState,
    fragment_tools as ft,
    graph::{AtomId, Element, FragmentId},
    parse_tree::{Arena, NodeId},
    stereo_analyser,
    stereochemistry_handler::StereochemistryHandler,
    structure_building_methods::{Assembly, Result, bond_order, error},
    valence,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn build_fragment(
    state: &mut BuildState,
    arena: &mut Arena,
    molecule: NodeId,
) -> Result<FragmentId> {
    Builder {
        assembly: Assembly { state, arena },
        polymer_attachment_points: Vec::new(),
        top_level_word_rule_count: 0,
    }
    .build(molecule)
}
struct Builder<'a> {
    assembly: Assembly<'a>,
    polymer_attachment_points: Vec<AtomId>,
    top_level_word_rule_count: usize,
}
impl Builder<'_> {
    fn build(&mut self, molecule: NodeId) -> Result<FragmentId> {
        let mut rules = self.assembly.arena.children_named(molecule, "wordRule");
        self.top_level_word_rule_count = rules.len();
        if rules.is_empty() {
            return Err(error("Molecule contains no word rules!?"));
        }
        for &rule in &rules {
            self.process_word_rule_children_then_rule(rule)?;
        }
        if self.top_level_word_rule_count != rules.len() {
            rules = self.assembly.arena.children_named(molecule, "wordRule");
        }
        let groups = self.assembly.arena.descendants_named(molecule, "group");
        self.process_special_cases(&groups)?;
        self.process_oxidation_numbers(&groups)?;
        self.assembly
            .state
            .fragment_manager
            .convert_spare_valencies_to_double_bonds()?;
        self.assembly.state.fragment_manager.check_valencies()?;
        self.manipulate_stoichiometry(molecule, &rules)?;
        self.assembly
            .state
            .fragment_manager
            .make_hydrogens_explicit()?;
        let fragment = self.assembly.state.fragment_manager.unified_fragment();
        self.process_stereochemistry(molecule, fragment)?;
        if !self
            .assembly
            .graph()
            .fragment(fragment)
            .out_atoms
            .is_empty()
        {
            if !self.assembly.state.options.allow_radicals {
                return Err(error(
                    "Radicals are currently set to not convert to structures",
                ));
            }
            if self
                .assembly
                .state
                .options
                .output_radicals_as_wildcard_atoms
            {
                self.assembly
                    .graph_mut()
                    .convert_out_atoms_to_attachment_atoms(fragment)?;
            }
        }
        if !self.polymer_attachment_points.is_empty() {
            for atom in self.polymer_attachment_points.clone() {
                self.assembly.graph_mut().atom_mut(atom).element = Element::R;
            }
            self.assembly
                .graph_mut()
                .fragment_mut(fragment)
                .polymer_attachment_points = Some(self.polymer_attachment_points.clone());
        }
        Ok(fragment)
    }
    fn process_word_rule_children_then_rule(&mut self, rule: NodeId) -> Result<()> {
        for child in self.assembly.arena.children_named(rule, "wordRule") {
            self.process_word_rule_children_then_rule(child)?;
        }
        self.process_word_rule(rule)
    }
    fn process_word_rule(&mut self, rule: NodeId) -> Result<()> {
        let kind = self
            .assembly
            .attr(rule, "wordRule")
            .ok_or_else(|| error("Word rule has no type"))?;
        let words = self.assembly.children_any(rule, &["word", "wordRule"]);
        self.assembly.state.current_word_rule = Some(kind.clone());
        match kind.as_str() {
            "simple" => {
                for word in words {
                    if self.assembly.arena[word].name != "word"
                        || self.assembly.attr(word, "type").as_deref() != Some("full")
                    {
                        return Err(error("OPSIN bug: Unexpected contents of 'simple' wordRule"));
                    }
                    self.assembly.resolve_word_or_bracket(word)?;
                }
            }
            "substituent" => {
                for word in words {
                    if self.assembly.arena[word].name != "word"
                        || self.assembly.attr(word, "type").as_deref() != Some("substituent")
                        || !self.assembly.state.options.allow_radicals
                    {
                        return Err(error(
                            "OPSIN bug: Unexpected contents of 'substituent' wordRule",
                        ));
                    }
                    self.assembly.resolve_word_or_bracket(word)?;
                }
            }
            "ester" | "multiEster" => self.build_ester(&words)?,
            "divalentFunctionalGroup" => self.build_divalent_functional_group(&words)?,
            "monovalentFunctionalGroup" => self.build_monovalent_functional_group(&words)?,
            "functionalClassEster" => self.build_functional_class_ester(&words)?,
            "acidReplacingFunctionalGroup" => {
                for word in words {
                    self.assembly.resolve_word_or_bracket(word)?;
                }
            }
            "oxide" => self.build_oxide(&words)?,
            "carbonylDerivative" => self.build_carbonyl_derivative(&words)?,
            "anhydride" => self.build_anhydride(&words)?,
            "acidHalideOrPseudoHalide" => self.build_acid_halide_or_pseudo_halide(&words)?,
            "additionCompound" => self.build_addition_compound(&words)?,
            "glycol" => self.build_glycol(&words)?,
            "glycolEther" => self.build_glycol_ether(&words)?,
            "acetal" => self.build_acetal(&words)?,
            "potentialAlcoholEster" => {
                if !self.build_alcohol_ester(&words)? {
                    self.split_alcohol_ester_rule(&words)?;
                    self.top_level_word_rule_count += 1;
                }
            }
            "cyclicPeptide" => self.build_cyclic_peptide(&words)?,
            "amineDiConjunctiveSuffix" => self.build_amine_di_conjunctive_suffix(&words)?,
            "polymer" => self.build_polymer(&words)?,
            _ => return Err(error(format!("Unexpected Word Rule: {kind}"))),
        }
        Ok(())
    }
    fn results(&self, node: NodeId) -> Result<BuildResults> {
        BuildResults::from_node(self.assembly.arena, self.assembly.graph(), node)
    }
    fn first_atom(&self, fragment: FragmentId) -> Result<AtomId> {
        self.assembly
            .graph()
            .fragment(fragment)
            .atoms
            .first()
            .copied()
            .ok_or_else(|| error("Fragment has no atoms"))
    }
    fn default_in_atom(&self, fragment: FragmentId) -> Result<AtomId> {
        self.assembly
            .graph()
            .fragment(fragment)
            .default_in_atom
            .map(Ok)
            .unwrap_or_else(|| self.first_atom(fragment))
    }
    fn out_atom(&mut self, results: &BuildResults, index: usize) -> Result<AtomId> {
        let out = results.out_atom(self.assembly.graph(), index)?.clone();
        if out.explicitly_set {
            Ok(out.atom)
        } else {
            self.assembly.find_atom_for_unlocanted_radical(
                self.assembly.graph().atom(out.atom).fragment,
                &out,
            )
        }
    }
    fn add_charge_and_protons(&mut self, atom: AtomId, charge: i32, protons: i32) {
        let a = self.assembly.graph_mut().atom_mut(atom);
        a.charge += charge;
        a.protons_explicitly_added_or_removed += protons;
    }
    fn neutralise_charge(&mut self, atom: AtomId) {
        let a = self.assembly.graph_mut().atom_mut(atom);
        a.charge = 0;
        a.protons_explicitly_added_or_removed = 0;
    }
    fn functional_group(&self, word: NodeId) -> Result<NodeId> {
        let groups = self
            .assembly
            .arena
            .descendants_named(word, "functionalGroup");
        if groups.len() != 1 {
            return Err(error(format!(
                "Expected exactly 1 functionalGroup. Found {}",
                groups.len()
            )));
        }
        Ok(groups[0])
    }
    fn make_functional_group(&mut self, node: NodeId) -> Result<FragmentId> {
        let smiles = self
            .assembly
            .attr(node, "value")
            .ok_or_else(|| error("Functional group has no SMILES"))?;
        let labels = self
            .assembly
            .attr(node, "labels")
            .unwrap_or_else(|| "none".into());
        self.assembly
            .state
            .fragment_manager
            .build_smiles(&smiles, "functionalClass", &labels)
    }
    fn build_ester(&mut self, words: &[NodeId]) -> Result<()> {
        let mut in_substituents = true;
        let mut substituents = BuildResults::new();
        let mut ate = Vec::new();
        for &word in words {
            self.assembly.resolve_word_or_bracket(word)?;
            let mut results = self.results(word)?;
            if in_substituents && !results.functional_atoms.is_empty() {
                in_substituents = false;
            }
            if in_substituents {
                if self.assembly.attr(word, "type").as_deref() != Some("substituent") {
                    return Err(error(
                        "OPSIN bug: Non substituent word found where substituent expected in ester",
                    ));
                }
                let mut traditional = false;
                for id in results.out_atoms.clone() {
                    let out = self
                        .assembly
                        .graph()
                        .fragments
                        .iter()
                        .flat_map(|f| &f.out_atoms)
                        .find(|out| out.id == id)
                        .cloned()
                        .ok_or_else(|| error("Missing ester out atom"))?;
                    if out.valency > 1 {
                        let f = self.assembly.graph().atom(out.atom).fragment;
                        let index = self
                            .assembly
                            .graph()
                            .fragment(f)
                            .out_atoms
                            .iter()
                            .position(|candidate| candidate.id == id)
                            .unwrap();
                        ft::split_out_atom_into_valency_one_out_atoms(
                            self.assembly.graph_mut(),
                            f,
                            index,
                        );
                        traditional = true;
                    }
                }
                if traditional {
                    results = self.results(word)?;
                }
                if results.out_atoms.is_empty() {
                    return Err(error(
                        "Substituent was expected to have at least one outAtom",
                    ));
                }
                if results.out_atoms.len() == 1
                    && let Some(locant) = self.assembly.attr(word, "locant")
                {
                    let out = results.out_atom(self.assembly.graph(), 0)?.clone();
                    let f = self.assembly.graph().atom(out.atom).fragment;
                    let index = self
                        .assembly
                        .graph()
                        .fragment(f)
                        .out_atoms
                        .iter()
                        .position(|candidate| candidate.id == out.id)
                        .unwrap();
                    self.assembly
                        .graph_mut()
                        .set_out_atom_locant(f, index, Some(locant));
                }
                substituents.merge(results);
            } else {
                if results.functional_atoms.is_empty() {
                    return Err(error("bug? ate group did not have any functional atoms!"));
                }
                ate.push((results, self.assembly.attr(word, "locant")));
            }
        }
        if ate.is_empty() {
            return Err(error("OPSIN bug: Missing ate group in ester"));
        }
        let count = substituents.out_atoms.len();
        if count == 0 {
            return Err(error("OPSIN bug: Missing outatom on ester substituents"));
        }
        let functional_count = ate
            .iter()
            .map(|(br, _)| br.functional_atoms.len())
            .sum::<usize>();
        if count > functional_count || functional_count > count && !count.is_multiple_of(ate.len())
        {
            return Err(error(
                "Number of ester radicals disagrees with available functional atoms",
            ));
        }
        let ate_count = ate.len();
        for i in 0..count {
            let (acid, acid_locant) = &mut ate[i % ate_count];
            let locant = substituents
                .out_atom(self.assembly.graph(), 0)?
                .locant
                .clone();
            let oxygen = if let Some(locant) = locant {
                self.determine_functional_atom(&locant, acid)?
            } else {
                acid.remove_functional_atom(self.assembly.graph_mut(), 0)?
            };
            let index = if let Some(locant) = acid_locant {
                let mut found = None;
                for j in 0..substituents.out_atoms.len() {
                    if self
                        .assembly
                        .graph()
                        .atom(substituents.out_atom(self.assembly.graph(), j)?.atom)
                        .locants
                        .contains(locant)
                    {
                        found = Some(j);
                        break;
                    }
                }
                found.ok_or_else(|| {
                    error(format!(
                        "Unable to find substituent with locant: {locant} to form ester!"
                    ))
                })?
            } else {
                0
            };
            let atom = if acid_locant.is_some() {
                substituents.out_atom(self.assembly.graph(), index)?.atom
            } else {
                self.out_atom(&substituents, index)?
            };
            self.assembly
                .state
                .fragment_manager
                .create_bond(oxygen, atom, 1)?;
            substituents.remove_out_atom(self.assembly.graph_mut(), index)?;
            self.neutralise_charge(oxygen);
        }
        Ok(())
    }
    fn build_divalent_functional_group(&mut self, words: &[NodeId]) -> Result<()> {
        let mut words = words.to_vec();
        let first = *words
            .first()
            .ok_or_else(|| error("Divalent functional rule has no words"))?;
        if self.assembly.attr(first, "type").as_deref() != Some("substituent") {
            return Err(error("word: 0 was expected to be a substituent"));
        }
        self.assembly.resolve_word_or_bracket(first)?;
        let mut one = self.results(first)?;
        if one.out_atom(self.assembly.graph(), 0)?.valency != 1 {
            return Err(error("OutAtom has unexpected valency. Expected 1"));
        }
        let same = one.out_atoms.len() == 2;
        let mut index = 0;
        let mut two = BuildResults::new();
        if same {
            if one.out_atom(self.assembly.graph(), 1)?.valency != 1 {
                return Err(error("OutAtom has unexpected valency. Expected 1"));
            }
        } else {
            if one.out_atoms.len() != 1 {
                return Err(error("Expected one outAtom"));
            }
            index = 1;
            let next = *words
                .get(index)
                .ok_or_else(|| error("Missing second divalent word"))?;
            if self.assembly.attr(next, "type").as_deref() == Some("functionalTerm") {
                let clone = self.assembly.clone_element(first, 0)?;
                self.assembly.arena.insert_after(first, clone);
                words = self.assembly.arena[self.assembly.arena[first].parent.unwrap()]
                    .children
                    .clone();
            } else {
                self.assembly.resolve_word_or_bracket(next)?;
            }
            two = self.results(words[index])?;
            if two.out_atoms.len() != 1 || two.out_atom(self.assembly.graph(), 0)?.valency != 1 {
                return Err(error("Expected one outAtom of valency 1"));
            }
        }
        index += 1;
        let word = *words
            .get(index)
            .ok_or_else(|| error("Missing divalent functional term"))?;
        if self.assembly.attr(word, "type").as_deref() != Some("functionalTerm") {
            return Err(error("Expected a functionalTerm"));
        }
        let node = self.functional_group(word)?;
        let linker = self.make_functional_group(node)?;
        let a = self.out_atom(&one, 0)?;
        one.remove_out_atom(self.assembly.graph_mut(), 0)?;
        let b = if same {
            let b = self.out_atom(&one, 0)?;
            one.remove_out_atom(self.assembly.graph_mut(), 0)?;
            b
        } else {
            let b = self.out_atom(&two, 0)?;
            two.remove_out_atom(self.assembly.graph_mut(), 0)?;
            b
        };
        let centre = self.first_atom(linker)?;
        if self.assembly.graph().fragment(linker).out_atoms.len() == 1 {
            let outgoing = self.assembly.graph_mut().remove_out_atom(linker, 0).atom;
            self.assembly
                .state
                .fragment_manager
                .create_bond(a, outgoing, 1)?;
            self.assembly
                .state
                .fragment_manager
                .create_bond(b, centre, 1)?;
        } else if a != b {
            self.assembly
                .state
                .fragment_manager
                .create_bond(a, centre, 1)?;
            self.assembly
                .state
                .fragment_manager
                .create_bond(b, centre, 1)?;
        } else {
            self.assembly
                .state
                .fragment_manager
                .create_bond(a, centre, 2)?;
        }
        let parent = self.assembly.graph().atom(a).fragment;
        self.assembly
            .state
            .fragment_manager
            .incorporate_fragment(linker, parent)?;
        Ok(())
    }
    fn build_monovalent_functional_group(&mut self, words: &[NodeId]) -> Result<()> {
        let first = *words
            .first()
            .ok_or_else(|| error("Missing monovalent substituent"))?;
        self.assembly.resolve_word_or_bracket(first)?;
        for group in self.assembly.arena.descendants_named(first, "group") {
            let f = self.assembly.fragment(group)?;
            for index in (0..self.assembly.graph().fragment(f).out_atoms.len()).rev() {
                if self.assembly.graph().fragment(f).out_atoms[index].valency > 1 {
                    ft::split_out_atom_into_valency_one_out_atoms(
                        self.assembly.graph_mut(),
                        f,
                        index,
                    );
                }
            }
        }
        let mut substituent = self.results(first)?;
        let mut functional = Vec::new();
        for &word in &words[1..] {
            let group = self.functional_group(word)?;
            let f = self.make_functional_group(group)?;
            if self.assembly.attr(group, "type").as_deref() == Some("monoValentStandaloneGroup") {
                let atom = self.default_in_atom(f)?;
                self.add_charge_and_protons(atom, 1, 1);
            }
            functional.push(f);
            if let Some(multiplier) = self.assembly.arena.previous_sibling(group) {
                let n = self.assembly.number(multiplier, "value")?;
                for _ in 1..n {
                    functional.push(self.assembly.state.fragment_manager.copy_fragment(f)?);
                }
                self.assembly.arena.detach(multiplier);
            }
        }
        let count = substituent.out_atoms.len();
        if count > functional.len() {
            if functional.len() != 1 {
                return Err(error(
                    "Incorrect number of functional groups found to balance outAtoms",
                ));
            }
            for _ in 1..count {
                functional.push(
                    self.assembly
                        .state
                        .fragment_manager
                        .copy_fragment(functional[0])?,
                );
            }
        } else if functional.len() > count {
            return Err(error(
                "There are more function groups to attach than there are positions to attach them to!",
            ));
        }
        for f in functional {
            let target = self.default_in_atom(f)?;
            let from = self.out_atom(&substituent, 0)?;
            self.assembly
                .state
                .fragment_manager
                .create_bond(target, from, 1)?;
            substituent.remove_out_atom(self.assembly.graph_mut(), 0)?;
            let parent = self.assembly.graph().atom(from).fragment;
            self.assembly
                .state
                .fragment_manager
                .incorporate_fragment(f, parent)?;
        }
        Ok(())
    }
    fn build_functional_class_ester(&mut self, words: &[NodeId]) -> Result<()> {
        let first = *words.first().ok_or_else(|| error("Missing ester acid"))?;
        if self.assembly.attr(first, "type").as_deref() != Some("full") {
            return Err(error(
                "Don't alter wordRules.xml without checking the consequences!",
            ));
        }
        self.assembly.resolve_word_or_bracket(first)?;
        let mut acid = self.results(first)?;
        if acid.functional_atoms.is_empty() {
            return Err(error("No functionalAtoms detected!"));
        }
        if words.len() < 3
            || self
                .assembly
                .attr(*words.last().unwrap(), "type")
                .as_deref()
                != Some("functionalTerm")
        {
            return Err(error(
                "OPSIN Bug: Bug in functionalClassEster rule; 'ester' not found where it was expected",
            ));
        }
        for &word in &words[1..words.len() - 1] {
            if self.assembly.attr(word, "type").as_deref() != Some("substituent") {
                if self.assembly.attr(word, "type").as_deref() == Some("functionalTerm")
                    && self
                        .assembly
                        .attr(word, "value")
                        .is_some_and(|s| s.eq_ignore_ascii_case("ester"))
                {
                    continue;
                }
                return Err(error("OPSIN Bug: Unexpected word in functionalClassEster"));
            }
            self.assembly.resolve_word_or_bracket(word)?;
            let mut substituent = self.results(word)?;
            if acid.functional_atoms.len() < substituent.out_atoms.len() {
                return Err(error("Insufficient functionalAtoms on acid"));
            }
            for index in 0..substituent.out_atoms.len() {
                let oxygen = if let Some(locant) = self.assembly.attr(word, "locant") {
                    self.determine_functional_atom(&locant, &mut acid)?
                } else {
                    acid.remove_functional_atom(self.assembly.graph_mut(), 0)?
                };
                if substituent.out_atom(self.assembly.graph(), index)?.valency != 1 {
                    return Err(error(
                        "Substituent was expected to only have an outgoing valency of 1",
                    ));
                }
                let from = self.out_atom(&substituent, index)?;
                self.assembly
                    .state
                    .fragment_manager
                    .create_bond(oxygen, from, 1)?;
                if self.assembly.graph().atom(oxygen).charge == -1 {
                    self.neutralise_charge(oxygen);
                }
            }
            substituent.remove_all_out_atoms(self.assembly.graph_mut())?;
        }
        Ok(())
    }
    fn rightmost_group_in_word(&self, word: NodeId) -> Result<NodeId> {
        if self.assembly.arena[word].name == "wordRule" {
            let words = self.assembly.arena.descendants_named(word, "word");
            let chosen = words
                .into_iter()
                .rev()
                .find(|&w| self.assembly.attr(w, "type").as_deref() != Some("functionalTerm"))
                .ok_or_else(|| error("OPSIN bug: word element not found where expected"))?;
            self.rightmost_group_in_word(chosen)
        } else if self.assembly.arena[word].name == "word" {
            let child = self
                .assembly
                .children_any(word, &["substituent", "bracket", "root"])
                .last()
                .copied()
                .ok_or_else(|| error("Word contains no group"))?;
            self.assembly.rightmost_group(child)
        } else {
            Err(error("OPSIN bug: expected word or wordRule"))
        }
    }
    fn optional_multiplier(&mut self, word: NodeId) -> Result<Option<usize>> {
        let multipliers = self.assembly.arena.descendants_named(word, "multiplier");
        if multipliers.len() > 1 {
            return Err(error(format!(
                "Expected 0 or 1 multiplier found: {}",
                multipliers.len()
            )));
        }
        if let Some(&node) = multipliers.first() {
            let number = self.assembly.number(node, "value")?;
            self.assembly.arena.detach(node);
            Ok(Some(number))
        } else {
            Ok(None)
        }
    }
    fn optional_locants(&mut self, word: NodeId) -> Result<Vec<String>> {
        let locants = self.assembly.arena.descendants_named(word, "locant");
        if locants.len() > 1 {
            return Err(error(format!(
                "Expected 0 or 1 locant elements found: {}",
                locants.len()
            )));
        }
        if let Some(&node) = locants.first() {
            let value = self.assembly.arena.value(node);
            self.assembly.arena.detach(node);
            Ok(value
                .trim_end_matches('-')
                .split(',')
                .map(str::to_owned)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }
    fn build_oxide(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() != 2 {
            return Err(error("Oxide rule expects two words"));
        }
        self.assembly.resolve_word_or_bracket(words[0])?;
        if self.assembly.attr(words[1], "type").as_deref() != Some("functionalTerm") {
            return Err(error("Oxide functional term not found where expected!"));
        }
        let group = if self.assembly.arena[words[0]].name == "wordRule" {
            let full = self
                .assembly
                .arena
                .descendants_named(words[0], "word")
                .into_iter()
                .rfind(|&w| self.assembly.attr(w, "type").as_deref() == Some("full"))
                .ok_or_else(|| {
                    error(
                        "OPSIN is entirely unsure where the oxide goes so has decided not to guess",
                    )
                })?;
            self.rightmost_group_in_word(full)?
        } else {
            self.rightmost_group_in_word(words[0])?
        };
        let parent = self.assembly.fragment(group)?;
        let mut count = self.optional_multiplier(words[1])?;
        if count.is_none() && self.assembly.attr(group, "type").as_deref() == Some("elementaryAtom")
        {
            let atom = self.first_atom(parent)?;
            let a = self.assembly.graph().atom(atom);
            if a.charge > 0 && a.charge % 2 == 0 {
                count = Some((a.charge / 2) as usize);
            } else if let Some(oxidation) = a.properties.oxidation_number {
                let valency = oxidation - self.assembly.graph().incoming_valency(atom);
                if valency > 0 && valency % 2 == 0 {
                    count = Some((valency / 2) as usize);
                }
            }
        }
        let node = self.functional_group(words[1])?;
        let mut oxides = Vec::new();
        for _ in 0..count.unwrap_or(1) {
            oxides.push(self.make_functional_group(node)?);
        }
        let locants = self.optional_locants(words[1])?;
        if !locants.is_empty() && locants.len() != oxides.len() {
            return Err(error(
                "Mismatch between number of locants and number of oxides specified",
            ));
        }
        for (index, &oxide) in oxides.iter().enumerate() {
            let oxygen = self.first_atom(oxide)?;
            if let Some(locant) = locants.get(index) {
                let target = self.assembly.atom_by_locant(parent, locant)?;
                if self.assembly.graph().atom(target).element == Element::C
                    && self.assembly.graph().fragment(parent).fragment_type != "elementaryAtom"
                {
                    return Err(error(format!(
                        "Locant {locant} indicated oxide applied to carbon, but this would lead to hypervalency!"
                    )));
                }
                self.form_oxide_bond(target, oxygen)?;
                continue;
            }
            if self.assembly.graph().fragment(parent).fragment_type == "elementaryAtom" {
                let target = self.first_atom(parent)?;
                self.form_oxide_bond(target, oxygen)?;
                if self.assembly.graph().atom(target).charge >= 2 {
                    self.assembly.graph_mut().atom_mut(target).charge -= 2;
                }
                continue;
            }
            let atoms = self.assembly.graph().fragment(parent).atoms.clone();
            let mut target = None;
            for suffix_only in [true, false] {
                target = atoms.iter().copied().find(|&a| {
                    let atom = self.assembly.graph().atom(a);
                    (!suffix_only || atom.atom_type == "suffix")
                        && !matches!(atom.element, Element::C | Element::O)
                });
                if target.is_some() {
                    break;
                }
            }
            if let Some(target) = target {
                self.form_oxide_bond(target, oxygen)?;
                continue;
            }
            let bonds = self.assembly.graph().fragment(parent).bonds.clone();
            let mut bridged = false;
            for spare in [false, true] {
                for &bond in &bonds {
                    let b = self.assembly.graph().bond(bond).clone();
                    if self.assembly.graph().atom(b.from).element != Element::C
                        || self.assembly.graph().atom(b.to).element != Element::C
                    {
                        continue;
                    }
                    if if spare {
                        self.assembly.graph().atom(b.from).spare_valency
                            && self.assembly.graph().atom(b.to).spare_valency
                    } else {
                        b.order == 2
                    } {
                        if spare {
                            self.assembly.graph_mut().atom_mut(b.from).spare_valency = false;
                            self.assembly.graph_mut().atom_mut(b.to).spare_valency = false;
                        } else {
                            self.assembly.graph_mut().bond_mut(bond).order = 1;
                        }
                        self.assembly
                            .state
                            .fragment_manager
                            .create_bond(b.from, oxygen, 1)?;
                        self.assembly
                            .state
                            .fragment_manager
                            .create_bond(b.to, oxygen, 1)?;
                        bridged = true;
                        break;
                    }
                }
                if bridged {
                    break;
                }
            }
            if bridged {
                continue;
            }
            for suffix_only in [true, false] {
                target = atoms.iter().copied().find(|&a| {
                    let atom = self.assembly.graph().atom(a);
                    (!suffix_only || atom.atom_type == "suffix") && atom.element != Element::C
                });
                if target.is_some() {
                    break;
                }
            }
            self.form_oxide_bond(
                target.ok_or_else(|| {
                    error("Unable to find suitable atom or a double bond to add oxide to")
                })?,
                oxygen,
            )?;
        }
        for oxide in oxides {
            self.assembly
                .state
                .fragment_manager
                .incorporate_fragment(oxide, parent)?;
        }
        Ok(())
    }
    fn form_oxide_bond(&mut self, target: AtomId, oxide: AtomId) -> Result<()> {
        let a = self.assembly.graph().atom(target);
        let max = valence::maximum_valency(a.element, a.charge);
        if max.is_none_or(|max| {
            self.assembly.graph().incoming_valency(target) + a.out_valency + 2 <= max
        }) {
            if a.lambda_convention_valency.is_none()
                || !valence::check_valency_available_for_bond(self.assembly.graph(), target, 2)
            {
                self.add_charge_and_protons(target, 0, 2);
            }
            self.assembly
                .state
                .fragment_manager
                .create_bond(target, oxide, 2)?;
        } else {
            if a.charge != 0 || self.assembly.graph().atom(oxide).charge != 0 {
                return Err(error(
                    "Oxide referred to atom with insufficient valency to accept oxygen",
                ));
            }
            self.add_charge_and_protons(target, 1, 1);
            self.add_charge_and_protons(oxide, -1, -1);
            let a = self.assembly.graph().atom(target);
            if valence::maximum_valency(a.element, a.charge).is_some_and(|max| {
                self.assembly.graph().incoming_valency(target) + a.out_valency + 1 > max
            }) {
                return Err(error(
                    "Oxide referred to atom with insufficient valency to accept oxygen",
                ));
            }
            self.assembly
                .state
                .fragment_manager
                .create_bond(target, oxide, 1)?;
        }
        Ok(())
    }
    fn search_numeric_locant(&self, start: AtomId) -> Option<AtomId> {
        let mut stack = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(atom) = stack.pop() {
            visited.insert(atom);
            for neighbour in self.assembly.graph().neighbours(atom) {
                if visited.contains(&neighbour) {
                    continue;
                }
                if self
                    .assembly
                    .graph()
                    .atom(neighbour)
                    .locants
                    .iter()
                    .any(|s| ft::is_numeric_locant(s))
                {
                    return Some(neighbour);
                }
                stack.push(neighbour);
            }
        }
        None
    }
    fn find_carbonyl_oxygens(&self, fragment: FragmentId, locants: &[String]) -> Vec<AtomId> {
        self.assembly
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .copied()
            .filter(|&atom| {
                let a = self.assembly.graph().atom(atom);
                if a.element != Element::O || a.charge != 0 || a.bonds.len() != 1 {
                    return false;
                }
                let bond = self.assembly.graph().bond(a.bonds[0]);
                let neighbour = bond.other_atom(atom).unwrap();
                if self.assembly.graph().atom(neighbour).element != Element::C || bond.order != 2 {
                    return false;
                }
                locants.is_empty()
                    || self.search_numeric_locant(atom).is_some_and(|a| {
                        locants
                            .iter()
                            .any(|l| self.assembly.graph().atom(a).locants.contains(l))
                    })
            })
            .collect()
    }
    fn strip_non_element_locants(&mut self, atom: AtomId) {
        for locant in self.assembly.graph().atom(atom).locants.clone() {
            if !ft::is_element_symbol_locant(&locant) {
                self.assembly.graph_mut().remove_locant(atom, &locant);
            }
        }
    }
    fn build_carbonyl_derivative(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() < 2 || self.assembly.attr(words[0], "type").as_deref() != Some("full") {
            return Err(error(
                "OPSIN bug: Wrong word type encountered when applying carbonylDerivative wordRule",
            ));
        }
        let mut replacements = Vec::new();
        let mut locants = Vec::new();
        if self.assembly.attr(words[1], "type").as_deref() != Some("functionalTerm") {
            for &word in &words[1..] {
                let group = self.rightmost_group_in_word(word)?;
                let f = self.assembly.fragment(group)?;
                replacements.push(f);
                let children = self.assembly.arena[word].children.clone();
                if children.len() == 1 && self.assembly.arena[children[0]].name == "bracket" {
                    if let Some(locant) = self.assembly.attr(children[0], "locant") {
                        locants.push(locant);
                    }
                } else if children.len() == 2
                    && let Some(locant) = self.assembly.attr(children[0], "locant")
                    && self.assembly.arena[children[1]].name == "root"
                    && self.assembly.graph().atom_by_locant(f, &locant).is_none()
                    && ft::is_numeric_locant(&locant)
                {
                    locants.push(locant);
                    self.assembly.arena[children[0]].remove_attribute("locant");
                }
            }
        } else {
            let count = self.optional_multiplier(words[1])?.unwrap_or(1);
            let group = self.functional_group(words[1])?;
            for index in 0..count {
                let f = self.make_functional_group(group)?;
                if index > 0 {
                    let atoms = self.assembly.graph().fragment(f).atoms.clone();
                    ft::relabel_locants(self.assembly.graph_mut(), &atoms, &"'".repeat(index));
                }
                for atom in self.assembly.graph().fragment(f).atoms.clone() {
                    self.strip_non_element_locants(atom);
                }
                replacements.push(f);
            }
            locants = self.optional_locants(words[1])?;
        }
        if !locants.is_empty() && locants.len() != replacements.len() {
            return Err(error(
                "Mismatch between number of locants and number of carbonyl replacements",
            ));
        }
        let group = self.rightmost_group_in_word(words[0])?;
        let mut parent = self.assembly.arena[group].parent;
        let mut multiplied = false;
        while let Some(node) = parent {
            if node == words[0] {
                break;
            }
            if self.assembly.attr(node, "multiplier").is_some() {
                multiplied = true;
            }
            parent = self.assembly.arena[node].parent;
        }
        if !multiplied {
            let mut oxygens = self.find_carbonyl_oxygens(self.assembly.fragment(group)?, &locants);
            let n = replacements.len().min(oxygens.len());
            self.replace_carbonyl_oxygens(words, &mut replacements, &mut oxygens, n)?;
        }
        self.assembly.resolve_word_or_bracket(words[0])?;
        if !replacements.is_empty() {
            let mut oxygens = Vec::new();
            for &f in self.results(words[0])?.fragments.iter().rev() {
                oxygens.extend(self.find_carbonyl_oxygens(f, &locants));
            }
            let n = replacements.len();
            self.replace_carbonyl_oxygens(words, &mut replacements, &mut oxygens, n)?;
        }
        Ok(())
    }
    fn replace_carbonyl_oxygens(
        &mut self,
        words: &[NodeId],
        replacements: &mut Vec<FragmentId>,
        oxygens: &mut Vec<AtomId>,
        count: usize,
    ) -> Result<()> {
        if count > oxygens.len() {
            return Err(error("Insufficient carbonyl groups found!"));
        }
        for index in 0..count {
            let oxygen = oxygens.remove(0);
            let parent = self.assembly.graph().atom(oxygen).fragment;
            let replacement = replacements.remove(0);
            let atoms = self.assembly.graph().fragment(replacement).atoms.clone();
            if atoms.len() == 2
                && let Some(backbone) = self.search_numeric_locant(oxygen)
                && let Some(locant) = self.assembly.graph().atom(backbone).locants.first()
            {
                let locant = format!("{}{locant}", self.assembly.graph().atom(atoms[1]).element);
                self.assembly.graph_mut().add_locant(atoms[1], locant);
            }
            let functional_class =
                self.assembly.attr(words[1], "type").as_deref() == Some("functionalTerm");
            if !functional_class {
                self.assembly.resolve_word_or_bracket(
                    *words
                        .get(index + 1)
                        .ok_or_else(|| error("Missing carbonyl replacement word"))?,
                )?;
            }
            for atom in atoms {
                self.strip_non_element_locants(atom);
                for locant in self.assembly.graph().atom(atom).locants.clone() {
                    if self
                        .assembly
                        .graph()
                        .atom_by_locant(parent, &locant)
                        .is_some()
                    {
                        self.assembly.graph_mut().remove_locant(atom, &locant);
                    }
                }
            }
            if self.assembly.graph().fragment(replacement).out_atoms.len() == 2 {
                let carbon = self.assembly.graph().neighbours(oxygen)[0];
                let out = self.assembly.graph_mut().remove_out_atom(replacement, 1);
                if self.assembly.graph().incoming_valency(carbon) >= 4 {
                    return Err(error("Insufficient substitutable hydrogen for haloxime"));
                }
                self.assembly.state.fragment_manager.create_bond(
                    carbon,
                    out.atom,
                    bond_order(out.valency)?,
                )?;
            }
            if self.assembly.graph().fragment(replacement).out_atoms.len() != 1 {
                return Err(error(
                    "OPSIN Bug: Carbonyl replacement fragment expected to have one outatom",
                ));
            }
            let out = self.assembly.graph_mut().remove_out_atom(replacement, 0);
            let atom_type = self.assembly.graph().atom(oxygen).atom_type.clone();
            self.assembly
                .state
                .fragment_manager
                .replace_atom_preserving_connectivity(oxygen, out.atom)?;
            self.assembly.graph_mut().atom_mut(out.atom).atom_type = atom_type;
            if functional_class {
                self.assembly
                    .state
                    .fragment_manager
                    .incorporate_fragment(replacement, parent)?;
            }
        }
        Ok(())
    }
    fn form_anhydride_link(&mut self, smiles: &str, one: AtomId, two: AtomId) -> Result<()> {
        if [one, two].iter().any(|&a| {
            self.assembly.graph().atom(a).element != Element::O
                || self.assembly.graph().atom(a).bonds.len() != 1
        }) {
            return Err(error("Problem building anhydride"));
        }
        let other = self.assembly.graph().neighbours(two)[0];
        let parent = self.assembly.graph().atom(one).fragment;
        self.assembly
            .state
            .fragment_manager
            .remove_atom_and_associated_bonds(two);
        let link =
            self.assembly
                .state
                .fragment_manager
                .build_smiles(smiles, "functionalClass", "none")?;
        let first = self.first_atom(link)?;
        self.assembly
            .state
            .fragment_manager
            .replace_atom_preserving_connectivity(one, first)?;
        let last = *self.assembly.graph().fragment(link).atoms.last().unwrap();
        self.assembly
            .state
            .fragment_manager
            .create_bond(last, other, 1)?;
        self.assembly
            .state
            .fragment_manager
            .incorporate_fragment(link, parent)?;
        Ok(())
    }
    fn link_anhydride_results(
        &mut self,
        smiles: &str,
        one: &mut BuildResults,
        two: &mut BuildResults,
    ) -> Result<()> {
        let a = one.remove_functional_atom(self.assembly.graph_mut(), 0)?;
        let b = two.remove_functional_atom(self.assembly.graph_mut(), 0)?;
        self.form_anhydride_link(smiles, a, b)
    }
    fn build_anhydride(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() != 2 && words.len() != 3 {
            return Err(error("Unexpected number of words in anhydride"));
        }
        let word = *words.last().unwrap();
        let node = self.functional_group(word)?;
        let smiles = self
            .assembly
            .attr(node, "value")
            .ok_or_else(|| error("Anhydride has no SMILES"))?;
        let count = self.optional_multiplier(word)?.unwrap_or(1);
        let locants = self
            .assembly
            .arena
            .descendants_named_any(word, &["locant", "colonOrSemiColonDelimitedLocant"]);
        if locants.len() > 1 {
            return Err(error("Expected 0 or 1 anhydrideLocants"));
        }
        let locant = locants.first().map(|&id| self.assembly.arena.value(id));
        for node in locants {
            self.assembly.arena.detach(node);
        }
        self.assembly.resolve_word_or_bracket(words[0])?;
        let mut one = self.results(words[0])?;
        if one.functional_atoms.is_empty() {
            return Err(error("Cannot find functionalAtom to form anhydride"));
        }
        if words.len() == 3 {
            if locant.is_some() {
                return Err(error("Unsupported or invalid anhydride"));
            }
            self.assembly.resolve_word_or_bracket(words[1])?;
            let mut two = self.results(words[1])?;
            if two.functional_atoms.is_empty() {
                return Err(error("Cannot find functionalAtom to form anhydride"));
            }
            if count > 1 {
                for i in (0..count).rev() {
                    if two.functional_atoms.is_empty() {
                        return Err(error("Cannot find functionalAtom to form anhydride"));
                    }
                    if i == 0 {
                        self.link_anhydride_results(&smiles, &mut one, &mut two)?;
                    } else {
                        let copy = self.assembly.clone_element(words[0], 0)?;
                        self.assembly.arena.insert_after(words[0], copy);
                        let mut acid = self.results(copy)?;
                        self.link_anhydride_results(&smiles, &mut acid, &mut two)?;
                    }
                }
            } else {
                if one.functional_atoms.len() != 1 && two.functional_atoms.len() != 1 {
                    return Err(error("Invalid anhydride description"));
                }
                self.link_anhydride_results(&smiles, &mut one, &mut two)?;
            }
        } else if one.functional_atoms.len() > 1 {
            if one.functional_atoms.len() == 2 {
                if count != 1 || locant.is_some() {
                    return Err(error("Unsupported or invalid anhydride"));
                }
                let a = one.remove_functional_atom(self.assembly.graph_mut(), 0)?;
                let b = one.remove_functional_atom(self.assembly.graph_mut(), 0)?;
                self.form_anhydride_link(&smiles, a, b)?;
            } else {
                let locant=locant.ok_or_else(||error("Anhydride formation appears to be ambiguous; More than 2 acids, no locants"))?;
                let pairs = locant
                    .trim_end_matches('-')
                    .split([':', ';'])
                    .collect::<Vec<_>>();
                if pairs.len() != count || one.functional_atoms.len() < 2 * count {
                    return Err(error(
                        "Mismatch between number of locants/acid atoms and anhydride linkages",
                    ));
                }
                let mut available = one.functional_atoms.clone();
                for pair in pairs {
                    let pair = pair.split(',').collect::<Vec<_>>();
                    if pair.len() != 2 {
                        return Err(error("Each anhydride linkage requires two locants"));
                    }
                    let mut selected = Vec::new();
                    for locant in pair {
                        let index = available
                            .iter()
                            .rposition(|&atom| {
                                self.search_numeric_locant(atom).is_some_and(|a| {
                                    self.assembly
                                        .graph()
                                        .atom(a)
                                        .locants
                                        .iter()
                                        .any(|l| l == locant)
                                })
                            })
                            .ok_or_else(|| {
                                error("Unable to find locanted atom for anhydride formation")
                            })?;
                        selected.push(available.remove(index));
                    }
                    self.form_anhydride_link(&smiles, selected[0], selected[1])?;
                }
            }
        } else {
            if count != 1 || locant.is_some() {
                return Err(error("Unsupported or invalid anhydride"));
            }
            let copy = self.assembly.clone_element(words[0], 0)?;
            self.assembly.arena.insert_after(words[0], copy);
            let mut two = self.results(copy)?;
            self.link_anhydride_results(&smiles, &mut one, &mut two)?;
        }
        Ok(())
    }
    fn monovalent_fragments(&mut self, words: &[NodeId]) -> Result<(Vec<FragmentId>, bool)> {
        let mut fragments = Vec::new();
        let mut mono = false;
        for &word in words {
            let group = self.functional_group(word)?;
            let fragment = self.make_functional_group(group)?;
            if self.assembly.attr(group, "type").as_deref() == Some("monoValentStandaloneGroup") {
                let atom = self.default_in_atom(fragment)?;
                self.add_charge_and_protons(atom, 1, 1);
            }
            fragments.push(fragment);
            if let Some(node) = self.assembly.arena.previous_sibling(group) {
                let n = self.assembly.number(node, "value")?;
                mono |= n == 1;
                for _ in 1..n {
                    fragments.push(
                        self.assembly
                            .state
                            .fragment_manager
                            .copy_fragment(fragment)?,
                    );
                }
                self.assembly.arena.detach(node);
            }
        }
        Ok((fragments, mono))
    }
    fn build_acid_halide_or_pseudo_halide(&mut self, words: &[NodeId]) -> Result<()> {
        let first = *words.first().ok_or_else(|| error("Missing acid"))?;
        if self.assembly.attr(first, "type").as_deref() != Some("full") {
            return Err(error(
                "Don't alter wordRules.xml without checking the consequences!",
            ));
        }
        self.assembly.resolve_word_or_bracket(first)?;
        let mut acid = self.results(first)?;
        let count = acid.functional_atoms.len();
        if count == 0 {
            return Err(error("No functionalAtoms detected!"));
        }
        let (mut halides, mono) = self.monovalent_fragments(&words[1..])?;
        if halides.len() == 1 && halides.len() < count && !mono {
            for _ in halides.len()..count {
                halides.push(
                    self.assembly
                        .state
                        .fragment_manager
                        .copy_fragment(halides[0])?,
                );
            }
        } else if halides.len() > count || !mono && halides.len() < count {
            return Err(error(
                "Mismatch between number of halide/pseudo halide fragments and acidic oxygens",
            ));
        }
        for index in (0..halides.len()).rev() {
            let f = halides[index];
            let replacement = self.default_in_atom(f)?;
            let oxygen = acid.functional_atoms[index];
            if self.assembly.graph().atom(oxygen).element != Element::O {
                return Err(error("Acid functional atom expected to be oxygen"));
            }
            acid.remove_functional_atom(self.assembly.graph_mut(), index)?;
            let parent = self.assembly.graph().atom(oxygen).fragment;
            self.assembly
                .state
                .fragment_manager
                .replace_atom_preserving_connectivity(oxygen, replacement)?;
            self.assembly
                .state
                .fragment_manager
                .incorporate_fragment(f, parent)?;
        }
        Ok(())
    }
    fn build_addition_compound(&mut self, words: &[NodeId]) -> Result<()> {
        let first = *words
            .first()
            .ok_or_else(|| error("Missing addition compound word"))?;
        if self.assembly.attr(first, "type").as_deref() != Some("full") {
            return Err(error(
                "Don't alter wordRules.xml without checking the consequences!",
            ));
        }
        self.assembly.resolve_word_or_bracket(first)?;
        let group = self.rightmost_group_in_word(first)?;
        let parent = self.assembly.fragment(group)?;
        let atom = self.first_atom(parent)?;
        let charge = self.assembly.graph().atom(atom).charge;
        let mut fragments = Vec::new();
        for &word in &words[1..] {
            let node = self.functional_group(word)?;
            let f = self.make_functional_group(node)?;
            if self.assembly.attr(node, "type").as_deref() == Some("monoValentStandaloneGroup") {
                let target = self.default_in_atom(f)?;
                self.add_charge_and_protons(target, 1, 1);
            }
            fragments.push(f);
            if let Some(multiplier) = self.assembly.arena.previous_sibling(node) {
                let n = self.assembly.number(multiplier, "value")?;
                for _ in 1..n {
                    fragments.push(self.assembly.state.fragment_manager.copy_fragment(f)?);
                }
                self.assembly.arena.detach(multiplier);
            } else if words.len() == 2 {
                let incoming = self.assembly.graph().incoming_valency(atom);
                let expected = if charge > 0 {
                    incoming + charge
                } else if let Some(oxidation) =
                    self.assembly.graph().atom(atom).properties.oxidation_number
                {
                    oxidation
                } else if let Some(states) =
                    self.assembly.attr(group, "commonOxidationStatesAndMax")
                {
                    states
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .parse()
                        .map_err(|_| error("Invalid common oxidation states"))?
                } else {
                    *valence::possible_valencies(self.assembly.graph().atom(atom).element, charge)
                        .and_then(|states| states.first())
                        .ok_or_else(|| error("No known valency for addition compound"))?
                };
                for _ in 1..(expected - incoming).max(1) {
                    fragments.push(self.assembly.state.fragment_manager.copy_fragment(f)?);
                }
            }
        }
        if charge > 0 {
            self.assembly.graph_mut().atom_mut(atom).charge = charge - fragments.len() as i32;
        }
        self.aluminium_hydride_special_case(first, atom, &mut fragments)?;
        if valence::maximum_valency(
            self.assembly.graph().atom(atom).element,
            self.assembly.graph().atom(atom).charge,
        )
        .is_some_and(|maximum| fragments.len() as i32 > maximum)
        {
            return Err(error(
                "Too many halides/pseudo halides added to elementary atom",
            ));
        }
        for &f in fragments.iter().rev() {
            let target = self.default_in_atom(f)?;
            self.assembly
                .state
                .fragment_manager
                .incorporate_fragment_with_bond(f, target, parent, atom, 1)?;
        }
        Ok(())
    }
    fn aluminium_hydride_special_case(
        &mut self,
        first: NodeId,
        atom: AtomId,
        fragments: &mut Vec<FragmentId>,
    ) -> Result<()> {
        let a = self.assembly.graph().atom(atom);
        if !matches!(a.element, Element::Al | Element::B) || a.charge != 0 {
            return Ok(());
        }
        let all_h = fragments.iter().all(|&f| {
            self.default_in_atom(f)
                .is_ok_and(|a| self.assembly.graph().atom(a).element == Element::H)
        });
        if fragments.len() == 4 && all_h {
            self.assembly.graph_mut().atom_mut(atom).charge = -1;
            return Ok(());
        }
        if fragments.len() != 3 || !all_h {
            return Ok(());
        }
        let Some(parent) = self.assembly.arena[first].parent else {
            return Ok(());
        };
        let Some(rule) = self.assembly.arena.previous_sibling(parent) else {
            return Ok(());
        };
        if self.assembly.arena[rule].children.len() != 1 {
            return Ok(());
        }
        let Some(word) = self.assembly.arena.first_child_named(rule, "word") else {
            return Ok(());
        };
        if self.assembly.arena[word].children.len() != 1 {
            return Ok(());
        }
        let Some(root) = self.assembly.arena.first_child_named(word, "root") else {
            return Ok(());
        };
        if self.assembly.arena[root].children.len() != 1 {
            return Ok(());
        }
        let Some(group) = self.assembly.arena.first_child_named(root, "group") else {
            return Ok(());
        };
        if self.assembly.attr(group, "type").as_deref() != Some("elementaryAtom") {
            return Ok(());
        }
        let cation = self.first_atom(self.assembly.fragment(group)?)?;
        if matches!(
            self.assembly.graph().atom(cation).element,
            Element::Li | Element::Na | Element::K | Element::Rb | Element::Cs
        ) {
            fragments.push(
                self.assembly
                    .state
                    .fragment_manager
                    .copy_fragment(fragments[0])?,
            );
            self.assembly.graph_mut().atom_mut(atom).charge = -1;
        }
        Ok(())
    }
    fn build_glycol(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() != 2 {
            return Err(error("Glycol functionalTerm word expected"));
        }
        self.assembly.resolve_word_or_bracket(words[0])?;
        let parent = self
            .assembly
            .fragment(self.rightmost_group_in_word(words[0])?)?;
        if self.assembly.graph().fragment(parent).out_atoms.len() != 2 {
            return Err(error("Glycol class names expect two outAtoms"));
        }
        if self.assembly.attr(words[1], "type").as_deref() != Some("functionalTerm") {
            return Err(error("Glycol functionalTerm word expected"));
        }
        let nodes = self
            .assembly
            .arena
            .descendants_named(words[1], "functionalClass");
        if nodes.len() != 1 {
            return Err(error("Glycol functional class not found where expected"));
        }
        for index in 0..2 {
            let out = self.assembly.graph().fragment(parent).out_atoms[index].clone();
            if out.valency != 1 {
                return Err(error("OutAtom has unexpected valency. Expected 1"));
            }
            let from = if out.explicitly_set {
                out.atom
            } else {
                self.assembly
                    .find_atom_for_unlocanted_radical(parent, &out)?
            };
            let smiles = if index == 0 {
                self.assembly
                    .attr(nodes[0], "value")
                    .ok_or_else(|| error("Glycol group has no SMILES"))?
            } else {
                "O".into()
            };
            let hydroxy = self.assembly.state.fragment_manager.build_smiles(
                &smiles,
                "functionalClass",
                "none",
            )?;
            let target = self.first_atom(hydroxy)?;
            self.assembly
                .state
                .fragment_manager
                .create_bond(from, target, 1)?;
            self.assembly
                .state
                .fragment_manager
                .incorporate_fragment(hydroxy, parent)?;
        }
        self.assembly.graph_mut().remove_out_atom(parent, 1);
        self.assembly.graph_mut().remove_out_atom(parent, 0);
        Ok(())
    }
    fn build_glycol_ether(&mut self, words: &[NodeId]) -> Result<()> {
        let first = *words
            .first()
            .ok_or_else(|| error("Cannot find glycol word"))?;
        self.assembly.resolve_word_or_bracket(first)?;
        if self.assembly.attr(first, "type").as_deref() != Some("full") {
            return Err(error("OPSIN Bug: Cannot find glycol word!"));
        }
        let mut attached = Vec::new();
        for &word in &words[1..] {
            if self.assembly.attr(word, "type").as_deref() != Some("functionalTerm") {
                self.assembly.resolve_word_or_bracket(word)?;
                attached.push(word);
            } else if !self
                .assembly
                .attr(word, "value")
                .is_some_and(|s| s.eq_ignore_ascii_case("ether"))
            {
                return Err(error(
                    "Unexpected word encountered when applying glycol ether word rule",
                ));
            }
        }
        if attached.is_empty() {
            return Err(error(
                "OPSIN Bug: Unexpected number of substituents for glycol ether",
            ));
        }
        let group = self.rightmost_group_in_word(first)?;
        let hydroxy =
            ft::find_hydroxy_groups(self.assembly.graph(), self.assembly.fragment(group)?)?;
        if hydroxy.len() < attached.len() {
            return Err(error(
                "Insufficient hydroxy groups to form required number of ethers",
            ));
        }
        for (&word, &oxygen) in attached.iter().zip(&hydroxy) {
            let mut results = self.results(word)?;
            if !results.out_atoms.is_empty() {
                let from = results.out_atom(self.assembly.graph(), 0)?.atom;
                self.assembly
                    .state
                    .fragment_manager
                    .create_bond(oxygen, from, 1)?;
                results.remove_out_atom(self.assembly.graph_mut(), 0)?;
            } else if !results.functional_atoms.is_empty() {
                let target = results.functional_atoms[0];
                self.neutralise_charge(target);
                self.assembly
                    .state
                    .fragment_manager
                    .replace_atom_preserving_connectivity(oxygen, target)?;
                results.remove_functional_atom(self.assembly.graph_mut(), 0)?;
            } else {
                return Err(error(
                    "Word had neither an outAtom nor a functionalAtom; ether or ester could not be formed",
                ));
            }
        }
        Ok(())
    }
    fn build_acetal(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() < 2 {
            return Err(error("Acetal rule has too few words"));
        }
        for &word in &words[..words.len() - 1] {
            self.assembly.resolve_word_or_bracket(word)?;
        }
        let mut substituents = BuildResults::new();
        for &word in &words[1..words.len() - 1] {
            let results = self.results(word)?;
            if results.out_atoms.is_empty() {
                return Err(error(
                    "Substituent was expected to have at least one outAtom",
                ));
            }
            if results.out_atoms.len() == 1
                && let Some(locant) = self.assembly.attr(word, "locant")
            {
                let out = results.out_atom(self.assembly.graph(), 0)?.clone();
                let f = self.assembly.graph().atom(out.atom).fragment;
                let index = self
                    .assembly
                    .graph()
                    .fragment(f)
                    .out_atoms
                    .iter()
                    .position(|a| a.id == out.id)
                    .unwrap();
                self.assembly
                    .graph_mut()
                    .set_out_atom_locant(f, index, Some(locant));
            }
            substituents.merge(results);
        }
        let root = self
            .assembly
            .fragment(self.rightmost_group_in_word(words[0])?)?;
        let mut oxygens = self.find_carbonyl_oxygens(root, &[]);
        let nodes = self
            .assembly
            .arena
            .descendants_named(*words.last().unwrap(), "functionalClass");
        if nodes.len() != 1 {
            return Err(error("OPSIN bug: unable to find acetal functionalClass"));
        }
        let node = nodes[0];
        let class = self.assembly.arena.value(node);
        let mut elements = self
            .assembly
            .attr(node, "value")
            .unwrap_or_default()
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if elements.len() != 2 {
            return Err(error("Acetal requires two chalcogens"));
        }
        let mut count = 1;
        if let Some(previous) = self.assembly.arena.previous_sibling(node) {
            if self.assembly.arena[previous].name == "multiplier" {
                count = self.assembly.number(previous, "value")?;
            } else {
                self.replace_acetal_chalcogens(node, &mut elements)?;
            }
        }
        if oxygens.len() < count {
            return Err(error(format!(
                "Insufficient carbonyls to form {count} {class}"
            )));
        }
        let hemi = class.contains("hemi");
        let mut acetals = Vec::new();
        for _ in 0..count {
            let oxygen = oxygens.remove(0);
            let carbon = self.assembly.graph().neighbours(oxygen)[0];
            self.assembly
                .state
                .fragment_manager
                .remove_atom_and_associated_bonds(oxygen);
            let f = self.assembly.state.fragment_manager.build_smiles(
                &elements.join("."),
                "",
                "none",
            )?;
            ft::assign_element_locants(self.assembly.graph_mut(), f, &[])?;
            let atoms = self.assembly.graph().fragment(f).atoms.clone();
            if atoms.len() != 2 {
                return Err(error("Acetal fragment requires two atoms"));
            }
            for &atom in &atoms {
                self.assembly
                    .state
                    .fragment_manager
                    .create_bond(carbon, atom, 1)?;
            }
            let parent = self.assembly.graph().atom(carbon).fragment;
            self.assembly
                .state
                .fragment_manager
                .incorporate_fragment(f, parent)?;
            acetals.push(f);
        }
        let expected = if hemi { count } else { 2 * count };
        if substituents.out_atoms.len() != expected {
            return Err(error(format!(
                "incorrect number of substituents when forming {class}"
            )));
        }
        self.connect_substituents_to_acetal(acetals, substituents, hemi)
    }
    fn replace_acetal_chalcogens(&self, node: NodeId, elements: &mut [String]) -> Result<()> {
        let parent = self.assembly.arena[node]
            .parent
            .ok_or_else(|| error("Acetal term has no parent"))?;
        let mut current = self.assembly.arena[parent]
            .children
            .first()
            .copied()
            .ok_or_else(|| error("Empty acetal term"))?;
        let mut multiplier = 1;
        if self.assembly.arena[current].name == "multiplier" {
            multiplier = self.assembly.number(current, "value")?;
            if multiplier > 2 {
                return Err(error("Acetal only has two oxygen!"));
            }
            current = self
                .assembly
                .arena
                .next_sibling(current)
                .ok_or_else(|| error("Missing acetal replacement"))?;
        }
        let mut i = 0;
        while current != node {
            if self.assembly.arena[current].name != "group" {
                return Err(error("Unexpected element before acetal"));
            }
            for _ in 0..multiplier {
                if i >= 2 {
                    return Err(error("Acetal only has two oxygen!"));
                }
                if elements[i] != "O" {
                    return Err(error(
                        "Replacement on acetal can only be used to replace oxygen!",
                    ));
                }
                elements[i] = self
                    .assembly
                    .attr(current, "value")
                    .ok_or_else(|| error("Missing chalcogen replacement"))?;
                i += 1;
            }
            current = self
                .assembly
                .arena
                .next_sibling(current)
                .ok_or_else(|| error("Missing acetal term"))?;
        }
        Ok(())
    }
    fn search_non_suffix_locant(&self, start: AtomId, locant: &str) -> Option<AtomId> {
        let mut stack = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(atom) = stack.pop() {
            visited.insert(atom);
            for neighbour in self.assembly.graph().neighbours(atom) {
                if visited.contains(&neighbour) {
                    continue;
                }
                let a = self.assembly.graph().atom(neighbour);
                let locants = a
                    .locants
                    .iter()
                    .filter(|l| !ft::is_element_symbol_locant(l))
                    .collect::<Vec<_>>();
                if !locants.is_empty() && a.atom_type != "suffix" {
                    if locants.iter().any(|l| l.as_str() == locant) {
                        return Some(neighbour);
                    }
                    continue;
                }
                stack.push(neighbour);
            }
        }
        None
    }
    fn connect_substituents_to_acetal(
        &mut self,
        mut acetals: Vec<FragmentId>,
        mut results: BuildResults,
        hemi: bool,
    ) -> Result<()> {
        let mut usages = BTreeMap::new();
        for index in (0..results.out_atoms.len()).rev() {
            let out = results.out_atom(self.assembly.graph(), index)?.clone();
            results.remove_out_atom(self.assembly.graph_mut(), index)?;
            let mut chosen = None;
            if let Some(locant) = &out.locant {
                for &f in &acetals {
                    if ft::is_numeric_locant(locant) {
                        if self
                            .search_non_suffix_locant(self.first_atom(f)?, locant)
                            .is_some()
                            && let Some(&atom) = self
                                .assembly
                                .graph()
                                .fragment(f)
                                .atoms
                                .iter()
                                .find(|&&a| self.assembly.graph().atom(a).bonds.len() == 1)
                        {
                            chosen = Some(atom);
                            break;
                        }
                    } else if let Some(atom) = self.assembly.graph().atom_by_locant(f, locant) {
                        chosen = Some(atom);
                        break;
                    }
                }
            } else {
                let f = *acetals
                    .first()
                    .ok_or_else(|| error("OPSIN bug: no acetal fragment available"))?;
                chosen = self
                    .assembly
                    .graph()
                    .fragment(f)
                    .atoms
                    .iter()
                    .copied()
                    .find(|&a| self.assembly.graph().atom(a).bonds.len() == 1);
            }
            let atom = chosen.ok_or_else(|| error("Unable to find suitable acetalFrag"))?;
            let f = self.assembly.graph().atom(atom).fragment;
            self.assembly.state.fragment_manager.create_bond(
                out.atom,
                atom,
                bond_order(out.valency)?,
            )?;
            let usage = usages.entry(f).or_insert(0usize);
            *usage += 1;
            if *usage >= 2 || hemi {
                acetals.retain(|&a| a != f);
            }
        }
        Ok(())
    }
    fn build_alcohol_ester(&mut self, words: &[NodeId]) -> Result<bool> {
        for &word in words {
            if self.assembly.attr(word, "type").as_deref() != Some("full") {
                return Err(error("Bug in word rule for potentialAlcoholEster"));
            }
            self.assembly.resolve_word_or_bracket(word)?;
        }
        if words.len() < 2 {
            return Err(error("Bug in word rule for potentialAlcoholEster"));
        }
        let count = words.len() - 1;
        let group = self.rightmost_group_in_word(words[0])?;
        let alcohol = self.assembly.fragment(group)?;
        let hydroxy = ft::find_hydroxy_groups(self.assembly.graph(), alcohol)?;
        let mut chosen = Vec::new();
        let mut acids = Vec::new();
        for &word in &words[1..] {
            let acid = self.results(word)?;
            if !self.appropriate_ate_for_alcohol(word, &acid)? {
                return Ok(false);
            }
            if let Some(locant) = self.assembly.attr(word, "locant") {
                let mut atom = self.assembly.atom_by_locant(alcohol, &locant)?;
                if !hydroxy.contains(&atom) || chosen.contains(&atom) {
                    atom = self
                        .assembly
                        .atom_by_locant(alcohol, &format!("O{locant}"))?;
                }
                if !hydroxy.contains(&atom) || chosen.contains(&atom) {
                    return Err(error(format!(
                        "{locant} did not point to a hydroxy group to be used for ester formation"
                    )));
                }
                chosen.push(atom);
            } else if words.len() == 2
                && let Some(atom) = self.assembly.graph().atom_by_locant(alcohol, "O5'")
                && hydroxy.contains(&atom)
            {
                chosen.push(atom);
            }
            acids.push(acid);
        }
        if chosen.len() < count {
            if !chosen.is_empty() {
                return Err(error(
                    "OPSIN Bug: Either all or none of the esters should be locanted in alcohol ester rule",
                ));
            }
            if hydroxy.len() == count
                || hydroxy.len() > count
                    && (crate::ambiguity::all_atoms_equivalent(self.assembly.graph(), &hydroxy)?
                        || self.assembly.arena.value(group) == "glycerol")
            {
                chosen.extend(hydroxy.iter().take(count).copied());
            } else {
                return Ok(false);
            }
        }
        for (index, acid) in acids.iter_mut().enumerate() {
            let group = self.rightmost_group_in_word(words[index + 1])?;
            if self
                .assembly
                .attr(group, "numberOfFunctionalAtomsToRemove")
                .is_none()
                && self.top_level_word_rule_count == 1
            {
                for index in (1..acid.functional_atoms.len()).rev() {
                    let atom = acid.remove_functional_atom(self.assembly.graph_mut(), index)?;
                    self.neutralise_charge(atom);
                }
            }
            let oxygen = acid.remove_functional_atom(self.assembly.graph_mut(), 0)?;
            self.neutralise_charge(oxygen);
            self.assembly
                .state
                .fragment_manager
                .replace_atom_preserving_connectivity(oxygen, chosen[index])?;
        }
        Ok(true)
    }
    fn appropriate_ate_for_alcohol(&self, word: NodeId, acid: &BuildResults) -> Result<bool> {
        if acid.functional_atoms.is_empty() {
            return Ok(false);
        }
        if self.assembly.attr(word, "locant").is_some() {
            return Ok(true);
        }
        if acid.functional_atoms.len() == 1 {
            let value = self.assembly.attr(word, "value").unwrap_or_default();
            let normalized = value.to_ascii_lowercase().replace('-', "");
            return Ok(!normalized.ends_with("trifluoroacetate")
                && !normalized.ends_with("trifluoroacetat"));
        }
        let group = self.rightmost_group_in_word(word)?;
        let text = self.assembly.arena.value(group);
        let re=regex::Regex::new(r"(?i)\A(?:(?:ortho-?)?(?:bor|phosphor|phosphate?|phosphite?)|carbam|carbon|sulfur|sulfate?|sulfite?|diphosphate?|triphosphate?)\z").map_err(|e|error(e.to_string()))?;
        Ok(re.is_match(&text))
    }
    fn split_alcohol_ester_rule(&mut self, words: &[NodeId]) -> Result<()> {
        let rule = self.assembly.arena[words[0]]
            .parent
            .ok_or_else(|| error("Alcohol ester word has no rule"))?;
        self.assembly.arena[rule].set_attribute("wordRule", "simple");
        let value = self.assembly.attr(words[0], "value").unwrap_or_default();
        self.assembly.arena[rule].set_attribute("value", value);
        let new_rule = self.assembly.arena.grouping("wordRule");
        self.assembly.arena[new_rule].set_attribute("type", "full");
        self.assembly.arena[new_rule].set_attribute("wordRule", "simple");
        let value = self.assembly.attr(words[1], "value").unwrap_or_default();
        self.assembly.arena[new_rule].set_attribute("value", value);
        self.assembly.arena.insert_after(rule, new_rule);
        for &word in &words[1..] {
            self.assembly.arena.detach(word);
            self.assembly.arena.add_child(new_rule, word);
        }
        Ok(())
    }
    fn build_amine_di_conjunctive_suffix(&mut self, words: &[NodeId]) -> Result<()> {
        for &word in words {
            if self.assembly.attr(word, "type").as_deref() != Some("full") {
                return Err(error("Bug in word rule for amineDiConjunctiveSuffix"));
            }
            self.assembly.resolve_word_or_bracket(word)?;
        }
        if words.len() != 3 {
            return Err(error("amineDiConjunctiveSuffix expects 3 words"));
        }
        let f = self
            .assembly
            .fragment(self.rightmost_group_in_word(words[0])?)?;
        let amine = self
            .assembly
            .graph()
            .fragment(f)
            .default_in_atom
            .ok_or_else(|| error("OPSIN did not know where the amino acid amine was located"))?;
        for &word in &words[1..] {
            let f = self
                .assembly
                .fragment(self.rightmost_group_in_word(word)?)?;
            if self.assembly.attr(word, "locant").is_some_and(|s| s != "N") {
                return Err(error("OPSIN Bug: locant expected to be N"));
            }
            let carbon =
                ft::last_non_suffix_carbon_with_sufficient_valency(self.assembly.graph(), f)
                    .ok_or_else(|| {
                        error("OPSIN Bug: Unable to find non suffix carbon with sufficient valency")
                    })?;
            self.assembly
                .state
                .fragment_manager
                .create_bond(carbon, amine, 1)?;
        }
        Ok(())
    }
    fn build_cyclic_peptide(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() != 2 {
            return Err(error("OPSIN Bug: Expected 2 words in cyclic peptide name"));
        }
        let word = words[1];
        self.assembly.resolve_word_or_bracket(word)?;
        let mut results = self.results(word)?;
        if results.out_atoms.len() != 1 {
            return Err(error("Cyclic peptide building failed: Expected 1 outAtom"));
        }
        let from = self.out_atom(&results, 0)?;
        let amino = self
            .assembly
            .arena
            .descendants_named(word, "group")
            .into_iter()
            .filter(|&g| self.assembly.attr(g, "type").as_deref() == Some("aminoAcid"))
            .collect::<Vec<_>>();
        if amino.len() < 2 {
            return Err(error(
                "Cyclic peptide building failed: Requires at least two amino acids!",
            ));
        }
        let target = self.default_in_atom(self.assembly.fragment(amino[0])?)?;
        let order = results.out_atom(self.assembly.graph(), 0)?.valency;
        self.assembly
            .state
            .fragment_manager
            .create_bond(from, target, bond_order(order)?)?;
        results.remove_all_out_atoms(self.assembly.graph_mut())?;
        Ok(())
    }
    fn build_polymer(&mut self, words: &[NodeId]) -> Result<()> {
        if words.len() != 2 {
            return Err(error("Currently unsupported polymer name type"));
        }
        self.assembly.resolve_word_or_bracket(words[1])?;
        let mut results = self.results(words[1])?;
        if results.out_atoms.len() != 2 {
            return Err(error("Polymer building failed: Two termini were not found"));
        }
        let incoming = self.out_atom(&results, 0)?;
        let outgoing = self.out_atom(&results, 1)?;
        for (index, from, other, label) in [
            (0, incoming, outgoing, "alpha"),
            (1, outgoing, incoming, "omega"),
        ] {
            let order = results.out_atom(self.assembly.graph(), index)?.valency;
            let smiles = format!("[{}|{order}]", self.assembly.graph().atom(other).element);
            let f = self
                .assembly
                .state
                .fragment_manager
                .build_smiles(&smiles, "", label)?;
            let r = self.first_atom(f)?;
            self.assembly.graph_mut().atom_mut(r).properties.atom_class = Some(index as u32 + 1);
            self.assembly
                .state
                .fragment_manager
                .create_bond(from, r, bond_order(order)?)?;
            self.polymer_attachment_points.push(r);
        }
        results.remove_all_out_atoms(self.assembly.graph_mut())?;
        Ok(())
    }
    fn atom_has_locant(&self, atom: AtomId, locant: &str) -> bool {
        let a = self.assembly.graph().atom(atom);
        if a.locants.iter().any(|l| l == locant) {
            return true;
        }
        let Some((symbol, primes, backbone)) = ft::parse_amino_acid_style_locant(locant) else {
            return false;
        };
        if a.element.symbol() != symbol {
            return false;
        }
        if !primes.is_empty() && !a.locants.iter().any(|l| l == &format!("{symbol}{primes}")) {
            return false;
        }
        self.search_non_suffix_locant(atom, backbone).is_some()
    }
    fn determine_functional_atom(
        &mut self,
        locant: &str,
        results: &mut BuildResults,
    ) -> Result<AtomId> {
        for (index, &atom) in results.functional_atoms.iter().enumerate() {
            if self.atom_has_locant(atom, locant) {
                results.remove_functional_atom(self.assembly.graph_mut(), index)?;
                self.assembly
                    .graph_mut()
                    .remove_ambiguous_element_assignment_members(atom, &[atom]);
                return Ok(atom);
            }
        }
        if ft::is_numeric_locant(locant) {
            for (index, &atom) in results.functional_atoms.iter().enumerate() {
                if self.search_non_suffix_locant(atom, locant).is_some() {
                    results.remove_functional_atom(self.assembly.graph_mut(), index)?;
                    self.assembly
                        .graph_mut()
                        .remove_ambiguous_element_assignment_members(atom, &[atom]);
                    return Ok(atom);
                }
            }
        } else if ft::is_element_symbol_locant(locant) {
            let bare = Element::from_symbol(locant).is_some();
            for (index, &atom) in results.functional_atoms.iter().enumerate() {
                if bare && self.assembly.graph().atom(atom).element.symbol() == locant {
                    results.remove_functional_atom(self.assembly.graph_mut(), index)?;
                    return Ok(atom);
                }
                for candidate in self
                    .assembly
                    .graph()
                    .atom(atom)
                    .properties
                    .ambiguous_element_assignment
                    .clone()
                {
                    if self.atom_has_locant(candidate, locant)
                        || bare && self.assembly.graph().atom(candidate).element.symbol() == locant
                    {
                        let a = self.assembly.graph().atom(atom).clone();
                        let b = self.assembly.graph().atom(candidate).clone();
                        self.assembly.graph_mut().clear_locants(atom);
                        self.assembly.graph_mut().clear_locants(candidate);
                        for locant in b.locants {
                            self.assembly.graph_mut().add_locant(atom, locant);
                        }
                        for locant in a.locants {
                            self.assembly.graph_mut().add_locant(candidate, locant);
                        }
                        self.assembly.graph_mut().atom_mut(atom).element = b.element;
                        self.assembly.graph_mut().atom_mut(candidate).element = a.element;
                        results.remove_functional_atom(self.assembly.graph_mut(), index)?;
                        self.assembly
                            .graph_mut()
                            .remove_ambiguous_element_assignment_members(atom, &[atom]);
                        return Ok(atom);
                    }
                }
            }
        }
        Err(error(format!(
            "Cannot find functional atom with locant: {locant} to form an ester with"
        )))
    }
    fn first_leaf(&self, mut node: NodeId) -> Option<NodeId> {
        while let Some(&child) = self.assembly.arena[node].children.first() {
            node = child;
        }
        Some(node)
    }
    fn clone_after(&mut self, node: NodeId) -> Result<NodeId> {
        let copy = self.assembly.clone_element(node, 0)?;
        self.assembly.arena.insert_after(node, copy);
        Ok(copy)
    }
    fn manipulate_stoichiometry(&mut self, molecule: NodeId, rules: &[NodeId]) -> Result<()> {
        let mut explicit = false;
        for &rule in rules {
            if self.assembly.attr(rule, "stoichiometry").is_some() {
                let count = self.assembly.number(rule, "stoichiometry")?;
                self.assembly.arena[rule].remove_attribute("stoichiometry");
                for _ in 1..count {
                    self.clone_after(rule)?;
                }
                explicit = true;
            }
        }
        let mut fractional = Vec::new();
        let mut charged = false;
        for &rule in rules {
            if let Some(first) = self.first_leaf(rule)
                && self.assembly.arena[first].name == "fractionalMultiplier"
            {
                if explicit {
                    return Err(error(
                        "Fractional multipliers should not be used in conjunction with explicit stoichiometry",
                    ));
                }
                let value = self.assembly.attr(first, "value").unwrap_or_default();
                let parts = value.split('/').collect::<Vec<_>>();
                if parts.len() != 2 {
                    return Err(error("OPSIN Bug: malformed fractional multiplier"));
                }
                let numerator = parts[0]
                    .parse::<usize>()
                    .map_err(|_| error("OPSIN Bug: malformed fractional multiplier"))?;
                let denominator = parts[1]
                    .parse::<usize>()
                    .map_err(|_| error("OPSIN Bug: malformed fractional multiplier"))?;
                if denominator != 2 {
                    return Err(error("Only fractions of a 1/2 currently supported"));
                }
                for _ in 1..numerator {
                    fractional.push(self.clone_after(rule)?);
                }
                fractional.push(rule);
                charged |= self.results(rule)?.charge(self.assembly.graph()) != 0;
            }
        }
        if !fractional.is_empty() {
            if rules.len() == 1 {
                return Err(error(
                    "Unexpected fractional multiplier found at start of word",
                ));
            }
            if charged {
                for &rule in rules {
                    if !fractional.contains(&rule) {
                        self.clone_after(rule)?;
                    }
                }
            }
        }
        if self.assembly.attr(molecule, "isSalt").is_some() {
            self.deprotonate_acid_if_salt_with_metal(molecule)?;
        }
        let charge = self.assembly.state.fragment_manager.overall_charge();
        if charge != 0 {
            self.balance_charge_if_possible(molecule, charge, explicit)?;
        }
        if !fractional.is_empty() && !charged {
            for rule in self.assembly.arena.children_named(molecule, "wordRule") {
                if !fractional.contains(&rule) {
                    self.clone_after(rule)?;
                }
            }
        }
        Ok(())
    }
    fn deprotonate_acid_if_salt_with_metal(&mut self, molecule: NodeId) -> Result<()> {
        let mut neutral = Vec::new();
        let mut positive = 0;
        let mut negative = 0;
        for rule in self.assembly.arena.children_named(molecule, "wordRule") {
            let br = self.results(rule)?;
            let charge = br.charge(self.assembly.graph());
            if charge > 0 {
                positive += 1;
            } else if charge < 0 {
                negative += 1;
            } else {
                neutral.push(br);
            }
        }
        if negative == 0 && (positive > 0 || !self.implicit_cationic_metals(molecule)?.is_empty()) {
            for results in neutral.iter().rev() {
                let atoms = results
                    .fragments
                    .iter()
                    .flat_map(|&f| self.assembly.graph().fragment(f).functional_atoms.clone())
                    .collect::<Vec<_>>();
                for atom in atoms {
                    if self.assembly.graph().atom(atom).charge == 0
                        && self.assembly.graph().incoming_valency(atom) == 1
                    {
                        self.add_charge_and_protons(atom, -1, -1);
                    }
                }
            }
        }
        Ok(())
    }
    fn implicit_cationic_metals(&self, molecule: NodeId) -> Result<Vec<NodeId>> {
        let mut metals = Vec::new();
        for group in self.assembly.arena.descendants_named(molecule, "group") {
            if self.assembly.attr(group, "type").as_deref() != Some("elementaryAtom") {
                continue;
            }
            if let Some(states) = self.assembly.attr(group, "commonOxidationStatesAndMax") {
                let atom = self.first_atom(self.assembly.fragment(group)?)?;
                let a = self.assembly.graph().atom(atom);
                if a.charge == 0 && a.properties.oxidation_number.is_none() {
                    let highest = states
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .split(',')
                        .next_back()
                        .unwrap_or("")
                        .parse::<usize>()
                        .map_err(|_| error("Invalid typical oxidation state"))?;
                    if highest > a.bonds.len() {
                        metals.push(group);
                    }
                }
            }
        }
        Ok(metals)
    }
    fn set_typical_cationic_charges(&mut self, metals: &[NodeId], mut charge: i32) -> Result<i32> {
        for &group in metals {
            let atom = self.first_atom(self.assembly.fragment(group)?)?;
            let incoming = self.assembly.graph().incoming_valency(atom);
            let states = self
                .assembly
                .attr(group, "commonOxidationStatesAndMax")
                .ok_or_else(|| error("Metal has no typical oxidation states"))?;
            for state in states.split(':').next().unwrap_or("").split(',') {
                let state = state
                    .parse::<i32>()
                    .map_err(|_| error("Invalid typical oxidation state"))?;
                if state >= incoming {
                    let added = state - incoming;
                    charge += added;
                    self.assembly.graph_mut().atom_mut(atom).charge = added;
                    break;
                }
            }
        }
        Ok(charge)
    }
    fn set_cationic_charge_appropriately(
        &mut self,
        charge: i32,
        group: NodeId,
        common: bool,
    ) -> Result<bool> {
        let atom = self.first_atom(self.assembly.fragment(group)?)?;
        let needed = -(charge - self.assembly.graph().atom(atom).charge);
        let states = self
            .assembly
            .attr(group, "commonOxidationStatesAndMax")
            .ok_or_else(|| error("Metal has no oxidation state limits"))?;
        let parts = states.split(':').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(error("Invalid oxidation state limits"));
        }
        let valid = if common {
            let states = parts[0]
                .split(',')
                .map(|s| {
                    s.parse::<i32>()
                        .map_err(|_| error("Invalid common oxidation state"))
                })
                .collect::<Result<Vec<_>>>()?;
            states.contains(&needed)
        } else {
            let maximum = parts[1]
                .parse::<i32>()
                .map_err(|_| error("Invalid maximum oxidation state"))?;
            needed >= 0 && needed <= maximum
        };
        if valid {
            self.assembly.graph_mut().atom_mut(atom).charge = needed;
        }
        Ok(valid)
    }
    fn component_can_be_multiplied(&self, rule: NodeId) -> bool {
        if self.assembly.attr(rule, "wordRule").as_deref() == Some("simple")
            && self
                .assembly
                .arena
                .children_named(rule, "word")
                .iter()
                .filter(|&&word| self.assembly.attr(word, "type").as_deref() == Some("full"))
                .count()
                > 1
        {
            return false;
        }
        self.first_leaf(rule).is_some_and(|node| {
            !matches!(
                self.assembly.arena[node].name.as_str(),
                "multiplier" | "fractionalMultiplier"
            )
        })
    }
    fn multiply_charged_components(
        &mut self,
        negative: &[NodeId],
        positive: &[NodeId],
        charges: &BTreeMap<NodeId, i32>,
        overall: i32,
    ) -> Result<bool> {
        let multiply = if overall > 0 {
            if negative.len() != 1 {
                return Ok(false);
            }
            negative[0]
        } else {
            if positive.len() != 1 {
                return Ok(false);
            }
            positive[0]
        };
        let charge = charges[&multiply];
        if charge == 0 {
            return Err(error(
                "Cannot multiply an uncharged component to balance charge",
            ));
        }
        if overall % charge == 0 {
            if !self.component_can_be_multiplied(multiply) {
                return Ok(false);
            }
            let count = (overall / charge).unsigned_abs();
            for _ in 0..count {
                self.clone_after(multiply)?;
            }
        } else {
            if positive.len() != 1
                || negative.len() != 1
                || !self.component_can_be_multiplied(positive[0])
                || !self.component_can_be_multiplied(negative[0])
            {
                return Ok(false);
            }
            let p = charges[&positive[0]];
            let n = charges[&negative[0]].abs();
            let total = p
                .checked_mul(n)
                .ok_or_else(|| error("Charge balancing multiplier overflow"))?;
            for _ in 1..total / n {
                self.clone_after(negative[0])?;
            }
            for _ in 1..total / p {
                self.clone_after(positive[0])?;
            }
        }
        Ok(true)
    }
    fn balance_charge_if_possible(
        &mut self,
        molecule: NodeId,
        mut overall: i32,
        explicit: bool,
    ) -> Result<()> {
        let rules = self.assembly.arena.children_named(molecule, "wordRule");
        if rules.len() == 1 {
            if overall == 1 {
                self.phospho_zwitterion_special_case(rules[0])?;
            }
            return Ok(());
        }
        let metals = self.implicit_cationic_metals(molecule)?;
        overall = self.set_typical_cationic_charges(&metals, overall)?;
        if overall == 0 {
            return Ok(());
        }
        if overall == -2 && self.trihalide_special_case(&rules)? {
            return Ok(());
        }
        let (mut positive, mut negative) = (Vec::new(), Vec::new());
        let mut charges = BTreeMap::new();
        let mut results = BTreeMap::new();
        for &rule in &rules {
            let br = self.results(rule)?;
            let charge = br.charge(self.assembly.graph());
            if charge > 0 {
                positive.push(rule);
            } else if charge < 0 {
                negative.push(rule);
            }
            charges.insert(rule, charge);
            results.insert(rule, br);
        }
        if metals.len() == 1 && overall < 0 {
            let common = negative.len() == 1
                && self
                    .assembly
                    .arena
                    .children_named(negative[0], "word")
                    .len()
                    == 1;
            if self.set_cationic_charge_appropriately(overall, metals[0], common)? {
                return Ok(());
            }
        }
        if !explicit
            && (positive.len() == 1 && metals.is_empty() && !negative.is_empty()
                || !positive.is_empty() && negative.len() == 1)
            && self.multiply_charged_components(&negative, &positive, &charges, overall)?
        {
            return Ok(());
        }
        if metals.len() == 1 && self.set_cationic_charge_appropriately(overall, metals[0], false)? {
            return Ok(());
        }
        if overall < 0 {
            if overall == -1 && self.acetylide_special_case(&rules)? {
                return Ok(());
            }
            let functional_charge = results
                .values()
                .flat_map(|br| &br.functional_atoms)
                .map(|&atom| self.assembly.graph().atom(atom).charge)
                .sum::<i32>();
            if functional_charge <= overall {
                for &rule in &rules {
                    let br = results.get_mut(&rule).unwrap();
                    for i in (0..br.functional_atoms.len()).rev() {
                        if overall == 0 {
                            return Ok(());
                        }
                        let atom = br.functional_atoms[i];
                        overall -= self.assembly.graph().atom(atom).charge;
                        self.neutralise_charge(atom);
                        br.remove_functional_atom(self.assembly.graph_mut(), i)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn trihalide_special_case(&mut self, rules: &[NodeId]) -> Result<bool> {
        for &rule in rules {
            if self.assembly.arena[rule].children.len() == 3
                && matches!(
                    self.assembly.attr(rule, "value").as_deref(),
                    Some("tribromide" | "tribromid" | "triiodide" | "triiodid")
                )
            {
                let mut atoms = Vec::new();
                for child in self.assembly.arena[rule].children.clone() {
                    let groups = self.assembly.arena.descendants_named(child, "group");
                    if groups.len() != 1 {
                        return Err(error("OPSIN Bug: Unexpected trihalide representation"));
                    }
                    atoms.push(self.first_atom(self.assembly.fragment(groups[0])?)?);
                }
                for &other in &atoms[1..] {
                    self.assembly.graph_mut().atom_mut(other).charge = 0;
                    self.assembly
                        .state
                        .fragment_manager
                        .create_bond(atoms[0], other, 1)?;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn phospho_zwitterion_special_case(&mut self, rule: NodeId) -> Result<bool> {
        let mut fragments = Vec::new();
        for group in self.assembly.arena.descendants_named(rule, "group") {
            if self.assembly.attr(group, "subType").as_deref() == Some("phospho")
                && self.assembly.arena.value(group).ends_with("phospho")
            {
                fragments.push(self.assembly.fragment(group)?);
            }
        }
        if fragments.len() == 1 {
            let matches = ft::find_hydroxy_like_terminal_atoms(
                self.assembly.graph(),
                &self.assembly.graph().fragment(fragments[0]).atoms,
                Element::O,
            );
            for atom in matches {
                if self
                    .assembly
                    .graph()
                    .atom(self.assembly.graph().neighbours(atom)[0])
                    .element
                    == Element::P
                {
                    self.add_charge_and_protons(atom, -1, -1);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn acetylide_special_case(&mut self, rules: &[NodeId]) -> Result<bool> {
        for &rule in rules {
            if matches!(
                self.assembly.attr(rule, "value").as_deref(),
                Some("acetylide" | "acetylid")
            ) {
                let groups = self.assembly.arena.descendants_named(rule, "group");
                if groups.len() != 1 {
                    return Err(error("OPSIN Bug: Unexpected acetylide representation"));
                }
                let f = self.assembly.fragment(groups[0])?;
                let first = self.first_atom(f)?;
                let charge = self
                    .assembly
                    .graph()
                    .fragment(f)
                    .atoms
                    .iter()
                    .map(|&a| self.assembly.graph().atom(a).charge)
                    .sum::<i32>();
                if charge == -2 && self.assembly.graph().atom(first).charge == -1 {
                    self.add_charge_and_protons(first, 1, 1);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn check_connected_oxo(&self, atom: AtomId) -> bool {
        self.assembly
            .graph()
            .neighbours(atom)
            .into_iter()
            .any(|other| {
                self.assembly
                    .state
                    .fragment_manager
                    .token_for_fragment(self.assembly.graph().atom(other).fragment)
                    .is_some_and(|token| self.assembly.arena.value(token) == "oxo")
            })
    }
    fn process_special_cases(&mut self, groups: &[NodeId]) -> Result<()> {
        for &group in groups {
            let subtype = self.assembly.attr(group, "subType");
            if subtype.as_deref() == Some("oxidoLike") {
                let fragment = self.assembly.fragment(group)?;
                let oxido = self.first_atom(fragment)?;
                if self.assembly.graph().atom(oxido).bonds.len() != 1 {
                    continue;
                }
                let connected = self.assembly.graph().neighbours(oxido)[0];
                let a = self.assembly.graph().atom(connected);
                let element = a.element;
                if self.check_connected_oxo(connected) {
                    continue;
                }
                if self.assembly.graph().fragment(a.fragment).fragment_type == "elementaryAtom"
                    || matches!(element, Element::S | Element::P)
                        && a.charge == 0
                        && valence::check_valency_available_for_bond(
                            self.assembly.graph(),
                            connected,
                            1,
                        )
                {
                    self.neutralise_charge(oxido);
                    let bond = self.assembly.graph().atom(oxido).bonds[0];
                    self.assembly.graph_mut().bond_mut(bond).order = 2;
                } else if element == Element::N && a.charge == 0 {
                    let valency = self.assembly.graph().incoming_valency(connected) + a.out_valency;
                    if valency == 3 && a.spare_valency {
                        self.add_charge_and_protons(connected, 1, 1);
                    } else if valency == 4 {
                        if a.lambda_convention_valency == Some(5) {
                            self.neutralise_charge(oxido);
                            let bond = self.assembly.graph().atom(oxido).bonds[0];
                            self.assembly.graph_mut().bond_mut(bond).order = 2;
                        } else {
                            self.add_charge_and_protons(connected, 1, 1);
                        }
                    }
                }
            } else if self.assembly.attr(group, "type").as_deref() == Some("aminoAcid") {
                let fragment = self.assembly.fragment(group)?;
                for atom in self.assembly.graph().fragment(fragment).atoms.clone() {
                    let a = self.assembly.graph().atom(atom);
                    if a.element.is_chalcogen()
                        && a.element != Element::O
                        && a.bonds.len() == 3
                        && self.assembly.graph().incoming_valency(atom) == 3
                        && a.charge == 0
                    {
                        self.add_charge_and_protons(atom, 1, 1);
                    }
                }
            } else if subtype.as_deref() == Some("biochemical") {
                let fragment = self.assembly.fragment(group)?;
                if let Some(atom) = self.assembly.graph().atom_by_locant(fragment, "7") {
                    let name = self.assembly.arena.value(group);
                    let nucleoside = matches!(
                        name.as_str(),
                        "adenosin"
                            | "guanosin"
                            | "inosin"
                            | "thioinosin"
                            | "xanthosin"
                            | "nucleocidin"
                    ) || ["adenylic", "guanylic", "inosinic", "xanthylic"]
                        .iter()
                        .any(|s| name.contains(s))
                        || [
                            "adenylyl",
                            "adenosyl",
                            "guanylyl",
                            "guanosyl",
                            "inosinylyl",
                            "inosyl",
                            "xanthylyl",
                            "xanthosyl",
                        ]
                        .iter()
                        .any(|s| name.ends_with(s));
                    let a = self.assembly.graph().atom(atom);
                    if nucleoside
                        && a.element == Element::N
                        && a.spare_valency
                        && a.bonds.len() == 3
                        && self.assembly.graph().incoming_valency(atom) == 3
                        && a.charge == 0
                    {
                        self.add_charge_and_protons(atom, 1, 1);
                    }
                }
            } else {
                let name = self.assembly.arena.value(group);
                if matches!(
                    name.as_str(),
                    "borodeuterid" | "borodeuteride" | "borotritid" | "borotritide"
                ) {
                    let fragment = self.assembly.fragment(group)?;
                    let atom = self.first_atom(fragment)?;
                    let count =
                        ft::calculate_substitutable_hydrogen_atoms(self.assembly.graph(), atom);
                    let smiles = if name.contains("deuter") {
                        "[2H]"
                    } else {
                        "[3H]"
                    };
                    for _ in 0..count {
                        let f = self
                            .assembly
                            .state
                            .fragment_manager
                            .build_smiles(smiles, "", "none")?;
                        let hydrogen = self.first_atom(f)?;
                        self.assembly
                            .state
                            .fragment_manager
                            .create_bond(atom, hydrogen, 1)?;
                        self.assembly
                            .state
                            .fragment_manager
                            .incorporate_fragment(f, fragment)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn process_oxidation_numbers(&mut self, groups: &[NodeId]) -> Result<()> {
        for &group in groups {
            if self.assembly.attr(group, "type").as_deref() != Some("elementaryAtom") {
                continue;
            }
            let atom = self.first_atom(self.assembly.fragment(group)?)?;
            if let Some(oxidation) = self.assembly.graph().atom(atom).properties.oxidation_number {
                let mut removed_charge = 0;
                for neighbour in self.assembly.graph().neighbours(atom) {
                    let neutral = self
                        .assembly
                        .state
                        .fragment_manager
                        .token_for_fragment(self.assembly.graph().atom(neighbour).fragment)
                        .is_some_and(|token| {
                            self.assembly.arena.value(token) == "nitrosyl"
                                || self.assembly.arena.value(token) == "carbon"
                                    && self.assembly.attr(token, "type").as_deref()
                                        == Some("nonCarboxylicAcid")
                        });
                    if !neutral {
                        let bond = self.assembly.graph().bond_between(atom, neighbour).unwrap();
                        removed_charge += i32::from(self.assembly.graph().bond(bond).order);
                    }
                }
                self.assembly.graph_mut().atom_mut(atom).charge = oxidation - removed_charge;
            }
        }
        Ok(())
    }
    fn stereo_processing_order(&self, parent: NodeId) -> Vec<NodeId> {
        let mut descendants = Vec::new();
        let mut this_level = Vec::new();
        for &child in self.assembly.arena[parent].children.iter().rev() {
            if self.assembly.arena[child].name == crate::xml_declarations::STEREOCHEMISTRY_EL {
                this_level.push(child);
            } else {
                descendants.extend(self.stereo_processing_order(child));
            }
        }
        this_level.reverse();
        descendants.extend(this_level);
        descendants
    }
    fn process_stereochemistry(&mut self, molecule: NodeId, fragment: FragmentId) -> Result<()> {
        let elements = self.stereo_processing_order(molecule);
        let f = self.assembly.graph().fragment(fragment);
        let atoms = f
            .atoms
            .iter()
            .copied()
            .filter(|&a| self.assembly.graph().atom(a).parity.is_some())
            .collect::<Vec<_>>();
        let bonds = f
            .bonds
            .iter()
            .copied()
            .filter(|&b| self.assembly.graph().bond(b).stereo.is_some())
            .collect::<Vec<_>>();
        if elements.is_empty() && atoms.is_empty() && bonds.is_empty() {
            return Ok(());
        }
        let analysis = stereo_analyser::analyse(self.assembly.graph(), fragment)?;
        let stereo_bonds = analysis
            .bonds
            .into_iter()
            .filter(|b| ft::not_in_six_member_or_smaller_ring(self.assembly.graph(), b.bond))
            .collect::<Vec<_>>();
        let mut handler = StereochemistryHandler::new(
            self.assembly.state,
            self.assembly.arena,
            &analysis.centres,
            &stereo_bonds,
        );
        handler
            .apply_stereochemical_elements(&elements)
            .map_err(|e| error(e.to_string()))?;
        handler.remove_redundant_stereo_centres(&atoms, &bonds);
        Ok(())
    }
}
