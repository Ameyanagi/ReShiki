//! A catalytic cycle's steps as ordinary reactions. Step i's reactants are
//! intermediate i and its reagents, its products the next intermediate and
//! its products; texts are its conditions. Each reaction lives in a
//! throwaway drawing that holds only those parts, one arrow per step and the
//! reactions. Reaction tools run on that copy and what they change goes back
//! onto the same atoms, so nothing about the reactions is stored.
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, Document},
    reaction_colors::{Refusal, paint},
    reactions::{Participant, Reaction, Role},
    ring_fills::RingFill,
};
use std::collections::{BTreeMap, HashMap, HashSet};

#[cfg(test)]
mod tests;

/// Whether a part is a structure that takes part in reactions.
fn structure(doc: &Document, part: &[u64]) -> bool {
    !part.is_empty()
        && part
            .iter()
            .all(|a| doc.atom(*a).is_some_and(|a| a.purpose.is_chemistry()))
}

fn find(doc: &Document, id: u64) -> Result<&crate::cycles::Cycle, String> {
    doc.graphics
        .iter()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref())
        .ok_or_else(|| "Select a catalytic cycle first".into())
}

/// The steps of cycle `id` that `pick` keeps, as reactions in a copy of
/// their parts. Returns the copy and each step's arrow, in step order.
fn build(
    doc: &Document,
    id: u64,
    pick: impl Fn(usize) -> bool,
) -> Result<(Document, Vec<u64>), String> {
    let cycle = find(doc, id)?;
    let n = cycle.intermediates.len();
    if n < 2 {
        return Err("Add a second intermediate first".into());
    }
    let centers: Vec<_> = crate::scene::selections_bounds(doc, &cycle.intermediates)
        .into_iter()
        .map(|b| b.map(|(lo, hi)| lo.lerp(hi, 0.5)).unwrap_or_default())
        .collect();
    let mut ids = vec![];
    let mut arrows = vec![];
    let mut reactions = vec![];
    let mut next = doc.next_id();
    for (s, step) in cycle.steps.iter().enumerate().filter(|(s, _)| pick(*s)) {
        let to = (s + 1) % n;
        let (Some(first), Some(last)) = (cycle.intermediates.get(s), cycle.intermediates.get(to))
        else {
            continue;
        };
        let (start, end) = (
            centers.get(s).copied().unwrap_or_default(),
            centers.get(to).copied().unwrap_or_default(),
        );
        let arrow = Arrow::new(
            next,
            start,
            end,
            Preset::Forward,
            ArrowStyle::preset(Preset::Forward),
        );
        let mut reaction = Reaction::new(next);
        next += 1;
        for (role, parts) in [
            (Role::Reactant, std::iter::once(first).chain(&step.reagents)),
            (Role::Product, std::iter::once(last).chain(&step.products)),
        ] {
            for part in parts {
                if let [text] = part.as_slice()
                    && doc.atom(*text).is_none()
                {
                    ids.push(*text);
                    reaction.annotations.push(*text);
                } else if structure(doc, part) {
                    ids.extend(part);
                    reaction.participants_mut(role).push(Participant {
                        atoms: part.clone(),
                        coefficient: 1,
                    });
                }
                // Reference and Illustration structures take no part.
            }
        }
        arrows.push(arrow);
        reactions.push(reaction);
    }
    let mut copy = crate::editing::selection(doc, &ids);
    let ids = arrows.iter().map(|a| a.id).collect();
    copy.arrows = arrows;
    copy.reactions = reactions;
    copy.validate()?;
    Ok((copy, ids))
}

/// Every step of cycle `id` as a reaction, with the steps' arrows. A step
/// with a text intermediate is there too, with no structure on that side.
pub fn reactions(doc: &Document, id: u64) -> Result<(Document, Vec<u64>), String> {
    build(doc, id, |_| true)
}

/// Step `s` of cycle `id` alone as a reaction, with its arrow. Both its
/// intermediates must be structures.
pub fn step(doc: &Document, id: u64, s: usize) -> Result<(Document, u64), String> {
    let intermediates = &find(doc, id)?.intermediates;
    let n = intermediates.len().max(1);
    if ![s, (s + 1) % n]
        .iter()
        .all(|i| intermediates.get(*i).is_some_and(|p| structure(doc, p)))
    {
        return Err(format!("Step {} needs a structure on each side", s + 1));
    }
    let (copy, arrows) = build(doc, id, |i| i == s)?;
    match arrows.as_slice() {
        [arrow] => Ok((copy, *arrow)),
        _ => Err("That step is no longer in the cycle".into()),
    }
}

/// The atoms of a copy's reaction participants.
fn participants(from: &Document) -> HashSet<u64> {
    from.reactions
        .iter()
        .flat_map(|r| [Role::Reactant, Role::Product].map(|role| r.participants(role)))
        .flatten()
        .flat_map(|p| p.atoms.iter().copied())
        .collect()
}

/// Put the atom maps of a copy's reaction participants back onto the same
/// atoms. As for any reaction, atoms inside an abbreviation such as PPh3 get
/// maps too, which expands it; leaving them out would let a later step pair
/// a mapped atom with an unmapped hidden one.
pub fn copy_maps(doc: &mut Document, from: &Document) {
    let held = participants(from);
    for atom in &mut doc.atoms {
        if held.contains(&atom.id)
            && let Some(a) = from.atom(atom.id)
        {
            atom.map_num = a.map_num;
        }
    }
}

