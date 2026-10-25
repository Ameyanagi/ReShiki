//! Assemble imported participants without changing their chemistry or conformers.
use super::{Error, Imported, document, invalid, molfile};
use crate::{
    atom_labels::HydrogenPosition,
    document::{Annotation, Arrow, Document, Point},
    reactions::{Participant, Reaction, Role},
};

#[derive(Debug, Clone, Copy)]
struct Position {
    x: f64,
    y: f64,
}
impl Position {
    fn point(self) -> Point {
        Point::new(self.x as f32, self.y as f32)
    }
    fn shift(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
}

#[derive(Debug)]
struct Part {
    drawing: document::Drawing,
    file: molfile::FileAnnotations,
    positions: Vec<Position>,
}

/// Detached reaction scene awaiting full CIP labels for each participant.
/// Nothing is published until every participant and the complete scene validate.
#[derive(Debug)]
pub struct Drawing {
    parts: Vec<Part>,
    separators: Vec<(u64, Position)>,
    reaction: Reaction,
    arrow: Arrow,
}
impl Drawing {
    /// Participants follow canvas order: reactants, agents, then products.
    pub fn participants(
        &self,
    ) -> impl ExactSizeIterator<Item = (&document::Molecule, &molfile::FileAnnotations)> {
        self.parts.iter().map(|p| (p.drawing.molecule(), &p.file))
    }

    /// Compute each participant's complete CIP labels before publishing a scene.
    pub fn labels(&self) -> Result<Vec<document::Labels>, Error> {
        self.parts
            .iter()
            .map(|part| part.drawing.labels().map_err(Error::from))
            .collect()
    }

    pub fn finish(self, labels: Vec<document::Labels>) -> Result<Document, Error> {
        if labels.len() != self.parts.len() {
            return Err(invalid("Reaction label participant count changed"));
        }
        let mut doc = Document::default();
        for (part, labels) in self.parts.into_iter().zip(labels) {
            let positions: Vec<_> = part.positions.into_iter().map(Position::point).collect();
            let mut drawing = part.drawing.finish_at(labels, &positions)?;
            if let [atom] = drawing.atoms.as_mut_slice()
                && atom.element == "O"
                && atom.label_h == 2
            {
                atom.display.hydrogen_position = HydrogenPosition::Left;
            }
            doc.atoms.extend(drawing.atoms);
            doc.bonds.extend(drawing.bonds);
        }
        doc.annotations = self
            .separators
            .into_iter()
            .map(|(id, position)| Annotation {
                id,
                position: position.point(),
                text: "+".into(),
                format: Default::default(),
            })
            .collect();
        doc.arrows.push(self.arrow);
        doc.reactions.push(self.reaction);
        layout_ink(&mut doc);
        doc.validate().map_err(invalid)?;
        Ok(doc)
    }
}

/// Labels exist only after participant chemistry finishes. Arrange the final
/// rows from their visible extents, then fit the arrow to the agent row.
fn layout_ink(doc: &mut Document) {
    let Some(reaction) = doc.reactions.first().cloned() else {
        return;
    };
    let mut separators = reaction.annotations.iter().copied();
    let mut row = |doc: &mut Document, parts: &[Participant], mut x: f32| {
        for (index, part) in parts.iter().enumerate() {
            if index > 0
                && let Some(id) = separators.next()
                && let Some(sign) = doc.annotations.iter_mut().find(|a| a.id == id)
            {
                let layout = crate::typography::layout(&sign.text, &sign.format);
                // Keep a fixed gap on both sides of the visible + glyph.
                let gap = doc.drawing_style.world(6.);
                sign.position.x = x + gap;
                x += gap * 2. + layout.width;
            }
            let (lo, hi) = participant_bounds(doc, part);
            let dx = x - lo.x;
            for atom in doc.atoms.iter_mut().filter(|a| part.atoms.contains(&a.id)) {
                atom.position.x += dx;
            }
            x += hi.x - lo.x;
        }
        x
    };
    let reactant_end = row(doc, &reaction.reactants, 0.);
    let agent_width = row(doc, &reaction.agents, 0.);
    let start = reactant_end + 42.;
    let width = 140_f32.max(agent_width + 42.);
    let agent_shift = start + (width - agent_width) * 0.5;
    for atom in doc
        .atoms
        .iter_mut()
        .filter(|a| reaction.agents.iter().any(|p| p.atoms.contains(&a.id)))
    {
        atom.position.x += agent_shift;
    }
    let first_agent_sign = reaction.reactants.len().saturating_sub(1);
    for id in reaction
        .annotations
        .iter()
        .skip(first_agent_sign)
        .take(reaction.agents.len().saturating_sub(1))
    {
        if let Some(sign) = doc.annotations.iter_mut().find(|a| a.id == *id) {
            sign.position.x += agent_shift;
        }
    }
    row(doc, &reaction.products, start + width + 42.);
    if let Some(arrow) = doc.arrows.iter_mut().find(|a| a.id == reaction.arrow) {
        arrow.start.x = start;
        arrow.end.x = start + width;
    }
}

fn participant_bounds(doc: &Document, part: &Participant) -> (Point, Point) {
    let positions = part
        .atoms
        .iter()
        .filter_map(|id| doc.atom(*id))
        .map(|atom| atom.position);
    let labels = crate::scene::participant_label_ink_boxes(doc, &part.atoms)
        .into_iter()
        .flat_map(|(lo, hi)| [lo, hi]);
    positions.chain(labels).fold(
        (
            Point::new(f32::INFINITY, f32::INFINITY),
            Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        ),
        |(lo, hi), p| {
            (
                Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point::new(hi.x.max(p.x), hi.y.max(p.y)),
            )
        },
    )
}

#[derive(Default)]
struct Builder<'a> {
    parts: Vec<Part>,
    separators: Vec<(u64, Position)>,
    next_id: u64,
    attachments: Option<&'a [Vec<bool>]>,
}
impl Builder<'_> {
    fn id(&mut self) -> Result<u64, Error> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(Error::Limit)?;
        Ok(id)
    }

    fn row(
        &mut self,
        sources: &[molfile::Imported],
        reaction: &mut Reaction,
        role: Role,
        mut x: f64,
    ) -> Result<f64, Error> {
        for (index, source) in sources.iter().enumerate() {
            let mut molecule = source.molecule.clone();
            molecule.ids = (0..molecule.state.graph.atoms.len())
                .map(|_| self.id())
                .collect::<Result<_, _>>()?;
            let attachments = if let Some(attachments) = self.attachments {
                attachments
                    .get(self.parts.len())
                    .ok_or_else(|| invalid("Missing reaction attachment markers"))?
                    .clone()
            } else {
                vec![false; molecule.ids.len()]
            };
            let drawing = document::for_import_with_attachments(
                &molecule,
                source.annotations.is_3d,
                &source.annotations.dummy_labels,
                attachments,
            )?;
            let mut positions: Vec<_> = molecule
                .positions
                .iter()
                .map(|p| Position {
                    x: p.x * 28.,
                    y: -p.y * 28.,
                })
                .collect();
            let first = positions
                .first()
                .ok_or_else(|| invalid("Empty reaction participant"))?;
            if positions
                .iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite())
            {
                return Err(invalid("Nonfinite reaction coordinates"));
            }
            let (lo, hi, bottom, top) = positions.iter().fold(
                (first.x - 36., first.x + 36., first.y, first.y),
                |(lo, hi, bottom, top), p| {
                    (
                        lo.min(p.x - 36.),
                        hi.max(p.x + 36.),
                        bottom.min(p.y),
                        top.max(p.y),
                    )
                },
            );
            let cy = (bottom + top) / 2.;
            if index > 0 {
                let id = self.id()?;
                self.separators.push((
                    id,
                    Position {
                        x: x + 12.,
                        y: -15.,
                    },
                ));
                reaction.annotations.push(id);
                x += 70.;
            }
            for p in &mut positions {
                p.shift(x - lo, -cy);
            }
            reaction.participants_mut(role).push(Participant {
                atoms: molecule.ids,
                coefficient: 1,
            });
            self.parts.push(Part {
                drawing,
                file: source.annotations.clone(),
                positions,
            });
            x += hi - lo;
        }
        Ok(x)
    }
}

