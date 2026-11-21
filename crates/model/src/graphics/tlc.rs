//! Native TLC plate geometry. Lanes and labels belong to one editable graphic.
use super::{Graphic, GraphicStyle, LinePattern, PathCommand};
use crate::{
    document::{Annotation, Point},
    scientific::Part,
    typography::TextFormat,
};
use serde::{Deserialize, Serialize};

pub const MAX_TLC_LANES: usize = 256;
pub const MAX_TLC_SPOTS: usize = 4096;

/// A fourth corner in the graphic's local affine frame. `(1, 1)` is a rectangle
/// or parallelogram; all four corners follow ordinary affine graphic transforms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlcPlate {
    pub bottom_right: Point,
    pub origin_fraction: f64,
    pub solvent_front_fraction: f64,
    pub show_origin: bool,
    pub show_solvent_front: bool,
    pub show_borders: bool,
    pub lanes: Vec<TlcLane>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlcLane {
    pub spots: Vec<TlcSpot>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlcSpot {
    pub rf: f64,
    /// Nominal physical dimensions, in points, before ChemDraw's stroke allowance.
    /// f64 retains every signed 16.16 value at the editable interchange boundary.
    pub width_pt: f64,
    pub height_pt: f64,
    pub style: GraphicStyle,
    pub show_rf: bool,
    pub label: Option<TlcRfLabel>,
}
impl TlcSpot {
    /// Move a spot without overwriting custom Rf text. A standard native label
    /// follows the value at its existing precision, retaining its rich style.
    pub fn set_rf(&mut self, value: f64) -> Result<bool, String> {
        if !value.is_finite() || !(0. ..=1.).contains(&value) {
            return Err("TLC Rf must be a finite value from 0 to 1".into());
        }
        if self.rf == value {
            return Ok(false);
        }
        const PREFIX: &str = "Rf = ";
        if let Some(label) = &mut self.label
            && label.text.len() <= crate::document::MAX_ANNOTATION_TEXT
            && let Some(decimal) = label.text.strip_prefix(PREFIX)
        {
            let (whole, fraction) = decimal
                .split_once('.')
                .map_or((decimal, None), |(a, b)| (a, Some(b)));
            if !whole.is_empty()
                && whole.bytes().all(|c| c.is_ascii_digit())
                && fraction
                    .is_none_or(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
            {
                let precision = fraction.map_or(0, str::len);
                if decimal == format!("{:.*}", precision, self.rf) {
                    let text = format!("{PREFIX}{value:.*}", precision);
                    if text != label.text {
                        let name_breaks = label.format.name_breaks;
                        label
                            .format
                            .edited(&label.text, &text, PREFIX.len()..label.text.len());
                        label.format.name_breaks = name_breaks;
                        label.text = text;
                    }
                }
            }
        }
        self.rf = value;
        Ok(true)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlcRfLabel {
    pub text: String,
    pub format: TextFormat,
    /// Physical drawing-unit offset from the spot, in the plate's unit axes.
    pub offset: Point,
    /// Explicit clockwise text turn; frame reflections never mirror its glyphs.
    pub angle: f32,
}
impl Default for TlcPlate {
    fn default() -> Self {
        Self {
            bottom_right: Point::new(1., 1.),
            origin_fraction: 0.1,
            solvent_front_fraction: 0.1,
            show_origin: true,
            show_solvent_front: true,
            show_borders: true,
            lanes: vec![],
        }
    }
}
impl TlcPlate {
    pub fn validate(&self, g: &Graphic) -> Result<(), String> {
        if g.style.fill.is_some()
            || g.style.corner_pt.is_some()
            || g.style.effects.shaded
            || g.style.effects.gradient.is_some()
            || g.style.effects.shadow_pt.is_some()
        {
            return Err("Unsupported TLC plate appearance".into());
        }
        let br = self.bottom_right;
        let fourth = Point::new(
            g.axis_x.x * br.x + g.axis_y.x * br.y,
            g.axis_x.y * br.x + g.axis_y.y * br.y,
        );
        let (x, y) = (
            g.axis_x.distance(Point::default()),
            g.axis_y.distance(Point::default()),
        );
        if !br.is_finite()
            || br.x <= 1e-6
            || br.y <= 1e-6
            || br.x + br.y <= 1.000001
            || !x.is_finite()
            || !y.is_finite()
            || !(0.01..=1_000_000.).contains(&x)
            || !(0.01..=1_000_000.).contains(&y)
            || g.frame_determinant().abs() <= x * y * 1e-6
            || !self.point(g, 1., 1.).is_finite()
            || !(0.01..=1_000_000.).contains(&fourth.distance(g.axis_x))
            || !(0.01..=1_000_000.).contains(&fourth.distance(g.axis_y))
        {
            return Err("TLC plate corners must form a finite convex quadrilateral".into());
        }
        if ![self.origin_fraction, self.solvent_front_fraction]
            .into_iter()
            .all(|f| f.is_finite() && (0. ..=1.).contains(&f))
            || self.origin_fraction + self.solvent_front_fraction >= 1.
        {
            return Err("TLC origin must lie below the solvent front".into());
        }
        if self.lanes.len() > MAX_TLC_LANES
            || self.lanes.iter().map(|l| l.spots.len()).sum::<usize>() > MAX_TLC_SPOTS
        {
            return Err("TLC plate exceeds its lane or spot limit".into());
        }
        let mut text_bytes = 0usize;
        for (lane, spot) in self
            .lanes
            .iter()
            .enumerate()
            .flat_map(|(i, l)| l.spots.iter().map(move |s| (i, s)))
        {
            if !spot.rf.is_finite()
                || !(0. ..=1.).contains(&spot.rf)
                || ![spot.width_pt, spot.height_pt]
                    .into_iter()
                    .all(|n| n.is_finite() && (1. / 65536. ..=32767.).contains(&n))
            {
                return Err("Invalid TLC spot fraction or physical dimensions".into());
            }
            spot.style.validate()?;
            if !spot.style.effects.is_plain()
                || spot.style.corner_pt.is_some()
                || !matches!(spot.style.pattern, LinePattern::Solid | LinePattern::Dashed)
            {
                return Err("Unsupported TLC spot appearance".into());
            }
            if spot.show_rf && spot.label.is_none() {
                return Err("A shown TLC Rf value needs a label".into());
            }
            if let Some(label) = &spot.label {
                label.format.validate(&label.text)?;
                text_bytes = text_bytes
                    .checked_add(label.text.len())
                    .ok_or("TLC label limit exceeded")?;
                let (x, y) = self.basis(g);
                let at = self.spot_center(g, lane, spot.rf).offset(
                    x.x * label.offset.x + y.x * label.offset.y,
                    x.y * label.offset.x + y.y * label.offset.y,
                );
                if !at.is_finite()
                    || label.text.len() > crate::document::MAX_ANNOTATION_TEXT
                    || !label.offset.is_finite()
                    || !label.angle.is_finite()
                    || !(0. ..360.).contains(&label.angle)
                {
                    return Err("Invalid TLC Rf label".into());
                }
            }
        }
        if text_bytes > crate::document::MAX_DRAWING_TEXT {
            return Err("TLC label limit exceeded".into());
        }
        Ok(())
    }
    pub fn point(&self, g: &Graphic, u: f32, v: f32) -> Point {
        let local = Point::new(
            u + (self.bottom_right.x - 1.) * u * v,
            v + (self.bottom_right.y - 1.) * u * v,
        );
        g.origin.offset(
            g.axis_x.x * local.x + g.axis_y.x * local.y,
            g.axis_x.y * local.x + g.axis_y.y * local.y,
        )
    }
    pub fn basis(&self, g: &Graphic) -> (Point, Point) {
        let unit = |p: Point| {
            let n = p.distance(Point::default()).max(1e-12);
            Point::new(p.x / n, p.y / n)
        };
        (unit(g.axis_x), unit(g.axis_y))
    }
    /// Native lanes are cell centres inside five-point bottom-edge margins.
    /// Keep at least one native 16.16 point tick of available span when those
    /// margins cannot fit. This stays ordered and continuous across ten points,
    /// including when native coordinate rounding crosses that boundary.
    pub fn lane_fraction(bottom_width_pt: f64, lane: usize, count: usize) -> f64 {
        let cell = (lane as f64 + 0.5) / count.max(1) as f64;
        if bottom_width_pt.is_finite() && bottom_width_pt > 0. {
            let span = (bottom_width_pt - 10.)
                .max(1. / 65536.)
                .min(bottom_width_pt);
            (0.5 * (bottom_width_pt - span) + cell * span) / bottom_width_pt
        } else {
            cell
        }
    }
    fn lane_point(&self, g: &Graphic, lane: usize, v: f32) -> Point {
        let left = self.point(g, 0., v);
        let bottom_left = self.point(g, 0., 1.);
        let bottom_right = self.point(g, 1., 1.);
        let width_pt = f64::from(bottom_left.distance(bottom_right))
            * f64::from(crate::style::DEFAULT.points_per_world());
        let u = Self::lane_fraction(width_pt, lane, self.lanes.len()) as f32;
        left.offset(
            (bottom_right.x - bottom_left.x) * u,
            (bottom_right.y - bottom_left.y) * u,
        )
    }
    pub fn spot_center(&self, g: &Graphic, lane: usize, rf: f64) -> Point {
        let v = 1.
            - self.origin_fraction
            - rf * (1. - self.origin_fraction - self.solvent_front_fraction);
        self.lane_point(g, lane, v as f32)
    }
    /// An authored label's record, including one the spot currently hides.
    pub fn label_annotation(&self, g: &Graphic, lane: usize, spot: &TlcSpot) -> Option<Annotation> {
        let label = spot.label.as_ref()?;
        let (x, y) = self.basis(g);
        let centre = self.spot_center(g, lane, spot.rf);
        Some(Annotation {
            id: g.id,
            position: centre.offset(
                x.x * label.offset.x + y.x * label.offset.y,
                x.y * label.offset.x + y.y * label.offset.y,
            ),
            text: label.text.clone(),
            format: label.format.clone(),
            angle: label.angle,
        })
    }
    pub fn labels(&self, g: &Graphic) -> Vec<Annotation> {
        self.lanes
            .iter()
            .enumerate()
            .flat_map(|(i, lane)| {
                lane.spots
                    .iter()
                    .filter(|s| s.show_rf)
                    .filter_map(move |spot| self.label_annotation(g, i, spot))
            })
            .collect()
    }
    pub fn parts(&self, g: &Graphic) -> Vec<Part> {
        use PathCommand::{Close, Cubic, Line, Move};
        let mut parts = vec![];
        let mut line = |commands: Vec<PathCommand>, pattern| {
            let commands = if pattern == LinePattern::Wavy {
                super::wavy(&commands, super::wave_radius(g.style.width_pt), [0.; 2])
            } else {
                commands
            };
            let mut style = g.style.clone();
            style.fill = None;
            style.pattern = pattern;
            parts.push(Part {
                commands,
                style,
                filled: false,
            });
        };
        if self.show_borders {
            line(
                vec![
                    Move(self.point(g, 0., 0.)),
                    Line(self.point(g, 1., 0.)),
                    Line(self.point(g, 1., 1.)),
                    Line(self.point(g, 0., 1.)),
                    Close,
                ],
                g.style.pattern,
            );
        }
        let (x, y) = self.basis(g);
        let bottom_left = self.point(g, 0., 1.);
        let bottom_right = self.point(g, 1., 1.);
        let bottom = Point::new(
            bottom_right.x - bottom_left.x,
            bottom_right.y - bottom_left.y,
        );
        for (shown, v, span) in [
            (self.show_origin, 1. - self.origin_fraction, bottom),
            (
                self.show_solvent_front,
                self.solvent_front_fraction,
                g.axis_x,
            ),
        ] {
            if shown {
                let start = self.point(g, 0., v as f32);
                line(
                    vec![Move(start), Line(start.offset(span.x, span.y))],
                    LinePattern::Dashed,
                );
            }
        }
        if self.show_origin {
            // Native origin ticks are filled rectangles, six line widths tall.
            // A round-capped stroke would add a width to their visible height.
            let half = g.style.width() * 3.;
            let side = g.style.width() * 0.5;
            for i in 0..self.lanes.len() {
                let p = self.spot_center(g, i, 0.);
                let at = |a: f32, b: f32| {
                    p.offset(
                        x.x * side * a + y.x * half * b,
                        x.y * side * a + y.y * half * b,
                    )
                };
                let mut style = g.style.clone();
                style.fill = Some(style.stroke);
                style.width_pt = 0.;
                style.pattern = LinePattern::Solid;
                parts.push(Part {
                    commands: vec![
                        Move(at(-1., -1.)),
                        Line(at(1., -1.)),
                        Line(at(1., 1.)),
                        Line(at(-1., 1.)),
                        Close,
                    ],
                    style,
                    filled: true,
                });
            }
        }
        for (i, lane) in self.lanes.iter().enumerate() {
            for spot in &lane.spots {
                let c = self.spot_center(g, i, spot.rf);
                let w = crate::style::DEFAULT
                    .world((spot.width_pt + f64::from(spot.style.width_pt)) as f32)
                    * 0.5;
                let h = crate::style::DEFAULT
                    .world((spot.height_pt + f64::from(spot.style.width_pt)) as f32)
                    * 0.5;
                let at =
                    |a: f32, b: f32| c.offset(x.x * w * a + y.x * h * b, x.y * w * a + y.y * h * b);
                let k = 0.552_284_8;
                let mut style = spot.style.clone();
                let filled = style.fill.is_some();
                if filled {
                    style.width_pt = 0.;
                }
                parts.push(Part {
                    commands: vec![
                        Move(at(1., 0.)),
                        Cubic(at(1., k), at(k, 1.), at(0., 1.)),
                        Cubic(at(-k, 1.), at(-1., k), at(-1., 0.)),
                        Cubic(at(-1., -k), at(-k, -1.), at(0., -1.)),
                        Cubic(at(k, -1.), at(1., -k), at(1., 0.)),
                        Close,
                    ],
                    style,
                    filled,
                });
            }
        }
        parts
    }
    pub fn colors(&self) -> impl Iterator<Item = crate::palette::Color> + '_ {
        self.lanes.iter().flat_map(|l| &l.spots).flat_map(|s| {
            std::iter::once(s.style.stroke)
                .chain(s.style.fill)
                .chain(s.label.iter().flat_map(|l| {
                    std::iter::once(l.format.style.color)
                        .chain(l.format.spans.iter().map(|s| s.style.color))
                }))
        })
    }
    pub fn for_each_color_mut(&mut self, mut visit: impl FnMut(&mut crate::palette::Color)) {
        for spot in self.lanes.iter_mut().flat_map(|l| &mut l.spots) {
            visit(&mut spot.style.stroke);
            if let Some(c) = &mut spot.style.fill {
                visit(c);
            }
            if let Some(label) = &mut spot.label {
                visit(&mut label.format.style.color);
                for span in &mut label.format.spans {
                    visit(&mut span.style.color);
                }
            }
        }
    }
}
impl Graphic {
    pub fn tlc_labels(&self) -> Vec<Annotation> {
        self.tlc
            .as_deref()
            .map_or_else(Vec::new, |t| t.labels(self))
    }
}

#[cfg(test)]
mod tests;
