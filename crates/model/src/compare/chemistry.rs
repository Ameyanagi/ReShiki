//! One molecule as chemistry, for comparing: its canonical isomeric SMILES
//! with drawn hydrogens folded in, plus the stereo the plain SMILES leaves
//! out (unknown stereo, AND/OR groups). Drawing choices are not chemistry:
//! aromatic or Kekulé bonds, which bond carries the wedge, a collapsed label
//! and computed labels all give the same signature. What cannot be read as
//! concrete chemistry gives the reason instead, never a match.
use super::{Part, PartKind};
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

/// What changed between two versions of one molecule.
#[derive(Debug, Default)]
pub(super) struct Edit {
    /// Atoms only on the left, only on the right, and changed on both as
    /// (left, right).
    pub removed: Vec<u64>,
    pub added: Vec<u64>,
    pub changed: Vec<(u64, u64)>,
    pub removed_bonds: Vec<(u64, u64)>,
    pub added_bonds: Vec<(u64, u64)>,
    pub changed_bonds: Vec<((u64, u64), (u64, u64))>,
    /// Centres whose configuration changed, and whether it is inverted:
    /// the same neighbours, the other way round.
    pub centres: Vec<((u64, u64), bool)>,
    /// Nothing above explains it, so the stereo changed.
    pub stereo: bool,
    /// Only stereo changed, and every centre is inverted: the mirror image.
    pub mirror_image: bool,
}

/// A right-side centre in left ids: its neighbours carried over and sorted,
/// the winding flipped once per odd reordering. `None` if one is not paired.
fn carried(
    (neighbors, tag): &(Vec<u64>, u8),
    to_left: impl Fn(u64) -> Option<u64>,
) -> Option<(Vec<u64>, u8)> {
    let mut ids: Vec<u64> = neighbors
        .iter()
        .map(|id| to_left(*id))
        .collect::<Option<_>>()?;
    let odd = (0..ids.len())
        .flat_map(|x| (x + 1..ids.len()).map(move |y| (x, y)))
        .filter(|&(x, y)| ids[x] > ids[y])
        .count()
        % 2
        == 1;
    ids.sort_unstable();
    Some((ids, if odd { 3 - tag } else { *tag }))
}

