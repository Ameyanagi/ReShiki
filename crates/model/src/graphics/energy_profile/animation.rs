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
        let layout = Layout::with_frame(profile, style, Some(self), Some(source));
        let Some(&level) = layout.states.get(state) else {
            return vec![];
        };
        let accent = Color::Palette(Hue::Green, Row::Strong);
        let mut out = vec![Primitive::Path {
            commands: vec![
                PathCommand::Move(layout.world(self, level.offset(-layout.bar / 2., 0.))),
                PathCommand::Line(layout.world(self, level.offset(layout.bar / 2., 0.))),
            ],
            style: GraphicStyle {
                stroke: accent,
                width_pt: layout.bar_width_pt,
                flat_caps: true,
                ..GraphicStyle::default()
            },
            filled: false,
        }];
        let marker = sample
            .and_then(|(step, sample)| layout.sample_point(profile, step, sample.x))
            .unwrap_or(level);
        if let Some((radius, width_pt)) = layout.marker_size(self, marker) {
            out.push(Primitive::Path {
                commands: circle(layout.world(self, marker), radius),
                style: GraphicStyle {
                    stroke: if width_pt > 0. { Color::Paper } else { accent },
                    fill: Some(accent),
                    width_pt,
                    ..GraphicStyle::default()
                },
                filled: true,
            });
        }
        if let Some(geometry) = geometry {
            let slot = sample
                .and_then(|(index, _)| profile.steps.get(index))
                .and_then(|step| step.transition)
                .unwrap_or(state);
            if let Some(slot) = layout.pictures.iter().find(|picture| picture.state == slot)
                && let Some(picture) =
                    geometry_picture(geometry, profile.rotation, layout.picture_side, source)
            {
                out.extend(
                    layout
                        .replacement_picture(self, source, slot, picture)
                        .into_iter()
                        .map(|primitive| {
                            if let Some(alpha) = self.style.effects.opacity {
                                Primitive::Opacity {
                                    alpha,
                                    primitive: Box::new(primitive),
                                }
                            } else {
                                primitive
                            }
                        }),
                );
            }
        }
        out
    }
}

impl Layout {
    fn marker_size(&self, g: &Graphic, marker: Point) -> Option<(f32, f32)> {
        let [x, y] = frame_projection(g)?;
        let edge_distance = |t: f32, row: Point| t.min(1. - t) / row.x.hypot(row.y);
        let clearance = edge_distance(marker.x / self.width, x)
            .min(edge_distance((marker.y - self.min_y) / self.height, y));
        let scale = (g.axis_x.x.hypot(g.axis_x.y) / self.width)
            .min(g.axis_y.x.hypot(g.axis_y.y) / self.height);
        // Leave room for an outline whose width is at most half the radius.
        let radius = (self.font.size() * scale * 0.32).min(clearance / 1.25);
        let width_pt = (g.style.width_pt * 1.3)
            .clamp(0.7, 12.)
            .min(self.bar_width_pt)
            .min(style::DEFAULT.points_per_world() * radius * 0.5);
        let width_pt = if width_pt >= 0.1 { width_pt } else { 0. };
        // Keep the fill visible while leaving room for any outline.
        let radius = radius
            .max(style::DEFAULT.world(0.5))
            .min(clearance - style::DEFAULT.world(width_pt) / 2.);
        (radius.is_finite() && radius > 0.).then_some((radius, width_pt))
    }

    fn replacement_picture(
        &self,
        g: &Graphic,
        source: &Document,
        slot: &PictureSlot,
        mut picture: Vec<Primitive>,
    ) -> Vec<Primitive> {
        let (lo, hi) = crate::scene::bounds(&picture);
        let (slot_lo, slot_hi) = crate::scene::bounds(&slot.artwork);
        // Keep the moving model inside the space reserved beside the label.
        let margin = 2. * style::DEFAULT.world(4.);
        let fit = ((slot_hi.x - slot_lo.x - margin) / (hi.x - lo.x - margin))
            .min((slot_hi.y - slot_lo.y - margin) / (hi.y - lo.y - margin))
            .min(1.);
        let side = self.picture_world_side(g);
        let scale = side / self.picture_side * fit;
        if !scale.is_finite() || scale <= 0. {
            return vec![];
        }
        let centre = self.world(g, slot.centre);
        let ink_centre = lo.lerp(hi, 0.5);
        let change_bond_width = source.drawing_style.line_width_pt * 0.85;
        let outline_width = (side / 400.).clamp(0.6, 1.1);
        for primitive in &mut picture {
            place_picture_ink(
                primitive,
                centre,
                ink_centre,
                scale,
                change_bond_width,
                outline_width,
            );
        }
        picture
    }

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
        let start = a.x + self.bar / 2.;
        let end = b.x - self.bar / 2.;
        let x = start + (end - start) * t as f32;
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
