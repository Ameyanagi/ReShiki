//! Resolve alpha-affected ink once: fill and stroke must not blend twice.
use super::{PathCommand, Point, Primitive, flatten};
use crate::graphics::{GraphicStyle, LinePattern};
use resvg::tiny_skia::{self, PathBuilder, PathSegment};

pub(super) fn single_paint(part: Primitive) -> Primitive {
    let Primitive::Path {
        commands,
        mut style,
        filled,
    } = part
    else {
        return part;
    };
    if style.width_pt <= 0. || filled && style.fill != Some(style.stroke) {
        return Primitive::Path {
            commands,
            style,
            filled,
        };
    }
    let Some(outline) = stroke(&commands, &style) else {
        return Primitive::Path {
            commands,
            style,
            filled,
        };
    };
    // Nonzero winding combines same-color stroke and fill into one silhouette.
    // Align the stroke's outer contour with the fill's outer contour so the
    // interior stroke edge subtracts only its own interior, never the fill.
    let mut contours = flatten(&outline);
    if filled {
        let fill = flatten(&commands);
        if outer_sign(&fill) * outer_sign(&contours) < 0. {
            for contour in &mut contours {
                contour.reverse();
            }
        }
        contours.extend(fill);
    }
    let mut commands = Vec::new();
    for contour in contours {
        if let Some(first) = contour.first().filter(|_| contour.len() >= 3) {
            commands.push(PathCommand::Move(*first));
            commands.extend(contour.iter().skip(1).copied().map(PathCommand::Line));
            commands.push(PathCommand::Close);
        }
    }
    style.fill = Some(style.stroke);
    style.width_pt = 0.;
    style.pattern = LinePattern::Solid;
    Primitive::Path {
        commands,
        style,
        filled: true,
    }
}
fn area(points: &[Point]) -> f64 {
    let Some(origin) = points.first() else {
        return 0.;
    };
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| {
            (f64::from(a.x) - f64::from(origin.x)) * (f64::from(b.y) - f64::from(origin.y))
                - (f64::from(a.y) - f64::from(origin.y)) * (f64::from(b.x) - f64::from(origin.x))
        })
        .sum()
}
fn outer_sign(contours: &[Vec<Point>]) -> f64 {
    contours
        .iter()
        .map(|p| area(p))
        .max_by(|a, b| a.abs().total_cmp(&b.abs()))
        .unwrap_or(0.)
        .signum()
}
fn stroke(commands: &[PathCommand], style: &GraphicStyle) -> Option<Vec<PathCommand>> {
    let mut builder = PathBuilder::new();
    for command in commands {
        match *command {
            PathCommand::Move(p) => builder.move_to(p.x, p.y),
            PathCommand::Line(p) => builder.line_to(p.x, p.y),
            PathCommand::Cubic(a, b, z) => builder.cubic_to(a.x, a.y, b.x, b.y, z.x, z.y),
            PathCommand::Close => builder.close(),
        }
    }
    let path = builder.finish()?;
    let dashes = style.dashes();
    let stroke = tiny_skia::Stroke {
        width: style.width(),
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        dash: if dashes.is_empty() {
            None
        } else {
            tiny_skia::StrokeDash::new(dashes, 0.)
        },
        ..Default::default()
    };
    let outline = path.stroke(&stroke, 32.)?;
    let mut commands = Vec::new();
    let mut cursor = Point::default();
    let point = |p: tiny_skia::Point| Point::new(p.x, p.y);
    for segment in outline.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                cursor = point(p);
                commands.push(PathCommand::Move(cursor));
            }
            PathSegment::LineTo(p) => {
                cursor = point(p);
                commands.push(PathCommand::Line(cursor));
            }
            PathSegment::QuadTo(a, z) => {
                let a = point(a);
                let z = point(z);
                commands.push(PathCommand::Cubic(
                    super::mix(cursor, a, 2. / 3.),
                    super::mix(z, a, 2. / 3.),
                    z,
                ));
                cursor = z;
            }
            PathSegment::CubicTo(a, b, z) => {
                cursor = point(z);
                commands.push(PathCommand::Cubic(point(a), point(b), cursor));
            }
            PathSegment::Close => commands.push(PathCommand::Close),
        }
    }
    Some(commands)
}
