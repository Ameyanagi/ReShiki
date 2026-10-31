//! Amplification of simplified phane skeletons from OPSIN `PhaneAmplifier.java`.
//!
//! Source: upstream main, 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91,
//! `PhaneAmplifier.java`, lines 20–268.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
//! Inventory: Amplification; processPhaneAmplification;
//! warnIfAmbiguousLocantsAreUsed; collectPlainIntegers; getBondingAtom;
//! isRemainingSkeletonLocant; parseAmplifications.

use crate::ParsingError;
use crate::build_state::BuildState;
use crate::graph::{AtomId, Element, FragmentId, GraphError};
use crate::parse_tree::{Arena, NodeId};
use crate::xml_declarations::*;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

type Result<T> = std::result::Result<T, ParsingError>;

static MATCH_PLAIN_INTEGER: LazyLock<Regex> = LazyLock::new(|| Regex::new("[0-9]+").unwrap());
static MATCH_AMPLIFICATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([1-9][0-9]*(?:,[1-9][0-9]*)*)\(([^()]+)\)").unwrap());

struct Amplification {
    superatom: usize,
    attachment_locants: Vec<String>,
    amplificant: Option<FragmentId>,
}

fn error(message: impl Into<String>) -> ParsingError {
    ParsingError(message.into())
}

fn graph_error(error: GraphError) -> ParsingError {
    ParsingError(error.to_string())
}

fn fragment(arena: &Arena, group: NodeId) -> Result<FragmentId> {
    arena[group].fragment.ok_or_else(|| {
        error(format!(
            "No fragment associated with {}",
            arena.value(group)
        ))
    })
}

fn locanted_atom(state: &BuildState, fragment: FragmentId, locant: &str) -> Result<AtomId> {
    state
        .graph()
        .atom_by_locant(fragment, locant)
        .ok_or_else(|| error(format!("Could not find the atom with locant {locant}.")))
}

// Character.isDigit(char) / Character.digit(char, 10), as used by the source.
// Supplementary characters are surrogate pairs to these Java char operations.
fn decimal_digit(character: char) -> Option<u32> {
    const ZEROES: [u32; 37] = [
        0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66,
        0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946,
        0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0,
        0xa9f0, 0xaa50, 0xabf0, 0xff10,
    ];
    let codepoint = u32::from(character);
    ZEROES
        .iter()
        .find_map(|&zero| codepoint.checked_sub(zero).filter(|&digit| digit < 10))
}

fn integer(text: Option<&str>) -> Result<i32> {
    let Some(text) = text else {
        return Err(error("Cannot parse null string"));
    };
    let invalid = || error(format!("For input string: \"{text}\""));
    let (digits, negative) = if let Some(digits) = text.strip_prefix('-') {
        (digits, true)
    } else {
        (text.strip_prefix('+').unwrap_or(text), false)
    };
    if digits.is_empty() {
        return Err(invalid());
    }
    let limit = if negative { i32::MIN } else { -i32::MAX };
    let mut result = 0i32;
    for character in digits.chars() {
        let digit = decimal_digit(character).ok_or_else(invalid)? as i32;
        result = result
            .checked_mul(10)
            .and_then(|number| number.checked_sub(digit))
            .filter(|&number| number >= limit)
            .ok_or_else(invalid)?;
    }
    Ok(if negative { result } else { -result })
}

