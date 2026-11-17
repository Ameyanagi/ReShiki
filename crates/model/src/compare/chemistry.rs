//! One molecule as chemistry, for comparing: its canonical isomeric SMILES
//! with drawn hydrogens folded in, plus the stereo the plain SMILES leaves
//! out (unknown stereo, AND/OR groups). Drawing choices are not chemistry:
//! aromatic or Kekulé bonds, which bond carries the wedge, a collapsed label
//! and computed labels all give the same signature. What cannot be read as
//! concrete chemistry gives the reason instead, never a match.
use crate::chemistry::{
    ELEMENTS, document,
    kekulize::Direction,
    smiles::{self, write},
    stereo::perception::State,
};
use crate::document::Document;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) struct Signature {
    /// Canonical isomeric SMILES, drawn hydrogens folded in.
    pub smiles: String,
    /// Stereo beyond the plain SMILES, in canonical atom order.
    pub extra: String,
    /// Canonical SMILES as drawn, hydrogens included: equal text means
    /// `order` pairs atoms one to one.
    pub drawn: String,
    /// Drawing atom ids in canonical order.
    pub order: Vec<u64>,
    pub atoms: HashMap<u64, Facts>,
    pub bonds: HashMap<(u64, u64), u8>,
    /// Stereocenters and stereo double bonds, by drawing ids.
    pub stereo: Vec<u64>,
    /// Each tetrahedral centre's neighbours, sorted, and its winding about
    /// them in that order, so the two sides compare id for id.
    pub chirality: HashMap<u64, (Vec<u64>, u8)>,
    /// Each stereo double bond's code, with its reference atoms for cis and
    /// trans.
    pub double: HashMap<(u64, u64), (u8, Vec<u64>)>,
    /// Each AND/OR group member's kind and fellow members, by id.
    pub grouped: HashMap<u64, (u8, Vec<u64>)>,
}

impl Signature {
    pub fn same(&self, other: &Self) -> bool {
        self.smiles == other.smiles && self.extra == other.extra
    }
}

/// What an atom is, as chemistry: element, isotope, charge, hydrogens and
/// unpaired electrons after sanitization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Facts {
    pub element: u8,
    pub isotope: u16,
    pub charge: i8,
    pub hydrogens: u32,
    pub radicals: u8,
    pub singlet: bool,
}

pub(super) fn key(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

/// The atoms `ids` of `doc`, with the bonds between them, as a drawing of
/// their own.
fn part(doc: &Document, ids: &HashSet<u64>) -> Document {
    Document {
        version: doc.version,
        atoms: doc
            .atoms
            .iter()
            .filter(|atom| ids.contains(&atom.id))
            .cloned()
            .collect(),
        bonds: doc
            .bonds
            .iter()
            .filter(|bond| ids.contains(&bond.a) && ids.contains(&bond.b))
            .cloned()
            .collect(),
        ..Default::default()
    }
}

pub(super) fn signature(doc: &Document, ids: &[u64]) -> Result<Signature, String> {
    if !doc.generic.is_empty() {
        return Err("Generic structures (R-groups, alternatives, repeats)".into());
    }
    let ids: HashSet<u64> = ids.iter().copied().collect();
    // A wildcard or a free label is not concrete chemistry.
    if doc
        .atoms
        .iter()
        .filter(|atom| ids.contains(&atom.id))
        .any(|atom| atom.element == "*" || atom.display.variable.is_some())
    {
        return Err("Unresolved labels such as R1".into());
    }
    // A stereo group shared with another molecule ties them together.
    let groups: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|atom| ids.contains(&atom.id))
        .filter_map(|atom| atom.stereo_group)
        .collect();
    if doc.atoms.iter().any(|atom| {
        !ids.contains(&atom.id) && atom.stereo_group.is_some_and(|g| groups.contains(&g))
    }) {
        return Err("A stereo group shared with another molecule".into());
    }
    let molecule = document::prepare(&part(doc, &ids)).map_err(|error| error.to_string())?;
    let options = write::Options {
        ignore_maps: true,
        ..Default::default()
    };
    let drawn = write::write(&molecule.state, options).map_err(|error| error.to_string())?;
    let id = |index: usize| molecule.ids.get(index).copied();
    let order: Vec<u64> = drawn.atom_order.iter().filter_map(|&i| id(i)).collect();
    let singlet: Vec<bool> = molecule
        .ids
        .iter()
        .map(|id| doc.atom(*id).is_some_and(|atom| atom.singlet))
        .collect();
    let extra = extra(&molecule.state, &drawn.atom_order, &singlet)?;
    // Reading the SMILES back folds drawn hydrogens into their atoms.
    let smiles = smiles::prepare(&drawn.text)
        .ok()
        .and_then(|folded| write::write(&folded.state, options).ok())
        .map_or_else(|| drawn.text.clone(), |written| written.text);
    let state = &molecule.state;
    let atoms = state
        .graph
        .atoms
        .iter()
        .zip(&state.valences)
        .enumerate()
        .filter_map(|(i, (atom, valence))| {
            Some((
                id(i)?,
                Facts {
                    element: atom.atomic_number,
                    isotope: atom.isotope,
                    charge: atom.charge,
                    hydrogens: u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens,
                    radicals: atom.radical_electrons,
                    singlet: singlet.get(i) == Some(&true),
                },
            ))
        })
        .collect();
    let bonds = state
        .graph
        .bonds
        .iter()
        .filter_map(|bond| {
            let order = if bond.aromatic { 4 } else { bond.order };
            Some((key(id(bond.a)?, id(bond.b)?), order))
        })
        .collect();
    let mut stereo: Vec<u64> = state
        .metadata
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, meta)| meta.chiral_tag != 0)
        .filter_map(|(i, _)| id(i))
        .collect();
    let mut double = HashMap::new();
    for (bond, meta) in state.graph.bonds.iter().zip(&state.metadata.bonds) {
        if meta.stereo != 0 || meta.unknown_stereo {
            stereo.extend([id(bond.a), id(bond.b)].into_iter().flatten());
            if let (Some(a), Some(b)) = (id(bond.a), id(bond.b)) {
                let code = if meta.unknown_stereo { 1 } else { meta.stereo };
                let atoms = match code {
                    4 | 5 => meta.stereo_atoms.iter().filter_map(|&i| id(i)).collect(),
                    _ => Vec::new(),
                };
                double.insert(key(a, b), (code, atoms));
            }
        }
    }
    Ok(Signature {
        smiles,
        extra,
        drawn: drawn.text,
        order,
        atoms,
        bonds,
        stereo,
        chirality: chirality(state, &molecule.ids),
        double,
        grouped: state
            .metadata
            .groups
            .iter()
            .filter(|group| group.kind != 0)
            .flat_map(|group| {
                let mut members: Vec<u64> = group.atoms.iter().filter_map(|&i| id(i)).collect();
                members.sort_unstable();
                members
                    .clone()
                    .into_iter()
                    .map(move |member| (member, (group.kind, members.clone())))
            })
            .collect(),
    })
}

