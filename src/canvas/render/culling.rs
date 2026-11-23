//! Conservative viewport tests for rendered picture and text ink.
use super::*;

/// Test the transformed corners of already padded ink bounds.
fn visible(rect: Rectangle, turn: Option<(Point, f32)>, bounds: Rectangle) -> bool {
    if !cull() {
        return true;
    }
    let corners = [
        Point::new(rect.x, rect.y),
        Point::new(rect.x + rect.width, rect.y),
        Point::new(rect.x, rect.y + rect.height),
        Point::new(rect.x + rect.width, rect.y + rect.height),
    ];
    let (mut lo, mut hi) = (
        Point::new(f32::INFINITY, f32::INFINITY),
        Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    );
    for mut p in corners {
        if let Some((pivot, degrees)) = turn {
            let (sin, cos) = degrees.to_radians().sin_cos();
            let delta = p - pivot;
            p = Point::new(
                pivot.x + delta.x * cos - delta.y * sin,
                pivot.y + delta.x * sin + delta.y * cos,
            );
        }
        // Bad coordinates must not turn an optimization into missing artwork.
        if !p.x.is_finite() || !p.y.is_finite() {
            return true;
        }
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    hi.x >= 0. && hi.y >= 0. && lo.x <= bounds.width && lo.y <= bounds.height
}

pub(super) fn picture_visible(rect: Rectangle, radians: f32, bounds: Rectangle) -> bool {
    let padded = Rectangle {
        x: rect.x - 1.,
        y: rect.y - 1.,
        width: rect.width + 2.,
        height: rect.height + 2.,
    };
    visible(padded, Some((rect.center(), radians.to_degrees())), bounds)
}

pub(super) fn text_visible(
    ink: Option<Rectangle>,
    at: Point,
    scale: f32,
    pad: f32,
    turn: Option<(Point, f32)>,
    underline: Option<Rectangle>,
    bounds: Rectangle,
) -> bool {
    if !cull() {
        return true;
    }
    let ink = ink.map(|rect| Rectangle {
        x: rect.x * scale + at.x - pad,
        y: rect.y * scale + at.y - pad,
        width: rect.width * scale + 2. * pad,
        height: rect.height * scale + 2. * pad,
    });
    ink.into_iter()
        .chain(underline)
        .any(|rect| visible(rect, turn, bounds))
}
