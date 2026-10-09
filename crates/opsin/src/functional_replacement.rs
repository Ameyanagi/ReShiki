//! Functional replacement, ported from OPSIN 2.9.0 `FunctionalReplacement.java`.
//! Source commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
//!
//! The source's atom and suffix insertion order is retained. In particular,
//! replacement ambiguity is represented as a shared ordered set of candidate
//! atom IDs and is left for the later element-locant assignment stage.

use crate::ParsingError;
use crate::build_state::BuildState;
use crate::graph::{AtomId, Element, FragmentId, Graph};
use crate::parse_tree::{Arena, NodeId};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, ParsingError>;

fn error(message: impl Into<String>) -> ParsingError {
    ParsingError(message.into())
}
fn fragment(arena: &Arena, node: NodeId) -> Result<FragmentId> {
    arena[node]
        .fragment
        .ok_or_else(|| error(format!("No fragment associated with {}", arena.value(node))))
}
fn first_atom(graph: &Graph, frag: FragmentId) -> Result<AtomId> {
    graph
        .fragment(frag)
        .atoms
        .first()
        .copied()
        .ok_or_else(|| error("Fragment has no atoms"))
}
fn value_number(arena: &Arena, node: NodeId) -> Result<usize> {
    arena[node]
        .attribute("value")
        .unwrap_or("")
        .parse()
        .map_err(|_| error("Malformed multiplier"))
}
fn numeric_locant(value: &str) -> bool {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"^\d+[a-z]?'*$").unwrap())
        .is_match(value)
}
fn element_locant(value: &str) -> bool {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"^[A-Z][a-z]?'*$").unwrap())
        .is_match(value)
}

/// `OpsinTools.depthFirstSearchForNonSuffixAtomWithLocant`.
fn backbone_with_locant(graph: &Graph, starting: AtomId, target: &str) -> Option<AtomId> {
    let mut stack = vec![starting];
    let mut visited = BTreeSet::new();
    while let Some(current) = stack.pop() {
        visited.insert(current);
        for neighbour in graph.neighbours(current) {
            if visited.contains(&neighbour) {
                continue;
            }
            let atom = graph.atom(neighbour);
            let locants: Vec<_> = atom.locants.iter().filter(|s| !element_locant(s)).collect();
            if !locants.is_empty() && atom.atom_type != "suffix" {
                if locants.iter().any(|s| s.as_str() == target) {
                    return Some(neighbour);
                }
                continue;
            }
            stack.push(neighbour);
        }
    }
    None
}

/// `OpsinTools.depthFirstSearchForAtomWithNumericLocant`.
fn nearest_numeric_locant_atom(graph: &Graph, starting: AtomId) -> Option<AtomId> {
    let mut stack = vec![starting];
    let mut visited = BTreeSet::new();
    while let Some(current) = stack.pop() {
        visited.insert(current);
        for neighbour in graph.neighbours(current) {
            if visited.contains(&neighbour) {
                continue;
            }
            if graph
                .atom(neighbour)
                .locants
                .iter()
                .any(|s| numeric_locant(s))
            {
                return Some(neighbour);
            }
            stack.push(neighbour);
        }
    }
    None
}

fn terminal_oxygens(graph: &Graph, atoms: &[AtomId]) -> Result<(Vec<AtomId>, Vec<AtomId>)> {
    let (mut single, mut double) = (Vec::new(), Vec::new());
    for &atom in atoms {
        if graph.atom(atom).element == Element::O && graph.atom(atom).bonds.len() == 1 {
            match graph.incoming_valency(atom) {
                1 => single.push(atom),
                2 => double.push(atom),
                other => {
                    return Err(error(format!(
                        "Unexpected bond order to oxygen; excepted 1 or 2 found: {other}"
                    )));
                }
            }
        }
    }
    Ok((single, double))
}

fn functional_oxygens_in_group(
    state: &BuildState,
    arena: &Arena,
    group: NodeId,
) -> Result<Vec<AtomId>> {
    let graph = &state.fragment_manager.graph;
    Ok(graph
        .fragment(fragment(arena, group)?)
        .functional_atoms
        .iter()
        .copied()
        .filter(|&a| graph.atom(a).element == Element::O)
        .collect())
}
fn functional_oxygens_in_suffixes(state: &BuildState, arena: &Arena, group: NodeId) -> Vec<AtomId> {
    let graph = &state.fragment_manager.graph;
    let mut result = Vec::new();
    for suffix in arena.next_siblings_named(group, "suffix") {
        if let Some(frag) = arena[suffix].fragment {
            result.extend(
                graph
                    .fragment(frag)
                    .functional_atoms
                    .iter()
                    .copied()
                    .filter(|&a| graph.atom(a).element == Element::O),
            );
        }
    }
    result
}
fn oxygens_in_group(state: &BuildState, arena: &Arena, group: NodeId) -> Result<Vec<AtomId>> {
    let graph = &state.fragment_manager.graph;
    Ok(graph
        .fragment(fragment(arena, group)?)
        .atoms
        .iter()
        .copied()
        .filter(|&a| graph.atom(a).element == Element::O)
        .collect())
}
fn oxygens_in_suffixes(state: &BuildState, arena: &Arena, group: NodeId) -> Vec<AtomId> {
    let graph = &state.fragment_manager.graph;
    let mut result = Vec::new();
    for suffix in arena.next_siblings_named(group, "suffix") {
        if let Some(frag) = arena[suffix].fragment
            && (!graph.fragment(frag).functional_atoms.is_empty()
                || arena[group].attribute("type") == Some("acidStem")
                || arena[suffix].attribute("value") == Some("aldehyde"))
        {
            result.extend(
                graph
                    .fragment(frag)
                    .atoms
                    .iter()
                    .copied()
                    .filter(|&a| graph.atom(a).element == Element::O),
            );
        }
    }
    result
}
fn etheric_oxygens(state: &BuildState, arena: &Arena, group: NodeId) -> Result<Vec<AtomId>> {
    let graph = &state.fragment_manager.graph;
    Ok(oxygens_in_group(state, arena, group)?
        .into_iter()
        .filter(|&a| {
            graph.atom(a).bonds.len() == 2
                && graph.atom(a).charge == 0
                && graph.incoming_valency(a) == 2
        })
        .collect())
}
fn acidic_oxygens(state: &BuildState, arena: &Arena, group: NodeId) -> Result<Vec<AtomId>> {
    let mut atoms = functional_oxygens_in_suffixes(state, arena, group);
    if atoms.is_empty() {
        atoms = functional_oxygens_in_group(state, arena, group)?;
    }
    if atoms.is_empty() {
        for conjunctive in arena.next_siblings_named(group, "conjunctiveSuffixGroup") {
            atoms.extend(functional_oxygens_in_group(state, arena, conjunctive)?);
        }
    }
    Ok(atoms)
}
fn replacement_oxygens(state: &BuildState, arena: &Arena, group: NodeId) -> Result<Vec<AtomId>> {
    let atoms = oxygens_in_suffixes(state, arena, group);
    if atoms.is_empty() {
        oxygens_in_group(state, arena, group)
    } else {
        Ok(atoms)
    }
}

