//! Carbohydrate construction from OPSIN `ComponentProcessor.java`, lines 1446–1950.
//!
//! Source: OPSIN 2.9.0, commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
//! D/L prefix normalization is performed by the enclosing component processor;
//! configurational-prefix assignment after structure building belongs to the
//! stereochemistry handler. This module ports the intervening ten methods.

use crate::ParsingError;
use crate::build_state::BuildState;
use crate::graph::Element as ChemEl;
use crate::graph::{AtomId, AtomParity, FragmentId, Graph, StereoReference};
use crate::parse_tree::{Arena, NodeId};
use crate::xml_declarations::*;

type Result<T> = std::result::Result<T, ParsingError>;

fn error(message: impl Into<String>) -> ParsingError {
    ParsingError(message.into())
}
fn fragment(arena: &Arena, group: NodeId) -> Result<FragmentId> {
    arena[group]
        .fragment
        .ok_or_else(|| error(format!("OPSIN bug: No fragment on: {}", arena.value(group))))
}
fn attribute(arena: &Arena, node: NodeId, name: &str) -> Result<String> {
    arena[node]
        .attribute(name)
        .map(str::to_owned)
        .ok_or_else(|| {
            error(format!(
                "OPSIN bug: Missing {name} on: {}",
                arena.value(node)
            ))
        })
}
fn integer(value: &str) -> Result<i32> {
    value
        .parse()
        .map_err(|_| error(format!("OPSIN bug: Invalid integer: {value}")))
}
fn atom_by_locant(graph: &Graph, fragment: FragmentId, locant: &str) -> Result<AtomId> {
    graph
        .atom_by_locant(fragment, locant)
        .ok_or_else(|| error(format!("Could not find the atom with locant {locant}.")))
}
fn relative_atom(graph: &Graph, fragment: FragmentId, id: &str, name: &str) -> Result<AtomId> {
    let first = graph.fragment(fragment).atoms.first().copied();
    let atom = integer(id)?
        .checked_sub(1)
        .and_then(|offset| usize::try_from(offset).ok())
        .and_then(|offset| first.and_then(|first| first.0.checked_add(offset)))
        .map(AtomId)
        .filter(|atom| graph.fragment(fragment).atoms.contains(atom));
    atom.ok_or_else(|| {
        error(format!(
            "OPSIN bug: {id} did not point to an atom on: {name}"
        ))
    })
}
fn first_locant(graph: &Graph, atom: AtomId) -> Option<&str> {
    graph.atom(atom).locants.first().map(String::as_str)
}
// Fragment.getChainLength: consecutive numeric locants must also be bonded.
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
// Java String.split(regex), as used by the source, drops trailing empty fields.
fn split_locants(value: &str) -> Vec<&str> {
    let mut values: Vec<_> = value.split(',').collect();
    if !value.is_empty() {
        while values.last() == Some(&"") {
            values.pop();
        }
    }
    values
}

