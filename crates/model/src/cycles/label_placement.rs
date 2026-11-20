//! Place each step name on one side of the ring with space around the ink.
use super::{Drawn, Span, center_of, grown, polar, wrapped};
use crate::{
    arrows::ArrowPath,
    atom_labels::segment_hits_rect,
    document::{Document, Point},
    graphics::{Graphic, PathCommand},
    style::DEFAULT,
    typography::{TextAlign, TextFormat, layout},
};
use kurbo::{BezPath, Line, ParamCurve, ParamCurveExtrema, PathSeg, Rect, Shape};

type Bounds = (Point, Point);

struct PaintedPath {
    bounds: Bounds,
    path: BezPath,
    filled: bool,
    stroke: f32,
}

impl PaintedPath {
    fn new(painted: ArrowPath) -> Self {
        let mut path = BezPath::new();
        for command in painted.commands {
            match command {
                PathCommand::Move(p) => path.move_to(kurbo_point(p)),
                PathCommand::Line(p) => path.line_to(kurbo_point(p)),
                PathCommand::Cubic(a, b, c) => {
                    path.curve_to(kurbo_point(a), kurbo_point(b), kurbo_point(c));
                }
                PathCommand::Close => path.close_path(),
            }
        }
        let b = path.bounding_box();
        let bounds = (
            Point::new(b.x0 as f32, b.y0 as f32),
            Point::new(b.x1 as f32, b.y1 as f32),
        );
        Self {
            bounds,
            path,
            filled: painted.filled,
            stroke: 0.5 * painted.style.width(),
        }
    }

    fn hits(&self, label: Bounds) -> bool {
        let (lo, hi) = grown(label, self.stroke);
        if !overlaps((lo, hi), self.bounds) {
            return false;
        }
        let rect = Rect::from_points(kurbo_point(lo), kurbo_point(hi));
        let corners = [
            rect.origin(),
            kurbo::Point::new(rect.x1, rect.y0),
            kurbo::Point::new(rect.x1, rect.y1),
            kurbo::Point::new(rect.x0, rect.y1),
        ];
        let includes =
            |p: kurbo::Point| rect.x0 <= p.x && p.x <= rect.x1 && rect.y0 <= p.y && p.y <= rect.y1;
        self.path.segments().any(|segment| {
            if let PathSeg::Line(line) = segment {
                return segment_hits_rect(
                    Point::new(line.p0.x as f32, line.p0.y as f32),
                    Point::new(line.p1.x as f32, line.p1.y as f32),
                    lo,
                    hi,
                );
            }
            let b = ParamCurveExtrema::bounding_box(&segment);
            let overlaps = rect.x0 <= b.x1 && b.x0 <= rect.x1 && rect.y0 <= b.y1 && b.y0 <= rect.y1;
            // A straight cubic on a rectangle edge, or an arc tangent to it,
            // may have no edge crossing. Include its extrema in the test.
            overlaps
                && (includes(segment.start())
                    || includes(segment.end())
                    || segment
                        .extrema()
                        .into_iter()
                        .any(|t| includes(segment.eval(t)))
                    || corners
                        .into_iter()
                        .zip(corners.into_iter().cycle().skip(1))
                        .any(|(a, b)| !segment.intersect_line(Line::new(a, b)).is_empty()))
        }) || (self.filled && self.path.winding(kurbo_point(center_of(label))) != 0)
    }
}

fn kurbo_point(p: Point) -> kurbo::Point {
    kurbo::Point::new(f64::from(p.x), f64::from(p.y))
}

fn overlaps((lo, hi): Bounds, (other_lo, other_hi): Bounds) -> bool {
    lo.x <= other_hi.x && hi.x >= other_lo.x && lo.y <= other_hi.y && hi.y >= other_lo.y
}

fn far_corner((lo, hi): Bounds, origin: Point) -> Point {
    let farther = |a: f32, b: f32, o: f32| if (a - o).abs() > (b - o).abs() { a } else { b };
    Point::new(farther(lo.x, hi.x, origin.x), farther(lo.y, hi.y, origin.y))
}

struct Ring {
    center: Point,
    radius: f32,
    clearance: f32,
    spacing: f32,
}

impl Ring {
    fn has_room(&self, (lo, hi): Bounds, inside: bool) -> bool {
        if inside {
            far_corner((lo, hi), self.center).distance(self.center) < self.radius - self.clearance
        } else {
            let nearest = Point::new(
                self.center.x.clamp(lo.x, hi.x),
                self.center.y.clamp(lo.y, hi.y),
            );
            nearest.distance(self.center) > self.radius + self.clearance
        }
    }