/// Put the colors a copy's reaction participants took back onto the same
/// structures: label and bond colors, highlights and ring fills.
fn copy_colors(doc: &mut Document, from: &Document) {
    let held = participants(from);
    for atom in &mut doc.atoms {
        if held.contains(&atom.id)
            && let Some(a) = from.atom(atom.id)
        {
            atom.text_style.clone_from(&a.text_style);
            atom.display.color_override = a.display.color_override;
        }
    }
    let key = |a: u64, b: u64| (a.min(b), a.max(b));
    let bonds: HashMap<_, _> = from
        .bonds
        .iter()
        .filter(|b| held.contains(&b.a) && held.contains(&b.b))
        .map(|b| (key(b.a, b.b), b))
        .collect();
    for bond in &mut doc.bonds {
        if let Some(b) = bonds.get(&key(bond.a, bond.b)) {
            bond.color = b.color;
            bond.highlight = b.highlight;
        }
    }
    // Fills on the copy's rings replace theirs in place, so the stacking of
    // other fills stays as it was.
    let ring = |f: &RingFill| crate::ring_fills::key(&f.atoms);
    let inside = |f: &RingFill| f.atoms.iter().all(|id| held.contains(id));
    let mut fills: HashMap<Vec<u64>, &RingFill> = from
        .ring_fills
        .iter()
        .filter(|f| inside(f))
        .map(|f| (ring(f), f))
        .collect();
    doc.ring_fills.retain_mut(|f| {
        if !inside(f) {
            return true;
        }
        match fills.remove(&ring(f)) {
            Some(new) => {
                f.clone_from(new);
                true
            }
            None => false,
        }
    });
    let mut added: Vec<_> = fills.into_iter().collect();
    added.sort_by(|a, b| a.0.cmp(&b.0));
    doc.ring_fills
        .extend(added.into_iter().map(|(_, f)| f.clone()));
}

/// Color cycle `id` by origin from step `s`. As in a scheme of several
/// steps, every step joined to it whose sides share atom maps takes color,
/// so an intermediate keeps its colors from one step into the next.
pub fn color(doc: &mut Document, id: u64, s: usize) -> Result<(), Refusal> {
    let (mut copy, arrows) = reactions(doc, id).map_err(|_| Refusal::Roles)?;
    let arrow = *arrows.get(s).ok_or(Refusal::Roles)?;
    // No settings are kept, so an earlier run's colors go first from every
    // step joined to this one, also from steps that have since lost their
    // maps. A text intermediate ends the chain.
    let cycle = find(doc, id).map_err(|_| Refusal::Roles)?;
    let n = cycle.intermediates.len();
    let joins = |i: usize| {
        cycle
            .intermediates
            .get(i % n)
            .is_some_and(|p| structure(doc, p))
    };
    let mut chain = vec![s];
    let mut i = s;
    while chain.len() < n && joins(i + 1) {
        i = (i + 1) % n;
        chain.push(i);
    }
    let mut i = s;
    while chain.len() < n && joins(i) {
        i = (i + n - 1) % n;
        chain.push(i);
    }
    let held: HashSet<u64> = chain
        .iter()
        .filter_map(|i| copy.reactions.get(*i))
        .flat_map(|r| [Role::Reactant, Role::Product].map(|role| r.participants(role)))
        .flatten()
        .flat_map(|p| p.atoms.iter().copied())
        .collect();
    crate::reaction_colors::clear_defaults(&mut copy, &held);
    paint(&mut copy, arrow)?;
    copy_colors(doc, &copy);
    Ok(())
}

/// What one side of a step has more of than the other, counted from its
/// drawn structures with their hydrogens.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Balance {
    /// Elements and counts the reactants have more of.
    pub before: Vec<(String, u32)>,
    /// Elements and counts the products have more of.
    pub after: Vec<(String, u32)>,
    /// The total charge before and after the step.
    pub charge: [i32; 2],
}
impl Balance {
    pub fn balanced(&self) -> bool {
        self.before.is_empty() && self.after.is_empty() && self.charge[0] == self.charge[1]
    }
}

/// Whether step `s` of cycle `id` keeps every atom and the charge.
pub fn balance(doc: &Document, id: u64, s: usize) -> Result<Balance, String> {
    let (copy, _) = step(doc, id, s)?;
    let reaction = copy.reactions.first().ok_or("The step has no reaction")?;
    let count = |role: Role| -> Result<(BTreeMap<&'static str, i64>, i32), String> {
        let ids: Vec<u64> = reaction
            .participants(role)
            .iter()
            .flat_map(|p| p.atoms.iter().copied())
            .collect();
        let side = crate::editing::selection(&copy, &ids);
        let molecule = crate::chemistry::document::prepare(&side).map_err(|e| e.to_string())?;
        let mut counts = BTreeMap::new();
        let mut charge = 0;
        for atom in molecule.state.graph.atom_facts()? {
            let symbol = crate::chemistry::ELEMENTS
                .get(usize::from(atom.atomic_number))
                .map_or("*", |e| e.symbol);
            *counts.entry(symbol).or_default() += 1;
            *counts.entry("H").or_default() += i64::from(atom.hydrogens);
            charge += i32::from(atom.charge);
        }
        counts.retain(|_, n| *n != 0);
        Ok((counts, charge))
    };
    let (left, left_charge) = count(Role::Reactant)?;
    let (right, right_charge) = count(Role::Product)?;
    let more = |a: &BTreeMap<&str, i64>, b: &BTreeMap<&str, i64>| {
        a.iter()
            .filter_map(|(symbol, n)| {
                let extra = u32::try_from(n - b.get(symbol).copied().unwrap_or(0)).ok()?;
                (extra > 0).then(|| (symbol.to_string(), extra))
            })
            .collect()
    };
    Ok(Balance {
        before: more(&left, &right),
        after: more(&right, &left),
        charge: [left_charge, right_charge],
    })
}
