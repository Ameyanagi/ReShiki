//! Align the models of a reaction beneath its drawing.

use super::{Display, InkBox, Model3d, frame, frame_with_panels, require_version};
use crate::{
    document::{Document, Point},
    reactions::{self, Role},
    scene,
};
use std::{borrow::Cow, collections::HashSet};

/// Arrange the models of one reaction. A transient path model can occupy the
/// arrow's slot alongside the original species models.
pub fn place_reaction_row(doc: &mut Document, arrow: u64, path_model: Option<usize>) {
    let (Some(arrow), Ok(reaction)) = (
        doc.arrows.iter().find(|a| a.id == arrow),
        reactions::preview(doc, arrow),
    ) else {
        return;
    };
    let path_model = path_model.filter(|&index| index < doc.models.len());
    let bond = doc.drawing_style.bond_length_world;
    let species: Vec<_> = Role::ALL
        .into_iter()
        .flat_map(|role| {
            let atoms: Vec<_> = reaction
                .participants(role)
                .iter()
                .flat_map(|p| p.atoms.iter().copied())
                .collect();
            reactions::molecules(doc, &atoms)
                .into_iter()
                .map(move |atoms| (role, atoms))
        })
        .collect();
    let mut used = path_model.into_iter().collect::<HashSet<_>>();
    let models: Vec<_> = species
        .iter()
        .map(|(_, atoms)| {
            let index = doc.models.iter().enumerate().position(|(i, model)| {
                !used.contains(&i)
                    && !model.linked_atoms.is_empty()
                    && model.linked_atoms.iter().all(|id| atoms.contains(id))
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
    for &i in &used {
        if drawing
            .models
            .get(i)
            .is_some_and(|m| m.display == Display::ThreeD)
            && let Some(model) = drawing.to_mut().models.get_mut(i)
        {
            model.display = Display::Both;
        }
    }
    let scheme = crate::molecule_names::include_annotations(&drawing, &reaction.ids());
    let mut selections: Vec<_> = species.iter().map(|(_, atoms)| atoms.clone()).collect();
    let mut seen: HashSet<_> = selections.iter().flatten().copied().collect();
    selections.push(scheme.into_iter().filter(|id| seen.insert(*id)).collect());
    let outside = selections.len();
    let atoms: Vec<_> = drawing
        .atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| !seen.contains(id))
        .collect();
    selections.extend(
        reactions::molecules(&drawing, &atoms)
            .into_iter()
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

    // Older schemes may have plain text in the arrow's condition slots and
    // loose '+' signs between species on the same side.
    for (ids, measured) in selections.iter().zip(&bounds).skip(objects) {
        let (&[id], Some((lo, hi))) = (ids.as_slice(), measured) else {
            continue;
        };
        let text = drawing.annotations.iter().find(|a| a.id == id);
        let plus = text.is_some_and(|a| a.text.trim() == "+")
            || drawing.graphics.iter().any(|g| {
                g.id == id
                    && g.kind
                        == crate::graphics::GraphicKind::Symbol(crate::scientific::SymbolKind::Plus)
            });
        let belongs = if plus {
            Role::ALL.into_iter().any(|role| {
                let mut side = species
                    .iter()
                    .zip(&bounds)
                    .filter_map(|((r, _), b)| (*r == role).then_some((*b)?));
                let Some(first) = side.next() else {
                    return false;
                };
                let last = side.fold(first, |(a, b), (lo, hi)| {
                    (
                        Point::new(a.x.min(lo.x), a.y.min(lo.y)),
                        Point::new(b.x.max(hi.x), b.y.max(hi.y)),
                    )
                });
                let x = (lo.x + hi.x) / 2.;
                (last.0.x..=last.1.x).contains(&x)
                    && lo.y <= last.1.y + bond
                    && hi.y >= last.0.y - bond
                    && species.iter().filter(|(r, _)| *r == role).count() > 1
            })
        } else {
            text.is_some_and(|text| {
                !drawing
                    .arrow_labels
                    .iter()
                    .any(|label| label.annotation == id)
                    && drawing
                        .arrows
                        .iter()
                        .filter(|a| a.purpose.is_chemical())
                        .filter_map(|a| {
                            crate::arrow_labels::slot(&drawing, a, text).map(|(_, d)| (a.id, d))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .is_some_and(|(id, _)| id == arrow.id)
            })
        };
        if belongs {
            bottom = bottom.max(hi.y);
        }
    }

    let mut slots: Vec<_> = species
        .iter()
        .zip(&bounds)
        .zip(models)
        .filter_map(|(((role, _), bounds), model)| {
            (*role != Role::Agent || model.is_some()).then_some(slot((*bounds)?, model))
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

    let obstacles = bounds.iter().skip(outside).copied().flatten().chain(
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
