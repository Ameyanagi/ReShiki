//! Ring-family passes from OPSIN `ComponentProcessor.java` (2826–3924).
//!
//! Source: OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
//! Inventory: processHW; assignElementSymbolLocants; processRingAssemblies;
//! determineElementsToResolveIntoRingAssembly; processPolyCyclicSpiroNomenclature;
//! processNonIdenticalPolyCyclicSpiro; processOldMethodPolyCyclicSpiro;
//! processSpiroBiOrTer; processDispiroter;
//! determineFeaturesToResolveInSingleComponentSpiro; resolveFeaturesOntoGroup;
//! SortBridgesByHighestLocantedBridgehead; processFusedRingBridges;
//! getLocantNumber; getHighestNumericLocant. The special HW table comes from
//! the same class's static initializer (lines 65–119).

use crate::build_state::BuildState;
use crate::graph::{AtomId, Element as ChemEl, FragmentId, GraphError};
use crate::parse_tree::{Arena, NodeId};
use crate::suffix_applier::SuffixApplier;
use crate::suffix_rules::SuffixRules;
use crate::xml_declarations::*;
use crate::{ParsingError, ambiguity, fragment_tools};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

type Result<T> = std::result::Result<T, ParsingError>;
fn error(message: impl Into<String>) -> ParsingError {
    ParsingError(message.into())
}
fn graph_error(error: GraphError) -> ParsingError {
    ParsingError(error.to_string())
}
fn fragment(arena: &Arena, node: NodeId) -> Result<FragmentId> {
    arena[node]
        .fragment
        .ok_or_else(|| error(format!("No fragment associated with {}", arena.value(node))))
}
fn number(value: &str) -> Result<usize> {
    value
        .parse()
        .map_err(|_| error(format!("Expected integer, found: {value}")))
}
fn locanted_atom(state: &BuildState, fragment: FragmentId, locant: &str) -> Result<AtomId> {
    state
        .graph()
        .atom_by_locant(fragment, locant)
        .ok_or_else(|| error(format!("No atom with locant {locant}")))
}
fn next_named(arena: &Arena, node: NodeId, name: &str) -> Option<NodeId> {
    let mut next = arena.next_sibling(node);
    while let Some(id) = next {
        if arena[id].name == name {
            return Some(id);
        }
        next = arena.next_sibling(id);
    }
    None
}
fn siblings_until(arena: &Arena, node: NodeId, name: &str) -> Vec<NodeId> {
    let mut elements = Vec::new();
    let mut next = arena.next_sibling(node);
    while let Some(id) = next {
        if arena[id].name == name {
            break;
        }
        elements.push(id);
        next = arena.next_sibling(id);
    }
    elements
}
fn heteroatom_element(arena: &Arena, heteroatom: NodeId) -> Result<ChemEl> {
    static ELEMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new("[A-Z][a-z]?").unwrap());
    let value = arena[heteroatom].attribute(VALUE_ATR).unwrap_or("");
    ELEMENT
        .find(value)
        .and_then(|m| ChemEl::from_symbol(m.as_str()))
        .ok_or_else(|| error("Failed to extract element from Hantzsch-Widman heteroatom"))
}

#[derive(Clone, Copy)]
enum HwInstruction {
    Blocked,
    Saturated,
    NotIcAcid,
    NotNothingOrOlate,
    AsStandaloneRing,
    None,
}
fn special_hw_ring(name: &str) -> Option<(HwInstruction, &'static [ChemEl])> {
    use ChemEl::*;
    use HwInstruction as I;
    Some(match name {
        "oxin" | "azin" => (I::Blocked, &[]),
        "selenin" => (I::NotIcAcid, &[Se, C, C, C, C, C]),
        "tellurin" => (I::NotIcAcid, &[Te, C, C, C, C, C]),
        "thiol" => (I::NotNothingOrOlate, &[S, C, C, C, C]),
        "selenol" => (I::NotNothingOrOlate, &[Se, C, C, C, C]),
        "tellurol" => (I::NotNothingOrOlate, &[Te, C, C, C, C]),
        "oxazol" | "oxazolidin" | "oxazolid" | "oxazolin" => (I::None, &[O, C, N, C, C]),
        "thiazol" | "thiazolidin" | "thiazolid" | "thiazolin" => (I::None, &[S, C, N, C, C]),
        "selenazol" | "selenazolidin" | "selenazolid" | "selenazolin" => {
            (I::None, &[Se, C, N, C, C])
        }
        "tellurazol" | "tellurazolidin" | "tellurazolid" | "tellurazolin" => {
            (I::None, &[Te, C, N, C, C])
        }
        "azazazin" => (I::AsStandaloneRing, &[N, C, N, C, N, C]),
        "oxoxolan" | "oxoxol" => (I::None, &[O, C, O, C, C]),
        "oxoxan" | "oxoxin" => (I::None, &[O, C, C, O, C, C]),
        "oxoxoxan" => (I::AsStandaloneRing, &[O, C, O, C, O, C]),
        "boroxin" => (I::Saturated, &[O, B, O, B, O, B]),
        "borazin" => (I::Saturated, &[N, B, N, B, N, B]),
        "borthiin" => (I::Saturated, &[S, B, S, B, S, B]),
        _ => return None,
    })
}