/// What changed between two versions of one molecule, its atoms paired by
/// `map`, left id to right id; an atom it leaves out is on one side only.
/// Molecules matched by id pair each atom with itself.
pub(super) fn edit_mapped(left: &Signature, right: &Signature, map: &HashMap<u64, u64>) -> Edit {
    let back: HashMap<u64, u64> = map.iter().map(|(l, r)| (*r, *l)).collect();
    let to_right = |id: u64| map.get(&id).copied();
    let to_left = |id: u64| back.get(&id).copied();
    let bond_right = |(a, b): (u64, u64)| Some(key(to_right(a)?, to_right(b)?));
    let bond_left = |(a, b): (u64, u64)| Some(key(to_left(a)?, to_left(b)?));
    let mut edit = Edit::default();
    let mut kept = Vec::new();
    for (id, facts) in &left.atoms {
        match to_right(*id).and_then(|r| Some((r, right.atoms.get(&r)?))) {
            None => edit.removed.push(*id),
            Some((r, theirs)) => {
                kept.push((*id, r));
                if theirs != facts {
                    edit.changed.push((*id, r));
                }
            }
        }
    }
    edit.added = (right.atoms.keys())
        .filter(|id| to_left(**id).is_none_or(|l| !left.atoms.contains_key(&l)))
        .copied()
        .collect();
    for (bond, order) in &left.bonds {
        match bond_right(*bond).and_then(|r| Some((r, right.bonds.get(&r)?))) {
            None => edit.removed_bonds.push(*bond),
            Some((r, theirs)) if theirs != order => edit.changed_bonds.push((*bond, r)),
            Some(_) => {}
        }
    }
    edit.added_bonds = (right.bonds.keys())
        .filter(|bond| bond_left(**bond).is_none_or(|l| !left.bonds.contains_key(&l)))
        .copied()
        .collect();
    let nothing = edit.removed.is_empty()
        && edit.added.is_empty()
        && edit.changed.is_empty()
        && edit.removed_bonds.is_empty()
        && edit.added_bonds.is_empty()
        && edit.changed_bonds.is_empty();
    // Centres whose configuration changed, alongside any other edit.
    for &(l, r) in &kept {
        let mine = left.chirality.get(&l);
        let theirs = right
            .chirality
            .get(&r)
            .map(|centre| carried(centre, to_left));
        if mine.cloned() != theirs.clone().flatten() {
            let inverted = matches!(
                (mine, &theirs),
                (Some((a, x)), Some(Some((b, y)))) if a == b && x != y
            );
            edit.centres.push(((l, r), inverted));
        }
    }
    edit.centres.sort_unstable();
    // Reference atoms sorted: cis or trans reads the same either way round.
    let geometry = |(code, refs): &(u8, Vec<u64>), to: &dyn Fn(u64) -> Option<u64>| {
        let mut refs: Vec<Option<u64>> = refs.iter().map(|id| to(*id)).collect();
        refs.sort_unstable();
        (*code, refs)
    };
    let mut doubles: Vec<((u64, u64), (u64, u64))> = Vec::new();
    // A stereo double bond gone or new: its kept ends are marked.
    for &(a, b) in left
        .double
        .keys()
        .filter(|bond| bond_right(**bond).is_none())
    {
        edit.changed
            .extend([a, b].iter().filter_map(|l| Some((*l, to_right(*l)?))));
    }
    for &(a, b) in right
        .double
        .keys()
        .filter(|bond| bond_left(**bond).is_none())
    {
        edit.changed
            .extend([a, b].iter().filter_map(|r| Some((to_left(*r)?, *r))));
    }
    for (bond, _) in &left.bonds {
        let Some(r) = bond_right(*bond) else {
            continue;
        };
        let mine = left.double.get(bond).map(|d| geometry(d, &|id| Some(id)));
        let theirs = right.double.get(&r).map(|d| geometry(d, &to_left));
        if mine != theirs {
            doubles.push((*bond, r));
        }
    }
    let grouped: Vec<(u64, u64)> = kept
        .iter()
        .filter(|(l, r)| {
            let theirs = right.grouped.get(r).map(|(kind, members)| {
                let mut members: Vec<Option<u64>> = members.iter().map(|m| to_left(*m)).collect();
                members.sort_unstable();
                (*kind, members)
            });
            let mine = (left.grouped.get(l)).map(|(kind, members)| {
                (*kind, members.iter().map(|m| Some(*m)).collect::<Vec<_>>())
            });
            mine != theirs
        })
        .copied()
        .collect();
    if nothing {
        edit.stereo = true;
        edit.mirror_image = !edit.centres.is_empty()
            && edit.centres.iter().all(|(_, inverted)| *inverted)
            && edit.centres.len() == left.chirality.len()
            && edit.centres.len() == right.chirality.len()
            && doubles.is_empty()
            && grouped.is_empty()
            && left.extra == right.extra;
        if edit.centres.is_empty() && doubles.is_empty() && grouped.is_empty() {
            // Groups and the like: point at every stereo unit.
            edit.changed
                .extend(left.stereo.iter().filter_map(|l| Some((*l, to_right(*l)?))));
            edit.changed
                .extend(right.stereo.iter().filter_map(|r| Some((to_left(*r)?, *r))));
        }
    }
    edit.changed
        .extend(edit.centres.iter().map(|(pair, _)| *pair));
    edit.changed.extend(grouped);
    for ((a, b), (c, d)) in doubles {
        // Each end with its partner.
        let ends = if to_right(a) == Some(c) {
            [(a, c), (b, d)]
        } else {
            [(a, d), (b, c)]
        };
        edit.changed.extend(ends);
        edit.changed_bonds.push(((a, b), (c, d)));
    }
    edit.removed.sort_unstable();
    edit.added.sort_unstable();
    edit.changed.sort_unstable();
    edit.changed.dedup();
    for list in [&mut edit.removed_bonds, &mut edit.added_bonds] {
        list.sort_unstable();
        list.dedup();
    }
    edit.changed_bonds.sort_unstable();
    edit.changed_bonds.dedup();
    edit
}

