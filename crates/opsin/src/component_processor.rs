//! Structure-aware component processing from OPSIN 2.9.0.
//! Source: `ComponentProcessor.java`, commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::ParsingError;
use crate::build_state::BuildState;
use crate::functional_replacement;
use crate::graph::Element as ChemEl;
use crate::graph::{AtomId, FragmentId, StereoGroup, StereoGroupType};
use crate::parse_tree::{Arena, NodeId};
use crate::suffix_applier::SuffixApplier;
use crate::suffix_rules::{SuffixRuleType, SuffixRules};
use crate::xml_declarations::*;

type Result<T> = std::result::Result<T, ParsingError>;
fn error(s: impl Into<String>) -> ParsingError {
    ParsingError(s.into())
}
fn frag(arena: &Arena, node: NodeId) -> Result<FragmentId> {
    arena[node]
        .fragment
        .ok_or_else(|| error(format!("No fragment associated with {}", arena.value(node))))
}
fn parse_num(value: &str) -> Result<usize> {
    value
        .parse()
        .map_err(|_| error(format!("Expected integer, found: {value}")))
}
fn attr(arena: &Arena, node: NodeId, name: &str) -> String {
    arena[node].attribute(name).unwrap_or("").into()
}
fn first_atom(state: &BuildState, fragment: FragmentId) -> Result<AtomId> {
    state
        .fragment_manager
        .graph
        .fragment(fragment)
        .atoms
        .first()
        .copied()
        .ok_or_else(|| error("Empty fragment"))
}
fn relative_atom(state: &BuildState, fragment: FragmentId, relative: &str) -> Result<AtomId> {
    let offset = parse_num(relative)?
        .checked_sub(1)
        .ok_or_else(|| error("Atom IDs start at one"))?;
    let first = first_atom(state, fragment)?;
    let atom = AtomId(first.0 + offset);
    if state
        .fragment_manager
        .graph
        .fragment(fragment)
        .atoms
        .contains(&atom)
    {
        Ok(atom)
    } else {
        Err(error(format!("No atom with relative ID {relative}")))
    }
}
fn locanted_atom(state: &BuildState, fragment: FragmentId, locant: &str) -> Result<AtomId> {
    state
        .fragment_manager
        .graph
        .atom_by_locant(fragment, locant)
        .ok_or_else(|| error(format!("No atom with locant {locant}")))
}

#[derive(Clone)]
struct AtomReference {
    kind: String,
    reference: String,
}
impl AtomReference {
    fn resolve(
        &self,
        state: &BuildState,
        arena: &Arena,
        group: NodeId,
        fragment: FragmentId,
        ambiguous: &mut bool,
    ) -> Result<AtomId> {
        match self.kind.to_ascii_lowercase().as_str() {
            "defaultlocant" | "locant" => {
                *ambiguous |= self.kind.eq_ignore_ascii_case("defaultlocant");
                if self.reference == "required" {
                    return Err(error(format!(
                        "{} requires an allowed locant",
                        arena.value(group)
                    )));
                }
                locanted_atom(state, fragment, &self.reference)
            }
            "defaultid" | "id" => {
                *ambiguous |= self.kind.eq_ignore_ascii_case("defaultid");
                relative_atom(state, fragment, &self.reference)
            }
            _ => Err(error("Malformed atom reference")),
        }
    }
}

pub struct ComponentProcessor<'a> {
    pub state: &'a mut BuildState,
    pub suffix_rules: &'a SuffixRules,
}
impl<'a> ComponentProcessor<'a> {
    pub fn new(state: &'a mut BuildState, suffix_rules: &'a SuffixRules) -> Self {
        Self {
            state,
            suffix_rules,
        }
    }

    /// Resolves a group using its upstream SMILES and all token metadata.
    pub fn resolve_group(&mut self, arena: &mut Arena, group: NodeId) -> Result<FragmentId> {
        let smiles = attr(arena, group, VALUE_ATR);
        let labels = arena[group]
            .attribute(LABELS_ATR)
            .unwrap_or(NONE_LABELS_VAL)
            .to_owned();
        let fragment = self
            .state
            .fragment_manager
            .build_token_smiles(&smiles, arena, group, &labels)
            .map_err(|e| error(e.to_string()))?;
        arena[group].fragment = Some(fragment);
        self.process_xylene_like_nomenclature(arena, group, fragment)?;
        arena[group].fragment = Some(fragment);
        if let Some(locant) = arena[group].attribute(DEFAULTINLOCANT_ATR) {
            let atom = locanted_atom(self.state, fragment, locant)?;
            self.state
                .fragment_manager
                .graph
                .fragment_mut(fragment)
                .default_in_atom = Some(atom);
        } else if let Some(id) = arena[group].attribute(DEFAULTINID_ATR) {
            let atom = relative_atom(self.state, fragment, id)?;
            self.state
                .fragment_manager
                .graph
                .fragment_mut(fragment)
                .default_in_atom = Some(atom);
        }
        if let Some(ids) = arena[group].attribute(FUNCTIONALIDS_ATR) {
            for id in ids.split(',') {
                let atom = relative_atom(self.state, fragment, id)?;
                self.state
                    .fragment_manager
                    .graph
                    .fragment_mut(fragment)
                    .functional_atoms
                    .push(atom);
            }
        }
        self.apply_traditional_alkane_numbering(arena, group, fragment)?;
        if let Some(labels) = arena[group].attribute(HOMOLOGY_ATR) {
            let values: Vec<_> = labels.split(';').collect();
            let homology: Vec<_> = self
                .state
                .fragment_manager
                .graph
                .fragment(fragment)
                .atoms
                .iter()
                .copied()
                .filter(|&a| self.state.fragment_manager.graph.atom(a).element == ChemEl::R)
                .collect();
            if values.len() != homology.len() {
                return Err(error(format!(
                    "OPSIN Bug: Number of homology atoms should match number of homology labels! for: {}",
                    arena.value(group)
                )));
            }
            for (atom, value) in homology.into_iter().zip(values) {
                self.state
                    .fragment_manager
                    .graph
                    .atom_mut(atom)
                    .properties
                    .homology_group = Some(value.into());
            }
        }
        if arena[group].attribute(TYPE_ATR) == Some(ELEMENTARYATOM_TYPE_VAL) {
            for atom in self
                .state
                .fragment_manager
                .graph
                .fragment(fragment)
                .atoms
                .clone()
            {
                self.state
                    .fragment_manager
                    .graph
                    .atom_mut(atom)
                    .implicit_hydrogen_allowed = false;
            }
        }
        Ok(fragment)
    }

    fn acceptable_front_locants(
        &self,
        arena: &Arena,
        group: NodeId,
        values: &[String],
    ) -> Result<bool> {
        let allowed = arena[group]
            .attribute(FRONTLOCANTSEXPECTED_ATR)
            .ok_or_else(|| {
                error("Group must have frontLocantsExpected to implement xylene-like nomenclature")
            })?;
        Ok(values
            .iter()
            .all(|value| allowed.split(',').any(|s| s == value)))
    }
    fn override_front_locants(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        parent: FragmentId,
        references: &mut [AtomReference],
        allow_one_implicit: bool,
    ) -> Result<bool> {
        let Some(locant) = arena
            .previous_sibling(group)
            .filter(|&n| arena[n].name == LOCANT_EL)
        else {
            return Ok(false);
        };
        let mut locants: Vec<_> = arena.value(locant).split(',').map(str::to_owned).collect();
        if !(locants.len() == references.len()
            || (allow_one_implicit && locants.len() + 1 == references.len()))
            || !self.acceptable_front_locants(arena, group, &locants)?
        {
            return Ok(false);
        }
        if locants.len() != references.len() {
            let atom = references[0].resolve(self.state, arena, group, parent, &mut false)?;
            if self
                .state
                .fragment_manager
                .graph
                .atom(atom)
                .locants
                .first()
                .map(String::as_str)
                != Some("1")
            {
                return Ok(false);
            }
        }
        for reference in references.iter_mut().rev() {
            let Some(locant) = locants.pop() else {
                break;
            };
            *reference = AtomReference {
                kind: "locant".into(),
                reference: locant,
            };
        }
        arena[group].remove_attribute(FRONTLOCANTSEXPECTED_ATR);
        arena.detach(locant);
        Ok(true)
    }
    fn process_xylene_like_nomenclature(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        parent: FragmentId,
    ) -> Result<()> {
        let mut ambiguous = false;
        if let Some(instructions) = arena[group].attribute(ADDGROUP_ATR).map(str::to_owned) {
            let (mut fragments, mut references) = (Vec::new(), Vec::new());
            for instruction in instructions.split(';') {
                let description: Vec<_> = instruction.split(' ').collect();
                if !(3..=4).contains(&description.len()) {
                    return Err(error("malformed addGroup tag"));
                }
                let fragment = self
                    .state
                    .fragment_manager
                    .build_token_smiles(
                        description[0],
                        arena,
                        group,
                        description.get(3).copied().unwrap_or(NONE_LABELS_VAL),
                    )
                    .map_err(|e| error(e.to_string()))?;
                fragments.push(fragment);
                references.push(AtomReference {
                    kind: description[1].into(),
                    reference: description[2].into(),
                });
            }
            self.override_front_locants(arena, group, parent, &mut references, true)?;
            for (new, reference) in fragments.into_iter().zip(references) {
                let parent_atom =
                    reference.resolve(self.state, arena, group, parent, &mut ambiguous)?;
                if self
                    .state
                    .fragment_manager
                    .graph
                    .fragment(new)
                    .out_atoms
                    .len()
                    > 1
                {
                    return Err(error("too many outAtoms on group to be added"));
                }
                let (new_atom, order) = if !self
                    .state
                    .fragment_manager
                    .graph
                    .fragment(new)
                    .out_atoms
                    .is_empty()
                {
                    let out = self.state.fragment_manager.graph.remove_out_atom(new, 0);
                    (
                        out.atom,
                        u8::try_from(out.valency)
                            .map_err(|_| error("Malformed outAtom valency"))?,
                    )
                } else {
                    (
                        self.state
                            .fragment_manager
                            .graph
                            .fragment(new)
                            .default_in_atom
                            .unwrap_or(first_atom(self.state, new)?),
                        1,
                    )
                };
                self.state
                    .fragment_manager
                    .create_bond(new_atom, parent_atom, order)
                    .map_err(|e| error(e.to_string()))?;
                self.state
                    .fragment_manager
                    .incorporate_fragment(new, parent)
                    .map_err(|e| error(e.to_string()))?;
            }
        }
        if let Some(instructions) = arena[group].attribute(ADDHETEROATOM_ATR).map(str::to_owned) {
            let (mut smiles, mut references) = (Vec::new(), Vec::new());
            for instruction in instructions.split(';') {
                let d: Vec<_> = instruction.split(' ').collect();
                if d.len() != 3 {
                    return Err(error("malformed addHeteroAtom tag"));
                }
                smiles.push(d[0]);
                references.push(AtomReference {
                    kind: d[1].into(),
                    reference: d[2].into(),
                });
            }
            self.override_front_locants(arena, group, parent, &mut references, false)?;
            for (smiles, reference) in smiles.into_iter().zip(references) {
                let atom = reference.resolve(self.state, arena, group, parent, &mut ambiguous)?;
                self.state
                    .fragment_manager
                    .replace_atom_with_smiles(atom, smiles)
                    .map_err(|e| error(e.to_string()))?;
            }
        }
        if arena[group].attribute(SUBTYPE_ATR) != Some(HANTZSCHWIDMAN_SUBTYPE_VAL)
            && let Some(instructions) = arena[group].attribute(ADDBOND_ATR).map(str::to_owned)
        {
            let (mut orders, mut references) = (Vec::new(), Vec::new());
            for instruction in instructions.split(';') {
                let d: Vec<_> = instruction.split(' ').collect();
                if d.len() != 3 {
                    return Err(error("malformed addBond tag"));
                }
                orders.push(
                    d[0].parse::<u8>()
                        .map_err(|_| error("malformed addBond tag"))?,
                );
                references.push(AtomReference {
                    kind: d[1].into(),
                    reference: d[2].into(),
                });
            }
            let locanted =
                self.override_front_locants(arena, group, parent, &mut references, false)?;
            for (order, reference) in orders.into_iter().zip(references) {
                let atom = reference.resolve(self.state, arena, group, parent, &mut ambiguous)?;
                let bond = crate::fragment_tools::unsaturate(
                    &mut self.state.fragment_manager.graph,
                    atom,
                    order,
                    parent,
                )
                .map_err(|e| error(e.to_string()))?;
                let graph = &mut self.state.fragment_manager.graph;
                let from = graph.bond(bond).from;
                let to = graph.bond(bond).to;
                if !locanted
                    && graph.bond(bond).order == 2
                    && graph.fragment(parent).atoms.len() == 5
                    && graph.atom(from).in_cycle
                    && graph.atom(to).in_cycle
                {
                    graph.bond_mut(bond).order = 1;
                    graph.atom_mut(from).spare_valency = true;
                    graph.atom_mut(to).spare_valency = true;
                }
            }
        }
        if ambiguous {
            self.state.add_is_ambiguous(format!(
                "{} describes multiple structures",
                arena.value(group)
            ));
        }
        Ok(())
    }