/// Hantzsch–Widman replacement, default locants, delta bonds and name checks.
pub fn process_hw(state: &mut BuildState, arena: &mut Arena, sub_or_root: NodeId) -> Result<()> {
    let groups: Vec<_> = arena
        .children_named(sub_or_root, GROUP_EL)
        .into_iter()
        .filter(|&id| arena[id].attribute(SUBTYPE_ATR) == Some(HANTZSCHWIDMAN_SUBTYPE_VAL))
        .collect();
    for group in groups {
        let ring = fragment(arena, group)?;
        let atoms = state.graph().fragment(ring).atoms.clone();
        let mut no_locants = true;
        let mut heteroatoms = Vec::new();
        let mut previous = arena.previous_sibling(group);
        while let Some(id) = previous.filter(|&id| arena[id].name == HETEROATOM_EL) {
            heteroatoms.push(id);
            if arena[id].attribute(LOCANT_ATR).is_some() {
                no_locants = false;
            }
            previous = arena.previous_sibling(id);
        }
        heteroatoms.reverse();
        if atoms.len() == 6 && arena.value(group) == "an" {
            let mut nitrogen = false;
            let mut si_ge_sn_pb = false;
            for &heteroatom in &heteroatoms {
                let element = heteroatom_element(arena, heteroatom)?;
                nitrogen |= element == ChemEl::N;
                si_ge_sn_pb |= matches!(element, ChemEl::Si | ChemEl::Ge | ChemEl::Sn | ChemEl::Pb);
            }
            if atoms
                .iter()
                .all(|&id| !state.graph().atom(id).spare_valency)
                && !nitrogen
                && si_ge_sn_pb
            {
                return Err(error(
                    "Blocked Hantzsch-Widman system (6 member saturated ring with no nitrogen but has Si/Ge/Sn/Pb)",
                ));
            }
        }
        let mut name = String::new();
        for &heteroatom in &heteroatoms {
            let value = arena.value(heteroatom);
            name.push_str(value.strip_suffix('a').unwrap_or(&value));
        }
        name.push_str(&arena.value(group));
        arena[group].set_value(name.clone());
        if no_locants
            && !heteroatoms.is_empty()
            && let Some((instruction, elements)) = special_hw_ring(&name)
        {
            let mut apply_defaults = true;
            match instruction {
                HwInstruction::Blocked => return Err(error("Blocked Hantzsch-Widman system")),
                HwInstruction::Saturated => {
                    for &atom in &atoms {
                        state.graph_mut().atom_mut(atom).spare_valency = false;
                    }
                }
                HwInstruction::NotIcAcid => {
                    if arena[group]
                        .attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR)
                        .is_none()
                        && let Some(next) = arena.next_sibling(group).filter(|&id| {
                            arena[id].name == SUFFIX_EL
                                && arena[id].attribute(LOCANT_ATR).is_none()
                                && arena[id].attribute(VALUE_ATR) == Some("ic")
                        })
                    {
                        return Err(error(format!(
                            "{}{} appears to be a generic class name, not a Hantzsch-Widman ring",
                            name,
                            arena.value(next)
                        )));
                    }
                }
                HwInstruction::NotNothingOrOlate => {
                    if arena[group]
                        .attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR)
                        .is_none()
                    {
                        let next = arena.next_sibling(group);
                        if next.is_none()
                            || next.is_some_and(|id| {
                                arena[id].name == SUFFIX_EL
                                    && arena[id].attribute(LOCANT_ATR).is_none()
                                    && arena[id].attribute(VALUE_ATR) == Some("ate")
                            })
                        {
                            return Err(error(format!(
                                "{name} has the syntax for a Hantzsch-Widman ring but probably does not mean that in this context"
                            )));
                        }
                    }
                }
                HwInstruction::AsStandaloneRing => {
                    let parent = arena[group]
                        .parent
                        .ok_or_else(|| error("Hantzsch-Widman group has no parent"))?;
                    if arena.children_named(parent, GROUP_EL).len() > 1 {
                        apply_defaults = false;
                    }
                }
                HwInstruction::None => {}
            }
            if apply_defaults {
                for (index, &element) in elements.iter().enumerate() {
                    let atom = locanted_atom(state, ring, &(index + 1).to_string())?;
                    state.graph_mut().atom_mut(atom).element = element;
                }
                for heteroatom in heteroatoms.drain(..) {
                    arena.detach(heteroatom);
                }
            }
        }
        let mut unlocanted = Vec::new();
        for heteroatom in heteroatoms {
            let Some(locant) = arena[heteroatom].attribute(LOCANT_ATR) else {
                unlocanted.push(heteroatom);
                continue;
            };
            let element = heteroatom_element(arena, heteroatom)?;
            let atom = locanted_atom(state, ring, locant)?;
            if state.graph().atom(atom).element != ChemEl::C {
                return Err(error("Duplicate locants present in Hantzsch-Widman system"));
            }
            state.graph_mut().atom_mut(atom).element = element;
            if let Some(lambda) = arena[heteroatom].attribute(LAMBDA_ATR) {
                state.graph_mut().atom_mut(atom).lambda_convention_valency = Some(
                    lambda
                        .parse()
                        .map_err(|_| error("Malformed lambda valency"))?,
                );
            }
            arena.detach(heteroatom);
        }
        for delta in arena.children_named(sub_or_root, DELTA_EL) {
            let locant = arena.value(delta);
            if locant.is_empty() {
                let unsaturator = arena.token(UNSATURATOR_EL, "");
                arena[unsaturator].add_attribute(VALUE_ATR, "2");
                arena.insert_after(group, unsaturator);
            } else {
                let atom = locanted_atom(state, ring, &locant)?;
                fragment_tools::unsaturate(state.graph_mut(), atom, 2, ring)
                    .map_err(graph_error)?;
            }
            arena.detach(delta);
        }
        if !unlocanted.is_empty() {
            let carbons: Vec<_> = atoms
                .iter()
                .copied()
                .filter(|&a| state.graph().atom(a).element == ChemEl::C)
                .collect();
            let count = unlocanted.len();
            if count > 1 && count < carbons.len().saturating_sub(1) {
                let benzo = arena
                    .previous_sibling_named(group, GROUP_EL)
                    .is_some_and(|id| matches!(arena.value(id).as_str(), "benz" | "benzo"));
                if !(benzo || arena[group].attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR) == Some("o")) {
                    state.add_is_ambiguous(format!(
                        "Heteroatom positioning in the Hantzsch-Widman name {name}"
                    ));
                }
            }
            if count > carbons.len() {
                return Err(error(format!(
                    "{count} heteroatoms were specified for a Hantzsch-Widman ring with only {} atoms",
                    carbons.len()
                )));
            }
            for (heteroatom, atom) in unlocanted.into_iter().zip(carbons) {
                state.graph_mut().atom_mut(atom).element = heteroatom_element(arena, heteroatom)?;
                if let Some(lambda) = arena[heteroatom].attribute(LAMBDA_ATR) {
                    state.graph_mut().atom_mut(atom).lambda_convention_valency = Some(
                        lambda
                            .parse()
                            .map_err(|_| error("Malformed lambda valency"))?,
                    );
                }
                arena.detach(heteroatom);
            }
        }
        if name == "thithiazol"
            && let Some(suffix) = arena.next_sibling(group).filter(|&id| {
                arena[id].attribute(TYPE_ATR) == Some(CHARGE_TYPE_VAL)
                    && arena[id].attribute(LOCANT_ATR).is_none()
            })
            && let Some(locant) = atoms
                .iter()
                .copied()
                .filter(|&a| state.graph().atom(a).element == ChemEl::S)
                .find_map(|a| state.graph().atom(a).locants.first().cloned())
        {
            arena[suffix].add_attribute(LOCANT_EL, locant);
        }
    }
    Ok(())
}