/// Cyclises carbohydrate groups and regularises carbohydrate suffixes.
pub fn process_carbohydrates(
    state: &mut BuildState,
    arena: &mut Arena,
    sub_or_root: NodeId,
) -> Result<()> {
    let carbohydrates =
        arena.children_with_attribute(sub_or_root, GROUP_EL, TYPE_ATR, CARBOHYDRATE_TYPE_VAL);
    for carbohydrate in carbohydrates {
        let frag = fragment(arena, carbohydrate)?;
        let subtype = arena[carbohydrate]
            .attribute(SUBTYPE_ATR)
            .unwrap_or("")
            .to_owned();
        let mut is_aldose = match subtype.as_str() {
            CARBOHYDRATESTEMKETOSE_SUBTYPE_VAL => false,
            CARBOHYDRATESTEMALDOSE_SUBTYPE_VAL | SYSTEMATICCARBOHYDRATESTEMALDOSE_SUBTYPE_VAL => {
                true
            }
            _ => {
                if let Some(id) = arena[carbohydrate]
                    .attribute(SUFFIXAPPLIESTO_ATR)
                    .map(str::to_owned)
                {
                    let anomeric_carbon =
                        relative_atom(state.graph(), frag, &id, &arena.value(carbohydrate))?;
                    if subtype == APIOFURANOSE_SUBTYPE_VAL {
                        if let Some(suffix) = arena
                            .next_sibling(carbohydrate)
                            .filter(|&node| arena[node].name == SUFFIX_EL)
                        {
                            arena[suffix]
                                .add_attribute(LOCANTID_ATR, (anomeric_carbon.0 + 1).to_string());
                        }
                    } else {
                        apply_alpha_beta_stereo_to_cyclised_carbohydrate(
                            state,
                            arena,
                            carbohydrate,
                            anomeric_carbon,
                        )?;
                    }
                    arena[carbohydrate].remove_attribute(SUFFIXAPPLIESTO_ATR);
                }
                // Trivial carbohydrates have no suffixes except apiofuranoses.
                continue;
            }
        };
        let mut cyclisation_performed = false;
        let id = arena[carbohydrate]
            .attribute(SUFFIXAPPLIESTO_ATR)
            .map(str::to_owned)
            .ok_or_else(|| {
                error(format!(
                    "OPSIN bug: Missing suffixAppliesTo on: {}",
                    arena.value(carbohydrate)
                ))
            })?;
        let mut potential_carbonyl =
            relative_atom(state.graph(), frag, &id, &arena.value(carbohydrate))?;
        arena[carbohydrate].remove_attribute(SUFFIXAPPLIESTO_ATR);

        let mut next_sibling = arena.next_sibling(carbohydrate);
        while let Some(next) = next_sibling {
            let next_next_sibling = arena.next_sibling(next);
            match arena[next].name.as_str() {
                SUFFIX_EL => {
                    let value = attribute(arena, next, VALUE_ATR)?;
                    if matches!(value.as_str(), "dialdose" | "aric acid" | "arate") {
                        if !is_aldose {
                            return Err(error(format!("{value} may only be used with aldoses")));
                        }
                        if cyclisation_performed {
                            return Err(error(format!(
                                "OPSIN bug: {value} not expected after carbohydrate cycliser"
                            )));
                        }
                        process_aldose_di_suffix(
                            state,
                            arena,
                            &value,
                            carbohydrate,
                            potential_carbonyl,
                        )?;
                        arena.detach(next);
                    } else if value.starts_with("uron") {
                        arena[next].add_attribute(
                            LOCANT_ATR,
                            chain_length(state.graph(), frag).to_string(),
                        );
                    } else if !cyclisation_performed
                        && matches!(value.as_str(), "ulose" | "osulose")
                    {
                        if value == "ulose" {
                            is_aldose = false;
                            if subtype == SYSTEMATICCARBOHYDRATESTEMALDOSE_SUBTYPE_VAL {
                                arena[carbohydrate].set_attribute(
                                    SUBTYPE_ATR,
                                    SYSTEMATICCARBOHYDRATESTEMKETOSE_SUBTYPE_VAL,
                                );
                            }
                        }
                        potential_carbonyl = process_ulose_suffix(
                            state,
                            arena,
                            carbohydrate,
                            next,
                            potential_carbonyl,
                        )?;
                        arena.detach(next);
                    } else if matches!(value.as_str(), "itol" | "yl" | "glycoside") {
                        let locant = first_locant(state.graph(), potential_carbonyl)
                            .ok_or_else(|| error("OPSIN bug: Carbohydrate carbonyl has no locant"))?
                            .to_owned();
                        arena[next].add_attribute(LOCANT_ATR, locant);
                        if value == "glycoside" {
                            let rule = arena.parent_word_rule(sub_or_root).ok_or_else(|| {
                                error("OPSIN bug: Carbohydrate has no parent word rule")
                            })?;
                            if arena[rule].attribute(WORDRULE_ATR) == Some("simple") {
                                return Err(error(
                                    "A glycoside requires a space-separated substituent e.g. methyl alpha-D-glucopyranoside",
                                ));
                            }
                        }
                    }
                }
                CARBOHYDRATERINGSIZE_EL => {
                    if cyclisation_performed {
                        return Err(error("OPSIN bug: Carbohydate cyclised twice!"));
                    }
                    cyclise_carbohydrate_and_apply_alpha_beta_stereo(
                        state,
                        arena,
                        carbohydrate,
                        next,
                        potential_carbonyl,
                    )?;
                    arena.detach(next);
                    cyclisation_performed = true;
                }
                LOCANT_EL
                | MULTIPLIER_ATR
                | UNSATURATOR_EL
                | COLONORSEMICOLONDELIMITEDLOCANT_EL => {}
                _ => break,
            }
            next_sibling = next_next_sibling;
        }
        if !cyclisation_performed {
            apply_unspecified_ring_size_cyclisation_if_present(
                state,
                arena,
                carbohydrate,
                potential_carbonyl,
            )?;
        }
    }
    Ok(())
}

