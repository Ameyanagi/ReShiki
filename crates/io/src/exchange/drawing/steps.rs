//! ChemDraw reaction steps for ordinary arrows and catalytic cycles.
use super::{Key, P, Result, Writer, invalid};
use crate::{
    arrow_labels::{self, Side},
    document::{Arrow, Point},
    graphics::GraphicKind,
    reactions::{self, Participant, Reaction},
    scientific::SymbolKind,
};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub(super) struct Step {
    pub reactants: Vec<u64>,
    pub products: Vec<u64>,
    pub plusses: Vec<u64>,
    pub arrows: Vec<u64>,
    pub above: Vec<u64>,
    pub below: Vec<u64>,
    pub atom_map: Vec<u64>,
}

pub(super) struct Preview<'a> {
    reaction: Reaction,
    arrow: &'a Arrow,
    file_arrow: Key,
    pub plusses: Vec<u64>,
}

impl Writer<'_> {
    pub(super) fn scheme(&mut self, steps: &[Step]) -> Result<()> {
        let id = self.id()?;
        let scheme = self.tree.add(Some(self.page), "scheme", [("id", id)])?;
        for step in steps {
            let mut attrs = vec![("id", self.id()?)];
            for (name, ids) in [
                ("ReactionStepReactants", &step.reactants),
                ("ReactionStepProducts", &step.products),
                ("ReactionStepPlusses", &step.plusses),
                ("ReactionStepArrows", &step.arrows),
                ("ReactionStepObjectsAboveArrow", &step.above),
                ("ReactionStepObjectsBelowArrow", &step.below),
                ("ReactionStepAtomMap", &step.atom_map),
            ] {
                if !ids.is_empty() {
                    attrs.push((
                        name,
                        ids.iter().map(u64::to_string).collect::<Vec<_>>().join(" "),
                    ));
                }
            }
            self.tree.add(Some(scheme), "step", attrs)?;
        }
        Ok(())
    }

    fn object(&self, id: u64) -> Option<Key> {
        self.objects
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, key)| *key)
    }

    pub(super) fn xml_id(&self, key: Key) -> Result<u64> {
        self.tree
            .value(key, "id")?
            .parse()
            .map_err(|_| invalid("Invalid object ID"))
    }

    fn outer_fragment(&self, atom: u64) -> Result<Option<Key>> {
        let mut key = self.atom_node(atom)?;
        let mut fragment = None;
        while let Some(parent) = self.tree.node(key)?.parent {
            if self.tree.node(parent)?.tag == "fragment" {
                fragment = Some(parent);
            }
            key = parent;
        }
        Ok(fragment)
    }

    /// The cycle part's file object, and whether it holds exactly that part.
    pub(super) fn part_id(&self, part: &[u64]) -> Result<(u64, bool)> {
        let first = *part.first().ok_or_else(|| invalid("Empty cycle part"))?;
        if !self.atom_indices.contains_key(&first) {
            let key = self
                .object(first)
                .ok_or_else(|| invalid("Missing cycle part"))?;
            return Ok((self.xml_id(key)?, true));
        }
        let fragment = self
            .outer_fragment(first)?
            .ok_or_else(|| invalid("Cycle structure outside a fragment"))?;
        let mut held = HashSet::new();
        for key in self.tree.descendants(fragment)? {
            if self.tree.node(key)?.tag == "n"
                && let Some(atom) = self
                    .tree
                    .get(key, "id")?
                    .and_then(|id| id.parse::<usize>().ok())
                    .and_then(|id| self.doc.atoms.get(id.checked_sub(3)?))
            {
                held.insert(atom.id);
            }
        }
        let exact = held.len() == part.len() && part.iter().all(|id| held.contains(id));
        Ok((self.xml_id(fragment)?, exact))
    }
}

