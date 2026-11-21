//! Native geometry for one deformed, open cycle lane. Logical stations keep
//! the original perimeter's arclength scale even after Bow and lane offsets.
use super::{CubicSection, Piece, Segment, V, nearer};
use crate::document::Point;
#[cfg(test)]
use crate::graphics::PathCommand;

#[derive(Debug, Clone)]
pub(in crate::cycles) struct Profile {
    pieces: Vec<ProfilePiece>,
    length: f64,
}

#[derive(Debug, Clone)]
struct ProfilePiece {
    baseline: Piece,
    painted: Piece,
    from: f64,
    to: f64,
    corner_after: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::cycles) struct ProfileProjection {
    pub u: f64,
    pub distance: f64,
}

impl Profile {
    pub(in crate::cycles) fn new(
        baseline: &[CubicSection],
        painted: &[CubicSection],
    ) -> Option<Self> {
        if baseline.is_empty() || baseline.len() != painted.len() {
            return None;
        }
        let (mut pieces, mut from, mut length) = (vec![], 0., 0.);
        let (mut baseline_previous, mut painted_previous) = (None, None);
        for (original, deformed) in baseline.iter().zip(painted) {
            let to = original.end_t;
            if !to.is_finite()
                || to <= from
                || to > 1.
                || deformed.end_t != to
                || original.corner_after != deformed.corner_after
            {
                return None;
            }
            let baseline = joined(native(original)?, baseline_previous)?;
            let painted = joined(native(deformed)?, painted_previous)?;
            baseline_previous = Some(baseline);
            painted_previous = Some(painted);
            let baseline = Piece::new(baseline, 0.)?;
            let painted = Piece::new(painted, length)?;
            let end = length + painted.length;
            if !end.is_finite() || end <= length {
                return None;
            }
            pieces.push(ProfilePiece {
                baseline,
                painted,
                from,
                to,
                corner_after: original.corner_after,
            });
            from = to;
            length = end;
        }
        (from == 1.).then_some(Self { pieces, length })
    }

    /// Project onto a closed logical bracket. Equal-distance solutions keep
    /// the earliest original station, including on a reversed native path.
    pub(in crate::cycles) fn project(
        &self,
        point: Point,
        bracket: (f64, f64),
    ) -> Option<ProfileProjection> {
        if !point.is_finite() || !valid_bracket(bracket) {
            return None;
        }
        let p = V::from(point);
        let mut best = None;
        for piece in &self.pieces {
            let from = piece.from.max(bracket.0);
            let to = piece.to.min(bracket.1);
            if from > to {
                continue;
            }
            let (t0, t1) = (piece.parameter(from), piece.parameter(to));
            let t = if t0 == t1 {
                t0
            } else {
                let trimmed = piece.painted.curve.trim(t0, t1);
                let (local, _) = trimmed.closest(p, self.length * 1e-10);
                if local == 0. {
                    t0
                } else if local == 1. {
                    t1
                } else {
                    t0 + (t1 - t0) * local
                }
            };
            let on = curve_point(piece.painted.curve, t);
            let u = piece.station(t).clamp(from, to);
            if best.is_none_or(|(old_u, old)| nearer(p, on, u, old, old_u)) {
                best = Some((u, on));
            }
        }
        let (u, on) = best?;
        Some(ProfileProjection {
            u,
            distance: p.sub(on).length(),
        })
    }

    pub(in crate::cycles) fn point(&self, u: f64) -> Option<Point> {
        let (_, piece) = self.at(u)?;
        curve_point(piece.painted.curve, piece.parameter(u)).point()
    }

    /// At a source corner, incoming selects the arriving unit tangent.
    pub(in crate::cycles) fn tangent(&self, u: f64, incoming: bool) -> Option<Point> {
        let (index, piece) = self.at(u)?;
        let t = piece.parameter(u);
        let tangent = if incoming && t == 0. && index > 0 {
            self.pieces.get(index - 1)?.painted.curve.tangent(1.)
        } else {
            piece.painted.curve.tangent(t)
        };
        tangent.point()
    }

