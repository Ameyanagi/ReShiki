//! One view-only overlay for the drawing and exported animation frames.

use crate::app::xtb::ts::profile::source_reaction_prefix;
use reshiki::document::{Document, Point};
use reshiki::graphics::{
    EnergyProfileGeometry, EnergyProfileSample, EnergyProfileStateKind as Kind, Graphic,
    GraphicStyle, LinePattern, PathCommand,
};
use reshiki::palette::{Color, Hue, Row};
use reshiki::reactions::{self, Reaction, Role};
use reshiki::scene::{self, Primitive};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Changes {
    forming: Vec<[u64; 2]>,
    breaking: Vec<[u64; 2]>,
}

struct SpeciesBox {
    owner: u64,
    lo: Point,
    hi: Point,
}

#[derive(Default)]
pub(super) struct Artwork {
    species: Vec<Vec<SpeciesBox>>,
    changes: Vec<Changes>,
    matched_steps: BTreeSet<usize>,
    selection: Vec<u64>,
    clips: BTreeMap<u64, [Point; 4]>,
}

impl Artwork {
    pub(super) fn new(doc: &Document, graphic: &Graphic) -> Self {
        let Some(profile) = graphic.energy_profile.as_deref() else {
            return Self::default();
        };
        let mut artwork = Self {
            species: (0..profile.states.len()).map(|_| vec![]).collect(),
            changes: (0..profile.states.len())
                .map(|_| Changes::default())
                .collect(),
            selection: vec![graphic.id],
            ..Self::default()
        };
        let reactions = source_reaction_prefix(doc, profile);
        for (index, (step, current)) in profile.steps.iter().zip(reactions).enumerate() {
            artwork.matched_steps.insert(index);
            artwork.selection.extend(current.ids());
            // Implicit atoms can leave some or all of the saddle's changes off the drawing.
            let boxed_transition = step.transition.filter(|&state| {
                let (Some(transition), Some(changes)) =
                    (profile.states.get(state), artwork.changes.get_mut(state))
                else {
                    return false;
                };
                let Some(geometry) = transition.geometry.as_ref() else {
                    return true;
                };
                *changes = scheme_changes(doc, &current, geometry);
                for pairs in [&mut changes.forming, &mut changes.breaking] {
                    pairs.retain(|&[a, b]| doc.atom_drawn(a) && doc.atom_drawn(b));
                }
                let drawn = changes.forming.len() + changes.breaking.len();
                drawn == 0 || drawn < geometry.forming.len() + geometry.breaking.len()
            });
            for (state, participants) in [
                (step.from, &current.reactants),
                (step.to, &current.products),
            ] {
                let atoms: Vec<_> = participants.iter().map(|p| p.atoms.clone()).collect();
                let boxes = scene::selections_ink_bounds(doc, &atoms);
                for (bounds, atoms) in boxes.into_iter().zip(atoms) {
                    let (Some((lo, hi)), Some(&owner)) = (bounds, atoms.first()) else {
                        continue;
                    };
                    if !atoms.iter().any(|&id| doc.atom_drawn(id)) {
                        continue;
                    }
                    for state in std::iter::once(state).chain(boxed_transition) {
                        if let Some(species) = artwork.species.get_mut(state)
                            && !species.iter().any(|other| other.owner == owner)
                        {
                            species.push(SpeciesBox { owner, lo, hi });
                        }
                    }
                }
            }
        }
        artwork.selection.sort_unstable();
        artwork.selection.dedup();
        for table in &doc.graphics {
            for cell in table.table.iter().flat_map(|table| &table.cells) {
                if let Some(frame) = cell.frame(table) {
                    for &id in cell
                        .members
                        .iter()
                        .filter(|id| artwork.selection.contains(id))
                    {
                        artwork.clips.insert(id, frame.corners());
                    }
                }
            }
        }
        artwork
    }