/// Replaces superatoms with amplificants, preserving skeleton-neighbour order.
pub fn process_phane_amplification(
    state: &mut BuildState,
    arena: &mut Arena,
    sub_or_root: NodeId,
) -> Result<()> {
    let mut skeleton_group = None;
    for group in arena.children_named(sub_or_root, GROUP_EL) {
        if arena[group].attribute(SUBTYPE_ATR) == Some(PHANE_SUBTYPE_VAL) {
            skeleton_group = Some(group);
        }
    }
    let Some(skeleton_group) = skeleton_group else {
        return Ok(());
    };
    let skeleton = fragment(arena, skeleton_group)?;
    let node_count = state.graph().fragment(skeleton).atoms.len();

    let mut amplifications = Vec::new();
    let mut consumed = Vec::new();
    let mut pending = None;
    let mut pending_multiplier = None;
    for element in arena[sub_or_root].children.clone() {
        if element == skeleton_group {
            break;
        }
        match arena[element].name.as_str() {
            PHANELOCANTS_EL => {
                pending = Some(parse_amplifications(&arena.value(element), node_count)?);
                consumed.push(element);
            }
            MULTIPLIER_EL => {
                pending_multiplier = Some(integer(arena[element].attribute(VALUE_ATR))?);
                consumed.push(element);
            }
            GROUP_EL => {
                let Some(mut current) = pending.take() else {
                    return Err(error(format!(
                        "Amplificant {} has no locants indicating the superatoms it replaces",
                        arena.value(element)
                    )));
                };
                if pending_multiplier
                    .is_some_and(|multiplier| i64::from(multiplier) != current.len() as i64)
                    || pending_multiplier.is_none() && current.len() != 1
                {
                    return Err(error(
                        "Disagreement between the multiplier and the number of superatoms of the amplificant",
                    ));
                }
                let template = fragment(arena, element)?;
                for (index, amplification) in current.iter_mut().enumerate() {
                    amplification.amplificant = Some(if index == 0 {
                        template
                    } else {
                        state
                            .fragment_manager
                            .copy_fragment(template)
                            .map_err(graph_error)?
                    });
                }
                amplifications.extend(current);
                consumed.push(element);
                pending_multiplier = None;
            }
            STRUCTURALOPENBRACKET_EL | STRUCTURALCLOSEBRACKET_EL => {
                consumed.push(element);
            }
            _ => {}
        }
    }
    if pending.is_some() {
        return Err(error(
            "Amplification locants are not followed by an amplificant",
        ));
    }
    if amplifications.is_empty() {
        return Ok(());
    }

    let mut nodes = vec![None; node_count + 1];
    for (index, node) in nodes.iter_mut().enumerate().skip(1) {
        *node = Some(locanted_atom(state, skeleton, &index.to_string())?);
    }
    let mut neighbours = vec![Vec::new()];
    for node in nodes.iter().skip(1) {
        let mut locants = Vec::new();
        for neighbour in state.graph().neighbours(node.expect("skeleton node")) {
            let locant = state.graph().atom(neighbour).locants.first();
            locants.push(integer(locant.map(String::as_str))? as usize);
        }
        locants.sort();
        neighbours.push(locants);
    }

    let mut by_superatom = vec![None; node_count + 1];
    for (index, amplification) in amplifications.iter().enumerate() {
        let superatom = amplification.superatom;
        if by_superatom[superatom].is_some() {
            return Err(error(format!(
                "Skeleton node {superatom} is amplified more than once"
            )));
        }
        by_superatom[superatom] = Some(index);
        if amplification.attachment_locants.len() != neighbours[superatom].len() {
            return Err(error(format!(
                "Superatom {superatom} is bonded to {} skeleton nodes but {} attachment locants were given",
                neighbours[superatom].len(),
                amplification.attachment_locants.len()
            )));
        }
    }

    for amplification in &amplifications {
        state
            .graph_mut()
            .clear_locants(nodes[amplification.superatom].expect("skeleton node"));
    }
    // The source uses a LinkedHashMap: preserve first insertion and owner order.
    let mut concatenated_locant_owners: Vec<(String, Vec<AtomId>)> = Vec::new();
    let mut attachment_atoms = vec![None; node_count + 1];
    for amplification in &amplifications {
        let amplificant = amplification.amplificant.expect("resolved amplificant");
        let mut attachments = Vec::new();
        for locant in &amplification.attachment_locants {
            attachments.push(locanted_atom(state, amplificant, locant)?);
        }
        for &attachment in &attachments {
            let skeleton_bonds = attachments
                .iter()
                .filter(|&&other| other == attachment)
                .count();
            // A mancude carbon needs one valency for its double bond and can
            // therefore form at most three sigma bonds.
            if state.graph().atom(attachment).element == Element::C
                && state.graph().atom(attachment).spare_valency
                && i64::from(state.graph().incoming_valency(attachment)) + skeleton_bonds as i64 > 3
            {
                return Err(error(format!(
                    "The mancude amplificant atom {} cannot form the requested number of bonds to the simplified skeleton",
                    state
                        .graph()
                        .atom(attachment)
                        .locants
                        .first()
                        .map_or("null", String::as_str)
                )));
            }
        }
        attachment_atoms[amplification.superatom] = Some(attachments);
        for atom in state.graph().fragment(amplificant).atoms.clone() {
            let locants = state.graph().atom(atom).locants.clone();
            state.graph_mut().clear_locants(atom);
            for locant in locants {
                state
                    .graph_mut()
                    .add_locant(atom, format!("{}^{locant}", amplification.superatom));
                if locant.chars().next().and_then(decimal_digit).is_some() {
                    let concatenated = format!("{}{locant}", amplification.superatom);
                    if let Some((_, owners)) = concatenated_locant_owners
                        .iter_mut()
                        .find(|(locant, _)| *locant == concatenated)
                    {
                        owners.push(atom);
                    } else {
                        concatenated_locant_owners.push((concatenated, vec![atom]));
                    }
                }
            }
        }
    }
    let mut ambiguous_locants = Vec::new();
    for (locant, owners) in concatenated_locant_owners {
        let skeleton_atom_has_locant =
            is_remaining_skeleton_locant(&locant, node_count, &by_superatom);
        if owners.len() == 1 && !skeleton_atom_has_locant {
            state.graph_mut().add_locant(owners[0], locant);
        } else if skeleton_atom_has_locant || owners.len() > 1 {
            ambiguous_locants.push(locant);
        }
    }
    if !ambiguous_locants.is_empty() {
        warn_if_ambiguous_locants_are_used(state, arena, sub_or_root, &ambiguous_locants)?;
    }
    for amplification in &amplifications {
        state
            .fragment_manager
            .incorporate_fragment(
                amplification.amplificant.expect("resolved amplificant"),
                skeleton,
            )
            .map_err(graph_error)?;
    }

    for i in 1..=node_count {
        for &j in &neighbours[i] {
            if j < i || by_superatom[i].is_none() && by_superatom[j].is_none() {
                continue;
            }
            let from = get_bonding_atom(i, j, &nodes, &neighbours, &attachment_atoms);
            let to = get_bonding_atom(j, i, &nodes, &neighbours, &attachment_atoms);
            state
                .fragment_manager
                .create_bond(from, to, 1)
                .map_err(graph_error)?;
        }
    }
    for amplification in &amplifications {
        state.fragment_manager.remove_atom_and_associated_bonds(
            nodes[amplification.superatom].expect("skeleton node"),
        );
    }
    for element in consumed {
        arena.detach(element);
    }
    Ok(())
}

