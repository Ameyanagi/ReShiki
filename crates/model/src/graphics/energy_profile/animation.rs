//! Playback artwork shares the placed figure's layout without editing it.

use super::*;

impl Graphic {
    /// Current level, marker on the drawn trace, and an optional replacement
    /// molecular picture. `sample` identifies the step as well as its position.
    /// Its energy belongs to the readout; the marker follows the actual curve,
    /// including the smooth joins between the IRC and the level bars.
    pub fn energy_profile_overlay(
        &self,
        style: &DrawingStyle,
        source: &Document,
        state: usize,
        sample: Option<(usize, EnergyProfileSample)>,
        geometry: Option<&EnergyProfileGeometry>,
    ) -> Vec<Primitive> {
        let Some(profile) = self.energy_profile.as_deref() else {
            return vec![];
        };
        let layout = Layout::new(profile, style);
        let Some(&level) = layout.states.get(state) else {
            return vec![];
        };
        let accent = Color::Palette(Hue::Green, Row::Strong);
        let line_style = GraphicStyle {
            stroke: accent,
            width_pt: (self.style.width_pt * 3.).clamp(1.5, 12.),
            flat_caps: true,
            ..GraphicStyle::default()
        };
        let mut out = vec![Primitive::Path {
            commands: vec![
                PathCommand::Move(layout.world(self, level.offset(-layout.bar / 2., 0.))),
                PathCommand::Line(layout.world(self, level.offset(layout.bar / 2., 0.))),
            ],
            style: line_style.clone(),
            filled: false,
        }];
        let marker = sample
            .and_then(|(step, sample)| layout.sample_point(profile, step, sample.x))
            .unwrap_or(level);
        let marker = layout.world(self, marker);
        let radius = (style.text_style().size() * layout.text_scale(self) * 0.32).max(0.5);
        out.push(Primitive::Path {
            commands: circle(marker, radius),
            style: GraphicStyle {
                stroke: Color::Paper,
                fill: Some(accent),
                width_pt: (self.style.width_pt * 1.3).clamp(0.7, 12.),
                ..GraphicStyle::default()
            },
            filled: true,
        });
        if let Some(geometry) = geometry {
            let slot = sample
                .and_then(|(index, _)| profile.steps.get(index))
                .and_then(|step| step.transition)
                .unwrap_or(state);
            if let Some(slot) = layout.pictures.iter().find(|picture| picture.state == slot)
                && let Some(picture) = geometry_picture(
                    geometry,
                    profile.rotation,
                    layout.world(self, slot.centre),
                    layout.picture_world_side(self),
                    source,
                )
            {
                out.extend(picture.into_iter().map(|primitive| {
                    if let Some(alpha) = self.style.effects.opacity {
                        Primitive::Opacity {
                            alpha,
                            primitive: Box::new(primitive),
                        }
                    } else {
                        primitive
                    }
                }));
            }
        }
        out
    }
}

impl Layout {
    fn sample_point(&self, profile: &EnergyProfile, index: usize, x: f64) -> Option<Point> {
        if !x.is_finite() {
            return None;
        }
        let step = profile.steps.get(index)?;
        let x = x.clamp(0., 1.);
        let (from, to, t) = match step.transition {
            Some(ts) if x <= 0.5 => (step.from, ts, x * 2.),
            Some(ts) => (ts, step.to, (x - 0.5) * 2.),
            None => (step.from, step.to, x),
        };
        let (a, b) = (*self.states.get(from)?, *self.states.get(to)?);
        if t == 0. {
            return Some(a);
        }
        if t == 1. {
            return Some(b);
        }
        // The IRC trace occupies the gap between the level bars.
        let x = a.x + self.bar / 2. + (b.x - a.x - self.bar) * t as f32;
        point_at_x(self.curves.get(from)?, x)
    }
}

/// Connectors are monotonic in x. Solve their exact Bézier parameter instead
/// of interpolating energies that no longer describe the blended end joins.
fn point_at_x(commands: &[PathCommand], x: f32) -> Option<Point> {
    let mut at = None;
    for command in commands {
        match *command {
            PathCommand::Move(point) => at = Some(point),
            PathCommand::Line(end) => {
                let start = at?;
                if start.x <= x && x <= end.x {
                    let width = end.x - start.x;
                    if width == 0. {
                        return Some(end);
                    }
                    let t = (x - start.x) / width;
                    return Some(Point::new(x, start.y + (end.y - start.y) * t));
                }
                at = Some(end);
            }
            PathCommand::Cubic(a, b, end) => {
                let start = at?;
                if start.x <= x && x <= end.x {
                    let evaluate = |t: f32| {
                        let s = 1. - t;
                        let value = |p0, p1, p2, p3| {
                            s * s * s * p0
                                + 3. * s * s * t * p1
                                + 3. * s * t * t * p2
                                + t * t * t * p3
                        };
                        Point::new(
                            value(start.x, a.x, b.x, end.x),
                            value(start.y, a.y, b.y, end.y),
                        )
                    };
                    let (mut lo, mut hi) = (0., 1.);
                    for _ in 0..28 {
                        let mid = (lo + hi) / 2.;
                        if evaluate(mid).x < x {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    return Some(evaluate((lo + hi) / 2.));
                }
                at = Some(end);
            }
            _ => {}
        }
    }
    None
}

fn circle(centre: Point, radius: f32) -> Vec<PathCommand> {
    let h = radius * 0.552_284_8;
    vec![
        PathCommand::Move(centre.offset(radius, 0.)),
        PathCommand::Cubic(
            centre.offset(radius, h),
            centre.offset(h, radius),
            centre.offset(0., radius),
        ),
        PathCommand::Cubic(
            centre.offset(-h, radius),
            centre.offset(-radius, h),
            centre.offset(-radius, 0.),
        ),
        PathCommand::Cubic(
            centre.offset(-radius, -h),
            centre.offset(-h, -radius),
            centre.offset(0., -radius),
        ),
        PathCommand::Cubic(
            centre.offset(h, -radius),
            centre.offset(radius, -h),
            centre.offset(radius, 0.),
        ),
        PathCommand::Close,
    ]
}