    pub(super) fn overlay(
        &self,
        doc: &Document,
        graphic: &Graphic,
        state: usize,
        sample: Option<(usize, EnergyProfileSample)>,
        geometry: Option<&EnergyProfileGeometry>,
        highlight: bool,
    ) -> Vec<Primitive> {
        let mut out = self.clip(
            graphic.id,
            graphic.energy_profile_overlay(&doc.drawing_style, doc, state, sample, geometry),
        );
        if !highlight || sample.is_some_and(|(step, _)| !self.matched_steps.contains(&step)) {
            return out;
        }
        let padding = doc.drawing_style.bond_length_world * 0.22;
        if let Some(species) = self.species.get(state) {
            for &SpeciesBox { owner, lo, hi } in species {
                let (lo, hi) = (lo.offset(-padding, -padding), hi.offset(padding, padding));
                out.extend(self.clip(
                    owner,
                    vec![path(
                        vec![
                            PathCommand::Move(lo),
                            PathCommand::Line(Point::new(hi.x, lo.y)),
                            PathCommand::Line(hi),
                            PathCommand::Line(Point::new(lo.x, hi.y)),
                            PathCommand::Close,
                        ],
                        Hue::Green,
                        false,
                        doc,
                    )],
                ));
            }
        }
        if graphic
            .energy_profile
            .as_deref()
            .and_then(|p| p.states.get(state))
            .is_some_and(|state| state.kind == Kind::TransitionState)
            && let Some(changes) = self.changes.get(state)
        {
            for (pairs, hue, crossed) in [
                (&changes.forming, Hue::Green, false),
                (&changes.breaking, Hue::Red, true),
            ] {
                for &[a, b] in pairs {
                    let owner = a;
                    let Some((a, b)) = doc.atom(a).zip(doc.atom(b)) else {
                        continue;
                    };
                    let (a, b) = (a.position, b.position);
                    let mut bond = vec![path(
                        vec![PathCommand::Move(a), PathCommand::Line(b)],
                        hue,
                        true,
                        doc,
                    )];
                    if crossed {
                        let centre = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                        let d = doc.drawing_style.bond_length_world * 0.11;
                        bond.push(path(
                            vec![
                                PathCommand::Move(centre.offset(-d, -d)),
                                PathCommand::Line(centre.offset(d, d)),
                                PathCommand::Move(centre.offset(-d, d)),
                                PathCommand::Line(centre.offset(d, -d)),
                            ],
                            hue,
                            false,
                            doc,
                        ));
                    }
                    out.extend(self.clip(owner, bond));
                }
            }
        }
        out
    }

    fn clip(&self, id: u64, drawing: Vec<Primitive>) -> Vec<Primitive> {
        if let Some(&corners) = self.clips.get(&id) {
            vec![Primitive::Clip {
                corners,
                primitives: drawing,
            }]
        } else {
            drawing
        }
    }
}

fn side_ids(reaction: &Reaction, role: Role) -> BTreeSet<u64> {
    reaction
        .participants(role)
        .iter()
        .flat_map(|part| part.atoms.iter().copied())
        .collect()
}

