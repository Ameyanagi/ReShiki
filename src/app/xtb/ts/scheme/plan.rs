//! Read the selected, drawn reaction path without changing the drawing.
//!
//! Ordering uses shared atom instances, while calculation inputs are shared by
//! molecular identity. Balancing is deliberately limited to one exact, unique
//! molecule; a formula match alone never chooses between structural isomers.
use reshiki::document::{Document, Point};
use reshiki::reactions::{self, Participant, Reaction};
use std::collections::{BTreeMap, BTreeSet, HashSet};

mod chemistry;
#[cfg(test)]
mod tests;

use chemistry::{Composition, molecule};

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct Species {
    /// Representative drawing atoms. Empty for an added built-in molecule.
    pub atoms: Vec<u64>,
    /// Every drawn copy remains eligible for existing calculation lookup.
    pub instances: Vec<Vec<u64>>,
    /// Exactly one molecule, preserving its drawing stereo and eligible model.
    pub calculation: Document,
    pub label: String,
    pub name: String,
    pub formula: String,
    pub charge: i32,
    pub multiplicity: u32,
    /// Molecular identity and electronic state, independent of atom maps.
    pub key: String,
    composition: Composition,
    balance_label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::xtb::ts) struct Term {
    pub species: usize,
    pub coefficient: u32,
}

/// One complete chemical system at a minimum, retaining the labels of the
/// drawn instances while sharing their molecular calculation identities.
#[derive(Clone, Debug, Default)]
pub(in crate::app::xtb::ts) struct Level {
    entries: Vec<(Term, String)>,
}

impl Level {
    pub fn terms(&self) -> impl Iterator<Item = Term> + '_ {
        self.entries.iter().map(|(term, _)| *term)
    }

    pub fn label(&self) -> String {
        self.entries
            .iter()
            .map(|(term, label)| {
                if term.coefficient == 1 {
                    label.clone()
                } else {
                    format!("{} {label}", term.coefficient)
                }
            })
            .collect::<Vec<_>>()
            .join(" + ")
    }

    pub fn spectators(&self, reactants: &[Term]) -> Option<Self> {
        let mut remaining = self.clone();
        for &term in reactants {
            if remaining.take(term, "") != 0 {
                return None;
            }
        }
        Some(remaining)
    }

    fn add(&mut self, term: Term, label: String) -> Option<()> {
        if term.coefficient == 0 {
            return Some(());
        }
        if let Some((existing, _)) = self
            .entries
            .iter_mut()
            .find(|(existing, name)| existing.species == term.species && *name == label)
        {
            existing.coefficient = existing.coefficient.checked_add(term.coefficient)?;
        } else {
            self.entries.push((term, label));
        }
        Some(())
    }

    /// Prefer the requested drawn instance, then use chemically identical
    /// material already present. Return only the missing quantity.
    fn take(&mut self, term: Term, label: &str) -> u32 {
        let mut needed = term.coefficient;
        for matching_label in [true, false] {
            for (available, name) in &mut self.entries {
                if available.species == term.species && (name.as_str() == label) == matching_label {
                    let used = available.coefficient.min(needed);
                    available.coefficient -= used;
                    needed -= used;
                }
            }
        }
        self.entries.retain(|(term, _)| term.coefficient != 0);
        needed
    }
}

pub(super) fn sum_energy(
    terms: impl IntoIterator<Item = Term>,
    mut energy: impl FnMut(usize) -> Option<f64>,
) -> Option<f64> {
    terms.into_iter().try_fold(0., |total, term| {
        let total = total + f64::from(term.coefficient) * energy(term.species)?;
        total.is_finite().then_some(total)
    })
}

#[derive(Clone, Debug)]
struct DrawnTerm {
    species: usize,
    atoms: Vec<u64>,
    coefficient: u32,
    label: String,
}

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct Step {
    pub arrow: u64,
    pub reactants: Vec<Term>,
    pub products: Vec<Term>,
    pub notes: Vec<String>,
    drawn_reactants: Vec<DrawnTerm>,
    drawn_products: Vec<DrawnTerm>,
}

impl Step {
    pub fn has_added_copies(&self) -> bool {
        let added = |terms: &[Term], drawn: &[DrawnTerm]| {
            terms
                .iter()
                .map(|term| u64::from(term.coefficient))
                .sum::<u64>()
                > drawn.len() as u64
        };
        added(&self.reactants, &self.drawn_reactants) || added(&self.products, &self.drawn_products)
    }
}

