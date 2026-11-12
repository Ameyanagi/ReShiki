use super::{Graphic, GraphicStyle, PathCommand, PathNodeType};
use crate::{
    arrows::{ArrowStyle, Head, HeadShape, Preset},
    document::{Arrow, ArrowNode},
    scientific::Part,
    style::DEFAULT,
};
use serde::{Deserialize, Serialize};

/// Arrowheads on an open pen path or Bézier curve, styled like arrow heads.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathHeads {
    pub head: Head,
    pub tail: Head,
    pub shape: HeadShape,
    pub head_length_pt: f32,
    pub head_width_pt: f32,
    pub head_notch: f32,
}
impl Default for PathHeads {
    fn default() -> Self {
        Self {
            head: Head::None,
            tail: Head::None,
            shape: ArrowStyle::DEFAULT.shape,
            head_length_pt: ArrowStyle::DEFAULT.head_length_pt,
            head_width_pt: ArrowStyle::DEFAULT.head_width_pt,
            head_notch: ArrowStyle::DEFAULT.head_notch,
        }
    }
}
impl PathHeads {
    fn style(self, style: &GraphicStyle) -> ArrowStyle {
        ArrowStyle {
            head: self.head,
            tail: self.tail,
            shape: self.shape,
            head_length_pt: self.head_length_pt,
            head_width_pt: self.head_width_pt,
            head_notch: self.head_notch,
            color: style.stroke,
            width_pt: style.width_pt,
            pattern: style.pattern,
            ..ArrowStyle::DEFAULT
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.style(&GraphicStyle::default()).validate()
    }
}
impl Graphic {
    pub(crate) fn path_parts(&self) -> Vec<Part> {
        let mut shaft = Part {
            commands: self.commands(),
            style: self.style.clone(),
            filled: self.filled(),
        };
        let Some(heads) = self.heads else {
            return vec![shaft];
        };
        let [PathCommand::Move(start), segments @ ..] = shaft.commands.as_slice() else {
            return vec![shaft];
        };
        if segments.len() >= 2 {
            let mut previous = *start;
            let mut cubics = Vec::with_capacity(segments.len());
            for command in segments {
                let (a, b, end) = match *command {
                    PathCommand::Line(end) => (
                        previous.lerp(end, 1. / 3.),
                        previous.lerp(end, 2. / 3.),
                        end,
                    ),
                    PathCommand::Cubic(a, b, end) => (a, b, end),
                    _ => return vec![shaft],
                };
                cubics.push([previous, a, b, end]);
                previous = end;
            }
            if let Some((&[start, departure, _, first_end], &[last_start, _, arrival, end])) =
                cubics.first().zip(cubics.last())
            {
                let style = heads.style(&self.style);
                let length = DEFAULT.world(style.head_length_pt);
                if style.head != Head::None && length > last_start.distance(end) * 0.4
                    || style.tail != Head::None && length > start.distance(first_end) * 0.4
                {
                    // Short terminal segments share the whole path's head-size cap.
                    let mut arrow = Arrow::new(self.id, start, end, Preset::Forward, style);
                    arrow.cubic = Some([departure, arrival]);
                    arrow.nodes = cubics
                        .iter()
                        .zip(cubics.iter().skip(1))
                        .map(
                            |(&[_, _, incoming, point], &[_, outgoing, _, _])| ArrowNode {
                                point,
                                controls: [incoming, outgoing],
                                kind: PathNodeType::Corner,
                            },
                        )
                        .collect();
                    let mut paths = arrow.paths().into_iter();
                    let Some(body) = paths.next() else {
                        return vec![shaft];
                    };
                    shaft.commands = body.commands;
                    let mut parts = vec![shaft];
                    parts.extend(paths.map(|p| Part {
                        commands: p.commands,
                        style: p.style,
                        filled: p.filled,
                    }));
                    return parts;
                }
            }
        }
        let mut previous = *start;
        let mut commands = vec![PathCommand::Move(previous)];
        let mut extra = vec![];
        // A single segment needs both trims together, so its heads share one Arrow.
        for (index, command) in segments.iter().enumerate() {
            let (end, cubic) = match *command {
                PathCommand::Line(end) => (end, None),
                PathCommand::Cubic(a, b, end) => (end, Some([a, b])),
                // Closed and compound paths draw no heads.
                _ => return vec![shaft],
            };
            let mut style = heads.style(&self.style);
            if index + 1 != segments.len() {
                style.head = Head::None;
            }
            if index != 0 {
                style.tail = Head::None;
            }
            if style.head == Head::None && style.tail == Head::None {
                commands.push(command.clone());
                previous = end;
                continue;
            }
            let mut arrow = Arrow::new(self.id, previous, end, Preset::Forward, style);
            arrow.cubic = cubic;
            let mut paths = arrow.paths().into_iter();
            let Some(body) = paths.next() else {
                return vec![shaft];
            };
            let [start, segment] = body.commands.as_slice() else {
                return vec![shaft];
            };
            if index == 0 {
                commands.clear();
                commands.push(start.clone());
            }
            commands.push(segment.clone());
            extra.extend(paths.map(|p| Part {
                commands: p.commands,
                style: p.style,
                filled: p.filled,
            }));
            previous = end;
        }
        shaft.commands = commands;
        let mut parts = vec![shaft];
        parts.extend(extra);
        parts
    }
}
