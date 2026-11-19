//! Arrow bodies (block, hollow, chevron, gradient) and shaft badges. A body or
//! a badge changes how an arrow looks, never what kind of arrow it is.
use super::{Arrow, ArrowStyle, Head, unit};
use crate::{
    document::Point,
    graphics::{GraphicStyle, LinePattern, PathCommand},
    style::DEFAULT,
    typography::TextStyle,
};
use serde::{Deserialize, Serialize};

choices!(Body { Line => "Line", Block => "Block", Hollow => "Hollow", Chevron => "Chevron", Gradient => "Gradient" });
impl Body {
    pub fn is_line(&self) -> bool {
        *self == Self::Line
    }
}

choices!(BadgeIcon { Light => "Light (hν)", Electrode => "Electrode (e⁻)", Heat => "Heat (Δ)", Metal => "Metal", Enzyme => "Enzyme" });

/// A small icon disc on the shaft. It moves with the arrow and names a
/// condition: light, an electrode, heat, a metal or an enzyme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Badge {
    pub icon: BadgeIcon,
    /// The metal's symbol, such as Pd; other icons ignore it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// Longest metal symbol a badge holds.
pub const MAX_BADGE_TEXT: usize = 12;
const BADGE_RADIUS_PT: f32 = 5.5;
const BADGE_TEXT_PT: f32 = 7.;