/// Suffixes and conjunctive suffixes receive element locants before the parent.
pub fn assign_element_symbol_locants(
    state: &mut BuildState,
    arena: &Arena,
    sub_or_root: NodeId,
) -> Result<()> {
    let groups = arena.children_named(sub_or_root, GROUP_EL);
    let last = *groups
        .last()
        .ok_or_else(|| error("No group available for element symbol locants"))?;
    let mut suffixes = state.xml_suffix_map.get(&last).cloned().unwrap_or_default();
    for conjunctive in arena.children_named(sub_or_root, CONJUNCTIVESUFFIXGROUP_EL) {
        suffixes.push(fragment(arena, conjunctive)?);
    }
    fragment_tools::assign_element_locants(state.graph_mut(), fragment(arena, last)?, &suffixes)
        .map_err(graph_error)?;
    for &group in groups[..groups.len() - 1].iter().rev() {
        fragment_tools::assign_element_locants(state.graph_mut(), fragment(arena, group)?, &[])
            .map_err(graph_error)?;
    }
    Ok(())
}

fn resolve_features(state: &mut BuildState, arena: &mut Arena, node: NodeId) -> Result<()> {
    crate::structure_building_methods::resolve_locanted_features(state, arena, node)
        .map_err(graph_error)?;
    crate::structure_building_methods::resolve_unlocanted_features(state, arena, node)
        .map_err(graph_error)
}

/// Constructs bi/ter/etc. assemblies using exact per-ring locants or the
/// source's substitutable-atom selection and ambiguity checks.
pub fn process_ring_assemblies(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    sub_or_root: NodeId,
) -> Result<()> {
    for multiplier in arena.children_named(sub_or_root, RINGASSEMBLYMULTIPLIER_EL) {
        let count = number(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))?;
        let mut joining_locants: Vec<[String; 2]> = Vec::new();
        let potential_locant = arena.previous_sibling(multiplier);
        let group = next_named(arena, multiplier, GROUP_EL)
            .ok_or_else(|| error("Ring assembly has no group"))?;
        if let Some(locant) = potential_locant.filter(|&id| {
            matches!(
                arena[id].name.as_str(),
                COLONORSEMICOLONDELIMITEDLOCANT_EL | LOCANT_EL
            )
        }) {
            if arena[locant].attribute(TYPE_ATR) == Some(ORTHOMETAPARA_TYPE_VAL) {
                let second = arena.value(locant);
                joining_locants.push(["1".into(), "1'".into()]);
                for i in 1..count.saturating_sub(1) {
                    joining_locants.push([
                        format!("{second}{}", "'".repeat(i)),
                        format!("1{}", "'".repeat(i + 1)),
                    ]);
                }
                arena.detach(locant);
            } else {
                let text = arena.value(locant);
                let text = text.strip_suffix('-').unwrap_or(&text);
                let per_ring: Vec<_> = text.split([':', ';']).collect();
                if per_ring.len() != count.saturating_sub(1) {
                    return Err(error(format!(
                        "Disagreement between number of locants({text}) and ring assembly multiplier: {count}"
                    )));
                }
                if per_ring.len() != 1 || per_ring[0].split(',').count() != 1 {
                    for pair in per_ring {
                        let locants: Vec<_> = pair.split(',').collect();
                        if locants.len() != 2 {
                            return Err(error(format!(
                                "missing locant, expected 2 locants: {pair}"
                            )));
                        }
                        joining_locants.push([locants[0].into(), locants[1].into()]);
                    }
                    arena.detach(locant);
                }
            }
        }
        let ring = fragment(arena, group)?;
        let next = arena
            .next_sibling(multiplier)
            .ok_or_else(|| error("Ring assembly has no component"))?;
        let element_to_resolve = if arena[next].name == STRUCTURALOPENBRACKET_EL {
            let temporary = arena.grouping(SUBSTITUENT_EL);
            let mut current = arena.next_sibling(next);
            arena.detach(next);
            while let Some(id) = current.filter(|&id| arena[id].name != STRUCTURALCLOSEBRACKET_EL) {
                current = arena.next_sibling(id);
                arena.detach(id);
                arena.add_child(temporary, id);
            }
            if let Some(close) = current {
                arena.detach(close);
            }
            temporary
        } else {
            determine_elements_to_resolve_into_ring_assembly(
                arena,
                multiplier,
                joining_locants.len(),
                state.graph().fragment(ring).out_atoms.len(),
            )?
        };
        let suffixes = arena.children_named(element_to_resolve, SUFFIX_EL);
        SuffixApplier::new(state, suffix_rules).resolve_suffixes(arena, group, &suffixes)?;
        let out_count = state.graph().fragment(ring).out_atoms.len();
        if out_count > 1 {
            return Err(error(
                "Ring assembly fragment should have one or no OutAtoms; not more than one!",
            ));
        }
        let bond_order = if out_count == 1 {
            state.graph().fragment(ring).out_atoms[0].valency
        } else {
            1
        };
        let use_suffix_position = joining_locants.is_empty() && count == 2 && out_count == 1;
        if !use_suffix_position && out_count == 1 {
            state.graph_mut().remove_out_atom(ring, 0);
        }
        resolve_features(state, arena, element_to_resolve)?;
        arena.detach(group);
        arena.insert_after(multiplier, group);
        let order =
            u8::try_from(bond_order).map_err(|_| error("Invalid ring assembly bond order"))?;
        if use_suffix_position {
            let clone = state
                .fragment_manager
                .copy_and_relabel_fragment(ring, 1)
                .map_err(graph_error)?;
            let parent_atom = state.graph().fragment(ring).out_atoms[0].atom;
            let clone_atom = state.graph().fragment(clone).out_atoms[0].atom;
            state.graph_mut().remove_out_atom(ring, 0);
            state.graph_mut().remove_out_atom(clone, 0);
            state
                .fragment_manager
                .incorporate_fragment_with_bond(clone, clone_atom, ring, parent_atom, order)
                .map_err(graph_error)?;
        } else {
            let mut clones = Vec::new();
            for i in 1..count {
                clones.push(
                    state
                        .fragment_manager
                        .copy_and_relabel_fragment(ring, i)
                        .map_err(graph_error)?,
                );
            }
            let mut last_unlocanted_ring = None;
            for (index, clone) in clones.into_iter().enumerate() {
                let (parent_atom, clone_atom) = if !joining_locants.is_empty() {
                    let parent = locanted_atom(state, ring, &joining_locants[index][0])?;
                    let second = &joining_locants[index][1];
                    let clone_atom = if count == 2 && !second.ends_with('\'') {
                        state
                            .graph()
                            .atom_by_locant(clone, second)
                            .or_else(|| state.graph().atom_by_locant(clone, &format!("{second}'")))
                            .ok_or_else(|| error(format!("No atom with locant {second}")))?
                    } else {
                        locanted_atom(state, clone, second)?
                    };
                    (parent, clone_atom)
                } else {
                    let parent_atoms = fragment_tools::find_substitutable_atoms(
                        state.graph(),
                        last_unlocanted_ring.unwrap_or(ring),
                        bond_order,
                    )
                    .map_err(graph_error)?;
                    let clone_atoms =
                        fragment_tools::find_substitutable_atoms(state.graph(), clone, bond_order)
                            .map_err(graph_error)?;
                    if parent_atoms.is_empty() || clone_atoms.is_empty() {
                        return Err(error(
                            "Unable to find suitable atom for unlocanted ring assembly construction",
                        ));
                    }
                    for atoms in [&parent_atoms, &clone_atoms] {
                        if ambiguity::is_substitution_ambiguous(state.graph(), atoms, 1)
                            .map_err(graph_error)?
                        {
                            state.add_is_ambiguous(format!(
                                "Choice of atoms to form ring assembly: {}",
                                arena.value(group)
                            ));
                        }
                    }
                    last_unlocanted_ring = Some(clone);
                    (parent_atoms[0], clone_atoms[0])
                };
                state
                    .fragment_manager
                    .incorporate_fragment_with_bond(clone, clone_atom, ring, parent_atom, order)
                    .map_err(graph_error)?;
            }
        }
        let name = format!("{}{}", arena.value(multiplier), arena.value(group));
        arena[group].set_value(name);
        if let Some(open) = arena
            .previous_sibling(multiplier)
            .filter(|&id| arena[id].name == STRUCTURALOPENBRACKET_EL)
        {
            let close = next_named(arena, open, STRUCTURALCLOSEBRACKET_EL)
                .ok_or_else(|| error("Ring assembly structural bracket has no closing bracket"))?;
            arena.detach(close);
            arena.detach(open);
        }
        arena.detach(multiplier);
    }
    Ok(())
}