/// The edit in parts to word it: groups added, removed or put in another's
/// place, atoms and bonds changed in place, centres inverted. `at` names an
/// atom by its left and right ids, either absent. Wording is not paint: an
/// atom that only gained or lost the hydrogen a group or a bond order took
/// is painted but not worded.
pub(super) fn parts(
    left: &Signature,
    right: &Signature,
    edit: &Edit,
    map: &HashMap<u64, u64>,
    at: impl Fn(Option<u64>, Option<u64>) -> Option<String>,
) -> Vec<Part> {
    let back: HashMap<u64, u64> = map.iter().map(|(l, r)| (*r, *l)).collect();
    let both = |l: Option<u64>, r: Option<u64>| {
        at(
            l.or_else(|| back.get(&r?).copied()),
            r.or_else(|| map.get(&l?).copied()),
        )
    };
    // A hydrogen drawn or no longer drawn is a drawing choice, not a group:
    // its atom is only marked as attached.
    let protium = |signature: &Signature, atoms: &[u64]| {
        atoms.iter().all(|id| {
            (signature.atoms.get(id)).is_some_and(|facts| facts.element == 1 && facts.isotope == 0)
        })
    };
    let (removed, drawn_l): (Vec<_>, Vec<_>) = fragments(left, &edit.removed)
        .into_iter()
        .partition(|(atoms, ..)| !protium(left, atoms));
    let (added, drawn_r): (Vec<_>, Vec<_>) = fragments(right, &edit.added)
        .into_iter()
        .partition(|(atoms, ..)| !protium(right, atoms));
    let in_left = |r: &u64| back.get(r).copied();
    let mut attached: HashSet<u64> = (removed.iter().chain(&drawn_l))
        .flat_map(|f| f.1.iter().copied())
        .collect();
    attached.extend((added.iter().chain(&drawn_r)).flat_map(|f| f.1.iter().filter_map(in_left)));
    // A bond's new order takes hydrogens from its ends, as a group does.
    attached.extend(edit.changed_bonds.iter().flat_map(|((a, b), _)| [*a, *b]));
    let place = |attachments: &[u64]| -> Vec<String> {
        attachments
            .iter()
            .filter_map(|l| both(Some(*l), None))
            .collect()
    };
    let mut parts = Vec::new();
    let mut used = vec![false; added.len()];
    for (atoms, attachments, first) in &removed {
        let formula = fragment_formula(left, atoms, *first);
        let into = added.iter().enumerate().position(|(n, (_, theirs, _))| {
            !used[n] && !attachments.is_empty() && {
                let mut theirs: Vec<u64> = theirs.iter().filter_map(in_left).collect();
                theirs.sort_unstable();
                theirs == *attachments
            }
        });
        let (kind, what) = match into {
            Some(n) => {
                used[n] = true;
                let (atoms, _, first) = &added[n];
                (
                    PartKind::Replaced,
                    format!("{formula} → {}", fragment_formula(right, atoms, *first)),
                )
            }
            None => (PartKind::Removed, formula),
        };
        parts.push(Part {
            kind,
            what,
            at: place(attachments),
        });
    }
    for ((atoms, attachments, first), _) in added.iter().zip(&used).filter(|(_, used)| !**used) {
        let attachments: Vec<u64> = attachments.iter().filter_map(in_left).collect();
        parts.push(Part {
            kind: PartKind::Added,
            what: fragment_formula(right, atoms, *first),
            at: place(&attachments),
        });
    }
    for &(l, r) in &edit.changed {
        let (Some(a), Some(b)) = (left.atoms.get(&l), right.atoms.get(&r)) else {
            continue;
        };
        let hydrogens_only = Facts {
            hydrogens: b.hydrogens,
            ..*a
        } == *b;
        if a == b || hydrogens_only && attached.contains(&l) {
            continue;
        }
        let what = format!("{} → {}", atom_text(a), atom_text(b));
        // A terminal atom given another element, as the atom tool does: a
        // group replaced where it is attached.
        let neighbors: Vec<u64> = (left.bonds.keys())
            .filter_map(|&(x, y)| match (x == l, y == l) {
                (true, _) => Some(y),
                (_, true) => Some(x),
                _ => None,
            })
            .collect();
        parts.push(match neighbors[..] {
            [at] if a.element != b.element && both(Some(l), Some(r)).is_none() => Part {
                kind: PartKind::Replaced,
                what,
                at: both(Some(at), None).into_iter().collect(),
            },
            _ => Part {
                kind: PartKind::Atom,
                what,
                at: both(Some(l), Some(r)).into_iter().collect(),
            },
        });
    }
    for &((a, b), (c, d)) in &edit.changed_bonds {
        let (Some(x), Some(y)) = (left.bonds.get(&(a, b)), right.bonds.get(&(c, d))) else {
            continue;
        };
        let what = if x == y {
            "cis/trans changed".to_owned()
        } else {
            format!("{} → {}", order_name(*x), order_name(*y))
        };
        let ends: Option<Vec<String>> = [a, b].iter().map(|l| both(Some(*l), None)).collect();
        parts.push(Part {
            kind: PartKind::Bond,
            what,
            at: ends.unwrap_or_default(),
        });
    }
    for &((l, r), inverted) in &edit.centres {
        // A centre a group was added to or taken from: the group says it.
        if !inverted && attached.contains(&l) {
            continue;
        }
        parts.push(Part {
            kind: if inverted {
                PartKind::Inverted
            } else {
                PartKind::Stereo
            },
            what: String::new(),
            at: both(Some(l), Some(r)).into_iter().collect(),
        });
    }
    parts
}

