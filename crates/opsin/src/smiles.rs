//! OPSIN resource SMILES dialect and semantic serialization foundations.
//!
//! Translated from SMILESFragmentBuilder and SMILESWriter in OPSIN 2.9.0,
//! b91b610af5ab07560fedb20730d7aef46bb2bca0, MIT Daniel Lowe/contributors.
//! The reader produces a construction graph: bracket hydrogens are valency
//! hints, lower-case atoms have spare valency, and leading/trailing bonds are
//! out atoms. This is intentionally different from a generic SMILES parser.

use std::collections::{HashMap, VecDeque};

use crate::api::SerializationError;
use crate::graph::{
    AtomId, AtomParity, BondDirection, BondId, BondStereo, BondStereoValue, Element, FragmentId,
    Graph, GraphError, StereoGroupType, StereoReference,
};
use crate::valence;

#[derive(Clone)]
struct Frame {
    atom: Option<AtomId>,
    order: u8,
    slash: Option<BondDirection>,
    dummy_index: Option<usize>,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            atom: None,
            order: 1,
            slash: None,
            dummy_index: None,
        }
    }
}
impl Frame {
    fn branch(&self) -> Self {
        Self {
            atom: self.atom,
            order: self.order,
            slash: None,
            dummy_index: None,
        }
    }
}

fn organic(symbol: &str) -> bool {
    matches!(
        symbol,
        "B" | "C" | "N" | "O" | "P" | "S" | "F" | "Cl" | "Br" | "I"
    )
}
fn aromatic(symbol: &str) -> bool {
    matches!(
        symbol,
        "c" | "n" | "o" | "p" | "s" | "si" | "as" | "se" | "sb" | "te"
    )
}

/// Build one resource fragment transactionally. A malformed resource leaves
/// the graph unchanged. Labelling occurs before explicit inorganic H creation,
/// matching the upstream ordering.
pub fn build_fragment(
    graph: &mut Graph,
    smiles: &str,
    fragment_type: &str,
    label_mapping: &str,
) -> Result<FragmentId, GraphError> {
    let backup = graph.clone();
    let result = build_fragment_inner(graph, smiles, fragment_type, label_mapping);
    if result.is_err() {
        *graph = backup;
    }
    result
}