pub fn determine_elements_to_resolve_into_ring_assembly(
    arena: &mut Arena,
    multiplier: NodeId,
    ring_joining_locants: usize,
    out_atom_count: usize,
) -> Result<NodeId> {
    let temporary = arena.grouping(SUBSTITUENT_EL);
    let mut group_found = false;
    let mut inline_suffix_seen = out_atom_count > 0;
    let mut current = arena.next_sibling(multiplier);
    while let Some(id) = current {
        let next = arena.next_sibling(id);
        if !group_found {
            arena.detach(id);
            arena.add_child(temporary, id);
            if arena[id].name == GROUP_EL {
                group_found = true;
            }
        } else {
            let take = match arena[id].name.as_str() {
                SUFFIX_EL => {
                    if arena[id].attribute(TYPE_ATR) == Some(CHARGE_TYPE_VAL)
                        && arena[id].attribute(LOCANT_ATR).is_none()
                    {
                        true
                    } else if !inline_suffix_seen
                        && arena[id].attribute(TYPE_ATR) == Some(INLINE_TYPE_VAL)
                        && arena[id].attribute(MULTIPLIED_ATR).is_none()
                        && (arena[id].attribute(LOCANT_ATR).is_none()
                            || (arena[multiplier].attribute(VALUE_ATR) == Some("2")
                                && ring_joining_locants == 0))
                        && arena[id].fragment.is_none()
                    {
                        inline_suffix_seen = true;
                        true
                    } else {
                        false
                    }
                }
                UNSATURATOR_EL => arena[id].attribute(LOCANT_ATR).is_none(),
                _ => false,
            };
            if !take {
                break;
            }
            arena.detach(id);
            arena.add_child(temporary, id);
        }
        current = next;
    }
    let parent = arena[multiplier]
        .parent
        .ok_or_else(|| error("Ring assembly multiplier has no parent"))?;
    if arena[parent].name != SUBSTITUENT_EL
        && arena
            .children_named(parent, SUFFIX_EL)
            .iter()
            .any(|&id| arena[id].attribute(TYPE_ATR) == Some(INLINE_TYPE_VAL))
    {
        return Err(error("Unexpected radical adding suffix on ring assembly"));
    }
    Ok(temporary)
}

/// Dispatches all upstream polycyclic-spiro syntaxes in source order.
pub fn process_poly_cyclic_spiro_nomenclature(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    sub_or_root: NodeId,
) -> Result<()> {
    let spiros = arena.children_named(sub_or_root, POLYCYCLICSPIRO_EL);
    let Some(&descriptor) = spiros.first() else {
        return Ok(());
    };
    match arena[descriptor].attribute(VALUE_ATR).unwrap_or("") {
        "spiro" | "spirobi" | "spiroter" | "dispiroter" if spiros.len() != 1 => {
            return Err(error("Nested polyspiro systems are not supported"));
        }
        "spiro" => process_non_identical_poly_cyclic_spiro(state, arena, suffix_rules, descriptor)?,
        "spiroOldMethod" => {
            process_old_method_poly_cyclic_spiro(state, arena, suffix_rules, &spiros)?
        }
        "spirobi" => process_spiro_bi_or_ter(state, arena, suffix_rules, descriptor, 2)?,
        "spiroter" => process_spiro_bi_or_ter(state, arena, suffix_rules, descriptor, 3)?,
        "dispiroter" => process_dispiroter(state, arena, suffix_rules, descriptor)?,
        _ => return Err(error("Unsupported spiro system encountered")),
    }
    arena.detach(descriptor);
    Ok(())
}