#[derive(Clone, Debug)]
struct BalanceCandidate {
    species: Option<usize>,
    builtin: Option<String>,
}

#[derive(Clone, Debug)]
struct BalanceNote {
    step: usize,
    added: bool,
    candidates: Vec<BalanceCandidate>,
    text: String,
}

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct Plan {
    pub arrows: Vec<u64>,
    pub species: Vec<Species>,
    pub steps: Vec<Step>,
    pub notes: Vec<String>,
    pub snapshot: String,
    pub cyclic: bool,
    balance_notes: Vec<BalanceNote>,
}

impl Plan {
    #[cfg(test)]
    pub(in crate::app::xtb::ts) fn example() -> (Document, Self) {
        tests::two_step()
    }

    pub fn read(doc: &Document, selected: &[u64]) -> Result<Self, String> {
        let selected: HashSet<_> = selected.iter().copied().collect();
        let mut plan = Self {
            arrows: Vec::new(),
            species: Vec::new(),
            steps: Vec::new(),
            notes: Vec::new(),
            snapshot: String::new(),
            cyclic: false,
            balance_notes: Vec::new(),
        };
        let mut excluded = HashSet::new();
        for arrow in doc
            .arrows
            .iter()
            .filter(|arrow| selected.contains(&arrow.id))
        {
            if !super::super::review::reaction_arrow(doc, arrow.id) {
                return Err("Select forward reaction arrows to calculate a scheme".into());
            }
            let reaction = preview(doc, arrow.id)?;
            if !reaction.ready() {
                return Err(
                    "Each selected arrow needs starting material and product molecules".into(),
                );
            }
            validate_side(doc, &reaction.reactants)?;
            validate_side(doc, &reaction.products)?;
            let drawn_reactants = plan.read_side(doc, &reaction.reactants)?;
            let drawn_products = plan.read_side(doc, &reaction.products)?;
            if !reaction.agents.is_empty() || !reaction.annotations.is_empty() {
                excluded.insert(reaction.arrow);
            }
            plan.steps.push(Step {
                arrow: reaction.arrow,
                reactants: terms(&drawn_reactants)?,
                products: terms(&drawn_products)?,
                notes: Vec::new(),
                drawn_reactants,
                drawn_products,
            });
        }
        if plan.steps.len() < 2 {
            return Err("Select two or more consecutive reaction arrows".into());
        }
        let (order, cyclic) = order(&plan.steps)?;
        plan.cyclic = cyclic;
        plan.steps = order
            .into_iter()
            .map(|index| {
                plan.steps
                    .get(index)
                    .cloned()
                    .ok_or("The reaction path changed")
            })
            .collect::<Result<_, _>>()?;
        for (index, step) in plan.steps.iter_mut().enumerate() {
            plan.arrows.push(step.arrow);
            if excluded.contains(&step.arrow) {
                step.notes.push(format!(
                    "Reagents and conditions are excluded from step {}.",
                    index + 1
                ));
            }
        }
        // Discover every drawn candidate before balancing any step. A second
        // reactant elsewhere in the scheme can therefore balance an early step.
        let drawn_candidates = plan.species.len();
        let candidates = chemistry::candidates()?;
        for index in 0..plan.steps.len() {
            plan.balance(doc, index, drawn_candidates, &candidates)?;
        }
        for step in &plan.steps {
            plan.notes.extend(step.notes.iter().cloned());
        }
        plan.snapshot = snapshot(doc, &plan.arrows)?;
        Ok(plan)
    }

    pub fn current(&self, doc: &Document) -> bool {
        snapshot(doc, &self.arrows).as_ref() == Ok(&self.snapshot)
    }

    /// Resolve names shared with the reaction card as its naming queue runs.
    /// Instance numbers and authored names keep their established precedence;
    /// cached labels are presentation only and never enter the source snapshot.
    pub fn resolve_names(
        &mut self,
        doc: &Document,
        mut name: impl FnMut(&[u64]) -> Option<String>,
    ) {
        for species in &mut self.species {
            if species.atoms.is_empty() {
                continue;
            }
            species.name = super::super::card::molecule_name(doc, &species.atoms)
                .map(str::to_owned)
                .or_else(|| name(&species.atoms))
                .unwrap_or_else(|| species.formula.clone());
            species.label = super::super::card::compound_label(doc, &species.atoms)
                .unwrap_or(&species.name)
                .to_owned();
        }
        for step in &mut self.steps {
            for term in step
                .drawn_reactants
                .iter_mut()
                .chain(&mut step.drawn_products)
            {
                term.label = super::super::card::compound_label(doc, &term.atoms)
                    .or_else(|| super::super::card::molecule_name(doc, &term.atoms))
                    .map(str::to_owned)
                    .or_else(|| name(&term.atoms))
                    .or_else(|| {
                        self.species
                            .get(term.species)
                            .map(|species| species.formula.clone())
                    })
                    .unwrap_or_else(|| "Unnamed molecule".into());
            }
        }
        let updates: Vec<_> = self
            .balance_notes
            .iter()
            .map(|balance| self.balance_note(doc, balance))
            .collect();
        for (balance, text) in self.balance_notes.iter_mut().zip(updates) {
            if text != balance.text {
                if let Some(step) = self.steps.get_mut(balance.step) {
                    for note in step.notes.iter_mut().chain(&mut self.notes) {
                        if *note == balance.text {
                            *note = text.clone();
                        }
                    }
                }
                balance.text = text;
            }
        }
    }

