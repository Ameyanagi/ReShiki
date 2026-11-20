//! Noncircle names follow the final forward shaft, with actual painted ink
//! deciding between inside, outside and nearby positions.
use super::super::{Drawn, Outline, grown, shapes::Shaft, wrapped};
use crate::{
    document::{Document, Point},
    graphics::{Graphic, PathCommand},
    style::DEFAULT,
    typography::{TextAlign, TextFormat},
};
use kurbo::{BezPath, Line, ParamCurve, PathSeg, Rect, Shape};

#[cfg(test)]
#[path = "perimeter_name_unit_tests.rs"]
mod tests;

type Box = (Point, Point);

pub(super) struct Name {
    text: String,
    shaft: Option<Shaft>,
    reverse: Option<Shaft>,
    fallback: f64,
}

impl Name {
    pub(super) fn new(text: String, lanes: Vec<Option<Shaft>>, fallback: f64) -> Self {
        // Preserve native lane identity even if one painted shaft is unusable.
        let mut lanes = lanes.into_iter();
        Self {
            text,
            shaft: lanes.next().flatten(),
            reverse: lanes.next().flatten(),
            fallback,
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    point: Point,
    incoming: Point,
    outgoing: Point,
    inward: [f32; 2],
}

impl Frame {
    fn at(name: &Name, perimeter: &Outline, fraction: f64, dir: f32) -> Option<Self> {
        if let Some(shaft) = &name.shaft {
            let s = fraction * shaft.length();
            let (point, incoming) = shaft.sample(s, true)?;
            let (_, outgoing) = shaft.sample(s, false)?;
            let normal = |t: Point| Point::new(dir * t.y, -dir * t.x);
            let (incoming, outgoing) = (normal(incoming), normal(outgoing));
            let inward =
                name.reverse
                    .as_ref()
                    .and_then(|s| s.project(point))
                    .map_or([0., 0.], |p| {
                        [incoming, outgoing].map(|n| {
                            (-(p.point.x - point.x) * n.x - (p.point.y - point.y) * n.y).max(0.)
                        })
                    });
            return Some(Self {
                point,
                incoming,
                outgoing,
                inward,
            });
        }
        let tangent = perimeter.tangents(name.fallback)?;
        // Outline tangents are already canonical clockwise, including after
        // reflection. A missing route's fallback must not reverse them again.
        let normal = |t: Point| Point::new(t.y, -t.x);
        Some(Self {
            point: perimeter.point(name.fallback)?,
            incoming: normal(tangent.incoming),
            outgoing: normal(tangent.outgoing),
            inward: [0., 0.],
        })
    }

    fn offsets(self, width: f32, height: f32, gap: f32, inside: bool) -> Vec<Point> {
        let support = |n: Point| 0.5 * (width * n.x.abs() + height * n.y.abs()) + gap;
        let a = self.incoming;
        let b = self.outgoing;
        let [extra_a, extra_b] = if inside { self.inward } else { [0., 0.] };
        let (ha, hb) = (support(a) + extra_a, support(b) + extra_b);
        let mut out = vec![];
        let determinant = f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x);
        if determinant.abs() > 1e-5 {
            // The text box has a different support radius along each side.
            // Solve both clearances; a unit bisector would round the corner.
            let x = (f64::from(ha) * f64::from(b.y) - f64::from(a.y) * f64::from(hb)) / determinant;
            let y = (f64::from(a.x) * f64::from(hb) - f64::from(ha) * f64::from(b.x)) / determinant;
            // Nearly folded corners need nearby one-sided/alternate-position
            // candidates rather than a name sent far outside the drawing.
            if x.hypot(y) <= 4. * f64::from(ha.max(hb)) {
                out.push(Point::new(x as f32, y as f32));
            }
        }
        out.push(Point::new(b.x * hb, b.y * hb));
        if a.x * b.x + a.y * b.y < 1. - 1e-5 {
            out.push(Point::new(a.x * ha, a.y * ha));
        }
        out
    }
}

struct Ink {
    path: BezPath,
    bounds: Rect,
    width: f32,
    filled: bool,
}

impl Ink {
    fn new(commands: &[PathCommand], width: f32, filled: bool) -> Self {
        let mut path = BezPath::new();
        let p = |p: Point| (f64::from(p.x), f64::from(p.y));
        for c in commands {
            match *c {
                PathCommand::Move(a) => path.move_to(p(a)),
                PathCommand::Line(a) => path.line_to(p(a)),
                PathCommand::Cubic(a, b, c) => path.curve_to(p(a), p(b), p(c)),
                PathCommand::Close => path.close_path(),
            }
        }
        let bounds = path.bounding_box();
        Self {
            path,
            bounds,
            width,
            filled,
        }
    }

