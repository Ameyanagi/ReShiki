//! FragmentTools from OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! MIT, Daniel Lowe and contributors. Insertion order, progressive substitution
//! heuristics and indicated-hydrogen tautomer rules are intentionally retained.
use crate::graph::{AtomId, BondId, Element, FragmentId, Graph, GraphError, StereoReference};
use crate::valence;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

pub fn calculate_substitutable_hydrogen_atoms(graph: &Graph, atom: AtomId) -> i32 {
    if !graph.atom(atom).implicit_hydrogen_allowed {
        return 0;
    }
    (graph.determine_valency(atom, true)
        - graph.incoming_valency(atom)
        - graph.atom(atom).out_valency)
        .max(0)
}
pub fn intra_fragment_neighbours(graph: &Graph, atom: AtomId) -> Vec<AtomId> {
    let fragment = graph.atom(atom).fragment;
    graph
        .neighbours(atom)
        .into_iter()
        .filter(|neighbour| graph.atom(*neighbour).fragment == fragment)
        .collect()
}
pub fn intra_fragment_incoming_valency(graph: &Graph, atom: AtomId) -> i32 {
    graph
        .atom(atom)
        .bonds
        .iter()
        .filter(|bond| {
            graph
                .atom(graph.bond(**bond).other_atom(atom).unwrap())
                .fragment
                == graph.atom(atom).fragment
        })
        .map(|bond| i32::from(graph.bond(*bond).order))
        .sum()
}
pub fn is_element_symbol_locant(locant: &str) -> bool {
    let stem = locant.trim_end_matches('\'');
    let bytes = stem.as_bytes();
    matches!(bytes, [b'A'..=b'Z'] | [b'A'..=b'Z', b'a'..=b'z'])
}
pub fn is_numeric_locant(locant: &str) -> bool {
    let stem = locant.trim_end_matches('\'');
    let stem = stem
        .strip_suffix(|ch: char| ch.is_ascii_lowercase())
        .unwrap_or(stem);
    !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_digit())
}
pub fn parse_amino_acid_style_locant(locant: &str) -> Option<(&str, &str, &str)> {
    let bytes = locant.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_uppercase) {
        return None;
    }
    let end = if bytes.get(1).is_some_and(u8::is_ascii_lowercase) {
        2
    } else {
        1
    };
    let prime_end = end
        + bytes[end..]
            .iter()
            .take_while(|byte| **byte == b'\'')
            .count();
    let backbone = &locant[prime_end..];
    if !(is_numeric_locant(backbone)
        || matches!(
            backbone.trim_end_matches('\''),
            "alpha" | "beta" | "gamma" | "delta" | "epsilon" | "zeta" | "eta" | "omega"
        ))
    {
        return None;
    }
    Some((&locant[..end], &locant[end..prime_end], backbone))
}
pub fn compare_atoms_by_locants(graph: &Graph, a: AtomId, b: AtomId) -> Ordering {
    let a = graph.atom(a);
    let b = graph.atom(b);
    match (a.atom_type == "suffix", b.atom_type == "suffix") {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    fn split(locant: &str) -> Option<(usize, u64, &str)> {
        if !is_numeric_locant(locant) {
            return None;
        }
        let stem = locant.trim_end_matches('\'');
        let digits = stem.bytes().take_while(u8::is_ascii_digit).count();
        Some((
            locant.len() - stem.len(),
            stem[..digits].parse().ok()?,
            &stem[digits..],
        ))
    }
    let (Some(a), Some(b)) = (
        a.locants.first().and_then(|s| split(s)),
        b.locants.first().and_then(|s| split(s)),
    ) else {
        return Ordering::Equal;
    };
    a.cmp(&b)
}
pub fn relabel_locants_as_fused_ring_system(graph: &mut Graph, atoms: &[AtomId]) {
    for &atom in atoms {
        graph.clear_locants(atom);
    }
    let (mut number, mut letter) = (0, b'a');
    for &atom in atoms {
        if graph.atom(atom).element != Element::C || graph.atom(atom).bonds.len() < 3 {
            number += 1;
            letter = b'a';
            graph.add_locant(atom, number.to_string());
        } else {
            graph.add_locant(atom, format!("{number}{}", letter as char));
            letter += 1;
        }
    }
}
pub fn relabel_locants(graph: &mut Graph, atoms: &[AtomId], suffix: &str) {
    for &atom in atoms {
        let locants = graph.atom(atom).locants.clone();
        graph.clear_locants(atom);
        for locant in locants {
            graph.add_locant(atom, format!("{locant}{suffix}"));
        }
    }
}
pub fn relabel_numeric_locants(graph: &mut Graph, atoms: &[AtomId], suffix: &str) {
    for &atom in atoms {
        for locant in graph.atom(atom).locants.clone() {
            if is_numeric_locant(&locant) {
                graph.remove_locant(atom, &locant);
                graph.add_locant(atom, format!("{locant}{suffix}"));
            }
        }
    }
}
pub fn unsaturate(
    graph: &mut Graph,
    from: AtomId,
    order: u8,
    fragment: FragmentId,
) -> Result<BondId, GraphError> {
    let numeric = graph.atom(from).locants.first().and_then(|locant| {
        let stem = locant.trim_end_matches('\'');
        stem.parse::<u32>()
            .ok()
            .map(|number| (number, &locant[stem.len()..]))
    });
    let to = if let Some((number, primes)) = numeric {
        let next = graph.atom_by_locant(fragment, &format!("{}{primes}", number + 1));
        if let Some(next) = next.filter(|next| graph.bond_between(from, *next).is_some()) {
            Some(next)
        } else if next.is_none() && graph.atom(from).in_cycle {
            graph
                .atom_by_locant(fragment, &format!("1{primes}"))
                .filter(|next| graph.bond_between(from, *next).is_some())
        } else {
            None
        }
    } else {
        graph
            .fragment(fragment)
            .atoms
            .iter()
            .position(|atom| *atom == from)
            .and_then(|index| graph.fragment(fragment).atoms.get(index + 1).copied())
            .filter(|next| graph.bond_between(from, *next).is_some())
    }
    .ok_or_else(|| {
        GraphError(if let Some((number, _)) = numeric {
            format!(
                "Could not find bond to unsaturate starting from the atom with locant: {number}"
            )
        } else {
            "Could not find bond to unsaturate".into()
        })
    })?;
    unsaturate_between(graph, from, to, order)
}
pub fn unsaturate_to_locant(
    graph: &mut Graph,
    from: AtomId,
    to: &str,
    order: u8,
    fragment: FragmentId,
) -> Result<BondId, GraphError> {
    let to = graph
        .atom_by_locant(fragment, to)
        .ok_or_else(|| GraphError(format!("Could not find the atom with locant {to}.")))?;
    unsaturate_between(graph, from, to, order)
}
fn unsaturate_between(
    graph: &mut Graph,
    from: AtomId,
    to: AtomId,
    order: u8,
) -> Result<BondId, GraphError> {
    let bond = graph
        .bond_between(from, to)
        .ok_or_else(|| GraphError("Could not find bond to unsaturate".into()))?;
    if graph.bond(bond).order != 1 {
        return Err(GraphError(
            "Bond indicated to be unsaturated was already unsaturated".into(),
        ));
    }
    if !(1..=3).contains(&order) {
        return Err(GraphError("Invalid unsaturation bond order".into()));
    }
    graph.bond_mut(bond).order = order;
    Ok(bond)
}
pub fn split_out_atom_into_valency_one_out_atoms(
    graph: &mut Graph,
    fragment: FragmentId,
    index: usize,
) {
    let out = graph.fragment(fragment).out_atoms[index].clone();
    for _ in 1..out.valency {
        graph.add_out_atom(
            graph.atom(out.atom).fragment,
            out.atom,
            1,
            out.explicitly_set,
        );
    }
    graph.set_out_atom_valency(fragment, index, 1);
}
pub fn detect_simple_nitrogen_tautomer(graph: &Graph, nitrogen: AtomId) -> Option<AtomId> {
    let n = graph.atom(nitrogen);
    if n.element != Element::N || !n.in_cycle {
        return None;
    }
    for neighbour in graph.neighbours(nitrogen) {
        let a = graph.atom(neighbour);
        if a.spare_valency && a.element == Element::C && a.in_cycle {
            for second in graph
                .neighbours(neighbour)
                .into_iter()
                .filter(|atom| *atom != nitrogen)
            {
                let a = graph.atom(second);
                if a.spare_valency && a.element == Element::N && a.in_cycle && a.charge == 0 {
                    return Some(second);
                }
            }
        }
    }
    None
}
pub fn ensure_spare_valency_consistent_with_valency(
    graph: &mut Graph,
    atom: AtomId,
    take_external: bool,
) -> Result<(), GraphError> {
    let a = graph.atom(atom);
    if !a.spare_valency {
        return Ok(());
    }
    let maximum = a
        .lambda_convention_valency
        .or_else(|| valence::hw_valency(a.element))
        .ok_or_else(|| GraphError(format!("{} is not expected to be aromatic!", a.element)))?
        + a.protons_explicitly_added_or_removed;
    let used = if take_external {
        graph.incoming_valency(atom) + a.out_valency
    } else {
        intra_fragment_incoming_valency(graph, atom)
    };
    if maximum - used < 1 {
        graph.atom_mut(atom).spare_valency = false;
    }
    Ok(())
}
pub fn convert_spare_valencies_to_double_bonds(
    graph: &mut Graph,
    fragment: FragmentId,
) -> Result<(), GraphError> {
    let backup = graph.clone();
    let result = convert_spare_inner(graph, fragment);
    if result.is_err() {
        *graph = backup;
    }
    result
}
fn convert_spare_inner(graph: &mut Graph, fragment: FragmentId) -> Result<(), GraphError> {
    let atoms = graph.fragment(fragment).atoms.clone();
    for &atom in &atoms {
        ensure_spare_valency_consistent_with_valency(graph, atom, true)?;
    }
    for &atom in &atoms {
        if graph.atom(atom).spare_valency
            && !intra_fragment_neighbours(graph, atom)
                .iter()
                .any(|atom| graph.atom(*atom).spare_valency)
        {
            graph.atom_mut(atom).spare_valency = false;
        }
    }
    let original = graph.fragment(fragment).indicated_hydrogens.clone();
    let indicated: Vec<_> = original
        .iter()
        .copied()
        .filter(|atom| graph.atom(*atom).spare_valency && graph.atom(*atom).charge == 0)
        .collect();
    let mut reduce = None;
    if indicated.len() > 1 {
        for indicated in indicated {
            let a = graph.atom(indicated);
            let could_tautomerize = a.element == Element::N
                && a.in_cycle
                && graph.neighbours(indicated).into_iter().any(|neighbour| {
                    let n = graph.atom(neighbour);
                    n.element == Element::C
                        && n.in_cycle
                        && graph.neighbours(neighbour).into_iter().any(|second| {
                            second != indicated
                                && graph.atom(second).element == Element::N
                                && graph.atom(second).in_cycle
                                && !original.contains(&second)
                        })
                });
            if !could_tautomerize || detect_simple_nitrogen_tautomer(graph, indicated).is_some() {
                graph.atom_mut(indicated).spare_valency = false;
            }
        }
    } else if indicated.len() == 1 {
        reduce = Some(indicated[0]);
    }
    let mut count = atoms
        .iter()
        .filter(|atom| graph.atom(**atom).spare_valency)
        .count();
    if count % 2 == 1 {
        let atom = reduce
            .or_else(|| find_best_atom_to_remove_spare_valency_from(graph, &atoms))
            .ok_or_else(|| GraphError("OPSIN Bug: No atom had spare valency!".into()))?;
        graph.atom_mut(atom).spare_valency = false;
        count -= 1;
    }
    while count > 0 {
        let mut terminal = false;
        for &atom in &atoms {
            if graph.atom(atom).spare_valency {
                let possible: Vec<_> = intra_fragment_neighbours(graph, atom)
                    .into_iter()
                    .filter(|atom| graph.atom(*atom).spare_valency)
                    .collect();
                if possible.len() == 1 {
                    pair_spare(graph, atom, possible[0])?;
                    count -= 2;
                    terminal = true;
                }
            }
        }
        if terminal {
            continue;
        }
        let mut selected = None;
        for &atom in &atoms {
            let neighbours = intra_fragment_neighbours(graph, atom);
            if graph.atom(atom).spare_valency
                && neighbours.len() < 3
                && let Some(other) = neighbours
                    .into_iter()
                    .find(|atom| graph.atom(*atom).spare_valency)
            {
                selected = Some((atom, other));
                break;
            }
        }
        if selected.is_none() {
            for &atom in &atoms {
                if graph.atom(atom).spare_valency
                    && let Some(other) = intra_fragment_neighbours(graph, atom)
                        .into_iter()
                        .find(|atom| graph.atom(*atom).spare_valency)
                {
                    selected = Some((atom, other));
                    break;
                }
            }
        }
        let (a,b) = selected.ok_or_else(||GraphError("Failed to assign all double bonds! (Check that indicated hydrogens have been appropriately specified)".into()))?;
        pair_spare(graph, a, b)?;
        count -= 2;
    }
    Ok(())
}
fn pair_spare(graph: &mut Graph, a: AtomId, b: AtomId) -> Result<(), GraphError> {
    let bond = graph
        .bond_between(a, b)
        .ok_or_else(|| GraphError("Spare-valency pair has no bond".into()))?;
    graph.atom_mut(a).spare_valency = false;
    graph.atom_mut(b).spare_valency = false;
    graph.bond_mut(bond).order += 1;
    Ok(())
}
fn find_best_atom_to_remove_spare_valency_from(graph: &Graph, atoms: &[AtomId]) -> Option<AtomId> {
    for &atom in atoms {
        if graph.atom(atom).spare_valency
            && intra_fragment_neighbours(graph, atom)
                .iter()
                .filter(|atom| graph.atom(**atom).spare_valency)
                .count()
                == 1
        {
            return Some(atom);
        }
    }
    for &atom in atoms {
        if graph.atom(atom).spare_valency {
            let neighbours = intra_fragment_neighbours(graph, atom);
            if neighbours.len() == 2
                && neighbours
                    .iter()
                    .all(|atom| intra_fragment_neighbours(graph, *atom).len() >= 3)
            {
                return Some(atom);
            }
        }
    }
    let (mut first, mut hetero) = (None, None);
    for &atom in atoms {
        let a = graph.atom(atom);
        if !a.spare_valency {
            continue;
        }
        if a.element != Element::C {
            if a.charge == 0 {
                return Some(atom);
            }
            hetero.get_or_insert(atom);
        }
        first.get_or_insert(atom);
    }
    hetero.or(first)
}

fn neighbours_with_bond_order(
    graph: &Graph,
    atom: AtomId,
    visited_order: &mut HashMap<AtomId, u8>,
) -> Vec<AtomId> {
    let neighbours = intra_fragment_neighbours(graph, atom);
    for &neighbour in &neighbours {
        visited_order.insert(
            neighbour,
            graph
                .bond(graph.bond_between(atom, neighbour).unwrap())
                .order,
        );
    }
    neighbours
}
fn compare_element_symbols(
    graph: &Graph,
    a: AtomId,
    b: AtomId,
    order: &HashMap<AtomId, u8>,
) -> Ordering {
    order[&a]
        .cmp(&order[&b])
        .then_with(|| graph.atom(b).out_valency.cmp(&graph.atom(a).out_valency))
        .then_with(|| {
            calculate_substitutable_hydrogen_atoms(graph, b)
                .cmp(&calculate_substitutable_hydrogen_atoms(graph, a))
        })
}
fn assign_element_locant(graph: &mut Graph, atom: AtomId, count: &mut HashMap<String, usize>) {
    let element = graph.atom(atom).element.symbol().to_owned();
    let primes = *count.get(&element).unwrap_or(&0);
    graph.add_locant(atom, format!("{element}{}", "'".repeat(primes)));
    count.insert(element, primes + 1);
}
pub fn assign_element_locants(
    graph: &mut Graph,
    parent: FragmentId,
    suffixes: &[FragmentId],
) -> Result<(), GraphError> {
    let mut count = HashMap::<String, usize>::new();
    let mut ignore = HashSet::new();
    let all: Vec<_> = suffixes
        .iter()
        .copied()
        .chain(std::iter::once(parent))
        .collect();
    for &fragment in &all {
        for &atom in &graph.fragment(fragment).atoms {
            for locant in &graph.atom(atom).locants {
                if is_element_symbol_locant(locant) {
                    let element = locant.trim_end_matches('\'');
                    let primes = locant.len() - element.len();
                    let seen = count.entry(element.into()).or_default();
                    *seen = (*seen).max(primes + 1);
                    ignore.insert(atom);
                }
            }
        }
    }
    for &fragment in &all {
        for &atom in &graph.fragment(fragment).atoms {
            if count.contains_key(graph.atom(atom).element.symbol()) {
                ignore.insert(atom);
            }
        }
    }
    let acid = matches!(
        graph.fragment(parent).fragment_type.as_str(),
        "nonCarboxylicAcid" | "chalcogenAcidStem"
    );
    if acid && !suffixes.is_empty() {
        return Err(GraphError(
            "No suffix fragments were expected to be present on non carboxylic acid".into(),
        ));
    }
    let mut visited_order = HashMap::new();
    let mut visited = HashSet::new();
    let mut start = Vec::new();
    if acid {
        let first = graph
            .fragment(parent)
            .atoms
            .first()
            .copied()
            .ok_or_else(|| GraphError("Acid fragment is empty".into()))?;
        start = neighbours_with_bond_order(graph, first, &mut visited_order);
        visited.insert(first);
    } else {
        for &suffix in suffixes {
            let first = graph
                .fragment(suffix)
                .atoms
                .first()
                .copied()
                .ok_or_else(|| GraphError("Suffix fragment is empty".into()))?;
            let next = neighbours_with_bond_order(graph, first, &mut visited_order);
            visited.extend(next.iter().copied());
            start.extend(next);
        }
    }
    start.sort_by(|a, b| compare_element_symbols(graph, *a, *b, &visited_order));
    let mut queue = VecDeque::from(start);
    while let Some(atom) = queue.pop_front() {
        visited.insert(atom);
        if !ignore.contains(&atom) {
            assign_element_locant(graph, atom, &mut count);
        }
        let mut next = neighbours_with_bond_order(graph, atom, &mut visited_order);
        next.retain(|atom| !visited.contains(atom));
        next.sort_by(|a, b| compare_element_symbols(graph, *a, *b, &visited_order));
        for atom in next.into_iter().rev() {
            queue.push_front(atom);
        }
    }
    if acid {
        let first = graph.fragment(parent).atoms[0];
        if !ignore.contains(&first)
            && graph.determine_valency(first, true) > graph.incoming_valency(first)
        {
            assign_element_locant(graph, first, &mut count);
        }
        return Ok(());
    }
    if count.get("N").is_some_and(|count| *count > 1) {
        for &suffix in suffixes {
            'fragment: for atom in graph.fragment(suffix).atoms.clone() {
                let a = graph.atom(atom);
                if a.element == Element::N
                    && graph.incoming_valency(atom) == 3
                    && a.locants.len() == 1
                    && is_element_symbol_locant(&a.locants[0])
                {
                    for neighbour in graph.neighbours(atom) {
                        if graph.atom(neighbour).element == Element::N
                            && graph.incoming_valency(neighbour) == 1
                        {
                            let locant = graph.atom(atom).locants[0].clone();
                            graph.clear_locants(atom);
                            graph.add_locant(neighbour, locant);
                            break 'fragment;
                        }
                    }
                }
            }
        }
    }
    let elements_to_ignore: HashSet<_> = count.keys().cloned().collect();
    let mut atoms = graph.fragment(parent).atoms.clone();
    atoms.sort_by(|a, b| {
        let a_atom = graph.atom(*a);
        let b_atom = graph.atom(*b);
        a_atom
            .element
            .cmp(&b_atom.element)
            .then_with(|| {
                (calculate_substitutable_hydrogen_atoms(graph, *b) > 0)
                    .cmp(&(calculate_substitutable_hydrogen_atoms(graph, *a) > 0))
            })
            .then_with(|| {
                a_atom
                    .locants
                    .is_empty()
                    .cmp(&b_atom.locants.is_empty())
                    .reverse()
            })
    });
    let (mut carbon, mut multiple_carbons) = (None, false);
    for atom in atoms {
        let element = graph.atom(atom).element;
        if ignore.contains(&atom) || elements_to_ignore.contains(element.symbol()) {
            continue;
        }
        if element == Element::C {
            if multiple_carbons {
                continue;
            }
            if carbon.is_some() {
                carbon = None;
                multiple_carbons = true;
            } else {
                carbon = Some(atom);
            }
        } else {
            assign_element_locant(graph, atom, &mut count);
        }
    }
    if let Some(carbon) = carbon {
        graph.add_locant(carbon, "C");
    }
    Ok(())
}
pub fn get_atom_by_amino_acid_style_locant(
    graph: &Graph,
    backbone: AtomId,
    element: &str,
    primes: &str,
) -> Option<AtomId> {
    let mut order = HashMap::new();
    let mut visited = HashSet::new();
    let mut start = Vec::new();
    for atom in neighbours_with_bond_order(graph, backbone, &mut order) {
        visited.insert(atom);
        if graph.atom(atom).atom_type != "suffix"
            && graph
                .atom(atom)
                .locants
                .iter()
                .any(|locant| is_numeric_locant(locant))
        {
            continue;
        }
        start.push(atom);
    }
    start.sort_by(|a, b| compare_element_symbols(graph, *a, *b, &order));
    let mut queue = VecDeque::from(start);
    let mut count = HashMap::<Element, usize>::new();
    let mut hydrazone = false;
    while let Some(atom) = queue.pop_front() {
        visited.insert(atom);
        let a = graph.atom(atom);
        let seen = count.entry(a.element).or_default();
        let current = *seen;
        *seen += 1;
        if hydrazone {
            if a.element.symbol() == element && current.checked_sub(1) == Some(primes.len()) {
                return Some(atom);
            }
            hydrazone = false;
        }
        let mut next = neighbours_with_bond_order(graph, atom, &mut order);
        next.retain(|atom| {
            !visited.contains(atom)
                && (graph.atom(*atom).atom_type == "suffix"
                    || !graph
                        .atom(*atom)
                        .locants
                        .iter()
                        .any(|locant| is_numeric_locant(locant)))
        });
        if a.element == Element::N
            && graph.incoming_valency(atom) == 3
            && a.charge == 0
            && next.len() == 1
            && graph.atom(next[0]).element == Element::N
        {
            hydrazone = true;
        } else if a.element.symbol() == element && primes.len() == current {
            return Some(atom);
        }
        next.sort_by(|a, b| compare_element_symbols(graph, *a, *b, &order));
        for atom in next.into_iter().rev() {
            queue.push_front(atom);
        }
    }
    (primes.is_empty() && graph.atom(backbone).element.symbol() == element).then_some(backbone)
}
pub fn is_covalent(a: Element, b: Element) -> bool {
    let (Some(a), Some(b)) = (
        valence::pauling_electronegativity(a),
        valence::pauling_electronegativity(b),
    ) else {
        return false;
    };
    let half_sum = (a + b) / 2.;
    half_sum >= 1.6 && (a - b).abs() < 1.76 * half_sum - 3.03
}
pub fn is_functional_atom(graph: &Graph, atom: AtomId) -> bool {
    graph.atom(atom).element.is_chalcogen()
        && graph
            .fragment(graph.atom(atom).fragment)
            .functional_atoms
            .contains(&atom)
}
pub fn is_functional_atom_or_aldehyde(graph: &Graph, atom: AtomId) -> bool {
    graph.atom(atom).properties.is_aldehyde || is_functional_atom(graph, atom)
}
pub fn is_characteristic_atom(graph: &Graph, atom: AtomId) -> bool {
    let a = graph.atom(atom);
    a.atom_type == "suffix"
        || (a.element.is_chalcogen()
            && graph.fragment(a.fragment).sub_type != "heteroStem"
            && graph.incoming_valency(atom) == 1
            && a.out_valency == 0
            && a.charge == 0)
        || is_functional_atom_or_aldehyde(graph, atom)
}
pub fn all_atoms_in_ring_are_identical(graph: &Graph, fragment: FragmentId) -> bool {
    let atoms = &graph.fragment(fragment).atoms;
    let Some(first) = atoms.first() else {
        return true;
    };
    let first = graph.atom(*first);
    atoms.iter().all(|atom| {
        let a = graph.atom(*atom);
        a.element == first.element
            && graph.incoming_valency(*atom) == graph.incoming_valency(first.id)
            && a.spare_valency == first.spare_valency
    })
}
pub fn remove_terminal_atom(graph: &mut Graph, atom: AtomId) -> Result<(), GraphError> {
    let neighbours = graph.neighbours(atom);
    if neighbours.len() != 1 {
        return Err(GraphError("Expected a terminal atom".into()));
    }
    if let Some(parity) = &mut graph.atom_mut(neighbours[0]).parity
        && let Some(reference) = parity
            .atom_refs
            .iter_mut()
            .find(|reference| **reference == Some(StereoReference::Atom(atom)))
    {
        *reference = Some(StereoReference::DeoxyHydrogen);
    }
    graph.remove_atom_and_associated_bonds(atom);
    Ok(())
}
pub fn remove_terminal_oxygen(
    graph: &mut Graph,
    atom: AtomId,
    desired_order: u8,
) -> Result<(), GraphError> {
    for neighbour in graph.neighbours(atom) {
        let n = graph.atom(neighbour);
        if n.element != Element::O || n.bonds.len() != 1 {
            continue;
        }
        let bond = graph.bond_between(atom, neighbour).unwrap();
        if graph.bond(bond).order == desired_order && n.charge == 0 {
            remove_terminal_atom(graph, neighbour)?;
            let a = graph.atom_mut(atom);
            a.lambda_convention_valency = a
                .lambda_convention_valency
                .map(|value| value - i32::from(desired_order));
            a.minimum_valency = a
                .minimum_valency
                .map(|value| value - i32::from(desired_order));
            return Ok(());
        } else if n.charge == -1
            && graph.bond(bond).order == 1
            && desired_order == 2
            && graph.atom(atom).charge == 1
            && graph.atom(atom).element == Element::N
        {
            remove_terminal_atom(graph, neighbour)?;
            let a = graph.atom_mut(atom);
            a.charge = 0;
            a.protons_explicitly_added_or_removed = 0;
            return Ok(());
        }
    }
    Err(GraphError(match desired_order {2=>"Double bonded oxygen not found at suffix attachment position. Perhaps a suffix has been used inappropriately",1=>"Hydroxy oxygen not found at suffix attachment position. Perhaps a suffix has been used inappropriately",_=>"Suitable oxygen not found at suffix attachment position Perhaps a suffix has been used inappropriately"}.into()))
}
pub fn find_hydroxy_like_terminal_atoms(
    graph: &Graph,
    atoms: &[AtomId],
    element: Element,
) -> Vec<AtomId> {
    atoms
        .iter()
        .copied()
        .filter(|atom| {
            let a = graph.atom(*atom);
            a.element == element
                && graph.incoming_valency(*atom) == 1
                && a.out_valency == 0
                && a.charge == 0
        })
        .collect()
}
pub fn not_in_six_member_or_smaller_ring(graph: &Graph, bond: BondId) -> bool {
    let b = graph.bond(bond);
    if !graph.atom(b.from).in_cycle || !graph.atom(b.to).in_cycle {
        return true;
    }
    let mut visited = HashSet::from([b.from]);
    let mut queue: VecDeque<_> = graph
        .neighbours(b.from)
        .into_iter()
        .filter(|atom| *atom != b.to)
        .collect();
    for _ in 0..5 {
        let mut next = VecDeque::new();
        while let Some(atom) = queue.pop_front() {
            if atom == b.to {
                return false;
            }
            visited.insert(atom);
            for neighbour in graph.neighbours(atom) {
                if !visited.contains(&neighbour) && graph.atom(neighbour).in_cycle {
                    next.push_back(neighbour);
                }
            }
        }
        queue = next;
    }
    true
}
pub fn find_hydroxy_groups(graph: &Graph, fragment: FragmentId) -> Result<Vec<AtomId>, GraphError> {
    let mut result = Vec::new();
    for &atom in &graph.fragment(fragment).atoms {
        let a = graph.atom(atom);
        if a.element != Element::O
            || graph.incoming_valency(atom) != 1
            || a.out_valency != 0
            || a.charge != 0
        {
            continue;
        }
        let adjacent = graph
            .neighbours(atom)
            .first()
            .copied()
            .ok_or_else(|| GraphError("Hydroxy atom has no neighbour".into()))?;
        if graph.atom(adjacent).element != Element::C {
            continue;
        }
        let others: Vec<_> = graph
            .neighbours(adjacent)
            .into_iter()
            .filter(|other| *other != atom)
            .collect();
        if others.iter().take(2).any(|other| {
            graph.atom(*other).element == Element::O
                && graph
                    .bond(graph.bond_between(adjacent, *other).unwrap())
                    .order
                    == 2
        }) {
            continue;
        }
        result.push(atom);
    }
    Ok(result)
}
pub fn find_n_atoms_for_substitution(
    graph: &Graph,
    atoms: &[AtomId],
    preferred: Option<AtomId>,
    required: usize,
    bond_order: i32,
    take_out: bool,
    preserve_valency: bool,
) -> Result<Option<Vec<AtomId>>, GraphError> {
    if bond_order <= 0 {
        return Err(GraphError(
            "Substitution bond order must be positive".into(),
        ));
    }
    let start = if let Some(preferred) = preferred {
        atoms.iter().position(|atom|*atom==preferred).ok_or_else(||GraphError("OPSIN Bug: preferredAtom should be part of the list of atoms to search through".into()))?
    } else {
        0
    };
    let ordered: Vec<_> = atoms
        .iter()
        .cycle()
        .skip(start)
        .take(atoms.len())
        .copied()
        .collect();
    for stage in 0..4 {
        if stage == 2 && preserve_valency {
            return Ok(None);
        }
        let mut result = Vec::new();
        if stage == 0
            && atoms.len() == 1
            && graph.fragment(graph.atom(atoms[0]).fragment).fragment_type == "elementaryAtom"
        {
            let atom = atoms[0];
            let a = graph.atom(atom);
            let oxidation = a
                .properties
                .oxidation_number
                .or_else(|| {
                    graph
                        .fragment(a.fragment)
                        .token_attributes
                        .get("commonOxidationStatesAndMax")
                        .and_then(|value| {
                            value
                                .split(':')
                                .next()?
                                .split(',')
                                .next_back()?
                                .parse()
                                .ok()
                        })
                })
                .unwrap_or(0);
            for _ in 0..(oxidation - graph.incoming_valency(atom)).max(0) {
                result.push(atom);
            }
        } else {
            for &atom in &ordered {
                let a = graph.atom(atom);
                let preferred = required == 1 && Some(atom) == preferred;
                if stage == 0 && is_characteristic_atom(graph, atom) && !preferred {
                    continue;
                }
                if stage == 1 && is_functional_atom_or_aldehyde(graph, atom) && !preferred {
                    continue;
                }
                let maximum = if stage < 2 {
                    Some(graph.determine_valency(atom, take_out))
                } else {
                    valence::maximum_atom_valency(graph, atom)
                };
                if let Some(maximum) = maximum {
                    let used = graph.incoming_valency(atom)
                        + if stage < 3 && a.spare_valency { 1 } else { 0 }
                        + if take_out { a.out_valency } else { 0 };
                    for _ in 0..((maximum - used) / bond_order).max(0) {
                        result.push(atom);
                    }
                } else {
                    for _ in 0..required {
                        result.push(atom);
                    }
                }
            }
        }
        if result.len() >= required {
            return Ok(Some(result));
        }
    }
    Ok(None)
}
pub fn find_substitutable_atoms(
    graph: &Graph,
    fragment: FragmentId,
    bond_order: i32,
) -> Result<Vec<AtomId>, GraphError> {
    let f = graph.fragment(fragment);
    Ok(find_n_atoms_for_substitution(
        graph,
        &f.atoms,
        f.default_in_atom,
        1,
        bond_order,
        true,
        false,
    )?
    .unwrap_or_default())
}
pub fn last_non_suffix_carbon_with_sufficient_valency(
    graph: &Graph,
    fragment: FragmentId,
) -> Option<AtomId> {
    graph
        .fragment(fragment)
        .atoms
        .iter()
        .rev()
        .copied()
        .find(|atom| {
            graph.atom(*atom).atom_type != "suffix"
                && graph.atom(*atom).element == Element::C
                && valence::check_valency_available_for_bond(graph, *atom, 1)
        })
}
