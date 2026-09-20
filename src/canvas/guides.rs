//! Canvas-only measuring aids; coordinates share the drawing/export scale.
use super::{Camera, rgb};
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::{Point, Rectangle, Size};

pub const RULER_WIDTH: f32 = 38.;
pub const RULER_HEIGHT: f32 = 26.;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Unit {
    #[default]
    Millimetres,
    Centimetres,
    Inches,
    Points,
}
impl Unit {
    pub const ALL: [Self; 4] = [
        Self::Millimetres,
        Self::Centimetres,
        Self::Inches,
        Self::Points,
    ];

    pub fn per_world(self) -> f64 {
        let points = f64::from(reshiki::style::DEFAULT.points_per_world());
        points
            * match self {
                Self::Millimetres => 25.4 / 72.,
                Self::Centimetres => 2.54 / 72.,
                Self::Inches => 1. / 72.,
                Self::Points => 1.,
            }
    }
    fn precision(self) -> usize {
        match self {
            Self::Millimetres | Self::Points => 1,
            Self::Centimetres | Self::Inches => 2,
        }
    }
}
impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Millimetres => "mm",
            Self::Centimetres => "cm",
            Self::Inches => "in",
            Self::Points => "pt",
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Guides {
    pub rulers: bool,
    pub crosshair: bool,
    pub unit: Unit,
}
impl Guides {
    pub fn paper(self, bounds: Rectangle) -> Rectangle {
        let (x, y) = if self.rulers {
            (RULER_WIDTH, RULER_HEIGHT)
        } else {
            (0., 0.)
        };
        Rectangle {
            x: bounds.x + x,
            y: bounds.y + y,
            width: (bounds.width - x).max(0.),
            height: (bounds.height - y).max(0.),
        }
    }

    pub fn draw_crosshair(self, frame: &mut Frame, camera: Camera, pointer: Option<Point>) {
        let Some(p) = pointer.filter(|_| self.crosshair) else {
            return;
        };
        let stroke = Stroke {
            line_dash: canvas::LineDash {
                segments: &[4., 4.],
                offset: 0,
            },
            ..Stroke::default().with_color(iced::Color::from_rgba8(17, 126, 108, 0.45))
        };
        for (a, b) in [
            (Point::new(0., p.y), Point::new((p.x - 7.).max(0.), p.y)),
            (
                Point::new((p.x + 7.).min(frame.width()), p.y),
                Point::new(frame.width(), p.y),
            ),
            (Point::new(p.x, 0.), Point::new(p.x, (p.y - 7.).max(0.))),
            (
                Point::new(p.x, (p.y + 7.).min(frame.height())),
                Point::new(p.x, frame.height()),
            ),
        ] {
            frame.stroke(&Path::line(a, b), stroke);
        }
        let world = camera.world(p, Rectangle::with_size(frame.size()));
        let scale = self.unit.per_world();
        let precision = self.unit.precision();
        // Fixed corner readout stays clear of the pointer and drawing gesture.
        let position = Point::new((frame.width() - 240.).max(4.), 8.);
        frame.fill_rectangle(
            position,
            Size::new(232., 22.),
            iced::Color::from_rgba8(245, 249, 247, 0.94),
        );
        frame.fill_text(canvas::Text {
            content: format!(
                "x  {:.precision$}    y  {:.precision$}  {}",
                f64::from(world.x) * scale,
                f64::from(world.y) * scale,
                self.unit
            ),
            position: Point::new(position.x + 8., position.y + 4.),
            color: rgb([47, 107, 94]),
            size: 11.into(),
            ..Default::default()
        });
    }