/// Inline superscripts can coincide with surviving skeleton locants.
fn warn_if_ambiguous_locants_are_used(
    state: &mut BuildState,
    arena: &Arena,
    sub_or_root: NodeId,
    ambiguous_locants: &[String],
) -> Result<()> {
    let scope = arena
        .parent_word_rule(sub_or_root)
        .ok_or_else(|| error("Cannot find enclosing wordRule element"))?;
    let mut used = HashSet::new();
    for element in arena.descendants_named(scope, LOCANT_EL) {
        let value = arena.value(element);
        if value.contains('^') {
            return Ok(());
        }
        collect_plain_integers(&value, &mut used);
    }
    for element in arena.descendants_named_any(
        scope,
        &[
            INDICATEDHYDROGEN_EL,
            ADDEDHYDROGEN_EL,
            HYDRO_EL,
            HETEROATOM_EL,
            UNSATURATOR_EL,
            SUFFIX_EL,
            STEREOCHEMISTRY_EL,
        ],
    ) {
        if let Some(locant) = arena[element].attribute(LOCANT_ATR) {
            if locant.contains('^') {
                return Ok(());
            }
            collect_plain_integers(locant, &mut used);
        }
    }
    for index in ambiguous_locant_iteration_order(ambiguous_locants) {
        let locant = &ambiguous_locants[index];
        if used.contains(locant.as_str()) {
            state.add_is_ambiguous(format!(
                "The locant {locant} could refer to an atom of the simplified skeleton or to an atom of an amplificant (superscript locants should be written e.g. 1^4)"
            ));
        }
    }
    Ok(())
}