    fn balance_note(&self, doc: &Document, note: &BalanceNote) -> String {
        let label = |candidate: &BalanceCandidate| {
            let species = candidate.species.and_then(|index| self.species.get(index));
            if note.added {
                if let Some(number) = species
                    .and_then(|species| super::super::card::compound_label(doc, &species.atoms))
                {
                    return format!("another copy of compound {number}");
                }
                if let Some(builtin) = &candidate.builtin {
                    return builtin.clone();
                }
                return format!(
                    "another copy of {}",
                    species.map_or("molecule", |species| species.label.as_str())
                );
            }
            species.map_or_else(
                || {
                    candidate
                        .builtin
                        .clone()
                        .unwrap_or_else(|| "molecule".into())
                },
                |species| species.label.clone(),
            )
        };
        let names = note
            .candidates
            .iter()
            .map(label)
            .collect::<Vec<_>>()
            .join(", ");
        if note.added {
            format!("Balanced by adding {names} to step {}.", note.step + 1)
        } else {
            format!(
                "More than one molecule balances step {} ({names}); continuing as drawn.",
                note.step + 1
            )
        }
    }

    pub fn step_label(&self, index: usize) -> String {
        self.steps.get(index).map_or_else(
            || "Reaction step".into(),
            |_| {
                format!(
                    "{} → {}",
                    self.endpoint_label(index, false),
                    self.endpoint_label(index, true)
                )
            },
        )
    }

    /// Labels belong to each drawn instance, even when two differently
    /// numbered structures share one molecular calculation.
    pub fn endpoint_label(&self, index: usize, product: bool) -> String {
        self.endpoint_terms(index, product)
            .map_or_else(String::new, |level| level.label())
    }

    /// Carry the minimum starting inventory needed to complete every step.
    /// Later co-reactants are present from the start; earlier coproducts remain
    /// until consumed. The same entries drive labels, energies and models.
    pub fn levels(&self) -> Option<Vec<Level>> {
        if self.steps.is_empty() {
            return None;
        }
        let mut levels = vec![Level::default()];
        for index in 0..self.steps.len() {
            let reactants = self.endpoint_terms(index, false)?;
            let products = self.endpoint_terms(index, true)?;
            let mut available = levels.last()?.clone();
            for (term, label) in &reactants.entries {
                let missing = available.take(*term, label);
                if missing > 0 {
                    for level in &mut levels {
                        level.add(
                            Term {
                                coefficient: missing,
                                ..*term
                            },
                            label.clone(),
                        )?;
                    }
                }
            }
            let mut next = products;
            for (term, label) in available.entries {
                next.add(term, label)?;
            }
            levels.push(next);
        }
        Some(levels)
    }

    fn endpoint_terms(&self, index: usize, product: bool) -> Option<Level> {
        let step = self.steps.get(index)?;
        let (drawn, balanced) = if product {
            (&step.drawn_products, &step.products)
        } else {
            (&step.drawn_reactants, &step.reactants)
        };
        let mut level = Level::default();
        let mut counts = BTreeMap::<usize, u32>::new();
        for term in drawn {
            level.add(
                Term {
                    species: term.species,
                    coefficient: term.coefficient,
                },
                term.label.clone(),
            )?;
            let count = counts.entry(term.species).or_default();
            *count = count.checked_add(term.coefficient)?;
        }
        for term in balanced {
            let added = term
                .coefficient
                .saturating_sub(counts.get(&term.species).copied().unwrap_or(0));
            if added > 0 {
                let species = self.species.get(term.species)?;
                let label = if !species.label.is_empty() {
                    &species.label
                } else if !species.name.is_empty() {
                    &species.name
                } else {
                    &species.formula
                };
                level.add(
                    Term {
                        coefficient: added,
                        ..*term
                    },
                    label.clone(),
                )?;
            }
        }
        Some(level)
    }