fn pick_oxygens(graph: &Graph, arena: &Arena, locant: NodeId, atoms: &[AtomId]) -> Vec<AtomId> {
    let value = arena.value(locant);
    let locants: Vec<_> = value.split(',').collect();
    let all_p = !locants.is_empty() && locants.iter().all(|s| *s == "P");
    atoms
        .iter()
        .copied()
        .filter(|&atom| {
            let a = graph.atom(atom);
            if !a.locants.is_empty() {
                locants.iter().any(|s| a.locants.iter().any(|l| l == s))
            } else if all_p {
                graph
                    .neighbours(atom)
                    .iter()
                    .any(|&n| graph.atom(n).element == Element::P)
            } else {
                nearest_numeric_locant_atom(graph, atom).is_some_and(|n| {
                    locants
                        .iter()
                        .any(|s| graph.atom(n).locants.iter().any(|l| l == s))
                })
            }
        })
        .collect()
}
fn atom_has_locant(graph: &Graph, atom: AtomId, locant: &str) -> bool {
    let a = graph.atom(atom);
    if a.locants.iter().any(|l| l == locant) {
        return true;
    }
    if let Some((element, primes, backbone)) =
        crate::fragment_tools::parse_amino_acid_style_locant(locant)
    {
        if a.element.symbol() != element {
            return false;
        }
        if !primes.is_empty() && !a.locants.iter().any(|l| l == &format!("{element}{primes}")) {
            return false;
        }
        return backbone_with_locant(graph, atom, backbone).is_some();
    }
    false
}
fn remove_oxygen_with_locant(
    graph: &Graph,
    atoms: &mut Vec<AtomId>,
    locant: &str,
) -> Result<AtomId> {
    if let Some(index) = atoms
        .iter()
        .position(|&a| atom_has_locant(graph, a, locant))
    {
        return Ok(atoms.remove(index));
    }
    if let Some(index) = atoms
        .iter()
        .position(|&a| backbone_with_locant(graph, a, locant).is_some())
    {
        return Ok(atoms.remove(index));
    }
    Err(error(format!(
        "Failed to find acid group at locant: {locant}"
    )))
}
fn neutralise(graph: &mut Graph, atom: AtomId) {
    graph.atom_mut(atom).charge = 0;
    graph.atom_mut(atom).protons_explicitly_added_or_removed = 0;
}
fn remove_associated_functional_atom(state: &mut BuildState, atom: AtomId) -> Result<()> {
    let graph = &mut state.fragment_manager.graph;
    let frag = graph.atom(atom).fragment;
    let index = graph
        .fragment(frag)
        .functional_atoms
        .iter()
        .rposition(|&a| a == atom)
        .ok_or_else(|| error("OPSIN bug: Unable to find associated functionalAtom"))?;
    graph.fragment_mut(frag).functional_atoms.remove(index);
    Ok(())
}
fn remove_or_move_functional_atoms(
    state: &mut BuildState,
    atom: AtomId,
    replacement: FragmentId,
) -> Result<()> {
    let graph = &mut state.fragment_manager.graph;
    let original = graph.atom(atom).fragment;
    let last = *graph
        .fragment(replacement)
        .atoms
        .last()
        .ok_or_else(|| error("Empty replacement fragment"))?;
    for i in (0..graph.fragment(original).functional_atoms.len()).rev() {
        if graph.fragment(original).functional_atoms[i] == atom {
            graph.fragment_mut(original).functional_atoms.remove(i);
            if (graph.incoming_valency(last) == 1 || graph.fragment(replacement).atoms.len() == 1)
                && graph.atom(last).element.is_chalcogen()
            {
                graph.fragment_mut(replacement).functional_atoms.push(last);
                graph.atom_mut(last).charge = graph.atom(atom).charge;
                graph.atom_mut(last).protons_explicitly_added_or_removed =
                    graph.atom(atom).protons_explicitly_added_or_removed;
            }
            neutralise(graph, atom);
        }
    }
    Ok(())
}
fn move_out_atoms(state: &mut BuildState, atom: AtomId, replacement: FragmentId) -> Result<()> {
    let graph = &mut state.fragment_manager.graph;
    if graph.atom(atom).out_valency > 0 {
        let original = graph.atom(atom).fragment;
        let last = *graph
            .fragment(replacement)
            .atoms
            .last()
            .ok_or_else(|| error("Empty replacement fragment"))?;
        for i in (0..graph.fragment(original).out_atoms.len()).rev() {
            if graph.fragment(original).out_atoms[i].atom == atom {
                let out = graph.remove_out_atom(original, i);
                graph.add_out_atom(replacement, last, out.valency, out.explicitly_set);
            }
        }
    }
    Ok(())
}
fn push_unique(atoms: &mut Vec<AtomId>, atom: AtomId) {
    if !atoms.contains(&atom) {
        atoms.push(atom);
    }
}
fn record_ambiguity(graph: &mut Graph, candidates: &[AtomId], include_existing: bool) {
    let mut atoms = Vec::new();
    for &atom in candidates {
        push_unique(&mut atoms, atom);
        if include_existing {
            for &other in &graph.atom(atom).properties.ambiguous_element_assignment {
                push_unique(&mut atoms, other);
            }
        }
    }
    graph.set_ambiguous_element_assignment(&atoms, atoms.clone());
}