/// Uses the upstream implicit pyranose/furanose choice only when indicated by
/// a glycosyl suffix or an alpha/beta locant.
pub fn apply_unspecified_ring_size_cyclisation_if_present(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    potential_carbonyl: AtomId,
) -> Result<()> {
    let mut cyclise = false;
    if let Some(possible_yl) = arena
        .next_sibling(group)
        .filter(|&node| arena[node].name == SUFFIX_EL)
    {
        if arena[possible_yl].attribute(VALUE_ATR) == Some("yl") {
            cyclise = true;
        } else if arena.next_sibling(possible_yl).is_some_and(|node| {
            arena[node].name == SUFFIX_EL && arena[node].attribute(VALUE_ATR) == Some("yl")
        }) {
            // (on|uron)osyl
            cyclise = true;
        }
    }
    if !cyclise
        && let Some(locant) = arena
            .previous_sibling_ignoring(group, &[STEREOCHEMISTRY_EL])
            .filter(|&node| arena[node].name == LOCANT_EL)
    {
        cyclise = matches!(
            arena.value(locant).as_str(),
            "alpha" | "beta" | "alpha,beta" | "beta,alpha"
        );
    }
    if cyclise {
        let frag = fragment(arena, group)?;
        let stem = arena.value(group);
        let ring_size = arena.token(CARBOHYDRATERINGSIZE_EL, "");
        let size = if state.graph().atom_by_locant(frag, "5").is_some()
            && stem != "rib"
            && stem != "fruct"
        {
            "6"
        } else {
            "5"
        };
        arena[ring_size].add_attribute(VALUE_ATR, size);
        arena.insert_after(group, ring_size);
        cyclise_carbohydrate_and_apply_alpha_beta_stereo(
            state,
            arena,
            group,
            ring_size,
            potential_carbonyl,
        )?;
        arena.detach(ring_size);
    }
    Ok(())
}