    /// Painted world distance at an original station. This is deliberately
    /// independent of both Arrow's equal-segment parameter and logical u.
    pub(in crate::cycles) fn distance_at(&self, u: f64) -> Option<f64> {
        let (_, piece) = self.at(u)?;
        Some(piece.painted.start + piece.painted.distance(piece.parameter(u)))
    }

    /// Move by physical painted distance, clamping at the open lane's ends.
    pub(in crate::cycles) fn shift(&self, u: f64, world_delta: f64) -> Option<f64> {
        if !world_delta.is_finite() {
            return None;
        }
        let s = (self.distance_at(u)? + world_delta).clamp(0., self.length);
        let piece = self
            .pieces
            .iter()
            .find(|p| s < p.painted.start + p.painted.length)
            .or_else(|| self.pieces.last())?;
        Some(piece.station(piece.painted.parameter(s - piece.painted.start)))
    }

    /// Split native controls without fitting or flattening. end_t remains an
    /// absolute logical station and decreases when travelling in reverse.
    pub(in crate::cycles) fn subpath(&self, u0: f64, u1: f64) -> Option<Vec<CubicSection>> {
        if !valid_station(u0) || !valid_station(u1) {
            return None;
        }
        let (from, to) = (u0.min(u1), u0.max(u1));
        let mut sections = vec![];
        for (i, piece) in self.pieces.iter().enumerate() {
            let a = from.max(piece.from);
            let b = to.min(piece.to);
            if a >= b {
                continue;
            }
            let curve = piece
                .painted
                .curve
                .trim(piece.parameter(a), piece.parameter(b));
            let corner_before = a == piece.from
                && i.checked_sub(1)
                    .and_then(|i| self.pieces.get(i))
                    .is_some_and(|p| p.corner_after);
            sections.push((
                a,
                CubicSection {
                    points: curve.controls().map(|p| [p.x, p.y]),
                    end_t: b,
                    corner_after: b == piece.to && piece.corner_after,
                },
                corner_before,
            ));
        }
        Some(if u0 > u1 {
            sections
                .into_iter()
                .rev()
                .map(|(from, section, corner_before)| CubicSection {
                    points: [
                        section.points[3],
                        section.points[2],
                        section.points[1],
                        section.points[0],
                    ],
                    end_t: from,
                    corner_after: corner_before,
                })
                .collect()
        } else {
            sections
                .into_iter()
                .map(|(_, section, _)| section)
                .collect()
        })
    }

    #[cfg(test)]
    fn commands(&self, u0: f64, u1: f64) -> Option<Vec<PathCommand>> {
        let mut commands = vec![PathCommand::Move(self.point(u0)?)];
        for section in self.subpath(u0, u1)? {
            commands.push(native(&section)?.command()?);
        }
        Some(commands)
    }

    /// The connected interval in a grown ink box nearest station, restricted
    /// to an ordered logical bracket. Coordinate roots partition each native
    /// curve; midpoint classification only chooses between those exact roots.
    /// A lane missing the actual box has no opening.
    pub(in crate::cycles) fn opening(
        &self,
        station: f64,
        bracket: (f64, f64),
        bounds: [f64; 4],
    ) -> Option<(f64, f64)> {
        if !valid_station(station)
            || !valid_bracket(bracket)
            || station < bracket.0
            || station > bracket.1
            || bounds.iter().any(|v| !v.is_finite())
            || bounds[0] > bounds[2]
            || bounds[1] > bounds[3]
        {
            return None;
        }
        let mut intervals: Vec<(f64, f64)> = vec![];
        for piece in &self.pieces {
            let from = bracket.0.max(piece.from);
            let to = bracket.1.min(piece.to);
            if from > to {
                continue;
            }
            let (t0, t1) = (piece.parameter(from), piece.parameter(to));
            let curve = piece.painted.curve;
            let mut cuts = vec![t0, t1];
            for (axis, edge) in [
                (0, bounds[0]),
                (0, bounds[2]),
                (1, bounds[1]),
                (1, bounds[3]),
            ] {
                cuts.extend(
                    coordinate_roots(curve, axis, edge)
                        .into_iter()
                        .filter(|t| t0 <= *t && *t <= t1),
                );
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup();
            let station_at = |t| {
                if t == t0 {
                    from
                } else if t == t1 {
                    to
                } else {
                    piece.station(t).clamp(from, to)
                }
            };
            for pair in cuts.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if inside(curve_point(curve, a + (b - a) * 0.5), bounds) {
                    intervals.push((station_at(a), station_at(b)));
                }
            }
            // A tangent contact, a zero-size box, or a bracket endpoint can
            // have no inside interval but still has a native intersection.
            for t in cuts {
                if inside(curve_point(curve, t), bounds) {
                    let u = station_at(t);
                    intervals.push((u, u));
                }
            }
        }
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut connected: Vec<(f64, f64)> = vec![];
        for interval in intervals {
            if let Some(last) = connected.last_mut()
                && interval.0 <= last.1
            {
                last.1 = last.1.max(interval.1);
                continue;
            }
            connected.push(interval);
        }
        let s = self.distance_at(station)?;
        connected.into_iter().min_by(|a, b| {
            let distance = |interval: (f64, f64)| {
                let nearest = station.clamp(interval.0, interval.1);
                self.distance_at(nearest)
                    .map_or(f64::INFINITY, |on| (on - s).abs())
            };
            distance(*a)
                .total_cmp(&distance(*b))
                .then(a.0.total_cmp(&b.0))
        })
    }