/// The source iterates a HashSet: buckets in index order, insertion order within
/// a bucket (resizing keeps it). Treeified bins need 8+ colliding locants, which
/// the name grammar's node limit cannot produce, so they are not modelled.
fn ambiguous_locant_iteration_order(locants: &[String]) -> Vec<usize> {
    let mut capacity = 16;
    while locants.len() > capacity * 3 / 4 {
        capacity *= 2;
    }
    let bucket = |locant: &String| {
        let hash = locant.encode_utf16().fold(0u32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(u32::from(unit))
        });
        (hash ^ (hash >> 16)) as usize & (capacity - 1)
    };
    let mut order: Vec<usize> = (0..locants.len()).collect();
    order.sort_by_key(|&index| bucket(&locants[index]));
    order
}

fn collect_plain_integers(text: &str, used: &mut HashSet<String>) {
    // Rust regexes omit lookarounds; test the source pattern's adjacent ASCII
    // digit/caret exclusions around each complete digit run.
    let bytes = text.as_bytes();
    for matched in MATCH_PLAIN_INTEGER.find_iter(text) {
        let blocked_before = matched.start() > 0
            && (bytes[matched.start() - 1].is_ascii_digit() || bytes[matched.start() - 1] == b'^');
        let blocked_after = matched.end() < bytes.len()
            && (bytes[matched.end()].is_ascii_digit() || bytes[matched.end()] == b'^');
        if !blocked_before && !blocked_after {
            used.insert(matched.as_str().to_owned());
        }
    }
}

fn get_bonding_atom(
    node: usize,
    neighbour: usize,
    nodes: &[Option<AtomId>],
    neighbours: &[Vec<usize>],
    attachment_atoms: &[Option<Vec<AtomId>>],
) -> AtomId {
    let Some(attachments) = &attachment_atoms[node] else {
        return nodes[node].expect("skeleton node");
    };
    attachments[neighbours[node]
        .iter()
        .position(|&other| other == neighbour)
        .expect("skeleton neighbour")]
}

fn is_remaining_skeleton_locant(
    locant: &str,
    node_count: usize,
    by_superatom: &[Option<usize>],
) -> bool {
    if !locant
        .chars()
        .all(|character| decimal_digit(character).is_some())
        || locant.encode_utf16().count() > 4
    {
        return false;
    }
    let number = integer(Some(locant)).expect("at most four decimal digits") as usize;
    number <= node_count && by_superatom[number].is_none()
}