/// Changes hydroxy groups into ketones, replacing aldose functionality only
/// for `ulose`; returns the carbonyl subsequently used for cyclisation.
pub fn process_ulose_suffix(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    suffix: NodeId,
    mut potential_carbonyl: AtomId,
) -> Result<AtomId> {
    let mut locants_to_convert = Vec::new();
    let potential_locant_or_multiplier = arena
        .previous_sibling(suffix)
        .ok_or_else(|| error("OPSIN bug: Ul suffix has no preceding element"))?;
    if arena[potential_locant_or_multiplier].name == MULTIPLIER_ATR {
        let multiplier = integer(&attribute(
            arena,
            potential_locant_or_multiplier,
            VALUE_ATR,
        )?)?;
        if let Some(locant) = arena
            .previous_sibling(potential_locant_or_multiplier)
            .filter(|&node| arena[node].name == LOCANT_EL)
        {
            let value = arena.value(locant);
            let values = split_locants(&value);
            if i32::try_from(values.len()).ok() != Some(multiplier) {
                return Err(error(format!(
                    "Mismatch between locant and multiplier counts ({} and {multiplier}):{value}",
                    values.len()
                )));
            }
            locants_to_convert.extend(values.into_iter().map(str::to_owned));
            arena.detach(locant);
        } else {
            for i in 0..multiplier {
                locants_to_convert.push((i + 2).to_string());
            }
        }
        arena.detach(potential_locant_or_multiplier);
    } else {
        let locant = if arena[potential_locant_or_multiplier].name == LOCANT_EL {
            Some(potential_locant_or_multiplier)
        } else {
            arena.previous_sibling(group)
        };
        if let Some(locant) = locant.filter(|&node| arena[node].name == LOCANT_EL) {
            let value = arena.value(locant);
            if split_locants(&value).len() != 1 {
                return Err(error(format!(
                    "Incorrect number of locants for ul suffix: {value}"
                )));
            }
            locants_to_convert.push(value);
            arena.detach(locant);
        } else {
            locants_to_convert.push("2".into());
        }
    }
    let frag = fragment(arena, group)?;
    if arena[suffix].attribute(VALUE_ATR) == Some("ulose") {
        let bond = state
            .graph()
            .atom(potential_carbonyl)
            .bonds
            .iter()
            .copied()
            .find(|&bond| {
                let bond = state.graph().bond(bond);
                let other = state
                    .graph()
                    .atom(bond.other_atom(potential_carbonyl).expect("incident bond"));
                bond.order == 2
                    && other.element == ChemEl::O
                    && other.charge == 0
                    && other.bonds.len() == 1
            })
            .ok_or_else(|| error("OPSIN bug: Unable to convert aldose to ketose"))?;
        state.graph_mut().bond_mut(bond).order = 1;
        let first = locants_to_convert
            .first()
            .ok_or_else(|| error("OPSIN bug: No ketone locants for ul suffix"))?;
        potential_carbonyl = atom_by_locant(state.graph(), frag, first)?;
    }
    for locant in locants_to_convert {
        let backbone = atom_by_locant(state.graph(), frag, &locant)?;
        let bond = state
            .graph()
            .atom(backbone)
            .bonds
            .iter()
            .copied()
            .find(|&bond| {
                let bond = state.graph().bond(bond);
                let other = state
                    .graph()
                    .atom(bond.other_atom(backbone).expect("incident bond"));
                bond.order == 1
                    && other.element == ChemEl::O
                    && other.charge == 0
                    && other.bonds.len() == 1
            })
            .ok_or_else(|| error(format!("Failed to find hydroxy group at position:{locant}")))?;
        state.graph_mut().bond_mut(bond).order = 2;
        state.graph_mut().atom_mut(backbone).parity = None;
    }
    Ok(potential_carbonyl)
}

/// Forms the intrafragment ring bond, then assigns alpha/beta stereochemistry.
pub fn cyclise_carbohydrate_and_apply_alpha_beta_stereo(
    state: &mut BuildState,
    arena: &mut Arena,
    carbohydrate_group: NodeId,
    ring_size: NodeId,
    potential_carbonyl: AtomId,
) -> Result<()> {
    let frag = fragment(arena, carbohydrate_group)?;
    let ring_size_value = attribute(arena, ring_size, VALUE_ATR)?;
    let size = integer(&ring_size_value)?;
    let mut carbonyl_carbon = potential_carbonyl;
    let mut atom_to_join_with = None;
    let potential_locant = arena
        .previous_sibling(ring_size)
        .ok_or_else(|| error("OPSIN bug: Carbohydrate ring size has no preceding element"))?;
    if arena[potential_locant].name == LOCANT_EL {
        let value = arena.value(potential_locant);
        let locants = split_locants(&value);
        if locants.len() != 2 {
            return Err(error(format!(
                "Expected 2 locants in front of sugar ring size specifier but found: {value}"
            )));
        }
        let numeric = locants[0]
            .parse::<i32>()
            .ok()
            .zip(locants[1].parse::<i32>().ok())
            .ok_or_else(|| {
                error(format!(
                    "Locants for ring should be numeric but were: {value}"
                ))
            })?;
        let difference = numeric.1.wrapping_sub(numeric.0).wrapping_abs();
        if difference != size.wrapping_sub(2) {
            return Err(error(format!(
                "Mismatch between ring size: {ring_size_value} and ring size specified by locants: {}",
                difference.wrapping_add(2)
            )));
        }
        carbonyl_carbon = atom_by_locant(state.graph(), frag, locants[0])?;
        atom_to_join_with = Some(atom_by_locant(
            state.graph(),
            frag,
            &format!("O{}", locants[1]),
        )?);
        arena.detach(potential_locant);
    }
    if let Some(bond) = state
        .graph()
        .atom(carbonyl_carbon)
        .bonds
        .iter()
        .copied()
        .find(|&bond| state.graph().bond(bond).order == 2)
    {
        state.graph_mut().bond_mut(bond).order = 1;
    }
    let carbonyl_locant = first_locant(state.graph(), carbonyl_carbon)
        .and_then(|locant| locant.parse::<i32>().ok())
        .ok_or_else(|| {
            error("OPSIN bug: Could not determine locant of carbonyl carbon in carbohydrate")
        })?;
    let atom_to_join_with = match atom_to_join_with {
        Some(atom) => atom,
        None => {
            let locant = carbonyl_locant.wrapping_add(size).wrapping_sub(2);
            state.graph().atom_by_locant(frag, &format!("O{locant}"))
                .ok_or_else(|| error(format!("Carbohydrate was not an inappropriate length to form a ring of size: {ring_size_value}")))?
        }
    };
    state
        .fragment_manager
        .create_bond(carbonyl_carbon, atom_to_join_with, 1)
        .map_err(|error| ParsingError(error.to_string()))?;
    crate::cycle_detector::assign_cycle_membership(state.graph_mut(), frag);
    apply_alpha_beta_stereo_to_cyclised_carbohydrate(
        state,
        arena,
        carbohydrate_group,
        carbonyl_carbon,
    )
}

