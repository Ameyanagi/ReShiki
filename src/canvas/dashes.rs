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
mod tests;
