//! Chemical composition and electronic settings from the species drawings.
use super::super::energy::Species;
use crate::app::xtb::state;
use reshiki::chemistry::{ELEMENTS, document as chemistry};
use reshiki::document::Document;
use reshiki::reactions::ts_plan::Side;
use reshiki::reactions::{self, Participant, Reaction};
use std::collections::{BTreeMap, HashSet};

pub(in crate::app::xtb::ts) type Counts = BTreeMap<(u8, u16), i64>;

pub(super) struct Input {
    pub label: String,
    pub charge: String,
    pub unpaired: String,
    counts: Counts,
    species: Species,
}

fn counts(doc: &Document, ids: &[u64]) -> Result<Counts, String> {
    let drawing = reshiki::editing::selection(doc, ids);
    let prepared = chemistry::prepare(&drawing).map_err(|error| error.to_string())?;
    prepared_counts(&prepared)
}

/// Atom/isotope counts include both bracket and implicit hydrogens. Shared
/// with scheme balancing so both calculation paths use identical chemistry.
pub(in crate::app::xtb::ts) fn prepared_counts(
    prepared: &chemistry::Molecule,
) -> Result<Counts, String> {
    let mut counts = Counts::new();
    for (atom, valence) in prepared
        .state
        .graph
        .atoms
        .iter()
        .zip(&prepared.state.valences)
    {
        if atom.atomic_number == 0 {
            return Err("Replace query atoms and R-groups with concrete atoms first".into());
        }
        *counts
            .entry((atom.atomic_number, atom.isotope))
            .or_default() += 1;
        let h = i64::from(atom.explicit_hydrogens) + i64::from(valence.implicit_hydrogens);
        if h > 0 {
            *counts.entry((1, 0)).or_default() += h;
        }
    }
    Ok(counts)
}

pub(in crate::app::xtb::ts) fn symbol((element, isotope): (u8, u16)) -> String {
    let element = ELEMENTS.get(usize::from(element)).map_or("?", |e| e.symbol);
    if isotope == 0 {
        element.into()
    } else {
        format!("[{isotope}{element}]")
    }
}

pub(in crate::app::xtb::ts) fn formula(counts: &Counts) -> String {
    let mut terms: Vec<_> = counts
        .iter()
        .map(|(&key, &count)| (key.0, symbol(key), count))
        .collect();
    let has_carbon = terms.iter().any(|(element, ..)| *element == 6);
    terms.sort_by_key(|(element, symbol, _)| {
        let rank = match (has_carbon, element) {
            (true, 6) => 0,
            (true, 1) => 1,
            _ => 2,
        };
        (rank, symbol.clone())
    });
    terms
        .into_iter()
        .map(|(_, symbol, count)| {
            if count == 1 {
                symbol
            } else {
                format!("{symbol}{count}")
            }
        })
        .collect()
}

pub(super) fn species_formula(doc: &Document, atoms: &[u64]) -> Option<String> {
    counts(doc, atoms).ok().map(|counts| formula(&counts))
}

fn side(doc: &Document, participants: &[Participant], product: bool) -> Result<Vec<Input>, String> {
    let mut inputs = Vec::new();
    let mut seen = HashSet::new();
    let role = if product { "Product" } else { "Reactant" };
    for participant in participants {
        let selected: HashSet<_> = participant.atoms.iter().copied().collect();
        let molecules = reactions::molecules(doc, &participant.atoms);
        if molecules.is_empty()
            || !reactions::whole_molecules(doc, &selected)
            || molecules.iter().map(Vec::len).sum::<usize>() != participant.atoms.len()
            || !participant.atoms.iter().all(|id| seen.insert(*id))
        {
            return Err(format!("Assign complete molecules to the {role} role"));
        }
        for atoms in molecules {
            let counts = counts(doc, &atoms)?;
            let first = *atoms.first().ok_or("A reaction species has no atoms")?;
            let (charge, multiplicity) = reshiki::science::drawn_state(doc, first);
            let coefficient = u32::from(participant.coefficient);
            let formula = formula(&counts);
            let label = if coefficient == 1 {
                formula
            } else {
                format!("{coefficient} × {formula}")
            };
            inputs.push(Input {
                label: format!("{role} {} · {label}", inputs.len() + 1),
                charge: charge.to_string(),
                unpaired: multiplicity.saturating_sub(1).to_string(),
                counts,
                species: Species {
                    atoms,
                    coefficient,
                    product,
                    charge,
                    multiplicity,
                },
            });
        }
    }
    Ok(inputs)
}

pub(super) fn inputs(
    doc: &Document,
    reaction: &Reaction,
) -> Result<(Vec<Input>, (usize, usize)), String> {
    let mut left = side(doc, &reaction.reactants, false)?;
    let right = side(doc, &reaction.products, true)?;
    let shape = (left.len(), right.len());
    left.extend(right);
    Ok((left, shape))
}

