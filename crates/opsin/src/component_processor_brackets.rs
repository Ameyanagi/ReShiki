//! OPSIN's preceding-substituent implicit bracket rules.
//!
//! Faithful translation of `ComponentProcessor.java` lines 4191–4650 at
//! OPSIN 2.9.0 commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and contributors, MIT (see the retained license).

use std::sync::LazyLock;

use regex::Regex;

use crate::api::ParsingError;
use crate::build_state::BuildState;
use crate::component_processor::ComponentProcessor;
use crate::fragment_tools::calculate_substitutable_hydrogen_atoms;
use crate::graph::{Element, FragmentId};
use crate::parse_tree::{Arena, NodeId};
use crate::xml_declarations::*;

static ELEMENT_SYMBOL_OR_AMINO_ACID_LOCANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Z][a-z]?'*([0-9]+[a-z]?'*)?")
        .expect("OPSIN element or amino-acid locant pattern")
});

pub fn implicitly_bracket_to_previous_substituent_if_appropriate(
    state: &mut BuildState,
    arena: &mut Arena,
    substituent: NodeId,
    brackets: &mut Vec<NodeId>,
) -> Result<(), ParsingError> {
    let first = first_child(arena, substituent)?;
    if matches!(
        arena[first].name.as_str(),
        LOCANT_EL | MULTIPLIER_EL | STEREOCHEMISTRY_EL
    ) {
        return Ok(());
    }
    let group = arena
        .first_child_named(substituent, GROUP_EL)
        .ok_or_else(|| error("No group where group was expected"))?;
    if arena[group].attribute(USABLEASJOINER_ATR).is_none() {
        return Ok(());
    }
    let Some(mut before) = arena
        .previous_sibling(substituent)
        .filter(|&id| matches!(arena[id].name.as_str(), SUBSTITUENT_EL | BRACKET_EL))
    else {
        return Ok(());
    };
    let fragment = fragment(arena, group)?;
    let graph = &state.fragment_manager.graph;
    let expected_substituents = if arena[group].attribute(ACCEPTSADDITIVEBONDS_ATR).is_some() {
        graph.fragment(fragment).out_atoms.len() as i32 - 1
    } else {
        0
    };
    let after = arena.next_sibling(substituent);
    if let Some(after) = after
        && expected_substituents == 0
        && arena[before].name == BRACKET_EL
        && arena[before].attribute(TYPE_ATR) != Some(IMPLICIT_TYPE_VAL)
        && arena[after].name == BRACKET_EL
    {
        let first_after = first_child(arena, after)?;
        if matches!(
            arena[first_after].name.as_str(),
            SUBSTITUENT_EL | BRACKET_EL
        ) && arena
            .previous_element(first_after, true)
            .is_none_or(|id| arena[id].name != HYPHEN_EL)
        {
            return Ok(());
        }
    }
    if !is_sub_bracket_or_root(arena, after)
        && !(after.is_none()
            && locanted_ester_implicit_bracket_special_case(state, arena, substituent, before))
    {
        return Ok(());
    }
    if expected_substituents == 0
        && arena
            .previous_element(first, true)
            .is_some_and(|id| arena[id].name == HYPHEN_EL)
    {
        return Ok(());
    }
    let preceding_groups = arena.descendants_named(before, GROUP_EL);
    let preceding_group = *preceding_groups
        .last()
        .ok_or_else(|| error("No group where group was expected"))?;
    if substituents_are_end_to_end_alkyls(state, arena, group, preceding_group, before)? {
        return Ok(());
    }
    if arena[preceding_group]
        .attribute(ISAMULTIRADICAL_ATR)
        .is_some()
        && arena[preceding_group]
            .attribute(ACCEPTSADDITIVEBONDS_ATR)
            .is_none()
        && arena[preceding_group].attribute(IMINOLIKE_ATR).is_none()
    {
        return Ok(());
    }
    if arena[group].attribute(ISAMULTIRADICAL_ATR).is_some() {
        if arena[group].attribute(ACCEPTSADDITIVEBONDS_ATR).is_none()
            && arena[group].attribute(IMINOLIKE_ATR).is_none()
        {
            return Ok(());
        }
        let substitutable = graph
            .fragment(fragment)
            .atoms
            .iter()
            .any(|&atom| calculate_substitutable_hydrogen_atoms(graph, atom) > 0);
        if !substitutable && let Some(after) = after {
            let first_after = first_child(arena, after)?;
            if arena[first_after].name == MULTIPLIER_EL
                && graph.fragment(fragment).out_atoms.len()
                    == multiplier(arena, first_after)? as usize
            {
                if arena[after].name == ROOT_EL {
                    return Ok(());
                }
                if arena[after].name == SUBSTITUENT_EL
                    && arena
                        .descendants_named(after, GROUP_EL)
                        .iter()
                        .any(|&id| arena[id].attribute(ISAMULTIRADICAL_ATR).is_some())
                {
                    return Ok(());
                }
                if arena[after].name == BRACKET_EL
                    && !arena.descendants_named(after, ROOT_EL).is_empty()
                {
                    return Ok(());
                }
            }
        }
    }
    if arena[preceding_group].attribute(IMINOLIKE_ATR).is_some()
        && arena[group].attribute(IMINOLIKE_ATR).is_some()
    {
        return Ok(());
    }
    if implicit_bracket_would_prevent_additive_bonding(state, arena, before, after)?
        || implicit_bracket_would_prevent_connection_to_amine_suffix(arena, before, after)?
    {
        return Ok(());
    }
    if arena.value(group) == "sulf"
        && graph.fragment(fragment).atoms.len() == 1
        && arena
            .next_sibling_ignoring(group, &[UNSATURATOR_EL])
            .is_some_and(|id| arena[id].attribute(VALUE_ATR) == Some("ylidene"))
    {
        arena[group].remove_attribute(USABLEASJOINER_ATR);
        return Ok(());
    }
    let per_halo = arena[preceding_group].attribute(SUBTYPE_ATR) == Some(PERHALOGENO_SUBTYPE_VAL);
    if per_halo {
        let unlocanted = arena
            .previous_sibling(preceding_group)
            .is_none_or(|id| arena[id].name != LOCANT_EL);
        let alkyl_alkane_link =
            is_simple_alkyl(arena, group) && after.is_some_and(|id| is_simple_alkane(arena, id));
        if unlocanted && !alkyl_alkane_link {
            return Ok(());
        }
    }
    if graph.fragment(fragment).atoms.len() == 1
        && graph.atom(graph.fragment(fragment).atoms[0]).element == Element::Si
    {
        before =
            determine_substituent_for_multi_substituent_implicit_bracketting(arena, before, 3)?;
    } else if expected_substituents > 1 {
        before = determine_substituent_for_multi_substituent_implicit_bracketting(
            arena,
            before,
            expected_substituents,
        )?;
    }

    let children_before = arena[before].children.clone();
    let mut locant_related = Vec::new();
    let mut locant_values: Option<Vec<String>> = None;
    let mut stereochemistry = Vec::new();
    for &child in &children_before {
        match arena[child].name.as_str() {
            STEREOCHEMISTRY_EL => stereochemistry.push(child),
            LOCANT_EL => {
                if locant_values.is_some() {
                    break;
                }
                locant_related.push(child);
                locant_values = Some(
                    split_java(&arena.value(child))
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                );
            }
            _ => break,
        }
    }
    let mut move_locants = false;
    if let Some(locants) = &locant_values {
        let after_locant = arena.next_sibling(locant_related[0]);
        for locant in locants {
            let has_locant = graph.atom_by_locant(fragment, locant).is_some();
            if graph.fragment(fragment).atoms.len() == 1
                || !has_locant
                || ELEMENT_SYMBOL_OR_AMINO_ACID_LOCANT.is_match(locant)
                || (locants.len() == 1
                    && after_locant.is_some_and(|id| arena[id].name == MULTIPLIER_EL))
            {
                if ComponentProcessor::check_locant_present_on_potential_root(
                    state,
                    arena,
                    substituent,
                    locant,
                )? {
                    move_locants = true;
                    break;
                }
                if !(graph.fragment(fragment).atoms.len() == 1 && has_locant) {
                    move_locants = true;
                    break;
                }
            }
        }
        if per_halo {
            move_locants = true;
        }
        if move_locants && locants.len() > 1 {
            if let Some(after_locant) = after_locant.filter(|&id| arena[id].name == MULTIPLIER_EL) {
                if let Some(following) = arena.next_sibling_ignoring(after_locant, &[MULTIPLIER_EL])
                {
                    if (arena[following].name == GROUP_EL
                        && arena[after_locant].attribute(TYPE_ATR) == Some(GROUP_TYPE_VAL))
                        || expected_substituents == 1
                    {
                        locant_related.push(after_locant);
                    } else if arena[locant_related[0]].attribute(TYPE_ATR)
                        == Some(ORTHOMETAPARA_TYPE_VAL)
                    {
                        arena[locant_related[0]].set_value(&locants[1]);
                    } else if graph.fragment(fragment).atoms.len() == 1 {
                        locant_related.push(after_locant);
                    } else {
                        return Ok(());
                    }
                } else {
                    move_locants = false;
                }
            } else {
                move_locants = false;
            }
        }
    }
    let bracket = arena.grouping(BRACKET_EL);
    arena[bracket].add_attribute(TYPE_ATR, IMPLICIT_TYPE_VAL);
    for stereo in stereochemistry {
        arena.detach(stereo);
        arena.add_child(bracket, stereo);
    }
    if move_locants {
        for &locant in &locant_related {
            arena.detach(locant);
            arena.add_child(bracket, locant);
        }
    }
    if locant_related.is_empty() {
        let possible_multiplier = *children_before
            .first()
            .ok_or_else(|| error("OPSIN Bug: Substituent has no children"))?;
        if arena[possible_multiplier].name == MULTIPLIER_EL
            && (expected_substituents == 1
                || arena[possible_multiplier].attribute(TYPE_ATR) == Some(GROUP_TYPE_VAL))
            && arena
                .next_sibling_ignoring(possible_multiplier, &[MULTIPLIER_EL])
                .is_some_and(|id| arena[id].name == GROUP_EL)
        {
            arena.detach(possible_multiplier);
            arena.add_child(bracket, possible_multiplier);
        }
    }
    let parent = arena[substituent]
        .parent
        .ok_or_else(|| error("OPSIN Bug: Substituent has no parent"))?;
    let start = arena
        .index_of(parent, before)
        .ok_or_else(|| error("OPSIN Bug: Implicit bracket start not in parent"))?;
    let end = arena
        .index_of(parent, substituent)
        .ok_or_else(|| error("OPSIN Bug: Implicit bracket end not in parent"))?;
    for _ in start..=end {
        let child = arena.remove_child_at(parent, start);
        arena.add_child(bracket, child);
    }
    arena.insert_child(parent, bracket, start);
    brackets.push(bracket);
    Ok(())
}

