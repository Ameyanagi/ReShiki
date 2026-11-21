//! Preserve readable cell ink when its owning table is transformed.
use crate::{
    document::{Annotation, Document, Point},
    scene::table::Layout,
};
use std::collections::{HashMap, HashSet};

pub(super) struct Positions(HashMap<u64, Ink>);

impl Positions {
    pub(super) fn new(doc: &Document, selected: &HashSet<u64>) -> Self {
        let members: HashSet<_> = doc
            .graphics
            .iter()
            .filter(|g| selected.contains(&g.id))
            .filter_map(|g| g.table.as_ref())
            .flat_map(|t| &t.cells)
            .flat_map(|c| c.members.iter().copied())
            .collect();
        if members.is_empty() {
            return Self(HashMap::new());
        }
        let cells = Layout::new(doc);
        Self(
            doc.annotations
                .iter()
                .filter(|a| selected.contains(&a.id) && members.contains(&a.id))
                // Raw imported or independently moved overflow is retained;
                // this only preserves captions that currently paint in-cell.
                .filter_map(|a| {
                    Ink::new(&cells, a)
                        .filter(|ink| ink.visible > 0)
                        .map(|ink| (a.id, ink))
                })
                .collect(),
        )
    }

    pub(super) fn get(&self, id: u64) -> Option<&Ink> {
        self.0.get(&id)
    }
}

pub(super) struct Ink {
    position: Point,
    centre: Point,
    pivot: Point,
    angle: f32,
    visible: usize,
}

impl Ink {
    fn new(cells: &Layout, a: &Annotation) -> Option<Self> {
        let effective = cells.annotation(a);
        let layout = crate::typography::cell_layout(&a.text, &effective.format);
        let pivot = Point::new(layout.width / 2., layout.height / 2.);
        let mut bounds: Option<(Point, Point)> = None;
        let mut visible = 0;
        for run in layout.fragments {
            for (lo, hi) in crate::style::text_ink_boxes(&run.text, run.style.size(), &run.style) {
                let lo = run.position.offset(lo.x, lo.y);
                let hi = run.position.offset(hi.x, hi.y);
                bounds = Some(bounds.map_or((lo, hi), |(min, max)| {
                    (
                        Point::new(min.x.min(lo.x), min.y.min(lo.y)),
                        Point::new(max.x.max(hi.x), max.y.max(hi.y)),
                    )
                }));
                if [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)]
                    .into_iter()
                    .all(|p| {
                        let p = p.turned(pivot, a.angle);
                        cells.contains(a.id, a.position.offset(p.x, p.y))
                    })
                {
                    visible += 1;
                }
            }
        }
        let (lo, hi) = bounds?;
        Some(Self {
            position: a.position,
            centre: Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            pivot,
            angle: a.angle,
            visible,
        })
    }

    fn offset(&self, angle: f32) -> Point {
        self.centre.turned(self.pivot, angle)
    }

    fn world_centre(&self) -> Point {
        let p = self.offset(self.angle);
        self.position.offset(p.x, p.y)
    }

    pub(super) fn place(
        &self,
        a: &mut Annotation,
        cells: &Layout,
        convert: &impl Fn(Point) -> Point,
    ) {
        let target = convert(self.world_centre());
        let offset = self.offset(a.angle);
        a.position = target.offset(-offset.x, -offset.y);
        let mut best = (a.position, 0, f32::INFINITY);
        // Wrapping and alignment are derived from the new insertion point.
        // Re-evaluate that same paint layout without storing its width. At a
        // line-break discontinuity retain the most visible, nearest candidate.
        for _ in 0..16 {
            let Some(ink) = Self::new(cells, a) else {
                break;
            };
            let centre = ink.world_centre();
            let error = centre.distance(target);
            if ink.visible > best.1 || (ink.visible == best.1 && error < best.2) {
                best = (a.position, ink.visible, error);
            }
            if error < 0.0001 {
                break;
            }
            let next = a.position.offset(target.x - centre.x, target.y - centre.y);
            if !next.is_finite() {
                break;
            }
            a.position = next;
        }
        a.position = best.0;
    }
}

#[cfg(test)]
mod tests;