    pub fn title(&self) -> String {
        if self.steps.is_empty() {
            return "Reaction scheme".into();
        }
        let mut labels = vec![self.endpoint_label(0, false)];
        labels.extend((0..self.steps.len()).map(|index| self.endpoint_label(index, true)));
        labels.join(" → ")
    }

    /// A detached, calculation-only step for the existing atom-map review.
    /// Original atoms retain their IDs/maps. Stoichiometric copies and the
    /// balancing molecule get fresh IDs and no invented correspondence.
    pub fn review_document(&self, index: usize, doc: &Document) -> Result<Document, String> {
        if !self.current(doc) {
            return Err("The reaction scheme changed; review the selected arrows again".into());
        }
        let step = self.steps.get(index).ok_or("Unknown reaction step")?;
        let count = |side: &[Term]| {
            side.iter()
                .map(|term| u64::from(term.coefficient))
                .sum::<u64>()
        };
        if !matches!(
            (count(&step.reactants), count(&step.products)),
            (1, 1) | (1, 2) | (2, 1)
        ) {
            return Err(super::super::review::SHAPE_NOTE.into());
        }
        let ids: Vec<_> = std::iter::once(step.arrow)
            .chain(
                step.drawn_reactants
                    .iter()
                    .chain(&step.drawn_products)
                    .flat_map(|term| term.atoms.iter().copied()),
            )
            .collect();
        let mut result = reshiki::editing::selection(doc, &ids);
        result.reactions.clear();
        let mut used = BTreeSet::new();
        let mut copy_number = 0;
        let mut reaction = Reaction::new(step.arrow);
        reaction.reactants = self.review_side(
            &mut result,
            &step.drawn_reactants,
            &step.reactants,
            &mut used,
            &mut copy_number,
        )?;
        reaction.products = self.review_side(
            &mut result,
            &step.drawn_products,
            &step.products,
            &mut used,
            &mut copy_number,
        )?;
        result.reactions.push(reaction);
        Ok(result)
    }

    fn read_side(
        &mut self,
        doc: &Document,
        participants: &[Participant],
    ) -> Result<Vec<DrawnTerm>, String> {
        let mut side = Vec::new();
        for participant in participants {
            for mut atoms in reactions::molecules(doc, &participant.atoms) {
                atoms.sort_unstable();
                let known = self
                    .species
                    .iter()
                    .enumerate()
                    .find(|(_, species)| species.instances.contains(&atoms));
                let (index, label) = if let Some((index, species)) = known {
                    (index, chemistry::labels(doc, &atoms, &species.formula).0)
                } else {
                    let species = molecule(doc, &atoms)?;
                    let label = species.label.clone();
                    let index = self
                        .species
                        .iter()
                        .position(|item| item.key == species.key)
                        .unwrap_or(self.species.len());
                    if index == self.species.len() {
                        self.species.push(species);
                    } else if let Some(species) = self.species.get_mut(index) {
                        species.instances.push(atoms.clone());
                    }
                    (index, label)
                };
                side.push(DrawnTerm {
                    species: index,
                    atoms,
                    coefficient: u32::from(participant.coefficient),
                    label,
                });
            }
        }
        Ok(side)
    }

