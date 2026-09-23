//! Editable vector graphics in drawing coordinates. Shared by preview and export.
use crate::{document::Point, style};
use serde::{Deserialize, Serialize};

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
    pub const DRAWABLE: [Self; 9] = [
        Self::Rectangle,
        Self::RoundedRectangle,
        Self::Ellipse,
        Self::Line,
        Self::Arc,
        Self::Brackets,
        Self::Parentheses,
        Self::Braces,
        Self::Curve,
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
            Self::Path => "Custom path",
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
}
impl std::fmt::Display for LinePattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
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
    pub stroke: [u8; 3],
    pub fill: Option<[u8; 3]>,
    pub width_pt: f32,
    pub pattern: LinePattern,
}
impl Default for GraphicStyle {
    fn default() -> Self {
        Self {
            stroke: [0, 0, 0],
            fill: None,
            width_pt: 0.6,
            pattern: LinePattern::Solid,
        }
    }
}
impl GraphicStyle {
    pub fn width(&self) -> f32 {
        style::DEFAULT.world(self.width_pt)
    }
    pub fn dashes(&self) -> Vec<f32> {
        let w = self.width();
        match self.pattern {
            LinePattern::Solid => vec![],
            LinePattern::Dashed => vec![w * 5.0, w * 3.0],
            LinePattern::Dotted => vec![w * 0.1, w * 2.5],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.width_pt.is_finite() || !(0.1..=12.0).contains(&self.width_pt) {
            return Err("Graphic line width must be 0.1–12 pt".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub enum GraphicChange {
    Stroke([u8; 3]),
    Fill(Option<[u8; 3]>),
    Width(f32),
    Pattern(LinePattern),
}
impl GraphicChange {
    pub fn apply(&self, s: &mut GraphicStyle) {
        match self {
            Self::Stroke(v) => s.stroke = *v,
            Self::Fill(v) => s.fill = *v,
            Self::Width(v) => s.width_pt = *v,
            Self::Pattern(v) => s.pattern = *v,
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
        match self {
            Self::Move(p) | Self::Line(p) => vec![*p],
            Self::Cubic(a, b, c) => vec![*a, *b, *c],
            Self::Close => vec![],
        }
    }
}

/// An affine frame preserves rotated and reflected shapes without flattening them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graphic {
    /// Depth of origin, axis_x and axis_y for reversible 3D projection.
    #[serde(default)]
    pub depth: [f32; 3],
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
}
fn back_layer() -> i32 {
    -1
}
impl Graphic {
    pub fn dragged(
        id: u64,
        kind: GraphicKind,
        start: Point,
        end: Point,
        style: GraphicStyle,
        sides: BracketSides,
        constrain: bool,
    ) -> Self {
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
                picture: None,
                id,
                kind,
                origin: start,
                axis_x: axis,
                axis_y: Point::new(-axis.y, axis.x),
                style,
                sides,
                phase: Default::default(),
                phase_flipped: false,
                layer: if matches!(kind, GraphicKind::Symbol(_)) {
                    1
                } else {
                    -1
                },
                path: vec![],
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
            picture: None,
            id,
            kind,
            origin,
            axis_x,
            axis_y,
            style,
            sides,
            phase: Default::default(),
            phase_flipped: false,
            layer: -1,
            path: vec![],
        }
    }
    pub fn map_positions(&mut self, f: impl Fn(Point) -> Point) {
        let o = f(self.origin);
        let x = f(self.origin.offset(self.axis_x.x, self.axis_x.y));
        let y = f(self.origin.offset(self.axis_y.x, self.axis_y.y));
        self.origin = o;
        self.axis_x = Point::new(x.x - o.x, x.y - o.y);
        self.axis_y = Point::new(y.x - o.x, y.y - o.y);
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
                let radius = self
                    .axis_x
                    .distance(Point::default())
                    .min(self.axis_y.distance(Point::default()))
                    * 0.18;
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
            GraphicKind::Arc => vec![
                Move(p(0., 1.)),
                Cubic(p(0., 1. - k), p(0.5 - k * 0.5, 0.), p(0.5, 0.)),
                Cubic(p(0.5 + k * 0.5, 0.), p(1., 1. - k), p(1., 1.)),
            ],
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
        let points: Vec<_> = self
            .commands()
            .iter()
            .flat_map(PathCommand::points)
            .collect();
        let mut lo = self.origin;
        let mut hi = lo;
        if let Some(first) = points.first() {
            lo = *first;
            hi = *first;
        }
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

    pub fn edit_point(&mut self, index: usize, p: Point) {
        if self.kind == GraphicKind::Picture {
            return;
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
        self.origin = Point::default();
        self.axis_x = Point::new(1., 0.);
        self.axis_y = Point::new(0., 1.);
        self.path = commands;
    }
    pub fn validate(&self) -> Result<(), String> {
        self.style.validate()?;
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
        if [self.origin, self.axis_x, self.axis_y]
            .into_iter()
            .chain(self.path.iter().flat_map(PathCommand::points))
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
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
