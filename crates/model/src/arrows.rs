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
    pub color: crate::palette::Color,
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
        color: crate::palette::Color::Ink,
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
            cubic: None,
            style: Some(style),
        }
    }
    pub fn appearance(&self) -> ArrowStyle {
        self.style
            .clone()
            .unwrap_or_else(|| ArrowStyle::preset(Preset::from_kind(&self.kind)))
    }
    pub fn control_point(&self) -> Option<Point> {
        if self.cubic.is_some() {
            return None;
        }
        self.control.or_else(|| {
            matches!(self.kind.as_str(), "curved" | "fishhook" | "bent").then(|| {
                lerp(self.start, self.end, 0.5).offset(
                    -(self.end.y - self.start.y) * 0.5,
                    (self.end.x - self.start.x) * 0.5,
                )
            })
        })
    }
    /// Exact cubic controls, including the degree-elevated legacy quadratic.
    /// Inspecting a legacy arrow never converts its saved geometry.
    pub fn bezier_controls(&self) -> Option<[Point; 2]> {
        if self.kind == "bent" {
            return None;
        }
        self.cubic.or_else(|| {
            self.control_point()
                .map(|c| [lerp(self.start, c, 2. / 3.), lerp(self.end, c, 2. / 3.)])
        })
    }
    pub fn point(&self, t: f32) -> Point {
        if let Some([a, b]) = self.cubic {
            return lerp(
                lerp(lerp(self.start, a, t), lerp(a, b, t), t),
                lerp(lerp(a, b, t), lerp(b, self.end, t), t),
                t,
            );
        }
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
        if let Some([a, b]) = self.cubic {
            let u = 1. - t;
            return Point::new(
                3. * (u * u * (a.x - self.start.x)
                    + 2. * u * t * (b.x - a.x)
                    + t * t * (self.end.x - b.x)),
                3. * (u * u * (a.y - self.start.y)
                    + 2. * u * t * (b.y - a.y)
                    + t * t * (self.end.y - b.y)),
            );
        }
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
        } else if self.cubic.is_some() {
            self.shaft_tangent(t, false)
        } else {
            unit(self.start, self.end)
        };
        self.point(t)
            .offset(-direction.y * offset, direction.x * offset)
    }
    fn offset_velocity(&self, t: f32, offset: f32) -> Point {
        let v = self.velocity(t);
        if self.cubic.is_some() && v.distance(Point::default()) < 0.0001 {
            if offset == 0. {
                return v;
            }
            // Use the limiting offset tangent at a collapsed cubic control.
            // Dividing the normal derivative by a vanishing speed produces
            // enormous control points despite the visible curve being smooth.
            let from = (t - 0.0005).max(0.);
            let to = (t + 0.0005).min(1.);
            let a = self.offset_point(from, offset);
            let b = self.offset_point(to, offset);
            return Point::new((b.x - a.x) / (to - from), (b.y - a.y) / (to - from));
        }
        let speed = v.distance(Point::default()).max(0.0001);
        let a = if let Some([a, b]) = self.cubic {
            Point::new(
                6. * ((1. - t) * (b.x - 2. * a.x + self.start.x)
                    + t * (self.end.x - 2. * b.x + a.x)),
                6. * ((1. - t) * (b.y - 2. * a.y + self.start.y)
                    + t * (self.end.y - 2. * b.y + a.y)),
            )
        } else if let Some(c) = self.control_point() {
            Point::new(
                2. * (self.end.x - 2. * c.x + self.start.x),
                2. * (self.end.y - 2. * c.y + self.start.y),
            )
        } else {
            return v;
        };
        let projection = (v.x * a.x + v.y * a.y) / (speed * speed);
        v.offset(
            offset * (-a.y + v.y * projection) / speed,
            offset * (a.x - v.x * projection) / speed,
        )
    }
    pub fn handles(&self) -> Vec<Point> {
        let mut handles = vec![self.start, self.end, self.point(0.5)];
        if let Some(controls) = self.bezier_controls() {
            handles.extend(controls);
        }
        handles
    }
    /// Select the nearest visible handle when short tangent stems overlap.
    pub fn handle_at(&self, p: Point, radius: f32) -> Option<usize> {
        self.handles()
            .into_iter()
            .enumerate()
            .map(|(index, handle)| (index, handle.distance(p)))
            .filter(|(_, distance)| *distance < radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }
    pub fn edit_handle(&mut self, index: usize, p: Point) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return;
        }
        let control = self.control_point();
        match index {
            0 => {
                if let Some([a, _]) = self.cubic.as_mut() {
                    *a = a.offset(p.x - self.start.x, p.y - self.start.y);
                }
                self.start = p;
                self.control = control;
            }
            1 => {
                if let Some([_, b]) = self.cubic.as_mut() {
                    *b = b.offset(p.x - self.end.x, p.y - self.end.y);
                }
                self.end = p;
                self.control = control;
            }
            2 => {
                if let Some([a, b]) = self.cubic {
                    let middle = self.point(0.5);
                    let dx = (p.x - middle.x) * 4. / 3.;
                    let dy = (p.y - middle.y) * 4. / 3.;
                    self.cubic = Some([a.offset(dx, dy), b.offset(dx, dy)]);
                    return;
                }
                let mid = lerp(self.start, self.end, 0.5);
                self.control = Some(if self.kind == "bent" {
                    p
                } else {
                    Point::new(2. * p.x - mid.x, 2. * p.y - mid.y)
                });
            }
            3 | 4 => {
                if let Some([a, b]) = self.bezier_controls() {
                    self.cubic = Some(if index == 3 { [p, b] } else { [a, p] });
                    self.control = None;
                }
            }
            _ => {}
        }
    }
    pub fn map_points(&mut self, mut f: impl FnMut(Point) -> Point) {
        // Materialize the legacy default bend before reflecting or transforming it.
        self.control = self.control_point().map(&mut f);
        self.cubic = self.cubic.map(|[a, b]| [f(a), f(b)]);
        self.start = f(self.start);
        self.end = f(self.end);
    }
    pub fn reverse(&mut self) {
        self.control = self.control_point();
        self.cubic = self.cubic.map(|[a, b]| [b, a]);
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
            self.cubic = None;
        }
        self.kind = preset.kind().into();
        self.style = Some(style.clone());
        false
    }
    pub fn straighten(&mut self) {
        self.cubic = None;
        self.control = Some(lerp(self.start, self.end, 0.5));
    }
    pub fn flip_bend(&mut self) {
        if let Some([a, b]) = self.cubic {
            let u = unit(self.start, self.end);
            let reflect = |c: Point| {
                let d = (c.x - self.start.x) * u.x + (c.y - self.start.y) * u.y;
                let projection = self.start.offset(u.x * d, u.y * d);
                Point::new(2. * projection.x - c.x, 2. * projection.y - c.y)
            };
            self.cubic = Some([reflect(a), reflect(b)]);
            return;
        }
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
        if self.cubic.is_some() && (self.control.is_some() || self.kind == "bent") {
            return Err(
                "Cubic arrow controls cannot be combined with a quadratic or elbow bend".into(),
            );
        }
        if [Some(self.start), Some(self.end), self.control]
            .into_iter()
            .flatten()
            .chain(self.cubic.into_iter().flatten())
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err("Non-finite arrow position".into());
        }
        self.appearance().validate()
    }
    pub fn paths(&self) -> Vec<ArrowPath> {
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
        self.shafts(&s, n, gap, &mut path);
        if s.dipole {
            self.dipole_mark(&s, &mut path);
        }
        if s.no_go != NoGo::None {
            self.no_go_mark(&s, &mut path);
        }
        result
    }
    fn head_path(
        &self,
        s: &ArrowStyle,
        tip: Point,
        direction: Point,
        kind: Head,
        path: &mut dyn FnMut(Vec<PathCommand>, bool, bool),
    ) {
        use PathCommand::{Close, Line, Move};
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
    }
    // A trimmed cubic exactly reproduces either the cubic or legacy quadratic.
    fn shaft_body(&self, n: Point, from: f32, to: f32, offset: f32) -> Vec<PathCommand> {
        use PathCommand::{Cubic, Line, Move};
        let shift = |p: Point| p.offset(n.x * offset, n.y * offset);
        let start = self.point(from);
        let end = self.point(to);
        if self.control_point().is_some() || self.cubic.is_some() {
            if let Some(c) = self.control_point().filter(|_| self.kind == "bent") {
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
            let a = self.velocity(from);
            let b = self.velocity(to);
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
    }
    fn shaft_tangent(&self, t: f32, reverse: bool) -> Point {
        if let Some([a, b]) = self.cubic {
            let velocity = self.velocity(t);
            let mut direction = unit(Point::default(), velocity);
            if velocity.distance(Point::default()) < 0.0001 {
                direction = if t == 0. {
                    unit(self.start, if b == self.start { self.end } else { b })
                } else if t == 1. {
                    unit(if a == self.end { self.start } else { a }, self.end)
                } else {
                    unit(
                        self.point((t - 0.001).max(0.)),
                        self.point((t + 0.001).min(1.)),
                    )
                };
            }
            return if reverse {
                Point::new(-direction.x, -direction.y)
            } else {
                direction
            };
        }
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
    }
    /// Walk from a cubic tip by a drawing-space distance. Endpoint derivatives
    /// can vanish when a tangent handle is placed directly on its endpoint.
    fn cubic_inset(&self, at: f32, toward: f32, length: f32, limit: f32, offset: f32) -> f32 {
        let end = at + (toward - at) * limit;
        let steps = 32;
        let mut previous = self.offset_point(at, offset);
        let mut distance = 0.;
        for i in 1..=steps {
            let t = at + (end - at) * i as f32 / steps as f32;
            let point = self.offset_point(t, offset);
            let step = previous.distance(point);
            if distance + step >= length && step > 0. {
                let fraction = (length - distance) / step;
                return t - (end - at) / steps as f32 * (1. - fraction);
            }
            distance += step;
            previous = point;
        }
        end
    }
    fn half_head(
        &self,
        s: &ArrowStyle,
        n: Point,
        from: f32,
        to: f32,
        offset: f32,
        kind: Head,
    ) -> Option<(Vec<PathCommand>, f32)> {
        use PathCommand::{Close, Line, Move};
        if s.shape != HeadShape::Solid || !matches!(kind, Head::Left | Head::Right) {
            return None;
        }
        let sign = (to - from).signum();
        let span = (to - from).abs();
        let direction = self.shaft_tangent(to, sign < 0.);
        let radius = DEFAULT.world(s.width_pt) * 0.5;
        let length = DEFAULT
            .world(s.head_length_pt)
            .min(self.point(from).distance(self.point(to)) * 0.4);
        let width = DEFAULT.world(s.head_width_pt).max(radius * 1.5);
        let speed = self
            .offset_velocity(to, offset)
            .distance(Point::default())
            .max(0.0001);
        let neck = if self.cubic.is_some() {
            self.cubic_inset(to, from, length * (1. - s.head_notch), 0.4, offset)
        } else {
            to - sign * (length * (1. - s.head_notch) / speed).min(span * 0.4)
        };
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
        if self.kind == "bent" || self.control_point().is_none() && self.cubic.is_none() {
            commands.push(Line(tip));
        } else {
            commands.extend(self.shaft_body(n, neck, to, inner).into_iter().skip(1));
        }
        commands.push(Close);
        // Overlap the shortened shaft's cap inside the filled neck.
        let shaft_end = if self.cubic.is_some() {
            self.cubic_inset(neck, to, radius * 1.25, 0.25, offset)
        } else {
            let overlap = (radius * 1.25 / speed).min((to - neck).abs() * 0.25);
            neck + sign * overlap
        };
        Some((commands, shaft_end))
    }
    fn head_trim(&self, s: &ArrowStyle, kind: Head, at: f32, toward: f32) -> f32 {
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
        if self.cubic.is_some() {
            (self.cubic_inset(at, toward, inset, 0.35, 0.) - at).abs()
        } else {
            (inset / self.velocity(at).distance(Point::default()).max(0.0001))
                .min((toward - at).abs() * 0.35)
        }
    }
    fn shafts(
        &self,
        s: &ArrowStyle,
        n: Point,
        gap: f32,
        path: &mut impl FnMut(Vec<PathCommand>, bool, bool),
    ) {
        let mut shaft = |from: f32, to: f32, offset: f32, head_kind: Head, tail_kind: Head| {
            let sign = (to - from).signum();
            let end_head = self.half_head(s, n, from, to, offset, head_kind);
            let start_head = self.half_head(s, n, to, from, offset, tail_kind);
            let start = start_head
                .as_ref()
                .map(|(_, at)| *at)
                .unwrap_or_else(|| from + sign * self.head_trim(s, tail_kind, from, to));
            let end = end_head
                .as_ref()
                .map(|(_, at)| *at)
                .unwrap_or_else(|| to - sign * self.head_trim(s, head_kind, to, from));
            path(self.shaft_body(n, start, end, offset), false, false);
            if let Some((commands, _)) = end_head {
                path(commands, true, true);
            } else {
                self.head_path(
                    s,
                    self.offset_point(to, offset),
                    self.shaft_tangent(to, sign < 0.),
                    head_kind,
                    &mut *path,
                );
            }
            if let Some((commands, _)) = start_head {
                path(commands, true, true);
            } else {
                self.head_path(
                    s,
                    self.offset_point(from, offset),
                    self.shaft_tangent(from, sign > 0.),
                    tail_kind,
                    &mut *path,
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
            path(self.shaft_body(n, 0., 1. - inset, -gap), false, false);
            path(self.shaft_body(n, 0., 1. - inset, gap), false, false);
            self.head_path(
                s,
                self.end,
                self.shaft_tangent(1., false),
                s.head,
                &mut *path,
            );
            self.head_path(
                s,
                self.start,
                self.shaft_tangent(0., true),
                s.tail,
                &mut *path,
            );
        } else {
            shaft(0., 1., 0., s.head, s.tail);
        }
    }
    fn dipole_mark(&self, s: &ArrowStyle, path: &mut impl FnMut(Vec<PathCommand>, bool, bool)) {
        use PathCommand::{Line, Move};
        let p = self.point(0.);
        let d = self.shaft_tangent(0., false);
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
    fn no_go_mark(&self, s: &ArrowStyle, path: &mut impl FnMut(Vec<PathCommand>, bool, bool)) {
        use PathCommand::{Line, Move};
        let p = self.point(0.5);
        let d = self.shaft_tangent(0.5, false);
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

#[cfg(test)]
mod curve_tests;
#[cfg(test)]
mod paths_parity_tests;