/// Connected groups of `atoms` by the bonds of `signature`, each with the
/// atoms it is attached to, sorted, and its atom bonded to the first of
/// them.
fn fragments(signature: &Signature, atoms: &[u64]) -> Vec<(Vec<u64>, Vec<u64>, u64)> {
    let inside: HashSet<u64> = atoms.iter().copied().collect();
    let mut neighbors: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (a, b) in signature.bonds.keys() {
        neighbors.entry(*a).or_default().push(*b);
        neighbors.entry(*b).or_default().push(*a);
    }
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for start in atoms {
        if !seen.insert(*start) {
            continue;
        }
        let (mut group, mut stack, mut attachments) = (vec![], vec![*start], vec![]);
        while let Some(id) = stack.pop() {
            group.push(id);
            for next in neighbors.get(&id).into_iter().flatten() {
                if !inside.contains(next) {
                    attachments.push((*next, id));
                } else if seen.insert(*next) {
                    stack.push(*next);
                }
            }
        }
        group.sort_unstable();
        attachments.sort_unstable();
        let first = attachments.first().map_or(group[0], |(_, id)| *id);
        let mut attachments: Vec<u64> = attachments.into_iter().map(|(at, _)| at).collect();
        attachments.dedup();
        found.push((group, attachments, first));
    }
    found
}

/// An element with its isotope, if one is given: `C`, `¹⁸O`.
fn labelled(facts: &Facts) -> String {
    let mut text: String = (facts.isotope.to_string().chars())
        .filter(|_| facts.isotope > 0)
        .filter_map(|d| "⁰¹²³⁴⁵⁶⁷⁸⁹".chars().nth(d.to_digit(10)? as usize))
        .collect();
    text.push_str(symbol(facts));
    text
}

fn symbol(facts: &Facts) -> &'static str {
    ELEMENTS
        .get(usize::from(facts.element))
        .map_or("?", |element| element.symbol)
}

/// A group as a formula, the atom it is attached by first, then carbon,
/// then the others by symbol, then hydrogen: `OH`, `NO₂`, `CO₂H`, `OCH₃`.
pub(super) fn fragment_formula(signature: &Signature, ids: &[u64], first: u64) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut hydrogens = 0;
    for facts in ids.iter().filter_map(|id| signature.atoms.get(id)) {
        hydrogens += facts.hydrogens as usize;
        match labelled(facts) {
            label if label == "H" => hydrogens += 1,
            label => *counts.entry(label).or_default() += 1,
        }
    }
    let lead = signature
        .atoms
        .get(&first)
        .map_or_else(|| "C".into(), labelled);
    let lead = lead.as_str();
    let mut order = vec![lead];
    if lead != "C" {
        order.push("C");
    }
    order.extend(
        counts
            .keys()
            .map(String::as_str)
            .filter(|s| !["C", lead].contains(s)),
    );
    let mut text = String::new();
    for symbol in order {
        text.push_str(&counted(
            symbol,
            counts.get(symbol).copied().unwrap_or_default(),
        ));
    }
    text + &counted("H", hydrogens)
}

fn counted(symbol: &str, count: usize) -> String {
    match count {
        0 => String::new(),
        1 => symbol.to_owned(),
        _ => symbol
            .chars()
            .chain(count.to_string().chars().map(subscript))
            .collect(),
    }
}

/// An atom as chemistry: `¹³CH₃`, `NH₃⁺`, `O⁻`, `C•`, `CH₂•• (singlet)`.
fn atom_text(facts: &Facts) -> String {
    let script = |n: u32| -> String {
        n.to_string()
            .chars()
            .filter_map(|d| "⁰¹²³⁴⁵⁶⁷⁸⁹".chars().nth(d.to_digit(10)? as usize))
            .collect()
    };
    let mut text = labelled(facts);
    text.push_str(&counted("H", facts.hydrogens as usize));
    let charge = facts.charge.unsigned_abs();
    if charge > 1 {
        text.push_str(&script(u32::from(charge)));
    }
    if facts.charge != 0 {
        text.push(if facts.charge > 0 { '⁺' } else { '⁻' });
    }
    text.extend(std::iter::repeat_n('•', usize::from(facts.radicals)));
    if facts.singlet {
        text.push_str(" (singlet)");
    }
    text
}

fn order_name(order: u8) -> String {
    match order {
        1 => "single".into(),
        2 => "double".into(),
        3 => "triple".into(),
        4 => "aromatic".into(),
        other => format!("order {other}"),
    }
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
