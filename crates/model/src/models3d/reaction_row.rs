//! Align the models of a reaction beneath its drawing.

use super::{Display, InkBox, Model3d, frame, frame_with_panels, require_version};
use crate::{
    document::{Document, Point},
    reactions::{self, Role},
    scene,
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

/// Shared intermediates keep their neighboring arrows in the same model row.
/// Preview each arrow once so inferred schemes use the same roles as a run.
fn connected_reactions(
    doc: &Document,
    reaction: reactions::Reaction,
    part_of: &HashMap<u64, usize>,
) -> Vec<reactions::Reaction> {
    let arrow = reaction.arrow;
    let candidates: Vec<_> = std::iter::once(reaction)
        .chain(
            doc.arrows
                .iter()
                .filter(|a| a.id != arrow && a.purpose.is_chemistry())
                .filter_map(|a| reactions::preview(doc, a.id).ok()),
        )
        .collect();
    let endpoints: Vec<HashSet<_>> = candidates
        .iter()
        .map(|reaction| {
            [Role::Reactant, Role::Product]
                .into_iter()
                .flat_map(|role| reaction.participants(role))
                .flat_map(|p| &p.atoms)
                .filter_map(|id| part_of.get(id).copied())
                .collect()
        })
        .collect();
    let mut neighbors = HashMap::<usize, Vec<usize>>::new();
    for (index, parts) in endpoints.iter().enumerate() {
        for &part in parts {
            neighbors.entry(part).or_default().push(index);
        }
    }
    let mut included = HashSet::from([0]);
    let mut pending: Vec<_> = endpoints.first().into_iter().flatten().copied().collect();
    while let Some(part) = pending.pop() {
        for index in neighbors.remove(&part).into_iter().flatten() {
            if included.insert(index) {
                pending.extend(endpoints.get(index).into_iter().flatten().copied());
            }
        }
    }
    candidates
        .into_iter()
        .enumerate()
        .filter_map(|(index, reaction)| included.contains(&index).then_some(reaction))
        .collect()
}

/// Arrange the endpoint models of a connected reaction scheme. A transient
/// path model can occupy this arrow's slot alongside the original species.
pub fn place_reaction_row(doc: &mut Document, arrow: u64, path_model: Option<usize>) {
    let (Some(arrow), Ok(reaction)) = (
        doc.arrows.iter().find(|a| a.id == arrow),
        reactions::preview(doc, arrow),
    ) else {
        return;
    };
    let path_model = path_model.filter(|&index| index < doc.models.len());
    let bond = doc.drawing_style.bond_length_world;
    let atoms: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let parts = reactions::molecules(doc, &atoms);
    let part_of: HashMap<_, _> = parts
        .iter()
        .enumerate()
        .flat_map(|(index, atoms)| atoms.iter().map(move |&id| (id, index)))
        .collect();
    let reactions = connected_reactions(doc, reaction, &part_of);
    let arrows: HashSet<_> = reactions.iter().map(|r| r.arrow).collect();
    let labels: HashMap<_, _> = doc
        .arrow_labels
        .iter()
        .map(|label| (label.annotation, label.arrow))
        .collect();
    let mut endpoints = HashSet::new();
    let mut members = HashSet::new();
    for reaction in &reactions {
        for role in Role::ALL {
            for part in reaction
                .participants(role)
                .iter()
                .flat_map(|p| &p.atoms)
                .filter_map(|id| part_of.get(id).copied())
            {
                members.insert(part);
                if role != Role::Agent {
                    endpoints.insert(part);
                }
            }
        }
    }
    // Document order makes the shared row independent of the selected arrow.
    let species: Vec<_> = parts
        .iter()
        .enumerate()
        .filter(|(index, _)| members.contains(index))
        .collect();
    let mut used = path_model.into_iter().collect::<HashSet<_>>();
    let models: Vec<_> = species
        .iter()
        .map(|(part, atoms)| {
            if !endpoints.contains(part) {
                return None;
            }
            let index = doc.models.iter().enumerate().position(|(i, model)| {
                !used.contains(&i)
                    && model.linked_atoms.len() == atoms.len()
                    && atoms.iter().all(|id| model.linked_atoms.contains(id))
            })?;
            used.insert(index);
            Some(index)
        })
        .collect();
    if used.is_empty() {
        return;
    }

    // Measure the species as drawings even when a model has been hiding one.
    let mut drawing = Cow::Borrowed(&*doc);
    for (i, model) in doc.models.iter().enumerate() {
        if model.display == Display::ThreeD
            && (used.contains(&i)
                || model
                    .linked_atoms
                    .iter()
                    .any(|id| part_of.get(id).is_some_and(|part| endpoints.contains(part))))
            && let Some(model) = drawing.to_mut().models.get_mut(i)
        {
            model.display = Display::Both;
        }
    }
    let scheme: Vec<_> = reactions
        .iter()
        .flat_map(|r| r.ids())
        .filter(|id| labels.get(id).is_none_or(|owner| arrows.contains(owner)))
        .collect();
    let scheme = crate::molecule_names::include_annotations(&drawing, &scheme);
    let mut selections: Vec<_> = species.iter().map(|(_, atoms)| atoms.to_vec()).collect();
    let mut seen: HashSet<_> = selections.iter().flatten().copied().collect();
    selections.push(scheme.into_iter().filter(|id| seen.insert(*id)).collect());
    let outside = selections.len();
    selections.extend(
        parts
            .iter()
            .enumerate()
            .filter(|(index, _)| !members.contains(index))
            .map(|(_, ids)| ids.iter().copied())
            .map(|ids| ids.into_iter().filter(|id| seen.insert(*id)).collect()),
    );
    let objects = selections.len();
    selections.extend(
        drawing
            .object_ids()
            .filter(|id| !seen.contains(id))
            .map(|id| vec![id]),
    );
    let bounds = scene::selections_ink_bounds(&drawing, &selections);
    let mut bottom = bounds
        .iter()
        .take(outside)
        .flatten()
        .map(|(_, hi)| hi.y)
        .fold(arrow.bounds().1.y, f32::max);

    let species_bounds: HashMap<_, _> = species
        .iter()
        .zip(&bounds)
        .filter_map(|((part, _), bounds)| Some((*part, (*bounds)?)))
        .collect();
    let mut sides = Vec::new();
    for reaction in &reactions {
        for role in Role::ALL {
            let parts: HashSet<_> = reaction
                .participants(role)
                .iter()
                .flat_map(|p| &p.atoms)
                .filter_map(|id| part_of.get(id).copied())
                .collect();
            sides.push(
                parts
                    .into_iter()
                    .filter_map(|part| species_bounds.get(&part).copied())
                    .collect(),
            );
        }
    }
    if let Some(legacy_bottom) = legacy_scheme_bottom(
        &drawing,
        &arrows,
        &labels,
        &sides,
        selections
            .iter()
            .zip(&bounds)
            .skip(objects)
            .filter_map(|(ids, bounds)| {
                let &[id] = ids.as_slice() else { return None };
                Some((id, (*bounds)?))
            }),
    ) {
        bottom = bottom.max(legacy_bottom);
    }

    let mut slots: Vec<_> = species
        .iter()
        .zip(&bounds)
        .zip(models)
        .filter_map(|(((part, _), bounds), model)| {
            endpoints.contains(part).then_some(slot((*bounds)?, model))
        })
        .collect();
    if path_model.is_some() {
        slots.push(slot(arrow.bounds(), path_model));
    }
    slots.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut row = Vec::new();
    for (i, &(x, width, index)) in slots.iter().enumerate() {
        let Some(index) = index else { continue };
        let Some(mut model) = doc.models.get(index).cloned() else {
            continue;
        };
        let left = i
            .checked_sub(1)
            .and_then(|j| slots.get(j))
            .map(|slot| x - slot.0);
        let right = slots.get(i + 1).map(|slot| slot.0 - x);
        let span = match (left, right) {
            (Some(left), Some(right)) => (left + right) / 2.,
            (Some(gap), None) | (None, Some(gap)) => gap,
            (None, None) => f32::INFINITY,
        };
        model.offset = Point::default();
        if let Some(item) = RowModel::fit(doc, index, model, x, span.max(width)) {
            row.push(item);
        }
    }
    // The slot may be asymmetric or wider than the midpoint span. Fit two
    // adjacent frames into their shared space while keeping both centred.
    if let Some((mut left, rest)) = row.split_first_mut() {
        for right in rest {
            let gap = right.x - left.x;
            let (a, b) = (left.width(), right.width());
            if a + b > 2. * gap {
                left.shrink(doc, gap.max(2. * gap - b));
                right.shrink(doc, gap.max(2. * gap - a));
            }
            left = right;
        }
    }
    // Coincident centres (or minimum-size frames) cannot share the same x.
    // Keep the path on its arrow and make only the necessary clearance.
    let pivot = row
        .iter()
        .position(|m| Some(m.index) == path_model)
        .unwrap_or(0);
    let (left, right) = row.split_at_mut(pivot);
    let Some((center, right)) = right.split_first_mut() else {
        return;
    };
    let mut edge = center.x - center.half;
    for model in left.iter_mut().rev() {
        model.x = model.x.min(edge - model.half);
        edge = model.x - model.half;
    }
    let mut edge = center.x + center.half;
    for model in right {
        model.x = model.x.max(edge + model.half);
        edge = model.x + model.half;
    }

    let bond_obstacles = zero_order_obstacles(&drawing);
    let obstacles = bounds
        .iter()
        .skip(outside)
        .copied()
        .flatten()
        .chain(bond_obstacles)
        .chain(
            doc.models
                .iter()
                .enumerate()
                .filter(|(i, m)| !used.contains(i) && m.display != Display::TwoD)
                .filter_map(|(_, m)| frame_with_panels(doc, m)),
        );
    let mut blocked: Vec<_> = obstacles
        .filter_map(|(lo, hi)| {
            let height = row
                .iter()
                .filter(|m| m.x + m.width() / 2. > lo.x && m.x - m.width() / 2. < hi.x)
                .map(|m| m.height)
                .reduce(f32::max)?;
            Some((lo.y - height, hi.y))
        })
        .collect();
    blocked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut top = bottom + bond;
    for (first, last) in blocked {
        if first < top && top < last {
            top = last + bond;
        }
    }
    for item in row {
        let Some(model) = doc.models.get_mut(item.index) else {
            continue;
        };
        model.display = Display::Both;
        model.offset = Point::new(item.x - item.origin.x, top - item.origin.y);
        model.size = item.model.size;
    }
    require_version(doc);
}

/// Older schemes may have plain text in condition slots and loose '+' signs
/// between species on the same side. Explicit arrow ownership takes priority.
fn legacy_scheme_bottom(
    doc: &Document,
    arrows: &HashSet<u64>,
    labels: &HashMap<u64, u64>,
    sides: &[Vec<InkBox>],
    objects: impl Iterator<Item = (u64, InkBox)>,
) -> Option<f32> {
    let texts: HashMap<_, _> = doc.annotations.iter().map(|a| (a.id, a)).collect();
    let plus: HashSet<_> = doc
        .annotations
        .iter()
        .filter(|a| a.text.trim() == "+")
        .map(|a| a.id)
        .chain(doc.graphics.iter().filter_map(|g| {
            (g.kind == crate::graphics::GraphicKind::Symbol(crate::scientific::SymbolKind::Plus))
                .then_some(g.id)
        }))
        .collect();
    let sides: Vec<_> = sides
        .iter()
        .filter(|side| side.len() > 1)
        .filter_map(|side| {
            let mut side = side.clone();
            side.sort_by(|(a, _), (b, _)| a.x.total_cmp(&b.x));
            let (&(lo, hi), rest) = side.split_first()?;
            let (mut low, mut high) = (lo.y, hi.y);
            let mut edge = hi.x;
            let mut gaps = Vec::new();
            for &(lo, hi) in rest {
                low = low.min(lo.y);
                high = high.max(hi.y);
                if edge < lo.x {
                    gaps.push((edge, lo.x));
                }
                edge = edge.max(hi.x);
            }
            (!gaps.is_empty()).then_some((low, high, gaps))
        })
        .collect();
    let bond = doc.drawing_style.bond_length_world;
    objects
        .filter_map(|(id, (lo, hi))| {
            let belongs = if let Some(owner) = labels.get(&id) {
                arrows.contains(owner)
            } else if plus.contains(&id) {
                let x = (lo.x + hi.x) / 2.;
                sides.iter().any(|(low, high, gaps)| {
                    lo.y <= high + bond
                        && hi.y >= low - bond
                        && gaps
                            .partition_point(|&(left, _)| left <= x)
                            .checked_sub(1)
                            .and_then(|i| gaps.get(i))
                            .is_some_and(|&(_, right)| x <= right)
                })
            } else {
                texts.get(&id).is_some_and(|text| {
                    doc.arrows
                        .iter()
                        .filter(|a| a.purpose.is_chemical())
                        .filter_map(|a| {
                            crate::arrow_labels::slot(doc, a, text).map(|(_, d)| (a.id, d))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .is_some_and(|(id, _)| arrows.contains(&id))
                })
            };
            belongs.then_some(hi.y)
        })
        .reduce(f32::max)
}

/// Bounds of visible zero-order bonds, including spans between molecules.
pub fn zero_order_obstacles(doc: &Document) -> Vec<InkBox> {
    if !doc.bonds.iter().any(|bond| bond.order == 0) {
        return Vec::new();
    }
    let drawing = doc.abbreviation_render_copy();
    let doc = drawing.as_ref();
    let graph = scene::graph::Graph::new(doc);
    let hidden: HashSet<_> = doc
        .models
        .iter()
        .filter(|model| model.display == Display::ThreeD)
        .flat_map(Model3d::ids)
        .collect();
    doc.bonds
        .iter()
        .enumerate()
        .filter(|(_, bond)| {
            bond.order == 0
                && graph.bond_visible(bond.a, bond.b)
                && !hidden.contains(&bond.a)
                && !hidden.contains(&bond.b)
        })
        .filter_map(|(index, bond)| {
            let (a, b) = graph.ends(index)?;
            let pad = doc.drawing_style.world(if bond.display == "bold" {
                doc.drawing_style.bold_width_pt
            } else {
                doc.drawing_style.line_width_pt
            }) / 2.;
            // Label clipping only removes ink inside the endpoint molecules,
            // whose bounds are already obstacles. Keep the complete span.
            Some((
                Point::new(a.x.min(b.x) - pad, a.y.min(b.y) - pad),
                Point::new(a.x.max(b.x) + pad, a.y.max(b.y) + pad),
            ))
        })
        .collect()
}

fn slot((lo, hi): InkBox, model: Option<usize>) -> (f32, f32, Option<usize>) {
    ((lo.x + hi.x) / 2., hi.x - lo.x, model)
}

struct RowModel {
    index: usize,
    model: Model3d,
    x: f32,
    origin: Point,
    half: f32,
    height: f32,
}

impl RowModel {
    fn width(&self) -> f32 {
        2. * self.half
    }

    fn measure(&mut self, doc: &Document) -> Option<()> {
        let (lo, hi) = frame(doc, &self.model)?;
        let (ink_lo, ink_hi) = if self.model.panels.is_empty() {
            (lo, hi)
        } else {
            frame_with_panels(doc, &self.model)?
        };
        self.origin = Point::new((lo.x + hi.x) / 2., lo.y);
        self.half = (self.origin.x - ink_lo.x).max(ink_hi.x - self.origin.x);
        self.height = ink_hi.y - lo.y;
        Some(())
    }

    fn fit(doc: &Document, index: usize, model: Model3d, x: f32, width: f32) -> Option<Self> {
        let mut item = Self {
            index,
            model,
            x,
            origin: Point::default(),
            half: 0.,
            height: 0.,
        };
        item.measure(doc)?;
        item.shrink(doc, width);
        Some(item)
    }

    fn shrink(&mut self, doc: &Document, width: f32) {
        if self.width() <= width {
            return;
        }
        let mut high = self.model.size;
        let mut low = 1.;
        for _ in 0..14 {
            self.model.size = (low + high) / 2.;
            self.measure(doc);
            if self.width() > width {
                high = self.model.size;
            } else {
                low = self.model.size;
            }
        }
        self.model.size = low;
        self.measure(doc);
    }
}
