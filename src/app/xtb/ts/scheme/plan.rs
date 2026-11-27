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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::xtb::ts) struct Term {
    pub species: usize,
    pub coefficient: u32,
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

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct Plan {
    pub arrows: Vec<u64>,
    pub species: Vec<Species>,
    pub steps: Vec<Step>,
    pub notes: Vec<String>,
    pub snapshot: String,
    pub cyclic: bool,
}

impl Plan {
    #[cfg(test)]
    pub(in crate::app::xtb::ts) fn example() -> (Document, Self) {
        tests::two_step()
    }

    pub fn read(doc: &Document, selected: &[u64]) -> Result<Self, String> {
        let selected: HashSet<_> = selected.iter().copied().collect();
        let mut sources = Vec::new();
        for arrow in doc
            .arrows
            .iter()
            .filter(|arrow| selected.contains(&arrow.id))
        {
            if !super::super::review::reaction_arrow(doc, arrow.id) {
                return Err("Select forward reaction arrows to calculate a scheme".into());
            }
            let reaction = reactions::preview(doc, arrow.id).map_err(str::to_owned)?;
            if !reaction.ready() {
                return Err(
                    "Each selected arrow needs starting material and product molecules".into(),
                );
            }
            validate_side(doc, &reaction.reactants)?;
            validate_side(doc, &reaction.products)?;
            sources.push(reaction);
        }
        if sources.len() < 2 {
            return Err("Select two or more consecutive reaction arrows".into());
        }
        let (order, cyclic) = order(doc, &sources)?;
        let mut plan = Self {
            arrows: Vec::new(),
            species: Vec::new(),
            steps: Vec::new(),
            notes: Vec::new(),
            snapshot: String::new(),
            cyclic,
        };
        for index in order {
            let reaction = sources.get(index).ok_or("The reaction path changed")?;
            let drawn_reactants = plan.read_side(doc, &reaction.reactants)?;
            let drawn_products = plan.read_side(doc, &reaction.products)?;
            let mut notes = Vec::new();
            if !reaction.agents.is_empty() || !reaction.annotations.is_empty() {
                notes.push("Reagents, catalysts and conditions are not included unless assigned a reactant or product role".into());
            }
            plan.arrows.push(reaction.arrow);
            plan.steps.push(Step {
                arrow: reaction.arrow,
                reactants: terms(&drawn_reactants)?,
                products: terms(&drawn_products)?,
                notes,
                drawn_reactants,
                drawn_products,
            });
        }
        // Discover every drawn candidate before balancing any step. A second
        // reactant elsewhere in the scheme can therefore balance an early step.
        let drawn_candidates = plan.species.len();
        let candidates = chemistry::candidates()?;
        for index in 0..plan.steps.len() {
            plan.balance(index, drawn_candidates, &candidates)?;
        }
        for (index, step) in plan.steps.iter().enumerate() {
            plan.notes.extend(
                step.notes
                    .iter()
                    .map(|note| format!("Step {}: {note}", index + 1)),
            );
        }
        plan.snapshot = snapshot(doc, &plan.arrows)?;
        Ok(plan)
    }

    pub fn current(&self, doc: &Document) -> bool {
        snapshot(doc, &self.arrows).as_ref() == Ok(&self.snapshot)
    }

    /// Resolve names already cached by the reaction card once at opening.
    /// Instance numbers and authored names keep their established precedence;
    /// cached labels are presentation only and never enter the source snapshot.
    pub fn resolve_names(
        &mut self,
        doc: &Document,
        mut name: impl FnMut(&[u64]) -> Option<String>,
    ) {
        for species in &mut self.species {
            if !species.atoms.is_empty()
                && super::super::card::molecule_name(doc, &species.atoms).is_none()
                && let Some(cached) = name(&species.atoms)
            {
                species.name = cached.clone();
                if super::super::card::compound_label(doc, &species.atoms).is_none() {
                    species.label = cached;
                }
            }
        }
        for step in &mut self.steps {
            for term in step
                .drawn_reactants
                .iter_mut()
                .chain(&mut step.drawn_products)
            {
                if super::super::card::compound_label(doc, &term.atoms).is_none()
                    && super::super::card::molecule_name(doc, &term.atoms).is_none()
                    && let Some(cached) = name(&term.atoms)
                {
                    term.label = cached;
                }
            }
        }
    }

    pub fn side_label(&self, terms: &[Term]) -> String {
        terms
            .iter()
            .filter_map(|term| {
                let species = self.species.get(term.species)?;
                let label = if !species.label.is_empty() {
                    &species.label
                } else if !species.name.is_empty() {
                    &species.name
                } else {
                    &species.formula
                };
                Some(if term.coefficient == 1 {
                    label.clone()
                } else {
                    format!("{} {label}", term.coefficient)
                })
            })
            .collect::<Vec<_>>()
            .join(" + ")
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
        label_line(self.endpoint_terms(index, product))
    }

    /// A shared minimum can also hold a co-reactant for the next step. Include
    /// only its excess over species already shown at this product endpoint.
    pub fn minimum_label(&self, index: usize) -> String {
        let mut labels = self.endpoint_terms(index, true);
        let mut available = BTreeMap::<usize, u32>::new();
        for (species, _, count) in &labels {
            let entry = available.entry(*species).or_default();
            *entry = entry.saturating_add(*count);
        }
        if let Some(next) = index.checked_add(1).filter(|next| *next < self.steps.len()) {
            for (species, label, count) in self.endpoint_terms(next, false) {
                let used = available.entry(species).or_default();
                let extra = count.saturating_sub(*used);
                *used = used.saturating_sub(count);
                if extra > 0 {
                    add_label(&mut labels, species, label, extra);
                }
            }
        }
        label_line(labels)
    }

    fn endpoint_terms(&self, index: usize, product: bool) -> Vec<(usize, String, u32)> {
        let Some(step) = self.steps.get(index) else {
            return Vec::new();
        };
        let (drawn, balanced) = if product {
            (&step.drawn_products, &step.products)
        } else {
            (&step.drawn_reactants, &step.reactants)
        };
        let mut labels = Vec::new();
        let mut counts = BTreeMap::<usize, u32>::new();
        for term in drawn {
            add_label(
                &mut labels,
                term.species,
                term.label.clone(),
                term.coefficient,
            );
            let count = counts.entry(term.species).or_default();
            *count = count.saturating_add(term.coefficient);
        }
        for term in balanced {
            let added = term
                .coefficient
                .saturating_sub(counts.get(&term.species).copied().unwrap_or(0));
            if added > 0 {
                add_label(
                    &mut labels,
                    term.species,
                    self.side_label(&[Term {
                        species: term.species,
                        coefficient: 1,
                    }]),
                    added,
                );
            }
        }
        labels
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
                let species = molecule(doc, &atoms)?;
                let label = species.label.clone();
                let index = self
                    .species
                    .iter()
                    .position(|item| item.key == species.key)
                    .unwrap_or(self.species.len());
                if index == self.species.len() {
                    self.species.push(species);
                }
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
        let note = if candidates.len() == 1 {
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
            format!(
                "Added one copy of {} to the {} for calculation to balance atoms, isotopes and charge exactly",
                candidate.label,
                if product { "products" } else { "reactants" }
            )
        } else if candidates.is_empty() {
            format!(
                "Warning: {}. No single supported molecule balances this step exactly; proceeding as drawn",
                difference.description()
            )
        } else {
            let names = candidates
                .values()
                .map(|(candidate, _)| candidate.label.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Warning: {}. More than one molecule could balance this step ({names}); proceeding as drawn",
                difference.description()
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

fn add_label(
    labels: &mut Vec<(usize, String, u32)>,
    species: usize,
    label: String,
    coefficient: u32,
) {
    if let Some((_, _, count)) = labels
        .iter_mut()
        .find(|(item, name, _)| *item == species && *name == label)
    {
        *count = count.saturating_add(coefficient);
    } else {
        labels.push((species, label, coefficient));
    }
}

fn label_line(labels: Vec<(usize, String, u32)>) -> String {
    labels
        .into_iter()
        .map(|(_, label, coefficient)| {
            if coefficient == 1 {
                label
            } else {
                format!("{coefficient} {label}")
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
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

fn drawn_species(doc: &Document, participants: &[Participant]) -> BTreeSet<Vec<u64>> {
    participants
        .iter()
        .flat_map(|participant| reactions::molecules(doc, &participant.atoms))
        .map(|mut ids| {
            ids.sort_unstable();
            ids
        })
        .collect()
}

fn order(doc: &Document, sources: &[Reaction]) -> Result<(Vec<usize>, bool), String> {
    let sides: Vec<_> = sources
        .iter()
        .map(|reaction| {
            let mut reactants = drawn_species(doc, &reaction.reactants);
            let mut products = drawn_species(doc, &reaction.products);
            let spectators: BTreeSet<_> = reactants.intersection(&products).cloned().collect();
            reactants.retain(|atoms| !spectators.contains(atoms));
            products.retain(|atoms| !spectators.contains(atoms));
            (reactants, products)
        })
        .collect();
    let mut incoming = vec![Vec::new(); sources.len()];
    let mut outgoing = vec![Vec::new(); sources.len()];
    for (from, (_, products)) in sides.iter().enumerate() {
        for (to, (reactants, _)) in sides.iter().enumerate() {
            if from != to && !products.is_disjoint(reactants) {
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
    if incoming
        .iter()
        .chain(&outgoing)
        .any(|edges| edges.len() > 1)
    {
        return Err(
            "The selected scheme branches or merges. Select one consecutive reaction path".into(),
        );
    }
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
    if order.len() != sources.len() {
        return Err("The selected arrows contain disconnected paths or cycles. Select one consecutive reaction path".into());
    }
    Ok((order, cyclic))
}

fn snapshot(doc: &Document, arrows: &[u64]) -> Result<String, String> {
    let mut entries = Vec::new();
    for &arrow in arrows {
        let reaction = reactions::preview(doc, arrow).map_err(str::to_owned)?;
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
            .map(|atoms| chemistry::labels(doc, &atoms, ""))
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
