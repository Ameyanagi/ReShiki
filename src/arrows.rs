//! Editable reaction and electron-flow arrows. Geometry is shared by the editor,
//! hit testing, selection bounds and every figure export.
use crate::{
    document::{Arrow, Point},
    graphics::{GraphicStyle, LinePattern, PathCommand, flattened},
    style::DEFAULT,
};
use serde::{Deserialize, Serialize};

macro_rules! choices {
    ($name:ident { $first:ident => $label:literal $(, $variant:ident => $text:literal)* $(,)? }) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { #[default] $first, $($variant),* }
        impl $name { pub const ALL: &'static [Self] = &[Self::$first, $(Self::$variant),*]; }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(match self { Self::$first => $label, $(Self::$variant => $text),* })
            }
        }
    }
}
choices!(Head { Full => "Full", None => "None", Left => "Half left", Right => "Half right" });
choices!(HeadShape { Solid => "Solid", Hollow => "Hollow", Open => "Angled" });
choices!(NoGo { None => "None", Cross => "Cross", Hash => "Hash" });
choices!(Preset { Forward => "Forward", Equilibrium => "Equilibrium", Resonance => "Resonance", Retro => "Retrosynthesis", Curved => "Curved / electron pair", Bent => "Bent / elbow", Fishhook => "Single electron", Dipole => "Dipole", NoGo => "No reaction" });
impl Preset {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Equilibrium => "equilibrium",
            Self::Resonance => "resonance",
            Self::Retro => "retro",
            Self::Curved => "curved",
            Self::Bent => "bent",
            Self::Fishhook => "fishhook",
            Self::Dipole => "dipole",
            Self::NoGo => "no_go",
        }
    }
    pub fn from_kind(kind: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.kind() == kind)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArrowStyle {
    pub head: Head,
    pub tail: Head,
    pub shape: HeadShape,
    pub color: [u8; 3],
    pub width_pt: f32,
    pub pattern: LinePattern,
    pub head_length_pt: f32,
    /// Half-width of a full arrowhead.
    pub head_width_pt: f32,
    pub equilibrium_ratio: f32,
    pub head_notch: f32,
    pub gap_pt: f32,
    pub no_go: NoGo,
    pub dipole: bool,
}
impl Default for ArrowStyle {
    fn default() -> Self {
        Self {
            width_pt: DEFAULT.line_width_pt,
            ..Self::DEFAULT
        }
    }
}
impl ArrowStyle {
    pub const DEFAULT: Self = Self {
        head: Head::Full,
        tail: Head::None,
        shape: HeadShape::Solid,
        color: [0; 3],
        width_pt: 0.6,
        pattern: LinePattern::Solid,
        head_length_pt: 6.0,
        head_width_pt: 1.5,
        equilibrium_ratio: 1.0,
        head_notch: 0.125,
        gap_pt: 2.0,
        no_go: NoGo::None,
        dipole: false,
    };
}
impl ArrowStyle {
    pub fn preset(preset: Preset) -> Self {
        let mut s = Self::default();
        match preset {
            Preset::Equilibrium => {
                s.head = Head::Left;
                s.tail = Head::Left;
            }
            Preset::Resonance => {
                s.tail = Head::Full;
                s.shape = HeadShape::Open;
            }
            Preset::Retro => {
                s.shape = HeadShape::Open;
                s.head_width_pt = 2.4;
                s.gap_pt = 1.4;
            }
            Preset::Fishhook => {
                s.head = Head::Left;
            }
            Preset::Dipole => {
                s.dipole = true;
            }
            Preset::NoGo => s.no_go = NoGo::Cross,
            _ => {}
        }
        s
    }
    pub fn validate(&self) -> Result<(), String> {
        for (label, n, lo, hi) in [
            ("Arrow line width", self.width_pt, 0.1, 12.),
            ("Arrowhead notch", self.head_notch, 0., 0.9),
            ("Arrowhead length", self.head_length_pt, 0.5, 24.),
            ("Arrowhead half-width", self.head_width_pt, 0.25, 16.),
            ("Arrow separation", self.gap_pt, 0.5, 16.),
            ("Equilibrium ratio", self.equilibrium_ratio, 0.2, 1.),
        ] {
            if !n.is_finite() || !(lo..=hi).contains(&n) {
                return Err(format!("{label} must be {lo}–{hi}"));
            }
        }
        Ok(())
    }
}