    pub fn draw_rulers(self, frame: &mut Frame, camera: Camera, pointer: Option<Point>) {
        if !self.rulers {
            return;
        }
        let paper = self.paper(Rectangle::with_size(frame.size()));
        let background = rgb([244, 246, 247]);
        let line = Stroke::default().with_color(rgb([165, 179, 182]));
        frame.fill_rectangle(
            Point::ORIGIN,
            Size::new(frame.width(), RULER_HEIGHT),
            background,
        );
        frame.fill_rectangle(
            Point::ORIGIN,
            Size::new(RULER_WIDTH, frame.height()),
            background,
        );
        frame.stroke(
            &Path::line(
                Point::new(RULER_WIDTH, RULER_HEIGHT),
                Point::new(frame.width(), RULER_HEIGHT),
            ),
            line,
        );
        frame.stroke(
            &Path::line(
                Point::new(RULER_WIDTH, RULER_HEIGHT),
                Point::new(RULER_WIDTH, frame.height()),
            ),
            line,
        );
        for (horizontal, center, length, inset) in [
            (true, camera.center.x, paper.width, RULER_WIDTH),
            (false, camera.center.y, paper.height, RULER_HEIGHT),
        ] {
            for tick in ticks(center, camera.zoom, length, self.unit) {
                let position = inset + tick.pixel;
                let size = if tick.label.is_some() { 9. } else { 4. };
                let (a, b) = if horizontal {
                    (
                        Point::new(position, RULER_HEIGHT - size),
                        Point::new(position, RULER_HEIGHT),
                    )
                } else {
                    (
                        Point::new(RULER_WIDTH - size, position),
                        Point::new(RULER_WIDTH, position),
                    )
                };
                frame.stroke(&Path::line(a, b), line);
                if let Some(content) = tick.label {
                    // Keep edge labels inside the ruler, away from its unit corner.
                    if tick.pixel < 8. || tick.pixel > length - 26. {
                        continue;
                    }
                    frame.fill_text(canvas::Text {
                        content,
                        position: if horizontal {
                            Point::new(position + 3., 3.)
                        } else {
                            Point::new(RULER_WIDTH - 5., position - 14.)
                        },
                        align_x: if horizontal {
                            iced::alignment::Horizontal::Left.into()
                        } else {
                            iced::alignment::Horizontal::Right.into()
                        },
                        color: rgb([91, 105, 111]),
                        size: 10.into(),
                        ..Default::default()
                    });
                }
            }
        }
        frame.fill_text(canvas::Text {
            content: self.unit.to_string(),
            position: Point::new(9., 7.),
            color: rgb([67, 94, 89]),
            size: 10.into(),
            ..Default::default()
        });
        if let Some(p) = pointer {
            let accent = Stroke::default()
                .with_width(2.)
                .with_color(rgb([17, 126, 108]));
            frame.stroke(
                &Path::line(
                    Point::new(p.x + RULER_WIDTH, 0.),
                    Point::new(p.x + RULER_WIDTH, RULER_HEIGHT),
                ),
                accent,
            );
            frame.stroke(
                &Path::line(
                    Point::new(0., p.y + RULER_HEIGHT),
                    Point::new(RULER_WIDTH, p.y + RULER_HEIGHT),
                ),
                accent,
            );
        }
    }
}

#[derive(Debug)]
struct Tick {
    pixel: f32,
    label: Option<String>,
}

fn ticks(center: f32, zoom: f32, length: f32, unit: Unit) -> Vec<Tick> {
    if !center.is_finite() || !zoom.is_finite() || zoom <= 0. || !length.is_finite() || length <= 0.
    {
        return vec![];
    }
    let pixels_per_unit = f64::from(zoom) / unit.per_world();
    let desired = 64. / pixels_per_unit;
    let base = 10_f64.powf(desired.log10().floor());
    let major = [1., 2., 5., 10.]
        .into_iter()
        .map(|n| n * base)
        .find(|step| *step >= desired)
        .unwrap_or(base * 10.);
    let minor = major / 5.;
    let left = f64::from(center) * unit.per_world() - f64::from(length) / pixels_per_unit / 2.;
    let first = (left / minor).ceil();
    let decimals = (-major.log10().floor()).clamp(0., 6.) as usize;
    let mut result = Vec::new();
    // Finite, bounded work even for extreme imported coordinates or a bad camera.
    for offset in 0..2048 {
        let index = first + f64::from(offset);
        let value = index * minor;
        let pixel = (value - left) * pixels_per_unit;
        if pixel > f64::from(length) {
            break;
        }
        if pixel < 0. || !pixel.is_finite() {
            continue;
        }
        let label = (index.rem_euclid(5.).abs() < 0.0001).then(|| {
            let value = if value.abs() < minor * 0.01 {
                0.
            } else {
                value
            };
            format!("{value:.decimals$}")
        });
        result.push(Tick {
            pixel: pixel as f32,
            label,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::World;

    #[test]
    fn rulers_use_export_scale_and_follow_camera() {
        assert!((42. * Unit::Points.per_world() - 14.4).abs() < 0.00001);
        assert!((42. * Unit::Millimetres.per_world() - 5.08).abs() < 0.00001);
        for unit in Unit::ALL {
            for zoom in [0.25, 1., 1.7, 5.] {
                let camera = Camera {
                    center: World::new(42., -84.),
                    zoom,
                };
                for center in [camera.center.x, camera.center.y] {
                    let ticks = ticks(center, zoom, 800., unit);
                    assert!(ticks.len() > 4 && ticks.len() < 100);
                    let mut previous = None;
                    for tick in ticks {
                        if let Some(label) = tick.label {
                            let value: f64 = label.parse().expect("numeric ruler label");
                            let expected = ((value / unit.per_world() - f64::from(center))
                                * f64::from(zoom)
                                + 400.) as f32;
                            assert!((tick.pixel - expected).abs() < 0.001);
                            if let Some(p) = previous {
                                assert!(tick.pixel - p >= 63.9);
                            }
                            previous = Some(tick.pixel);
                        }
                    }
                }
            }
        }
        for invalid in [0., -1., f32::NAN, f32::INFINITY] {
            assert!(ticks(0., invalid, 800., Unit::Points).is_empty());
        }
    }
}
