//! Open geometry read from the native shaft after Bow, lanes and head trim.
use super::{Piece, Projection, V, nearer, segments};
use crate::document::{Arrow, Point};

#[derive(Debug)]
pub(in crate::cycles) struct Shaft {
    pieces: Vec<Piece>,
    length: f64,
}

impl Shaft {
    pub(in crate::cycles) fn from_arrow(arrow: &Arrow) -> Option<Self> {
        let path = arrow.paths().into_iter().find(|p| !p.filled)?;
        Self::new(&path.commands)
    }

    fn new(commands: &[crate::graphics::PathCommand]) -> Option<Self> {
        let (mut pieces, mut length) = (vec![], 0.);
        for curve in segments(commands)? {
            if curve.polygon_length() == 0. {
                continue;
            }
            let piece = Piece::new(curve, length)?;
            let end = length + piece.length;
            if end <= length {
                return None;
            }
            length = end;
            pieces.push(piece);
        }
        (length > 0.).then_some(Self { pieces, length })
    }

    pub(in crate::cycles) fn length(&self) -> f64 {
        self.length
    }

    pub(in crate::cycles) fn project(&self, point: Point) -> Option<Projection> {
        if !point.is_finite() {
            return None;
        }
        let p = V::from(point);
        let mut best = None;
        for piece in &self.pieces {
            let (t, on) = piece.curve.closest(p, self.length * 1e-8);
            let s = piece.start + piece.distance(t);
            if best.is_none_or(|(old_s, old)| nearer(p, on, s, old, old_s)) {
                best = Some((s, on));
            }
        }
        let (s, on) = best?;
        Some(Projection {
            s,
            point: on.point()?,
            distance: p.sub(on).length(),
        })
    }

    /// Open distance and the applicable one-sided unit tangent. Endpoints
    /// clamp; a sharp corner never averages its arriving and leaving ways.
    pub(in crate::cycles) fn sample(&self, s: f64, incoming: bool) -> Option<(Point, Point)> {
        if !s.is_finite() {
            return None;
        }
        let s = s.clamp(0., self.length);
        let (mut i, mut piece) = self
            .pieces
            .iter()
            .enumerate()
            .find(|(_, p)| s < p.start + p.length)
            .or_else(|| self.pieces.iter().enumerate().next_back())?;
        let mut t = piece.parameter(s - piece.start);
        let mut point = piece.curve.at(t).point()?;
        // A length shift can round just beside a boundary while its native
        // point is already exactly that knot. Canonicalize only coincident
        // points within scalar roundoff to retain the chosen corner tangent.
        if let Some((index, boundary)) = self.pieces.iter().enumerate().skip(1).find(|(_, p)| {
            (s - p.start).abs() <= 8. * f64::EPSILON * self.length
                && p.curve.ends().0.point() == Some(point)
        }) {
            i = index;
            piece = boundary;
            t = 0.;
            point = boundary.curve.ends().0.point()?;
        }
        let tangent = if incoming && t == 0. {
            i.checked_sub(1)
                .and_then(|i| self.pieces.get(i))
                .map_or_else(|| piece.curve.tangent(t), |p| p.curve.tangent(1.))
        } else {
            piece.curve.tangent(t)
        };
        Some((point, tangent.point()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        arrows::{ArrowStyle, Preset},
        graphics::PathCommand::*,
    };

    #[test]
    fn open_distance_and_projection_keep_both_ends_and_one_sided_corners() {
        let shaft = Shaft::new(&[
            Move(Point::default()),
            Line(Point::new(900., 0.)),
            Line(Point::new(900., 100.)),
        ])
        .unwrap();
        assert_eq!(shaft.length(), 1000.);
        assert_eq!(
            shaft.sample(500., false).unwrap(),
            (Point::new(500., 0.), Point::new(1., 0.))
        );
        assert_eq!(
            shaft.sample(900., true).unwrap(),
            (Point::new(900., 0.), Point::new(1., 0.))
        );
        assert_eq!(
            shaft.sample(900., false).unwrap(),
            (Point::new(900., 0.), Point::new(0., 1.))
        );
        assert_eq!(shaft.sample(-100., true).unwrap().0, Point::default());
        assert_eq!(
            shaft.sample(1100., false).unwrap().0,
            Point::new(900., 100.)
        );
        assert_eq!(shaft.project(Point::new(900., 100.)).unwrap().s, 1000.);
        assert!(shaft.sample(f64::NAN, false).is_none());
        assert!(shaft.project(Point::new(f32::INFINITY, 0.)).is_none());
    }

    #[test]
    fn native_head_trim_and_zero_length_commands_do_not_wrap_or_erase_loops() {
        for length in [0.01, 10., 50.] {
            let a = Arrow::new(
                1,
                Point::default(),
                Point::new(length, 0.),
                Preset::Forward,
                ArrowStyle::DEFAULT,
            );
            let shaft = Shaft::from_arrow(&a).unwrap();
            assert!(shaft.length() < f64::from(length));
            let end = shaft.sample(shaft.length(), false).unwrap().0;
            assert!(end.x < length);
            assert_eq!(shaft.project(a.end).unwrap().s, shaft.length());
            assert_eq!(shaft.sample(shaft.length() * 10., true).unwrap().0, end);
        }
        let origin = Point::default();
        let shaft = Shaft::new(&[
            Move(origin),
            Cubic(origin, origin, origin),
            Cubic(Point::new(100., -100.), Point::new(100., 100.), origin),
        ])
        .unwrap();
        assert_eq!(shaft.pieces.len(), 1);
        assert!(shaft.length() > 100.);
        assert!(shaft.project(Point::new(75., 0.)).unwrap().distance < 1e-6);
    }

    #[test]
    fn global_projection_checks_all_pieces_and_keeps_earliest_tie() {
        let shaft = Shaft::new(&[
            Move(Point::default()),
            Line(Point::new(100., 0.)),
            Line(Point::new(100., 100.)),
            Line(Point::new(0., 100.)),
        ])
        .unwrap();
        assert_eq!(shaft.project(Point::new(0., 50.)).unwrap().s, 0.);
        assert_eq!(shaft.project(Point::new(0., 100.)).unwrap().s, 300.);
        assert_eq!(
            shaft.project(Point::new(10., 120.)).unwrap().point,
            Point::new(10., 100.)
        );
    }
}