    fn label_bounds(
        &self,
        arc: Span,
        outward: f32,
        size: Point,
        occupied: &[Bounds],
        painted: &[PaintedPath],
    ) -> Bounds {
        let box_at = |p: Point| (p.offset(-size.x, -size.y), p.offset(size.x, size.y));
        let clear = |label: Bounds| {
            let padded = grown(label, self.clearance);
            !occupied.iter().any(|b| overlaps(padded, *b))
                && !painted.iter().any(|path| path.hits(padded))
        };
        for inside in [true, false] {
            let direction = if inside { -outward } else { outward };
            for depth in [0., 0.2, 0.45, 0.8, 1.3, 2.] {
                for fraction in [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8] {
                    let angle = arc.angle(fraction);
                    let reach = size.x * angle.cos().abs() + size.y * angle.sin().abs();
                    let radius =
                        arc.radius + direction * (reach + self.clearance + depth * self.spacing);
                    if radius > 0. {
                        let label = box_at(polar(arc.center, radius, angle));
                        if self.has_room(label, inside) && clear(label) {
                            return label;
                        }
                    }
                }
            }
        }

        // If the nearby candidates are blocked, advance along this step's
        // bearing only as far as needed to get past the occupied intervals.
        let mid = arc.at(0.5, 0.);
        let angle = (mid.y - self.center.y).atan2(mid.x - self.center.x);
        let (y, x) = angle.sin_cos();
        let direction = Point::new(x, y);
        let padding = size.offset(self.clearance, self.clearance);
        let bearing = |(lo, hi): Bounds| {
            let bounds = (
                lo.offset(-padding.x, -padding.y),
                hi.offset(padding.x, padding.y),
            );
            ray_interval(self.center, direction, bounds)
        };
        let mut blocked: Vec<_> = occupied.iter().copied().filter_map(bearing).collect();
        blocked.sort_by(|a, b| a.0.total_cmp(&b.0));
        let reach = size.x * x.abs() + size.y * y.abs();
        let mut radius = self.radius + reach + 2. * self.clearance;
        for _ in 0..8 {
            radius = first_clear(radius, &blocked, 0.25 * self.clearance);
            let label = box_at(polar(self.center, radius, angle));
            if clear(label) {
                return label;
            }
            radius += 0.25 * self.spacing;
        }

        // Bounds give a finite escape for a route running along this ray.
        blocked.extend(
            painted
                .iter()
                .filter_map(|p| bearing(grown(p.bounds, p.stroke))),
        );
        blocked.sort_by(|a, b| a.0.total_cmp(&b.0));
        let radius = first_clear(radius, &blocked, 0.25 * self.clearance);
        box_at(polar(self.center, radius, angle))
    }
}

fn first_clear(mut radius: f32, blocked: &[(f32, f32)], spacing: f32) -> f32 {
    for &(enter, exit) in blocked {
        if radius < enter {
            break;
        }
        if radius <= exit {
            radius = exit + spacing;
        }
    }
    radius
}

// Distances along the ray for which the center of a padded label would lie
// inside the obstacle expanded by half the label size.
fn ray_interval(origin: Point, direction: Point, (lo, hi): Bounds) -> Option<(f32, f32)> {
    let (mut start, mut end) = (0.0_f32, f32::INFINITY);
    for (p, d, low, high) in [
        (origin.x, direction.x, lo.x, hi.x),
        (origin.y, direction.y, lo.y, hi.y),
    ] {
        if d == 0. {
            if p < low || p > high {
                return None;
            }
        } else {
            let a = (low - p) / d;
            let b = (high - p) / d;
            start = start.max(a.min(b));
            end = end.min(a.max(b));
        }
    }
    (start <= end).then_some((start, end))
}

