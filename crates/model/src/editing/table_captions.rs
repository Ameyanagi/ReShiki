//! Preserve readable cell ink when its owning table is transformed.
use crate::{
    document::{Annotation, Document, Point},
    graphics::table::Frame,
    scene::table::Layout,
    typography::TextAlign,
};
use std::collections::{HashMap, HashSet};

pub(super) struct Positions(HashMap<u64, Caption>);

impl Positions {
    pub(super) fn new(doc: &Document, selected: &HashSet<u64>) -> Self {
        let members: HashMap<_, _> = doc
            .graphics
            .iter()
            .filter(|g| selected.contains(&g.id))
            .flat_map(|g| {
                g.table.iter().flat_map(move |t| {
                    t.cells
                        .iter()
                        .filter_map(move |c| c.frame(g).map(|frame| (c, frame)))
                })
            })
            .flat_map(|(c, frame)| c.members.iter().map(move |&id| (id, frame)))
            .collect();
        let annotations: Vec<_> = doc
            .annotations
            .iter()
            .filter(|a| selected.contains(&a.id) && members.contains_key(&a.id))
            .collect();
        if annotations.is_empty() {
            return Self(HashMap::new());
        }
        let cells = Layout::new(doc);
        Self(
            // Raw imported or independently moved overflow is retained;
            // every nonempty glyph quad must already fit its cell clip.
            annotations
                .into_iter()
                .filter_map(|a| {
                    Ink::new(&cells, a).filter(Ink::fits).map(|ink| {
                        (
                            a.id,
                            Caption {
                                ink,
                                frame: members[&a.id],
                            },
                        )
                    })
                })
                .collect(),
        )
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn get(&self, id: u64) -> Option<&Caption> {
        self.0.get(&id)
    }
}

pub(super) struct Caption {
    ink: Ink,
    frame: Frame,
}

struct Ink {
    position: Point,
    centre: Point,
    pivot: Point,
    angle: f32,
    visible: usize,
    total: usize,
    bounds: (Point, Point),
}

impl Ink {
    fn new(cells: &Layout, a: &Annotation) -> Option<Self> {
        let effective = cells.annotation(a);
        let layout = crate::typography::cell_layout(&a.text, &effective.format);
        let pivot = Point::new(layout.width / 2., layout.height / 2.);
        let mut bounds: Option<(Point, Point)> = None;
        let mut visible = 0;
        let mut total = 0;
        for run in layout.fragments {
            for (lo, hi) in crate::style::text_ink_boxes(&run.text, run.style.size(), &run.style) {
                if hi.x <= lo.x || hi.y <= lo.y {
                    continue;
                }
                total += 1;
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
            total,
            bounds: (lo, hi),
        })
    }

    fn fits(&self) -> bool {
        self.total > 0 && self.visible == self.total
    }

    fn offset(&self, angle: f32) -> Point {
        self.centre.turned(self.pivot, angle)
    }