impl<'a> Writer<'a> {
    pub(super) fn preview_steps(&self) -> Vec<Preview<'a>> {
        let doc = self.original;
        doc.arrows
            .iter()
            .filter_map(|arrow| {
                let reaction = reactions::preview(doc, arrow.id).ok()?;
                let file_arrow = self.object(arrow.id)?;
                let explicit = doc.reactions.iter().any(|r| r.arrow == arrow.id);
                let project = projection(arrow);
                let between = |parts: &[Participant]| {
                    parts
                        .iter()
                        .flat_map(|p| &p.atoms)
                        .filter_map(|&id| self.atom(id).ok())
                        .map(|atom| project(atom.position).0)
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), a| {
                            (lo.min(a), hi.max(a))
                        })
                };
                let ranges = [between(&reaction.reactants), between(&reaction.products)];
                let in_row = |point| {
                    let (along, across) = project(point);
                    across.abs() <= 0.5 && ranges.iter().any(|&(lo, hi)| lo < along && along < hi)
                };
                let plusses = doc
                    .annotations
                    .iter()
                    .filter(|a| {
                        a.text.trim() == "+"
                            && (reaction.annotations.contains(&a.id)
                                || !explicit && in_row(a.centre()))
                    })
                    .map(|a| a.id)
                    .chain(
                        doc.graphics
                            .iter()
                            .filter(|g| {
                                g.kind == GraphicKind::Symbol(SymbolKind::Plus)
                                    && !explicit
                                    && in_row(g.origin)
                            })
                            .map(|g| g.id),
                    )
                    .collect();
                Some(Preview {
                    reaction,
                    arrow,
                    file_arrow,
                    plusses,
                })
            })
            .collect()
    }

    fn reaction_step(&self, preview: Preview<'a>) -> Result<Option<Step>> {
        let Preview {
            reaction,
            arrow,
            file_arrow,
            plusses,
        } = preview;
        let Some([mut reactants, mut products, agents]) =
            participant_fragments(&reaction, |atom| {
                self.xml_id(self.outer_fragment(atom).ok()??).ok()
            })
        else {
            return Ok(None);
        };
        dedup(&mut reactants);
        dedup(&mut products);
        let mut step = Step {
            reactants,
            products,
            arrows: vec![self.xml_id(file_arrow)?],
            ..Step::default()
        };
        for id in plusses {
            if let Some(key) = self.object(id) {
                step.plusses.push(self.xml_id(key)?);
            }
        }
        let project = projection(arrow);
        for (agent, id) in reaction.agents.iter().zip(agents) {
            let (lo, hi) = agent
                .atoms
                .iter()
                .filter_map(|&id| self.atom(id).ok())
                .map(|a| project(a.position).1)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), a| {
                    (lo.min(a), hi.max(a))
                });
            let side = if lo + hi < 0. {
                &mut step.above
            } else {
                &mut step.below
            };
            side.push(id);
        }
        let labels: HashMap<_, _> = arrow_labels::of(self.original, arrow.id)
            .map(|l| (l.annotation, l.side))
            .collect();
        for annotation in self
            .original
            .annotations
            .iter()
            .filter(|a| a.text.trim() != "+" && reaction.annotations.contains(&a.id))
        {
            if let Some(key) = self.object(annotation.id) {
                let above = labels.get(&annotation.id).map_or_else(
                    || project(annotation.centre()).1 < 0.,
                    |side| *side == Side::Above,
                );
                let side = if above {
                    &mut step.above
                } else {
                    &mut step.below
                };
                side.push(self.xml_id(key)?);
            }
        }
        dedup(&mut step.above);
        dedup(&mut step.below);
        let reactant_maps = self.mapped_atoms(&reaction.reactants);
        let product_maps = self.mapped_atoms(&reaction.products);
        for (number, reactant) in reactant_maps {
            if let (Some(reactant), Some(Some(product))) = (reactant, product_maps.get(&number)) {
                step.atom_map.extend([
                    self.xml_id(self.atom_node(reactant)?)?,
                    self.xml_id(self.atom_node(*product)?)?,
                ]);
            }
        }
        Ok(Some(step))
    }

    fn mapped_atoms(&self, parts: &[Participant]) -> BTreeMap<u32, Option<u64>> {
        let mut mapped = BTreeMap::new();
        for atom in parts
            .iter()
            .flat_map(|p| &p.atoms)
            .filter_map(|&id| self.atom(id).ok())
        {
            if atom.map_num > 0 {
                mapped
                    .entry(atom.map_num)
                    .and_modify(|id| *id = None)
                    .or_insert(Some(atom.id));
            }
        }
        mapped
    }

    pub(super) fn reaction_schemes(&mut self, previews: Vec<Preview<'a>>) -> Result<()> {
        let mut steps = BTreeMap::new();
        let mut by_fragment = HashMap::<_, Vec<_>>::new();
        for (i, preview) in previews.into_iter().enumerate() {
            if let Some(step) = self.reaction_step(preview)? {
                for id in step.reactants.iter().chain(&step.products) {
                    by_fragment.entry(*id).or_default().push(i);
                }
                steps.insert(i, step);
            }
        }
        while let Some(first) = steps.pop_first() {
            let mut group = BTreeMap::new();
            let mut pending = vec![first];
            while let Some((i, step)) = pending.pop() {
                for id in step.reactants.iter().chain(&step.products) {
                    for other in by_fragment.remove(id).into_iter().flatten() {
                        if let Some(step) = steps.remove(&other) {
                            pending.push((other, step));
                        }
                    }
                }
                group.insert(i, step);
            }
            let scheme: Vec<_> = group.into_values().collect();
            self.scheme(&scheme)?;
        }
        Ok(())
    }
}

fn projection(arrow: &Arrow) -> impl Fn(Point) -> (f64, f64) {
    let (start, end) = (P::from(arrow.start), P::from(arrow.end));
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let mid = start.lerp(end, 0.5);
    let length_squared = dx * dx + dy * dy;
    move |point| {
        let (x, y) = (f64::from(point.x) - mid.x, f64::from(point.y) - mid.y);
        (
            (x * dx + y * dy) / length_squared,
            (y * dx - x * dy) / length_squared,
        )
    }
}

fn dedup(ids: &mut Vec<u64>) {
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(*id));
}

fn participant_fragments(
    reaction: &Reaction,
    fragment: impl Fn(u64) -> Option<u64>,
) -> Option<[Vec<u64>; 3]> {
    let ids = |parts: &[Participant]| {
        parts
            .iter()
            .map(|p| fragment(*p.atoms.first()?))
            .collect::<Option<Vec<_>>>()
    };
    let reactants = ids(&reaction.reactants)?;
    let products = ids(&reaction.products)?;
    let agents = ids(&reaction.agents)?;
    let unique: HashSet<_> = reactants.iter().collect();
    (!products.iter().any(|id| unique.contains(id))).then_some([reactants, products, agents])
}

pub(super) fn needs_fragments(previews: &[Preview<'_>], components: &[HashSet<u64>]) -> bool {
    let fragments: HashMap<_, _> = components
        .iter()
        .enumerate()
        .flat_map(|(i, ids)| ids.iter().map(move |&id| (id, i as u64)))
        .collect();
    previews
        .iter()
        .any(|p| participant_fragments(&p.reaction, |id| fragments.get(&id).copied()).is_some())
}