fn substituents_are_end_to_end_alkyls(
    state: &BuildState,
    arena: &Arena,
    group: NodeId,
    preceding_group: NodeId,
    preceding_substituent: NodeId,
) -> Result<bool, ParsingError> {
    if !is_potential_alkyl(arena, group) || !is_potential_alkyl(arena, preceding_group) {
        return Ok(false);
    }
    if arena
        .next_sibling_named(preceding_group, SUFFIX_EL)
        .and_then(|id| arena[id].fragment)
        .is_some_and(|fragment| {
            !state
                .fragment_manager
                .graph
                .fragment(fragment)
                .out_atoms
                .is_empty()
        })
    {
        return Ok(false);
    }
    let mut locant = None;
    let mut preceding_multiplier = None;
    for &child in &arena[preceding_substituent].children {
        if arena[child].name == LOCANT_EL {
            locant = Some(child);
            preceding_multiplier = arena
                .next_sibling(child)
                .filter(|&id| arena[id].name == MULTIPLIER_EL);
            break;
        }
        if arena[child].name != STEREOCHEMISTRY_EL {
            break;
        }
    }
    if let Some(locant) = locant {
        let value = arena.value(locant);
        let locants = split_java(&value);
        if !frag_has_locants(state, fragment(arena, group)?, &locants)
            && (preceding_multiplier.is_none()
                || multiplier(arena, preceding_multiplier.unwrap())? == locants.len() as i32)
            && is_simple_alkyl(arena, group)
            && is_simple_alkyl(arena, preceding_group)
        {
            return Ok(true);
        }
        if let Some(multiplier_node) = preceding_multiplier
            && multiplier(arena, multiplier_node)? == 2
            && let Some(next_group) = next_group(arena, group)
        {
            let next_fragment = fragment(arena, next_group)?;
            let graph = &state.fragment_manager.graph;
            if graph.fragment(next_fragment).atoms.len() == 1
                && graph.atom(graph.fragment(next_fragment).atoms[0]).element == Element::Si
            {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(true)
}

fn frag_has_locants(state: &BuildState, fragment: FragmentId, locants: &[&str]) -> bool {
    locants.iter().all(|locant| {
        state
            .fragment_manager
            .graph
            .atom_by_locant(fragment, locant)
            .is_some()
    })
}

fn is_potential_alkyl(arena: &Arena, group: NodeId) -> bool {
    (arena[group].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL)
        && arena[group].attribute(SUBTYPE_ATR) == Some(ALKANESTEM_SUBTYPE_VAL))
        || arena[group].attribute(TYPE_ATR) == Some(ACIDSTEM_TYPE_VAL)
}

fn is_simple_alkyl(arena: &Arena, group: NodeId) -> bool {
    arena[group].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL)
        && arena[group].attribute(SUBTYPE_ATR) == Some(ALKANESTEM_SUBTYPE_VAL)
        && arena
            .next_sibling_named(group, SUFFIX_EL)
            .is_some_and(|id| arena.value(id) == "yl")
}

fn is_simple_alkane(arena: &Arena, root: NodeId) -> bool {
    if arena[root].name != ROOT_EL || arena[root].children.len() != 2 {
        return false;
    }
    let group = arena[root].children[0];
    let suffix = arena[root].children[1];
    arena[group].name == GROUP_EL
        && arena[group].attribute(TYPE_ATR) == Some(CHAIN_TYPE_VAL)
        && arena[group].attribute(SUBTYPE_ATR) == Some(ALKANESTEM_SUBTYPE_VAL)
        && arena[suffix].name == UNSATURATOR_EL
        && arena[suffix].attribute(VALUE_ATR) == Some("1")
}

fn implicit_bracket_would_prevent_additive_bonding(
    state: &BuildState,
    arena: &Arena,
    before: NodeId,
    after: Option<NodeId>,
) -> Result<bool, ParsingError> {
    if let Some(after) = after.filter(|&id| arena[id].name == SUBSTITUENT_EL) {
        let group_after = arena
            .first_child_named(after, GROUP_EL)
            .ok_or_else(|| error("No group where group was expected"))?;
        if arena[group_after]
            .attribute(ACCEPTSADDITIVEBONDS_ATR)
            .is_some()
            && !is_sub_bracket_or_root(arena, arena.next_sibling(after))
            && arena[first_child(arena, before)?].name == LOCANT_EL
        {
            let fragment = fragment(arena, group_after)?;
            let graph = &state.fragment_manager.graph;
            let first_atom = graph
                .fragment(fragment)
                .atoms
                .first()
                .copied()
                .ok_or_else(|| error("OPSIN Bug: Fragment has no atoms"))?;
            let mut viable = Some(before);
            while let Some(id) = viable {
                if matches!(arena[id].name.as_str(), SUBSTITUENT_EL | BRACKET_EL) {
                    let possible_locant = first_child(arena, id)?;
                    if arena[possible_locant].name == LOCANT_EL
                        && graph.atom_by_locant(fragment, &arena.value(possible_locant))
                            == Some(first_atom)
                    {
                        return Ok(false);
                    }
                }
                viable = arena.previous_sibling(id);
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn implicit_bracket_would_prevent_connection_to_amine_suffix(
    arena: &Arena,
    before: NodeId,
    after: Option<NodeId>,
) -> Result<bool, ParsingError> {
    if let Some(after) =
        after.filter(|&id| arena[id].name == ROOT_EL && arena[id].children.len() == 1)
        && matches!(arena.value(after).as_str(), "amine" | "amin")
        && arena[first_child(arena, before)?].name == LOCANT_EL
    {
        return Ok(true);
    }
    Ok(false)
}

fn determine_substituent_for_multi_substituent_implicit_bracketting(
    arena: &Arena,
    original: NodeId,
    mut expected: i32,
) -> Result<NodeId, ParsingError> {
    let mut current = Some(original);
    while let Some(id) =
        current.filter(|&id| matches!(arena[id].name.as_str(), SUBSTITUENT_EL | BRACKET_EL))
    {
        let mut index = 0;
        let mut child = arena[id].children.get(index).copied();
        index += 1;
        let locant_present = child.is_some_and(|id| arena[id].name == LOCANT_EL);
        if locant_present {
            child = arena[id].children.get(index).copied();
            index += 1;
        }
        let mut count = 1;
        if let Some(multiplier_node) = child.filter(|&id| arena[id].name == MULTIPLIER_EL) {
            count = multiplier(arena, multiplier_node)?;
            child = arena[id].children.get(index).copied();
        }
        if child.is_none_or(|id| {
            !matches!(
                arena[id].name.as_str(),
                GROUP_EL | SUBSTITUENT_EL | BRACKET_EL
            )
        }) {
            return Ok(original);
        }
        expected -= count;
        if expected <= 0 {
            return Ok(if expected < 0 { original } else { id });
        }
        if locant_present {
            return Ok(original);
        }
        current = arena.previous_sibling(id);
    }
    Ok(original)
}

fn locanted_ester_implicit_bracket_special_case(
    state: &BuildState,
    arena: &Arena,
    substituent: NodeId,
    before: NodeId,
) -> bool {
    arena[substituent]
        .parent
        .is_some_and(|id| arena[id].name == WORD_EL)
        && arena.previous_sibling(before).is_none()
        && matches!(
            state.current_word_rule.as_deref(),
            Some("ester" | "functionalClassEster" | "multiEster" | "acetal")
        )
}

fn is_sub_bracket_or_root(arena: &Arena, element: Option<NodeId>) -> bool {
    element.is_some_and(|id| {
        matches!(
            arena[id].name.as_str(),
            SUBSTITUENT_EL | BRACKET_EL | ROOT_EL
        )
    })
}

/// OpsinTools.getNextGroup starts beyond the containing substituent/root when
/// given a group, rather than scanning further groups within that same scope.
fn next_group(arena: &Arena, starting: NodeId) -> Option<NodeId> {
    let current = if arena[starting].name == GROUP_EL {
        arena[starting].parent?
    } else {
        starting
    };
    let parent = arena[current].parent?;
    if arena[parent].name == MOLECULE_EL {
        return None;
    }
    let index = arena.index_of(parent, current)?;
    let Some(mut next) = arena[parent].children.get(index + 1).copied() else {
        return next_group(arena, parent);
    };
    while let Some(&child) = arena[next].children.first() {
        next = child;
    }
    let next_parent = arena[next].parent?;
    arena
        .first_child_named(next_parent, GROUP_EL)
        .or_else(|| next_group(arena, next))
}

fn fragment(arena: &Arena, group: NodeId) -> Result<FragmentId, ParsingError> {
    arena[group]
        .fragment
        .ok_or_else(|| error("OPSIN Bug: Group has no fragment"))
}
fn first_child(arena: &Arena, element: NodeId) -> Result<NodeId, ParsingError> {
    arena[element]
        .children
        .first()
        .copied()
        .ok_or_else(|| error("OPSIN Bug: Substituent or bracket has no children"))
}
fn multiplier(arena: &Arena, element: NodeId) -> Result<i32, ParsingError> {
    arena[element]
        .attribute(VALUE_ATR)
        .ok_or_else(|| error("OPSIN Bug: Multiplier has no value"))?
        .parse()
        .map_err(|_| error("OPSIN Bug: Multiplier value is not an integer"))
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
fn error(message: &str) -> ParsingError {
    ParsingError(message.into())
}