    fn hits(&self, ink: Box, gap: f32) -> bool {
        let (lo, hi) = grown(ink, gap + self.width * 0.5);
        let rect = Rect::new(
            f64::from(lo.x),
            f64::from(lo.y),
            f64::from(hi.x),
            f64::from(hi.y),
        );
        if rect.x1 < self.bounds.x0
            || self.bounds.x1 < rect.x0
            || rect.y1 < self.bounds.y0
            || self.bounds.y1 < rect.y0
        {
            return false;
        }
        let contains =
            |p: kurbo::Point| rect.x0 <= p.x && p.x <= rect.x1 && rect.y0 <= p.y && p.y <= rect.y1;
        let corners = [
            kurbo::Point::new(rect.x0, rect.y0),
            kurbo::Point::new(rect.x1, rect.y0),
            kurbo::Point::new(rect.x1, rect.y1),
            kurbo::Point::new(rect.x0, rect.y1),
        ];
        for segment in self.path.segments() {
            if contains(segment.start()) || contains(segment.end()) {
                return true;
            }
            if let PathSeg::Line(line) = segment {
                if crate::atom_labels::segment_hits_rect(
                    Point::new(line.p0.x as f32, line.p0.y as f32),
                    Point::new(line.p1.x as f32, line.p1.y as f32),
                    lo,
                    hi,
                ) {
                    return true;
                }
            } else {
                let bounds = segment.bounding_box();
                // An axis-aligned cubic lying on a rectangle edge has no
                // isolated intersection root, but its full image still hits.
                if (bounds.width() == 0. || bounds.height() == 0.)
                    && bounds.x1 >= rect.x0
                    && bounds.x0 <= rect.x1
                    && bounds.y1 >= rect.y0
                    && bounds.y0 <= rect.y1
                {
                    return true;
                }
                for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)).take(4) {
                    if !segment.intersect_line(Line::new(*a, *b)).is_empty() {
                        return true;
                    }
                }
            }
        }
        self.filled && self.path.winding(rect.center()) != 0
    }
}

fn overlap(a: Box, b: Box) -> f32 {
    (a.1.x.min(b.1.x) - a.0.x.max(b.0.x)).max(0.) * (a.1.y.min(b.1.y) - a.0.y.max(b.0.y)).max(0.)
}

pub(super) fn place(
    doc: &Document,
    g: &Graphic,
    perimeter: &Outline,
    names: Vec<Name>,
    bounds: &[Option<Box>],
    out: &mut Drawn,
) {
    let Some(cycle) = g.cycle.as_deref() else {
        return;
    };
    let gap = DEFAULT.world(cycle.gap_pt);
    let format = TextFormat {
        style: doc.drawing_style.text_style(),
        alignment: TextAlign::Center,
        ..Default::default()
    };
    let center = g.cycle_circle().0;
    let center_label = (!cycle.label.trim().is_empty()).then(|| {
        let layout = crate::typography::layout(&cycle.label, &format);
        let at = center.offset(-layout.width * 0.5, -layout.height * 0.5);
        (at, at.offset(layout.width, layout.height))
    });
    let mut taken: Vec<_> = bounds.iter().flatten().copied().collect();
    if let Some(b) = center_label {
        taken.push(b);
    }
    let painted: Vec<_> = if names.is_empty() {
        vec![]
    } else {
        out.arrows
            .iter()
            .flat_map(|a| a.paths())
            .map(|p| Ink::new(&p.commands, DEFAULT.world(p.style.width_pt), p.filled))
            .collect()
    };
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    out.labels.clear();
    for name in names {
        let text = wrapped(
            &name.text,
            &format,
            doc.drawing_style.bond_length_world * 2.5,
        );
        let layout = crate::typography::layout(&text, &format);
        let area = layout.width * layout.height;
        let mut best = None;
        'search: for depth in [1., 1.8] {
            for fraction in [0.5, 0.38, 0.62, 0.26, 0.74] {
                let Some(frame) = Frame::at(&name, perimeter, fraction, dir) else {
                    continue;
                };
                for sign in [-1., 1.] {
                    let offsets = frame.offsets(layout.width, layout.height, gap, sign < 0.);
                    for offset in &offsets {
                        let center = frame
                            .point
                            .offset(offset.x * sign * depth, offset.y * sign * depth);
                        let b = (
                            center.offset(-layout.width * 0.5, -layout.height * 0.5),
                            center.offset(layout.width * 0.5, layout.height * 0.5),
                        );
                        if !b.0.is_finite() || !b.1.is_finite() {
                            continue;
                        }
                        let covered = taken
                            .iter()
                            .map(|t| overlap(b, grown(*t, gap * 0.5)))
                            .sum::<f32>();
                        let hits = painted.iter().filter(|p| p.hits(b, gap * 0.5)).count();
                        let cost = covered + hits as f32 * area;
                        if best.is_none_or(|(_, old)| cost < old) {
                            best = Some((b, cost));
                        }
                        if cost == 0. {
                            break 'search;
                        }
                    }
                }
            }
        }
        if let Some((chosen, _)) = best {
            taken.push(chosen);
            out.labels.push((chosen.0, text, format.clone()));
        }
    }
    if let Some((at, _)) = center_label {
        out.labels.push((at, cycle.label.clone(), format));
    }
}