fn parse_amplifications(text: &str, node_count: usize) -> Result<Vec<Amplification>> {
    let mut amplifications = Vec::new();
    for matched in MATCH_AMPLIFICATION.captures_iter(text.strip_suffix('-').unwrap_or(text)) {
        let mut attachments: Vec<String> = matched[2].split(',').map(str::to_owned).collect();
        // String.split(",") discards all trailing empty fields in Java.
        while attachments.last().is_some_and(String::is_empty) {
            attachments.pop();
        }
        for locant in matched[1].split(',') {
            let superatom = integer(Some(locant))? as usize;
            if superatom > node_count {
                return Err(error(format!(
                    "Superatom locant {superatom} exceeds the number of skeleton nodes: {node_count}"
                )));
            }
            amplifications.push(Amplification {
                superatom,
                attachment_locants: attachments.clone(),
                amplificant: None,
            });
        }
    }
    Ok(amplifications)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParseOptions;

    fn token(arena: &mut Arena, parent: NodeId, name: &str, value: &str) -> NodeId {
        let token = arena.token(name, value);
        arena.add_child(parent, token);
        token
    }

    #[test]
    fn amplifies_adjacent_superatoms_in_sorted_neighbour_order() {
        let mut state = BuildState::new(ParseOptions::default());
        let mut arena = Arena::default();
        let root = arena.grouping(ROOT_EL);
        let locants = token(&mut arena, root, PHANELOCANTS_EL, "1,2(1,4)-");
        let multiplier = token(&mut arena, root, MULTIPLIER_EL, "di");
        arena[multiplier].add_attribute(VALUE_ATR, "2");
        let amplificant = token(&mut arena, root, GROUP_EL, "benzena");
        let template = state
            .fragment_manager
            .build_smiles("c1ccccc1", "ring", "numeric")
            .unwrap();
        let template_atoms = state.graph().fragment(template).atoms.clone();
        arena[amplificant].fragment = Some(template);
        let skeleton_group = token(&mut arena, root, GROUP_EL, "cyclotetraphane");
        arena[skeleton_group].add_attribute(SUBTYPE_ATR, PHANE_SUBTYPE_VAL);
        let skeleton = state
            .fragment_manager
            .build_smiles("C1CCC1", "ring", "numeric")
            .unwrap();
        let skeleton_atoms = state.graph().fragment(skeleton).atoms.clone();
        arena[skeleton_group].fragment = Some(skeleton);

        process_phane_amplification(&mut state, &mut arena, root).unwrap();

        assert_eq!(arena[root].children, vec![skeleton_group]);
        assert!(arena[locants].parent.is_none());
        assert!(arena[multiplier].parent.is_none());
        assert!(arena[amplificant].parent.is_none());
        assert_eq!(state.graph().fragment(skeleton).atoms.len(), 14);
        assert_eq!(state.graph().fragment(skeleton).bonds.len(), 16);
        let atoms = &state.graph().fragment(skeleton).atoms;
        assert_eq!(&atoms[..2], &skeleton_atoms[2..]);
        assert_eq!(&atoms[2..8], template_atoms);
        for (first, second) in [("1^1", "2^1"), ("1^4", "4"), ("2^4", "3")] {
            let first = locanted_atom(&state, skeleton, first).unwrap();
            let second = locanted_atom(&state, skeleton, second).unwrap();
            assert!(state.graph().bond_between(first, second).is_some());
        }
        for removed in &skeleton_atoms[..2] {
            assert!(!state.graph().atom(*removed).active);
        }
        assert_eq!(
            state.graph().atom(template_atoms[0]).locants,
            vec!["1^1", "11"]
        );
    }

    #[test]
    fn ambiguous_warnings_follow_java_buckets_after_resize() {
        let mut state = BuildState::new(ParseOptions::default());
        let mut arena = Arena::default();
        let word_rule = arena.grouping(WORDRULE_EL);
        let root = arena.grouping(ROOT_EL);
        arena.add_child(word_rule, root);
        token(
            &mut arena,
            root,
            LOCANT_EL,
            "11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26",
        );
        let ambiguous: Vec<_> = (11..=26).rev().map(|number| number.to_string()).collect();

        warn_if_ambiguous_locants_are_used(&mut state, &arena, root, &ambiguous).unwrap();

        let expected = [
            "22", "11", "23", "12", "24", "13", "25", "14", "26", "15", "16", "17", "18", "19",
            "20", "21",
        ];
        assert_eq!(state.warnings.len(), expected.len());
        for (warning, locant) in state.warnings.iter().zip(expected) {
            assert_eq!(
                warning.message,
                format!(
                    "The locant {locant} could refer to an atom of the simplified skeleton or to an atom of an amplificant (superscript locants should be written e.g. 1^4)"
                )
            );
        }
    }

    #[test]
    fn explicit_superscripts_in_word_rule_scope_suppress_all_ambiguity_warnings() {
        let mut state = BuildState::new(ParseOptions::default());
        let mut arena = Arena::default();
        let word_rule = arena.grouping(WORDRULE_EL);
        let root = arena.grouping(ROOT_EL);
        arena.add_child(word_rule, root);
        token(&mut arena, root, LOCANT_EL, "11,12");
        let sibling = arena.grouping(SUBSTITUENT_EL);
        arena.add_child(word_rule, sibling);
        let suffix = token(&mut arena, sibling, SUFFIX_EL, "ol");
        arena[suffix].add_attribute(LOCANT_ATR, "1^2");

        warn_if_ambiguous_locants_are_used(
            &mut state,
            &arena,
            root,
            &["11".to_owned(), "12".to_owned()],
        )
        .unwrap();

        assert!(state.warnings.is_empty());
    }

    #[test]
    fn amplification_and_plain_integer_patterns_preserve_source_boundaries() {
        let parsed = parse_amplifications("1,4(1,3),5(2,7)-", 6).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].superatom, 1);
        assert_eq!(parsed[1].superatom, 4);
        assert_eq!(parsed[1].attachment_locants, ["1", "3"]);
        assert_eq!(parsed[2].superatom, 5);
        assert_eq!(parsed[2].attachment_locants, ["2", "7"]);
        assert_eq!(
            parse_amplifications("7(1,4)-", 6).err().unwrap().0,
            "Superatom locant 7 exceeds the number of skeleton nodes: 6"
        );
        let mut used = HashSet::new();
        collect_plain_integers("a12,003;1^4 42^12 ^7 8^ 9,-15", &mut used);
        assert_eq!(
            used,
            HashSet::from(["12", "003", "9", "15"].map(str::to_owned))
        );
        assert_eq!(integer(Some("-2147483648")).unwrap(), i32::MIN);
        assert_eq!(integer(Some("+٢١")).unwrap(), 21);
        assert_eq!(
            integer(Some("2147483648")).unwrap_err().0,
            "For input string: \"2147483648\""
        );
    }

    #[test]
    fn invalid_amplifications_report_exact_source_errors() {
        let cases = [
            (
                None,
                Some("2"),
                true,
                "Amplificant benzena has no locants indicating the superatoms it replaces",
            ),
            (
                Some("1,2(1,4)-"),
                None,
                true,
                "Disagreement between the multiplier and the number of superatoms of the amplificant",
            ),
            (
                Some("1,2(1,4)-"),
                Some("3"),
                true,
                "Disagreement between the multiplier and the number of superatoms of the amplificant",
            ),
            (
                Some("1,1(1,4)-"),
                Some("2"),
                true,
                "Skeleton node 1 is amplified more than once",
            ),
            (
                Some("1(1)-"),
                None,
                true,
                "Superatom 1 is bonded to 2 skeleton nodes but 1 attachment locants were given",
            ),
            (
                Some("1(1,7)-"),
                None,
                true,
                "Could not find the atom with locant 7.",
            ),
            (
                Some("1(1,1)-"),
                None,
                true,
                "The mancude amplificant atom 1 cannot form the requested number of bonds to the simplified skeleton",
            ),
            (
                Some("1(1,4)-"),
                None,
                false,
                "Amplification locants are not followed by an amplificant",
            ),
            (
                Some("5(1,4)-"),
                None,
                true,
                "Superatom locant 5 exceeds the number of skeleton nodes: 4",
            ),
        ];
        for (locants, multiplier, with_amplificant, expected) in cases {
            let mut state = BuildState::new(ParseOptions::default());
            let mut arena = Arena::default();
            let root = arena.grouping(ROOT_EL);
            if let Some(locants) = locants {
                token(&mut arena, root, PHANELOCANTS_EL, locants);
            }
            if let Some(multiplier) = multiplier {
                let node = token(&mut arena, root, MULTIPLIER_EL, "di");
                arena[node].add_attribute(VALUE_ATR, multiplier);
            }
            if with_amplificant {
                let group = token(&mut arena, root, GROUP_EL, "benzena");
                arena[group].fragment = Some(
                    state
                        .fragment_manager
                        .build_smiles("c1ccccc1", "ring", "numeric")
                        .unwrap(),
                );
            }
            let group = token(&mut arena, root, GROUP_EL, "cyclotetraphane");
            arena[group].add_attribute(SUBTYPE_ATR, PHANE_SUBTYPE_VAL);
            arena[group].fragment = Some(
                state
                    .fragment_manager
                    .build_smiles("C1CCC1", "ring", "numeric")
                    .unwrap(),
            );

            assert_eq!(
                process_phane_amplification(&mut state, &mut arena, root)
                    .unwrap_err()
                    .0,
                expected
            );
        }
    }
}