fn fix_locant_capitalisation(locant: &str) -> String {
    let bytes = locant.as_bytes();
    if bytes.len() >= 2
        && matches!(bytes[bytes.len() - 1], b'A'..=b'G')
        && bytes[..bytes.len() - 1].iter().all(u8::is_ascii_digit)
    {
        locant.to_ascii_lowercase()
    } else {
        locant.into()
    }
}

fn added_hydrogen_in_spiro_locant(
    locant: &str,
    current_component: usize,
) -> Option<(String, String)> {
    static ADDED_HYDROGEN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[\[\(\{]([^\[\(\{]*)H[\]\)\}]").unwrap());
    let captures = ADDED_HYDROGEN.captures(locant)?;
    let mut hydrogen_locant = captures[1].to_owned();
    let prime_count = hydrogen_locant.len() - hydrogen_locant.trim_end_matches('\'').len();
    if prime_count > 0 && prime_count == current_component {
        hydrogen_locant.truncate(hydrogen_locant.len() - prime_count);
    }
    Some((
        hydrogen_locant,
        ADDED_HYDROGEN.replace_all(locant, "").into_owned(),
    ))
}

fn process_non_identical_poly_cyclic_spiro(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    descriptor: NodeId,
) -> Result<()> {
    let parent = arena[descriptor]
        .parent
        .ok_or_else(|| error("Spiro descriptor has no parent"))?;
    let open = arena
        .next_sibling(descriptor)
        .filter(|&id| arena[id].name == STRUCTURALOPENBRACKET_EL)
        .ok_or_else(|| error("OPSIN Bug: Open bracket not found where open bracket expeced"))?;
    let bracket_elements = siblings_until(arena, open, STRUCTURALCLOSEBRACKET_EL);
    let close = bracket_elements
        .last()
        .and_then(|&id| arena.next_sibling(id))
        .filter(|&id| arena[id].name == STRUCTURALCLOSEBRACKET_EL)
        .ok_or_else(|| error("OPSIN Bug: Open bracket not found where open bracket expeced"))?;
    let mut groups = Vec::new();
    for element in bracket_elements {
        if arena[element].name == GROUP_EL {
            groups.push(element);
        } else if arena[element].name == SPIROLOCANT_EL {
            let text = arena.value(element);
            let mut locants: Vec<String> = text
                .strip_suffix('-')
                .unwrap_or(&text)
                .split(',')
                .map(str::to_owned)
                .collect();
            if locants.len() != 2 {
                return Err(error(
                    "Incorrect number of locants found before component of polycyclic spiro system",
                ));
            }
            let mut changed = false;
            if let Some((locant, stripped)) =
                added_hydrogen_in_spiro_locant(&locants[0], groups.len().saturating_sub(1))
            {
                let added = arena.token(ADDEDHYDROGEN_EL, "");
                arena[added].add_attribute(LOCANT_ATR, locant);
                arena.insert_before(element, added);
                locants[0] = stripped;
                changed = true;
            }
            if let Some((locant, stripped)) =
                added_hydrogen_in_spiro_locant(&locants[1], groups.len())
            {
                let added = arena.token(ADDEDHYDROGEN_EL, "");
                arena[added].add_attribute(LOCANT_ATR, locant);
                arena.insert_after(element, added);
                locants[1] = stripped;
                changed = true;
            }
            if changed {
                arena[element].add_attribute(TYPE_ATR, ADDEDHYDROGENLOCANT_TYPE_VAL);
            }
            arena[element].set_value(locants.join(","));
        }
    }
    if groups.len() < 2 {
        return Err(error(
            "OPSIN Bug: Atleast two groups were expected in polycyclic spiro system",
        ));
    }
    let first_spiro_locant = next_named(arena, groups[0], SPIROLOCANT_EL)
        .ok_or_else(|| error("Unable to find spiroLocant for polycyclic spiro system"))?;
    let start = arena.index_of(parent, open).unwrap() + 1;
    let end = arena.index_of(parent, first_spiro_locant).unwrap();
    // Feature resolution detaches and reinserts children; keep the source
    // snapshot independent of the Arena that the resolver mutates.
    let selected_features = arena[parent].children[start..end].to_vec();
    resolve_features_onto_group(state, arena, suffix_rules, &selected_features)?;
    let mut spiro_atoms = HashSet::new();
    for i in 1..groups.len() {
        let next_group = groups[i];
        let locant = next_named(arena, groups[i - 1], SPIROLOCANT_EL)
            .ok_or_else(|| error("Unable to find spiroLocant for polycyclic spiro system"))?;
        let locants: Vec<_> = arena
            .value(locant)
            .split(',')
            .map(fix_locant_capitalisation)
            .collect();
        let end_node = if i + 1 < groups.len() {
            next_named(arena, next_group, SPIROLOCANT_EL)
        } else {
            next_named(arena, next_group, STRUCTURALCLOSEBRACKET_EL)
        }
        .ok_or_else(|| error("Unable to find end of component in polycyclic spiro system"))?;
        let start = arena.index_of(parent, locant).unwrap() + 1;
        let end = arena.index_of(parent, end_node).unwrap();
        let selected_features = arena[parent].children[start..end].to_vec();
        resolve_features_onto_group(state, arena, suffix_rules, &selected_features)?;
        arena.detach(locant);
        let next_fragment = fragment(arena, next_group)?;
        let next_atoms = state.graph().fragment(next_fragment).atoms.clone();
        fragment_tools::relabel_numeric_locants(state.graph_mut(), &next_atoms, &"'".repeat(i));
        let second = if locants[1].ends_with('\'') {
            locants[1].clone()
        } else {
            format!("{}'", locants[1])
        };
        let next_atom = locanted_atom(state, next_fragment, &second)?;
        let mut atom_to_replace = None;
        for &group in &groups[..i] {
            atom_to_replace = state
                .graph()
                .atom_by_locant(fragment(arena, group)?, &locants[0]);
            if atom_to_replace.is_some() {
                break;
            }
        }
        let old = atom_to_replace.ok_or_else(|| {
            error(format!(
                "Could not find the atom with locant {} for use in polycyclic spiro system",
                locants[0]
            ))
        })?;
        spiro_atoms.insert(old);
        let old_element = state.graph().atom(old).element;
        let next_element = state.graph().atom(next_atom).element;
        if old_element != next_element {
            if old_element != ChemEl::C && next_element == ChemEl::C {
                state.graph_mut().atom_mut(next_atom).element = old_element;
            } else if old_element != ChemEl::C && next_element != ChemEl::C {
                return Err(error(format!(
                    "Disagreement between which element the spiro atom should be: {old_element} and {next_element}"
                )));
            }
        }
        if state.graph().atom(old).spare_valency {
            state.graph_mut().atom_mut(next_atom).spare_valency = true;
        }
        state
            .fragment_manager
            .replace_atom_preserving_connectivity(old, next_atom)
            .map_err(graph_error)?;
    }
    if spiro_atoms.len() > 1
        && let Some(multiplier) = arena
            .previous_sibling(descriptor)
            .filter(|&id| arena[id].name == MULTIPLIER_EL)
        && number(arena[multiplier].attribute(VALUE_ATR).unwrap_or(""))? == spiro_atoms.len()
    {
        arena.detach(multiplier);
    }
    let root_group = *groups.last().unwrap();
    let root_fragment = fragment(arena, root_group)?;
    let mut name = arena.value(root_group);
    for &group in &groups[..groups.len() - 1] {
        state
            .fragment_manager
            .incorporate_fragment(fragment(arena, group)?, root_fragment)
            .map_err(graph_error)?;
        name = format!("{}{name}", arena.value(group));
        arena.detach(group);
    }
    let name = format!("{}{name}", arena.value(descriptor));
    arena[root_group].set_value(name);
    arena.detach(open);
    arena.detach(close);
    Ok(())
}

fn spiro_atom(
    state: &mut BuildState,
    arena: &Arena,
    group: NodeId,
    ring: FragmentId,
    locant: Option<&str>,
) -> Result<AtomId> {
    if let Some(locant) = locant {
        return locanted_atom(state, ring, locant);
    }
    let atoms =
        fragment_tools::find_substitutable_atoms(state.graph(), ring, 2).map_err(graph_error)?;
    if atoms.is_empty() {
        return Err(error("No suitable atom found for spiro fusion"));
    }
    if ambiguity::is_substitution_ambiguous(state.graph(), &atoms, 1).map_err(graph_error)? {
        state.add_is_ambiguous(format!(
            "Choice of atom for spiro fusion on: {}",
            arena.value(group)
        ));
    }
    Ok(atoms[0])
}

fn process_old_method_poly_cyclic_spiro(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    spiros: &[NodeId],
) -> Result<()> {
    let first = spiros[0];
    let parent = arena[first]
        .parent
        .ok_or_else(|| error("Spiro descriptor has no parent"))?;
    let first_element = *arena[parent]
        .children
        .first()
        .ok_or_else(|| error("Spiro parent has no children"))?;
    let mut features = siblings_until(arena, first_element, POLYCYCLICSPIRO_EL);
    features.insert(0, first_element);
    resolve_features_onto_group(state, arena, suffix_rules, &features)?;
    for (index, &descriptor) in spiros.iter().enumerate() {
        let previous_group = arena
            .previous_sibling_named(descriptor, GROUP_EL)
            .ok_or_else(|| {
                error("OPSIN bug: unable to locate group before polycylic spiro descriptor")
            })?;
        let next_group = next_named(arena, descriptor, GROUP_EL).ok_or_else(|| {
            error("OPSIN bug: unable to locate group after polycylic spiro descriptor")
        })?;
        let previous_fragment = fragment(arena, previous_group)?;
        let parent_fragment = fragment(arena, next_group)?;
        let atoms = state.graph().fragment(parent_fragment).atoms.clone();
        fragment_tools::relabel_numeric_locants(state.graph_mut(), &atoms, &"'".repeat(index + 1));
        let features = siblings_until(arena, descriptor, POLYCYCLICSPIRO_EL);
        resolve_features_onto_group(state, arena, suffix_rules, &features)?;
        let first_locant = if let Some(locant) = arena
            .previous_sibling(descriptor)
            .filter(|&id| arena[id].name == LOCANT_EL)
        {
            let value = arena.value(locant);
            if value.split(',').count() != 1 {
                return Err(error("Malformed locant before polycyclic spiro descriptor"));
            }
            arena.detach(locant);
            Some(value)
        } else {
            None
        };
        let old = spiro_atom(
            state,
            arena,
            previous_group,
            previous_fragment,
            first_locant.as_deref(),
        )?;
        let second_locant = if let Some(locant) = arena
            .next_sibling(descriptor)
            .filter(|&id| arena[id].name == LOCANT_EL)
        {
            let value = arena.value(locant);
            if value.split(',').count() != 1 {
                return Err(error("Malformed locant after polycyclic spiro descriptor"));
            }
            arena.detach(locant);
            Some(value)
        } else {
            None
        };
        let parent_atom = spiro_atom(
            state,
            arena,
            next_group,
            parent_fragment,
            second_locant.as_deref(),
        )?;
        state
            .fragment_manager
            .replace_atom_preserving_connectivity(old, parent_atom)
            .map_err(graph_error)?;
        if state.graph().atom(old).spare_valency {
            state.graph_mut().atom_mut(parent_atom).spare_valency = true;
        }
        if state.graph().atom(old).charge != 0 && state.graph().atom(parent_atom).charge == 0 {
            let old_atom = state.graph().atom(old).clone();
            state.graph_mut().atom_mut(parent_atom).charge = old_atom.charge;
            state
                .graph_mut()
                .atom_mut(parent_atom)
                .protons_explicitly_added_or_removed = old_atom.protons_explicitly_added_or_removed;
        }
        state
            .fragment_manager
            .incorporate_fragment(previous_fragment, parent_fragment)
            .map_err(graph_error)?;
        let name = format!(
            "{}{}{}",
            arena.value(previous_group),
            arena.value(descriptor),
            arena.value(next_group)
        );
        arena[next_group].set_value(name);
        arena.detach(previous_group);
    }
    Ok(())
}

fn process_spiro_bi_or_ter(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    descriptor: NodeId,
    components: usize,
) -> Result<()> {
    let text = if let Some(locant) = arena
        .previous_sibling(descriptor)
        .filter(|&id| arena[id].name == LOCANT_EL)
    {
        let value = arena.value(locant);
        arena.detach(locant);
        value
    } else if components == 2 {
        "1,1'".into()
    } else {
        return Err(error(
            "Unable to find locant indicating atoms to form polycyclic spiro system!",
        ));
    };
    let locants: Vec<_> = text.split(',').collect();
    if locants.len() != components {
        return Err(error(
            "Mismatch between spiro descriptor and number of locants provided",
        ));
    }
    let group = next_named(arena, descriptor, GROUP_EL)
        .ok_or_else(|| error("Cannot find group to which spirobi/ter descriptor applies"))?;
    determine_features_to_resolve_in_single_component_spiro(
        state,
        arena,
        suffix_rules,
        descriptor,
    )?;
    let ring = fragment(arena, group)?;
    let mut clones = Vec::new();
    for index in 1..components {
        clones.push(
            state
                .fragment_manager
                .copy_and_relabel_fragment(ring, index)
                .map_err(graph_error)?,
        );
    }
    let original_atom = locanted_atom(state, ring, locants[0])?;
    for (index, &clone) in clones.iter().enumerate() {
        let locant = locants[index + 1];
        let old = if components == 2 && !locant.ends_with('\'') {
            state
                .graph()
                .atom_by_locant(clone, locant)
                .or_else(|| state.graph().atom_by_locant(clone, &format!("{locant}'")))
                .ok_or_else(|| error(format!("No atom with locant {locant}")))?
        } else {
            locanted_atom(state, clone, locant)?
        };
        state
            .fragment_manager
            .replace_atom_preserving_connectivity(old, original_atom)
            .map_err(graph_error)?;
        if state.graph().atom(old).spare_valency {
            state.graph_mut().atom_mut(original_atom).spare_valency = true;
        }
    }
    for clone in clones {
        state
            .fragment_manager
            .incorporate_fragment(clone, ring)
            .map_err(graph_error)?;
    }
    let name = format!("{}{}", arena.value(descriptor), arena.value(group));
    arena[group].set_value(name);
    Ok(())
}

fn process_dispiroter(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    descriptor: NodeId,
) -> Result<()> {
    let value = arena.value(descriptor);
    let prefix = value
        .get(
            ..value
                .len()
                .checked_sub(10)
                .ok_or_else(|| error("Malformed dispiroter descriptor"))?,
        )
        .ok_or_else(|| error("Malformed dispiroter descriptor"))?;
    let text = prefix.strip_suffix('-').unwrap_or(prefix);
    let pairs: Vec<_> = text.split(':').collect();
    let group = next_named(arena, descriptor, GROUP_EL)
        .ok_or_else(|| error("Cannot find group to which dispiroter descriptor applies"))?;
    determine_features_to_resolve_in_single_component_spiro(
        state,
        arena,
        suffix_rules,
        descriptor,
    )?;
    let ring = fragment(arena, group)?;
    let mut clones = Vec::new();
    for index in 1..3 {
        clones.push(
            state
                .fragment_manager
                .copy_and_relabel_fragment(ring, index)
                .map_err(graph_error)?,
        );
    }
    for clone in clones {
        state
            .fragment_manager
            .incorporate_fragment(clone, ring)
            .map_err(graph_error)?;
    }
    for index in 0..2 {
        let pair = pairs
            .get(index)
            .ok_or_else(|| error("Malformed dispiroter locants"))?;
        let locants: Vec<_> = pair.split(',').collect();
        if locants.len() < 2 {
            return Err(error("Malformed dispiroter locants"));
        }
        let parent = locanted_atom(state, ring, &fix_locant_capitalisation(locants[0]))?;
        let old = locanted_atom(state, ring, &fix_locant_capitalisation(locants[1]))?;
        state
            .fragment_manager
            .replace_atom_preserving_connectivity(old, parent)
            .map_err(graph_error)?;
        if state.graph().atom(old).spare_valency {
            state.graph_mut().atom_mut(parent).spare_valency = true;
        }
    }
    let name = format!("dispiroter{}", arena.value(group));
    arena[group].set_value(name);
    Ok(())
}

fn determine_features_to_resolve_in_single_component_spiro(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    descriptor: NodeId,
) -> Result<()> {
    let next = arena
        .next_sibling(descriptor)
        .ok_or_else(|| error("Cannot find spiro component"))?;
    let features = if arena[next].name == STRUCTURALOPENBRACKET_EL {
        arena.detach(next);
        let features = siblings_until(arena, descriptor, STRUCTURALCLOSEBRACKET_EL);
        let close = features
            .last()
            .and_then(|&id| arena.next_sibling(id))
            .ok_or_else(|| error("Cannot find spiro closing bracket"))?;
        arena.detach(close);
        features
    } else {
        siblings_until(arena, descriptor, GROUP_EL)
    };
    resolve_features_onto_group(state, arena, suffix_rules, &features)
}

/// Temporarily isolates a ring's features, resolves them, and restores the
/// remaining children in their original order. Resolved suffixes stay detached.
pub fn resolve_features_onto_group(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_rules: &SuffixRules,
    features: &[NodeId],
) -> Result<()> {
    let Some(&first) = features.first() else {
        return Ok(());
    };
    let parent = arena[first]
        .parent
        .ok_or_else(|| error("Features have no parent"))?;
    let index = arena
        .index_of(parent, first)
        .ok_or_else(|| error("Feature is absent from its parent"))?;
    let temporary = arena.grouping(SUBSTITUENT_EL);
    let mut group = None;
    let mut suffixes = Vec::new();
    let mut locant = None;
    for &element in features {
        match arena[element].name.as_str() {
            GROUP_EL => group = Some(element),
            SUFFIX_EL => suffixes.push(element),
            LOCANT_EL if group.is_none() => locant = Some(element),
            _ => {}
        }
        arena.detach(element);
        arena.add_child(temporary, element);
    }
    let group =
        group.ok_or_else(|| error("OPSIN bug: group element should of been given to method"))?;
    if let Some(locant) = locant {
        let able = crate::component_processor::find_elements_missing_indirect_locants(
            arena, temporary, locant,
        );
        let values: Vec<_> = arena.value(locant).split(',').map(str::to_owned).collect();
        if able.len() >= values.len() {
            for (element, value) in able.into_iter().zip(values) {
                arena[element].add_attribute(LOCANT_ATR, value);
            }
            arena.detach(locant);
        }
    }
    if !suffixes.is_empty() {
        SuffixApplier::new(state, suffix_rules).resolve_suffixes(arena, group, &suffixes)?;
        for suffix in suffixes {
            arena.detach(suffix);
        }
    }
    if !arena[temporary].children.is_empty() {
        resolve_features(state, arena, temporary)?;
        for child in arena[temporary].children.clone().into_iter().rev() {
            arena.detach(child);
            arena.insert_child(parent, child, index);
        }
    }
    Ok(())
}

/// Adds bridges and numbers them by decreasing highest bridgehead locant;
/// within a bridge numbering runs from the higher locanted bridgehead.
pub fn process_fused_ring_bridges(
    state: &mut BuildState,
    arena: &mut Arena,
    sub_or_root: NodeId,
) -> Result<()> {
    let bridges = arena.children_named(sub_or_root, FUSEDRINGBRIDGE_EL);
    let Some(&last) = bridges.last() else {
        return Ok(());
    };
    let group =
        next_named(arena, last, GROUP_EL).ok_or_else(|| error("Bridge has no ring group"))?;
    let ring = fragment(arena, group)?;
    let mut bridge_ring_atoms: Vec<(FragmentId, [AtomId; 2])> = Vec::new();
    for bridge in bridges {
        let mut locants: Option<Vec<[String; 2]>> = None;
        let mut count = 1;
        if let Some(previous) = arena.previous_sibling(bridge) {
            if arena[previous].name == MULTIPLIER_EL {
                count = number(arena[previous].attribute(VALUE_ATR).unwrap_or(""))?;
                let possible_locant = arena.previous_sibling(previous);
                arena.detach(previous);
                if let Some(locant) = possible_locant
                    .filter(|&id| arena[id].name == COLONORSEMICOLONDELIMITEDLOCANT_EL)
                {
                    let text = arena.value(locant);
                    let values: Vec<_> =
                        text.strip_suffix('-').unwrap_or(&text).split(':').collect();
                    if values.len() != count {
                        return Err(error(format!(
                            "Mismatch between locant and multiplier counts ({} and {count}): {text}",
                            values.len()
                        )));
                    }
                    let mut parsed = Vec::new();
                    for value in values {
                        let pair: Vec<_> = value.split(',').collect();
                        if pair.len() != 2 {
                            return Err(error(format!(
                                "Expected two locants per bridge, but was: {text}"
                            )));
                        }
                        parsed.push([pair[0].into(), pair[1].into()]);
                    }
                    locants = Some(parsed);
                    arena.detach(locant);
                }
            } else if arena[previous].name == LOCANT_EL {
                let text = arena.value(previous);
                let pair: Vec<_> = text.split(',').collect();
                if pair.len() == 2 {
                    locants = Some(vec![[pair[0].into(), pair[1].into()]]);
                    arena.detach(previous);
                }
            }
        }
        for index in 0..count {
            // Java builds with the ring's token, without changing that token's
            // existing fragment association.
            let smiles = arena[bridge].attribute(VALUE_ATR).unwrap_or("");
            let group_type = arena[group].attribute(TYPE_ATR).unwrap_or("");
            let bridge_fragment = state
                .fragment_manager
                .build_smiles(smiles, group_type, NONE_LABELS_VAL)
                .map_err(graph_error)?;
            state.graph_mut().fragment_mut(bridge_fragment).sub_type =
                arena[group].attribute(SUBTYPE_ATR).unwrap_or("").into();
            state
                .graph_mut()
                .fragment_mut(bridge_fragment)
                .token_attributes = arena[group]
                .attributes
                .iter()
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect();
            state
                .fragment_manager
                .fragment_tokens
                .insert(bridge_fragment, group);
            let parent_atom = if let Some(locants) = &locants {
                if state.graph().fragment(bridge_fragment).out_atoms.len() < 2 {
                    return Err(error("Bridge must have two out atoms"));
                }
                state.graph_mut().set_out_atom_locant(
                    bridge_fragment,
                    0,
                    Some(locants[index][0].clone()),
                );
                state.graph_mut().set_out_atom_locant(
                    bridge_fragment,
                    1,
                    Some(locants[index][1].clone()),
                );
                state
                    .graph()
                    .fragment(ring)
                    .default_in_atom
                    .or_else(|| state.graph().fragment(ring).atoms.first().copied())
                    .ok_or_else(|| error("Ring fragment has no atoms"))?
            } else {
                let atoms = fragment_tools::find_substitutable_atoms(state.graph(), ring, 1)
                    .map_err(graph_error)?;
                if atoms.is_empty() {
                    return Err(error("Unable to find suitable atom to form bridge"));
                }
                if ambiguity::is_substitution_ambiguous(state.graph(), &atoms, 1)
                    .map_err(graph_error)?
                {
                    state
                        .add_is_ambiguous(format!("Addition of bridge to: {}", arena.value(group)));
                }
                atoms[0]
            };
            let ring_atoms = crate::structure_building_methods::form_epoxide(
                state,
                arena,
                bridge_fragment,
                parent_atom,
            )
            .map_err(graph_error)?;
            bridge_ring_atoms.push((bridge_fragment, ring_atoms));
            state
                .fragment_manager
                .incorporate_fragment(bridge_fragment, ring)
                .map_err(graph_error)?;
        }
        arena.detach(bridge);
    }
    let mut highest = highest_numeric_locant(state, ring);
    bridge_ring_atoms.sort_by(|(_, a), (_, b)| {
        let a = locant_number(state, a[0]).max(locant_number(state, a[1]));
        let b = locant_number(state, b[0]).max(locant_number(state, b[1]));
        b.cmp(&a)
    });
    for (bridge, ring_atoms) in bridge_ring_atoms {
        let mut atoms = state.graph().fragment(bridge).atoms.clone();
        if locant_number(state, ring_atoms[0]) <= locant_number(state, ring_atoms[1]) {
            atoms.reverse();
        }
        for atom in atoms {
            highest += 1;
            state.graph_mut().add_locant(atom, highest.to_string());
        }
    }
    Ok(())
}

fn locant_number(state: &BuildState, atom: AtomId) -> usize {
    static NUMERIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)[a-z]?'*$").unwrap());
    state
        .graph()
        .atom(atom)
        .locants
        .first()
        .and_then(|locant| NUMERIC.captures(locant))
        .and_then(|captures| captures[1].parse().ok())
        .unwrap_or(0)
}
fn highest_numeric_locant(state: &BuildState, ring: FragmentId) -> usize {
    let mut next = 1;
    while state
        .graph()
        .atom_by_locant(ring, &next.to_string())
        .is_some()
    {
        next += 1;
    }
    next - 1
}