    fn balance(
        &mut self,
        doc: &Document,
        index: usize,
        drawn_candidates: usize,
        builtins: &[Species],
    ) -> Result<(), String> {
        let step = self.steps.get(index).ok_or("Unknown reaction step")?;
        let difference = chemistry::difference(&self.species, step)?;
        if difference.is_zero() {
            return Ok(());
        }
        let mut candidates: BTreeMap<String, (Species, bool)> = BTreeMap::new();
        for candidate in self.species.iter().take(drawn_candidates).chain(builtins) {
            for product in [false, true] {
                if difference.balanced_by(&candidate.composition, product) {
                    candidates
                        .entry(candidate.key.clone())
                        .or_insert_with(|| (candidate.clone(), product));
                }
            }
        }
        let balance = if candidates.len() == 1 {
            let (candidate, product) = candidates
                .into_values()
                .next()
                .ok_or("The balancing candidate changed")?;
            let species = self
                .species
                .iter()
                .position(|item| item.key == candidate.key)
                .unwrap_or(self.species.len());
            if species == self.species.len() {
                self.species.push(candidate.clone());
            }
            let step = self.steps.get_mut(index).ok_or("Unknown reaction step")?;
            let side = if product {
                &mut step.products
            } else {
                &mut step.reactants
            };
            add_term(side, species, 1)?;
            Some(BalanceNote {
                step: index,
                added: true,
                candidates: vec![BalanceCandidate {
                    species: Some(species),
                    builtin: builtins
                        .iter()
                        .find(|item| item.key == candidate.key)
                        .and_then(|item| item.balance_label.clone()),
                }],
                text: String::new(),
            })
        } else if candidates.is_empty() {
            None
        } else {
            let candidates = candidates
                .values()
                .map(|(candidate, _)| BalanceCandidate {
                    species: self
                        .species
                        .iter()
                        .position(|item| item.key == candidate.key),
                    builtin: builtins
                        .iter()
                        .find(|item| item.key == candidate.key)
                        .and_then(|item| item.balance_label.clone()),
                })
                .collect();
            Some(BalanceNote {
                step: index,
                added: false,
                candidates,
                text: String::new(),
            })
        };
        let note = if let Some(mut balance) = balance {
            balance.text = self.balance_note(doc, &balance);
            let note = balance.text.clone();
            self.balance_notes.push(balance);
            note
        } else {
            format!(
                "No single molecule balances step {}; continuing as drawn.",
                index + 1
            )
        };
        self.steps
            .get_mut(index)
            .ok_or("Unknown reaction step")?
            .notes
            .push(note);
        Ok(())
    }

    fn review_side(
        &self,
        doc: &mut Document,
        drawn: &[DrawnTerm],
        balanced: &[Term],
        used: &mut BTreeSet<Vec<u64>>,
        copy_number: &mut u32,
    ) -> Result<Vec<Participant>, String> {
        let mut side = Vec::new();
        let mut counts = BTreeMap::<usize, u32>::new();
        for term in drawn {
            for _ in 0..term.coefficient {
                let atoms = if used.insert(term.atoms.clone()) {
                    term.atoms.clone()
                } else {
                    self.append_copy(doc, term.species, copy_number)?
                };
                side.push(Participant {
                    atoms,
                    coefficient: 1,
                });
                let count = counts.entry(term.species).or_default();
                *count = count.checked_add(1).ok_or("The coefficient is too large")?;
            }
        }
        for term in balanced {
            let drawn = counts.get(&term.species).copied().unwrap_or(0);
            for _ in drawn..term.coefficient {
                side.push(Participant {
                    atoms: self.append_copy(doc, term.species, copy_number)?,
                    coefficient: 1,
                });
            }
        }
        Ok(side)
    }

    fn append_copy(
        &self,
        doc: &mut Document,
        species: usize,
        copy_number: &mut u32,
    ) -> Result<Vec<u64>, String> {
        let species = self
            .species
            .get(species)
            .ok_or("Unknown reaction species")?;
        let mut copy = species.calculation.clone();
        for atom in &mut copy.atoms {
            atom.map_num = 0;
        }
        *copy_number = copy_number
            .checked_add(1)
            .ok_or("Too many molecular copies")?;
        let ids: Vec<_> = copy.atoms.iter().map(|atom| atom.id).collect();
        let center = reshiki::editing::center(&copy, &ids);
        let anchor = doc
            .arrows
            .first()
            .map_or(Point::default(), |arrow| arrow.start);
        let offset = Point::new(
            anchor.x - center.x,
            anchor.y - center.y - 100. * *copy_number as f32,
        );
        let inserted = reshiki::editing::append_preserving_layout(doc, &copy, offset);
        let atoms: Vec<_> = inserted
            .into_iter()
            .filter(|id| doc.atom(*id).is_some())
            .collect();
        if atoms.len() != copy.atoms.len() {
            return Err("Could not prepare a separate molecular copy for calculation".into());
        }
        Ok(atoms)
    }
}

fn add_term(terms: &mut Vec<Term>, species: usize, coefficient: u32) -> Result<(), String> {
    if let Some(term) = terms.iter_mut().find(|term| term.species == species) {
        term.coefficient = term
            .coefficient
            .checked_add(coefficient)
            .ok_or("The reaction coefficient is too large")?;
    } else {
        terms.push(Term {
            species,
            coefficient,
        });
    }
    Ok(())
}

fn terms(drawn: &[DrawnTerm]) -> Result<Vec<Term>, String> {
    let mut terms = Vec::new();
    for term in drawn {
        add_term(&mut terms, term.species, term.coefficient)?;
    }
    Ok(terms)
}