pub fn apply_alpha_beta_stereo_to_cyclised_carbohydrate(
    state: &mut BuildState,
    arena: &mut Arena,
    carbohydrate_group: NodeId,
    carbonyl_carbon: AtomId,
) -> Result<()> {
    let frag = fragment(arena, carbohydrate_group)?;
    if let Some(locant) = arena
        .previous_sibling_ignoring(carbohydrate_group, &[STEREOCHEMISTRY_EL])
        .filter(|&node| arena[node].name == LOCANT_EL)
    {
        let stereo_prefix_after_alpha_beta = arena.next_sibling(locant);
        let reference = get_anomeric_reference_atom(state.graph(), frag).ok_or_else(|| {
            error(format!(
                "OPSIN bug: Unable to determine anomeric reference atom in: {}",
                arena.value(carbohydrate_group)
            ))
        })?;
        apply_anomer_stereochemistry_if_present(
            state.graph_mut(),
            arena,
            locant,
            carbonyl_carbon,
            reference,
        )?;
        if state.graph().atom(carbonyl_carbon).parity.is_some()
            && matches!(
                arena[carbohydrate_group].attribute(SUBTYPE_ATR),
                Some(
                    SYSTEMATICCARBOHYDRATESTEMALDOSE_SUBTYPE_VAL
                        | SYSTEMATICCARBOHYDRATESTEMKETOSE_SUBTYPE_VAL
                )
            )
        {
            // Systematic chains receive their final configurations later. The
            // final prefix character is l for L and r for D at this stage.
            let prefix = stereo_prefix_after_alpha_beta
                .ok_or_else(|| error("OPSIN bug: Missing carbohydrate stereoprefix"))?;
            let value = attribute(arena, prefix, VALUE_ATR)?;
            if value.ends_with('l') {
                let parity = state
                    .graph_mut()
                    .atom_mut(carbonyl_carbon)
                    .parity
                    .as_mut()
                    .expect("checked parity");
                parity.parity = -parity.parity;
            }
        }
    }
    state
        .graph_mut()
        .atom_mut(carbonyl_carbon)
        .properties
        .is_anomeric = true;
    Ok(())
}