impl Badge {
    pub fn new(icon: BadgeIcon) -> Self {
        Self {
            icon,
            text: if icon == BadgeIcon::Metal {
                "Pd".into()
            } else {
                String::new()
            },
        }
    }
    /// The condition the badge stands for, as it would be written on the arrow.
    pub fn condition(&self) -> String {
        match self.icon {
            BadgeIcon::Light => "hν".into(),
            BadgeIcon::Electrode => "e⁻".into(),
            BadgeIcon::Heat => "Δ".into(),
            BadgeIcon::Metal => self.text.clone(),
            BadgeIcon::Enzyme => "enzyme".into(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.icon == BadgeIcon::Metal
            && (self.text.trim().is_empty()
                || self.text.chars().count() > MAX_BADGE_TEXT
                || self.text.contains('\n'))
        {
            return Err("Type a metal symbol, for example Pd".into());
        }
        Ok(())
    }
    /// Text drawn inside the disc, if the icon is lettered.
    fn letters(&self) -> Option<&str> {
        match self.icon {
            BadgeIcon::Electrode => Some("e"),
            BadgeIcon::Metal => Some(self.text.as_str()),
            _ => None,
        }
    }
}

/// Where a badge sits: its center, radius and the shaft direction there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BadgeFrame {
    pub center: Point,
    pub radius: f32,
    /// Shaft parameters where a line body leaves and re-enters the disc.
    pub gap: Option<(f32, f32)>,
}

fn text_style(s: &ArrowStyle) -> TextStyle {
    TextStyle {
        size_pt: BADGE_TEXT_PT,
        color: s.color,
        ..TextStyle::default()
    }
}

impl ArrowStyle {
    /// Change the body, switching between line and block sizes when the body
    /// family changes so the new look is usable at once.
    pub fn set_body(&mut self, body: Body) {
        if self.body.is_line() != body.is_line() {
            let (gap, length, width, notch) = if body.is_line() {
                let line = Self::DEFAULT;
                (
                    line.gap_pt,
                    line.head_length_pt,
                    line.head_width_pt,
                    line.head_notch,
                )
            } else {
                (5., 8., 6., 0.)
            };
            self.gap_pt = gap;
            self.head_length_pt = length;
            self.head_width_pt = width;
            self.head_notch = notch;
            if self.head == Head::None && self.tail == Head::None {
                self.head = Head::Full;
            }
        }
        self.body = body;
    }
}

impl Arrow {
    /// The body this arrow draws: double-shafted and elbow arrows keep lines.
    pub fn drawn_body(&self) -> super::Body {
        let body = self.appearance().body;
        if matches!(self.kind.as_str(), "equilibrium" | "retro" | "bent") {
            Body::Line
        } else {
            body
        }
    }
    fn shaft_length(&self) -> f32 {
        if self.nodes.is_empty() {
            self.start.distance(self.end)
        } else {
            self.node_span()
        }
    }
    /// The closed outline of a block, hollow or gradient body, or None when
    /// the arrow is too short for its heads.
    pub(super) fn block_outline(&self, s: &ArrowStyle) -> Option<Vec<PathCommand>> {
        use PathCommand::{Close, Line};
        let length = self.shaft_length();
        let w = DEFAULT.world(s.gap_pt) * 0.5;
        let h = DEFAULT.world(s.head_width_pt).max(w);
        let head = DEFAULT.world(s.head_length_pt).min(length * 0.45);
        let neck = head * (1. - s.head_notch);
        let u = unit(self.start, self.end);
        let n = Point::new(-u.y, u.x);
        let t1 = if s.head == Head::None {
            1.
        } else {
            self.cubic_radial_inset(1., 0., neck, 0.)
        };
        let t0 = if s.tail == Head::None {
            0.
        } else {
            self.cubic_radial_inset(0., 1., neck, 0.)
        };
        if t1 <= t0 + 1e-4 {
            return None;
        }
        // Arriving at the neck on the frame's right side, trace the cap.
        let cap = |neck_t: f32, tip_t: f32, kind: Head, commands: &mut Vec<PathCommand>| {
            let tip = self.point(tip_t);
            let d = if kind == Head::None {
                self.shaft_tangent(tip_t, tip_t < 0.5)
            } else {
                unit(self.point(neck_t), tip)
            };
            let right = Point::new(-d.y, d.x);
            let side = |p: Point, k: f32| p.offset(right.x * k, right.y * k);
            if kind == Head::None {
                commands.push(Line(side(tip, -w)));
                return;
            }
            let base = tip.offset(-d.x * head, -d.y * head);
            let (right_barb, left_barb) = (
                matches!(kind, Head::Full | Head::Right),
                matches!(kind, Head::Full | Head::Left),
            );
            if right_barb {
                commands.push(Line(side(base, h)));
            }
            commands.push(Line(match (right_barb, left_barb) {
                (true, false) => side(tip, -w),
                (false, true) => side(tip, w),
                _ => tip,
            }));
            if left_barb {
                commands.push(Line(side(base, -h)));
            }
            let neck = self.point(neck_t);
            commands.push(Line(side(neck, -w)));
        };
        let mut commands = self.shaft_body(n, t0, t1, w);
        cap(t1, 1., s.head, &mut commands);
        commands.extend(self.shaft_body(n, t1, t0, -w).into_iter().skip(1));
        cap(t0, 0., s.tail, &mut commands);
        commands.push(Close);
        Some(commands)
    }
    /// Filled chevrons along the shaft, pointing toward the head.
    pub(super) fn chevrons(&self, s: &ArrowStyle) -> Vec<Vec<PathCommand>> {
        use PathCommand::{Close, Line, Move};
        let length = self.shaft_length();
        let h = DEFAULT.world(s.head_width_pt);
        let thick = DEFAULT.world(s.gap_pt).min(length * 0.5);
        let depth = DEFAULT.world(s.head_length_pt).min(length * 0.5);
        let spacing = thick * 0.4;
        let span = thick + depth;
        if !length.is_finite() || length < span || span <= 0. {
            return vec![];
        }
        let count = ((length - span) / (thick + spacing)).floor().min(63.) as usize + 1;
        let pitch = if count > 1 {
            (length - span) / (count - 1) as f32
        } else {
            0.
        };
        // A double-headed arrow's chevrons point out from its middle.
        let both = s.head != Head::None && s.tail != Head::None;
        (0..count)
            .map(|i| {
                let middle = (i as f32 * pitch + span * 0.5) / length;
                let backward = s.head == Head::None && s.tail != Head::None || both && middle < 0.5;
                let t = self.length_parameter(middle);
                let mut d = self.shaft_tangent(t, false);
                if backward {
                    d = Point::new(-d.x, -d.y);
                }
                let c = self.point(t);
                let at = |x: f32, y: f32| {
                    let x = x - span * 0.5;
                    c.offset(d.x * x - d.y * y, d.y * x + d.x * y)
                };
                vec![
                    Move(at(0., -h)),
                    Line(at(thick, -h)),
                    Line(at(span, 0.)),
                    Line(at(thick, h)),
                    Line(at(0., h)),
                    Line(at(depth, 0.)),
                    Close,
                ]
            })
            .collect()
    }
    pub(crate) fn badge_frame(&self) -> Option<BadgeFrame> {
        let badge = self.badge.as_ref()?;
        let s = self.appearance();
        let letters = badge.letters().map_or(0., |text| {
            crate::typography::layout(text, &format(&s)).width
        });
        let radius = DEFAULT
            .world(BADGE_RADIUS_PT)
            .max(letters * 0.5 + DEFAULT.world(1.5));
        let t = self.length_parameter(0.5);
        let point = self.point(t);
        if self.drawn_body().is_line() {
            let gap = (
                self.cubic_radial_inset(t, 0., radius, 0.),
                self.cubic_radial_inset(t, 1., radius, 0.),
            );
            return Some(BadgeFrame {
                center: point,
                radius,
                gap: Some(gap),
            });
        }
        // Beside a broad body, on its left, clear of its widest part.
        let d = self.shaft_tangent(t, false);
        let clear =
            DEFAULT.world(s.gap_pt.max(s.head_width_pt * 2.)) * 0.5 + radius + DEFAULT.world(1.5);
        Some(BadgeFrame {
            center: point.offset(d.y * clear, -d.x * clear),
            radius,
            gap: None,
        })
    }
    /// The disc and icon strokes of the badge, in the arrow's color.
    pub(super) fn badge_paths(&self) -> Vec<Vec<PathCommand>> {
        use PathCommand::{Close, Cubic, Line, Move};
        let (Some(badge), Some(frame)) = (self.badge.as_ref(), self.badge_frame()) else {
            return vec![];
        };
        let c = frame.center;
        let r = frame.radius;
        let icon = DEFAULT.world(BADGE_RADIUS_PT);
        let at = |x: f32, y: f32| c.offset(x * icon, y * icon);
        let circle = |cx: f32, cy: f32, radius: f32| {
            let k = 0.552_284_8 * radius;
            let p = |x: f32, y: f32| c.offset(cx + x, cy + y);
            vec![
                Move(p(radius, 0.)),
                Cubic(p(radius, k), p(k, radius), p(0., radius)),
                Cubic(p(-k, radius), p(-radius, k), p(-radius, 0.)),
                Cubic(p(-radius, -k), p(-k, -radius), p(0., -radius)),
                Cubic(p(k, -radius), p(radius, -k), p(radius, 0.)),
                Close,
            ]
        };
        let mut paths = vec![circle(0., 0., r)];
        match badge.icon {
            BadgeIcon::Light => {
                paths.push(circle(0., -0.2 * icon, 0.36 * icon));
                paths.push(vec![
                    Move(at(-0.16, 0.3)),
                    Line(at(0.16, 0.3)),
                    Move(at(-0.11, 0.48)),
                    Line(at(0.11, 0.48)),
                ]);
                for angle in [-90_f32, -30., -150.] {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    paths.push(vec![
                        Move(at(cos * 0.5, -0.2 + sin * 0.5)),
                        Line(at(cos * 0.66, -0.2 + sin * 0.66)),
                    ]);
                }
            }
            BadgeIcon::Electrode => {
                // The superscript minus beside the lettered e.
                paths.push(vec![Move(at(0.2, -0.38)), Line(at(0.5, -0.38))]);
            }
            BadgeIcon::Heat => paths.push(vec![
                Move(at(0., -0.5)),
                Line(at(0.48, 0.36)),
                Line(at(-0.48, 0.36)),
                Close,
            ]),
            BadgeIcon::Enzyme => {
                // An enzyme with its open active site, holding a substrate.
                let radius = 0.5 * icon;
                let k = 0.552_284_8 * radius;
                let p = |x: f32, y: f32| c.offset(x - 0.08 * icon, y);
                let (sin, cos) = 35_f32.to_radians().sin_cos();
                paths.push(vec![
                    Move(p(0., 0.)),
                    Line(p(radius * cos, -radius * sin)),
                    Cubic(
                        p(radius * cos - k * 0.6, -radius - k * 0.2),
                        p(k, -radius),
                        p(0., -radius),
                    ),
                    Cubic(p(-k, -radius), p(-radius, -k), p(-radius, 0.)),
                    Cubic(p(-radius, k), p(-k, radius), p(0., radius)),
                    Cubic(
                        p(k, radius),
                        p(radius * cos - k * 0.6, radius + k * 0.2),
                        p(radius * cos, radius * sin),
                    ),
                    Close,
                ]);
                paths.push(circle(0.42 * icon, 0., 0.13 * icon));
            }
            BadgeIcon::Metal => {}
        }
        paths
    }
    /// The lettered part of a badge, laid out like a caption at its origin.
    pub fn badge_text(&self) -> Option<(Point, String, crate::typography::TextFormat)> {
        let letters = self.badge.as_ref()?.letters()?.to_owned();
        let frame = self.badge_frame()?;
        let format = format(&self.appearance());
        let layout = crate::typography::layout(&letters, &format);
        let mut origin = frame
            .center
            .offset(-layout.width * 0.5, -layout.height * 0.5);
        if self
            .badge
            .as_ref()
            .is_some_and(|b| b.icon == BadgeIcon::Electrode)
        {
            origin = origin.offset(-DEFAULT.world(BADGE_RADIUS_PT) * 0.12, 0.);
        }
        Some((origin, letters, format))
    }
    /// The shaft from `from` to `to`, leaving a gap where a badge sits on it.
    pub(super) fn gapped_body(
        &self,
        n: Point,
        from: f32,
        to: f32,
        offset: f32,
    ) -> Vec<Vec<PathCommand>> {
        let Some((a, b)) = self
            .badge_frame()
            .and_then(|frame| frame.gap)
            .filter(|(a, b)| a < b)
        else {
            return vec![self.shaft_body(n, from, to, offset)];
        };
        let pieces = if from <= to {
            [(from, a.min(to)), (b.max(from), to)]
        } else {
            [(from, b.max(to)), (a.min(from), to)]
        };
        pieces
            .into_iter()
            .filter(|(x, y)| (x - y).abs() > 1e-4 && (x < y) == (from < to))
            .map(|(x, y)| self.shaft_body(n, x, y, offset))
            .collect()
    }
    /// Badge strokes take the arrow's color and width, never a pattern.
    pub(super) fn badge_style(s: &ArrowStyle) -> GraphicStyle {
        GraphicStyle {
            stroke: s.color,
            fill: None,
            width_pt: s.width_pt.clamp(0.5, 1.2),
            pattern: LinePattern::Solid,
        }
    }
}

fn format(s: &ArrowStyle) -> crate::typography::TextFormat {
    crate::typography::TextFormat {
        style: text_style(s),
        ..Default::default()
    }
}