/// Tetrahedral centres keyed by drawing id. A tag winds about the atom's
/// neighbours in bond order; sorting them flips it once per odd swap.
fn chirality(state: &State, ids: &[u64]) -> HashMap<u64, (Vec<u64>, u8)> {
    let mut neighbors = vec![Vec::new(); state.graph.atoms.len()];
    for bond in &state.graph.bonds {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            if let (Some(list), Some(id)) = (neighbors.get_mut(a), ids.get(b)) {
                list.push(*id);
            }
        }
    }
    let mut found = HashMap::new();
    for (i, meta) in state.metadata.atoms.iter().enumerate() {
        let (Some(id), Some(order)) = (ids.get(i), neighbors.get(i)) else {
            continue;
        };
        if !matches!(meta.chiral_tag, 1 | 2) {
            continue;
        }
        let mut sorted = order.clone();
        sorted.sort_unstable();
        // Parity of the permutation, by counting inversions.
        let odd = (0..order.len())
            .flat_map(|x| (x + 1..order.len()).map(move |y| (x, y)))
            .filter(|&(x, y)| order.get(x) > order.get(y))
            .count()
            % 2
            == 1;
        let tag = if odd {
            3 - meta.chiral_tag
        } else {
            meta.chiral_tag
        };
        found.insert(*id, (sorted, tag));
    }
    found
}

/// What plain SMILES drops, by position in the canonical atom order:
/// unknown stereo, AND/OR groups (by membership, not by their numbers),
/// unpaired electrons and singlet spin. Axial stereo is not compared.
fn extra(state: &State, atom_order: &[usize], singlet: &[bool]) -> Result<String, String> {
    if state.metadata.bonds.iter().any(|meta| meta.stereo >= 6)
        || state
            .metadata
            .groups
            .iter()
            .any(|group| !group.bonds.is_empty())
    {
        return Err("Axial stereo (atropisomers)".into());
    }
    let position: HashMap<usize, usize> = atom_order
        .iter()
        .enumerate()
        .map(|(position, &atom)| (atom, position))
        .collect();
    let at = |atom: usize| position.get(&atom).copied();
    let mut parts: Vec<String> = Vec::new();
    for ((bond, direction), meta) in state
        .graph
        .bonds
        .iter()
        .zip(&state.directions)
        .zip(&state.metadata.bonds)
    {
        let (Some(a), Some(b)) = (at(bond.a), at(bond.b)) else {
            continue;
        };
        if bond.order == 2 || matches!(direction, Direction::EitherDouble) {
            if matches!(direction, Direction::Unknown | Direction::EitherDouble)
                || meta.stereo == 1
                || meta.unknown_stereo
            {
                parts.push(format!("w{}-{}", a.min(b), a.max(b)));
            }
        } else if matches!(direction, Direction::Unknown) || meta.unknown_stereo {
            // A wavy single bond: its first atom's configuration is unknown.
            parts.push(format!("u{a}"));
        }
    }
    let mut groups: Vec<String> = state
        .metadata
        .groups
        .iter()
        .filter(|group| group.kind != 0)
        .map(|group| {
            let mut atoms: Vec<usize> = group.atoms.iter().filter_map(|&atom| at(atom)).collect();
            atoms.sort_unstable();
            let kind = if group.kind == 2 { "&" } else { "o" };
            let atoms: Vec<String> = atoms.iter().map(usize::to_string).collect();
            format!("{kind}{}", atoms.join(","))
        })
        .collect();
    groups.sort_unstable();
    parts.extend(groups);
    for (i, atom) in state.graph.atoms.iter().enumerate() {
        let Some(p) = at(i) else {
            continue;
        };
        if atom.radical_electrons > 0 {
            parts.push(format!("r{p}:{}", atom.radical_electrons));
        }
        if singlet.get(i) == Some(&true) {
            parts.push(format!("s{p}"));
        }
    }
    parts.sort_unstable();
    parts.dedup();
    Ok(parts.join(" "))
}