fn has_substitution_hydrogen(
    state: &BuildState,
    arena: &Arena,
    acid: FragmentId,
    needed: i32,
    locant: Option<NodeId>,
) -> Result<bool> {
    let graph = &state.fragment_manager.graph;
    let fallback = graph
        .fragment(acid)
        .default_in_atom
        .or_else(|| graph.fragment(acid).atoms.first().copied())
        .ok_or_else(|| error("Empty acid fragment"))?;
    let mut atoms = Vec::new();
    if let Some(locant) = locant {
        for l in arena.value(locant).split(',') {
            if let Some(atom) = graph.atom_by_locant(acid, l) {
                atoms.push(atom);
            } else {
                atoms.clear();
                atoms.push(fallback);
                break;
            }
        }
    } else {
        atoms.push(fallback);
    }
    Ok(atoms.into_iter().all(|atom| {
        graph.atom(atom).implicit_hydrogen_allowed
            && (graph.determine_valency(atom, true)
                - graph.incoming_valency(atom)
                - graph.atom(atom).out_valency)
                .max(0)
                >= needed
    }))
}

fn chalcogen_replacement(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    locant: Option<NodeId>,
    mut count: usize,
    smiles: &str,
) -> Result<usize> {
    let mut oxygen = replacement_oxygens(state, arena, group)?;
    if let Some(locant) = locant {
        let picked = pick_oxygens(&state.fragment_manager.graph, arena, locant, &oxygen);
        if picked.len() < count {
            count = 1;
        } else {
            arena.detach(locant);
            oxygen = picked;
        }
    }
    let mut replaceable = Vec::new();
    let smiles = if let Some(smiles) = smiles.strip_prefix('=') {
        replaceable.extend(oxygen.iter().copied().filter(|&a| {
            state.fragment_manager.graph.atom(a).bonds.len() == 1
                && state.fragment_manager.graph.incoming_valency(a) == 2
        }));
        smiles
    } else {
        for (bonds, valency) in [(1, 2), (1, 1), (2, 2)] {
            replaceable.extend(oxygen.iter().copied().filter(|&a| {
                state.fragment_manager.graph.atom(a).bonds.len() == bonds
                    && state.fragment_manager.graph.incoming_valency(a) == valency
            }));
        }
        smiles
    };
    if count > 1 && replaceable.len() < count {
        count = 1;
    }
    if replaceable.len() < count {
        return Ok(0);
    }
    for &atom in replaceable.iter().take(count) {
        state
            .fragment_manager
            .replace_atom_with_smiles(atom, smiles)
            .map_err(|e| error(e.to_string()))?;
    }
    if replaceable.len() != count {
        record_ambiguity(&mut state.fragment_manager.graph, &replaceable, false);
    }
    Ok(count)
}

fn peroxy_replacement(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    locant: Option<NodeId>,
    mut count: usize,
) -> Result<usize> {
    let mut oxygen = functional_oxygens_in_suffixes(state, arena, group);
    if oxygen.is_empty() {
        oxygen = etheric_oxygens(state, arena, group)?;
        oxygen.extend(functional_oxygens_in_group(state, arena, group)?);
    }
    if let Some(locant) = locant {
        let picked = pick_oxygens(&state.fragment_manager.graph, arena, locant, &oxygen);
        if picked.len() < count {
            count = 1;
        } else {
            arena.detach(locant);
            oxygen = picked;
        }
    }
    if count > 1 && oxygen.len() < count {
        count = 1;
    }
    if oxygen.len() < count {
        return Ok(0);
    }
    for &atom in oxygen.iter().take(count) {
        if state.fragment_manager.graph.atom(atom).bonds.len() == 2 {
            let new_frag = state
                .fragment_manager
                .build_smiles("O", "suffix", "none")
                .map_err(|e| error(e.to_string()))?;
            let new_atom = first_atom(&state.fragment_manager.graph, new_frag)?;
            let bond = state.fragment_manager.graph.atom(atom).bonds[0];
            let neighbour = state
                .fragment_manager
                .graph
                .bond(bond)
                .other_atom(atom)
                .unwrap();
            state
                .fragment_manager
                .create_bond(neighbour, new_atom, 1)
                .map_err(|e| error(e.to_string()))?;
            state
                .fragment_manager
                .create_bond(new_atom, atom, 1)
                .map_err(|e| error(e.to_string()))?;
            state.fragment_manager.remove_bond(bond);
            state
                .fragment_manager
                .incorporate_fragment(new_frag, fragment(arena, group)?)
                .map_err(|e| error(e.to_string()))?;
        } else {
            let replacement = state
                .fragment_manager
                .build_smiles("OO", "suffix", "none")
                .map_err(|e| error(e.to_string()))?;
            remove_or_move_functional_atoms(state, atom, replacement)?;
            let new_atom = first_atom(&state.fragment_manager.graph, replacement)?;
            state
                .fragment_manager
                .replace_atom_preserving_connectivity(atom, new_atom)
                .map_err(|e| error(e.to_string()))?;
            state
                .fragment_manager
                .incorporate_fragment(replacement, fragment(arena, group)?)
                .map_err(|e| error(e.to_string()))?;
        }
    }
    Ok(count)
}