    fn world_centre(&self) -> Point {
        let p = self.offset(self.angle);
        self.position.offset(p.x, p.y)
    }
}

impl Caption {
    pub(super) fn place(
        &self,
        a: &mut Annotation,
        cells: &Layout,
        convert: &impl Fn(Point) -> Point,
        vector: &impl Fn(Point) -> Point,
    ) {
        let target = convert(self.ink.world_centre());
        let frame = Frame {
            origin: convert(self.frame.origin),
            axis_x: vector(self.frame.axis_x),
            axis_y: vector(self.frame.axis_y),
        };
        // Leaving the clip would disable Auto cell wrapping. That is not an
        // equivalent way to reach an otherwise unattainable aligned centre.
        let keep_origin = a.format.width_pt.is_none() && in_cell(self.frame, self.ink.position);
        let admissible = |p| !keep_origin || in_cell(frame, p);
        let source = self.source_inset(a, frame, target).filter(|&position| {
            a.position = position;
            Ink::new(cells, a).is_some_and(|ink| ink.fits())
        });
        let offset = self.ink.offset(a.angle);
        a.position = target.offset(-offset.x, -offset.y);
        let mut best = (a.position, 0, f32::INFINITY);
        let mut visited: Vec<Point> = vec![];
        let mut previous: Option<Point> = None;
        // The centre is only piecewise affine in the insertion point. In
        // particular Auto Right can have zero horizontal slope, and Auto
        // Center slope 1/2. These are bounded proposals, not a convergence
        // claim across line breaks or an over-wide indivisible glyph.
        for _ in 0..8 {
            if visited.iter().any(|p| p.distance(a.position) < 0.0001) {
                break;
            }
            visited.push(a.position);
            let Some(ink) = Ink::new(cells, a) else { break };
            let centre = ink.world_centre();
            let error = centre.distance(target);
            if admissible(a.position)
                && (ink.visible > best.1 || (ink.visible == best.1 && error < best.2))
            {
                best = (a.position, ink.visible, error);
            }
            if admissible(a.position) && ink.fits() && error < 0.0001 {
                return;
            }
            if previous.is_some_and(|p| p.distance(centre) < 0.0001) {
                break;
            }
            previous = Some(centre);
            let mut dx = target.x - centre.x;
            if a.angle == 0.
                && a.format.width_pt.is_none()
                && axis_aligned_bounds(frame).is_some()
                && in_cell(frame, a.position)
            {
                match a.format.alignment {
                    TextAlign::Right => dx = 0.,
                    TextAlign::Center => dx *= 2.,
                    _ => {}
                }
            }
            let next = a.position.offset(dx, target.y - centre.y);
            if !next.is_finite() {
                break;
            }
            a.position = next;
        }
        // Keep the original Auto wrap and alignment when the transformed
        // centre cannot be represented by an in-cell insertion point.
        if let Some(position) = source {
            a.position = position;
            return;
        }
        a.position = convert(self.ink.position);
        if let Some(ink) = Ink::new(cells, a) {
            let error = ink.world_centre().distance(target);
            if ink.visible > best.1 || (ink.visible == best.1 && error < best.2) {
                return;
            }
        }
        a.position = best.0;
    }

    fn source_inset(&self, a: &Annotation, frame: Frame, target: Point) -> Option<Point> {
        if self.ink.angle != 0. || a.angle != 0. {
            return None;
        }
        let (old_lo, old_hi) = axis_aligned_bounds(self.frame)?;
        let (lo, hi) = axis_aligned_bounds(frame)?;
        let old_size = Point::new(old_hi.x - old_lo.x, old_hi.y - old_lo.y);
        let size = Point::new(hi.x - lo.x, hi.y - lo.y);
        if old_size.distance(size) > old_size.x.max(old_size.y) * 0.00001
            || !in_cell(self.frame, self.ink.position)
        {
            return None;
        }
        // Either world edge can be the excluded local 1 edge after reflection.
        // Keep the insertion point just inside without materialising a width.
        let inset = size.x.max(size.y) * 0.000001;
        let min_y = (lo.y + inset).max(lo.y - self.ink.bounds.0.y);
        let max_y = (hi.y - inset).min(hi.y - self.ink.bounds.1.y);
        if min_y > max_y {
            return None;
        }
        Some(Point::new(
            (lo.x + self.ink.position.x - old_lo.x).clamp(lo.x + inset, hi.x - inset),
            (target.y - self.ink.centre.y).clamp(min_y, max_y),
        ))
    }
}

fn in_cell(frame: Frame, position: Point) -> bool {
    frame
        .local(position)
        .is_some_and(|p| (0. ..1.).contains(&p.x) && (0. ..1.).contains(&p.y))
}

fn axis_aligned_bounds(frame: Frame) -> Option<(Point, Point)> {
    let tolerance = frame
        .axis_x
        .distance(Point::default())
        .max(frame.axis_y.distance(Point::default()))
        * 0.000001;
    if (frame.axis_x.x.abs() > tolerance && frame.axis_x.y.abs() > tolerance)
        || (frame.axis_y.x.abs() > tolerance && frame.axis_y.y.abs() > tolerance)
    {
        return None;
    }
    let corners = frame.corners();
    Some(
        corners[1..]
            .iter()
            .fold((corners[0], corners[0]), |(lo, hi), p| {
                (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            }),
    )
}

#[cfg(test)]
mod tests;
