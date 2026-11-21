//! Pointer-local table clipping. Index ownership only when a geometric
//! candidate needs it; no document partitions or scenes are built for picking.
use super::{Document, World};
use reshiki::{
    document::Annotation,
    graphics::table::{Cell, Frame},
};
use std::{cell::OnceCell, collections::HashMap};

pub(super) struct CellClips<'a> {
    cells: Vec<(&'a Cell, Option<Frame>)>,
    owners: OnceCell<HashMap<u64, Option<Frame>>>,
}

impl<'a> CellClips<'a> {
    pub(super) fn new(doc: &'a Document) -> Self {
        Self {
            cells: doc
                .graphics
                .iter()
                .filter_map(|g| g.table.as_ref().map(|table| (g, table)))
                .flat_map(|(g, table)| table.cells.iter().map(move |cell| (cell, cell.frame(g))))
                .collect(),
            owners: OnceCell::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    fn owners(&self) -> &HashMap<u64, Option<Frame>> {
        self.owners.get_or_init(|| {
            let mut owners = HashMap::new();
            for &(cell, frame) in &self.cells {
                for &id in &cell.members {
                    // Invalid transient ownership must not leak through a
                    // different cell. Valid documents have unique membership.
                    owners
                        .entry(id)
                        .and_modify(|owner| *owner = None)
                        .or_insert(frame);
                }
            }
            owners
        })
    }

    pub(super) fn allows(&self, id: u64, p: World) -> bool {
        self.is_empty()
            || self
                .owners()
                .get(&id)
                .is_none_or(|frame| frame.is_some_and(|frame| frame.contains(p)))
    }

    fn frame(&self, id: u64) -> Option<Frame> {
        if self.is_empty() {
            return None;
        }
        self.owners().get(&id).copied().flatten()
    }

    pub(super) fn point_visible(&self, id: u64, p: World) -> bool {
        self.allows(id, p)
    }

    pub(super) fn box_hit(&self, id: u64, lo: World, hi: World, p: World, radius: f32) -> bool {
        self.allows(id, p)
            && self.frame(id).is_none_or(|frame| {
                reshiki::graphics::table::clip::box_hit(frame, lo, hi, p, p, radius)
            })
    }

    pub(super) fn part_hit(
        &self,
        id: u64,
        part: &reshiki::scientific::Part,
        p: World,
        radius: f32,
    ) -> bool {
        self.allows(id, p)
            && self.frame(id).is_none_or(|frame| {
                reshiki::graphics::table::clip::part_hit(frame, part, p, p, radius)
            })
    }

    pub(super) fn arrow_hit(
        &self,
        id: u64,
        part: &reshiki::arrows::ArrowPath,
        p: World,
        radius: f32,
    ) -> bool {
        self.allows(id, p)
            && self.frame(id).is_none_or(|frame| {
                reshiki::graphics::table::clip::arrow_hit(frame, part, p, p, radius)
            })
    }

    pub(super) fn line_hit(
        &self,
        id: u64,
        a: World,
        b: World,
        half: f32,
        p: World,
        radius: f32,
    ) -> bool {
        self.allows(id, p)
            && self.frame(id).is_none_or(|frame| {
                reshiki::graphics::table::clip::line_hit(frame, a, b, half, p, p, radius)
            })
    }

    pub(super) fn graphic_hit(
        &self,
        doc: &Document,
        graphic: &reshiki::graphics::Graphic,
        p: World,
        radius: f32,
    ) -> bool {
        if !self.allows(graphic.id, p) {
            return false;
        }
        let Some(frame) = self.frame(graphic.id) else {
            return true;
        };
        let style = doc.drawing_style.text_style();
        let boxes = |lo, hi| reshiki::graphics::table::clip::box_hit(frame, lo, hi, p, p, radius);
        if graphic.cycle.is_some() {
            let drawn = reshiki::cycles::drawn(doc, graphic);
            return drawn
                .arrows
                .iter()
                .flat_map(|arrow| arrow.paths())
                .any(|part| reshiki::graphics::table::clip::arrow_hit(frame, &part, p, p, radius))
                || drawn.labels.iter().any(|(at, text, format)| {
                    let layout = reshiki::typography::layout(text, format);
                    boxes(*at, at.offset(layout.width, layout.height))
                });
        }
        graphic
            .superscript_label(&style)
            .into_iter()
            .chain(graphic.panel_titles(&style))
            .any(|label| label.ink_boxes().into_iter().any(|(lo, hi)| boxes(lo, hi)))
            || graphic
                .panel_parts(&style)
                .iter()
                .chain(graphic.parts().iter())
                .any(|part| {
                    if graphic.kind == reshiki::graphics::GraphicKind::Picture {
                        reshiki::graphics::table::clip::path_hit(
                            frame,
                            &part.commands,
                            0.,
                            true,
                            false,
                            p,
                            p,
                            radius,
                        )
                    } else {
                        reshiki::graphics::table::clip::part_hit(frame, part, p, p, radius)
                    }
                })
    }

    fn layout(&self, annotation: &Annotation) -> Option<reshiki::typography::Layout> {
        if self.is_empty() {
            return None;
        }
        let frame = self.frame(annotation.id)?;
        Some(reshiki::scene::table_annotation_layout(annotation, frame))
    }

    pub(super) fn annotation_contains(&self, annotation: &Annotation, p: World) -> bool {
        if !self.allows(annotation.id, p) {
            return false;
        }
        let Some(layout) = self.layout(annotation) else {
            return annotation.contains(p);
        };
        let origin = annotation.position;
        let p = p.turned(
            origin.offset(layout.width / 2., layout.height / 2.),
            -annotation.angle,
        );
        (origin.x..origin.x + layout.width).contains(&p.x)
            && (origin.y..origin.y + layout.height).contains(&p.y)
    }

    pub(super) fn held_annotation_contains(&self, annotation: &Annotation, p: World) -> bool {
        if !self.is_empty() && self.owners().contains_key(&annotation.id) {
            return self.annotation_contains(annotation, p);
        }
        // Preserve the ordinary held-text hit box; cell text additionally
        // needs its derived dimensions, pivot and affine content boundary.
        let (width, height) = annotation.size();
        (annotation.position.x..annotation.position.x + width).contains(&p.x)
            && (annotation.position.y..annotation.position.y + height).contains(&p.y)
    }

    pub(super) fn annotation_ink_contains(&self, annotation: &Annotation, p: World) -> bool {
        if !self.allows(annotation.id, p) {
            return false;
        }
        let Some(layout) = self.layout(annotation) else {
            let Some((lo, hi)) = reshiki::atom_text::text_ink_bounds(annotation) else {
                return false;
            };
            let p = p.turned(annotation.centre(), -annotation.angle);
            return p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
        };
        let p = p.turned(
            annotation
                .position
                .offset(layout.width / 2., layout.height / 2.),
            -annotation.angle,
        );
        layout.fragments.into_iter().any(|fragment| {
            let origin = annotation
                .position
                .offset(fragment.position.x, fragment.position.y);
            reshiki::style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)
                .into_iter()
                .any(|(lo, hi)| {
                    p.x >= origin.x + lo.x
                        && p.x <= origin.x + hi.x
                        && p.y >= origin.y + lo.y
                        && p.y <= origin.y + hi.y
                })
        })
    }
}

pub(super) fn border_hit(graphic: &reshiki::graphics::Graphic, p: World, radius: f32) -> bool {
    reshiki::graphics::table::clip::border_hit(graphic, p, radius)
}

#[cfg(test)]
mod tests;