fn acid_replacement(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    locant: Option<NodeId>,
    mut count: usize,
    smiles: &str,
) -> Result<usize> {
    let valency = match smiles.as_bytes().first() {
        Some(b'-') => 1,
        Some(b'=') => 2,
        Some(b'#') => 3,
        _ => {
            return Err(error(
                "OPSIN bug: Unexpected valency on fragment for prefix functional replacement",
            ));
        }
    };
    let smiles = &smiles[1..];
    let mut oxygen = replacement_oxygens(state, arena, group)?;
    if let Some(locant) = locant {
        let mut picked = pick_oxygens(&state.fragment_manager.graph, arena, locant, &oxygen);
        let (single, double) = terminal_oxygens(&state.fragment_manager.graph, &picked)?;
        if valency == 1 {
            picked.retain(|a| !double.contains(a));
        } else if valency == 2 {
            picked.retain(|a| !single.contains(a));
        }
        if picked.len() < count {
            count = 1;
        } else {
            arena.detach(locant);
            oxygen = picked;
        }
    }
    let (mut single, double) = terminal_oxygens(&state.fragment_manager.graph, &oxygen)?;
    if valency == 1 {
        oxygen.retain(|a| !double.contains(a));
    } else if valency == 2 {
        oxygen.retain(|a| !single.contains(a) && !double.contains(a));
        oxygen.extend(double);
    } else {
        if single.is_empty() || double.is_empty() {
            return Err(error(
                "Both a -OH and =O are required for nitrido prefix functional replacement",
            ));
        }
        oxygen.retain(|a| !single.contains(a));
    }
    if count > 1 && oxygen.len() < count {
        count = 1;
    }
    if oxygen.len() < count {
        return Ok(0);
    }
    for &atom in oxygen.iter().take(count) {
        let original = state.fragment_manager.graph.atom(atom).fragment;
        let token = state.fragment_manager.graph.fragment(original).clone();
        let replacement = state
            .fragment_manager
            .build_smiles(smiles, &token.fragment_type, "none")
            .map_err(|e| error(e.to_string()))?;
        state
            .fragment_manager
            .graph
            .fragment_mut(replacement)
            .sub_type = token.sub_type;
        state
            .fragment_manager
            .graph
            .fragment_mut(replacement)
            .token_attributes = token.token_attributes;
        if valency == 3 {
            let bond = state.fragment_manager.graph.atom(atom).bonds[0];
            state.fragment_manager.graph.bond_mut(bond).order = 3;
            if single.is_empty() {
                return Err(error(
                    "Both a -OH and =O are required for nitrido prefix functional replacement",
                ));
            }
            let hydroxy = single.remove(0);
            state
                .fragment_manager
                .remove_atom_and_associated_bonds(hydroxy);
            remove_associated_functional_atom(state, hydroxy)?;
        }
        let new_atom = first_atom(&state.fragment_manager.graph, replacement)?;
        state
            .fragment_manager
            .replace_atom_preserving_connectivity(atom, new_atom)
            .map_err(|e| error(e.to_string()))?;
        if valency == 1 {
            remove_or_move_functional_atoms(state, atom, replacement)?;
        }
        move_out_atoms(state, atom, replacement)?;
        state
            .fragment_manager
            .incorporate_fragment(replacement, original)
            .map_err(|e| error(e.to_string()))?;
    }
    Ok(count)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrefixType {
    Chalcogen,
    Halide,
    Dedicated,
    Hydrazono,
    Peroxy,
}
fn is_chalcogen_substituent(state: &BuildState, arena: &Arena, group: NodeId) -> bool {
    if arena
        .next_sibling(group)
        .is_some_and(|n| arena[n].name == "hyphen")
        && arena.previous_sibling(group).is_none()
        && let Some(id) = crate::component_processor::previous_group(arena, group)
        && arena
            .next_sibling_named(id, "suffix")
            .is_none_or(|s| arena[s].fragment.is_none())
    {
        return arena[id].fragment.is_some_and(|f| {
            state
                .fragment_manager
                .graph
                .fragment(f)
                .atoms
                .iter()
                .any(|&a| state.fragment_manager.graph.atom(a).element == Element::C)
        });
    }
    false
}
fn prefix_is_blocked(arena: &Arena, group: NodeId) -> bool {
    let mut previous = arena.previous_sibling(group);
    while let Some(id) = previous {
        if arena[id].name == "subtractivePrefix"
            || (arena[id].name == "stereochemistry"
                && arena[id].attribute("type") == Some("carbohydrateConfigurationalPrefix"))
        {
            previous = arena.previous_sibling(id);
        } else {
            break;
        }
    }
    previous.is_some()
}

/// Full upstream prefix replacement dispatch; mutates the supplied inventories.
pub fn process_prefix_functional_replacement_nomenclature(
    state: &mut BuildState,
    arena: &mut Arena,
    groups: &mut Vec<NodeId>,
    substituents: &mut Vec<NodeId>,
) -> Result<bool> {
    let original_count = groups.len();
    for i in (0..original_count).rev() {
        let group = groups[i];
        let value = arena.value(group);
        let kind = if (matches!(value.as_str(), "thio" | "seleno" | "telluro")
            && !is_chalcogen_substituent(state, arena, group))
            || value == "thiono"
        {
            Some(PrefixType::Chalcogen)
        } else if arena[group].attribute("subType") == Some("halideOrPseudoHalide") {
            Some(PrefixType::Halide)
        } else if arena[group].attribute("subType") == Some("dedicatedFunctionalReplacementPrefix")
        {
            Some(PrefixType::Dedicated)
        } else if value == "hydrazono" {
            Some(PrefixType::Hydrazono)
        } else if value == "peroxy" {
            Some(PrefixType::Peroxy)
        } else {
            None
        };
        let Some(kind) = kind else {
            continue;
        };
        let inappropriate = || {
            error(format!(
                "dedicated Functional Replacement Prefix used in an inappropriate position :{value}"
            ))
        };
        let sub = arena[group]
            .parent
            .ok_or_else(|| error("Detached prefix group"))?;
        let Some(next) = arena
            .next_sibling(sub)
            .filter(|&n| matches!(arena[n].name.as_str(), "root" | "substituent"))
        else {
            if kind == PrefixType::Dedicated {
                return Err(inappropriate());
            } else {
                continue;
            }
        };
        let target = arena
            .first_child_named(next, "group")
            .ok_or_else(|| error("Prefix replacement target has no group"))?;
        if prefix_is_blocked(arena, target) {
            if kind == PrefixType::Dedicated {
                return Err(inappropriate());
            } else {
                continue;
            }
        }
        let (mut locant, mut multiplier, mut count) = (None, None, 1);
        if let Some(possible) = arena.previous_sibling(group) {
            let possible_locant = if arena[possible].name == "multiplier" {
                count = value_number(arena, possible)?;
                multiplier = Some(possible);
                arena.previous_sibling(possible)
            } else {
                Some(possible)
            };
            if let Some(id) = possible_locant
                .filter(|&n| arena[n].name == "locant" && arena[n].attribute("type").is_none())
            {
                if arena.value(id).split(',').count() == count {
                    locant = Some(id);
                } else if count > 1 {
                    if kind == PrefixType::Dedicated {
                        return Err(inappropriate());
                    } else {
                        continue;
                    }
                }
            }
        }
        let smiles = arena[group].attribute("value").unwrap_or("").to_owned();
        let replaced = match kind {
            PrefixType::Chalcogen => {
                chalcogen_replacement(state, arena, target, locant, count, &smiles)?
            }
            PrefixType::Peroxy => {
                if arena[next].name == "substituent" {
                    continue;
                }
                peroxy_replacement(state, arena, target, locant, count)?
            }
            PrefixType::Dedicated => {
                if arena[target].attribute("type") != Some("nonCarboxylicAcid")
                    && !(arena.value(target) == "form" && value == "imido")
                {
                    return Err(inappropriate());
                }
                let n = acid_replacement(state, arena, target, locant, count, &smiles)?;
                if n == 0 {
                    return Err(inappropriate());
                }
                n
            }
            PrefixType::Hydrazono | PrefixType::Halide => {
                let acid = fragment(arena, target)?;
                let prefix = fragment(arena, group)?;
                let needed = state
                    .fragment_manager
                    .graph
                    .fragment(prefix)
                    .out_atoms
                    .first()
                    .ok_or_else(|| error("Replacement prefix has no outAtom"))?
                    .valency;
                if arena[target].attribute("type") != Some("nonCarboxylicAcid")
                    || has_substitution_hydrogen(state, arena, acid, needed, locant)?
                {
                    continue;
                }
                acid_replacement(state, arena, target, locant, count, &smiles)?
            }
        };
        if replaced > 0 {
            state
                .fragment_manager
                .remove_fragment(fragment(arena, group)?)
                .map_err(|e| error(e.to_string()))?;
            arena.detach(group);
            groups.retain(|&id| id != group);
            for child in arena[sub].children.clone().into_iter().rev() {
                arena.detach(child);
                arena.insert_child(next, child, 0);
            }
            substituents.retain(|&id| id != sub);
            arena.detach(sub);
            if replaced > 1
                && let Some(multiplier) = multiplier
            {
                arena.detach(multiplier);
            }
        }
    }
    Ok(groups.len() != original_count)
}

fn disambiguate_multiplied_infix(
    state: &mut BuildState,
    arena: &mut Arena,
    suffixes: &mut Vec<NodeId>,
    suffix_fragments: &mut Vec<FragmentId>,
    suffix: NodeId,
    transformations: &mut Vec<String>,
    oxygen_available: usize,
) -> Result<()> {
    let Some(infix) = arena
        .previous_sibling(suffix)
        .filter(|&n| arena[n].name == "infix")
    else {
        return Ok(());
    };
    let multiplier = arena
        .previous_sibling(infix)
        .filter(|&n| arena[n].name == "multiplier")
        .ok_or_else(|| error("Multiplier expected in front of ambiguous infix"))?;
    let count = value_number(arena, multiplier)?;
    if transformations.len() + count.saturating_sub(1) <= oxygen_available {
        for _ in 1..count {
            transformations.insert(0, transformations[0].clone());
        }
    } else {
        let locant = arena
            .previous_sibling(multiplier)
            .filter(|&n| arena[n].name == "locant");
        let locants = locant.map(|l| {
            arena
                .value(l)
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
        if let Some(locants) = &locants {
            if locants.len() != count {
                return Err(error(
                    "Multiplier/locant disagreement when multiplying infixed suffix",
                ));
            }
            arena[suffix].set_attribute("locant", &locants[0]);
        }
        arena[suffix].set_attribute("multiplied", "multiplied");
        for j in 1..count {
            let copy = arena.copy(suffix);
            let frag_copy = state
                .fragment_manager
                .copy_fragment(fragment(arena, suffix)?)
                .map_err(|e| error(e.to_string()))?;
            arena[copy].fragment = Some(frag_copy);
            suffix_fragments.push(frag_copy);
            arena.insert_after(suffix, copy);
            suffixes.push(copy);
            if let Some(locants) = &locants {
                arena[copy].set_attribute("locant", &locants[j]);
            }
        }
        if let Some(locant) = locant {
            arena.detach(locant);
        }
    }
    arena.detach(multiplier);
    arena.detach(infix);
    Ok(())
}

/// Infix transformations, including nitrido and suffix/infix multiplier ambiguity.
pub fn process_infix_functional_replacement_nomenclature(
    state: &mut BuildState,
    arena: &mut Arena,
    suffixes: &mut Vec<NodeId>,
    suffix_fragments: &mut Vec<FragmentId>,
) -> Result<()> {
    let mut i = 0;
    while i < suffixes.len() {
        let suffix = suffixes[i];
        i += 1;
        let Some(infix_value) = arena[suffix].attribute("infix").map(str::to_owned) else {
            continue;
        };
        let mut frag = arena[suffix].fragment;
        if let Some(group) = arena
            .previous_sibling_ignoring(suffix, &["multiplier", "infix", "suffix"])
            .filter(|&n| {
                arena[n].name == "group"
                    && matches!(
                        arena[n].attribute("type"),
                        Some("nonCarboxylicAcid" | "chalcogenAcidStem")
                    )
            })
        {
            frag = arena[group].fragment;
        }
        let frag = frag.ok_or_else(|| error(format!("infix has erroneously been assigned to a suffix which does not correspond to a suffix fragment. suffix: {}", arena.value(suffix))))?;
        let mut transformations: Vec<_> = infix_value.split(';').map(str::to_owned).collect();
        let (mut single, mut double) = terminal_oxygens(
            &state.fragment_manager.graph,
            &state.fragment_manager.graph.fragment(frag).atoms,
        )?;
        let oxygen_available = single.len() + double.len();
        disambiguate_multiplied_infix(
            state,
            arena,
            suffixes,
            suffix_fragments,
            suffix,
            &mut transformations,
            oxygen_available,
        )?;
        transformations.sort_by_key(|s| s.split(',').count());
        for transformation in transformations {
            let parts: Vec<_> = transformation.split(':').collect();
            if parts.len() != 2 {
                return Err(error(format!(
                    "Atom to be replaced and replacement not specified correctly in infix: {transformation}"
                )));
            }
            let smiles = parts[1];
            let (mut accept_single, mut accept_double, mut nitrido) = (false, false, false);
            for input in parts[0].split(',') {
                match input.as_bytes().first() {
                    Some(b'=') => accept_double = true,
                    Some(b'-') => accept_single = true,
                    Some(b'#') => nitrido = true,
                    _ => {
                        return Err(error(format!(
                            "Malformed infix transformation. Expected to start with either - or =. Transformation was: {input}"
                        )));
                    }
                }
                if input.as_bytes().get(1) != Some(&b'O') {
                    return Err(error(
                        "Only replacement by oxygen is supported. Check infix defintions",
                    ));
                }
            }
            let mut ambiguous = false;
            if (accept_single || nitrido) && !accept_double {
                if single.is_empty() {
                    return Err(error(format!(
                        "Cannot find single bonded oxygen for infix with SMILES: {smiles} to modify!"
                    )));
                }
                ambiguous |= single.len() != 1;
            }
            if !accept_single && (accept_double || nitrido) {
                if double.is_empty() {
                    return Err(error(format!(
                        "Cannot find double bonded oxygen for infix with SMILES: {smiles} to modify!"
                    )));
                }
                ambiguous |= double.len() != 1;
            }
            if accept_single && accept_double {
                if oxygen_available == 0 {
                    return Err(error(format!(
                        "Cannot find oxygen for infix with SMILES: {smiles} to modify!"
                    )));
                }
                ambiguous |= oxygen_available != 1;
            }
            let atom = if (accept_double || nitrido) && !double.is_empty() {
                double.remove(0)
            } else if accept_single && !single.is_empty() {
                single.remove(0)
            } else {
                return Err(error(format!(
                    "Cannot find oxygen for infix with SMILES: {smiles} to modify!"
                )));
            };
            let replacement = state
                .fragment_manager
                .build_smiles(smiles, "suffix", "none")
                .map_err(|e| error(e.to_string()))?;
            if !state
                .fragment_manager
                .graph
                .fragment(replacement)
                .out_atoms
                .is_empty()
            {
                state.fragment_manager.graph.remove_out_atom(replacement, 0);
            }
            let new_atom = first_atom(&state.fragment_manager.graph, replacement)?;
            if state
                .fragment_manager
                .graph
                .fragment(replacement)
                .atoms
                .len()
                == 1
                && state
                    .fragment_manager
                    .graph
                    .atom(new_atom)
                    .element
                    .is_chalcogen()
            {
                let charge = state.fragment_manager.graph.atom(atom).charge;
                let protons = state
                    .fragment_manager
                    .graph
                    .atom(atom)
                    .protons_explicitly_added_or_removed;
                state.fragment_manager.graph.atom_mut(new_atom).charge = charge;
                state
                    .fragment_manager
                    .graph
                    .atom_mut(new_atom)
                    .protons_explicitly_added_or_removed = protons;
            }
            remove_or_move_functional_atoms(state, atom, replacement)?;
            move_out_atoms(state, atom, replacement)?;
            if nitrido {
                let bond = state.fragment_manager.graph.atom(atom).bonds[0];
                state.fragment_manager.graph.bond_mut(bond).order = 3;
                let hydroxy = single.remove(0);
                state
                    .fragment_manager
                    .remove_atom_and_associated_bonds(hydroxy);
                remove_associated_functional_atom(state, hydroxy)?;
            }
            let original = state.fragment_manager.graph.atom(atom).fragment;
            state
                .fragment_manager
                .incorporate_fragment(replacement, original)
                .map_err(|e| error(e.to_string()))?;
            state
                .fragment_manager
                .replace_atom_preserving_connectivity(atom, new_atom)
                .map_err(|e| error(e.to_string()))?;
            if ambiguous {
                let mut candidates = vec![new_atom];
                candidates.extend(&double);
                candidates.extend(&single);
                record_ambiguity(&mut state.fragment_manager.graph, &candidates, true);
            }
        }
    }
    Ok(())
}

fn rightmost_group(arena: &Arena, root: NodeId) -> Option<NodeId> {
    arena.descendants_named(root, "group").last().copied()
}
fn acid_replacing_full_word(
    state: &mut BuildState,
    arena: &mut Arena,
    acid_root: NodeId,
    word: NodeId,
) -> Result<()> {
    let locant = arena[word].attribute("locant").map(str::to_owned);
    let replacing_group = rightmost_group(arena, word).ok_or_else(|| error("OPSIN bug: acid replacing group not found where one was expected for acidReplacingFunctionalGroup wordRule"))?;
    let name = arena.value(replacing_group);
    let replacement = fragment(arena, replacing_group)?;
    if arena[arena[replacing_group].parent.unwrap()].children.len() != 1 {
        return Err(error(format!("Unexpected qualifier to: {name}")));
    }
    let target = arena
        .first_child_named(acid_root, "group")
        .ok_or_else(|| error("Acid root has no group"))?;
    let mut oxygen = acidic_oxygens(state, arena, target)?;
    if oxygen.is_empty() {
        return Err(error(format!(
            "Insufficient oxygen to replace with {name}s in {}",
            arena.value(target)
        )));
    }
    if matches!(name.as_str(), "amide" | "amid") {
        if state
            .fragment_manager
            .graph
            .fragment(replacement)
            .atoms
            .len()
            != 1
        {
            return Err(error(format!("OPSIN bug: {name} not found where expected")));
        }
        let nitrogen = first_atom(&state.fragment_manager.graph, replacement)?;
        neutralise(&mut state.fragment_manager.graph, nitrogen);
        state.fragment_manager.graph.clear_locants(nitrogen);
        // Upstream adds only the mapping, not an atom locant.
        state
            .fragment_manager
            .graph
            .fragment_mut(replacement)
            .locants
            .insert("N".into(), nitrogen);
    }
    let chosen = if let Some(locant) = locant {
        remove_oxygen_with_locant(&state.fragment_manager.graph, &mut oxygen, &locant)?
    } else {
        oxygen[0]
    };
    let new_atom = first_atom(&state.fragment_manager.graph, replacement)?;
    state
        .fragment_manager
        .replace_atom_preserving_connectivity(chosen, new_atom)
        .map_err(|e| error(e.to_string()))?;
    remove_associated_functional_atom(state, chosen)
}
fn acid_replacing_functional_word(
    state: &mut BuildState,
    arena: &mut Arena,
    acid_root: NodeId,
    word: NodeId,
) -> Result<()> {
    if arena[word].attribute("type") != Some("functionalTerm") {
        return Err(error("amide word not found where expected, bug?"));
    }
    let term = arena.first_child_named(word, "functionalTerm").ok_or_else(|| error("OPSIN bug: functionalTerm word not found where one was expected for acidReplacingFunctionalGroup wordRule"))?;
    let replacing = arena
        .first_child_named(term, "functionalGroup")
        .ok_or_else(|| error("Functional term has no functionalGroup"))?;
    let name = arena.value(replacing);
    let (mut count, mut locants) = (1, None);
    let mut previous = arena.previous_sibling(replacing);
    if let Some(multiplier) = previous.filter(|&n| arena[n].name == "multiplier") {
        count = value_number(arena, multiplier)?;
        arena.detach(multiplier);
        previous = arena.previous_sibling(replacing);
    }
    if let Some(id) = previous {
        if arena[id].name != "locant" {
            return Err(error(
                "Unexpected qualifier to acidReplacingFunctionalGroup functionalTerm",
            ));
        }
        locants = Some(
            arena
                .value(id)
                .trim_end_matches('-')
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        );
        arena.detach(id);
    }
    if arena[term].children.len() != 1 {
        return Err(error(
            "Unexpected qualifier to acidReplacingFunctionalGroup functionalTerm",
        ));
    }
    if locants.as_ref().is_some_and(|l| l.len() < count) {
        return Err(error(
            "Multiplier/locant disagreement in acid replacing functionalTerm",
        ));
    }
    let target = arena
        .first_child_named(acid_root, "group")
        .ok_or_else(|| error("Acid root has no group"))?;
    let mut oxygen = acidic_oxygens(state, arena, target)?;
    if count > oxygen.len() {
        return Err(error(format!(
            "Insufficient oxygen to replace with nitrogen in {}",
            arena.value(target)
        )));
    }
    if matches!(name.as_str(), "amide" | "amid") {
        for i in 0..count {
            let atom = if let Some(locants) = &locants {
                remove_oxygen_with_locant(&state.fragment_manager.graph, &mut oxygen, &locants[i])?
            } else {
                oxygen[i]
            };
            remove_associated_functional_atom(state, atom)?;
            state.fragment_manager.graph.atom_mut(atom).element = Element::N;
        }
    } else {
        let smiles = arena[replacing].attribute("value").unwrap_or("");
        let labels = arena[replacing].attribute("labels").unwrap_or("none");
        let replacement = state
            .fragment_manager
            .build_smiles(smiles, "suffix", labels)
            .map_err(|e| error(e.to_string()))?;
        let acid = fragment(arena, target)?;
        if state
            .fragment_manager
            .graph
            .atom_by_locant(acid, "2")
            .is_some()
        {
            for atom in state
                .fragment_manager
                .graph
                .fragment(replacement)
                .atoms
                .clone()
            {
                state.fragment_manager.graph.clear_locants(atom);
            }
        }
        let first = if let Some(locants) = &locants {
            remove_oxygen_with_locant(&state.fragment_manager.graph, &mut oxygen, &locants[0])?
        } else {
            oxygen[0]
        };
        let original = state.fragment_manager.graph.atom(first).fragment;
        let new_atom = first_atom(&state.fragment_manager.graph, replacement)?;
        state
            .fragment_manager
            .replace_atom_preserving_connectivity(first, new_atom)
            .map_err(|e| error(e.to_string()))?;
        remove_associated_functional_atom(state, first)?;
        for i in 1..count {
            let copy = state
                .fragment_manager
                .copy_and_relabel_fragment(replacement, i)
                .map_err(|e| error(e.to_string()))?;
            let atom = if let Some(locants) = &locants {
                remove_oxygen_with_locant(&state.fragment_manager.graph, &mut oxygen, &locants[i])?
            } else {
                oxygen[i]
            };
            let destination = state.fragment_manager.graph.atom(atom).fragment;
            let copy_atom = first_atom(&state.fragment_manager.graph, copy)?;
            state
                .fragment_manager
                .replace_atom_preserving_connectivity(atom, copy_atom)
                .map_err(|e| error(e.to_string()))?;
            state
                .fragment_manager
                .incorporate_fragment(copy, destination)
                .map_err(|e| error(e.to_string()))?;
            remove_associated_functional_atom(state, atom)?;
        }
        state
            .fragment_manager
            .incorporate_fragment(replacement, original)
            .map_err(|e| error(e.to_string()))?;
    }
    Ok(())
}

/// Acid replacing functional class nomenclature, applied before prefix/infix replacement.
pub fn process_acid_replacing_functional_class_nomenclature(
    state: &mut BuildState,
    arena: &mut Arena,
    acid_root: NodeId,
    word: NodeId,
) -> Result<()> {
    let rule = arena
        .parent_word_rule(word)
        .ok_or_else(|| error("Word has no wordRule"))?;
    if arena[rule].attribute("wordRule") != Some("acidReplacingFunctionalGroup") {
        return Ok(());
    }
    let parent = arena[word]
        .parent
        .ok_or_else(|| error("Word has no parent"))?;
    if arena.index_of(parent, word) != Some(0) {
        return Ok(());
    }
    for replacing_word in arena[parent].children.clone().into_iter().skip(1) {
        if arena[replacing_word].name != "word" {
            return Err(error(
                "OPSIN bug: problem with acidReplacingFunctionalGroup word rule",
            ));
        }
        match arena[replacing_word].attribute("type") {
            Some("full") => acid_replacing_full_word(state, arena, acid_root, replacing_word)?,
            Some("functionalTerm") => {
                acid_replacing_functional_word(state, arena, acid_root, replacing_word)?
            }
            _ => {
                return Err(error(
                    "OPSIN bug: problem with acidReplacingFunctionalGroup word rule",
                ));
            }
        }
    }
    Ok(())
}

/// All 25 methods in the pinned FunctionalReplacement source, including
/// its comparator and convenience routines. Folded bodies are identified.
pub const SOURCE_METHOD_COVERAGE: &[(&str, &str)] = &[
    (
        "compare",
        "process_infix_functional_replacement_nomenclature: stable sort_by_key",
    ),
    (
        "processAcidReplacingFunctionalClassNomenclature",
        "process_acid_replacing_functional_class_nomenclature",
    ),
    (
        "processPrefixFunctionalReplacementNomenclature",
        "process_prefix_functional_replacement_nomenclature",
    ),
    ("isChalcogenSubstituent", "is_chalcogen_substituent"),
    (
        "groupPrecededByElementThatBlocksPrefixReplacementInterpetation",
        "prefix_is_blocked",
    ),
    (
        "processInfixFunctionalReplacementNomenclature",
        "process_infix_functional_replacement_nomenclature",
    ),
    (
        "processAcidReplacingFunctionalClassNomenclatureFullWord",
        "acid_replacing_full_word",
    ),
    (
        "processAcidReplacingFunctionalClassNomenclatureFunctionalWord",
        "acid_replacing_functional_word",
    ),
    (
        "removeOxygenWithAppropriateLocant",
        "remove_oxygen_with_locant",
    ),
    (
        "acidHasSufficientHydrogenForSubstitutionInterpretation",
        "has_substitution_hydrogen",
    ),
    (
        "performChalcogenFunctionalReplacement",
        "chalcogen_replacement",
    ),
    ("performPeroxyFunctionalReplacement", "peroxy_replacement"),
    ("performFunctionalReplacementOnAcid", "acid_replacement"),
    (
        "disambiguateMultipliedInfixMeaning",
        "disambiguate_multiplied_infix",
    ),
    (
        "removeOrMoveObsoleteFunctionalAtoms",
        "remove_or_move_functional_atoms",
    ),
    ("moveObsoleteOutAtoms", "move_out_atoms"),
    (
        "removeAssociatedFunctionalAtom",
        "remove_associated_functional_atom",
    ),
    ("pickOxygensWithAppropriateLocants", "pick_oxygens"),
    ("allLocantsP", "pick_oxygens: all_p"),
    (
        "findFunctionalOxygenAtomsInApplicableSuffixes",
        "functional_oxygens_in_suffixes",
    ),
    (
        "findFunctionalOxygenAtomsInGroup",
        "functional_oxygens_in_group",
    ),
    ("findEthericOxygenAtomsInGroup", "etheric_oxygens"),
    ("findOxygenAtomsInApplicableSuffixes", "oxygens_in_suffixes"),
    ("findOxygenAtomsInGroup", "oxygens_in_group"),
    (
        "populateTerminalSingleAndDoubleBondedOxygen",
        "terminal_oxygens",
    ),
];