fn validate_side(doc: &Document, participants: &[Participant]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for participant in participants {
        let ids: HashSet<_> = participant.atoms.iter().copied().collect();
        let molecules = reactions::molecules(doc, &participant.atoms);
        if participant.coefficient == 0
            || molecules.is_empty()
            || !reactions::whole_molecules(doc, &ids)
            || molecules.iter().map(Vec::len).sum::<usize>() != participant.atoms.len()
            || !participant.atoms.iter().all(|id| seen.insert(*id))
        {
            return Err("Assign complete, nonoverlapping molecules and positive coefficients to every reactant and product role".into());
        }
    }
    Ok(())
}

/// Scheme inference treats molecules above or below the shaft as agents even
/// when they sit in its left or right half. Explicitly assigned roles win.
pub(in crate::app::xtb::ts) fn preview(doc: &Document, arrow: u64) -> Result<Reaction, String> {
    let mut reaction = reactions::preview(doc, arrow).map_err(str::to_owned)?;
    if doc.reactions.iter().any(|reaction| reaction.arrow == arrow) {
        return Ok(reaction);
    }
    let arrow = doc
        .arrows
        .iter()
        .find(|item| item.id == arrow)
        .ok_or("The reaction arrow changed")?;
    let dx = f64::from(arrow.end.x) - f64::from(arrow.start.x);
    let dy = f64::from(arrow.end.y) - f64::from(arrow.start.y);
    let length_squared = dx * dx + dy * dy;
    let agent = |participant: &Participant| {
        let (mut above, mut below) = (true, true);
        let mut visible = false;
        for atom in participant
            .atoms
            .iter()
            .filter(|id| doc.atom_visible(**id))
            .filter_map(|id| doc.atom(*id))
        {
            visible = true;
            let x = f64::from(atom.position.x) - f64::from(arrow.start.x);
            let y = f64::from(atom.position.y) - f64::from(arrow.start.y);
            let along = (x * dx + y * dy) / length_squared;
            if !(0. ..=1.).contains(&along) {
                return false;
            }
            let across = (y * dx - x * dy) / length_squared;
            above &= across < -1e-6;
            below &= across > 1e-6;
        }
        visible && (above || below)
    };
    for side in [&mut reaction.reactants, &mut reaction.products] {
        let mut participants = Vec::new();
        for participant in std::mem::take(side) {
            if agent(&participant) {
                reaction.agents.push(participant);
            } else {
                participants.push(participant);
            }
        }
        *side = participants;
    }
    Ok(reaction)
}

type DrawnInstances = BTreeSet<Vec<u64>>;
type PathSides = (DrawnInstances, DrawnInstances);

fn spectator_quantities(step: &Step) -> Result<BTreeMap<usize, u32>, String> {
    let mut quantities = BTreeMap::<usize, [u32; 2]>::new();
    for (drawn, product) in [(&step.drawn_reactants, false), (&step.drawn_products, true)] {
        for term in drawn {
            let [r, p] = quantities.entry(term.species).or_default();
            let count = if product { p } else { r };
            *count = count
                .checked_add(term.coefficient)
                .ok_or("The reaction coefficient is too large")?;
        }
    }
    // Cancel common quantities by molecular identity before linking drawn
    // instances. A wholly unchanged row can still link numbered copies of one
    // compound; preserve that row because it has no distinguishable spectator.
    let unchanged = quantities.values().all(|[r, p]| r == p);
    Ok(quantities
        .into_iter()
        .map(|(species, [r, p])| (species, if unchanged { 0 } else { r.min(p) }))
        .collect())
}

#[derive(Clone, Copy)]
enum PathCandidates {
    Certain,
    Chemical,
    Drawn,
}

/// Give unavoidable endpoints priority over interchangeable copies, and those
/// copies priority over a chemically unchanged molecule carried to the next row.
fn candidate_sides(step: &Step, candidates: PathCandidates) -> Result<PathSides, String> {
    let quantities = spectator_quantities(step)?;
    let side = |drawn: &[DrawnTerm]| {
        drawn
            .iter()
            .filter(|term| match candidates {
                PathCandidates::Certain => {
                    term.coefficient > quantities.get(&term.species).copied().unwrap_or(0)
                }
                PathCandidates::Chemical => {
                    drawn
                        .iter()
                        .filter(|other| other.species == term.species)
                        .map(|other| u64::from(other.coefficient))
                        .sum::<u64>()
                        > u64::from(quantities.get(&term.species).copied().unwrap_or(0))
                }
                PathCandidates::Drawn => true,
            })
            .map(|term| term.atoms.clone())
            .collect()
    };
    Ok((side(&step.drawn_reactants), side(&step.drawn_products)))
}

