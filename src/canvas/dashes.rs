use iced::Point;
use iced::advanced::graphics::geometry::path::lyon_path::{
    Event,
    geom::{CubicBezierSegment, QuadraticBezierSegment, euclid::default::Point2D},
};
use iced::widget::canvas::{Path, path::Builder};

type Curve = CubicBezierSegment<f32>;

/// Split strokes at dash boundaries while retaining the curve inside each dash.
/// The GPU backend's built-in dashing joins these boundaries with straight chords.
pub(super) fn dashed(path: &Path, pattern: &[f32]) -> Path {
    let Some(&first) = pattern.first() else {
        return path.clone();
    };
    if pattern.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return path.clone();
    }
    Path::new(|builder| {
        let mut dash = Dashes {
            pattern,
            index: 0,
            remaining: first,
            drawing: false,
        };
        for event in path.raw() {
            match event {
                Event::Begin { .. } => dash.reset(),
                Event::Line { from, to } => dash.line(builder, from, to),
                Event::Quadratic { from, ctrl, to } => dash.curve(
                    builder,
                    QuadraticBezierSegment { from, ctrl, to }.to_cubic(),
                ),
                Event::Cubic {
                    from,
                    ctrl1,
                    ctrl2,
                    to,
                } => {
                    dash.curve(
                        builder,
                        Curve {
                            from,
                            ctrl1,
                            ctrl2,
                            to,
                        },
                    );
                }
                Event::End {
                    last,
                    first,
                    close: true,
                } => dash.line(builder, last, first),
                Event::End { .. } => {}
            }
        }
    })
}

fn point(p: Point2D<f32>) -> Point {
    Point::new(p.x, p.y)
}

struct Dashes<'a> {
    pattern: &'a [f32],
    index: usize,
    remaining: f32,
    drawing: bool,
}

impl Dashes<'_> {
    fn reset(&mut self) {
        self.index = 0;
        self.remaining = self.pattern.first().copied().unwrap_or(f32::INFINITY);
        self.drawing = false;
    }

    fn intervals(&mut self, length: f32, mut emit: impl FnMut(f32, f32, bool)) {
        let mut distance = 0.;
        while distance < length {
            let end = (distance + self.remaining).min(length);
            if end <= distance {
                break;
            }
            if self.index.is_multiple_of(2) {
                emit(distance, end, !self.drawing);
                self.drawing = true;
            }
            self.remaining -= end - distance;
            distance = end;
            if self.remaining <= f32::EPSILON * length.max(1.) {
                self.index += 1;
                self.remaining = self
                    .pattern
                    .get(self.index % self.pattern.len())
                    .copied()
                    .unwrap_or(f32::INFINITY);
                if !self.index.is_multiple_of(2) {
                    self.drawing = false;
                }
            }
        }
    }

    fn line(&mut self, builder: &mut Builder, from: Point2D<f32>, to: Point2D<f32>) {
        let length = (to - from).length();
        self.intervals(length, |a, b, begin| {
            if begin {
                builder.move_to(point(from.lerp(to, a / length)));
            }
            builder.line_to(point(from.lerp(to, b / length)));
        });
    }

    fn curve(&mut self, builder: &mut Builder, curve: Curve) {
        // Flatten only to measure arc length; the visible dash remains an exact
        // Bezier subsection, even at high zoom or across a rounded corner.
        let mut samples = vec![(0., 0.)];
        let mut length = 0.;
        curve.for_each_flattened_with_t(0.02, &mut |line, range| {
            length += line.length();
            samples.push((length, range.end));
        });
        let parameter = |distance: f32| {
            let index = samples.partition_point(|(d, _)| *d < distance);
            let (d1, t1) = samples.get(index).copied().unwrap_or((length, 1.));
            let (d0, t0) = samples
                .get(index.saturating_sub(1))
                .copied()
                .unwrap_or((0., 0.));
            if d1 <= d0 {
                t1
            } else {
                t0 + (t1 - t0) * (distance - d0) / (d1 - d0)
            }
        };
        self.intervals(length, |a, b, begin| {
            let part = curve.split_range(parameter(a)..parameter(b));
            if begin {
                builder.move_to(point(part.from));
            }
            builder.bezier_curve_to(point(part.ctrl1), point(part.ctrl2), point(part.to));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_dashes_stay_on_the_curve_at_high_zoom() {
        use reshiki::{
            document::Point as World,
            graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
        };
        let ellipse = Graphic::dragged(
            1,
            GraphicKind::Ellipse,
            World::new(-250., -250.),
            World::new(250., 250.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        );
        let path = Path::new(|b| {
            for c in ellipse.commands() {
                let p = |v: World| Point::new(v.x, v.y);
                match c {
                    PathCommand::Move(v) => b.move_to(p(v)),
                    PathCommand::Line(v) => b.line_to(p(v)),
                    PathCommand::Cubic(a, z, v) => b.bezier_curve_to(p(a), p(z), p(v)),
                    PathCommand::Close => b.close(),
                }
            }
        });
        let dashes = dashed(&path, &[80., 30.]);
        let mut curves = 0;
        for event in dashes.raw() {
            if let Event::Cubic {
                from,
                ctrl1,
                ctrl2,
                to,
            } = event
            {
                let curve = Curve {
                    from,
                    ctrl1,
                    ctrl2,
                    to,
                };
                for i in 0..=20 {
                    let p = curve.sample(i as f32 / 20.);
                    assert!((p.to_vector().length() - 250.).abs() < 0.1);
                }
                curves += 1;
            }
            assert!(!matches!(event, Event::Line { .. }));
        }
        assert!(curves > 10);
    }

    #[test]
    fn a_dash_crossing_a_rectangle_corner_keeps_the_corner() {
        let path = Path::new(|b| {
            b.move_to(Point::ORIGIN);
            b.line_to(Point::new(10., 0.));
            b.line_to(Point::new(10., 10.));
        });
        let result = dashed(&path, &[15., 5.]);
        let lines: Vec<_> = result
            .raw()
            .into_iter()
            .filter_map(|e| {
                if let Event::Line { from, to } = e {
                    Some((from, to))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].1, Point2D::new(10., 0.));
        assert_eq!(lines[1].0, lines[0].1);
        assert_eq!(lines[1].1, Point2D::new(10., 5.));
    }
}