    fn apply_traditional_alkane_numbering(
        &mut self,
        arena: &Arena,
        group: NodeId,
        fragment: FragmentId,
    ) -> Result<()> {
        let names = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"];
        let graph = &mut self.state.fragment_manager.graph;
        if arena[group].attribute(TYPE_ATR) == Some(ACIDSTEM_TYPE_VAL) {
            let mut starting = graph.fragment(fragment).atoms[0];
            if let Some(instructions) = arena[group].attribute(SUFFIXAPPLIESTO_ATR) {
                let ids: Vec<_> = instructions.split(',').collect();
                if ids.len() != 1 {
                    return Ok(());
                }
                starting = *graph
                    .fragment(fragment)
                    .atoms
                    .get(
                        parse_num(ids[0])?
                            .checked_sub(1)
                            .ok_or_else(|| error("Malformed suffixAppliesTo"))?,
                    )
                    .ok_or_else(|| error("Malformed suffixAppliesTo"))?;
            }
            let mut neighbours: Vec<_> = graph
                .neighbours(starting)
                .into_iter()
                .filter(|&a| graph.atom(a).element == ChemEl::C)
                .collect();
            let mut previous = starting;
            let mut counter = 0;
            while neighbours.len() == 1 && counter < names.len() {
                let next = neighbours[0];
                if graph.atom(next).in_cycle {
                    break;
                }
                if !graph.atom(next).locants.iter().any(|s| s == names[counter]) {
                    graph.add_locant(next, names[counter]);
                }
                neighbours = graph
                    .neighbours(next)
                    .into_iter()
                    .filter(|&a| a != previous && graph.atom(a).element == ChemEl::C)
                    .collect();
                previous = next;
                counter += 1;
            }
        } else if arena[group].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL)
            && arena[group].attribute(SUBTYPE_ATR) == Some(ALKANESTEM_SUBTYPE_VAL)
        {
            if graph.fragment(fragment).atoms.len() == 1 {
                return Ok(());
            }
            let terminal = arena.next_sibling_named(group, SUFFIX_EL).is_some_and(|s| {
                arena[s].attribute(SUBTYPE_ATR) == Some(TERMINAL_SUBTYPE_VAL)
                    && arena[s].attribute(SUFFIXPREFIX_ATR).is_none()
            });
            for atom in graph.fragment(fragment).atoms.clone() {
                if graph.atom(atom).in_cycle {
                    continue;
                }
                let Some(locant) = graph.atom(atom).locants.first() else {
                    continue;
                };
                if locant.len() != 1 {
                    continue;
                }
                let Ok(number) = locant.parse::<usize>() else {
                    continue;
                };
                let offset = if terminal { 2 } else { 1 };
                if number >= offset && number < offset + names.len() {
                    graph.add_locant(atom, names[number - offset]);
                }
            }
        }
        Ok(())
    }

    pub fn process_charge_and_oxidation_number_specification(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        fragment: FragmentId,
    ) -> Result<()> {
        let first = first_atom(self.state, fragment)?;
        if arena[group].attribute(SUBTYPE_ATR) == Some(OUSICATOM_SUBTYPE_VAL) {
            let states = arena[group]
                .attribute(COMMONOXIDATIONSTATESANDMAX_ATR)
                .ok_or_else(|| {
                    error(format!(
                        "{COMMONOXIDATIONSTATESANDMAX_ATR} should be specified on: {}",
                        arena.value(group)
                    ))
                })?;
            self.state
                .fragment_manager
                .graph
                .atom_mut(first)
                .properties
                .oxidation_number = Some(
                states
                    .split(':')
                    .next()
                    .unwrap()
                    .parse()
                    .map_err(|_| error("Malformed oxidation state"))?,
            );
        }
        if let Some(next) = arena.next_sibling(group)
            && matches!(
                arena[next].name.as_str(),
                CHARGESPECIFIER_EL | OXIDATIONNUMBERSPECIFIER_EL
            )
        {
            let value = arena[next]
                .attribute(VALUE_ATR)
                .unwrap_or("")
                .parse()
                .map_err(|_| error("Malformed charge or oxidation number"))?;
            if arena[next].name == CHARGESPECIFIER_EL {
                self.state.fragment_manager.graph.atom_mut(first).charge = value;
            } else {
                self.state
                    .fragment_manager
                    .graph
                    .atom_mut(first)
                    .properties
                    .oxidation_number = Some(value);
            }
            arena.detach(next);
        }
        Ok(())
    }

    pub fn process_multipliers(&mut self, arena: &mut Arena, sub_or_root: NodeId) -> Result<()> {
        for multiplier in arena.children_named(sub_or_root, MULTIPLIER_EL) {
            let possible_locant = arena.previous_sibling(multiplier);
            let locants = possible_locant.and_then(|id| {
                if arena[id].name == LOCANT_EL {
                    Some(
                        arena
                            .value(id)
                            .split(',')
                            .map(str::to_owned)
                            .collect::<Vec<_>>(),
                    )
                } else if arena[id].name == COLONORSEMICOLONDELIMITEDLOCANT_EL {
                    Some(
                        arena
                            .value(id)
                            .trim_end_matches('-')
                            .split(':')
                            .map(str::to_owned)
                            .collect(),
                    )
                } else {
                    None
                }
            });
            let Some(feature) = arena.next_sibling(multiplier) else {
                continue;
            };
            if matches!(
                arena[feature].name.as_str(),
                UNSATURATOR_EL | SUFFIX_EL | SUBTRACTIVEPREFIX_EL | HYDRO_EL
            ) || (arena[feature].name == HETEROATOM_EL
                && arena[multiplier].attribute(TYPE_ATR) != Some(GROUP_TYPE_VAL))
            {
                let count = parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
                if count > 1 {
                    arena[feature].set_attribute(MULTIPLIED_ATR, "multiplied");
                }
                for i in (1..count).rev() {
                    let copy = arena.copy(feature);
                    if let Some(locants) = locants.as_ref().filter(|l| l.len() == count) {
                        arena[copy].set_attribute(LOCANT_ATR, &locants[i]);
                    }
                    arena.insert_after(feature, copy);
                }
                arena.detach(multiplier);
                if let Some(locants) = locants.filter(|l| l.len() == count) {
                    arena[feature].set_attribute(LOCANT_ATR, &locants[0]);
                    if let Some(locant) = possible_locant {
                        arena.detach(locant);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn assign_single_locants_to_adjacent_features(
        &mut self,
        arena: &mut Arena,
        locants: &[NodeId],
    ) {
        for &locant in locants {
            let value = arena.value(locant);
            if value.split(',').count() != 1 {
                continue;
            }
            let Some(mut referent) = arena.next_sibling(locant) else {
                continue;
            };
            if arena[referent].name == ISOTOPESPECIFICATION_EL {
                let Some(next) = arena.next_sibling(referent) else {
                    return;
                };
                referent = next;
            }
            if arena[referent].attribute(LOCANT_ATR).is_none()
                && arena[referent].attribute(MULTIPLIED_ATR).is_none()
                && (matches!(
                    arena[referent].name.as_str(),
                    UNSATURATOR_EL
                        | SUFFIX_EL
                        | HETEROATOM_EL
                        | CONJUNCTIVESUFFIXGROUP_EL
                        | SUBTRACTIVEPREFIX_EL
                ) || (arena[referent].name == HYDRO_EL
                    && !arena.value(referent).starts_with("per")))
            {
                arena[referent].set_attribute(LOCANT_ATR, value);
                arena.detach(locant);
            }
        }
    }

    fn next_non_charge_suffix(arena: &Arena, starting: NodeId) -> Option<NodeId> {
        let mut next = arena.next_sibling_named(starting, SUFFIX_EL);
        while let Some(suffix) = next {
            if arena[suffix].attribute(TYPE_ATR) != Some(CHARGE_TYPE_VAL) {
                return Some(suffix);
            }
            next = arena.next_sibling_named(suffix, SUFFIX_EL);
        }
        None
    }
    fn absolute_id(&self, fragment: FragmentId, relative: &str) -> Result<String> {
        Ok((relative_atom(self.state, fragment, relative)?.0 + 1).to_string())
    }
    pub fn process_suffix_applies_to(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        suffixes: &mut Vec<NodeId>,
        fragment: FragmentId,
    ) -> Result<()> {
        let Some(suffix) = Self::next_non_charge_suffix(arena, group) else {
            return if arena[group].attribute(TYPE_ATR) == Some(ACIDSTEM_TYPE_VAL) {
                Err(error("No suffix where suffix was expected"))
            } else {
                Ok(())
            };
        };
        if suffixes.len() > 1 && arena[group].attribute(TYPE_ATR) == Some(ACIDSTEM_TYPE_VAL) {
            return Err(error(
                "More than one suffix detected on trivial polyAcid. Not believed to be allowed",
            ));
        }
        let instructions = attr(arena, group, SUFFIXAPPLIESTO_ATR);
        let instructions: Vec<_> = instructions.split(',').collect();
        if arena[suffix].attribute(SUBTYPE_ATR) == Some(CYCLEFORMER_SUBTYPE_VAL) {
            if instructions.len() != 2 {
                return Err(error(format!(
                    "suffix: {} used on an inappropriate group",
                    arena.value(suffix)
                )));
            }
            let ids = instructions
                .iter()
                .map(|i| self.absolute_id(fragment, i))
                .collect::<Result<Vec<_>>>()?;
            arena[suffix].set_attribute(LOCANTID_ATR, ids.join(","));
            return Ok(());
        }
        let symmetric = arena[suffix].attribute(ADDITIONALVALUE_ATR).is_none();
        if !symmetric && instructions.len() < 2 {
            return Err(error(format!(
                "suffix: {} used on an inappropriate group",
                arena.value(suffix)
            )));
        }
        if arena[suffix].attribute(LOCANT_ATR).is_none() {
            arena[suffix].set_attribute(LOCANTID_ATR, self.absolute_id(fragment, instructions[0])?);
        }
        for instruction in instructions.iter().skip(1) {
            let copy = arena.token(SUFFIX_EL, "");
            if symmetric {
                for name in [VALUE_ATR, TYPE_ATR, SUBTYPE_ATR] {
                    if let Some(value) = arena[suffix].attribute(name).map(str::to_owned) {
                        arena[copy].set_attribute(name, value);
                    }
                }
                if let Some(infix) = arena[suffix]
                    .attribute(INFIX_ATR)
                    .filter(|s| s.starts_with('='))
                    .map(str::to_owned)
                {
                    arena[copy].set_attribute(INFIX_ATR, infix);
                }
            } else {
                let additional = attr(arena, suffix, ADDITIONALVALUE_ATR);
                arena[copy].set_attribute(VALUE_ATR, additional);
                arena[copy].set_attribute(TYPE_ATR, ROOT_EL);
            }
            arena[copy].set_attribute(LOCANTID_ATR, self.absolute_id(fragment, instruction)?);
            arena.insert_after(suffix, copy);
            suffixes.push(copy);
        }
        Ok(())
    }
    fn apply_default_suffix_locants(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        fragment: FragmentId,
    ) -> Result<()> {
        if let Some(default) = arena[group]
            .attribute(SUFFIXAPPLIESTOBYDEFAULT_ATR)
            .map(str::to_owned)
        {
            let instructions: Vec<_> = default.split(',').collect();
            let mut suffixes = Vec::new();
            let mut next = Self::next_non_charge_suffix(arena, group);
            while let Some(suffix) = next {
                suffixes.push(suffix);
                next = Self::next_non_charge_suffix(arena, suffix);
            }
            if instructions.len() == suffixes.len() {
                for (suffix, instruction) in suffixes.into_iter().zip(instructions) {
                    arena[suffix].set_attribute(
                        DEFAULTLOCANTID_ATR,
                        self.absolute_id(fragment, instruction)?,
                    );
                }
            }
        }
        Ok(())
    }
    fn hydroxy_neighbours(&self, atom: AtomId) -> Vec<AtomId> {
        let graph = &self.state.fragment_manager.graph;
        graph
            .neighbours(atom)
            .into_iter()
            .filter(|&a| {
                graph.atom(a).element == ChemEl::O
                    && graph.atom(a).charge == 0
                    && graph.atom(a).bonds.len() == 1
                    && graph.bond(graph.bond_between(atom, a).unwrap()).order == 1
            })
            .collect()
    }
    fn add_functional_atoms_to_hydroxy_groups(&mut self, atom: AtomId) {
        for hydroxy in self.hydroxy_neighbours(atom) {
            let fragment = self.state.fragment_manager.graph.atom(hydroxy).fragment;
            self.state
                .fragment_manager
                .graph
                .fragment_mut(fragment)
                .functional_atoms
                .push(hydroxy);
        }
    }
    fn charge_hydroxy_groups(&mut self, atom: AtomId) {
        for hydroxy in self.hydroxy_neighbours(atom) {
            self.state.fragment_manager.graph.atom_mut(hydroxy).charge -= 1;
            self.state
                .fragment_manager
                .graph
                .atom_mut(hydroxy)
                .protons_explicitly_added_or_removed -= 1;
        }
    }
    fn convert_hydroxy_groups(&mut self, fragment: FragmentId, to_out_atom: bool) {
        for atom in self
            .state
            .fragment_manager
            .graph
            .fragment(fragment)
            .atoms
            .clone()
        {
            let graph = &self.state.fragment_manager.graph;
            let a = graph.atom(atom);
            if a.element == ChemEl::O
                && a.charge == 0
                && a.bonds.len() == 1
                && graph.bond(a.bonds[0]).order == 1
                && a.out_valency == 0
            {
                let neighbour = graph.neighbours(atom)[0];
                if graph.atom(neighbour).element == ChemEl::O {
                    continue;
                }
                self.state
                    .fragment_manager
                    .remove_atom_and_associated_bonds(atom);
                if to_out_atom {
                    self.state
                        .fragment_manager
                        .graph
                        .add_out_atom(fragment, neighbour, 1, true);
                } else {
                    self.state.fragment_manager.graph.atom_mut(neighbour).charge += 1;
                    self.state
                        .fragment_manager
                        .graph
                        .atom_mut(neighbour)
                        .protons_explicitly_added_or_removed -= 1;
                }
            }
        }
    }
    pub fn resolve_group_adding_suffixes(
        &mut self,
        arena: &mut Arena,
        suffixes: &[NodeId],
        fragment: FragmentId,
    ) -> Result<Vec<FragmentId>> {
        let mut fragments = Vec::new();
        let group_type = self
            .state
            .fragment_manager
            .graph
            .fragment(fragment)
            .fragment_type
            .clone();
        let sub_type = self
            .state
            .fragment_manager
            .graph
            .fragment(fragment)
            .sub_type
            .clone();
        let rule_type = if self
            .suffix_rules
            .is_group_type_with_specific_suffix_rules(&group_type)
        {
            &group_type
        } else {
            STANDARDGROUP_TYPE_VAL
        };
        for &suffix in suffixes {
            let locant = arena[suffix].attribute(LOCANT_ATR);
            let locant_id = arena[suffix].attribute(LOCANTID_ATR);
            let likely = if let Some(locant) = locant.filter(|s| !s.contains(',')) {
                self.state
                    .fragment_manager
                    .graph
                    .atom_by_locant(fragment, locant)
            } else if let Some(id) = locant_id.filter(|s| !s.contains(',')) {
                let atom = AtomId(
                    parse_num(id)?
                        .checked_sub(1)
                        .ok_or_else(|| error("Atom IDs start at one"))?,
                );
                if self
                    .state
                    .fragment_manager
                    .graph
                    .fragment(fragment)
                    .atoms
                    .contains(&atom)
                {
                    Some(atom)
                } else {
                    return Err(error(format!("Atom ID {id} not present in fragment")));
                }
            } else {
                None
            }
            .unwrap_or(first_atom(self.state, fragment)?);
            let cyclic = self.state.fragment_manager.graph.atom(likely).in_cycle;
            let rules = self
                .suffix_rules
                .rule_tags(
                    rule_type,
                    arena[suffix].attribute(VALUE_ATR).unwrap_or(""),
                    Some(&sub_type),
                )
                .map_err(|e| error(e.to_string()))?;
            let mut suffix_frag = None;
            for rule in rules {
                match rule.kind {
                    SuffixRuleType::AddGroup => {
                        let added = self
                            .state
                            .fragment_manager
                            .build_smiles(
                                rule.attribute("SMILES").unwrap_or(""),
                                SUFFIX_TYPE_VAL,
                                rule.attribute("labels").unwrap_or(NONE_LABELS_VAL),
                            )
                            .map_err(|e| error(e.to_string()))?;
                        let atoms = self
                            .state
                            .fragment_manager
                            .graph
                            .fragment(added)
                            .atoms
                            .clone();
                        if let Some(ids) = rule.attribute(SUFFIXRULES_FUNCTIONALIDS_ATR) {
                            for id in ids.split(',') {
                                let atom = *atoms.get(parse_num(id)?.checked_sub(1).ok_or_else(|| error("Malformed functionalIds"))?).ok_or_else(|| error("Check suffixRules.xml: Atom requested to have a functionalAtom was not within the suffix fragment"))?;
                                self.state
                                    .fragment_manager
                                    .graph
                                    .fragment_mut(added)
                                    .functional_atoms
                                    .push(atom);
                            }
                        }
                        if let Some(ids) = rule.attribute(SUFFIXRULES_OUTIDS_ATR) {
                            for id in ids.split(',') {
                                let atom = *atoms.get(parse_num(id)?.checked_sub(1).ok_or_else(|| error("Malformed outIds"))?).ok_or_else(|| error("Check suffixRules.xml: Atom requested to have a outAtom was not within the suffix fragment"))?;
                                self.state
                                    .fragment_manager
                                    .graph
                                    .add_out_atom(added, atom, 1, true);
                            }
                        }
                        suffix_frag = Some(added);
                    }
                    SuffixRuleType::AddSuffixPrefixIfNonePresentAndCyclic => {
                        if cyclic && arena[suffix].attribute(SUFFIXPREFIX_ATR).is_none() {
                            arena[suffix].set_attribute(
                                SUFFIXPREFIX_ATR,
                                rule.attribute("SMILES").unwrap_or(""),
                            );
                        }
                    }
                    SuffixRuleType::AddFunctionalAtomsToHydroxyGroups => {
                        if suffix_frag.is_some() {
                            return Err(error(
                                "addFunctionalAtomsToHydroxyGroups is not currently compatable with the addGroup suffix rule",
                            ));
                        }
                        self.add_functional_atoms_to_hydroxy_groups(likely);
                    }
                    SuffixRuleType::ChargeHydroxyGroups => {
                        if suffix_frag.is_some() {
                            return Err(error(
                                "chargeHydroxyGroups is not currently compatable with the addGroup suffix rule",
                            ));
                        }
                        self.charge_hydroxy_groups(likely);
                    }
                    SuffixRuleType::RemoveTerminalOxygen => {
                        if suffix_frag.is_some() {
                            return Err(error(
                                "removeTerminalOxygen is not currently compatible with the addGroup suffix rule",
                            ));
                        }
                        let order = rule
                            .attribute("order")
                            .unwrap_or("")
                            .parse()
                            .map_err(|_| error("Malformed oxygen removal order"))?;
                        self.state
                            .fragment_manager
                            .remove_terminal_oxygen(likely, order)
                            .map_err(|e| error(e.to_string()))?;
                    }
                    _ => {}
                }
            }
            if let Some(added) = suffix_frag {
                fragments.push(added);
                arena[suffix].fragment = Some(added);
            }
        }
        Ok(fragments)
    }
    fn process_suffix_prefixes(&mut self, arena: &Arena, suffixes: &[NodeId]) -> Result<()> {
        for &suffix in suffixes {
            if let Some(smiles) = arena[suffix].attribute(SUFFIXPREFIX_ATR) {
                let prefix = self
                    .state
                    .fragment_manager
                    .build_smiles(smiles, SUFFIX_TYPE_VAL, NONE_LABELS_VAL)
                    .map_err(|e| error(e.to_string()))?;
                let prefix_atom = first_atom(self.state, prefix)?;
                self.add_functional_atoms_to_hydroxy_groups(prefix_atom);
                if arena.value(suffix).ends_with("ate") || arena.value(suffix).ends_with("at") {
                    self.charge_hydroxy_groups(prefix_atom);
                }
                self.state
                    .fragment_manager
                    .graph
                    .add_locant(prefix_atom, "X");
                let suffix_frag = frag(arena, suffix)?;
                self.state
                    .fragment_manager
                    .incorporate_fragment(prefix, suffix_frag)
                    .map_err(|e| error(e.to_string()))?;
                let r = first_atom(self.state, suffix_frag)?;
                for neighbour in self.state.fragment_manager.graph.neighbours(r) {
                    let bond = self
                        .state
                        .fragment_manager
                        .graph
                        .bond_between(r, neighbour)
                        .unwrap();
                    let order = self.state.fragment_manager.graph.bond(bond).order;
                    self.state.fragment_manager.remove_bond(bond);
                    self.state
                        .fragment_manager
                        .create_bond(neighbour, prefix_atom, order)
                        .map_err(|e| error(e.to_string()))?;
                }
                self.state
                    .fragment_manager
                    .create_bond(prefix_atom, r, 1)
                    .map_err(|e| error(e.to_string()))?;
            }
        }
        Ok(())
    }
    fn process_removal_of_hydroxy_groups_rules(
        &mut self,
        arena: &Arena,
        suffixes: &[NodeId],
        fragment: FragmentId,
    ) -> Result<()> {
        let group = self.state.fragment_manager.graph.fragment(fragment);
        let group_type = group.fragment_type.clone();
        let sub_type = group.sub_type.clone();
        let rule_type = if self
            .suffix_rules
            .is_group_type_with_specific_suffix_rules(&group_type)
        {
            &group_type
        } else {
            STANDARDGROUP_TYPE_VAL
        };
        for &suffix in suffixes {
            for rule in self
                .suffix_rules
                .rule_tags(
                    rule_type,
                    arena[suffix].attribute(VALUE_ATR).unwrap_or(""),
                    Some(&sub_type),
                )
                .map_err(|e| error(e.to_string()))?
            {
                match rule.kind {
                    SuffixRuleType::ConvertHydroxyGroupsToOutAtoms => {
                        self.convert_hydroxy_groups(fragment, true)
                    }
                    SuffixRuleType::ConvertHydroxyGroupsToPositiveCharge => {
                        self.convert_hydroxy_groups(fragment, false)
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    pub fn preliminary_process_suffixes(
        &mut self,
        arena: &mut Arena,
        group: NodeId,
        suffixes: &mut Vec<NodeId>,
    ) -> Result<()> {
        let fragment = frag(arena, group)?;
        if arena[group].attribute(SUFFIXAPPLIESTO_ATR).is_some() {
            self.process_suffix_applies_to(arena, group, suffixes, fragment)?;
        } else {
            for &suffix in suffixes.iter() {
                if arena[suffix].attribute(ADDITIONALVALUE_ATR).is_some() {
                    return Err(error(format!(
                        "suffix: {} used on an inappropriate group",
                        arena.value(suffix)
                    )));
                }
            }
        }
        self.apply_default_suffix_locants(arena, group, fragment)?;
        let mut fragments = self.resolve_group_adding_suffixes(arena, suffixes, fragment)?;
        self.state.xml_suffix_map.insert(group, fragments.clone());
        let mut resolved = false;
        if arena[group].attribute(TYPE_ATR) == Some(CHALCOGENACIDSTEM_TYPE_VAL) {
            SuffixApplier::new(self.state, self.suffix_rules)
                .resolve_suffixes(arena, group, suffixes)
                .map_err(|e| error(e.to_string()))?;
            // Java stores this same list in xmlSuffixMap, so resolveSuffixes
            // clears both views before infix replacement on the merged stem.
            fragments.clear();
            resolved = true;
        }
        self.process_suffix_prefixes(arena, suffixes)?;
        functional_replacement::process_infix_functional_replacement_nomenclature(
            self.state,
            arena,
            suffixes,
            &mut fragments,
        )?;
        self.state.xml_suffix_map.insert(group, fragments);
        self.process_removal_of_hydroxy_groups_rules(arena, suffixes, fragment)?;
        if arena.value(group) == "oxal" {
            SuffixApplier::new(self.state, self.suffix_rules)
                .resolve_suffixes(arena, group, suffixes)
                .map_err(|e| error(e.to_string()))?;
            arena[group].set_attribute(TYPE_ATR, NONCARBOXYLICACID_TYPE_VAL);
            resolved = true;
        }
        if resolved {
            for suffix in suffixes.drain(..).rev() {
                arena.detach(suffix);
            }
        }
        if let Some(count) = arena[group].attribute(NUMBEROFFUNCTIONALATOMSTOREMOVE_ATR) {
            let count = parse_num(count)?;
            if count
                > self
                    .state
                    .fragment_manager
                    .graph
                    .fragment(fragment)
                    .functional_atoms
                    .len()
            {
                return Err(error(
                    "Too many hydrogen for the number of positions on non carboxylic acid",
                ));
            }
            for _ in 0..count {
                let atom = self
                    .state
                    .fragment_manager
                    .graph
                    .fragment_mut(fragment)
                    .functional_atoms
                    .remove(0);
                self.state.fragment_manager.graph.atom_mut(atom).charge = 0;
                self.state
                    .fragment_manager
                    .graph
                    .atom_mut(atom)
                    .protons_explicitly_added_or_removed = 0;
            }
        }
        Ok(())
    }

    pub fn apply_dl_stereochemistry_to_amino_acid(
        &mut self,
        arena: &Arena,
        amino_acid: NodeId,
        value: &str,
    ) -> Result<bool> {
        let atoms: Vec<_> = self
            .state
            .fragment_manager
            .graph
            .fragment(frag(arena, amino_acid)?)
            .atoms
            .iter()
            .copied()
            .filter(|&a| self.state.fragment_manager.graph.atom(a).parity.is_some())
            .collect();
        if atoms.is_empty() {
            return Ok(false);
        }
        if value == "dl" {
            self.state.racemic_group_count += 1;
            for atom in atoms {
                self.state
                    .fragment_manager
                    .graph
                    .atom_mut(atom)
                    .parity
                    .as_mut()
                    .unwrap()
                    .stereo_group = StereoGroup {
                    kind: StereoGroupType::Racemic,
                    number: self.state.racemic_group_count,
                };
            }
        } else {
            let mut invert = match value {
                "l" | "ls" => false,
                "d" | "ds" => true,
                _ => {
                    return Err(error(format!(
                        "OPSIN bug: Unexpected value for D/L stereochemistry found before amino acid: {value}"
                    )));
                }
            };
            if arena[amino_acid].attribute(NATURALENTISOPPOSITE_ATR) == Some("yes") {
                invert = !invert;
            }
            if invert {
                for atom in atoms {
                    let parity = self
                        .state
                        .fragment_manager
                        .graph
                        .atom_mut(atom)
                        .parity
                        .as_mut()
                        .unwrap();
                    parity.parity = -parity.parity;
                }
            }
        }
        Ok(true)
    }
    pub fn apply_dl_stereochemistry_to_carbohydrate(
        &mut self,
        arena: &Arena,
        carbohydrate: NodeId,
        value: &str,
    ) -> Result<()> {
        let atoms: Vec<_> = self
            .state
            .fragment_manager
            .graph
            .fragment(frag(arena, carbohydrate)?)
            .atoms
            .iter()
            .copied()
            .filter(|&a| self.state.fragment_manager.graph.atom(a).parity.is_some())
            .collect();
        if atoms.is_empty() {
            return Err(error(format!(
                "D/L stereochemistry :{value} found before achiral carbohydrate"
            )));
        }
        let (mut invert, kind, number) = match value {
            "dl" => {
                self.state.racemic_group_count += 1;
                (
                    false,
                    StereoGroupType::Racemic,
                    self.state.racemic_group_count,
                )
            }
            "d" | "dg" => (false, StereoGroupType::Absolute, 0),
            "l" | "lg" => (true, StereoGroupType::Absolute, 0),
            _ => {
                return Err(error(format!(
                    "Unexpected value for D/L stereochemistry found before carbohydrate: {value}"
                )));
            }
        };
        if arena[carbohydrate].attribute(NATURALENTISOPPOSITE_ATR) == Some("yes") {
            invert = !invert;
        }
        if invert || kind != StereoGroupType::Absolute {
            for atom in atoms {
                let parity = self
                    .state
                    .fragment_manager
                    .graph
                    .atom_mut(atom)
                    .parity
                    .as_mut()
                    .unwrap();
                if invert {
                    parity.parity = -parity.parity;
                }
                parity.stereo_group = StereoGroup { kind, number };
            }
        }
        Ok(())
    }
    pub fn apply_dl_stereochemistry_to_carbohydrate_configurational_prefix(
        arena: &mut Arena,
        prefix: NodeId,
        value: &str,
    ) -> Result<()> {
        match value {
            "d" | "dg" => {}
            "l" | "lg" => {
                let values = attr(arena, prefix, VALUE_ATR);
                let flipped = values
                    .split('/')
                    .map(|v| match v {
                        "r" => Ok("l"),
                        "l" => Ok("r"),
                        _ => Err(error(format!(
                            "OPSIN Bug: Invalid carbohydrate prefix value: {values}"
                        ))),
                    })
                    .collect::<Result<Vec<_>>>()?;
                arena[prefix].set_attribute(VALUE_ATR, flipped.join("/"));
            }
            "dl" => {
                let count = attr(arena, prefix, VALUE_ATR)
                    .trim_end_matches('/')
                    .split('/')
                    .count();
                arena[prefix].set_attribute(VALUE_ATR, vec!["?"; count].join("/"));
            }
            _ => {
                return Err(error(format!(
                    "Unexpected value for D/L stereochemistry found before carbohydrate prefix: {value}"
                )));
            }
        }
        Ok(())
    }
    pub fn apply_dl_prefixes(&mut self, arena: &mut Arena, sub_or_root: NodeId) -> Result<()> {
        for stereo in arena.children_with_attribute(
            sub_or_root,
            STEREOCHEMISTRY_EL,
            TYPE_ATR,
            DLSTEREOCHEMISTRY_TYPE_VAL,
        ) {
            let value = attr(arena, stereo, VALUE_ATR);
            let Some(mut target) = arena.next_sibling(stereo) else {
                continue;
            };
            if arena[target].attribute(TYPE_ATR) == Some(OPTICALROTATION_TYPE_VAL) {
                let Some(next) = arena.next_sibling(target) else {
                    continue;
                };
                target = next;
            }
            match arena[target].attribute(TYPE_ATR) {
                Some(AMINOACID_TYPE_VAL) => {
                    if !self.apply_dl_stereochemistry_to_amino_acid(arena, target, &value)? {
                        continue;
                    }
                }
                Some(CARBOHYDRATE_TYPE_VAL) => {
                    self.apply_dl_stereochemistry_to_carbohydrate(arena, target, &value)?
                }
                Some(CARBOHYDRATECONFIGURATIONPREFIX_TYPE_VAL) => {
                    Self::apply_dl_stereochemistry_to_carbohydrate_configurational_prefix(
                        arena, target, &value,
                    )?
                }
                _ => continue,
            }
            arena.detach(stereo);
        }
        Ok(())
    }

    fn contains_cyclic_atoms(&self, arena: &Arena, group: NodeId) -> Result<bool> {
        Ok(self
            .state
            .fragment_manager
            .graph
            .fragment(frag(arena, group)?)
            .atoms
            .iter()
            .any(|&a| self.state.fragment_manager.graph.atom(a).in_cycle))
    }
    fn previous_locant(arena: &Arena, group: NodeId) -> Option<NodeId> {
        arena.previous_sibling_named(group, LOCANT_EL)
    }
    fn move_detachable_prefix(
        arena: &mut Arena,
        substituent: NodeId,
        adjacent: NodeId,
        target: NodeId,
        allowed: &[&str],
        prefix_name: &str,
    ) -> Result<()> {
        let mut in_prefix = true;
        for child in arena[substituent].children.clone().into_iter().rev() {
            let name = arena[child].name.as_str();
            if name == HYPHEN_EL {
                continue;
            }
            if target == adjacent || (in_prefix && allowed.contains(&name)) {
                arena.detach(child);
                arena.insert_child(target, child, 0);
            } else if name == STEREOCHEMISTRY_EL {
                in_prefix = false;
                arena.detach(child);
                arena.insert_child(adjacent, child, 0);
            } else {
                return Err(error(format!(
                    "Unexpected term found before detachable {prefix_name}: {}",
                    arena.value(child)
                )));
            }
        }
        arena.detach(substituent);
        Ok(())
    }
    /// Tree-only subtractive prefix relocation; biochemical parents take precedence.
    pub fn remove_and_move_to_appropriate_group_if_subtractive_prefix(
        arena: &mut Arena,
        substituent: NodeId,
    ) -> Result<bool> {
        let prefixes = arena.children_named(substituent, SUBTRACTIVEPREFIX_EL);
        if prefixes.is_empty() {
            return Ok(false);
        }
        let failure = || {
            error(format!(
                "Unable to find group for: {} to apply to!",
                arena.value(prefixes[0])
            ))
        };
        let adjacent = arena.next_sibling(substituent).ok_or_else(failure)?;
        let (mut biochemical, mut standard) = (None, None);
        let mut next = Some(adjacent);
        while let Some(id) = next {
            if let Some(group) = arena.first_child_named(id, GROUP_EL) {
                if matches!(
                    arena[group].attribute(TYPE_ATR),
                    Some(CARBOHYDRATE_TYPE_VAL | AMINOACID_TYPE_VAL)
                ) || arena[group].attribute(SUBTYPE_ATR) == Some(BIOCHEMICAL_SUBTYPE_VAL)
                {
                    biochemical = Some(group);
                    if Self::previous_locant(arena, group).is_none() {
                        break;
                    }
                } else {
                    standard = Some(group);
                }
            }
            next = arena.next_sibling(id);
        }
        let group = biochemical.or(standard).ok_or_else(failure)?;
        Self::move_detachable_prefix(
            arena,
            substituent,
            adjacent,
            arena[group].parent.unwrap(),
            &[SUBTRACTIVEPREFIX_EL],
            "substractive prefix",
        )?;
        Ok(true)
    }
    pub fn remove_and_move_to_appropriate_group_if_ring_bridge(
        &mut self,
        arena: &mut Arena,
        substituent: NodeId,
    ) -> Result<bool> {
        let bridges = arena.children_named(substituent, FUSEDRINGBRIDGE_EL);
        if bridges.is_empty() {
            return Ok(false);
        }
        let failure = || {
            error(format!(
                "Unable to find group for: {} to apply to!",
                arena.value(bridges[0])
            ))
        };
        let adjacent = arena.next_sibling(substituent).ok_or_else(failure)?;
        let (mut target, mut standard, mut next) = (None, None, Some(adjacent));
        while let Some(id) = next {
            if let Some(group) = arena.first_child_named(id, GROUP_EL) {
                if self.contains_cyclic_atoms(arena, group)?
                    && Self::previous_locant(arena, group).is_none()
                {
                    target = Some(group);
                    break;
                }
                standard = Some(group);
            }
            next = arena.next_sibling(id);
        }
        let group = target.or(standard).ok_or_else(failure)?;
        Self::move_detachable_prefix(
            arena,
            substituent,
            adjacent,
            arena[group].parent.unwrap(),
            &[
                FUSEDRINGBRIDGE_EL,
                COLONORSEMICOLONDELIMITEDLOCANT_EL,
                LOCANT_EL,
            ],
            "ring bridge",
        )?;
        Ok(true)
    }
    pub fn remove_and_move_to_appropriate_group_if_hydro_substituent(
        &mut self,
        arena: &mut Arena,
        substituent: NodeId,
    ) -> Result<bool> {
        let hydros = arena.children_named(substituent, HYDRO_EL);
        if hydros.is_empty() {
            return Ok(false);
        }
        let adjacent = arena
            .next_sibling(substituent)
            .ok_or_else(|| error("Cannot find ring for hydro substituent to apply to"))?;
        let mut target = None;
        if let Some(group) = arena.first_child_named(adjacent, GROUP_EL)
            && self.contains_cyclic_atoms(arena, group)?
        {
            if arena.previous_sibling(hydros[0]).is_some_and(|l| {
                arena[l].name == LOCANT_EL && arena.value(l).split(',').count() == 1
            }) {
                target = Some(group);
            } else if let Some(locant) = Self::previous_locant(arena, group) {
                if arena[group]
                    .attribute(FRONTLOCANTSEXPECTED_ATR)
                    .is_some_and(|expected| expected.split(',').any(|s| s == arena.value(locant)))
                {
                    target = Some(group);
                }
                if arena[group].attribute(SUBTYPE_ATR) == Some(FUSIONRING_SUBTYPE_VAL)
                    && matches!(arena.value(group).as_str(), "benzo" | "benz")
                    && arena
                        .next_sibling(group)
                        .is_some_and(|n| arena[n].name != FUSION_EL)
                {
                    target = Some(group);
                }
            } else {
                target = Some(group);
            }
        }
        if target.is_none() {
            let parent = arena[substituent].parent.unwrap();
            for id in arena[parent].children.iter().rev().copied() {
                if id == substituent {
                    break;
                }
                if let Some(group) = arena.first_child_named(id, GROUP_EL)
                    && self.contains_cyclic_atoms(arena, group)?
                {
                    target = Some(group);
                    break;
                }
            }
        }
        let group =
            target.ok_or_else(|| error("Cannot find ring for hydro substituent to apply to"))?;
        Self::move_detachable_prefix(
            arena,
            substituent,
            adjacent,
            arena[group].parent.unwrap(),
            &[HYDRO_EL],
            "hydro prefix",
        )?;
        Ok(true)
    }
    pub fn match_locants_to_direct_features(
        &mut self,
        arena: &mut Arena,
        sub_or_root: NodeId,
    ) -> Result<()> {
        let mut locants = arena.children_named(sub_or_root, LOCANT_EL);
        for group in arena.children_named(sub_or_root, GROUP_EL) {
            if arena[group].attribute(SUBTYPE_ATR) != Some(HANTZSCHWIDMAN_SUBTYPE_VAL) {
                continue;
            }
            if arena[group].attribute(ADDBOND_ATR).is_some()
                && arena.children_named(sub_or_root, DELTA_EL).is_empty()
            {
                let delta = arena.token(DELTA_EL, "");
                if let Some(locant) = arena
                    .previous_sibling_ignoring(group, &[HETEROATOM_EL, MULTIPLIER_EL])
                    .filter(|&l| {
                        arena[l].name == LOCANT_EL && arena.value(l).split(',').count() == 1
                    })
                {
                    let value = arena.value(locant);
                    arena[delta].set_value(value);
                    arena.insert_before(locant, delta);
                    arena.detach(locant);
                    locants.retain(|&l| l != locant);
                } else {
                    arena.insert_child(sub_or_root, delta, 0);
                }
            }
            if locants.is_empty() {
                continue;
            }
            let (mut before, mut hetero) = (None, Vec::new());
            let index = arena.index_of(sub_or_root, group).unwrap();
            for id in arena[sub_or_root].children[..index].iter().rev().copied() {
                if arena[id].name == LOCANT_EL {
                    before = Some(id);
                    break;
                } else if arena[id].name == HETEROATOM_EL {
                    hetero.push(id);
                    if arena[id].attribute(LOCANT_ATR).is_some() {
                        break;
                    }
                } else {
                    break;
                }
            }
            hetero.reverse();
            if let Some(before) = before {
                let values: Vec<_> = arena.value(before).split(',').map(str::to_owned).collect();
                if values.len() == 1
                    && self
                        .state
                        .fragment_manager
                        .graph
                        .fragment(frag(arena, group)?)
                        .atoms
                        .len()
                        <= 10
                {
                    locants.retain(|&l| l != before);
                } else if values.len() == hetero.len() {
                    for (atom, value) in hetero.into_iter().zip(values) {
                        arena[atom].set_attribute(LOCANT_ATR, value);
                    }
                    arena.detach(before);
                    locants.retain(|&l| l != before);
                } else if hetero.len() > 1 {
                    return Err(error(
                        "Mismatch between number of locants and Hantzsch-Widman heteroatoms",
                    ));
                }
            }
        }
        self.assign_single_locants_to_adjacent_features(arena, &locants);
        Ok(())
    }

    pub fn check_locant_present_on_potential_root(
        state: &BuildState,
        arena: &Arena,
        starting: NodeId,
        locant: &str,
    ) -> Result<bool> {
        let mut found_sibling = false;
        for include_explicit in [false, true] {
            if include_explicit && found_sibling {
                break;
            }
            let (mut stack, mut first_iteration) = (vec![starting], true);
            while let Some(current) = stack.pop() {
                let Some(parent) = arena[current].parent else {
                    continue;
                };
                let index = arena.index_of(parent, current).unwrap();
                for (i, &sibling) in arena[parent].children.iter().enumerate() {
                    if !matches!(
                        arena[sibling].name.as_str(),
                        BRACKET_EL | SUBSTITUENT_EL | ROOT_EL
                    ) || (first_iteration && i <= index)
                    {
                        continue;
                    }
                    if arena[sibling].name == BRACKET_EL {
                        if !include_explicit && arena[sibling].attribute(TYPE_ATR).is_none() {
                            continue;
                        }
                        if let Some(&child) = arena[sibling].children.first() {
                            stack.push(child);
                        }
                    } else {
                        let group = arena
                            .first_child_named(sibling, GROUP_EL)
                            .ok_or_else(|| error("Substituent or root has no group"))?;
                        let fragment = frag(arena, group)?;
                        if state
                            .fragment_manager
                            .graph
                            .atom_by_locant(fragment, locant)
                            .is_some()
                        {
                            return Ok(true);
                        }
                        for &suffix in state.xml_suffix_map.get(&group).into_iter().flatten() {
                            if state
                                .fragment_manager
                                .graph
                                .atom_by_locant(suffix, locant)
                                .is_some()
                            {
                                return Ok(true);
                            }
                        }
                        for conjunctive in
                            arena.next_siblings_named(group, CONJUNCTIVESUFFIXGROUP_EL)
                        {
                            if state
                                .fragment_manager
                                .graph
                                .atom_by_locant(frag(arena, conjunctive)?, locant)
                                .is_some()
                            {
                                return Ok(true);
                            }
                        }
                    }
                    found_sibling = true;
                }
                first_iteration = false;
            }
        }
        Ok(false)
    }
    pub fn apply_lambda_convention(
        &mut self,
        arena: &mut Arena,
        sub_or_root: NodeId,
    ) -> Result<()> {
        for lambda in arena.children_named(sub_or_root, LAMBDACONVENTION_EL) {
            let group = arena
                .first_child_named(sub_or_root, GROUP_EL)
                .ok_or_else(|| error("Lambda convention has no group"))?;
            let fragment = frag(arena, group)?;
            let atom = if let Some(locant) = arena[lambda].attribute(LOCANT_ATR) {
                locanted_atom(self.state, fragment, locant)?
            } else {
                if self
                    .state
                    .fragment_manager
                    .graph
                    .fragment(fragment)
                    .atoms
                    .len()
                    != 1
                {
                    return Err(error(
                        "Ambiguous use of lambda convention. Fragment has more than 1 atom but no locant was specified for the lambda",
                    ));
                }
                first_atom(self.state, fragment)?
            };
            self.state
                .fragment_manager
                .graph
                .atom_mut(atom)
                .lambda_convention_valency = Some(
                arena[lambda]
                    .attribute(LAMBDA_ATR)
                    .unwrap_or("")
                    .parse()
                    .map_err(|_| error("Malformed lambda valency"))?,
            );
            arena.detach(lambda);
        }
        Ok(())
    }
    pub fn move_substituent_detachable_het_atom_repl(
        &mut self,
        arena: &mut Arena,
        substituent: NodeId,
    ) -> Result<()> {
        let mut replacements = Vec::new();
        for &child in &arena[substituent].children {
            if arena[child].name == HETEROATOM_EL && arena[child].attribute(LOCANT_ATR).is_some() {
                replacements.push(child);
            } else {
                break;
            }
        }
        if replacements.is_empty() {
            return Ok(());
        }
        let after = arena.next_sibling(*replacements.last().unwrap());
        if after.is_none_or(|n| arena[n].name != LOCANT_EL) {
            return Ok(());
        }
        let mut rightmost = None;
        let mut next = arena.next_sibling(substituent);
        while let Some(id) = next {
            if let Some(group) = arena.first_child_named(id, GROUP_EL) {
                rightmost = Some(group);
            }
            next = arena.next_sibling(id);
        }
        let group = rightmost.ok_or_else(|| {
            error(format!(
                "Unable to find group for: {} to apply to!",
                arena.value(replacements[0])
            ))
        })?;
        let parent = arena[group].parent.unwrap();
        for replacement in replacements.into_iter().rev() {
            arena.detach(replacement);
            arena.insert_child(parent, replacement, 0);
        }
        Ok(())
    }
    pub fn move_erroneously_positioned_locants_and_multipliers(
        &mut self,
        arena: &mut Arena,
        brackets: &[NodeId],
    ) -> Result<()> {
        for &bracket in brackets.iter().rev() {
            let children = arena[bracket].children.clone();
            let hyphen =
                children.len() == 2 && children.iter().any(|&n| arena[n].name == HYPHEN_EL);
            if children.len() != 1 && !hyphen {
                continue;
            }
            let content = arena[children[0]].children.clone();
            if content.len() < 2 {
                continue;
            }
            let locant = (arena[content[0]].name == LOCANT_EL).then_some(content[0]);
            let possible = content[usize::from(locant.is_some())];
            let multiplier = (arena[possible].name == MULTIPLIER_EL).then_some(possible);
            if let Some(locant) = locant {
                if multiplier.is_none_or(|m| {
                    arena.value(locant).split(',').count().to_string() == attr(arena, m, VALUE_ATR)
                }) {
                    arena.detach(locant);
                    arena.insert_before(children[0], locant);
                } else {
                    continue;
                }
            }
            if let Some(multiplier) = multiplier {
                arena.detach(multiplier);
                arena.insert_before(children[0], multiplier);
            }
        }
        Ok(())
    }
    pub fn add_implicit_brackets_when_first_substituent_has_two_multipliers(
        &mut self,
        arena: &mut Arena,
        substituent: NodeId,
        brackets: &mut Vec<NodeId>,
    ) {
        if arena[substituent].name != SUBSTITUENT_EL {
            return;
        }
        let multipliers: Vec<_> = arena[substituent]
            .children
            .iter()
            .copied()
            .take_while(|&n| arena[n].name == MULTIPLIER_EL)
            .collect();
        if multipliers.len() != 2 {
            return;
        }
        let parent = arena[substituent].parent.unwrap();
        let children = arena[parent].children.clone();
        let bracket = arena.grouping(BRACKET_EL);
        arena[bracket].set_attribute(TYPE_ATR, IMPLICIT_TYPE_VAL);
        arena.detach(multipliers[0]);
        arena.add_child(bracket, multipliers[0]);
        for child in children {
            arena.detach(child);
            arena.add_child(bracket, child);
        }
        arena.add_child(parent, bracket);
        brackets.push(bracket);
    }
    pub fn assign_locants_to_multiplied_root_if_present(
        &mut self,
        arena: &mut Arena,
        rightmost: NodeId,
    ) -> Result<()> {
        let multipliers = arena.children_named(rightmost, MULTIPLIER_EL);
        if multipliers.len() == 1 {
            let multiplier = multipliers[0];
            if arena.previous_element(multiplier, true).is_none() {
                return Err(error("OPSIN bug: Unacceptable input to function"));
            }
            let locants = arena.children_named(rightmost, MULTIPLICATIVELOCANT_EL);
            if locants.len() > 1 {
                return Err(error(
                    "OPSIN bug: Only none or one multiplicative locant expected",
                ));
            }
            let count = parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
            if let Some(&locant) = locants.first() {
                let value = arena.value(locant);
                if value.split(',').count() != count {
                    return Err(error(
                        "Mismatch between number of locants and number of roots",
                    ));
                }
                arena[rightmost].set_attribute(INLOCANTS_ATR, value);
                arena.detach(locant);
            } else {
                arena[rightmost].set_attribute(INLOCANTS_ATR, INLOCANTS_DEFAULT);
            }
        } else if arena[rightmost].name == BRACKET_EL
            && let Some(&child) = arena[rightmost].children.last()
        {
            self.assign_locants_to_multiplied_root_if_present(arena, child)?;
        }
        Ok(())
    }
    fn locants_at_start(arena: &Arena, substituent: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        for &child in &arena[substituent].children {
            if arena[child].name == LOCANT_EL {
                result.push(child);
            } else if arena[child].name != STEREOCHEMISTRY_EL {
                break;
            }
        }
        result
    }
    pub fn add_implicit_brackets_when_substituent_has_two_locants(
        &mut self,
        arena: &mut Arena,
        substituent: NodeId,
        brackets: &mut Vec<NodeId>,
    ) {
        let Some(next) = arena
            .next_sibling(substituent)
            .filter(|&n| arena[n].name == SUBSTITUENT_EL)
        else {
            return;
        };
        let locants = Self::locants_at_start(arena, substituent);
        if locants.len() != 2
            || locants.iter().any(|&l| arena.value(l).contains(','))
            || !Self::locants_at_start(arena, next).is_empty()
        {
            return;
        }
        let parent = arena[substituent].parent.unwrap();
        let index = arena.index_of(parent, substituent).unwrap();
        let bracket = arena.grouping(BRACKET_EL);
        arena[bracket].set_attribute(TYPE_ATR, IMPLICIT_TYPE_VAL);
        let count = arena.index_of(substituent, locants[0]).unwrap() + 1;
        for _ in 0..count {
            let child = arena[substituent].children[0];
            arena.detach(child);
            arena.add_child(bracket, child);
        }
        arena.detach(substituent);
        arena.detach(next);
        arena.add_child(bracket, substituent);
        arena.add_child(bracket, next);
        arena.insert_child(parent, bracket, index);
        brackets.push(bracket);
    }
    fn locants_debug(arena: &Arena, locants: &[NodeId]) -> String {
        let mut message = format!(
            "Unable to assign all locants. {} not assigned: ",
            if locants.len() > 1 {
                "These locants were"
            } else {
                "This locant was"
            }
        );
        for &locant in locants {
            message.push_str(&arena.value(locant));
            message.push(' ');
        }
        message
    }
    fn word_level_locants_allowed(&self, arena: &Arena, element: NodeId, count: usize) -> bool {
        let Some(parent) = arena[element].parent else {
            return false;
        };
        let rule = self.state.current_word_rule.as_deref().unwrap_or("");
        let at_end_or_two = arena.next_sibling(element).is_none() || count >= 2;
        if arena[parent].attribute(TYPE_ATR) == Some(SUBSTITUENT_EL)
            && at_end_or_two
            && matches!(
                rule,
                "ester" | "functionalClassEster" | "multiEster" | "acetal"
            )
        {
            return true;
        }
        if (matches!(rule, "potentialAlcoholEster" | "amineDiConjunctiveSuffix")
            || (rule == "ester" && at_end_or_two))
            && arena[parent].name == WORD_EL
            && let Some(word_rule) = arena[parent].parent
            && arena.children_named(word_rule, WORD_EL).last() == Some(&parent)
        {
            return true;
        }
        if rule == "acidReplacingFunctionalGroup"
            && arena[parent].name == WORD_EL
            && at_end_or_two
            && let Some(word_rule) = arena[parent].parent
        {
            return arena.index_of(word_rule, parent).is_some_and(|i| i > 0);
        }
        false
    }
    pub fn assign_locants_and_multipliers(
        &mut self,
        arena: &mut Arena,
        element: NodeId,
    ) -> Result<()> {
        let mut locants = arena.children_named(element, LOCANT_EL);
        let multipliers = arena.children_named(element, MULTIPLIER_EL);
        let parent = arena[element]
            .parent
            .ok_or_else(|| error("Detached root/substituent/bracket"))?;
        let word_level = arena[parent].name == WORD_EL;
        let group = arena.first_child_named(element, GROUP_EL);
        let mut count = 1;
        if !multipliers.is_empty() {
            if multipliers.len() > 1 {
                return Err(error(format!(
                    "{} has multiple multipliers, unable to determine meaning!",
                    arena[element].name
                )));
            }
            if word_level
                && arena.next_sibling(element).is_none()
                && arena.previous_sibling(element).is_none()
            {
                return Ok(());
            }
            count = parse_num(arena[multipliers[0]].attribute(VALUE_ATR).unwrap_or(""))?;
            let value = attr(arena, multipliers[0], VALUE_ATR);
            arena[element].set_attribute(MULTIPLIER_ATR, value);
            if group
                .is_some_and(|g| arena[g].attribute(SUBTYPE_ATR) == Some(PERHALOGENO_SUBTYPE_VAL))
            {
                return Err(error(format!(
                    "{} cannot be multiplied",
                    arena.value(group.unwrap())
                )));
            }
        }
        if locants.is_empty() {
            return Ok(());
        }
        if count == 1
            && word_level
            && arena.previous_sibling(element).is_none()
            && self.word_level_locants_allowed(arena, element, locants.len())
        {
            let locant = locants.remove(0);
            let value = arena.value(locant);
            if value.split(',').count() != 1 {
                return Err(error(
                    "Multiplier and locant count failed to agree; All locants could not be assigned!",
                ));
            }
            arena[parent].set_attribute(LOCANT_ATR, value);
            arena.detach(locant);
            if locants.is_empty() {
                return Ok(());
            }
        }
        if arena[element].name == ROOT_EL || locants.len() != 1 {
            return Err(error(Self::locants_debug(arena, &locants)));
        }
        let locant = locants[0];
        let value = arena.value(locant);
        if value.split(',').count() != count {
            return Err(error(
                "Multiplier and locant count failed to agree; All locants could not be assigned!",
            ));
        }
        if !(arena[parent].name == WORD_EL
            && arena[parent].attribute(TYPE_ATR) == Some("full")
            && self.state.current_word_rule.as_deref() == Some("carbonylDerivative"))
        {
            let index = arena.index_of(parent, element).unwrap();
            if !arena[parent]
                .children
                .iter()
                .skip(index + 1)
                .any(|&n| arena[n].name != HYPHEN_EL)
            {
                return Err(error(Self::locants_debug(arena, &locants)));
            }
        }
        if group.is_some_and(|g| arena[g].attribute(SUBTYPE_ATR) == Some(PERHALOGENO_SUBTYPE_VAL)) {
            return Err(error(format!(
                "{} cannot be locanted",
                arena.value(group.unwrap())
            )));
        }
        arena[element].set_attribute(LOCANT_ATR, value);
        arena.detach(locant);
        Ok(())
    }
    pub fn process_word_level_multiplier_if_applicable(
        &mut self,
        arena: &mut Arena,
        word: NodeId,
        roots: &[NodeId],
        word_count: usize,
    ) -> Result<()> {
        if arena[word].children.len() == 1 {
            let first = arena[word].children[0];
            let Some(multiplier) = arena.first_child_named(first, MULTIPLIER_EL) else {
                return Ok(());
            };
            let count = parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
            let locants = arena.children_named(first, LOCANT_EL);
            let mut assigned = None;
            if locants.len() > 1 {
                return Err(error("Unable to assign all locants"));
            }
            if let Some(&locant) = locants.first() {
                let values: Vec<_> = arena.value(locant).split(',').map(str::to_owned).collect();
                if values.len() != count {
                    return Err(error("Unable to assign all locants"));
                }
                arena.detach(locant);
                if !self.word_level_locants_allowed(arena, first, locants.len()) {
                    return Err(error(Self::locants_debug(arena, &locants)));
                }
                arena[word].set_attribute(LOCANT_ATR, &values[0]);
                assigned = Some(values);
            }
            if arena.value(multiplier) == "non"
                && !arena[multiplier]
                    .attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR)
                    .is_some_and(|s| s.to_ascii_lowercase().starts_with('a'))
            {
                return Err(error(
                    "\"non\" probably means \"not\". If a multiplier of value 9 was intended \"nona\" should be used",
                ));
            }
            let mono_element = count == 1
                && arena.next_sibling(multiplier).is_some_and(|g| {
                    arena[g].name == GROUP_EL
                        && (arena[g].attribute(TYPE_ATR) == Some(ELEMENTARYATOM_TYPE_VAL)
                            || arena.value(g) == "hydrogen")
                });
            if word_count == 1 && !mono_element {
                return Err(error(
                    "Unexpected multiplier found at start of word. Perhaps the name is trivial e.g. triphosgene",
                ));
            }
            if count == 1 {
                return Ok(());
            }
            let index = arena.index_of(first, multiplier).unwrap();
            let mut excluded = Vec::new();
            let preceding_children = arena[first].children[..index].to_vec();
            for id in preceding_children.into_iter().rev() {
                arena.detach(id);
                excluded.push(id);
            }
            arena.detach(multiplier);
            for i in (1..count).rev() {
                let copy = self
                    .state
                    .clone_element(arena, word, 0)
                    .map_err(|e| error(e.to_string()))?;
                if let Some(values) = &assigned {
                    arena[copy].set_attribute(LOCANT_ATR, &values[i]);
                }
                arena.insert_after(word, copy);
            }
            for id in excluded {
                arena.insert_child(first, id, 0);
            }
        } else if roots.len() == 1
            && !arena
                .descendants_named(roots[0], FRACTIONALMULTIPLIER_EL)
                .is_empty()
        {
            return Err(error(
                "Unexpected fractional multiplier found within chemical name",
            ));
        }
        Ok(())
    }
}

pub(crate) fn next_group(arena: &Arena, mut current: NodeId) -> Option<NodeId> {
    if arena[current].name == GROUP_EL {
        current = arena[current].parent?;
    }
    let parent = arena[current].parent?;
    if arena[parent].name == MOLECULE_EL {
        return None;
    }
    let index = arena.index_of(parent, current)?;
    if index + 1 == arena[parent].children.len() {
        return next_group(arena, parent);
    }
    let mut next = arena[parent].children[index + 1];
    while let Some(&first) = arena[next].children.first() {
        next = first;
    }
    let groups = arena.children_named(arena[next].parent?, GROUP_EL);
    groups.first().copied().or_else(|| next_group(arena, next))
}
pub(crate) fn previous_group(arena: &Arena, mut current: NodeId) -> Option<NodeId> {
    if arena[current].name == GROUP_EL {
        current = arena[current].parent?;
    }
    let parent = arena[current].parent?;
    if arena[parent].name == WORDRULE_EL {
        return None;
    }
    let index = arena.index_of(parent, current)?;
    if index == 0 {
        return previous_group(arena, parent);
    }
    let mut previous = arena[parent].children[index - 1];
    while let Some(&last) = arena[previous].children.last() {
        previous = last;
    }
    let groups = arena.children_named(arena[previous].parent?, GROUP_EL);
    groups
        .last()
        .copied()
        .or_else(|| previous_group(arena, previous))
}
fn chain_length(state: &BuildState, fragment: FragmentId) -> usize {
    let mut length = 0;
    while state
        .fragment_manager
        .graph
        .atom_by_locant(fragment, &(length + 1).to_string())
        .is_some()
    {
        length += 1;
    }
    length
}
/// The shared indirect-feature selector used by ring-assembly resolution.
pub fn find_elements_missing_indirect_locants(
    arena: &Arena,
    sub_or_root: NodeId,
    locant: NodeId,
) -> Vec<NodeId> {
    let Some(locant_index) = arena.index_of(sub_or_root, locant) else {
        return Vec::new();
    };
    arena[sub_or_root]
        .children
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(i, id)| {
            if i <= locant_index
                || !matches!(
                    arena[id].name.as_str(),
                    SUFFIX_EL | UNSATURATOR_EL | CONJUNCTIVESUFFIXGROUP_EL
                )
                || [LOCANT_ATR, LOCANTID_ATR, MULTIPLIED_ATR]
                    .iter()
                    .any(|a| arena[id].attribute(a).is_some())
            {
                return None;
            }
            if arena[id].name == SUFFIX_EL
                && let Some(group) = arena.previous_sibling_named(id, GROUP_EL)
            {
                let kind = arena[group].attribute(TYPE_ATR);
                if kind == Some(ACIDSTEM_TYPE_VAL)
                    && arena[id].attribute(SUBTYPE_ATR) != Some(CYCLEFORMER_SUBTYPE_VAL)
                    || matches!(
                        kind,
                        Some(NONCARBOXYLICACID_TYPE_VAL | CHALCOGENACIDSTEM_TYPE_VAL)
                    )
                {
                    return None;
                }
            }
            Some(id)
        })
        .collect()
}

impl ComponentProcessor<'_> {
    pub fn determine_locant_meaning(
        &mut self,
        arena: &mut Arena,
        element: NodeId,
        final_element: NodeId,
    ) -> Result<()> {
        let group = arena.first_child_named(element, GROUP_EL);
        for locant in arena.children_named(element, LOCANT_EL) {
            let values: Vec<_> = arena.value(locant).split(',').map(str::to_owned).collect();
            if values.len() <= 1 {
                continue;
            }
            let mut next = arena.next_sibling(locant);
            let mut depth = 0;
            let mut multiplier = None;
            while let Some(id) = next {
                let name = arena[id].name.as_str();
                if name == STRUCTURALOPENBRACKET_EL {
                    depth += 1;
                } else if name == STRUCTURALCLOSEBRACKET_EL {
                    depth -= 1;
                }
                if depth != 0 {
                    next = arena.next_sibling(id);
                    continue;
                }
                if name == LOCANT_EL {
                    break;
                }
                if name == MULTIPLIER_EL {
                    if values.len() == parse_num(arena[id].attribute(VALUE_ATR).unwrap_or(""))?
                        && (Some(id)
                            == arena.next_sibling_ignoring(locant, &[INDICATEDHYDROGEN_EL])
                            || arena.next_sibling(id).is_some_and(|n| {
                                matches!(
                                    arena[n].name.as_str(),
                                    SUFFIX_EL | INFIX_EL | UNSATURATOR_EL | GROUP_EL
                                )
                            }))
                    {
                        multiplier = Some(id);
                        break;
                    }
                    if Some(id) == arena.next_sibling(locant) {
                        multiplier = Some(id);
                    }
                } else if name == RINGASSEMBLYMULTIPLIER_EL
                    && Some(id) == arena.next_sibling(locant)
                {
                    multiplier = Some(id);
                    if let Some(group) = group
                        && !crate::fragment_tools::all_atoms_in_ring_are_identical(
                            &self.state.fragment_manager.graph,
                            frag(arena, group)?,
                        )
                    {
                        break;
                    }
                } else if name == FUSEDRINGBRIDGE_EL
                    && values.len() == 2
                    && Some(id) == arena.next_sibling(locant)
                {
                    break;
                }
                next = arena.next_sibling(id);
            }
            if let Some(multiplier) = multiplier {
                let count = parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
                if count == values.len() {
                    let mut modified = false;
                    if values.last().is_some_and(|s| s.ends_with('\''))
                        && let Some(group) = group.filter(|&g| {
                            arena.index_of(element, g) > arena.index_of(element, locant)
                        })
                    {
                        let mut special = arena[group]
                            .attribute(OUTIDS_ATR)
                            .is_some_and(|s| s.split(',').count() > 1);
                        if !special {
                            let (mut inline_count, mut current_multiplier, mut next) =
                                (0, 1, arena.next_sibling(group));
                            while let Some(id) = next {
                                if arena[id].name == MULTIPLIER_EL {
                                    current_multiplier =
                                        parse_num(arena[id].attribute(VALUE_ATR).unwrap_or(""))?;
                                } else if arena[id].name == SUFFIX_EL
                                    && arena[id].attribute(TYPE_ATR) == Some(INLINE_TYPE_VAL)
                                {
                                    inline_count += current_multiplier;
                                    current_multiplier = 1;
                                }
                                next = arena.next_sibling(id);
                            }
                            special = inline_count >= 2;
                        }
                        if special {
                            modified = self.check_special_locant_uses(
                                arena,
                                locant,
                                &values,
                                final_element,
                            )?;
                        }
                    }
                    if !modified && arena.next_sibling(locant) != Some(multiplier) {
                        arena.detach(locant);
                        arena.insert_before(multiplier, locant);
                    }
                } else if !self.check_special_locant_uses(arena, locant, &values, final_element)? {
                    return Err(error(format!(
                        "Mismatch between locant and multiplier counts ({} and {}):{}",
                        values.len(),
                        count,
                        arena.value(locant)
                    )));
                }
            } else if !self.check_special_locant_uses(arena, locant, &values, final_element)? {
                return Err(error(format!(
                    "Multiple locants without a multiplier: {}",
                    arena.to_xml(locant)
                )));
            }
        }
        Ok(())
    }
    fn check_special_locant_uses(
        &mut self,
        arena: &mut Arena,
        locant: NodeId,
        values: &[String],
        final_element: NodeId,
    ) -> Result<bool> {
        let count = values.len();
        let mut current = arena.next_sibling(locant);
        let (mut hetero, mut multiplier) = (0, 1);
        while let Some(id) = current {
            if arena[id].name == GROUP_EL {
                break;
            }
            if arena[id].name == HETEROATOM_EL {
                hetero += multiplier;
                multiplier = 1;
            } else if arena[id].name == MULTIPLIER_EL {
                multiplier = parse_num(arena[id].attribute(VALUE_ATR).unwrap_or(""))?;
            } else {
                break;
            }
            current = arena.next_sibling(id);
        }
        if let Some(group) = current.filter(|&n| arena[n].name == GROUP_EL) {
            if arena[group].attribute(SUBTYPE_ATR) == Some(HANTZSCHWIDMAN_SUBTYPE_VAL) {
                if hetero == count {
                    return Ok(true);
                } else if hetero > 1 {
                    return Ok(false);
                }
            }
            if hetero == 0 && arena[group].attribute(OUTIDS_ATR).is_some() {
                let mut ids: Vec<_> = attr(arena, group, OUTIDS_ATR)
                    .split(',')
                    .map(str::to_owned)
                    .collect();
                let fragment = frag(arena, group)?;
                if count == ids.len()
                    && self
                        .state
                        .fragment_manager
                        .graph
                        .fragment(fragment)
                        .atoms
                        .len()
                        > 1
                {
                    let first = first_atom(self.state, fragment)?;
                    let mut found = true;
                    for i in (0..ids.len()).rev() {
                        if let Some(atom) = self
                            .state
                            .fragment_manager
                            .graph
                            .atom_by_locant(fragment, &values[i])
                        {
                            ids[i] = (atom.0 - first.0 + 1).to_string();
                        } else {
                            found = false;
                            break;
                        }
                    }
                    if found {
                        arena[group].set_attribute(OUTIDS_ATR, ids.join(","));
                        arena.detach(locant);
                        return Ok(true);
                    }
                }
            } else if matches!(arena.value(group).as_str(), "benz" | "benzo")
                && arena.next_sibling_named(group, GROUP_EL).is_some()
            {
                return Ok(true);
            }
        }
        if let Some(id) = current {
            if arena[id].name == POLYCYCLICSPIRO_EL
                || arena[id].name == FUSEDRINGBRIDGE_EL && count == 2
            {
                return Ok(true);
            }
            if arena[id].name == SUFFIX_EL
                && arena[id].attribute(SUBTYPE_ATR) == Some(CYCLEFORMER_SUBTYPE_VAL)
                && count == 2
            {
                let value = arena.value(locant);
                arena[id].set_attribute(LOCANT_ATR, value);
                arena.detach(locant);
                return Ok(true);
            }
            if arena[id].name == SUBTRACTIVEPREFIX_EL
                && arena[id].attribute(TYPE_ATR) == Some(ANHYDRO_TYPE_VAL)
            {
                if count != 2 {
                    return Err(error(format!(
                        "Two locants are required before an anhydro prefix, but found: {}",
                        arena.value(locant)
                    )));
                }
                let value = arena.value(locant);
                arena[id].set_attribute(LOCANT_ATR, value);
                arena.detach(locant);
                return Ok(true);
            }
        }
        if self.detect_multiplicative_nomenclature(arena, locant, values, final_element)? {
            return Ok(true);
        }
        if let Some(group) = current.filter(|&n| count == 2 && arena[n].name == GROUP_EL) {
            if arena[group].attribute(SUBTYPE_ATR) == Some(EPOXYLIKE_SUBTYPE_VAL) {
                return Ok(true);
            }
            if arena[group].attribute(IMINOLIKE_ATR) == Some("yes") {
                arena[group].set_attribute(SUBTYPE_ATR, EPOXYLIKE_SUBTYPE_VAL);
                return Ok(true);
            }
        }
        if let Some(parent) = arena[locant]
            .parent
            .filter(|&p| count == 2 && arena[p].name == BRACKET_EL)
            && let Some(&sub) = arena.children_named(parent, SUBSTITUENT_EL).last()
            && let Some(group) = arena
                .first_child_named(sub, GROUP_EL)
                .filter(|&g| arena[g].attribute(SUBTYPE_ATR) == Some(EPOXYLIKE_SUBTYPE_VAL))
        {
            arena.detach(locant);
            arena.insert_before(group, locant);
            return Ok(true);
        }
        Ok(false)
    }
    fn detect_multiplicative_nomenclature(
        &mut self,
        arena: &mut Arena,
        locant: NodeId,
        values: &[String],
        final_element: NodeId,
    ) -> Result<bool> {
        let Some(mut multiplier) = arena[final_element].children.first().copied() else {
            return Ok(false);
        };
        if let Some(parent) = arena[final_element]
            .parent
            .filter(|&p| arena[p].name == BRACKET_EL)
        {
            let use_parent = arena[multiplier].name != MULTIPLIER_EL
                || arena.next_sibling(multiplier).is_some_and(|n| {
                    matches!(
                        arena[n].name.as_str(),
                        HETEROATOM_EL | SUBTRACTIVEPREFIX_EL | FUSEDRINGBRIDGE_EL
                    ) || arena[n].name == HYDRO_EL && !arena.value(n).starts_with("per")
                });
            if use_parent {
                let Some(&first) = arena[parent].children.first() else {
                    return Ok(false);
                };
                multiplier = first;
            }
        }
        let common = arena[locant].parent.and_then(|p| arena[p].parent);
        let mut parent = arena[multiplier].parent;
        while let Some(id) = parent {
            if Some(id) == common
                && values.last().is_some_and(|s| s.ends_with('\''))
                && arena[multiplier].name == MULTIPLIER_EL
                && arena
                    .next_sibling(multiplier)
                    .is_none_or(|n| arena[n].name != MULTIPLICATIVELOCANT_EL)
                && parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))? == values.len()
            {
                arena[locant].name = MULTIPLICATIVELOCANT_EL.into();
                arena.detach(locant);
                arena.insert_after(multiplier, locant);
                return Ok(true);
            }
            parent = arena[id].parent;
        }
        Ok(false)
    }
    pub fn match_locants_to_indirect_features(
        &mut self,
        arena: &mut Arena,
        sub_or_root: NodeId,
    ) -> Result<()> {
        let mut locants = Vec::new();
        for &child in &arena[sub_or_root].children {
            if arena[child].name == GROUP_EL {
                break;
            }
            if arena[child].name == LOCANT_EL
                && arena
                    .next_sibling(child)
                    .is_none_or(|n| arena[n].name != MULTIPLIER_EL)
            {
                locants.push(child);
            }
        }
        let Some(&last) = locants.last() else {
            return Ok(());
        };
        let group = arena
            .first_child_named(sub_or_root, GROUP_EL)
            .ok_or_else(|| error("No group for indirect locants"))?;
        let values: Vec<_> = arena.value(last).split(',').map(str::to_owned).collect();
        if values.len() == 1
            && arena[group]
                .attribute(FRONTLOCANTSEXPECTED_ATR)
                .is_some_and(|allowed| allowed.split(',').any(|s| s == values[0]))
            && let Some(suffix) = arena
                .next_sibling(group)
                .filter(|&n| arena[n].name == SUFFIX_EL && arena[n].attribute(LOCANT_ATR).is_none())
        {
            arena[suffix].set_attribute(LOCANT_ATR, &values[0]);
            arena.detach(last);
            return Ok(());
        }
        let fragment = frag(arena, group)?;
        let parent = arena[sub_or_root].parent.unwrap();
        let multi_ester = self.state.current_word_rule.as_deref() == Some("multiEster")
            && arena[last].attribute(TYPE_ATR) != Some(ADDEDHYDROGENLOCANT_TYPE_VAL)
            && arena[parent].name == WORD_EL
            && arena[parent].attribute(TYPE_ATR) == Some(SUBSTITUENT_EL)
            && arena[parent].children.len() == 1
            && values.len() == 1
            && arena[last].attribute(TYPE_ATR) != Some(ORTHOMETAPARA_TYPE_VAL);
        if multi_ester
            || self
                .state
                .fragment_manager
                .graph
                .fragment(fragment)
                .atoms
                .len()
                <= 1
        {
            return Ok(());
        }
        if arena[last].attribute(TYPE_ATR) != Some(ADDEDHYDROGENLOCANT_TYPE_VAL)
            && locants.len() == 1
            && arena[group].attribute(ISAMULTIRADICAL_ATR).is_none()
            && values.len() == 1
            && Self::check_locant_present_on_potential_root(
                self.state,
                arena,
                sub_or_root,
                &values[0],
            )?
            && arena.previous_sibling_named(last, LOCANT_EL).is_none()
        {
            return Ok(());
        }
        let missing = find_elements_missing_indirect_locants(arena, sub_or_root, last);
        let assignable = missing.len() >= values.len()
            && values.iter().all(|l| {
                self.state
                    .fragment_manager
                    .graph
                    .atom_by_locant(fragment, l)
                    .is_some()
            });
        if assignable {
            for (element, value) in missing.into_iter().zip(values) {
                arena[element].set_attribute(LOCANT_ATR, value);
            }
            arena.detach(last);
        } else if values.len() == 1 {
            let regex = regex::Regex::new(r"^[A-Z][a-z]?'*(\d+[a-z]?'*)?$").unwrap();
            if regex.is_match(&values[0]) {
                return Ok(());
            }
            let suffixes = self
                .state
                .xml_suffix_map
                .get(&group)
                .cloned()
                .unwrap_or_default();
            let numeric = regex::Regex::new(r"^\d+[a-z]?'*$").unwrap();
            for suffix in suffixes {
                let Some(target) = self
                    .state
                    .fragment_manager
                    .graph
                    .atom_by_locant(suffix, &values[0])
                else {
                    continue;
                };
                let r = first_atom(self.state, suffix)?;
                let neighbour = self
                    .state
                    .fragment_manager
                    .graph
                    .neighbours(r)
                    .into_iter()
                    .find(|&a| {
                        self.state
                            .fragment_manager
                            .graph
                            .atom(a)
                            .locants
                            .iter()
                            .any(|s| numeric.is_match(s))
                    });
                if let Some(neighbour) = neighbour {
                    let bond = self
                        .state
                        .fragment_manager
                        .graph
                        .bond_between(r, neighbour)
                        .unwrap();
                    let order = self.state.fragment_manager.graph.bond(bond).order;
                    self.state.fragment_manager.remove_bond(bond);
                    self.state
                        .fragment_manager
                        .create_bond(r, target, order)
                        .map_err(|e| error(e.to_string()))?;
                    arena.detach(last);
                }
            }
        }
        Ok(())
    }
    pub fn assign_implicit_locants_to_di_terminal_suffixes(
        &mut self,
        arena: &mut Arena,
        sub_or_root: NodeId,
    ) -> Result<()> {
        fn terminal(arena: &Arena, id: NodeId) -> bool {
            arena[id].name == SUFFIX_EL
                && arena[id].attribute(LOCANT_ATR).is_none()
                && (arena[id].attribute(TYPE_ATR) == Some(INLINE_TYPE_VAL)
                    || arena[id].attribute(SUBTYPE_ATR) == Some(TERMINAL_SUBTYPE_VAL))
        }
        if let Some(first) = arena
            .first_child_named(sub_or_root, SUFFIX_EL)
            .filter(|&s| terminal(arena, s))
            && let Some(second) = arena.next_sibling(first).filter(|&s| terminal(arena, s))
            && let Some(group) = arena
                .previous_sibling_named(first, GROUP_EL)
                .filter(|&g| arena[g].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL))
        {
            let length = chain_length(self.state, frag(arena, group)?);
            if length >= 2 {
                arena[first].set_attribute(LOCANT_ATR, "1");
                arena[second].set_attribute(LOCANT_ATR, length.to_string());
            }
        }
        Ok(())
    }
}

impl ComponentProcessor<'_> {
    fn unsuitable_for_forming_chain_multiradical(
        &self,
        arena: &Arena,
        group: NodeId,
        multiplier: NodeId,
    ) -> Result<bool> {
        if let Some(previous) = previous_group(arena, group) {
            if let Some(radical_count) = arena[previous].attribute(ISAMULTIRADICAL_ATR) {
                if arena[previous]
                    .attribute(ACCEPTSADDITIVEBONDS_ATR)
                    .is_some()
                    && arena[previous]
                        .parent
                        .and_then(|p| arena.previous_sibling(p))
                        .is_some()
                {
                    return Ok(false);
                }
                if arena
                    .previous_element(multiplier, true)
                    .is_some_and(|n| arena[n].name == MULTIPLIER_EL)
                {
                    return Ok(false);
                }
                return Ok(Some(radical_count) == arena[multiplier].attribute(VALUE_ATR));
            } else if arena
                .previous_sibling_named(previous, MULTIPLIER_EL)
                .is_none()
            {
                let fragment = frag(arena, previous)?;
                let f = self.state.fragment_manager.graph.fragment(fragment);
                let mut valency = if f.out_atoms.len() == 1 {
                    f.out_atoms[0].valency
                } else {
                    0
                };
                if f.out_atoms.len() != 1
                    && let Some(suffix) = arena.next_sibling_named(previous, SUFFIX_EL)
                {
                    match arena[suffix].attribute(VALUE_ATR) {
                        Some("ylidene") => valency = 2,
                        Some("ylidyne") => valency = 3,
                        _ => {}
                    }
                }
                if valency
                    == parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))? as i32
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn calculate_out_atoms_to_be_added_from_inline_suffixes(
        &self,
        arena: &Arena,
        group: NodeId,
        suffixes: &[NodeId],
    ) -> Result<usize> {
        let fragment = frag(arena, group)?;
        let f = self.state.fragment_manager.graph.fragment(fragment);
        let kind = if self
            .suffix_rules
            .is_group_type_with_specific_suffix_rules(&f.fragment_type)
        {
            f.fragment_type.as_str()
        } else {
            STANDARDGROUP_TYPE_VAL
        };
        let mut count = self
            .state
            .xml_suffix_map
            .get(&group)
            .into_iter()
            .flatten()
            .map(|&s| {
                self.state
                    .fragment_manager
                    .graph
                    .fragment(s)
                    .out_atoms
                    .len()
            })
            .sum::<usize>();
        for &suffix in suffixes {
            count += self
                .suffix_rules
                .rule_tags(
                    kind,
                    arena[suffix].attribute(VALUE_ATR).unwrap_or(""),
                    Some(&f.sub_type),
                )
                .map_err(|e| error(e.to_string()))?
                .iter()
                .filter(|r| r.kind == SuffixRuleType::SetOutAtom)
                .count();
        }
        Ok(count)
    }
    pub fn handle_multi_radicals(&mut self, arena: &mut Arena, sub_or_root: NodeId) -> Result<()> {
        let group = arena
            .first_child_named(sub_or_root, GROUP_EL)
            .ok_or_else(|| error("No group in multiradical component"))?;
        let group_value = arena.value(group);
        let mut fragment = frag(arena, group)?;
        if matches!(
            group_value.as_str(),
            "methylene" | "methylen" | "oxy" | "thio" | "seleno" | "telluro"
        ) && let Some(multiplier) = arena.previous_sibling(group).filter(|&m| {
            arena[m].name == MULTIPLIER_ATR
                && arena[m].attribute(TYPE_ATR) == Some(BASIC_TYPE_VAL)
                && arena.previous_sibling(m).is_none()
        }) {
            let count = parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
            if !self.unsuitable_for_forming_chain_multiradical(arena, group, multiplier)? {
                let smiles_atom = match group_value.as_str() {
                    "methylene" | "methylen" => "C",
                    "oxy" => "O",
                    "thio" => "S",
                    "seleno" => "[SeH?]",
                    "telluro" => "[TeH?]",
                    _ => return Err(error("unexpected group value")),
                };
                arena[group].set_attribute(VALUE_ATR, smiles_atom.repeat(count));
                arena[group].set_attribute(OUTIDS_ATR, format!("1,{count}"));
                let value = format!("{}{group_value}", arena.value(multiplier));
                arena[group].set_value(value);
                arena.detach(multiplier);
                arena[group].set_attribute(LABELS_ATR, NUMERIC_LABELS_VAL);
                self.state
                    .fragment_manager
                    .remove_fragment(fragment)
                    .map_err(|e| error(e.to_string()))?;
                fragment = self.resolve_group(arena, group)?;
                arena[group].remove_attribute(USABLEASJOINER_ATR);
            }
        }
        if let Some(ids) = arena[group].attribute(OUTIDS_ATR) {
            for id in ids.split(',') {
                let atom = relative_atom(self.state, fragment, id)?;
                self.state
                    .fragment_manager
                    .graph
                    .add_out_atom(fragment, atom, 1, true);
            }
        }
        let out_count = self
            .state
            .fragment_manager
            .graph
            .fragment(fragment)
            .out_atoms
            .len();
        if out_count >= 2 {
            if matches!(group_value.as_str(), "amine" | "amin")
                && (previous_group(arena, group).is_none_or(|g| {
                    arena[g].fragment.is_none_or(|f| {
                        self.state
                            .fragment_manager
                            .graph
                            .fragment(f)
                            .out_atoms
                            .len()
                            < 2
                    })
                }) || next_group(arena, group).is_none())
            {
                return Err(error("Invalid use of amine as a substituent!"));
            }
            if self.state.current_word_rule.as_deref() == Some("polymer") && out_count >= 3 {
                let mut extra_valency = 0;
                for i in (2..out_count).rev() {
                    extra_valency += self
                        .state
                        .fragment_manager
                        .graph
                        .remove_out_atom(fragment, i)
                        .valency;
                }
                let valency = self
                    .state
                    .fragment_manager
                    .graph
                    .fragment(fragment)
                    .out_atoms[1]
                    .valency
                    + extra_valency;
                self.state
                    .fragment_manager
                    .graph
                    .set_out_atom_valency(fragment, 1, valency);
            }
        }
        if out_count == 2
            && arena[group].attribute(SUBTYPE_ATR) == Some(EPOXYLIKE_SUBTYPE_VAL)
            && let Some(locant) = arena.previous_sibling(group)
        {
            let values: Vec<_> = arena.value(locant).split(',').map(str::to_owned).collect();
            if values.len() == 2 {
                for (i, value) in values.iter().enumerate() {
                    self.state.fragment_manager.graph.set_out_atom_locant(
                        fragment,
                        i,
                        Some(value.clone()),
                    );
                }
                arena.detach(locant);
                arena[sub_or_root].set_attribute(LOCANT_ATR, &values[0]);
            }
        }
        let total = out_count
            + self.calculate_out_atoms_to_be_added_from_inline_suffixes(
                arena,
                group,
                &arena.children_named(sub_or_root, SUFFIX_EL),
            )?;
        if total >= 2 {
            arena[group].set_attribute(ISAMULTIRADICAL_ATR, total.to_string());
        }
        Ok(())
    }
    pub fn detect_conjunctive_suffix_groups(
        &mut self,
        arena: &mut Arena,
        sub_or_root: NodeId,
        all_groups: &mut Vec<NodeId>,
    ) -> Result<()> {
        let groups = arena.children_named(sub_or_root, GROUP_EL);
        if groups.len() <= 1 {
            return Ok(());
        }
        let mut conjunctive = Vec::new();
        let mut ring = None;
        for &group in groups.iter().rev() {
            if arena[group].attribute(TYPE_ATR) != Some(RING_TYPE_VAL) {
                conjunctive.push(group);
            } else {
                ring = Some(group);
                break;
            }
        }
        if conjunctive.is_empty() {
            return Ok(());
        }
        if ring.is_none() {
            return Err(error(
                "OPSIN bug: unable to find ring associated with conjunctive suffix group",
            ));
        }
        if conjunctive.len() != 1 {
            return Err(error(
                "OPSIN Bug: Two groups exactly should be present at this point when processing conjunctive nomenclature",
            ));
        }
        let primary = conjunctive[0];
        let fragment = frag(arena, primary)?;
        let atoms = self
            .state
            .fragment_manager
            .graph
            .fragment(fragment)
            .atoms
            .clone();
        for &atom in &atoms {
            self.state.fragment_manager.graph.clear_locants(atom);
        }
        let mut suffixes = arena.next_siblings_named(primary, SUFFIX_EL);
        self.preliminary_process_suffixes(arena, primary, &mut suffixes)?;
        SuffixApplier::new(self.state, self.suffix_rules)
            .resolve_suffixes(arena, primary, &suffixes)
            .map_err(|e| error(e.to_string()))?;
        for suffix in suffixes {
            arena.detach(suffix);
        }
        arena[primary].name = CONJUNCTIVESUFFIXGROUP_EL.into();
        all_groups.retain(|&g| g != primary);
        let alpha_at_first = self.state.fragment_manager.graph.incoming_valency(atoms[0]) < 3;
        for (atom, name) in atoms
            .into_iter()
            .skip(usize::from(!alpha_at_first))
            .zip(["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"])
        {
            self.state.fragment_manager.graph.add_locant(atom, name);
        }
        if let Some(multiplier) = arena
            .previous_sibling(primary)
            .filter(|&m| arena[m].name == MULTIPLIER_EL)
        {
            let count = parse_num(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
            for i in 1..count {
                let copy = arena.copy(primary);
                let f = self
                    .state
                    .fragment_manager
                    .copy_and_relabel_fragment(fragment, i)
                    .map_err(|e| error(e.to_string()))?;
                self.state.fragment_manager.fragment_tokens.insert(f, copy);
                arena[copy].fragment = Some(f);
                conjunctive.push(copy);
                arena.insert_after(primary, copy);
            }
            let locant = arena.previous_sibling(multiplier);
            arena.detach(multiplier);
            if let Some(locant) = locant.filter(|&l| arena[l].name == LOCANT_EL) {
                let values: Vec<_> = arena.value(locant).split(',').map(str::to_owned).collect();
                if values.len() != count {
                    return Err(error(
                        "mismatch between number of locants and multiplier in conjunctive nomenclature routine",
                    ));
                }
                for (group, value) in conjunctive.into_iter().zip(values) {
                    arena[group].set_attribute(LOCANT_ATR, value);
                }
                arena.detach(locant);
            }
        }
        Ok(())
    }
    pub fn process_conjunctive_nomenclature(
        &mut self,
        arena: &Arena,
        sub_or_root: NodeId,
    ) -> Result<()> {
        let conjunctive = arena.children_named(sub_or_root, CONJUNCTIVESUFFIXGROUP_EL);
        if conjunctive.is_empty() {
            return Ok(());
        }
        let group = arena
            .first_child_named(sub_or_root, GROUP_EL)
            .ok_or_else(|| error("Conjunctive component has no ring"))?;
        let ring = frag(arena, group)?;
        if !self
            .state
            .fragment_manager
            .graph
            .fragment(ring)
            .out_atoms
            .is_empty()
        {
            return Err(error("OPSIN Bug: Ring fragment should have no radicals"));
        }
        for conjunctive in conjunctive {
            let fragment = frag(arena, conjunctive)?;
            let carbon = crate::fragment_tools::last_non_suffix_carbon_with_sufficient_valency(
                &self.state.fragment_manager.graph,
                fragment,
            )
            .ok_or_else(|| {
                error("OPSIN Bug: Unable to find non suffix carbon with sufficient valency")
            })?;
            let ring_atom = if let Some(locant) = arena[conjunctive].attribute(LOCANT_ATR) {
                locanted_atom(self.state, ring, locant)?
            } else {
                let atoms = crate::fragment_tools::find_substitutable_atoms(
                    &self.state.fragment_manager.graph,
                    ring,
                    1,
                )
                .map_err(|e| error(e.to_string()))?;
                let first = atoms
                    .first()
                    .copied()
                    .ok_or_else(|| error("No suitable atom found for conjunctive operation"))?;
                if crate::ambiguity::is_substitution_ambiguous(
                    &self.state.fragment_manager.graph,
                    &atoms,
                    1,
                )
                .map_err(|e| error(e.to_string()))?
                {
                    self.state.add_is_ambiguous(format!(
                        "Connection of conjunctive group to: {}",
                        arena.value(group)
                    ));
                }
                first
            };
            self.state
                .fragment_manager
                .create_bond(carbon, ring_atom, 1)
                .map_err(|e| error(e.to_string()))?;
            self.state
                .fragment_manager
                .incorporate_fragment(fragment, ring)
                .map_err(|e| error(e.to_string()))?;
        }
        Ok(())
    }
    pub fn add_implicit_brackets_to_amino_acids(
        &mut self,
        arena: &mut Arena,
        groups: &[NodeId],
        brackets: &mut Vec<NodeId>,
    ) {
        for &group in groups.iter().rev() {
            if arena[group].attribute(TYPE_ATR) != Some(AMINOACID_TYPE_VAL)
                || next_group(arena, group).is_none()
                || arena
                    .previous_sibling_ignoring(group, &[MULTIPLIER_EL])
                    .is_some_and(|n| arena[n].name == LOCANT_EL)
            {
                continue;
            }
            let sub = arena[group].parent.unwrap();
            let mut previous = arena.previous_sibling(sub);
            let mut elements = Vec::new();
            while let Some(id) = previous {
                if !matches!(arena[id].name.as_str(), SUBSTITUENT_EL | BRACKET_EL) {
                    break;
                }
                elements.push(id);
                previous = arena.previous_sibling(id);
            }
            if elements.is_empty() {
                continue;
            }
            elements.reverse();
            let parent = arena[sub].parent.unwrap();
            let index = arena.index_of(parent, elements[0]).unwrap();
            let bracket = arena.grouping(BRACKET_EL);
            arena[bracket].set_attribute(TYPE_ATR, IMPLICIT_TYPE_VAL);
            for element in elements {
                arena.detach(element);
                arena.add_child(bracket, element);
            }
            arena.detach(sub);
            arena.add_child(bracket, sub);
            arena.insert_child(parent, bracket, index);
            brackets.push(bracket);
        }
    }
    fn check_and_apply_first_locant_of_biochemical_linkage(
        &mut self,
        arena: &Arena,
        sub: NodeId,
        linkage: &str,
    ) -> Result<()> {
        let group = arena
            .first_child_named(sub, GROUP_EL)
            .ok_or_else(|| error("Biochemical linkage component has no group"))?;
        let fragment = frag(arena, group)?;
        let first = linkage
            .split_once('-')
            .map(|p| p.0)
            .ok_or_else(|| error("Malformed biochemical linkage"))?;
        if arena[group].attribute(TYPE_ATR) == Some(CARBOHYDRATE_TYPE_VAL) {
            let atom = locanted_atom(self.state, fragment, first)?;
            if !self
                .state
                .fragment_manager
                .graph
                .fragment(fragment)
                .out_atoms
                .iter()
                .any(|o| o.atom == atom)
            {
                return Err(error(format!(
                    "Invalid glycoside linkage descriptor. Locant: {first} should point to the anomeric carbon"
                )));
            }
        } else {
            let oxygen = locanted_atom(self.state, fragment, &format!("O{first}"))?;
            if self.state.fragment_manager.graph.atom(oxygen).bonds.len() != 1 {
                return Err(error(format!(
                    "{first} should be the carbon to which a hydroxy group is attached!"
                )));
            }
            let out = &self
                .state
                .fragment_manager
                .graph
                .fragment(fragment)
                .out_atoms;
            if out.len() != 1 {
                return Err(error(
                    "OPSIN Bug: Biochemical linkage only expected on groups with 1 OutAtom",
                ));
            }
            self.state
                .fragment_manager
                .create_bond(oxygen, out[0].atom, 1)
                .map_err(|e| error(e.to_string()))?;
        }
        if next_group(arena, group).is_none() {
            return Err(error(format!(
                "Biochemical linkage descriptor should be followed by another biochemical: {linkage}"
            )));
        }
        Ok(())
    }
    pub fn process_biochemical_linkage_descriptors(
        &mut self,
        arena: &mut Arena,
        substituents: &[NodeId],
        brackets: &mut Vec<NodeId>,
    ) -> Result<()> {
        fn second(linkage: &str) -> Result<String> {
            let index = linkage
                .rfind(['>', '-'])
                .ok_or_else(|| error("Malformed biochemical linkage"))?;
            Ok(format!("O{}", &linkage[index + 1..]))
        }
        fn linkage_value(arena: &Arena, node: NodeId) -> Result<String> {
            let value = arena.value(node);
            if value.len() < 2 {
                return Err(error("Malformed biochemical linkage"));
            }
            Ok(value[1..value.len() - 1].into())
        }
        fn add_locant(arena: &mut Arena, node: NodeId, value: String) -> Result<()> {
            if let Some(existing) = arena[node].attribute(LOCANT_ATR) {
                return Err(error(format!(
                    "Substituent with biochemical linkage descriptor should not also have a locant: {existing}"
                )));
            }
            arena[node].set_attribute(LOCANT_ATR, value);
            Ok(())
        }
        for &sub in substituents {
            let links = arena.children_named(sub, BIOCHEMICALLINKAGE_EL);
            if links.is_empty() {
                continue;
            }
            if links.len() > 1 {
                return Err(error(
                    "OPSIN Bug: More than 1 biochemical linkage locant associated with subsituent",
                ));
            }
            let link = links[0];
            let linkage = linkage_value(arena, link)?;
            self.check_and_apply_first_locant_of_biochemical_linkage(arena, sub, &linkage)?;
            let locant = second(&linkage)?;
            let parent = arena[sub].parent.unwrap();
            let adjacent = arena.next_sibling(sub).is_some_and(|n| {
                matches!(
                    arena[n].name.as_str(),
                    SUBSTITUENT_EL | BRACKET_EL | ROOT_EL
                )
            });
            let mut bracket_added = false;
            if adjacent {
                let mut previous = arena.previous_sibling(sub);
                let mut elements = Vec::new();
                while let Some(id) = previous {
                    if !matches!(arena[id].name.as_str(), SUBSTITUENT_EL | BRACKET_EL) {
                        break;
                    }
                    elements.push(id);
                    previous = arena.previous_sibling(id);
                }
                if !elements.is_empty() {
                    elements.reverse();
                    let index = arena.index_of(parent, elements[0]).unwrap();
                    let bracket = arena.grouping(BRACKET_EL);
                    arena[bracket].set_attribute(LOCANT_ATR, &locant);
                    for id in elements {
                        arena.detach(id);
                        arena.add_child(bracket, id);
                    }
                    arena.detach(sub);
                    arena.add_child(bracket, sub);
                    arena.insert_child(parent, bracket, index);
                    brackets.push(bracket);
                    bracket_added = true;
                    if let Some(existing) = arena[sub].attribute(LOCANT_ATR) {
                        return Err(error(format!(
                            "Substituent with biochemical linkage descriptor should not also have a locant: {existing}"
                        )));
                    }
                }
            }
            if !bracket_added {
                let target = if arena[parent].name == BRACKET_EL && !adjacent {
                    parent
                } else {
                    sub
                };
                add_locant(arena, target, locant)?;
            }
            arena.detach(link);
        }
        for &bracket in brackets.iter() {
            let links = arena.children_named(bracket, BIOCHEMICALLINKAGE_EL);
            if links.is_empty() {
                continue;
            }
            if links.len() > 1 {
                return Err(error(
                    "OPSIN Bug: More than 1 biochemical linkage locant associated with bracket",
                ));
            }
            let link = links[0];
            let sub = arena
                .previous_sibling(link)
                .filter(|&n| arena[n].name == SUBSTITUENT_EL)
                .ok_or_else(|| {
                    error("OPSIN Bug: Substituent expected before biochemical linkage locant")
                })?;
            let linkage = linkage_value(arena, link)?;
            self.check_and_apply_first_locant_of_biochemical_linkage(arena, sub, &linkage)?;
            add_locant(arena, bracket, second(&linkage)?)?;
            arena.detach(link);
        }
        Ok(())
    }
}

impl ComponentProcessor<'_> {
    pub fn handle_group_irregularities(
        &mut self,
        arena: &mut Arena,
        groups: &[NodeId],
    ) -> Result<()> {
        for &group in groups {
            let value = arena.value(group);
            let fragment = frag(arena, group)?;
            if matches!(value.as_str(), "porphyrin" | "porphin") {
                let parent = arena[group].parent.unwrap();
                let explicitly_set = arena
                    .children_named(parent, INDICATEDHYDROGEN_EL)
                    .iter()
                    .any(|&h| {
                        matches!(
                            arena[h].attribute(LOCANT_ATR),
                            Some("21" | "22" | "23" | "24")
                        )
                    });
                if !explicitly_set {
                    for locant in ["21", "23"] {
                        let atom = locanted_atom(self.state, fragment, locant)?;
                        self.state
                            .fragment_manager
                            .graph
                            .atom_mut(atom)
                            .spare_valency = false;
                    }
                }
            } else if matches!(
                value.as_str(),
                "xanthate" | "xanthat" | "xanthic acid" | "xanthicacid"
            ) {
                if let Some(rule) = arena.parent_word_rule(group)
                    && arena[rule].attribute(WORDRULE_ATR) == Some("simple")
                    && arena.descendants_named(rule, SUBSTITUENT_EL).is_empty()
                {
                    return Err(error(format!(
                        "{value} describes a class of compounds rather than a particular compound"
                    )));
                }
            } else if matches!(
                value.as_str(),
                "adenosin" | "cytidin" | "guanosin" | "inosin" | "uridin" | "xanthosin"
            ) {
                if let Some(prefix) = arena.previous_sibling(group).filter(|&p| {
                    arena[p].name == SUBTRACTIVEPREFIX_EL
                        && arena[p].attribute(TYPE_ATR) == Some(DEOXY_TYPE_VAL)
                        && arena[p].attribute(VALUE_ATR) == Some("O")
                        && arena[p].attribute(LOCANT_ATR).is_none()
                }) && arena
                    .previous_element(prefix, true)
                    .is_none_or(|n| arena[n].name != SUBTRACTIVEPREFIX_EL)
                {
                    crate::structure_building_methods::Assembly {
                        state: self.state,
                        arena,
                    }
                    .apply_subtractive_prefix(fragment, ChemEl::O, "2'")
                    .map_err(|e| error(e.to_string()))?;
                    arena.detach(prefix);
                }
            } else if value == "imidazol"
                && let Some(suffix) = arena
                    .next_sibling(group)
                    .filter(|&s| arena[s].name == SUFFIX_EL && arena.value(s) == "ium")
                && let Some(atom) = self
                    .state
                    .fragment_manager
                    .graph
                    .atom_by_locant(fragment, "3")
            {
                arena[suffix].set_attribute(DEFAULTLOCANTID_ATR, (atom.0 + 1).to_string());
            }
            if arena[group].attribute(USABLEASJOINER_ATR) == Some("yes")
                && arena[group].attribute(DEFAULTINID_ATR).is_none()
                && arena[group].attribute(DEFAULTINLOCANT_ATR).is_none()
            {
                let length = chain_length(self.state, fragment);
                if length <= 1 {
                    continue;
                }
                let mut end_to_end = true;
                if arena[group].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL)
                    && arena[group].attribute(SUBTYPE_ATR) == Some(ALKANESTEM_SUBTYPE_VAL)
                    && let Some(previous_sub) =
                        arena[group].parent.and_then(|p| arena.previous_sibling(p))
                {
                    let previous_groups = arena.children_named(previous_sub, GROUP_EL);
                    if previous_groups.len() == 1 {
                        let previous = previous_groups[0];
                        if arena[previous].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL)
                            && arena[previous].attribute(SUBTYPE_ATR)
                                == Some(ALKANESTEM_SUBTYPE_VAL)
                            && arena
                                .next_sibling_named(previous, SUFFIX_EL)
                                .is_none_or(|s| {
                                    arena[s].fragment.is_none_or(|f| {
                                        self.state
                                            .fragment_manager
                                            .graph
                                            .fragment(f)
                                            .out_atoms
                                            .is_empty()
                                    })
                                })
                        {
                            end_to_end = false;
                        }
                    }
                }
                if end_to_end {
                    let mut parent = arena[group].parent.unwrap();
                    while arena[parent].name == BRACKET_EL {
                        parent = arena[parent].parent.unwrap();
                    }
                    if arena[parent].name != ROOT_EL {
                        let atom = locanted_atom(self.state, fragment, &length.to_string())?;
                        arena[group].set_attribute(DEFAULTINID_ATR, length.to_string());
                        self.state
                            .fragment_manager
                            .graph
                            .fragment_mut(fragment)
                            .default_in_atom = Some(atom);
                    }
                }
            }
        }
        Ok(())
    }
    /// Ordered procedural pass. The `process_components` wrapper also applies
    /// upstream's omitted-space correction after these per-word operations.
    pub fn process_parse(&mut self, arena: &mut Arena, parse: NodeId) -> Result<()> {
        fn children(arena: &Arena, node: NodeId) -> Vec<NodeId> {
            arena[node]
                .children
                .iter()
                .copied()
                .filter(|&n| {
                    matches!(
                        arena[n].name.as_str(),
                        ROOT_EL | SUBSTITUENT_EL | BRACKET_EL
                    )
                })
                .collect()
        }
        let words = arena.descendants_named(parse, WORD_EL);
        let word_count = words.len();
        for word in words.into_iter().rev() {
            let rule = arena
                .parent_word_rule(word)
                .ok_or_else(|| error("Word has no wordRule"))?;
            self.state.current_word_rule = arena[rule].attribute(WORDRULE_ATR).map(str::to_owned);
            if arena[word].attribute(TYPE_ATR) == Some("functionalTerm") {
                continue;
            }
            let roots = arena.descendants_named(word, ROOT_EL);
            if roots.len() > 1 {
                return Err(error(format!(
                    "Multiple roots, but only 0 or 1 were expected. Found: {}",
                    roots.len()
                )));
            }
            let mut substituents = arena.descendants_named(word, SUBSTITUENT_EL);
            let mut sub_and_root = substituents
                .iter()
                .chain(&roots)
                .copied()
                .collect::<Vec<_>>();
            let mut brackets = arena.descendants_named(word, BRACKET_EL);
            let mut all = sub_and_root
                .iter()
                .chain(&brackets)
                .copied()
                .collect::<Vec<_>>();
            let mut groups = arena.descendants_named(word, GROUP_EL);
            for &group in &groups {
                let fragment = self.resolve_group(arena, group)?;
                self.process_charge_and_oxidation_number_specification(arena, group, fragment)?;
            }
            for &sub in &sub_and_root {
                self.apply_dl_prefixes(arena, sub)?;
                crate::component_processor_carbohydrates::process_carbohydrates(
                    self.state, arena, sub,
                )?;
            }
            let mut final_sub = *arena[word]
                .children
                .last()
                .ok_or_else(|| error("Unable to find finalSubOrRootInWord"))?;
            while !matches!(arena[final_sub].name.as_str(), ROOT_EL | SUBSTITUENT_EL) {
                final_sub = *children(arena, final_sub)
                    .last()
                    .ok_or_else(|| error("Unable to find finalSubOrRootInWord"))?;
            }
            for &element in &all {
                self.determine_locant_meaning(arena, element, final_sub)?;
            }
            for &sub in &sub_and_root {
                self.process_multipliers(arena, sub)?;
                self.detect_conjunctive_suffix_groups(arena, sub, &mut groups)?;
                self.match_locants_to_direct_features(arena, sub)?;
                if let Some(&group) = arena.children_named(sub, GROUP_EL).last() {
                    let mut suffixes = arena.children_named(sub, SUFFIX_EL);
                    self.preliminary_process_suffixes(arena, group, &mut suffixes)?;
                }
            }
            for index in (0..substituents.len()).rev() {
                let sub = substituents[index];
                if !arena.children_named(sub, GROUP_EL).is_empty() {
                    continue;
                }
                let removed = self
                    .remove_and_move_to_appropriate_group_if_hydro_substituent(arena, sub)?
                    || Self::remove_and_move_to_appropriate_group_if_subtractive_prefix(
                        arena, sub,
                    )?
                    || self.remove_and_move_to_appropriate_group_if_ring_bridge(arena, sub)?;
                if !removed {
                    return Err(error(format!(
                        "OPSIN Bug: Encountered substituent with no group!: {}",
                        arena.to_xml(sub)
                    )));
                }
                substituents.remove(index);
                sub_and_root.retain(|&n| n != sub);
                all.retain(|&n| n != sub);
            }
            functional_replacement::process_acid_replacing_functional_class_nomenclature(
                self.state, arena, final_sub, word,
            )?;
            if functional_replacement::process_prefix_functional_replacement_nomenclature(
                self.state,
                arena,
                &mut groups,
                &mut substituents,
            )? {
                sub_and_root = substituents.iter().chain(&roots).copied().collect();
            }
            self.handle_group_irregularities(arena, &groups)?;
            for &sub in &sub_and_root {
                crate::component_processor_rings::process_hw(self.state, arena, sub)?;
                crate::fused_ring_builder::process_fused_rings(self.state, arena, sub)
                    .map_err(|e| error(e.to_string()))?;
                crate::component_processor_rings::process_fused_ring_bridges(
                    self.state, arena, sub,
                )?;
                crate::component_processor_rings::assign_element_symbol_locants(
                    self.state, arena, sub,
                )?;
                crate::component_processor_rings::process_ring_assemblies(
                    self.state,
                    arena,
                    self.suffix_rules,
                    sub,
                )?;
                crate::component_processor_rings::process_poly_cyclic_spiro_nomenclature(
                    self.state,
                    arena,
                    self.suffix_rules,
                    sub,
                )?;
            }
            for &sub in &sub_and_root {
                self.apply_lambda_convention(arena, sub)?;
                self.handle_multi_radicals(arena, sub)?;
            }
            self.add_implicit_brackets_to_amino_acids(arena, &groups, &mut brackets);
            for &sub in &substituents {
                self.match_locants_to_indirect_features(arena, sub)?;
                self.add_implicit_brackets_when_substituent_has_two_locants(
                    arena,
                    sub,
                    &mut brackets,
                );
                crate::component_processor_brackets::implicitly_bracket_to_previous_substituent_if_appropriate(self.state,arena,sub,&mut brackets)?;
            }
            for &root in &roots {
                self.match_locants_to_indirect_features(arena, root)?;
            }
            for &sub in &sub_and_root {
                self.assign_implicit_locants_to_di_terminal_suffixes(arena, sub)?;
                self.process_conjunctive_nomenclature(arena, sub)?;
                let group = arena
                    .first_child_named(sub, GROUP_EL)
                    .ok_or_else(|| error("No group after component processing"))?;
                let suffixes = arena.children_named(sub, SUFFIX_EL);
                SuffixApplier::new(self.state, self.suffix_rules)
                    .resolve_suffixes(arena, group, &suffixes)
                    .map_err(|e| error(e.to_string()))?;
                if arena[sub].name == SUBSTITUENT_EL {
                    self.move_substituent_detachable_het_atom_repl(arena, sub)?;
                }
            }
            self.move_erroneously_positioned_locants_and_multipliers(arena, &brackets)?;
            let mut elements = children(arena, word);
            if let Some(&first) = elements.first() {
                self.add_implicit_brackets_when_first_substituent_has_two_multipliers(
                    arena,
                    first,
                    &mut brackets,
                );
            }
            while elements.len() == 1 {
                elements = children(arena, elements[0]);
            }
            if let Some(&last) = elements.last() {
                self.assign_locants_to_multiplied_root_if_present(arena, last)?;
            }
            all = sub_and_root.iter().chain(&brackets).copied().collect();
            for &element in &all {
                self.assign_locants_and_multipliers(arena, element)?;
            }
            self.process_biochemical_linkage_descriptors(arena, &substituents, &mut brackets)?;
            self.process_word_level_multiplier_if_applicable(arena, word, &roots, word_count)?;
        }
        Ok(())
    }
}

/// Complete ComponentProcessor entry, including its final omitted-space corrector.
pub fn process_components(
    tree: &mut crate::parse_tree::ParseTree,
    state: &mut BuildState,
    rules: &SuffixRules,
) -> Result<()> {
    ComponentProcessor::new(state, rules).process_parse(&mut tree.arena, tree.root)?;
    crate::word_rules_omitted_space::correct_omitted_spaces(tree, state)
}

/// Source-method inventory. A mapping identifies folded helper bodies and the
/// three disjoint source families; it is a review ledger, not a completeness
/// claim about the entire OPSIN construction pipeline.
pub const SOURCE_METHOD_COVERAGE: &[(&str, &str)] = &[
    (
        "addSpecialHwRing",
        "component_processor_rings::special_hw_ring table",
    ),
    (
        "processParse",
        "component_processor::process_components + process_parse + correct_omitted_spaces",
    ),
    ("resolveGroup", "component_processor::resolve_group"),
    (
        "processXyleneLikeNomenclature",
        "component_processor::process_xylene_like_nomenclature",
    ),
    (
        "locantAreAcceptableForXyleneLikeNomenclatures",
        "component_processor::acceptable_front_locants",
    ),
    (
        "setFragmentDefaultInAtomIfSpecified",
        "component_processor::resolve_group: defaultInLocant/defaultInId",
    ),
    (
        "setFragmentFunctionalAtomsIfSpecified",
        "component_processor::resolve_group: functionalIds",
    ),
    (
        "applyTraditionalAlkaneNumberingIfAppropriate",
        "component_processor::apply_traditional_alkane_numbering",
    ),
    (
        "applyHomologyGroupLabelsIfSpecified",
        "component_processor::resolve_group: homology",
    ),
    (
        "processChargeAndOxidationNumberSpecification",
        "component_processor::process_charge_and_oxidation_number_specification",
    ),
    (
        "removeAndMoveToAppropriateGroupIfHydroSubstituent",
        "component_processor::remove_and_move_to_appropriate_group_if_hydro_substituent",
    ),
    (
        "removeAndMoveToAppropriateGroupIfSubtractivePrefix",
        "component_processor::remove_and_move_to_appropriate_group_if_subtractive_prefix",
    ),
    (
        "removeAndMoveToAppropriateGroupIfRingBridge",
        "component_processor::remove_and_move_to_appropriate_group_if_ring_bridge",
    ),
    (
        "containsCyclicAtoms",
        "component_processor::contains_cyclic_atoms",
    ),
    (
        "determineLocantMeaning",
        "component_processor::determine_locant_meaning",
    ),
    (
        "checkSpecialLocantUses",
        "component_processor::check_special_locant_uses",
    ),
    (
        "detectMultiplicativeNomenclature",
        "component_processor::detect_multiplicative_nomenclature",
    ),
    ("applyDLPrefixes", "component_processor::apply_dl_prefixes"),
    (
        "applyDlStereochemistryToAminoAcid",
        "component_processor::apply_dl_stereochemistry_to_amino_acid",
    ),
    (
        "applyDlStereochemistryToCarbohydrate",
        "component_processor::apply_dl_stereochemistry_to_carbohydrate",
    ),
    (
        "applyDlStereochemistryToCarbohydrateConfigurationalPrefix",
        "component_processor::apply_dl_stereochemistry_to_carbohydrate_configurational_prefix",
    ),
    (
        "processCarbohydrates",
        "component_processor_carbohydrates::process_carbohydrates",
    ),
    (
        "applyUnspecifiedRingSizeCyclisationIfPresent",
        "component_processor_carbohydrates::apply_unspecified_ring_size_cyclisation_if_present",
    ),
    (
        "processUloseSuffix",
        "component_processor_carbohydrates::process_ulose_suffix",
    ),
    (
        "cycliseCarbohydrateAndApplyAlphaBetaStereo",
        "component_processor_carbohydrates::cyclise_carbohydrate_and_apply_alpha_beta_stereo",
    ),
    (
        "applyAlphaBetaStereoToCyclisedCarbohydrate",
        "component_processor_carbohydrates::apply_alpha_beta_stereo_to_cyclised_carbohydrate",
    ),
    (
        "processAldoseDiSuffix",
        "component_processor_carbohydrates::process_aldose_di_suffix",
    ),
    (
        "getAnomericReferenceAtom",
        "component_processor_carbohydrates::get_anomeric_reference_atom",
    ),
    (
        "applyAnomerStereochemistryIfPresent",
        "component_processor_carbohydrates::apply_anomer_stereochemistry_if_present",
    ),
    (
        "getDeterministicAtomRefs4ForReferenceAtom",
        "component_processor_carbohydrates::get_deterministic_atom_refs4_for_reference_atom",
    ),
    (
        "getDeterministicAtomRefs4ForAnomericAtom",
        "component_processor_carbohydrates::get_deterministic_atom_refs4_for_anomeric_atom",
    ),
    (
        "processMultipliers",
        "component_processor::process_multipliers",
    ),
    (
        "detectConjunctiveSuffixGroups",
        "component_processor::detect_conjunctive_suffix_groups",
    ),
    (
        "matchLocantsToDirectFeatures",
        "component_processor::match_locants_to_direct_features",
    ),
    (
        "assignSingleLocantsToAdjacentFeatures",
        "component_processor::assign_single_locants_to_adjacent_features",
    ),
    (
        "preliminaryProcessSuffixes",
        "component_processor::preliminary_process_suffixes",
    ),
    (
        "applyDefaultLocantsToSuffixesIfApplicable",
        "component_processor::apply_default_suffix_locants",
    ),
    (
        "processSuffixAppliesTo",
        "component_processor::process_suffix_applies_to",
    ),
    (
        "resolveGroupAddingSuffixes",
        "component_processor::resolve_group_adding_suffixes",
    ),
    (
        "processRemovalOfHydroxyGroupsRules",
        "component_processor::process_removal_of_hydroxy_groups_rules",
    ),
    (
        "addFunctionalAtomsToHydroxyGroups",
        "component_processor::add_functional_atoms_to_hydroxy_groups",
    ),
    (
        "chargeHydroxyGroups",
        "component_processor::charge_hydroxy_groups",
    ),
    (
        "convertHydroxyGroupsToOutAtoms",
        "component_processor::convert_hydroxy_groups(true)",
    ),
    (
        "convertHydroxyGroupsToPositiveCharge",
        "component_processor::convert_hydroxy_groups(false)",
    ),
    (
        "processSuffixPrefixes",
        "component_processor::process_suffix_prefixes",
    ),
    (
        "checkLocantPresentOnPotentialRoot",
        "component_processor::check_locant_present_on_potential_root",
    ),
    (
        "handleGroupIrregularities",
        "component_processor::handle_group_irregularities",
    ),
    ("processHW", "component_processor_rings::process_hw"),
    (
        "assignElementSymbolLocants",
        "component_processor_rings::assign_element_symbol_locants",
    ),
    (
        "processRingAssemblies",
        "component_processor_rings::process_ring_assemblies",
    ),
    (
        "determineElementsToResolveIntoRingAssembly",
        "component_processor_rings::determine_elements_to_resolve_into_ring_assembly",
    ),
    (
        "processPolyCyclicSpiroNomenclature",
        "component_processor_rings::process_poly_cyclic_spiro_nomenclature",
    ),
    (
        "processNonIdenticalPolyCyclicSpiro",
        "component_processor_rings::process_non_identical_poly_cyclic_spiro",
    ),
    (
        "processOldMethodPolyCyclicSpiro",
        "component_processor_rings::process_old_method_poly_cyclic_spiro",
    ),
    (
        "processSpiroBiOrTer",
        "component_processor_rings::process_spiro_bi_or_ter",
    ),
    (
        "processDispiroter",
        "component_processor_rings::process_dispiroter",
    ),
    (
        "determineFeaturesToResolveInSingleComponentSpiro",
        "component_processor_rings::determine_features_to_resolve_in_single_component_spiro",
    ),
    (
        "resolveFeaturesOntoGroup",
        "component_processor_rings::resolve_features_onto_group",
    ),
    (
        "compare",
        "component_processor_rings::process_fused_ring_bridges comparator",
    ),
    (
        "processFusedRingBridges",
        "component_processor_rings::process_fused_ring_bridges",
    ),
    (
        "getLocantNumber",
        "component_processor_rings::locant_number",
    ),
    (
        "getHighestNumericLocant",
        "component_processor_rings::highest_numeric_locant",
    ),
    (
        "applyLambdaConvention",
        "component_processor::apply_lambda_convention",
    ),
    (
        "handleMultiRadicals",
        "component_processor::handle_multi_radicals",
    ),
    (
        "unsuitableForFormingChainMultiradical",
        "component_processor::unsuitable_for_forming_chain_multiradical",
    ),
    (
        "calculateOutAtomsToBeAddedFromInlineSuffixes",
        "component_processor::calculate_out_atoms_to_be_added_from_inline_suffixes",
    ),
    (
        "addImplicitBracketsToAminoAcids",
        "component_processor::add_implicit_brackets_to_amino_acids",
    ),
    (
        "implicitlyBracketToPreviousSubstituentIfAppropriate",
        "component_processor_brackets::implicitly_bracket_to_previous_substituent_if_appropriate",
    ),
    (
        "substituentsAreEndToEndAlkyls",
        "component_processor_brackets::substituents_are_end_to_end_alkyls",
    ),
    (
        "fragHasLocants",
        "component_processor_brackets::frag_has_locants",
    ),
    (
        "isPotentialAlkyl",
        "component_processor_brackets::is_potential_alkyl",
    ),
    (
        "isSimpleAlkyl",
        "component_processor_brackets::is_simple_alkyl",
    ),
    (
        "isSimpleAlkane",
        "component_processor_brackets::is_simple_alkane",
    ),
    (
        "implicitBracketWouldPreventAdditiveBonding",
        "component_processor_brackets::implicit_bracket_would_prevent_additive_bonding",
    ),
    (
        "implicitBracketWouldPreventConnectionToAmineSuffix",
        "component_processor_brackets::implicit_bracket_would_prevent_connection_to_amine_suffix",
    ),
    (
        "determineSubstituentForMultiSubstituentImplicitBracketting",
        "component_processor_brackets::determine_substituent_for_multi_substituent_implicit_bracketting",
    ),
    (
        "locantedEsterImplicitBracketSpecialCase",
        "component_processor_brackets::locanted_ester_implicit_bracket_special_case",
    ),
    (
        "matchLocantsToIndirectFeatures",
        "component_processor::match_locants_to_indirect_features",
    ),
    (
        "findLocantsThatCouldBeIndirectLocants",
        "component_processor::match_locants_to_indirect_features: prefix selector",
    ),
    (
        "findElementsMissingIndirectLocants",
        "component_processor::find_elements_missing_indirect_locants",
    ),
    (
        "assignImplicitLocantsToDiTerminalSuffixes",
        "component_processor::assign_implicit_locants_to_di_terminal_suffixes",
    ),
    (
        "isATerminalSuffix",
        "component_processor::assign_implicit_locants_to_di_terminal_suffixes: terminal",
    ),
    (
        "processConjunctiveNomenclature",
        "component_processor::process_conjunctive_nomenclature",
    ),
    (
        "processBiochemicalLinkageDescriptors",
        "component_processor::process_biochemical_linkage_descriptors",
    ),
    (
        "isSubBracketOrRoot",
        "component_processor::process_biochemical_linkage_descriptors: scope predicate",
    ),
    (
        "checkAndApplyFirstLocantOfBiochemicalLinkage",
        "component_processor::check_and_apply_first_locant_of_biochemical_linkage",
    ),
    (
        "moveSubstituentDetachableHetAtomRepl",
        "component_processor::move_substituent_detachable_het_atom_repl",
    ),
    (
        "moveErroneouslyPositionedLocantsAndMultipliers",
        "component_processor::move_erroneously_positionedlocants_and_multipliers",
    ),
    (
        "addImplicitBracketsWhenFirstSubstituentHasTwoMultipliers",
        "component_processor::add_implicit_brackets_when_first_substituent_has_two_multipliers",
    ),
    (
        "assignLocantsToMultipliedRootIfPresent",
        "component_processor::assign_locants_to_multiplied_root_if_present",
    ),
    (
        "addImplicitBracketsWhenSubstituentHasTwoLocants",
        "component_processor::add_implicit_brackets_when_substituent_has_two_locants",
    ),
    (
        "getLocantsAtStartOfSubstituent",
        "component_processor::locants_at_start",
    ),
    (
        "locantsAreSingular",
        "component_processor::add_implicit_brackets_when_substituent_has_two_locants: singular check",
    ),
    (
        "assignLocantsAndMultipliers",
        "component_processor::assign_locants_and_multipliers",
    ),
    ("locantsToDebugString", "component_processor::locants_debug"),
    (
        "wordLevelLocantsAllowed",
        "component_processor::wordlevel_locants_allowed",
    ),
    (
        "processWordLevelMultiplierIfApplicable",
        "component_processor::process_wordlevel_multiplier_if_applicable",
    ),
    (
        "checkForNonConfusedWithNona",
        "component_processor::process_word_level_multiplier_if_applicable: non/nona check",
    ),
    (
        "isMonoFollowedByElement",
        "component_processor::process_word_level_multiplier_if_applicable: mono element check",
    ),
];
