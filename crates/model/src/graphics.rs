//! Editable vector graphics in drawing coordinates. Shared by preview and export.
mod arc;
mod effects;
mod heads;
pub mod panel;
mod path;
mod superscript;
use crate::{document::Point, style};
pub use arc::ArcGeometry;
pub(crate) use effects::bounding_frame;
pub use effects::{
    EXPORT_NOTICE, Effects, GLOW_NOTICE, Gradient, GradientKind, SHADOW_RGB, export_notice, shade,
    tiled, wave_radius, wavy,
};
pub use heads::PathHeads;
pub use panel::{Divider, Panel, TitlePlace};
pub use path::{MIN_CLOSED_NODES, MIN_OPEN_NODES, PathHandle, PathNodeType, PathSummary};
use serde::{Deserialize, Serialize};
pub use superscript::GraphicLabel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicKind {
    Picture,
    Symbol(crate::scientific::SymbolKind),
    Orbital(crate::scientific::OrbitalKind),
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Line,
    Arc,
    Brackets,
    Parentheses,
    Braces,
    Curve,
    Path,
}
impl GraphicKind {
    pub const DRAWABLE: [Self; 10] = [
        Self::Rectangle,
        Self::RoundedRectangle,
        Self::Ellipse,
        Self::Line,
        Self::Arc,
        Self::Brackets,
        Self::Parentheses,
        Self::Braces,
        Self::Curve,
        Self::Path,
    ];
    pub fn closed(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::RoundedRectangle | Self::Ellipse
        )
    }
    pub fn brackets(self) -> bool {
        matches!(self, Self::Brackets | Self::Parentheses | Self::Braces)
    }
}
impl std::fmt::Display for GraphicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Symbol(kind) => return write!(f, "{kind}"),
            Self::Orbital(kind) => return write!(f, "{kind}"),
            Self::Rectangle => "Rectangle",
            Self::RoundedRectangle => "Rounded rectangle",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
            Self::Arc => "Arc",
            Self::Brackets => "Brackets",
            Self::Parentheses => "Parentheses",
            Self::Braces => "Braces",
            Self::Curve => "Bézier curve",
            Self::Path => "Pen path",
            Self::Picture => "Picture",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinePattern {
    #[default]
    Solid,
    Dashed,
    Dotted,
    /// Half-circle waves along the line, as ChemDraw's wavy line.
    Wavy,
}
impl LinePattern {
    pub const ALL: [Self; 4] = [Self::Solid, Self::Dashed, Self::Dotted, Self::Wavy];
    /// The patterns of strokes that cannot wave, such as symbols.
    pub const PLAIN: [Self; 3] = [Self::Solid, Self::Dashed, Self::Dotted];
}
impl std::fmt::Display for LinePattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::Wavy => "Wavy",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BracketSides {
    #[default]
    Both,
    Left,
    Right,
}
impl std::fmt::Display for BracketSides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Both => "Both sides",
            Self::Left => "Left only",
            Self::Right => "Right only",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicStyle {
    pub stroke: crate::palette::Color,
    pub fill: Option<crate::palette::Color>,
    pub width_pt: f32,
    pub pattern: LinePattern,
    #[serde(skip_serializing_if = "Effects::is_plain")]
    pub effects: Effects,
    /// A rounded rectangle's corner radius; `None` rounds 18% of the shorter side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_pt: Option<f32>,
}
impl Default for GraphicStyle {
    fn default() -> Self {
        Self {
            stroke: crate::palette::Color::Ink,
            fill: None,
            width_pt: 0.6,
            pattern: LinePattern::Solid,
            effects: Effects::default(),
            corner_pt: None,
        }
    }
}
/// The largest corner radius, beyond any page ChemDraw draws.
pub const MAX_CORNER_PT: f32 = 1000.;
impl GraphicStyle {
    pub fn width(&self) -> f32 {
        style::DEFAULT.world(self.width_pt)
    }
    pub fn dashes(&self) -> Vec<f32> {
        let w = self.width();
        match self.pattern {
            LinePattern::Solid | LinePattern::Wavy => vec![],
            LinePattern::Dashed => vec![w * 5.0, w * 3.0],
            LinePattern::Dotted => vec![w * 0.1, w * 2.5],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.width_pt.is_finite() || !(0.1..=12.0).contains(&self.width_pt) {
            return Err("Graphic line width must be 0.1–12 pt".into());
        }
        if self
            .corner_pt
            .is_some_and(|c| !c.is_finite() || !(0. ..=MAX_CORNER_PT).contains(&c))
        {
            return Err("Corner radius must be 0–1000 pt".into());
        }
        if self.effects.gradient.is_some() && self.fill.is_none() {
            return Err("A gradient needs a fill color".into());
        }
        self.effects.validate()
    }
}
#[derive(Debug, Clone)]
pub enum GraphicChange {
    Stroke(crate::palette::Color),
    Fill(Option<crate::palette::Color>),
    Width(f32),
    Pattern(LinePattern),
    Shaded(bool),
    Shadow(Option<f32>),
    Corner(Option<f32>),
    Opacity(Option<f32>),
    Gradient(Option<Gradient>),
    /// A gradient of this kind for each shape, keeping its own start color
    /// and, from linear to linear, its own angle; `None` removes it.
    GradientKind(Option<GradientKind>),
    /// A new start color for each shape's own gradient.
    GradientFrom(crate::palette::Color),
    /// A new angle for each shape's own linear gradient.
    GradientAngle(f32),
}
impl GraphicChange {
    pub fn apply(&self, s: &mut GraphicStyle) {
        match self {
            Self::Stroke(v) => s.stroke = *v,
            Self::Fill(v) => {
                s.fill = *v;
                // Shading and gradients paint the fill, so no fill means neither.
                if v.is_none() {
                    s.effects.shaded = false;
                    s.effects.gradient = None;
                }
            }
            Self::Width(v) => s.width_pt = *v,
            Self::Pattern(v) => s.pattern = *v,
            Self::Shaded(v) => {
                s.effects.shaded = *v;
                // Shading paints the fill color, so an outline gains one.
                if *v && s.fill.is_none() {
                    s.fill = Some(s.stroke);
                }
                // Shading replaces a gradient and is always opaque.
                if *v {
                    s.effects.gradient = None;
                    s.effects.opacity = None;
                }
            }
            Self::Shadow(v) => s.effects.shadow_pt = *v,
            Self::Corner(v) => s.corner_pt = *v,
            Self::Opacity(v) => {
                s.effects.opacity = *v;
                if v.is_some() {
                    s.effects.shaded = false;
                }
            }
            Self::Gradient(v) => {
                s.effects.gradient = *v;
                if v.is_some() {
                    s.effects.shaded = false;
                    s.fill = s.fill.or(Some(s.stroke));
                }
            }
            Self::GradientKind(None) => s.effects.gradient = None,
            Self::GradientKind(Some(kind)) => {
                let gradient = match s.effects.gradient {
                    Some(
                        g @ Gradient {
                            kind: GradientKind::Linear { .. },
                            ..
                        },
                    ) if matches!(kind, GradientKind::Linear { .. }) => g,
                    Some(g) => Gradient { kind: *kind, ..g },
                    None => Gradient {
                        from: Gradient::WHITE,
                        kind: *kind,
                    },
                };
                Self::Gradient(Some(gradient)).apply(s);
            }
            Self::GradientFrom(v) => {
                if let Some(gradient) = &mut s.effects.gradient {
                    gradient.from = *v;
                }
            }
            Self::GradientAngle(v) => {
                if let Some(Gradient {
                    kind: GradientKind::Linear { angle },
                    ..
                }) = &mut s.effects.gradient
                {
                    *angle = *v;
                }
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "points", rename_all = "snake_case")]
pub enum PathCommand {
    Move(Point),
    Line(Point),
    Cubic(Point, Point, Point),
    Close,
}
impl PathCommand {
    pub fn map(&self, mut f: impl FnMut(Point) -> Point) -> Self {
        match self {
            Self::Move(p) => Self::Move(f(*p)),
            Self::Line(p) => Self::Line(f(*p)),
            Self::Cubic(a, b, c) => Self::Cubic(f(*a), f(*b), f(*c)),
            Self::Close => Self::Close,
        }
    }
    pub fn points(&self) -> Vec<Point> {
        self.iter_points().collect()
    }
    pub(crate) fn iter_points(&self) -> impl Iterator<Item = Point> {
        let points = match self {
            Self::Move(p) | Self::Line(p) => [Some(*p), None, None],
            Self::Cubic(a, b, c) => [Some(*a), Some(*b), Some(*c)],
            Self::Close => [None, None, None],
        };
        points.into_iter().flatten()
    }
}

fn zero_depth(value: &[f32; 3]) -> bool {
    *value == [0.; 3]
}

/// An affine frame preserves rotated and reflected shapes without flattening them.
// The remote derive supplies wire helpers; arc.rs adds the compatible path fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Graphic")]
pub struct Graphic {
    /// Depth of origin, axis_x and axis_y for reversible 3D projection.
    #[serde(default, skip_serializing_if = "zero_depth")]
    pub depth: [f32; 3],
    /// None retains the half-ellipse frame used by older native drawings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<ArcGeometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<crate::pictures::Picture>,
    pub id: u64,
    pub kind: GraphicKind,
    pub origin: Point,
    pub axis_x: Point,
    pub axis_y: Point,
    #[serde(default)]
    pub style: GraphicStyle,
    #[serde(default)]
    pub sides: BracketSides,
    /// An annotation attached to the closing bracket, without chemical meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superscript: Option<String>,
    #[serde(default)]
    pub phase: crate::scientific::Phase,
    #[serde(default)]
    pub phase_flipped: bool,
    /// Negative layers are behind chemical structures; positive layers are in front.
    #[serde(default = "back_layer")]
    pub layer: i32,
    /// Local commands for a custom editable path; unused by parametric shapes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<PathCommand>,
    /// Explicit node behavior in connected-path order; empty uses geometric detection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_types: Vec<PathNodeType>,
    /// Optional arrowheads on a single open curve or path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<PathHeads>,
    /// Titles, corners and dividers that make a rectangle a figure panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<Panel>,
    /// A callout leader's attached ends; only lines have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<crate::leaders::Leader>,
    /// The intermediates and steps that make an ellipse a catalytic cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<crate::cycles::Cycle>,
}
fn back_layer() -> i32 {
    -1
}
impl Graphic {
    /// Determinant of the drawing frame; callers choose their own singularity threshold.
    pub(crate) fn frame_determinant(&self) -> f32 {
        self.axis_x.x * self.axis_y.y - self.axis_x.y * self.axis_y.x
    }
    /// Frame coordinates of a world point; `determinant` is the checked `frame_determinant()`.
    pub(crate) fn world_to_local(&self, p: Point, determinant: f32) -> Point {
        let delta = Point::new(p.x - self.origin.x, p.y - self.origin.y);
        Point::new(
            (delta.x * self.axis_y.y - delta.y * self.axis_y.x) / determinant,
            (self.axis_x.x * delta.y - self.axis_x.y * delta.x) / determinant,
        )
    }
    /// Identity drawing frame, for paths stored in world units.
    pub(crate) fn reset_frame(&mut self) {
        self.origin = Point::default();
        self.axis_x = Point::new(1., 0.);
        self.axis_y = Point::new(0., 1.);
    }
    pub fn dragged(
        id: u64,
        kind: GraphicKind,
        start: Point,
        end: Point,
        style: GraphicStyle,
        sides: BracketSides,
        constrain: bool,
    ) -> Self {
        let mut style = style;
        if !kind.closed() {
            // Shading, shadows and gradients stay with the drawing style for
            // the next closed shape, not a line or bracket drawn in between;
            // a see-through line keeps its opacity.
            style.effects = Effects {
                opacity: style.effects.opacity,
                ..Effects::default()
            };
        }
        if kind != GraphicKind::RoundedRectangle {
            style.corner_pt = None;
        }
        // Symbols and orbitals draw no waves.
        if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_))
            && style.pattern == LinePattern::Wavy
        {
            style.pattern = LinePattern::Solid;
        }
        if kind == GraphicKind::Path {
            return Self::pen_curve(id, start, end, style);
        }
        if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
            let delta = Point::new(end.x - start.x, end.y - start.y);
            let length = delta.distance(Point::default());
            let default = if matches!(kind, GraphicKind::Symbol(_)) {
                style::DEFAULT.world(8.)
            } else {
                42.
            };
            let length = if length < 3. { default } else { length };
            let angle = if delta.distance(Point::default()) < 3. {
                -std::f32::consts::FRAC_PI_2
            } else {
                delta.y.atan2(delta.x)
            };
            let angle = if constrain {
                (angle / (std::f32::consts::PI / 12.)).round() * (std::f32::consts::PI / 12.)
            } else {
                angle
            };
            let axis = if matches!(kind, GraphicKind::Symbol(_)) {
                Point::new(length, 0.)
            } else {
                Point::new(length * angle.cos(), length * angle.sin())
            };
            return Self {
                depth: [0.; 3],
                arc: None,
                picture: None,
                id,
                kind,
                origin: start,
                axis_x: axis,
                axis_y: Point::new(-axis.y, axis.x),
                style,
                sides,
                superscript: None,
                phase: Default::default(),
                phase_flipped: false,
                layer: if matches!(kind, GraphicKind::Symbol(_)) {
                    1
                } else {
                    -1
                },
                path: vec![],
                node_types: vec![],
                heads: None,
                panel: None,
                leader: None,
                cycle: None,
            };
        }
        let mut dx = end.x - start.x;
        let mut dy = end.y - start.y;
        if constrain {
            if matches!(kind, GraphicKind::Line | GraphicKind::Curve) {
                let angle = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round()
                    * std::f32::consts::FRAC_PI_4;
                let length = dx.hypot(dy);
                dx = length * angle.cos();
                dy = length * angle.sin();
            } else {
                let size = dx.abs().max(dy.abs());
                dx = size * if dx < 0.0 { -1.0 } else { 1.0 };
                dy = size * if dy < 0.0 { -1.0 } else { 1.0 };
            }
        }
        let (origin, axis_x, axis_y) = if matches!(kind, GraphicKind::Line | GraphicKind::Curve) {
            (start, Point::new(dx, dy), Point::new(-dy * 0.5, dx * 0.5))
        } else {
            (
                Point::new(start.x.min(start.x + dx), start.y.min(start.y + dy)),
                Point::new(dx.abs().max(1.0), 0.0),
                Point::new(0.0, dy.abs().max(1.0)),
            )
        };
        Self {
            depth: [0.; 3],
            arc: None,
            picture: None,
            id,
            kind,
            origin,
            axis_x,
            axis_y,
            style,
            sides,
            superscript: None,
            phase: Default::default(),
            phase_flipped: false,
            layer: -1,
            path: vec![],
            node_types: vec![],
            heads: None,
            panel: None,
            leader: None,
            cycle: None,
        }
    }
    /// A rounded rectangle's corner radius in drawing units, at most half its
    /// shorter side.
    pub fn corner_radius(&self) -> f32 {
        let side = self
            .axis_x
            .distance(Point::default())
            .min(self.axis_y.distance(Point::default()));
        self.style
            .corner_pt
            .map_or(side * 0.18, |c| style::DEFAULT.world(c))
            .min(side * 0.5)
    }
    pub fn map_positions(&mut self, f: impl Fn(Point) -> Point) {
        let o = f(self.origin);
        let x = f(self.origin.offset(self.axis_x.x, self.axis_x.y));
        let y = f(self.origin.offset(self.axis_y.x, self.axis_y.y));
        self.origin = o;
        self.axis_x = Point::new(x.x - o.x, x.y - o.y);
        self.axis_y = Point::new(y.x - o.x, y.y - o.y);
        self.normalize_panel();
    }
    pub fn commands(&self) -> Vec<PathCommand> {
        if matches!(self.kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
            return self.parts().into_iter().flat_map(|p| p.commands).collect();
        }
        use PathCommand::*;
        let p = Point::new;
        let k = 0.552_284_8;
        let local = match self.kind {
            GraphicKind::Rectangle | GraphicKind::Picture => vec![
                Move(p(0., 0.)),
                Line(p(1., 0.)),
                Line(p(1., 1.)),
                Line(p(0., 1.)),
                Close,
            ],
            GraphicKind::RoundedRectangle => {
                let radius = self.corner_radius();
                let rx = radius / self.axis_x.distance(Point::default()).max(0.001);
                let ry = radius / self.axis_y.distance(Point::default()).max(0.001);
                vec![
                    Move(p(rx, 0.)),
                    Line(p(1. - rx, 0.)),
                    Cubic(p(1. - rx + k * rx, 0.), p(1., ry - k * ry), p(1., ry)),
                    Line(p(1., 1. - ry)),
                    Cubic(
                        p(1., 1. - ry + k * ry),
                        p(1. - rx + k * rx, 1.),
                        p(1. - rx, 1.),
                    ),
                    Line(p(rx, 1.)),
                    Cubic(p(rx - k * rx, 1.), p(0., 1. - ry + k * ry), p(0., 1. - ry)),
                    Line(p(0., ry)),
                    Cubic(p(0., ry - k * ry), p(rx - k * rx, 0.), p(rx, 0.)),
                    Close,
                ]
            }
            GraphicKind::Ellipse => vec![
                Move(p(1., 0.5)),
                Cubic(p(1., 0.5 + k * 0.5), p(0.5 + k * 0.5, 1.), p(0.5, 1.)),
                Cubic(p(0.5 - k * 0.5, 1.), p(0., 0.5 + k * 0.5), p(0., 0.5)),
                Cubic(p(0., 0.5 - k * 0.5), p(0.5 - k * 0.5, 0.), p(0.5, 0.)),
                Cubic(p(0.5 + k * 0.5, 0.), p(1., 0.5 - k * 0.5), p(1., 0.5)),
                Close,
            ],
            GraphicKind::Line => vec![Move(p(0., 0.)), Line(p(1., 0.))],
            GraphicKind::Arc => {
                if let Some(arc) = self.arc {
                    arc.commands()
                } else {
                    vec![
                        Move(p(0., 1.)),
                        Cubic(p(0., 1. - k), p(0.5 - k * 0.5, 0.), p(0.5, 0.)),
                        Cubic(p(0.5 + k * 0.5, 0.), p(1., 1. - k), p(1., 1.)),
                    ]
                }
            }
            GraphicKind::Curve => {
                vec![Move(p(0., 0.)), Cubic(p(0.33, -1.), p(0.67, 1.), p(1., 0.))]
            }
            GraphicKind::Path => self.path.clone(),
            kind => {
                let mut commands = vec![];
                let lip = (self.axis_y.distance(Point::default()) * 0.1)
                    .min(style::DEFAULT.world(8.0))
                    / self.axis_x.distance(Point::default()).max(0.001);
                for right in [false, true] {
                    if (right && self.sides == BracketSides::Left)
                        || (!right && self.sides == BracketSides::Right)
                    {
                        continue;
                    }
                    let shape = match kind {
                        GraphicKind::Brackets => vec![
                            Move(p(lip, 0.)),
                            Line(p(0., 0.)),
                            Line(p(0., 1.)),
                            Line(p(lip, 1.)),
                        ],
                        GraphicKind::Parentheses => vec![
                            Move(p(lip, 0.)),
                            Cubic(p(-lip * 0.33, 0.2), p(-lip * 0.33, 0.8), p(lip, 1.)),
                        ],
                        _ => vec![
                            Move(p(lip, 0.)),
                            Cubic(p(0., 0.), p(lip * 0.6, 0.42), p(0., 0.5)),
                            Cubic(p(lip * 0.6, 0.58), p(0., 1.), p(lip, 1.)),
                        ],
                    };
                    commands.extend(
                        shape
                            .into_iter()
                            .map(|c| c.map(|q| if right { p(1. - q.x, q.y) } else { q })),
                    );
                }
                commands
            }
        };
        local
            .into_iter()
            .map(|c| {
                c.map(|p| {
                    self.origin.offset(
                        self.axis_x.x * p.x + self.axis_y.x * p.y,
                        self.axis_x.y * p.x + self.axis_y.y * p.y,
                    )
                })
            })
            .collect()
    }
    pub fn bounds(&self) -> (Point, Point) {
        self.bounds_with_text_style(&crate::typography::TextStyle::default())
    }
    /// Visible graphic extents using the drawing's font for bracket annotations.
    pub fn bounds_with_text_style(
        &self,
        text_style: &crate::typography::TextStyle,
    ) -> (Point, Point) {
        let (mut lo, mut hi) = self.shape_bounds();
        let extras = self
            .panel_parts(text_style)
            .into_iter()
            .flat_map(|part| part.commands)
            .flat_map(|c| c.points())
            .map(|p| (p, p));
        let titles = self
            .superscript_label(text_style)
            .into_iter()
            .chain(self.panel_titles(text_style))
            .flat_map(|label| label.ink_boxes());
        for (a, b) in extras.chain(titles) {
            lo.x = lo.x.min(a.x);
            lo.y = lo.y.min(a.y);
            hi.x = hi.x.max(b.x);
            hi.y = hi.y.max(b.y);
        }
        (lo, hi)
    }
    /// Vector extents, excluding an attached superscript.
    pub fn shape_bounds(&self) -> (Point, Point) {
        let commands: Vec<_> = self.parts().into_iter().flat_map(|p| p.commands).collect();
        let mut points = commands
            .iter()
            .flat_map(PathCommand::iter_points)
            .peekable();
        let mut lo = points.peek().copied().unwrap_or(self.origin);
        let mut hi = lo;
        for p in points {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        let pad = if self.kind == GraphicKind::Picture {
            0.
        } else {
            self.style.width() * 0.5
        };
        (lo.offset(-pad, -pad), hi.offset(pad, pad))
    }
    pub fn filled(&self) -> bool {
        self.style.fill.is_some()
            && (self.kind.closed()
                || self
                    .commands()
                    .iter()
                    .any(|c| matches!(c, PathCommand::Close)))
    }
    pub fn hit(&self, p: Point, r: f32) -> bool {
        self.hit_with_text_style(p, r, &crate::typography::TextStyle::default())
    }
    pub fn hit_with_text_style(
        &self,
        p: Point,
        r: f32,
        text_style: &crate::typography::TextStyle,
    ) -> bool {
        if self
            .superscript_label(text_style)
            .into_iter()
            .chain(self.panel_titles(text_style))
            .any(|label| {
                label.ink_boxes().into_iter().any(|(lo, hi)| {
                    p.x >= lo.x - r && p.x <= hi.x + r && p.y >= lo.y - r && p.y <= hi.y + r
                })
            })
            || self.panel_parts(text_style).iter().any(|part| {
                flattened(&part.commands).iter().any(|path| {
                    if part.filled {
                        inside_polygon(p, path)
                    } else {
                        // Dividers are lines: a click near one picks the panel.
                        path.windows(2).any(|pair| {
                            matches!(pair, [a, b] if segment_distance(p, *a, *b) < r + part.style.width() * 0.5)
                        })
                    }
                })
            })
        {
            return true;
        }
        if self.kind == GraphicKind::Picture {
            return flattened(&self.commands()).iter().any(|path| {
                inside_polygon(p, path)
                    || path
                        .windows(2)
                        .any(|pair| matches!(pair,[a,b] if segment_distance(p,*a,*b)<r))
            });
        }
        self.parts().iter().any(|part| {
            flattened(&part.commands).iter().any(|path| {
                path.windows(2).any(|pair| matches!(pair,[a,b] if segment_distance(p,*a,*b)<r+self.style.width()*0.5)) || (part.filled && inside_polygon(p, path))
            })
        })
    }

    /// Edit one handle, preserving a connected path if its frame rejects the edit.
    pub fn edit_point(&mut self, index: usize, p: Point) -> Result<(), String> {
        // Dragging a callout leader's end releases it from its target, and
        // the leader stays a line.
        let mut ends = self.line_ends();
        if let Some(leader) = self.leader.as_mut() {
            match index {
                0 => leader.start = None,
                1 => leader.end = None,
                _ => return Ok(()),
            }
            if *leader == Default::default() {
                self.leader = None;
            }
            if !p.is_finite() {
                return Err("Non-finite graphic point".into());
            }
            if let Some(end) = ends.get_mut(index) {
                *end = p;
            }
            self.set_line_ends(ends);
            return Ok(());
        }
        if self.kind == GraphicKind::Arc {
            self.edit_arc_endpoint(index, p);
            return Ok(());
        }
        if self.kind == GraphicKind::Picture {
            return Ok(());
        }
        if self.panel.is_some() {
            return Err("A panel keeps its shape: resize it with its handles".into());
        }
        if let Some(result) = self.edit_connected_path_point(index, p) {
            return result;
        }
        let mut commands = self.commands();
        let mut n = 0;
        for c in &mut commands {
            *c = c.map(|old| {
                let value = if n == index { p } else { old };
                n += 1;
                value
            });
        }
        self.kind = GraphicKind::Path;
        self.superscript = None;
        self.reset_frame();
        self.path = commands;
        self.node_types.clear();
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.style.validate()?;
        if self.superscript.is_some() && !self.kind.brackets() {
            return Err("A superscript requires a bracket graphic".into());
        }
        if let Some(heads) = self.heads {
            if !matches!(
                self.kind,
                GraphicKind::Line | GraphicKind::Curve | GraphicKind::Path
            ) {
                return Err("Path arrowheads do not match the graphic kind".into());
            }
            heads.validate()?;
        }
        if let Some(panel) = &self.panel {
            if !self.kind.panel_capable() {
                return Err("Panel settings require a rectangle".into());
            }
            panel.validate()?;
        }
        if let Some(cycle) = &self.cycle {
            if self.kind != GraphicKind::Ellipse {
                return Err("A catalytic cycle must be an ellipse".into());
            }
            cycle.validate()?;
        }
        if let Some(arc) = self.arc {
            if self.kind != GraphicKind::Arc {
                return Err("Arc parameters do not match the graphic kind".into());
            }
            arc.validate()?;
        }
        if (self.kind == GraphicKind::Picture) != self.picture.is_some() {
            return Err("Picture data does not match the graphic kind".into());
        }
        if self.picture.is_some() {
            let x = self.axis_x.distance(Point::default());
            let y = self.axis_y.distance(Point::default());
            let dot = self.axis_x.x * self.axis_y.x + self.axis_x.y * self.axis_y.y;
            if !(0.01..=1_000_000.).contains(&x)
                || !(0.01..=1_000_000.).contains(&y)
                || dot.abs() > x * y * 0.0001
            {
                return Err("Picture frames must be rectangular with positive dimensions".into());
            }
        }
        if self.path.len() > 20_000
            || (self.kind == GraphicKind::Path
                && !matches!(self.path.first(), Some(PathCommand::Move(_))))
        {
            return Err("Invalid graphic path".into());
        }
        if !self.node_types.is_empty()
            && (self.kind != GraphicKind::Path
                || self
                    .path_summary()
                    .is_none_or(|summary| summary.node_count() != self.node_types.len()))
        {
            return Err("Path node types do not match the graphic's connected nodes".into());
        }
        if [self.origin, self.axis_x, self.axis_y]
            .into_iter()
            .chain(self.path.iter().flat_map(PathCommand::iter_points))
            .any(|p| !p.is_finite())
        {
            return Err("Non-finite graphic coordinates".into());
        }
        Ok(())
    }
}
pub fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / length
    } else {
        0.0
    };
    p.distance(a.offset(dx * t.clamp(0., 1.), dy * t.clamp(0., 1.)))
}
// windows(2): an unclosed contour has no closing edge, unlike the closed-contour tests in
// scientific/label_halo.rs `contains` and mark_placement.rs `intersects`.
fn inside_polygon(p: Point, path: &[Point]) -> bool {
    let mut inside = false;
    for pair in path.windows(2) {
        let [a, b] = pair else {
            continue;
        };
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
pub fn flattened(commands: &[PathCommand]) -> Vec<Vec<Point>> {
    let mut paths: Vec<Vec<Point>> = vec![];
    for c in commands {
        match *c {
            PathCommand::Move(p) => paths.push(vec![p]),
            PathCommand::Line(p) => {
                if let Some(path) = paths.last_mut() {
                    path.push(p);
                }
            }
            PathCommand::Close => {
                if let Some(path) = paths.last_mut()
                    && let Some(p) = path.first().copied()
                {
                    path.push(p);
                }
            }
            PathCommand::Cubic(a, b, end) => {
                if let Some(path) = paths.last_mut()
                    && let Some(start) = path.last().copied()
                {
                    for step in 1..=32 {
                        let t = step as f32 / 32.;
                        let u = 1. - t;
                        path.push(Point::new(
                            u * u * u * start.x
                                + 3. * u * u * t * a.x
                                + 3. * u * t * t * b.x
                                + t * t * t * end.x,
                            u * u * u * start.y
                                + 3. * u * u * t * a.y
                                + 3. * u * t * t * b.y
                                + t * t * t * end.y,
                        ));
                    }
                }
            }
        }
    }
    paths
}

#[cfg(test)]
mod heads_tests;
#[cfg(test)]
mod node_type_tests;
#[cfg(test)]
mod path_tests;