fn height(parts: &[Part]) -> f64 {
    parts
        .iter()
        .flat_map(|p| &p.positions)
        .map(|p| p.y.abs())
        .fold(0., f64::max)
}

impl Imported {
    pub fn drawing(&self) -> Result<Drawing, Error> {
        self.drawing_with_attachments(None)
    }

    pub(super) fn drawing_with_attachments(
        &self,
        attachments: Option<&[Vec<bool>]>,
    ) -> Result<Drawing, Error> {
        if attachments.is_some_and(|items| {
            items.len() != self.reactants.len() + self.agents.len() + self.products.len()
        }) {
            return Err(invalid("Reaction attachment participant count changed"));
        }
        if self.reactants.is_empty() || self.products.is_empty() {
            return Err(invalid(
                "A reaction needs at least one reactant and one product",
            ));
        }
        let mut atoms = 0usize;
        for part in self
            .reactants
            .iter()
            .chain(&self.products)
            .chain(&self.agents)
        {
            let count = part.molecule.state.graph.atoms.len();
            if count == 0 {
                return Err(invalid("Empty reaction participant"));
            }
            atoms = atoms.checked_add(count).ok_or(Error::Limit)?;
            if atoms > 10_000 {
                return Err(Error::Limit);
            }
        }
        let mut builder = Builder {
            next_id: 1,
            attachments,
            ..Default::default()
        };
        let mut reaction = Reaction::new(0);
        let x = builder.row(&self.reactants, &mut reaction, Role::Reactant, 0.)? + 42.;
        let reactant_height = height(&builder.parts);
        let agent_start = builder.parts.len();
        let notes_start = builder.separators.len();
        let agent_width = builder.row(&self.agents, &mut reaction, Role::Agent, 0.)?;
        let width = 140f64.max(agent_width + 42.);
        let agents = builder
            .parts
            .get_mut(agent_start..)
            .ok_or_else(|| invalid("Missing reagent row"))?;
        let dx = x + (width - agent_width) / 2.;
        let dy = -(reactant_height + height(agents) + 100.);
        for position in agents.iter_mut().flat_map(|p| &mut p.positions) {
            position.shift(dx, dy);
        }
        for (_, position) in builder.separators.iter_mut().skip(notes_start) {
            position.shift(dx, dy);
        }
        reaction.arrow = builder.id()?;
        let arrow = Arrow {
            start_anchor: None,
            end_anchor: None,
            id: reaction.arrow,
            start: Point::new(x as f32, 0.),
            end: Point::new((x + width) as f32, 0.),
            kind: "forward".into(),
            control: None,
            cubic: None,
            style: None,
        };
        builder.row(
            &self.products,
            &mut reaction,
            Role::Product,
            x + width + 42.,
        )?;
        Ok(Drawing {
            parts: builder.parts,
            separators: builder.separators,
            reaction,
            arrow,
        })
    }
}