    fn at(&self, u: f64) -> Option<(usize, &ProfilePiece)> {
        if !valid_station(u) {
            return None;
        }
        self.pieces
            .iter()
            .enumerate()
            .find(|(_, p)| u < p.to)
            .or_else(|| self.pieces.iter().enumerate().next_back())
    }
}

impl ProfilePiece {
    fn parameter(&self, u: f64) -> f64 {
        self.baseline
            .parameter((u - self.from) / (self.to - self.from) * self.baseline.length)
    }

    fn station(&self, t: f64) -> f64 {
        if t <= 0. {
            self.from
        } else if t >= 1. {
            self.to
        } else {
            self.from + (self.to - self.from) * self.baseline.distance(t) / self.baseline.length
        }
    }
}

fn valid_station(u: f64) -> bool {
    u.is_finite() && (0. ..=1.).contains(&u)
}

fn valid_bracket((from, to): (f64, f64)) -> bool {
    valid_station(from) && valid_station(to) && from <= to
}

fn curve_point(curve: Segment, t: f64) -> V {
    if t <= 0. {
        curve.ends().0
    } else if t >= 1. {
        curve.ends().1
    } else {
        curve.at(t)
    }
}

fn joined(curve: Segment, previous: Option<Segment>) -> Option<Segment> {
    let Some(previous) = previous else {
        return Some(curve);
    };
    let previous_end = previous.ends().1;
    let first = curve.ends().0;
    let scale = curve
        .controls()
        .into_iter()
        .chain(previous.controls())
        .map(|p| p.x.abs().max(p.y.abs()))
        .fold(0., f64::max);
    if first.sub(previous_end).length() > 16. * f64::EPSILON * scale {
        return None;
    }
    // Native Arrow nodes share the preceding section's endpoint. Independently
    // trimmed cubics can calculate that same knot a few ulps apart.
    Some(match curve {
        Segment::Line(_, end) => Segment::Line(previous_end, end),
        Segment::Cubic([_, a, b, end]) => Segment::Cubic([previous_end, a, b, end]),
    })
}

fn native(section: &CubicSection) -> Option<Segment> {
    if section.points.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    let points = section.points.map(|p| V::new(p[0], p[1]));
    let [a, c, d, b] = points;
    // Original lines arrive as equally spaced cubic controls. Recognize only
    // their floating-point construction error, preserving any resolvable Bow.
    let scale = points
        .iter()
        .map(|p| p.x.abs().max(p.y.abs()))
        .fold(0., f64::max);
    let error = 8. * f64::EPSILON * scale;
    Some(
        if c.sub(a.lerp(b, 1. / 3.)).length() <= error
            && d.sub(a.lerp(b, 2. / 3.)).length() <= error
        {
            Segment::Line(a, b)
        } else {
            Segment::Cubic(points)
        },
    )
}