/// Match the planner's molecular shape without requiring a map or preparing
/// chemistry. One role participant can contain several disconnected species.
pub(super) fn shape(doc: &Document, reaction: &Reaction) -> Result<(usize, usize), ()> {
    let count = |participants: &[Participant]| {
        let ids: Vec<_> = participants
            .iter()
            .flat_map(|participant| participant.atoms.iter().copied())
            .collect();
        let selected: HashSet<_> = ids.iter().copied().collect();
        let molecules = reactions::molecules(doc, &ids);
        (reactions::whole_molecules(doc, &selected)
            && molecules.iter().map(Vec::len).sum::<usize>() == ids.len())
        .then_some(molecules.len())
        .ok_or(())
    };
    Ok((count(&reaction.reactants)?, count(&reaction.products)?))
}

pub(super) fn ignored(doc: &Document, reaction: &Reaction) -> Option<String> {
    let mut labels: Vec<_> = reaction
        .agents
        .iter()
        .enumerate()
        .map(|(index, reagent)| {
            counts(doc, &reagent.atoms)
                .map(|counts| formula(&counts))
                .unwrap_or_else(|_| format!("Reagent {}", index + 1))
        })
        .collect();
    labels.extend(
        doc.annotations
            .iter()
            .filter(|a| reaction.annotations.contains(&a.id))
            .map(|a| a.text.split_whitespace().collect::<Vec<_>>().join(" "))
            // A drawn plus between species is part of the scheme, not a condition.
            .filter(|text| !text.is_empty() && text != "+"),
    );
    (!labels.is_empty())
        .then(|| format!("Ignored: reagents and conditions · {}", labels.join(" · ")))
}

fn count_label(count: i64) -> String {
    match count.unsigned_abs() {
        1 => "one".into(),
        count => count.to_string(),
    }
}

fn imbalance(atoms: Counts, charge: i64) -> Option<String> {
    let mut differences: Vec<_> = atoms
        .into_iter()
        .filter(|(_, count)| *count != 0)
        .map(|(key, count)| {
            let relation = if count > 0 { "more" } else { "fewer" };
            format!("{} {relation} {}", count_label(count), symbol(key))
        })
        .collect();
    if charge != 0 {
        let sign = if charge < 0 { '−' } else { '+' };
        differences.push(format!("charge {sign}{}", charge.unsigned_abs()));
    }
    let last = differences.pop()?;
    let details = if differences.is_empty() {
        last
    } else {
        format!("{} and {last}", differences.join(", "))
    };
    Some(format!("Products have {details} relative to reactants"))
}

pub(super) fn validate(inputs: &[Input]) -> Result<Vec<Species>, String> {
    let mut atoms = Counts::new();
    let mut total_charge = 0;
    let mut species = Vec::with_capacity(inputs.len());
    for input in inputs {
        let mut item = input.species.clone();
        item.charge = input
            .charge
            .trim()
            .replace('−', "-")
            .parse()
            .map_err(|_| format!("{} needs a whole-number charge", input.label))?;
        let coefficient = i64::from(item.coefficient) * if item.product { 1 } else { -1 };
        total_charge += coefficient * i64::from(item.charge);
        for (&atom, &count) in &input.counts {
            *atoms.entry(atom).or_default() += coefficient * count;
        }
        species.push(item);
    }
    if let Some(error) = imbalance(atoms, total_charge) {
        return Err(error);
    }
    for (input, item) in inputs.iter().zip(&mut species) {
        item.multiplicity = input
            .unpaired
            .trim()
            .parse::<u32>()
            .ok()
            .and_then(|unpaired| unpaired.checked_add(1))
            .ok_or_else(|| {
                format!(
                    "{} needs a nonnegative whole-number count of unpaired electrons",
                    input.label
                )
            })?;
        let protons = input
            .counts
            .iter()
            .map(|(&(element, _), &count)| i64::from(element) * count)
            .sum::<i64>();
        let protons = u32::try_from(protons).map_err(|_| "The molecule has too many electrons")?;
        state::electrons(protons, item.charge, item.multiplicity - 1)
            .map_err(|error| format!("{} · {error}", input.label))?;
    }
    Ok(species)
}

/// The TS calculation follows one spin surface. Two fragments can couple
/// to any spin from their difference to their sum in integer steps.
pub(super) fn ts_state(species: &[Species], start: Side) -> Result<&Species, String> {
    let product = start == Side::Product;
    let start = species
        .iter()
        .find(|item| item.product == product)
        .ok_or("The reaction drawing changed")?;
    let mut fragments = species
        .iter()
        .filter(|item| item.product != product)
        .map(|item| u64::from(item.multiplicity - 1));
    let first = fragments.next().ok_or("The reaction drawing changed")?;
    let second = fragments.next().unwrap_or(0);
    let spin = u64::from(start.multiplicity - 1);
    let maximum = first + second;
    if !(first.abs_diff(second)..=maximum).contains(&spin) || !(maximum - spin).is_multiple_of(2) {
        return Err(
            "The reviewed unpaired electron counts cannot share one spin state along the reaction path"
                .into(),
        );
    }
    Ok(start)
}