fn build_fragment_inner(
    graph: &mut Graph,
    smiles: &str,
    fragment_type: &str,
    labels: &str,
) -> Result<FragmentId, GraphError> {
    let fragment = graph.add_fragment(fragment_type);
    if smiles.is_empty() {
        return Ok(fragment);
    }
    if !smiles.is_ascii() {
        return Err(GraphError(
            "Resource SMILES must contain ASCII characters".into(),
        ));
    }
    let bytes = smiles.as_bytes();
    let attachment = |byte| match byte {
        b'-' => Some(1),
        b'=' => Some(2),
        b'#' => Some(3),
        _ => None,
    };
    let first_out = attachment(bytes[0]);
    let last_out = attachment(bytes[bytes.len() - 1]);
    let mut index = usize::from(first_out.is_some());
    let end = bytes.len() - usize::from(last_out.is_some());
    let mut stack = vec![Frame::default()];
    let mut rings: HashMap<String, Frame> = HashMap::new();
    while index < end {
        let byte = bytes[index];
        match byte {
            b'(' => {
                let frame = stack.last().unwrap().branch();
                stack.push(frame);
                index += 1;
            }
            b')' => {
                if stack.len() == 1 {
                    return Err(GraphError("Unmatched closing branch".into()));
                }
                stack.pop();
                index += 1;
            }
            b'-' | b'=' | b'#' => {
                let frame = stack.last_mut().unwrap();
                if byte != b'-' && frame.order != 1 {
                    return Err(GraphError("Bond order already defined".into()));
                }
                frame.order = attachment(byte).unwrap();
                index += 1;
            }
            b'/' | b'\\' => {
                let frame = stack.last_mut().unwrap();
                if frame.slash.is_some() {
                    return Err(GraphError("Bond configuration already defined".into()));
                }
                frame.slash = Some(if byte == b'/' {
                    BondDirection::Slash
                } else {
                    BondDirection::Backslash
                });
                index += 1;
            }
            b'.' => {
                stack.last_mut().unwrap().atom = None;
                index += 1;
            }
            b'0'..=b'9' | b'%' => {
                let closure = if byte == b'%' {
                    if index + 2 >= end
                        || !bytes[index + 1].is_ascii_digit()
                        || !bytes[index + 2].is_ascii_digit()
                    {
                        return Err(GraphError(
                            "A ring opening indice after a % must be two digits long".into(),
                        ));
                    }
                    let value = smiles[index + 1..index + 3].to_owned();
                    index += 3;
                    value
                } else {
                    index += 1;
                    (byte as char).to_string()
                };
                let current = stack.last_mut().unwrap();
                let current_atom = current.atom.ok_or_else(|| {
                    GraphError("A ring opening has appeared before any atom".into())
                })?;
                if let Some(opening) = rings.remove(&closure) {
                    let opening_atom = opening.atom.unwrap();
                    if opening.order > 1 && current.order > 1 && opening.order != current.order {
                        return Err(GraphError(
                            "Ring closure has two different bond orders specified".into(),
                        ));
                    }
                    let order = opening.order.max(current.order);
                    let bond = if let Some(direction) = current.slash.take() {
                        if opening.slash == Some(direction) {
                            return Err(GraphError(
                                "Contradictory double bond stereoconfiguration".into(),
                            ));
                        }
                        let bond = graph.add_bond(current_atom, opening_atom, order)?;
                        graph.bond_mut(bond).smiles_direction = Some(direction);
                        bond
                    } else {
                        let bond = graph.add_bond(opening_atom, current_atom, order)?;
                        graph.bond_mut(bond).smiles_direction = opening.slash;
                        bond
                    };
                    let _ = bond;
                    if let Some(parity) = &mut graph.atom_mut(current_atom).parity {
                        parity.add_reference(StereoReference::Atom(opening_atom))?;
                    }
                    if let Some(parity) = &mut graph.atom_mut(opening_atom).parity {
                        let dummy = opening.dummy_index.ok_or_else(|| {
                            GraphError("Ring stereo placeholder was not set".into())
                        })?;
                        parity.atom_refs[dummy] = Some(StereoReference::Atom(current_atom));
                    }
                } else {
                    let mut opening = current.branch();
                    opening.slash = current.slash.take();
                    if let Some(parity) = &mut graph.atom_mut(current_atom).parity {
                        opening.dummy_index =
                            Some(parity.add_reference(StereoReference::RingOpening)?);
                    }
                    rings.insert(closure, opening);
                }
                current.order = 1;
            }
            b'[' => {
                let close = bytes[index + 1..end]
                    .iter()
                    .position(|byte| *byte == b']')
                    .map(|offset| index + 1 + offset)
                    .ok_or_else(|| GraphError("[ without matching ]".into()))?;
                let atom = parse_bracket(
                    graph,
                    fragment,
                    &smiles[index + 1..close],
                    stack.last_mut().unwrap(),
                    first_out,
                )?;
                stack.last_mut().unwrap().atom = Some(atom);
                index = close + 1;
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'*' => {
                let mut symbol = (byte as char).to_string();
                if byte.is_ascii_uppercase()
                    && index + 1 < end
                    && organic(&smiles[index..index + 2])
                {
                    symbol = smiles[index..index + 2].to_owned();
                    index += 1;
                }
                let spare = byte.is_ascii_lowercase();
                if byte == b'*' {
                    symbol = "R".into();
                } else if spare {
                    if !aromatic(&symbol) {
                        return Err(GraphError(format!("{symbol} is not an aromatic Element")));
                    }
                    symbol.make_ascii_uppercase();
                } else if !organic(&symbol) {
                    return Err(GraphError(format!(
                        "{symbol} is not an organic Element; square brackets required"
                    )));
                }
                let element = Element::from_symbol(&symbol)
                    .ok_or_else(|| GraphError(format!("Unknown element {symbol}")))?;
                let atom = graph.add_atom(fragment, element);
                graph.atom_mut(atom).spare_valency = spare;
                connect_atom(graph, stack.last_mut().unwrap(), atom)?;
                index += 1;
            }
            _ => {
                return Err(GraphError(format!(
                    "{} is in an unexpected position in OPSIN resource SMILES",
                    byte as char
                )));
            }
        }
    }
    if stack.len() != 1 {
        return Err(GraphError("Unmatched opening branch".into()));
    }
    if !rings.is_empty() {
        return Err(GraphError("Unmatched ring opening".into()));
    }
    if stack[0].order != 1 || stack[0].slash.is_some() {
        return Err(GraphError("Unconsumed bond descriptor".into()));
    }
    if graph.fragment(fragment).atoms.is_empty() {
        return Err(GraphError("Resource SMILES has no atoms".into()));
    }
    if let Some(order) = first_out {
        let first = graph.fragment(fragment).atoms[0];
        graph.add_out_atom(fragment, first, order.into(), true);
    }
    if let Some(order) = last_out {
        let last = stack[0]
            .atom
            .ok_or_else(|| GraphError("Trailing attachment has no atom".into()))?;
        graph.add_out_atom(fragment, last, order.into(), true);
    }
    label_fragment(graph, fragment, labels)?;
    verify_atom_parities(graph, fragment)?;
    add_bond_stereo(graph, fragment)?;
    for atom in graph.fragment(fragment).atoms.clone() {
        if graph.atom(atom).properties.smiles_hydrogen_count.is_some()
            && graph.atom(atom).lambda_convention_valency.is_none()
        {
            setup_atom_valency(graph, atom)?;
        }
    }
    graph.assign_cycle_membership(fragment);
    Ok(fragment)
}

fn connect_atom(graph: &mut Graph, frame: &mut Frame, atom: AtomId) -> Result<(), GraphError> {
    if let Some(previous) = frame.atom {
        let bond = graph.add_bond(previous, atom, frame.order)?;
        graph.bond_mut(bond).smiles_direction = frame.slash.take();
        if let Some(parity) = &mut graph.atom_mut(previous).parity {
            parity.add_reference(StereoReference::Atom(atom))?;
        }
    }
    frame.atom = Some(atom);
    frame.order = 1;
    Ok(())
}

fn number(bytes: &[u8], index: &mut usize) -> Result<Option<u32>, GraphError> {
    let start = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    if start == *index {
        return Ok(None);
    }
    let value = std::str::from_utf8(&bytes[start..*index])
        .unwrap()
        .parse()
        .map_err(|_| GraphError("SMILES numeric field is too large".into()))?;
    Ok(Some(value))
}

fn parse_bracket(
    graph: &mut Graph,
    fragment: FragmentId,
    content: &str,
    frame: &mut Frame,
    first_out: Option<u8>,
) -> Result<AtomId, GraphError> {
    let bytes = content.as_bytes();
    let mut index = 0;
    let isotope = number(bytes, &mut index)?;
    let element_start = index;
    let first = *bytes
        .get(index)
        .ok_or_else(|| GraphError("No element found in square brackets".into()))?;
    index += 1;
    if bytes.get(index).is_some_and(u8::is_ascii_lowercase) {
        index += 1;
    }
    let symbol = &content[element_start..index];
    let spare = first.is_ascii_lowercase();
    let element = if symbol == "*" {
        Element::R
    } else if spare {
        if !aromatic(symbol) {
            return Err(GraphError(format!("{symbol} is not an aromatic Element")));
        }
        let mut normal = symbol.to_owned();
        normal[..1].make_ascii_uppercase();
        Element::from_symbol(&normal)
            .ok_or_else(|| GraphError(format!("Unknown element {symbol}")))?
    } else {
        Element::from_symbol(symbol)
            .ok_or_else(|| GraphError(format!("Unknown element {symbol}")))?
    };
    let atom = graph.add_atom(fragment, element);
    graph.atom_mut(atom).spare_valency = spare;
    graph.atom_mut(atom).isotope = isotope;
    let previous = frame.atom;
    connect_atom(graph, frame, atom)?;
    let mut hydrogen_count = Some(0);
    let mut hydrogen_specified = false;
    let mut charge_specified = false;
    while index < bytes.len() {
        match bytes[index] {
            b'@' => {
                if graph.atom(atom).parity.is_some() {
                    return Err(GraphError("Atom parity specified twice".into()));
                }
                index += 1;
                let clockwise = bytes.get(index) == Some(&b'@');
                if clockwise {
                    index += 1;
                }
                let mut parity = AtomParity::new([None; 4], if clockwise { 1 } else { -1 });
                if let Some(previous) = previous {
                    parity.add_reference(StereoReference::Atom(previous))?;
                } else if graph.fragment(fragment).atoms.len() == 1 && first_out == Some(1) {
                    parity.add_reference(StereoReference::DeoxyHydrogen)?;
                }
                if bytes.get(index) == Some(&b'H') {
                    parity.add_reference(StereoReference::ImplicitHydrogen)?;
                }
                graph.atom_mut(atom).parity = Some(parity);
            }
            b'H' => {
                if hydrogen_specified {
                    return Err(GraphError("Hydrogen count specified twice".into()));
                }
                hydrogen_specified = true;
                index += 1;
                if bytes.get(index) == Some(&b'?') {
                    index += 1;
                    hydrogen_count = None;
                } else {
                    let count = number(bytes, &mut index)?.unwrap_or(1);
                    hydrogen_count = Some(count);
                    if spare && (!matches!(element, Element::C | Element::Si) || count >= 2) {
                        graph.fragment_mut(fragment).indicated_hydrogens.push(atom);
                    }
                }
            }
            b'+' | b'-' => {
                if charge_specified {
                    return Err(GraphError("Charge specified twice".into()));
                }
                charge_specified = true;
                let sign = bytes[index];
                index += 1;
                let count = if let Some(count) = number(bytes, &mut index)? {
                    count
                } else {
                    let mut count = 1;
                    while bytes.get(index) == Some(&sign) {
                        count += 1;
                        index += 1;
                    }
                    count
                };
                let count =
                    i32::try_from(count).map_err(|_| GraphError("Charge is too large".into()))?;
                graph.atom_mut(atom).charge = if sign == b'+' { count } else { -count };
            }
            b'|' => {
                index += 1;
                let value = number(bytes, &mut index)?
                    .ok_or_else(|| GraphError("Lambda convention has no valency".into()))?;
                let value = i32::try_from(value)
                    .map_err(|_| GraphError("Lambda valency is too large".into()))?;
                graph.atom_mut(atom).lambda_convention_valency = Some(value);
            }
            _ => {
                return Err(GraphError(format!(
                    "Unexpected character {} in square brackets",
                    bytes[index] as char
                )));
            }
        }
    }
    graph.atom_mut(atom).properties.smiles_hydrogen_count = hydrogen_count;
    Ok(atom)
}

fn label_fragment(graph: &mut Graph, fragment: FragmentId, labels: &str) -> Result<(), GraphError> {
    let atoms = graph.fragment(fragment).atoms.clone();
    match labels {
        "none" | "" => {}
        "numeric" => {
            for (index, atom) in atoms.into_iter().enumerate() {
                graph.add_locant(atom, (index + 1).to_string());
            }
        }
        "fusedRing" => {
            let mut number = 0;
            let mut letter = b'a';
            for atom in atoms {
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
        _ => {
            let mapping: Vec<_> = labels.split('/').collect();
            if mapping.len() != atoms.len() {
                return Err(GraphError(format!(
                    "Group numbering invalid in resource file: labels: {}, atoms: {}",
                    mapping.len(),
                    atoms.len()
                )));
            }
            for (atom, labels) in atoms.into_iter().zip(mapping) {
                for label in labels.split(',').filter(|label| !label.is_empty()) {
                    graph.add_locant(atom, label);
                }
            }
        }
    }
    Ok(())
}

fn verify_atom_parities(graph: &mut Graph, fragment: FragmentId) -> Result<(), GraphError> {
    let atoms = graph.fragment(fragment).atoms.clone();
    for (index, atom) in atoms.iter().copied().enumerate() {
        let Some(mut parity) = graph.atom(atom).parity.clone() else {
            continue;
        };
        let missing = parity
            .atom_refs
            .iter()
            .filter(|reference| reference.is_none())
            .count();
        let hydrogen = parity
            .atom_refs
            .contains(&Some(StereoReference::ImplicitHydrogen));
        if missing != 0 {
            if missing == 1
                && !hydrogen
                && matches!(
                    graph.atom(atom).element,
                    Element::N | Element::S | Element::Se
                )
            {
                let preceding = match parity.atom_refs[0] {
                    Some(StereoReference::Atom(id)) => atoms
                        .iter()
                        .position(|atom| *atom == id)
                        .is_some_and(|position| position < index),
                    _ => true, // Java indexOf(dummy) returns -1.
                };
                parity.atom_refs[3] = parity.atom_refs[2];
                parity.atom_refs[2] = parity.atom_refs[1];
                if preceding {
                    parity.atom_refs[1] = Some(StereoReference::Atom(atom));
                } else {
                    parity.atom_refs[1] = parity.atom_refs[0];
                    parity.atom_refs[0] = Some(StereoReference::Atom(atom));
                }
            } else {
                return Err(GraphError("SMILES is malformed. Tetrahedral stereochemistry defined on a non tetrahedral centre".into()));
            }
        }
        graph.atom_mut(atom).parity = Some(parity);
    }
    Ok(())
}

fn add_bond_stereo(graph: &mut Graph, fragment: FragmentId) -> Result<(), GraphError> {
    let bonds = graph.fragment(fragment).bonds.clone();
    for central in &bonds {
        let central_bond = graph.bond(*central).clone();
        if central_bond.order != 2 {
            continue;
        }
        for preceding in graph.atom(central_bond.from).bonds.clone() {
            let first = graph.bond(preceding).clone();
            let Some(first_direction) = first.smiles_direction else {
                continue;
            };
            for following in graph.atom(central_bond.to).bonds.clone() {
                let second = graph.bond(following).clone();
                let Some(second_direction) = second.smiles_direction else {
                    continue;
                };
                let atom1 = first.other_atom(central_bond.from).unwrap();
                let atom4 = second.other_atom(central_bond.to).unwrap();
                let up_first = match first_direction {
                    BondDirection::Backslash => first.to == central_bond.from,
                    BondDirection::Slash => first.to != central_bond.from,
                };
                let up_second = match second_direction {
                    BondDirection::Backslash => second.from != central_bond.to,
                    BondDirection::Slash => second.from == central_bond.to,
                };
                let value = if up_first == up_second {
                    BondStereoValue::Cis
                } else {
                    BondStereoValue::Trans
                };
                if let Some(existing) = &graph.bond(*central).stereo {
                    // Exact upstream redundant slash conflict rule.
                    let one_ref_same =
                        existing.atom_refs[0] == atom1 || existing.atom_refs[3] == atom4;
                    if (one_ref_same && existing.value == value)
                        || (!one_ref_same && existing.value != value)
                    {
                        return Err(GraphError(
                            "Contradictory double bond stereoconfiguration".into(),
                        ));
                    }
                } else {
                    graph.bond_mut(*central).stereo = Some(BondStereo {
                        atom_refs: [atom1, central_bond.from, central_bond.to, atom4],
                        value,
                    });
                }
            }
        }
    }
    for bond in bonds {
        graph.bond_mut(bond).smiles_direction = None;
    }
    Ok(())
}

fn setup_atom_valency(graph: &mut Graph, atom: AtomId) -> Result<(), GraphError> {
    let a = graph.atom(atom).clone();
    let hydrogen = a.properties.smiles_hydrogen_count.unwrap();
    let mut incoming = graph.incoming_valency(atom) + hydrogen as i32 + a.out_valency;
    let charge = a.charge;
    let absolute_charge = charge.abs();
    if a.spare_valency {
        let mut hw = valence::hw_valency(a.element)
            .ok_or_else(|| GraphError(format!("{} is not expected to be aromatic", a.element)))?;
        if absolute_charge > 1 {
            return Err(GraphError(format!(
                "{} is not expected to be aromatic",
                a.element
            )));
        }
        if absolute_charge != 0 {
            hw = valence::possible_valencies(a.element, charge)
                .and_then(|values| values.first().copied())
                .ok_or_else(|| {
                    GraphError(format!(
                        "{} with charge {charge} is not expected to be aromatic",
                        a.element
                    ))
                })?;
        }
        if incoming < hw {
            incoming += 1;
        }
    }
    if let Some(default) = valence::default_valency(a.element) {
        if default != incoming || charge != 0 {
            if (incoming - default).abs() == absolute_charge {
                graph.atom_mut(atom).protons_explicitly_added_or_removed = incoming - default;
            } else {
                let plausible = valence::possible_valencies(a.element, 0)
                    .unwrap()
                    .iter()
                    .copied()
                    .find(|value| (incoming - value).abs() == absolute_charge);
                if let Some(value) = plausible {
                    graph.atom_mut(atom).protons_explicitly_added_or_removed = incoming - value;
                    if charge != 0 {
                        graph.atom_mut(atom).lambda_convention_valency = Some(value);
                    } else {
                        graph.atom_mut(atom).minimum_valency = Some(incoming);
                    }
                } else {
                    graph.atom_mut(atom).minimum_valency = Some(incoming);
                }
            }
        }
    } else {
        for _ in 0..hydrogen {
            let hydrogen = graph.add_atom(a.fragment, Element::H);
            graph.add_bond(atom, hydrogen, 1)?;
        }
    }
    Ok(())
}

/// OPSIN semantic CXSMILES. Spare valency and stereo placeholders must be
/// resolved before export. Finalized radicals retain out atoms in upstream;
/// their explicit valence, rather than the out-atom metadata, encodes radicals.
pub fn write_semantic_cxsmiles(
    graph: &Graph,
    fragment: FragmentId,
) -> Result<String, SerializationError> {
    Writer::new(graph, fragment)?.write()
}

struct Writer<'a> {
    graph: &'a Graph,
    fragment: FragmentId,
    depth: Vec<Option<usize>>,
    next: Vec<Option<AtomId>>,
    bond_order: Vec<BondId>,
    directions: Vec<Option<BondDirection>>,
    closure: HashMap<BondId, String>,
    available_closures: VecDeque<String>,
    output_order: Vec<AtomId>,
    output: String,
}

impl<'a> Writer<'a> {
    fn new(graph: &'a Graph, fragment: FragmentId) -> Result<Self, SerializationError> {
        let frag = graph
            .fragments
            .get(fragment.0)
            .filter(|fragment| fragment.active)
            .ok_or_else(|| {
                SerializationError("Cannot export missing or inactive fragment".into())
            })?;
        for out in &frag.out_atoms {
            if out.valency <= 0
                || graph
                    .atoms
                    .get(out.atom.0)
                    .is_none_or(|atom| !atom.active || atom.fragment != fragment)
            {
                return Err(SerializationError(
                    "Radical out atom has invalid valency or ownership".into(),
                ));
            }
        }
        for atom in &frag.atoms {
            let a = graph
                .atoms
                .get(atom.0)
                .ok_or_else(|| SerializationError("Fragment references a missing atom".into()))?;
            if !a.active || a.fragment != fragment {
                return Err(SerializationError("Invalid fragment ownership".into()));
            }
            if a.spare_valency {
                return Err(SerializationError(
                    "Spare valency must be converted to double bonds before semantic export".into(),
                ));
            }
            for bond_id in &a.bonds {
                let bond = graph
                    .bonds
                    .get(bond_id.0)
                    .filter(|bond| bond.active && (1..=3).contains(&bond.order))
                    .ok_or_else(|| {
                        SerializationError("Atom references a missing or invalid bond".into())
                    })?;
                let neighbour = bond.other_atom(*atom).and_then(|neighbour| graph.atoms.get(neighbour.0))
                    .filter(|neighbour| neighbour.active && neighbour.fragment == fragment)
                    .ok_or_else(|| SerializationError("Fragment has invalid or interfragment bonds; consolidate the structure before export".into()))?;
                if !neighbour.bonds.contains(bond_id) || !frag.bonds.contains(bond_id) {
                    return Err(SerializationError("Bond membership is inconsistent".into()));
                }
                if let Some(stereo) = &bond.stereo {
                    for reference in stereo.atom_refs {
                        if graph
                            .atoms
                            .get(reference.0)
                            .is_none_or(|atom| !atom.active || atom.fragment != fragment)
                        {
                            return Err(SerializationError(
                                "Bond stereo references a missing atom".into(),
                            ));
                        }
                    }
                }
            }
            if let Some(parity) = &a.parity {
                for reference in parity.atom_refs {
                    if let Some(StereoReference::Atom(reference)) = reference
                        && graph
                            .atoms
                            .get(reference.0)
                            .is_none_or(|atom| !atom.active || atom.fragment != fragment)
                    {
                        return Err(SerializationError(
                            "Atom parity references a missing atom".into(),
                        ));
                    }
                }
            }
        }
        let mut available_closures: VecDeque<_> = (1..=9)
            .map(|number| number.to_string())
            .chain((10..=99).map(|number| format!("%{number}")))
            .collect();
        available_closures.push_back("0".into());
        Ok(Self {
            graph,
            fragment,
            depth: vec![None; graph.atoms.len()],
            next: vec![None; graph.bonds.len()],
            bond_order: Vec::new(),
            directions: vec![None; graph.bonds.len()],
            closure: HashMap::new(),
            available_closures,
            output_order: Vec::new(),
            output: String::new(),
        })
    }

    fn implicit_proton(&self, atom: AtomId) -> bool {
        let a = self.graph.atom(atom);
        if a.element != Element::H
            || a.charge != 0
            || a.isotope.is_some_and(|isotope| isotope != 1)
            || a.bonds.len() != 1
        {
            return false;
        }
        let neighbour = self.graph.bond(a.bonds[0]).other_atom(atom).unwrap();
        let n = self.graph.atom(neighbour);
        if matches!(n.element, Element::H | Element::R) {
            return false;
        }
        !(n.element == Element::N
            && n.bonds.len() == 2
            && n.bonds
                .iter()
                .any(|bond| self.graph.bond(*bond).stereo.is_some()))
    }

    fn assign_order(&mut self, start: AtomId) {
        let mut stack: Vec<(AtomId, Option<BondId>, usize)> = vec![(start, None, 0)];
        while let Some((atom, bond_taken, depth)) = stack.pop() {
            if let Some(bond) = bond_taken {
                if self.next[bond.0].is_none() {
                    self.bond_order.push(bond);
                }
                self.next[bond.0] = Some(atom);
            }
            if self.depth[atom.0].is_some() {
                continue;
            }
            self.depth[atom.0] = Some(depth);
            for &bond in self.graph.atom(atom).bonds.iter().rev() {
                if Some(bond) == bond_taken {
                    continue;
                }
                let neighbour = self.graph.bond(bond).other_atom(atom).unwrap();
                if !self.implicit_proton(neighbour) {
                    stack.push((neighbour, Some(bond), depth + 1));
                }
            }
        }
    }

    fn write(mut self) -> Result<String, SerializationError> {
        let mut roots = Vec::new();
        let atoms = self.graph.fragment(self.fragment).atoms.clone();
        for &atom in &atoms {
            if self.depth[atom.0].is_none() && self.graph.atom(atom).element == Element::R {
                self.assign_order(atom);
                roots.push(atom);
            }
        }
        for &atom in &atoms {
            if self.depth[atom.0].is_none() && !self.implicit_proton(atom) {
                self.assign_order(atom);
                roots.push(atom);
            }
        }
        self.assign_stereo_directions()?;
        for root in roots {
            if !self.output.is_empty() {
                self.output.push('.');
            }
            self.traverse(root)?;
        }
        self.extended_layer()?;
        Ok(self.output)
    }

    fn assign_stereo_directions(&mut self) -> Result<(), SerializationError> {
        let mut pending: VecDeque<_> = self
            .bond_order
            .iter()
            .copied()
            .filter(|bond| self.graph.bond(*bond).stereo.is_some())
            .collect();
        let mut visited = vec![false; self.graph.bonds.len()];
        while let Some(central) = pending.pop_front() {
            if visited[central.0] {
                continue;
            }
            visited[central.0] = true;
            let next = self.assign_one_stereo(central)?;
            for bond in next {
                pending.push_front(bond);
            }
        }
        Ok(())
    }

    fn assign_one_stereo(&mut self, central: BondId) -> Result<Vec<BondId>, SerializationError> {
        let stereo = self.graph.bond(central).stereo.as_ref().unwrap();
        let refs = stereo.atom_refs;
        if self.graph.bond(central).order != 2
            || self.graph.bond(central).other_atom(refs[1]) != Some(refs[2])
        {
            return Err(SerializationError(
                "Bond stereo has invalid central bond references".into(),
            ));
        }
        let first = self
            .graph
            .bond_between(refs[0], refs[1])
            .ok_or_else(|| SerializationError("Bond stereo references are not bonded".into()))?;
        let second = self
            .graph
            .bond_between(refs[2], refs[3])
            .ok_or_else(|| SerializationError("Bond stereo references are not bonded".into()))?;
        let first_to = self.next[first.0].ok_or_else(|| {
            SerializationError("Bond stereo references an implicit hydrogen".into())
        })?;
        let second_to = self.next[second.0].ok_or_else(|| {
            SerializationError("Bond stereo references an implicit hydrogen".into())
        })?;
        let mut d1 = BondDirection::Backslash;
        let mut d2 = if stereo.value == BondStereoValue::Cis {
            BondDirection::Slash
        } else {
            BondDirection::Backslash
        };
        if first_to != refs[1] {
            d1 = d1.flipped();
        }
        if second_to != refs[3] {
            d2 = d2.flipped();
        }
        if self.directions[first.0].is_some_and(|direction| direction != d1)
            || self.directions[second.0].is_some_and(|direction| direction != d2)
        {
            d1 = d1.flipped();
            d2 = d2.flipped();
        }
        let other = |atom, primary| {
            let bonds: Vec<_> = self
                .graph
                .atom(atom)
                .bonds
                .iter()
                .copied()
                .filter(|bond| *bond != primary && *bond != central)
                .collect();
            if bonds.len() == 1 && self.next[bonds[0].0].is_some() {
                Some(bonds[0])
            } else {
                None
            }
        };
        let other1 = other(refs[1], first);
        let other2 = other(refs[2], second);
        let mut od1 = other1.map(|bond| {
            let mut direction = d1.flipped();
            if first_to != refs[1] {
                direction = direction.flipped();
            }
            if self.next[bond.0] != Some(refs[1]) {
                direction = direction.flipped();
            }
            direction
        });
        let mut od2 = other2.map(|bond| {
            let mut direction = d2.flipped();
            if second_to != refs[3] {
                direction = direction.flipped();
            }
            if self.next[bond.0] != self.graph.bond(bond).other_atom(refs[2]) {
                direction = direction.flipped();
            }
            direction
        });
        if other1.is_some_and(|bond| {
            self.directions[bond.0].is_some_and(|direction| Some(direction) != od1)
        }) || other2.is_some_and(|bond| {
            self.directions[bond.0].is_some_and(|direction| Some(direction) != od2)
        }) {
            d1 = d1.flipped();
            d2 = d2.flipped();
            od1 = od1.map(BondDirection::flipped);
            od2 = od2.map(BondDirection::flipped);
        }
        let mut related = Vec::new();
        for (bond, direction, endpoint) in [
            (Some(first), Some(d1), refs[1]),
            (Some(second), Some(d2), refs[2]),
            (other1, od1, refs[1]),
            (other2, od2, refs[2]),
        ] {
            if let (Some(bond), Some(direction)) = (bond, direction) {
                if self.directions[bond.0].is_some_and(|existing| existing != direction) {
                    return Err(SerializationError(
                        "Contradictory conjugated bond stereochemistry".into(),
                    ));
                }
                self.directions[bond.0] = Some(direction);
                let outer = self.graph.bond(bond).other_atom(endpoint).unwrap();
                related.extend(
                    self.graph
                        .atom(outer)
                        .bonds
                        .iter()
                        .copied()
                        .filter(|bond| self.graph.bond(*bond).stereo.is_some()),
                );
            }
        }
        Ok(related)
    }

    fn bond_text(&self, bond: BondId) -> &'static str {
        match self.graph.bond(bond).order {
            2 => "=",
            3 => "#",
            _ => match self.directions[bond.0] {
                Some(BondDirection::Slash) => "/",
                Some(BondDirection::Backslash) => "\\",
                None => "",
            },
        }
    }

    fn traverse(&mut self, root: AtomId) -> Result<(), SerializationError> {
        enum State {
            Atom(AtomId, Option<BondId>, usize),
            Open,
            Close,
        }
        let mut stack = vec![State::Atom(root, None, 0)];
        while let Some(state) = stack.pop() {
            let (atom, taken, depth) = match state {
                State::Open => {
                    self.output.push('(');
                    continue;
                }
                State::Close => {
                    self.output.push(')');
                    continue;
                }
                State::Atom(atom, taken, depth) => (atom, taken, depth),
            };
            if let Some(taken) = taken {
                self.output.push_str(self.bond_text(taken));
            }
            self.output.push_str(&self.atom_text(atom, taken, depth)?);
            self.output_order.push(atom);
            let bonds = &self.graph.atom(atom).bonds;
            let mut released = Vec::new();
            for &bond in bonds {
                if Some(bond) == taken {
                    continue;
                }
                let neighbour = self.graph.bond(bond).other_atom(atom).unwrap();
                if self.depth[neighbour.0].is_some_and(|other| other <= depth) {
                    let closure = self.closure.get(&bond).ok_or_else(|| {
                        SerializationError("Missing ring closure identifier".into())
                    })?;
                    self.output.push_str(closure);
                    released.push(closure.clone());
                }
            }
            for &bond in bonds {
                let neighbour = self.graph.bond(bond).other_atom(atom).unwrap();
                if self.depth[neighbour.0].is_some_and(|other| other > depth + 1) {
                    let closure = self.available_closures.pop_front().ok_or_else(|| {
                        SerializationError("More than 100 simultaneous ring closures".into())
                    })?;
                    self.output.push_str(self.bond_text(bond));
                    self.output.push_str(&closure);
                    self.closure.insert(bond, closure);
                }
            }
            for closure in released.into_iter().rev() {
                self.available_closures.push_front(closure);
            }
            let mut first = true;
            for &bond in bonds.iter().rev() {
                let neighbour = self.graph.bond(bond).other_atom(atom).unwrap();
                if self.depth[neighbour.0] == Some(depth + 1) {
                    if first {
                        stack.push(State::Atom(neighbour, Some(bond), depth + 1));
                        first = false;
                    } else {
                        stack.push(State::Close);
                        stack.push(State::Atom(neighbour, Some(bond), depth + 1));
                        stack.push(State::Open);
                    }
                }
            }
        }
        Ok(())
    }

    fn atom_text(
        &self,
        atom: AtomId,
        taken: Option<BondId>,
        depth: usize,
    ) -> Result<String, SerializationError> {
        let a = self.graph.atom(atom);
        let hydrogen = self
            .graph
            .neighbours(atom)
            .iter()
            .filter(|atom| self.depth[atom.0].is_none())
            .count() as i32
            + a.explicit_hydrogens as i32;
        let expected: Option<&[i32]> = match a.element {
            Element::B => Some(&[3]),
            Element::C => Some(&[4]),
            Element::N | Element::P => Some(&[3, 5]),
            Element::O => Some(&[2]),
            Element::S => Some(&[2, 4, 6]),
            Element::F | Element::Cl | Element::Br | Element::I => Some(&[1]),
            Element::R => Some(&[1, 2, 3, 4, 5, 6, 7, 8, 9]),
            _ => None,
        };
        let valency = self.graph.incoming_valency(atom);
        let brackets = if let Some(expected) = expected {
            let described = expected.contains(&valency) || valency > *expected.last().unwrap();
            let implicit = expected
                .iter()
                .copied()
                .find(|expected| *expected >= valency - hydrogen)
                .unwrap_or(valency - hydrogen);
            a.charge != 0
                || a.isotope.is_some()
                || a.parity.is_some()
                || !described
                || valency != implicit
                || a.properties.atom_class.is_some()
        } else {
            true
        };
        let mut text = String::new();
        if brackets {
            text.push('[');
        }
        if let Some(isotope) = a.isotope {
            text.push_str(&isotope.to_string());
        }
        text.push_str(if a.element == Element::R {
            "*"
        } else {
            a.element.symbol()
        });
        if a.parity.is_some() {
            text.push_str(self.parity_text(atom, taken, depth)?);
        }
        if hydrogen != 0 && brackets && a.element != Element::H {
            text.push('H');
            if hydrogen != 1 {
                text.push_str(&hydrogen.to_string());
            }
        }
        match a.charge {
            0 => {}
            1 => text.push('+'),
            -1 => text.push('-'),
            charge => {
                if charge > 0 {
                    text.push('+');
                }
                text.push_str(&charge.to_string());
            }
        }
        if brackets {
            if let Some(class) = a.properties.atom_class {
                text.push(':');
                text.push_str(&class.to_string());
            }
            text.push(']');
        }
        Ok(text)
    }

    fn parity_text(
        &self,
        atom: AtomId,
        taken: Option<BondId>,
        depth: usize,
    ) -> Result<&'static str, SerializationError> {
        let parity = self.graph.atom(atom).parity.as_ref().unwrap();
        if !matches!(parity.parity, -1 | 1) {
            return Err(SerializationError(
                "Tetrahedral parity must be +1 or -1".into(),
            ));
        }
        let mut current = Vec::new();
        if let Some(taken) = taken {
            current.push(self.graph.bond(taken).other_atom(atom).unwrap());
        }
        if parity
            .atom_refs
            .contains(&Some(StereoReference::Atom(atom)))
        {
            current.push(atom);
        }
        for &bond in &self.graph.atom(atom).bonds {
            if self.depth[self.graph.bond(bond).other_atom(atom).unwrap().0].is_none() {
                current.push(atom);
            }
        }
        for _ in 0..self.graph.atom(atom).explicit_hydrogens {
            current.push(atom);
        }
        for selector in 0..3 {
            for &bond in &self.graph.atom(atom).bonds {
                let neighbour = self.graph.bond(bond).other_atom(atom).unwrap();
                let Some(other_depth) = self.depth[neighbour.0] else {
                    continue;
                };
                let selected = match selector {
                    0 => Some(bond) != taken && other_depth <= depth,
                    1 => other_depth > depth + 1,
                    _ => other_depth == depth + 1,
                };
                if selected {
                    current.push(neighbour);
                }
            }
        }
        let mut stored = Vec::new();
        for reference in parity.atom_refs {
            match reference {
                Some(StereoReference::Atom(id)) => {
                    stored.push(if self.depth.get(id.0).copied().flatten().is_none() {
                        atom
                    } else {
                        id
                    })
                }
                Some(StereoReference::ImplicitHydrogen) => stored.push(atom),
                Some(StereoReference::DeoxyHydrogen | StereoReference::RingOpening) | None => {
                    return Err(SerializationError(
                        "Unresolved stereo placeholder in semantic export".into(),
                    ));
                }
            }
        }
        if current.len() != 4 {
            return Err(SerializationError(
                "Tetrahedral centre has incomplete output references".into(),
            ));
        }
        let mut permutation = Vec::new();
        let mut used = [false; 4];
        for reference in stored {
            let position = current
                .iter()
                .enumerate()
                .position(|(index, value)| !used[index] && *value == reference)
                .ok_or_else(|| {
                    SerializationError("Tetrahedral references do not match atom neighbours".into())
                })?;
            used[position] = true;
            permutation.push(position);
        }
        let inversions = (0..4)
            .flat_map(|i| (i + 1..4).map(move |j| (i, j)))
            .filter(|(i, j)| permutation[*i] > permutation[*j])
            .count();
        let equivalent = parity.parity * if inversions % 2 == 0 { 1 } else { -1 } == 1;
        Ok(if equivalent { "@@" } else { "@" })
    }

    fn extended_layer(&mut self) -> Result<(), SerializationError> {
        let fragment = self.graph.fragment(self.fragment);
        let polymer = fragment
            .polymer_attachment_points
            .as_ref()
            .is_some_and(|points| !points.is_empty());
        let indices: HashMap<_, _> = self
            .output_order
            .iter()
            .enumerate()
            .map(|(index, atom)| (*atom, index))
            .collect();
        let mut labels = Vec::new();
        let mut last_label = None;
        let mut positions = Vec::new();
        let mut stereo: HashMap<_, Vec<usize>> = HashMap::new();
        let mut seen_classes = std::collections::HashSet::new();
        let mut next_class = 1;
        for (index, atom) in self.output_order.iter().copied().enumerate() {
            let a = self.graph.atom(atom);
            let label = if let Some(group) = &a.properties.homology_group {
                let label = escape_label(group);
                if label.starts_with('_') {
                    label
                } else {
                    format!("{label}_p")
                }
            } else if a.element == Element::R {
                if polymer {
                    "star_e".into()
                } else {
                    let class = if let Some(class) = a.properties.atom_class {
                        seen_classes.insert(class);
                        class
                    } else {
                        while seen_classes.contains(&next_class) {
                            next_class += 1;
                        }
                        let class = next_class;
                        next_class += 1;
                        class
                    };
                    format!("_AP{class}")
                }
            } else {
                String::new()
            };
            if !label.is_empty() {
                last_label = Some(index);
            }
            labels.push(label);
            if let Some(references) = &a.properties.position_variation_bond {
                let mut referenced_indices = Vec::new();
                for reference in references {
                    referenced_indices.push(
                        indices
                            .get(reference)
                            .ok_or_else(|| {
                                SerializationError(
                                    "Unresolved position variation bond reference".into(),
                                )
                            })?
                            .to_string(),
                    );
                }
                positions.push(format!("{index}:{}", referenced_indices.join(".")));
            }
            if let Some(parity) = &a.parity
                && parity.stereo_group.kind != StereoGroupType::Unknown
            {
                stereo.entry(parity.stereo_group).or_default().push(index);
            }
        }
        let mut layers = Vec::new();
        if let Some(last_label) = last_label {
            layers.push(format!("${}$", labels[..=last_label].join(";")));
        }
        // Semantic CXSMILES deliberately excludes OPSIN's optional atom-value
        // locant annotation: this API uses flags 1|4|8 rather than all flags.
        if stereo.len() == 1 {
            let (group, atoms) = stereo.iter().next().unwrap();
            match (group.kind, group.number) {
                (StereoGroupType::Racemic, 1 | 2) => layers.push("r".into()),
                (StereoGroupType::Relative, 1) => {
                    layers.push(format!("o1:{}", join_indices(atoms)))
                }
                (StereoGroupType::Absolute, _) => {}
                _ => {
                    return Err(SerializationError(
                        "Single enhanced stereo group number unsupported by OPSIN 2.9.0 export"
                            .into(),
                    ));
                }
            }
        } else {
            let mut groups: Vec<_> = stereo.into_iter().collect();
            groups.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let (mut relative, mut racemic) = (1, 1);
            for (group, atoms) in groups {
                match group.kind {
                    StereoGroupType::Relative => {
                        layers.push(format!("o{relative}:{}", join_indices(&atoms)));
                        relative += 1;
                    }
                    StereoGroupType::Racemic => {
                        layers.push(format!("&{racemic}:{}", join_indices(&atoms)));
                        racemic += 1;
                    }
                    _ => {}
                }
            }
        }
        if !positions.is_empty() {
            layers.push(format!("m:{}", positions.join(",")));
        }
        if polymer {
            let atoms: Vec<_> = self
                .output_order
                .iter()
                .enumerate()
                .filter_map(|(index, atom)| {
                    (self.graph.atom(*atom).element != Element::R).then_some(index)
                })
                .collect();
            layers.push(format!("Sg:n:{}::ht", join_indices(&atoms)));
        }
        if !layers.is_empty() {
            self.output.push_str(" |");
            self.output.push_str(&layers.join(","));
            self.output.push('|');
        }
        Ok(())
    }
}

fn join_indices(indices: &[usize]) -> String {
    indices
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
fn escape_label(label: &str) -> String {
    // Java's original escape routine operates on UTF-16 code units.
    label
        .encode_utf16()
        .map(|unit| {
            if unit <= 127 && (unit as u8).is_ascii_alphanumeric() {
                (unit as u8 as char).to_string()
            } else {
                format!("&#{unit};")
            }
        })
        .collect()
}