fn path_sides(step: &Step, neighbors: &PathSides, others: &PathSides) -> Result<PathSides, String> {
    let quantities = spectator_quantities(step)?;
    let remaining = |side: &[DrawnTerm], preferred: &DrawnInstances, other: &DrawnInstances| {
        let mut cancel = quantities.clone();
        let mut drawn: Vec<_> = side.iter().collect();
        // Keep the copy linking the adjacent step. Cancel a nonneighbor's
        // shortcut first, then an unshared copy, without using atom-ID order.
        drawn.sort_by_key(|term| {
            (
                preferred.contains(&term.atoms),
                !other.contains(&term.atoms),
            )
        });
        let mut remaining: DrawnInstances = drawn
            .iter()
            .filter_map(|term| {
                let count = cancel.get_mut(&term.species)?;
                let used = (*count).min(term.coefficient);
                *count -= used;
                (used < term.coefficient).then(|| term.atoms.clone())
            })
            .collect();
        // Retain the same shared instances on both ends of a chosen link, even
        // if their chemical quantity canceled. The energy terms are unchanged.
        remaining.extend(
            side.iter()
                .filter(|term| preferred.contains(&term.atoms))
                .map(|term| term.atoms.clone()),
        );
        remaining
    };
    Ok((
        remaining(&step.drawn_reactants, &neighbors.0, &others.0),
        remaining(&step.drawn_products, &neighbors.1, &others.1),
    ))
}

fn order(steps: &[Step]) -> Result<(Vec<usize>, bool), String> {
    let certain: Vec<_> = steps
        .iter()
        .map(|step| candidate_sides(step, PathCandidates::Certain))
        .collect::<Result<_, _>>()?;
    let chemical: Vec<_> = steps
        .iter()
        .map(|step| candidate_sides(step, PathCandidates::Chemical))
        .collect::<Result<_, _>>()?;
    let possible: Vec<_> = steps
        .iter()
        .map(|step| candidate_sides(step, PathCandidates::Drawn))
        .collect::<Result<_, _>>()?;
    let mut preferred_links = path_links(&certain, |_, _| true)?;
    check_branches(&preferred_links.0, &preferred_links.1)?;
    // Certain links take precedence over optional links from interchangeable
    // copies. In particular, a carried spectator cannot skip a known step.
    for (sides, connect_components) in [(&chemical, false), (&possible, true)] {
        let (incoming, outgoing) = &preferred_links;
        let components = path_components(incoming, outgoing);
        preferred_links = path_links(sides, |from, to| {
            outgoing
                .get(from)
                .is_some_and(|edges| edges.is_empty() || edges.contains(&to))
                && incoming
                    .get(to)
                    .is_some_and(|edges| edges.is_empty() || edges.contains(&from))
                && (!connect_components
                    || outgoing.get(from).is_some_and(|edges| edges.contains(&to))
                    || components.get(from) != components.get(to))
        })?;
        check_branches(&preferred_links.0, &preferred_links.1)?;
    }
    let (path, cyclic) = path_order(&preferred_links.0, &preferred_links.1)?;
    let mut sides = vec![PathSides::default(); steps.len()];
    for (position, &index) in path.iter().enumerate() {
        let previous = position
            .checked_sub(1)
            .and_then(|i| path.get(i))
            .copied()
            .or_else(|| cyclic.then(|| path.last().copied()).flatten());
        let next = path
            .get(position + 1)
            .copied()
            .or_else(|| cyclic.then(|| path.first().copied()).flatten());
        let mut neighbors = PathSides::default();
        let mut others = PathSides::default();
        for (other, (reactants, products)) in possible.iter().enumerate() {
            if other == index {
                continue;
            }
            if Some(other) == previous {
                neighbors.0.extend(products.iter().cloned());
            } else {
                others.0.extend(products.iter().cloned());
            }
            if Some(other) == next {
                neighbors.1.extend(reactants.iter().cloned());
            } else {
                others.1.extend(reactants.iter().cloned());
            }
        }
        let step = steps.get(index).ok_or("Unknown reaction step")?;
        *sides.get_mut(index).ok_or("Unknown reaction step")? =
            path_sides(step, &neighbors, &others)?;
    }
    // A coefficient can require several copies to remain. Verify that choosing
    // the neighboring instances did not leave an unavoidable extra link.
    let (incoming, outgoing) = path_links(&sides, |_, _| true)?;
    path_order(&incoming, &outgoing)
}