fn inside(point: V, bounds: [f64; 4]) -> bool {
    let x_error = 16. * f64::EPSILON * point.x.abs().max(bounds[0].abs()).max(bounds[2].abs());
    let y_error = 16. * f64::EPSILON * point.y.abs().max(bounds[1].abs()).max(bounds[3].abs());
    bounds[0] - x_error <= point.x
        && point.x <= bounds[2] + x_error
        && bounds[1] - y_error <= point.y
        && point.y <= bounds[3] + y_error
}

/// Isolate roots between coordinate extrema, then bisect monotone spans.
/// Including exact endpoints avoids a polynomial solver rounding a seam root
/// just outside [0, 1], which otherwise loses horizontal/vertical box edges.
fn coordinate_roots(curve: Segment, axis: usize, value: f64) -> Vec<f64> {
    if let Segment::Line(a, b) = curve {
        let (a, b) = if axis == 0 { (a.x, b.x) } else { (a.y, b.y) };
        return if a == b {
            if a == value { vec![0., 1.] } else { vec![] }
        } else {
            let t = (value - a) / (b - a);
            if (0. ..=1.).contains(&t) {
                vec![t]
            } else {
                vec![]
            }
        };
    }
    let controls = curve.controls().map(|p| if axis == 0 { p.x } else { p.y });
    let q = controls.map(|p| p - value);
    let eval = |t: f64| {
        if t == 0. {
            return q[0];
        }
        if t == 1. {
            return q[3];
        }
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        let a = lerp(q[0], q[1]);
        let b = lerp(q[1], q[2]);
        let c = lerp(q[2], q[3]);
        lerp(lerp(a, b), lerp(b, c))
    };
    let mut knots = vec![0., 1.];
    knots.extend(
        kurbo::common::solve_quadratic(
            q[1] - q[0],
            2. * (q[2] - 2. * q[1] + q[0]),
            q[3] - 3. * q[2] + 3. * q[1] - q[0],
        )
        .into_iter()
        .filter(|t| 0. < *t && *t < 1.),
    );
    knots.sort_by(f64::total_cmp);
    knots.dedup();
    let scale = controls
        .into_iter()
        .map(f64::abs)
        .fold(value.abs(), f64::max);
    let error = 16. * f64::EPSILON * scale;
    let mut roots = vec![];
    for &t in &knots {
        if eval(t).abs() <= error {
            roots.push(t);
        }
    }
    for pair in knots.windows(2) {
        let (mut a, mut b) = (pair[0], pair[1]);
        let (mut fa, fb) = (eval(a), eval(b));
        if fa == 0. || fb == 0. || fa.is_sign_positive() == fb.is_sign_positive() {
            continue;
        }
        for _ in 0..64 {
            let mid = a + (b - a) * 0.5;
            if mid == a || mid == b {
                break;
            }
            let fm = eval(mid);
            if fm == 0. {
                a = mid;
                b = mid;
                break;
            }
            if fa.is_sign_positive() == fm.is_sign_positive() {
                a = mid;
                fa = fm;
            } else {
                b = mid;
            }
        }
        roots.push(a + (b - a) * 0.5);
    }
    roots.sort_by(f64::total_cmp);
    roots.dedup();
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(points: [[f64; 2]; 4], end_t: f64, corner_after: bool) -> CubicSection {
        CubicSection {
            points,
            end_t,
            corner_after,
        }
    }

    fn line(a: [f64; 2], b: [f64; 2], end_t: f64, corner_after: bool) -> CubicSection {
        let a = V::new(a[0], a[1]);
        let b = V::new(b[0], b[1]);
        section(
            Segment::Line(a, b).controls().map(|p| [p.x, p.y]),
            end_t,
            corner_after,
        )
    }

    fn close(a: f64, b: f64, error: f64) {
        assert!((a - b).abs() <= error, "{a} != {b} (error {error})");
    }

    fn controls_close(a: [[f64; 2]; 4], b: [[f64; 2]; 4]) {
        for (a, b) in a.into_iter().flatten().zip(b.into_iter().flatten()) {
            close(a, b, 1e-7);
        }
    }

    fn point_close(a: Point, b: Point, error: f64) {
        close(f64::from(a.x), f64::from(b.x), error);
        close(f64::from(a.y), f64::from(b.y), error);
    }

    #[test]
    fn native_slices_rejoin_exact_controls_and_keep_both_corner_tangents() {
        let baseline = [
            line([0., 0.], [90., 0.], 0.9, true),
            line([90., 0.], [90., 10.], 1., false),
        ];
        let painted = [
            section([[0., 0.], [30., 20.], [60., 20.], [90., 0.]], 0.9, true),
            line([90., 0.], [100., 20.], 1., false),
        ];
        let profile = Profile::new(&baseline, &painted).unwrap();
        let left = profile.subpath(0., 0.36).unwrap();
        let right = profile.subpath(0.36, 1.).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(right.len(), 2);
        assert_eq!(left[0].points[3], right[0].points[0]);
        let source = native(&painted[0]).unwrap();
        controls_close(
            left[0].points,
            source.trim(0., 0.4).controls().map(|p| [p.x, p.y]),
        );
        controls_close(
            right[0].points,
            source.trim(0.4, 1.).controls().map(|p| [p.x, p.y]),
        );
        assert!(right[0].corner_after);
        point_close(
            profile.tangent(0.9, true).unwrap(),
            source.tangent(1.).point().unwrap(),
            1e-7,
        );
        point_close(
            profile.tangent(0.9, false).unwrap(),
            native(&painted[1]).unwrap().tangent(0.).point().unwrap(),
            1e-7,
        );
        let reversed = profile.subpath(0.95, 0.18).unwrap();
        assert_eq!(reversed.len(), 2);
        assert_eq!(reversed[0].end_t, 0.9);
        assert!(reversed[0].corner_after);
        close(reversed[1].end_t, 0.18, 1e-15);
        controls_close(
            reversed[0].points,
            native(&painted[1])
                .unwrap()
                .trim(0., 0.5)
                .reversed()
                .controls()
                .map(|p| [p.x, p.y]),
        );
        controls_close(
            reversed[1].points,
            source
                .trim(0.2, 1.)
                .reversed()
                .controls()
                .map(|p| [p.x, p.y]),
        );
        assert!(matches!(
            profile.commands(0.91, 0.99).unwrap()[1],
            PathCommand::Line(_)
        ));
        assert!(matches!(
            profile.commands(0.1, 0.8).unwrap()[1],
            PathCommand::Cubic(..)
        ));
        assert_eq!(profile.commands(0.5, 0.5).unwrap().len(), 1);
    }

    #[test]
    fn cubic_projection_uses_original_arclength_and_shift_uses_painted_arclength() {
        let baseline = [section(
            [[0., 0.], [0., 300.], [100., 0.], [100., 0.]],
            1.,
            false,
        )];
        let painted = [section(
            [[0., 40.], [200., 200.], [100., -80.], [100., 40.]],
            1.,
            false,
        )];
        let profile = Profile::new(&baseline, &painted).unwrap();
        let original = &profile.pieces[0].baseline;
        let deformed = &profile.pieces[0].painted;
        let (t0, t1) = (0.2, 0.7);
        let u0 = original.distance(t0) / original.length;
        let u1 = original.distance(t1) / original.length;
        assert!((u0 - t0).abs() > 0.1);
        assert!((u0 - deformed.distance(t0) / deformed.length).abs() > 0.01);
        let on = deformed.curve.at(t0).point().unwrap();
        let projected = profile.project(on, (0., 1.)).unwrap();
        close(projected.u, u0, 1e-6);
        assert!(projected.distance < 1e-5);
        point_close(profile.point(u0).unwrap(), on, 1e-5);
        close(
            profile.distance_at(u0).unwrap(),
            deformed.distance(t0),
            1e-6,
        );
        let delta = deformed.distance(t1) - deformed.distance(t0);
        close(profile.shift(u0, delta).unwrap(), u1, 1e-8);
        close(profile.shift(u1, -delta).unwrap(), u0, 1e-8);
        assert_eq!(profile.shift(0.3, -10000.), Some(0.));
        assert_eq!(profile.shift(0.3, 10000.), Some(1.));
        let cut = profile.subpath(u0, u1).unwrap();
        controls_close(
            cut[0].points,
            deformed.curve.trim(t0, t1).controls().map(|p| [p.x, p.y]),
        );
    }

    #[test]
    fn reversed_geometry_maps_projection_to_its_original_logical_orientation() {
        let baseline = section([[0., 0.], [0., 200.], [90., -20.], [100., 0.]], 1., false);
        let painted = section(
            [[0., 10.], [50., 240.], [130., -30.], [100., 10.]],
            1.,
            false,
        );
        let forward = Profile::new(&[baseline.clone()], &[painted.clone()]).unwrap();
        let reverse_section = |source: &CubicSection| {
            section(
                native(source)
                    .unwrap()
                    .reversed()
                    .controls()
                    .map(|p| [p.x, p.y]),
                1.,
                false,
            )
        };
        let reverse =
            Profile::new(&[reverse_section(&baseline)], &[reverse_section(&painted)]).unwrap();
        let t = 0.37;
        let point = forward.pieces[0].painted.curve.at(t).point().unwrap();
        let u = forward.pieces[0].station(t);
        close(forward.project(point, (0., 1.)).unwrap().u, u, 1e-6);
        close(reverse.project(point, (0., 1.)).unwrap().u, 1. - u, 1e-6);
        point_close(reverse.point(0.).unwrap(), forward.point(1.).unwrap(), 1e-6);
        point_close(reverse.point(1.).unwrap(), forward.point(0.).unwrap(), 1e-6);
        let clipped = forward.project(point, (0.8, 0.9)).unwrap();
        assert!((0.8..=0.9).contains(&clipped.u));
        assert!(clipped.distance > 1.);
    }

    #[test]
    fn paired_lanes_have_independent_native_openings_and_actual_misses() {
        let baseline = [line([0., 0.], [100., 0.], 1., false)];
        let upper = Profile::new(&baseline, &[line([0., -10.], [100., 10.], 1., false)]).unwrap();
        let lower = Profile::new(&baseline, &[line([0., 10.], [100., -10.], 1., false)]).unwrap();
        let bounds = [35., -2., 65., 6.];
        let a = upper.opening(0.5, (0.1, 0.9), bounds).unwrap();
        let b = lower.opening(0.5, (0.1, 0.9), bounds).unwrap();
        close(a.0, 0.4, 1e-12);
        close(a.1, 0.65, 1e-12);
        close(b.0, 0.35, 1e-12);
        close(b.1, 0.6, 1e-12);
        assert!(
            upper
                .opening(0.5, (0., 1.), [35., 100., 65., 110.])
                .is_none()
        );
        assert!(
            upper
                .opening(0.5, (0., 1.), [35., 10. + 1e-7, 65., 11.])
                .is_none()
        );
        assert!(upper.opening(0.5, (0.1, 0.3), bounds).is_none());
    }

    #[test]
    fn box_rounding_on_one_axis_does_not_turn_a_miss_on_the_other_axis_into_an_opening() {
        let distant = 1e14;
        let horizontal = [line([distant, 0.], [distant + 1000., 0.], 1., false)];
        let horizontal = Profile::new(&horizontal, &horizontal).unwrap();
        assert!(
            horizontal
                .opening(0.5, (0., 1.), [distant + 400., 0.1, distant + 600., 0.2])
                .is_none()
        );
        assert!(
            horizontal
                .opening(0.5, (0., 1.), [distant + 400., -0.1, distant + 600., 0.1])
                .is_some()
        );
        let vertical = [line([0., distant], [0., distant + 1000.], 1., false)];
        let vertical = Profile::new(&vertical, &vertical).unwrap();
        assert!(
            vertical
                .opening(0.5, (0., 1.), [0.1, distant + 400., 0.2, distant + 600.])
                .is_none()
        );
    }

    #[test]
    fn cubic_box_roots_include_endpoint_axes_and_choose_nearby_component() {
        let arch = [section(
            [[0., 0.], [0., 100.], [100., 100.], [100., 0.]],
            1.,
            false,
        )];
        let profile = Profile::new(&arch, &arch).unwrap();
        let opening = profile
            .opening(0.5, (0., 1.), [48., 74., 52., 76.])
            .unwrap();
        close(f64::from(profile.point(opening.0).unwrap().x), 48., 1e-5);
        close(f64::from(profile.point(opening.1).unwrap().x), 52., 1e-5);
        let seam = profile.opening(0., (0., 0.2), [-1., -1., 1., 1.]).unwrap();
        assert_eq!(seam.0, 0.);
        assert!(seam.1 > 0.);
        let tangent = profile
            .opening(0.5, (0., 1.), [40., 75., 60., 76.])
            .unwrap();
        close(tangent.0, 0.5, 1e-12);
        close(tangent.1, 0.5, 1e-12);
        let wave = [section(
            [[0., 0.], [100. / 3., 100.], [200. / 3., -100.], [100., 0.]],
            1.,
            false,
        )];
        let profile = Profile::new(&wave, &wave).unwrap();
        let middle = profile
            .opening(0.5, (0., 1.), [-1., -1., 101., 1.])
            .unwrap();
        assert!(0.4 < middle.0 && middle.0 < 0.5);
        assert!(0.5 < middle.1 && middle.1 < 0.6);
        let early = profile.opening(0., (0., 1.), [-1., -1., 101., 1.]).unwrap();
        assert_eq!(early.0, 0.);
        assert!(early.1 < 0.1);
        assert!(
            profile
                .opening(0.5, (0., 1.), [-1., 100., 101., 101.])
                .is_none()
        );
    }

    #[test]
    fn reflected_acute_native_cubics_keep_projection_and_trim_controls() {
        let transform = |p: [f64; 2]| {
            [
                -1.7 * p[0] + 2.3 * p[1] + 70.,
                0.02 * p[0] + 0.009 * p[1] - 11.,
            ]
        };
        let baseline = [section(
            [[0., 0.], [20., -50.], [60., 50.], [100., 0.]].map(transform),
            1.,
            false,
        )];
        let painted = [section(
            [[0., 4.], [20., -40.], [60., 60.], [100., 4.]].map(transform),
            1.,
            false,
        )];
        let profile = Profile::new(&baseline, &painted).unwrap();
        let piece = &profile.pieces[0];
        let t = 0.37;
        let u = piece.station(t);
        let p = piece.painted.curve.at(t);
        let projected = profile.project(p.point().unwrap(), (0.1, 0.9)).unwrap();
        close(projected.u, u, 1e-5);
        assert!(projected.distance < 1e-5);
        let opening = profile
            .opening(u, (0.1, 0.9), [p.x - 0.1, p.y - 0.1, p.x + 0.1, p.y + 0.1])
            .unwrap();
        assert!(opening.0 < u && u < opening.1);
        let to = piece.station(0.61);
        let native = profile.subpath(u, to).unwrap();
        controls_close(
            native[0].points,
            piece
                .painted
                .curve
                .trim(t, 0.61)
                .controls()
                .map(|p| [p.x, p.y]),
        );
    }

    #[test]
    fn repeated_geometry_keeps_lowest_logical_tie_and_invalid_inputs_fail() {
        let path = [
            line([0., 0.], [100., 0.], 0.5, true),
            line([100., 0.], [0., 0.], 1., false),
        ];
        let profile = Profile::new(&path, &path).unwrap();
        close(
            profile.project(Point::new(50., 20.), (0., 1.)).unwrap().u,
            0.25,
            1e-12,
        );
        close(
            profile.project(Point::new(50., 20.), (0.5, 1.)).unwrap().u,
            0.75,
            1e-12,
        );
        assert!(
            profile
                .project(Point::new(f32::NAN, 0.), (0., 1.))
                .is_none()
        );
        assert!(profile.project(Point::default(), (0.8, 0.2)).is_none());
        assert!(profile.point(-0.01).is_none());
        assert!(profile.tangent(f64::INFINITY, false).is_none());
        assert!(profile.distance_at(1.01).is_none());
        assert!(profile.shift(0.5, f64::NAN).is_none());
        assert!(profile.subpath(0., f64::NAN).is_none());
        assert!(profile.opening(0.5, (0., 1.), [0., 0., -1., 1.]).is_none());
        assert!(Profile::new(&[], &[]).is_none());
        assert!(Profile::new(&path, &path[..1]).is_none());
        let mut broken = path.clone();
        broken[1].points[0][0] += 1.;
        assert!(Profile::new(&path, &broken).is_none());
        let collapsed = [line([0., 0.], [0., 0.], 1., false)];
        assert!(Profile::new(&collapsed, &collapsed).is_none());
    }
}