fn scheme_changes(
    doc: &Document,
    reaction: &Reaction,
    geometry: &EnergyProfileGeometry,
) -> Changes {
    if geometry.forming.is_empty() && geometry.breaking.is_empty() {
        return Changes::default();
    }
    let hash = geometry.frame.hash();
    let ids = side_ids(reaction, Role::Reactant)
        .union(&side_ids(reaction, Role::Product))
        .copied()
        .collect::<BTreeSet<_>>();
    // Geometry identity ignores drawing references. Attachments survive paste
    // renumbering and retain the exact calculation atom order, including H.
    let order =
        doc.science
            .attachments
            .iter()
            .filter(|attachment| {
                attachment.frame == hash
                    || doc.science.results.iter().any(|stored| {
                        stored.attachment == attachment.id && stored.result.frame == hash
                    })
            })
            .map(|attachment| (true, attachment.atoms.as_slice()))
            .chain(
                doc.science
                    .frame(&hash)
                    .map(|frame| (false, frame.drawing_atoms.as_slice())),
            )
            .filter(|(_, atoms)| {
                atoms.len() == geometry.frame.numbers.len()
                    && atoms.iter().flatten().all(|id| ids.contains(id))
            })
            .map(|(attachment, atoms)| (atoms.iter().flatten().count(), attachment, atoms))
            .filter(|(score, _, _)| *score > 0)
            .max_by_key(|(score, attachment, _)| (*score, *attachment))
            .map(|(_, _, atoms)| atoms);
    if let Some(order) = order {
        let pair = |&[a, b]: &[usize; 2]| {
            let (a, b) = ((*order.get(a)?)?, (*order.get(b)?)?);
            [Role::Reactant, Role::Product]
                .into_iter()
                .find_map(|side| {
                    Some([
                        on_side(doc, reaction, a, side)?,
                        on_side(doc, reaction, b, side)?,
                    ])
                })
        };
        return Changes {
            forming: geometry.forming.iter().filter_map(pair).collect(),
            breaking: geometry.breaking.iter().filter_map(pair).collect(),
        };
    }
    // An older light drawing may omit every calculation frame. The reviewed
    // atom map still identifies the same chemical changes without guessing an
    // element-sorted order for the owned model.
    reactions::ts_plan::map_review(doc, reaction.arrow)
        .ok()
        .filter(|plan| {
            plan.form.len() == geometry.forming.len()
                && plan.break_.len() == geometry.breaking.len()
        })
        .map_or_else(Changes::default, |plan| Changes {
            forming: plan.form.into_iter().map(|(a, b)| [a, b]).collect(),
            breaking: plan.break_.into_iter().map(|(a, b)| [a, b]).collect(),
        })
}

fn on_side(doc: &Document, reaction: &Reaction, id: u64, role: Role) -> Option<u64> {
    let side = side_ids(reaction, role);
    if side.contains(&id) {
        return Some(id);
    }
    let atom = doc.atom(id)?;
    if atom.map_num == 0 {
        return None;
    }
    let mut matches = side
        .into_iter()
        .filter_map(|id| doc.atom(id))
        .filter(|other| {
            other.map_num == atom.map_num
                && other.element == atom.element
                && other.isotope == atom.isotope
        });
    let matched = matches.next()?.id;
    matches.next().is_none().then_some(matched)
}