/// Adds the second aldehyde or both acid functionalities directly, as upstream
/// does before ordinary suffix application.
pub fn process_aldose_di_suffix(
    state: &mut BuildState,
    arena: &mut Arena,
    suffix_value: &str,
    group: NodeId,
    aldehyde_atom: AtomId,
) -> Result<()> {
    let frag = fragment(arena, group)?;
    let alcohol_atom = atom_by_locant(
        state.graph(),
        frag,
        &chain_length(state.graph(), frag).to_string(),
    )?;
    match suffix_value {
        "aric acid" | "arate" => {
            state
                .fragment_manager
                .remove_terminal_oxygen(alcohol_atom, 1)
                .map_err(|error| ParsingError(error.to_string()))?;
            incorporate_oxygen(state, arena, group, frag, alcohol_atom, 2, false)?;
            let hydroxy = incorporate_oxygen(
                state,
                arena,
                group,
                frag,
                alcohol_atom,
                1,
                suffix_value == "arate",
            )?;
            state
                .graph_mut()
                .fragment_mut(frag)
                .functional_atoms
                .push(hydroxy);
            let hydroxy = incorporate_oxygen(
                state,
                arena,
                group,
                frag,
                aldehyde_atom,
                1,
                suffix_value == "arate",
            )?;
            state
                .graph_mut()
                .fragment_mut(frag)
                .functional_atoms
                .push(hydroxy);
        }
        "dialdose" => {
            state
                .fragment_manager
                .remove_terminal_oxygen(alcohol_atom, 1)
                .map_err(|error| ParsingError(error.to_string()))?;
            incorporate_oxygen(state, arena, group, frag, alcohol_atom, 2, false)?;
        }
        _ => {
            return Err(error(format!(
                "OPSIN Bug: Unexpected suffix value: {suffix_value}"
            )));
        }
    }
    Ok(())
}

fn incorporate_oxygen(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    frag: FragmentId,
    carbon: AtomId,
    order: u8,
    deprotonate: bool,
) -> Result<AtomId> {
    // buildSMILES(token) stores the token on this auxiliary fragment without
    // changing the token's primary fragment association.
    let new_fragment = state
        .fragment_manager
        .build_token_smiles("O", arena, group, NONE_LABELS_VAL)
        .map_err(|error| ParsingError(error.to_string()))?;
    let oxygen = state.graph().fragment(new_fragment).atoms[0];
    if deprotonate {
        state.graph_mut().atom_mut(oxygen).charge -= 1;
        state
            .graph_mut()
            .atom_mut(oxygen)
            .protons_explicitly_added_or_removed -= 1;
    }
    state
        .fragment_manager
        .incorporate_fragment_with_bond(new_fragment, oxygen, frag, carbon, order)
        .map_err(|error| ParsingError(error.to_string()))?;
    Ok(oxygen)
}

/// Selects the defined stereocentre with the highest numeric first locant.
pub fn get_anomeric_reference_atom(graph: &Graph, frag: FragmentId) -> Option<AtomId> {
    let mut highest = i32::MIN;
    let mut configurational_atom = None;
    for &atom in &graph.fragment(frag).atoms {
        if graph.atom(atom).parity.is_none() {
            continue;
        }
        if let Some(locant) =
            first_locant(graph, atom).and_then(|locant| locant.parse::<i32>().ok())
            && locant > highest
        {
            highest = locant;
            configurational_atom = Some(atom);
        }
    }
    configurational_atom
}

pub fn apply_anomer_stereochemistry_if_present(
    graph: &mut Graph,
    arena: &mut Arena,
    alpha_or_beta_locant: NodeId,
    anomeric_atom: AtomId,
    anomeric_reference_atom: AtomId,
) -> Result<()> {
    let value = arena.value(alpha_or_beta_locant);
    match value.as_str() {
        "alpha" | "beta" => {
            let references =
                get_deterministic_atom_refs4_for_reference_atom(graph, anomeric_reference_atom)?;
            let reference_parity = graph
                .atom(anomeric_reference_atom)
                .parity
                .as_ref()
                .ok_or_else(|| error("OPSIN bug: Anomeric reference atom has no parity"))?;
            let flip = check_equivalency_of_atom_refs4_and_parity(
                &references,
                1,
                &reference_parity.atom_refs,
                reference_parity.parity,
            )?;
            let references = get_deterministic_atom_refs4_for_anomeric_atom(graph, anomeric_atom)?;
            let parity = match (flip, value.as_str()) {
                (true, "alpha") | (false, "beta") => 1,
                _ => -1,
            };
            graph.atom_mut(anomeric_atom).parity = Some(AtomParity::new(references, parity));
            arena.detach(alpha_or_beta_locant);
        }
        "alpha,beta" | "beta,alpha" => {
            arena.detach(alpha_or_beta_locant);
        }
        _ => {}
    }
    Ok(())
}