type PathLinks = (Vec<Vec<usize>>, Vec<Vec<usize>>);

fn path_components(incoming: &[Vec<usize>], outgoing: &[Vec<usize>]) -> Vec<Option<usize>> {
    let mut components = vec![None; incoming.len()];
    for start in 0..incoming.len() {
        let mut pending = vec![start];
        while let Some(index) = pending.pop() {
            let Some(component) = components.get_mut(index) else {
                continue;
            };
            if component.is_some() {
                continue;
            }
            *component = Some(start);
            pending.extend(incoming.get(index).into_iter().flatten().copied());
            pending.extend(outgoing.get(index).into_iter().flatten().copied());
        }
    }
    components
}

fn path_links(
    sides: &[PathSides],
    include: impl Fn(usize, usize) -> bool,
) -> Result<PathLinks, String> {
    let mut incoming = vec![Vec::new(); sides.len()];
    let mut outgoing = vec![Vec::new(); sides.len()];
    for (from, (_, products)) in sides.iter().enumerate() {
        for (to, (reactants, _)) in sides.iter().enumerate() {
            if from != to && include(from, to) && !products.is_disjoint(reactants) {
                incoming
                    .get_mut(to)
                    .ok_or("Unknown reaction step")?
                    .push(from);
                outgoing
                    .get_mut(from)
                    .ok_or("Unknown reaction step")?
                    .push(to);
            }
        }
    }
    Ok((incoming, outgoing))
}

fn check_branches(incoming: &[Vec<usize>], outgoing: &[Vec<usize>]) -> Result<(), String> {
    if incoming.iter().chain(outgoing).any(|edges| edges.len() > 1) {
        return Err(
            "The selected scheme branches or merges. Select one consecutive reaction path".into(),
        );
    }
    Ok(())
}

fn path_order(
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
) -> Result<(Vec<usize>, bool), String> {
    check_branches(incoming, outgoing)?;
    let starts: Vec<_> = incoming
        .iter()
        .enumerate()
        .filter(|(_, edges)| edges.is_empty())
        .map(|(index, _)| index)
        .collect();
    let cyclic = starts.is_empty();
    let first = if cyclic {
        0
    } else if starts.len() == 1 {
        *starts.first().ok_or("The reaction path has no beginning")?
    } else {
        return Err("The selected arrows are disconnected or branching. Consecutive steps must share a drawn product and reactant molecule".into());
    };
    let mut order = Vec::new();
    let mut visited = BTreeSet::new();
    let mut next = Some(first);
    while let Some(index) = next {
        if !visited.insert(index) {
            if !cyclic || index != first {
                return Err("Select one unbranched reaction path or a single closed cycle".into());
            }
            break;
        }
        order.push(index);
        next = outgoing.get(index).and_then(|edges| edges.first()).copied();
    }
    if order.len() != incoming.len() {
        return Err("The selected arrows contain disconnected paths or cycles. Select one consecutive reaction path".into());
    }
    Ok((order, cyclic))
}

fn snapshot(doc: &Document, arrows: &[u64]) -> Result<String, String> {
    let mut entries = Vec::new();
    for &arrow in arrows {
        let reaction = preview(doc, arrow)?;
        let mut atoms: Vec<_> = reaction
            .reactants
            .iter()
            .chain(&reaction.products)
            .flat_map(|participant| participant.atoms.iter().copied())
            .collect();
        atoms.sort_unstable();
        atoms.dedup();
        let maps: Vec<_> = atoms
            .iter()
            .filter_map(|&id| doc.atom(id).map(|atom| (id, atom.map_num)))
            .collect();
        let labels: Vec<_> = reactions::molecules(doc, &atoms)
            .into_iter()
            .map(|atoms| {
                (
                    super::super::card::compound_label(doc, &atoms),
                    super::super::card::authored_molecule_name(doc, &atoms),
                )
            })
            .collect();
        let conditions: Vec<_> = reaction
            .annotations
            .iter()
            .filter_map(|id| {
                doc.annotations
                    .iter()
                    .find(|annotation| annotation.id == *id)
            })
            .map(|annotation| (&annotation.id, &annotation.text))
            .collect();
        entries.push(serde_json::json!([
            reaction,
            reshiki::science::baseline(doc, &atoms),
            maps,
            labels,
            conditions,
        ]));
    }
    serde_json::to_string(&entries).map_err(|error| error.to_string())
}