fn path(commands: Vec<PathCommand>, hue: Hue, dashed: bool, doc: &Document) -> Primitive {
    Primitive::Path {
        commands,
        style: GraphicStyle {
            stroke: Color::Palette(hue, Row::Strong),
            width_pt: (doc.drawing_style.line_width_pt * 1.8).clamp(0.8, 12.),
            pattern: if dashed {
                LinePattern::Dashed
            } else {
                LinePattern::Solid
            },
            ..GraphicStyle::default()
        },
        filled: false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(in crate::app) struct FrameRenderer {
    doc: Document,
    graphic: Graphic,
    artwork: Artwork,
    base: Vec<Primitive>,
    bounds: (Point, Point),
    size: u16,
    transparent: bool,
    skip: Option<usize>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FrameRenderer {
    pub(in crate::app) fn new(
        doc: &Document,
        id: u64,
        size: u16,
        transparent: bool,
    ) -> Result<Self, String> {
        use reshiki::models3d::{self, Display};

        if ![480, 720, 1080].contains(&size) {
            return Err("Choose a movie size of 480, 720 or 1080 pixels".into());
        }
        let doc = if transparent {
            doc.clone()
        } else {
            reshiki::export::Background::Color([255; 3])
                .apply(doc)
                .0
                .into_owned()
        };
        let graphic = doc
            .graphics
            .iter()
            .find(|g| g.id == id && g.energy_profile.is_some())
            .cloned()
            .ok_or("The energy profile is no longer available")?;
        let artwork = Artwork::new(&doc, &graphic);
        let mut selected = reshiki::editing::selection(&doc, &artwork.selection);
        // Selection may omit a model with links outside the movie. Use the
        // source's 3D-only models to remove the ink they hide before rendering.
        let selected_atoms: BTreeSet<_> = selected.atoms.iter().map(|atom| atom.id).collect();
        let hiding_models: Vec<_> = doc
            .models
            .iter()
            .filter(|model| {
                model.display == Display::ThreeD
                    && model
                        .linked_atoms
                        .iter()
                        .any(|id| selected_atoms.contains(id))
            })
            .cloned()
            .collect();
        if !hiding_models.is_empty() {
            selected.models = hiding_models;
            selected = models3d::drawn(&selected).into_owned();
        }
        // Use the drawn scheme and the 3D pictures owned by the profile.
        selected.models.clear();
        // A copied child normally leaves its table behind. Movies retain only
        // the selected children's clipping scopes, without the other cells.
        let retained: BTreeSet<_> = selected.object_ids().collect();
        for owner in doc.graphics.iter().filter(|g| g.table.is_some()) {
            let mut owner = owner.clone();
            if let Some(table) = &mut owner.table {
                for cell in &mut table.cells {
                    cell.members.retain(|id| retained.contains(id));
                    cell.borders.clear();
                }
                table.cells.retain(|cell| !cell.members.is_empty());
                if !table.cells.is_empty() {
                    selected.graphics.push(owner);
                }
            }
        }
        // The selection operation intentionally strips calculations from
        // figures; mapping has already been cached from the full snapshot.
        selected.science = Default::default();
        let base = scene::primitives(&selected);
        let (lo, hi) = scene::bounds(&base);
        let span = (hi.x - lo.x).max(hi.y - lo.y);
        if !lo.is_finite() || !hi.is_finite() || !span.is_finite() || span <= 0. {
            return Err("The energy profile has an invalid frame".into());
        }
        let padding = (span * 0.04).max(doc.drawing_style.bond_length_world * 0.5);
        let bounds = (lo.offset(-padding, -padding), hi.offset(padding, padding));
        Ok(Self {
            doc: selected,
            graphic,
            artwork,
            base,
            bounds,
            size,
            transparent,
            skip: None,
        })
    }

    /// Exactly `size × size` straight-alpha RGBA pixels for either encoder.
    pub(in crate::app) fn render(
        &mut self,
        state: usize,
        sample: Option<(usize, EnergyProfileSample)>,
        geometry: Option<&EnergyProfileGeometry>,
        highlight: bool,
    ) -> Result<Vec<u8>, String> {
        let skip = geometry.map(|_| {
            sample
                .and_then(|(index, _)| {
                    self.graphic
                        .energy_profile
                        .as_deref()?
                        .steps
                        .get(index)?
                        .transition
                })
                .unwrap_or(state)
        });
        if skip != self.skip {
            self.base = if let Some(state) = skip {
                scene::primitives_without_profile_picture(&self.doc, self.graphic.id, state)
            } else {
                scene::primitives(&self.doc)
            };
            self.skip = skip;
        }
        let mut drawing = self.base.clone();
        drawing.extend(self.artwork.overlay(
            &self.doc,
            &self.graphic,
            state,
            sample,
            geometry,
            highlight,
        ));
        let (lo, hi) = self.bounds;
        let part = scene::svg_part_on(
            &drawing,
            lo,
            hi,
            self.doc.canvas_theme,
            (!self.transparent).then_some([255; 3]),
        );
        let (_, body) = part.split_once('>').ok_or("Invalid energy profile SVG")?;
        let body = body
            .strip_suffix("</svg>")
            .ok_or("Invalid energy profile SVG")?;
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 {size} {size}\"><svg width=\"{size}\" height=\"{size}\" viewBox=\"{} {} {} {}\">{body}</svg></svg>",
            lo.x,
            lo.y,
            hi.x - lo.x,
            hi.y - lo.y,
            size = self.size,
        );
        crate::app::models3d::thumbnail::raster_animation_frame(
            svg,
            u32::from(self.size),
            f32::from(self.size),
            self.transparent,
        )
    }
}

#[cfg(test)]
mod tests;