pub(super) fn place(
    doc: &Document,
    g: &Graphic,
    cycle: &super::Cycle,
    names: Vec<(&str, Span, f32)>,
    parts: &[Option<Bounds>],
    out: &mut Drawn,
) {
    let format = TextFormat {
        style: doc.drawing_style.text_style(),
        alignment: TextAlign::Center,
        ..Default::default()
    };
    let (center, radius) = g.cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    let cycle_label = (!cycle.label.trim().is_empty()).then(|| {
        let text = layout(&cycle.label, &format);
        let at = center.offset(-0.5 * text.width, -0.5 * text.height);
        (at, at.offset(text.width, text.height))
    });
    if !names.is_empty() {
        let ring = Ring {
            center,
            radius,
            clearance: (0.5 * DEFAULT.world(cycle.gap_pt)).max(0.1 * bond),
            spacing: bond,
        };
        let mut occupied: Vec<_> = parts.iter().flatten().copied().chain(cycle_label).collect();
        let painted: Vec<_> = out
            .arrows
            .iter()
            .flat_map(|arrow| arrow.paths())
            .map(PaintedPath::new)
            .collect();
        for (name, arc, outward) in names {
            let text = wrapped(name, &format, bond * 2.5);
            let dimensions = layout(&text, &format);
            let size = Point::new(0.5 * dimensions.width, 0.5 * dimensions.height);
            let chosen = ring.label_bounds(arc, outward, size, &occupied, &painted);
            occupied.push(chosen);
            out.labels.push((chosen.0, text, format.clone()));
        }
    }
    if let Some((at, _)) = cycle_label {
        out.labels.push((at, cycle.label.clone(), format));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidied_suzuki_step_names_fit_inside_the_ring_clear_of_painted_arrows() {
        let mut doc = Document::from_json(include_bytes!(
            "../../../../tests/fixtures/cycles/tidy-before.rsk"
        ))
        .unwrap();
        let cycle_id = 191;
        let graphic = |doc: &Document| doc.graphics.iter().position(|g| g.id == cycle_id).unwrap();
        let index = graphic(&doc);
        doc.graphics[index].cycle.as_mut().unwrap().label = "Suzuki–Miyaura\ncoupling".into();
        assert!(super::super::tidy(&mut doc, cycle_id));

        let g = &doc.graphics[graphic(&doc)];
        let (center, radius) = g.cycle_circle();
        let drawn = super::super::drawn(&doc, g);
        let paths: Vec<_> = drawn
            .arrows
            .iter()
            .flat_map(|arrow| arrow.paths())
            .map(PaintedPath::new)
            .collect();

        assert_eq!(drawn.labels.len(), 4);
        for (at, name, format) in drawn.labels.iter().take(3) {
            let dimensions = layout(name, format);
            let label = (*at, at.offset(dimensions.width, dimensions.height));
            assert!(
                far_corner(label, center).distance(center) < radius,
                "{name} should fit entirely inside the ring"
            );
            assert!(
                paths.iter().all(|path| !path.hits(label)),
                "{name} overlaps a painted arrow"
            );
        }
    }

    #[test]
    fn an_occupied_interior_moves_the_whole_label_outside() {
        let center = Point::new(0., 0.);
        let ring = Ring {
            center,
            radius: 50.,
            clearance: 4.,
            spacing: 42.,
        };
        let occupied = [(Point::new(-50., -50.), Point::new(50., 50.))];
        let arc = Span {
            center,
            radius: ring.radius,
            first: -45.,
            sweep: 90.,
        };
        let label = ring.label_bounds(arc, 1., Point::new(24., 9.), &occupied, &[]);
        assert!(ring.has_room(label, false));
        assert!(!overlaps(grown(label, ring.clearance), occupied[0]));
        assert!(center_of(label).x > 50.);
    }

    #[test]
    fn a_crowded_ring_uses_the_first_clear_space_along_the_step() {
        let center = Point::new(0., 0.);
        let ring = Ring {
            center,
            radius: 50.,
            clearance: 4.,
            spacing: 42.,
        };
        let occupied = [
            (Point::new(-200., -200.), Point::new(200., 200.)),
            (Point::new(1000., -20.), Point::new(1050., 20.)),
            (Point::new(10000., 10000.), Point::new(10010., 10010.)),
        ];
        let arc = Span {
            center,
            radius: ring.radius,
            first: -45.,
            sweep: 90.,
        };
        let connector = PaintedPath::new(ArrowPath {
            commands: vec![
                PathCommand::Move(Point::new(50., 0.)),
                PathCommand::Cubic(
                    Point::new(50., -7053.41),
                    Point::new(5025., 1473.29),
                    Point::new(10000., 10000.),
                ),
            ],
            style: Default::default(),
            filled: false,
        });
        let label = ring.label_bounds(arc, 1., Point::new(24., 9.), &occupied, &[connector]);
        assert!(ring.has_room(label, false));
        assert!(
            occupied
                .iter()
                .all(|b| !overlaps(grown(label, ring.clearance), *b))
        );
        assert!((228. ..235.).contains(&center_of(label).x));
    }

    #[test]
    fn collision_check_follows_a_large_curve_between_sample_points() {
        let path = PaintedPath::new(ArrowPath {
            commands: vec![
                PathCommand::Move(Point::new(40000., 0.)),
                PathCommand::Cubic(
                    Point::new(40000., 22091.39),
                    Point::new(22091.39, 40000.),
                    Point::new(0., 40000.),
                ),
            ],
            style: Default::default(),
            filled: false,
        });
        let bounds = (Point::new(39985.94, 1012.42), Point::new(40055.94, 1052.42));
        assert!(path.hits(bounds));
    }
}
