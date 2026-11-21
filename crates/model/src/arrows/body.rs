//! Arrow bodies (block, hollow, chevron, gradient). A body changes how an
//! arrow looks, never what kind of arrow it is.
use super::{Arrow, ArrowStyle, Head, unit};
use crate::{document::Point, graphics::PathCommand, style::DEFAULT};
use serde::{Deserialize, Serialize};

choices!(Body { Line => "Line", Block => "Block", Hollow => "Hollow block", Chevron => "Chevron", Gradient => "Gradient" });
impl Body {
    pub fn is_line(&self) -> bool {
        *self == Self::Line
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
}