/// What changed between two versions of one molecule, by drawing id.
#[derive(Debug, Default)]
pub(super) struct Edit {
    /// Atoms only on the left, only on the right, and changed on both.
    pub removed: Vec<u64>,
    pub added: Vec<u64>,
    pub changed: Vec<u64>,
    pub removed_bonds: Vec<(u64, u64)>,
    pub added_bonds: Vec<(u64, u64)>,
    pub changed_bonds: Vec<(u64, u64)>,
    /// Nothing above explains it, so the stereo changed.
    pub stereo: bool,
}

pub(super) fn edit(left: &Signature, right: &Signature) -> Edit {
    let mut edit = Edit::default();
    for (id, facts) in &left.atoms {
        match right.atoms.get(id) {
            None => edit.removed.push(*id),
            Some(theirs) if theirs != facts => edit.changed.push(*id),
            Some(_) => {}
        }
    }
    edit.added = right
        .atoms
        .keys()
        .filter(|id| !left.atoms.contains_key(id))
        .copied()
        .collect();
    for (bond, order) in &left.bonds {
        match right.bonds.get(bond) {
            None => edit.removed_bonds.push(*bond),
            Some(theirs) if theirs != order => edit.changed_bonds.push(*bond),
            Some(_) => {}
        }
    }
    edit.added_bonds = right
        .bonds
        .keys()
        .filter(|bond| !left.bonds.contains_key(bond))
        .copied()
        .collect();
    let nothing = edit.removed.is_empty()
        && edit.added.is_empty()
        && edit.changed.is_empty()
        && edit.removed_bonds.is_empty()
        && edit.added_bonds.is_empty()
        && edit.changed_bonds.is_empty();
    // Centres whose configuration changed, alongside any other edit.
    let centres: Vec<u64> = left
        .chirality
        .keys()
        .chain(right.chirality.keys())
        .filter(|id| left.chirality.get(*id) != right.chirality.get(*id))
        .copied()
        .collect();
    let doubles: Vec<(u64, u64)> = left
        .double
        .keys()
        .chain(right.double.keys())
        .filter(|bond| left.double.get(*bond) != right.double.get(*bond))
        .copied()
        .collect();
    let grouped: Vec<u64> = left
        .grouped
        .keys()
        .chain(right.grouped.keys())
        .filter(|atom| left.grouped.get(*atom) != right.grouped.get(*atom))
        .copied()
        .collect();
    if nothing {
        edit.stereo = true;
        if centres.is_empty() && doubles.is_empty() && grouped.is_empty() {
            // Groups and the like: point at every stereo unit.
            edit.changed = left.stereo.iter().chain(&right.stereo).copied().collect();
        }
    }
    edit.changed.extend(centres);
    edit.changed.extend(grouped);
    for (a, b) in doubles {
        edit.changed.extend([a, b]);
        edit.changed_bonds.push((a, b));
    }
    edit.changed.sort_unstable();
    edit.changed.dedup();
    for list in [&mut edit.removed, &mut edit.added, &mut edit.changed] {
        list.sort_unstable();
    }
    for list in [
        &mut edit.removed_bonds,
        &mut edit.added_bonds,
        &mut edit.changed_bonds,
    ] {
        list.sort_unstable();
        list.dedup();
    }
    edit
}

/// Heavy atoms of `ids` as a formula, carbon first then by symbol, such as
/// `NO₂`. Empty for none.
pub(super) fn formula(signature: &Signature, ids: &[u64]) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut carbons = 0;
    for facts in ids.iter().filter_map(|id| signature.atoms.get(id)) {
        let Some(element) = ELEMENTS.get(usize::from(facts.element)) else {
            continue;
        };
        match element.symbol {
            "H" => {}
            "C" => carbons += 1,
            symbol => *counts.entry(symbol).or_default() += 1,
        }
    }
    let mut text = String::new();
    let mut push = |symbol: &str, count: usize| {
        if count == 0 {
            return;
        }
        text.push_str(symbol);
        if count > 1 {
            text.extend(count.to_string().chars().map(subscript));
        }
    };
    push("C", carbons);
    for (symbol, count) in counts {
        push(symbol, count);
    }
    text
}

fn subscript(digit: char) -> char {
    digit
        .to_digit(10)
        .and_then(|d| char::from_u32(0x2080 + d))
        .unwrap_or(digit)
}