pub struct ArrowPath {
    pub commands: Vec<PathCommand>,
    pub style: GraphicStyle,
    pub filled: bool,
}
fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
fn unit(a: Point, b: Point) -> Point {
    let d = a.distance(b).max(0.0001);
    Point::new((b.x - a.x) / d, (b.y - a.y) / d)
}
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let d = a.distance(b).powi(2);
    let t = if d > 0.00001 {
        ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / d
    } else {
        0.
    };
    p.distance(lerp(a, b, t.clamp(0., 1.)))
}
impl Arrow {
    pub fn new(id: u64, start: Point, end: Point, preset: Preset, style: ArrowStyle) -> Self {
        Self {
            id,
            start,
            end,
            kind: preset.kind().into(),
            control: None,
            style: Some(style),
        }
    }
    pub fn appearance(&self) -> ArrowStyle {
        self.style
            .clone()
            .unwrap_or_else(|| ArrowStyle::preset(Preset::from_kind(&self.kind)))
    }
    pub fn control_point(&self) -> Option<Point> {
        self.control.or_else(|| {
            matches!(self.kind.as_str(), "curved" | "fishhook" | "bent").then(|| {
                lerp(self.start, self.end, 0.5).offset(
                    -(self.end.y - self.start.y) * 0.5,
                    (self.end.x - self.start.x) * 0.5,
                )
            })
        })
    }
    pub fn point(&self, t: f32) -> Point {
        match self.control_point() {
            Some(c) if self.kind == "bent" => {
                if t <= 0.5 {
                    lerp(self.start, c, t * 2.)
                } else {
                    lerp(c, self.end, (t - 0.5) * 2.)
                }
            }
            Some(c) => lerp(lerp(self.start, c, t), lerp(c, self.end, t), t),
            None => lerp(self.start, self.end, t),
        }
    }
    fn velocity(&self, t: f32) -> Point {
        if let Some(c) = self.control_point() {
            if self.kind == "bent" {
                let (a, b) = if t < 0.5 {
                    (self.start, c)
                } else {
                    (c, self.end)
                };
                return Point::new(2. * (b.x - a.x), 2. * (b.y - a.y));
            }
            Point::new(
                2. * ((1. - t) * (c.x - self.start.x) + t * (self.end.x - c.x)),
                2. * ((1. - t) * (c.y - self.start.y) + t * (self.end.y - c.y)),
            )
        } else {
            Point::new(self.end.x - self.start.x, self.end.y - self.start.y)
        }
    }
    fn offset_point(&self, t: f32, offset: f32) -> Point {
        let v = self.velocity(t);
        let speed = v.distance(Point::default());
        let direction = if speed > 0.0001 {
            Point::new(v.x / speed, v.y / speed)
        } else {
            unit(self.start, self.end)
        };
        self.point(t)
            .offset(-direction.y * offset, direction.x * offset)
    }
    fn offset_velocity(&self, t: f32, offset: f32) -> Point {
        let v = self.velocity(t);
        let speed = v.distance(Point::default()).max(0.0001);
        let Some(c) = self.control_point() else {
            return v;
        };
        let a = Point::new(
            2. * (self.end.x - 2. * c.x + self.start.x),
            2. * (self.end.y - 2. * c.y + self.start.y),
        );
        let projection = (v.x * a.x + v.y * a.y) / (speed * speed);
        v.offset(
            offset * (-a.y + v.y * projection) / speed,
            offset * (a.x - v.x * projection) / speed,
        )
    }
    pub fn handles(&self) -> [Point; 3] {
        [self.start, self.end, self.point(0.5)]
    }
    pub fn edit_handle(&mut self, index: usize, p: Point) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return;
        }
        let control = self.control_point();
        match index {
            0 => {
                self.start = p;
                self.control = control;
            }
            1 => {
                self.end = p;
                self.control = control;
            }
            2 => {
                let mid = lerp(self.start, self.end, 0.5);
                self.control = Some(if self.kind == "bent" {
                    p
                } else {
                    Point::new(2. * p.x - mid.x, 2. * p.y - mid.y)
                });
            }
            _ => {}
        }
    }
    pub fn map_points(&mut self, mut f: impl FnMut(Point) -> Point) {
        // Materialize the legacy default bend before reflecting or transforming it.
        self.control = self.control_point().map(&mut f);
        self.start = f(self.start);
        self.end = f(self.end);
    }
    pub fn reverse(&mut self) {
        self.control = self.control_point();
        std::mem::swap(&mut self.start, &mut self.end);
    }
    /// Apply an arrow tool, or cycle its direction/half-head on another click.
    /// Returns whether the reaction direction was reversed.
    pub fn apply_tool(&mut self, preset: Preset, style: &ArrowStyle) -> bool {
        if self.kind == preset.kind() && self.appearance() == *style {
            if self.kind != "equilibrium" && matches!(style.head, Head::Left | Head::Right) {
                let mut next = style.clone();
                next.head = if style.head == Head::Left {
                    Head::Right
                } else {
                    Head::Left
                };
                self.style = Some(next);
                return false;
            }
            self.reverse();
            return true;
        }
        let curved = |kind: &str| matches!(kind, "curved" | "fishhook");
        if self.kind != preset.kind() && !(curved(&self.kind) && curved(preset.kind())) {
            self.control = None;
        }
        self.kind = preset.kind().into();
        self.style = Some(style.clone());
        false
    }
    pub fn straighten(&mut self) {
        self.control = Some(lerp(self.start, self.end, 0.5));
    }
    pub fn flip_bend(&mut self) {
        if let Some(c) = self.control_point() {
            let u = unit(self.start, self.end);
            let d = (c.x - self.start.x) * u.x + (c.y - self.start.y) * u.y;
            let projection = self.start.offset(u.x * d, u.y * d);
            self.control = Some(Point::new(2. * projection.x - c.x, 2. * projection.y - c.y));
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !Preset::ALL.iter().any(|p| p.kind() == self.kind) {
            return Err("Unsupported arrow style".into());
        }
        if [Some(self.start), Some(self.end), self.control]
            .into_iter()
            .flatten()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err("Non-finite arrow position".into());
        }
        self.appearance().validate()
    }
    pub fn paths(&self) -> Vec<ArrowPath> {
        use PathCommand::{Close, Cubic, Line, Move};
        let s = self.appearance();
        let mut result = Vec::new();
        let u = unit(self.start, self.end);
        let n = Point::new(-u.y, u.x);
        let gap = DEFAULT.world(s.gap_pt) * 0.5;
        let stroke = GraphicStyle {
            stroke: s.color,
            fill: None,
            width_pt: s.width_pt,
            pattern: s.pattern,
        };
        let mut path = |commands: Vec<PathCommand>, filled: bool, head: bool| {
            let mut style = stroke.clone();
            if head {
                style.pattern = LinePattern::Solid;
            }
            if filled {
                style.fill = Some(s.color);
                style.width_pt = 0.;
            }
            result.push(ArrowPath {
                commands,
                style,
                filled,
            });
        };
        let head = |tip: Point,
                    direction: Point,
                    kind: Head,
                    path: &mut dyn FnMut(Vec<PathCommand>, bool, bool)| {
            if kind == Head::None {
                return;
            }
            let length = DEFAULT
                .world(s.head_length_pt)
                .min(self.start.distance(self.end) * 0.4);
            let width = DEFAULT.world(s.head_width_pt);
            let base = tip.offset(-direction.x * length, -direction.y * length);
            let left = base.offset(direction.y * width, -direction.x * width);
            let right = base.offset(-direction.y * width, direction.x * width);
            let center = tip.offset(
                -direction.x * length * (1. - s.head_notch),
                -direction.y * length * (1. - s.head_notch),
            );
            let mut pts = vec![Move(tip)];
            match kind {
                Head::Full => {
                    pts.extend([Line(left), Line(center), Line(right)]);
                }
                Head::Left => {
                    pts.extend([Line(left), Line(center)]);
                }
                Head::Right => {
                    pts.extend([Line(right), Line(center)]);
                }
                Head::None => {}
            }
            if s.shape == HeadShape::Open {
                pts = match kind {
                    Head::Full => vec![Move(left), Line(tip), Line(right)],
                    Head::Left => vec![Move(left), Line(tip)],
                    _ => vec![Move(right), Line(tip)],
                };
            } else {
                pts.push(Close);
            }
            path(pts, s.shape == HeadShape::Solid, true);
        };
        // Cubic representation exactly reproduces the editable quadratic curve.
        let body = |from: f32, to: f32, offset: f32| {
            let shift = |p: Point| p.offset(n.x * offset, n.y * offset);
            let start = self.point(from);
            let end = self.point(to);
            if let Some(c) = self.control_point() {
                if self.kind == "bent" {
                    let mut commands = vec![Move(shift(start))];
                    if from.min(to) < 0.5 && from.max(to) > 0.5 {
                        commands.push(Line(shift(c)));
                    }
                    commands.push(Line(shift(end)));
                    return commands;
                }
                if offset != 0. {
                    // A curve's parallel follows its local normal. Hermite cubic
                    // segments retain smooth tangents and a constant shaft gap.
                    let steps = 16;
                    let dt = (to - from) / steps as f32;
                    let mut commands = vec![Move(self.offset_point(from, offset))];
                    for i in 0..steps {
                        let t0 = from + i as f32 * dt;
                        let t1 = from + (i + 1) as f32 * dt;
                        let p0 = self.offset_point(t0, offset);
                        let p1 = self.offset_point(t1, offset);
                        let v0 = self.offset_velocity(t0, offset);
                        let v1 = self.offset_velocity(t1, offset);
                        commands.push(Cubic(
                            p0.offset(v0.x * dt / 3., v0.y * dt / 3.),
                            p1.offset(-v1.x * dt / 3., -v1.y * dt / 3.),
                            p1,
                        ));
                    }
                    return commands;
                }
                let tangent = |t: f32| {
                    Point::new(
                        2. * ((1. - t) * (c.x - self.start.x) + t * (self.end.x - c.x)),
                        2. * ((1. - t) * (c.y - self.start.y) + t * (self.end.y - c.y)),
                    )
                };
                let a = tangent(from);
                let b = tangent(to);
                let dt = (to - from) / 3.;
                vec![
                    Move(shift(start)),
                    Cubic(
                        shift(start.offset(a.x * dt, a.y * dt)),
                        shift(end.offset(-b.x * dt, -b.y * dt)),
                        shift(end),
                    ),
                ]
            } else {
                vec![Move(shift(start)), Line(shift(end))]
            }
        };
        let tangent = |t: f32, reverse: bool| {
            let c = self
                .control_point()
                .unwrap_or_else(|| lerp(self.start, self.end, 0.5));
            let d = if self.kind == "bent" {
                if t < 0.5 {
                    unit(self.start, c)
                } else {
                    unit(c, self.end)
                }
            } else {
                unit(lerp(self.start, c, t), lerp(c, self.end, t))
            };
            if reverse { Point::new(-d.x, -d.y) } else { d }
        };
        let half_head = |from: f32, to: f32, offset: f32, kind: Head| {
            if s.shape != HeadShape::Solid || !matches!(kind, Head::Left | Head::Right) {
                return None;
            }
            let sign = (to - from).signum();
            let span = (to - from).abs();
            let direction = tangent(to, sign < 0.);
            let radius = DEFAULT.world(s.width_pt) * 0.5;
            let length = DEFAULT
                .world(s.head_length_pt)
                .min(self.point(from).distance(self.point(to)) * 0.4);
            let width = DEFAULT.world(s.head_width_pt).max(radius * 1.5);
            let speed = self
                .offset_velocity(to, offset)
                .distance(Point::default())
                .max(0.0001);
            let neck = to - sign * (length * (1. - s.head_notch) / speed).min(span * 0.4);
            let side = if kind == Head::Left { 1. } else { -1. };
            let inner = offset + side * sign * radius;
            let outer = offset - side * sign * radius;
            // Continue the shaft's unbarbed edge all the way to the tip. Closing
            // a triangle on the shaft centerline leaves its round cap exposed.
            let tip = self.offset_point(to, inner);
            let wing = tip.offset(
                -direction.x * length + direction.y * side * (width + radius),
                -direction.y * length - direction.x * side * (width + radius),
            );
            let mut commands = vec![
                Move(tip),
                Line(wing),
                Line(self.offset_point(neck, outer)),
                Line(self.offset_point(neck, inner)),
            ];
            if self.kind == "bent" || self.control_point().is_none() {
                commands.push(Line(tip));
            } else {
                commands.extend(body(neck, to, inner).into_iter().skip(1));
            }
            commands.push(Close);
            // Overlap the shortened shaft's cap inside the filled neck.
            let overlap = (radius * 1.25 / speed).min((to - neck).abs() * 0.25);
            Some((commands, neck + sign * overlap))
        };
        let trim = |kind: Head, at: f32, span: f32| {
            if kind == Head::None || s.shape == HeadShape::Open {
                return 0.;
            }
            let length = DEFAULT
                .world(s.head_length_pt)
                .min(self.start.distance(self.end) * 0.4);
            let inset = if s.shape == HeadShape::Hollow && kind == Head::Full {
                length * (1. - s.head_notch)
            } else {
                (DEFAULT.world(s.width_pt) * 0.5 * (s.head_length_pt / s.head_width_pt + 1.))
                    .min(length * 0.7)
            };
            (inset / self.velocity(at).distance(Point::default()).max(0.0001)).min(span * 0.35)
        };
        let mut shaft = |from: f32, to: f32, offset: f32, head_kind: Head, tail_kind: Head| {
            let sign = (to - from).signum();
            let span = (to - from).abs();
            let end_head = half_head(from, to, offset, head_kind);
            let start_head = half_head(to, from, offset, tail_kind);
            let start = start_head
                .as_ref()
                .map(|(_, at)| *at)
                .unwrap_or_else(|| from + sign * trim(tail_kind, from, span));
            let end = end_head
                .as_ref()
                .map(|(_, at)| *at)
                .unwrap_or_else(|| to - sign * trim(head_kind, to, span));
            path(body(start, end, offset), false, false);
            if let Some((commands, _)) = end_head {
                path(commands, true, true);
            } else {
                head(
                    self.offset_point(to, offset),
                    tangent(to, sign < 0.),
                    head_kind,
                    &mut path,
                );
            }
            if let Some((commands, _)) = start_head {
                path(commands, true, true);
            } else {
                head(
                    self.offset_point(from, offset),
                    tangent(from, sign > 0.),
                    tail_kind,
                    &mut path,
                );
            }
        };
        if self.kind == "equilibrium" {
            shaft(0., 1., -gap, s.head, Head::None);
            let inset = (1. - s.equilibrium_ratio) / 2.;
            shaft(1. - inset, inset, gap, s.tail, Head::None);
        } else if self.kind == "retro" {
            // Keep the double shaft inside the angled arrowhead.
            let length = self.start.distance(self.end).max(0.001);
            let head_length = DEFAULT.world(s.head_length_pt).min(length * 0.4);
            let head_width = DEFAULT.world(s.head_width_pt).max(0.001);
            let inset = (head_length * gap / head_width / length).min(0.4);
            path(body(0., 1. - inset, -gap), false, false);
            path(body(0., 1. - inset, gap), false, false);
            head(self.end, tangent(1., false), s.head, &mut path);
            head(self.start, tangent(0., true), s.tail, &mut path);
        } else {
            shaft(0., 1., 0., s.head, s.tail);
        }
        if s.dipole {
            let p = self.point(0.);
            let d = tangent(0., false);
            let w = DEFAULT.world(s.head_width_pt);
            path(
                vec![
                    Move(p.offset(-d.y * w, d.x * w)),
                    Line(p.offset(d.y * w, -d.x * w)),
                ],
                false,
                true,
            );
        }
        if s.no_go != NoGo::None {
            let p = self.point(0.5);
            let d = tangent(0.5, false);
            let w = DEFAULT.world(2.);
            let q = |x: f32, y: f32| p.offset((d.x * x - d.y * y) * w, (d.y * x + d.x * y) * w);
            if s.no_go == NoGo::Hash {
                path(
                    vec![
                        Move(q(-1.5, 1.)),
                        Line(q(0.5, -1.)),
                        Move(q(-0.5, 1.)),
                        Line(q(1.5, -1.)),
                    ],
                    false,
                    true,
                );
            } else {
                path(vec![Move(q(-1., 1.)), Line(q(1., -1.))], false, true);
            }
            if s.no_go == NoGo::Cross {
                path(vec![Move(q(-1., -1.)), Line(q(1., 1.))], false, true);
            }
        }
        result
    }
    pub fn hit(&self, point: Point, radius: f32) -> bool {
        self.paths().iter().any(|p| {
            flattened(&p.commands).iter().any(|points| {
                points
                    .windows(2)
                    .any(|p| matches!(p,[a,b] if segment_distance(point,*a,*b)<=radius))
            })
        })
    }
    pub fn bounds(&self) -> (Point, Point) {
        let mut lo = self.start;
        let mut hi = self.start;
        for path in self.paths() {
            let pad = path.style.width() * 0.5;
            for p in flattened(&path.commands).into_iter().flatten() {
                lo.x = lo.x.min(p.x - pad);
                lo.y = lo.y.min(p.y - pad);
                hi.x = hi.x.max(p.x + pad);
                hi.y = hi.y.max(p.y + pad);
            }
        }
        (lo, hi)
    }
}