pub fn get_deterministic_atom_refs4_for_reference_atom(
    graph: &Graph,
    reference_atom: AtomId,
) -> Result<[Option<StereoReference>; 4]> {
    let neighbours = graph.neighbours(reference_atom);
    if neighbours.len() != 3 {
        return Err(error(
            "OPSIN bug: Unexpected number of atoms connected to anomeric reference atom of carbohydrate",
        ));
    }
    let next_lowest = first_locant(graph, reference_atom)
        .and_then(|locant| locant.parse::<i32>().ok())
        .and_then(|locant| locant.checked_sub(1))
        .ok_or_else(|| error("OPSIN bug: Anomeric reference locant is not numeric"))?
        .to_string();
    let mut references = [None; 4];
    for neighbour in neighbours {
        let index = match graph.atom(neighbour).element {
            ChemEl::O => 0,
            ChemEl::C if first_locant(graph, neighbour) == Some(next_lowest.as_str()) => 1,
            ChemEl::C => 2,
            _ => {
                return Err(error(
                    "OPSIN bug: Unexpected atom element type connected to for anomeric reference atom",
                ));
            }
        };
        references[index] = Some(StereoReference::Atom(neighbour));
    }
    references[3] = Some(StereoReference::ImplicitHydrogen);
    if references.contains(&None) {
        return Err(error(
            "OPSIN bug: Unable to determine atomRefs4 for anomeric reference atom",
        ));
    }
    Ok(references)
}

pub fn get_deterministic_atom_refs4_for_anomeric_atom(
    graph: &Graph,
    anomeric_atom: AtomId,
) -> Result<[Option<StereoReference>; 4]> {
    let neighbours = graph.neighbours(anomeric_atom);
    let mut references = [None; 4];
    match neighbours.len() {
        3 | 4 => {}
        2 if graph.atom(anomeric_atom).out_valency == 1 => {
            references[1] = Some(StereoReference::DeoxyHydrogen);
        }
        _ => {
            return Err(error(
                "OPSIN bug: Unexpected number of atoms connected to anomeric atom of carbohydrate",
            ));
        }
    }
    for neighbour in neighbours {
        let index = match graph.atom(neighbour).element {
            ChemEl::C if graph.atom(neighbour).in_cycle => 0,
            ChemEl::C => 3,
            ChemEl::O => match graph.incoming_valency(neighbour) {
                1 => 1,
                2 => 2,
                _ => {
                    return Err(error(
                        "OPSIN bug: Unexpected valency on oxygen in carbohydrate",
                    ));
                }
            },
            _ => {
                return Err(error(
                    "OPSIN bug: Unexpected atom element type connected to anomeric atom of carbohydrate",
                ));
            }
        };
        references[index] = Some(StereoReference::Atom(neighbour));
    }
    if references[3].is_none() {
        references[3] = Some(StereoReference::ImplicitHydrogen);
    }
    if references.contains(&None) {
        return Err(error(
            "OPSIN bug: Unable to assign anomeric carbon stereochemistry on carbohydrate",
        ));
    }
    Ok(references)
}

// Validate the construction references, then use the ported upstream stable
// atom-ID sort/parity comparison. Both dummy hydrogens have source ID 0.
fn check_equivalency_of_atom_refs4_and_parity(
    first: &[Option<StereoReference>; 4],
    first_parity: i8,
    second: &[Option<StereoReference>; 4],
    second_parity: i8,
) -> Result<bool> {
    if first
        .iter()
        .chain(second)
        .any(|reference| matches!(reference, None | Some(StereoReference::RingOpening)))
    {
        return Err(error("OPSIN bug: Invalid carbohydrate atomRefs4"));
    }
    Ok(
        crate::stereochemistry_handler::check_equivalency_of_atom_refs_and_parity(
            first,
            first_parity,
            second,
            second_parity,
        ),
    )
}
