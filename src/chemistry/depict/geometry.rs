//! Coordinate arithmetic adapted from RDKit 2026.03.6 DepictUtils.cpp,
//! EmbeddedFrag.cpp and Geometry/Transform2D.cpp. BSD-3-Clause;
//! see licenses/rdkit/NOTICE for the individual copyright notices.
//!
//! Arithmetic preserves source order and the verified contractions of the
//! pinned native ABI. Platform libm behavior is validated separately; see the
//! numeric audit. This remains a library stage, without layout dispatch.

use super::arithmetic;
use std::{collections::BTreeMap, f64::consts::PI};

/// Maximum input/output coordinate count for one geometry operation.
pub const MAX_POINTS: usize = 100_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Native coordinate maps iterate in increasing atom-index order.
pub type Coordinates = BTreeMap<usize, Point>;

/// Native signed extents: the negative fields are the negated minima.
///
/// RDKit initializes each maximum to -1e8 and each minimum to +1e8.
/// These sentinels, including the empty-fragment result, are preserved.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bounds {
    pub positive_x: f64,
    pub negative_x: f64,
    pub positive_y: f64,
    pub negative_y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Depiction geometry exceeds the supported point or atom-index limit")]
    Limit,
    #[error("Depiction geometry requires finite inputs")]
    NonFinite,
    /// Finite input caused overflow/nonfinite arithmetic, or an undefined
    /// polygon (one atom). This is separate from document import validation.
    #[error("Depiction geometry produced a nonfinite intermediate or result")]
    Numeric,
}

impl Point {
    fn input(self) -> Result<Self, Error> {
        if self.x.is_finite() && self.y.is_finite() {
            Ok(self)
        } else {
            Err(Error::NonFinite)
        }
    }

    fn result(self) -> Result<Self, Error> {
        if self.x.is_finite() && self.y.is_finite() {
            Ok(self)
        } else {
            Err(Error::Numeric)
        }
    }

    fn length(self) -> f64 {
        arithmetic::squared_length(self.x, self.y).sqrt()
    }
}

fn finite(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Numeric)
    }
}

fn validate(points: &Coordinates) -> Result<(), Error> {
    if points.len() > MAX_POINTS {
        return Err(Error::Limit);
    }
    for (&id, &point) in points {
        i32::try_from(id).map_err(|_| Error::Limit)?;
        point.input()?;
    }
    Ok(())
}

/// Embed the supplied ordered ring as the reference regular polygon.
///
/// IDs determine output order, not polygon order. Repeated IDs retain the last
/// generated position, matching the native map assignment. An empty ring gives
/// an empty map; a one-atom ring has no finite native polygon and is rejected.
/// Bond length is explicit rather than stored in the native global variable.
pub fn embed_ring(ring: &[usize], bond_length: f64) -> Result<Coordinates, Error> {
    if ring.len() > MAX_POINTS {
        return Err(Error::Limit);
    }
    if !bond_length.is_finite() {
        return Err(Error::NonFinite);
    }
    for &id in ring {
        i32::try_from(id).map_err(|_| Error::Limit)?;
    }
    if ring.is_empty() {
        return Ok(Coordinates::new());
    }
    let count = u32::try_from(ring.len()).map_err(|_| Error::Limit)?;
    let angle = 2.0 * PI / f64::from(count);
    let arm = finite(bond_length / (2.0 * (1.0 - angle.cos())).sqrt())?;
    let mut output = Coordinates::new();
    for (index, &id) in ring.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::Limit)?;
        let phase = f64::from(index) * angle;
        let (sine, cosine) = arithmetic::sin_cos(phase);
        output.insert(
            id,
            Point {
                x: arm * cosine,
                y: arm * sine,
            }
            .result()?,
        );
    }
    Ok(output)
}

/// Native bisector construction, including its angle-greater-than-pi branch.
pub fn bisect_point(
    center: Point,
    angle: f64,
    first: Point,
    second: Point,
) -> Result<Point, Error> {
    center.input()?;
    first.input()?;
    second.input()?;
    if !angle.is_finite() {
        return Err(Error::NonFinite);
    }
    let mut point = Point {
        x: first.x + second.x,
        y: first.y + second.y,
    }
    .result()?;
    point.x *= 0.5;
    point.y *= 0.5;
    if angle > PI {
        point.x -= center.x;
        point.y -= center.y;
        point.x *= -1.0;
        point.y *= -1.0;
        point.x += center.x;
        point.y += center.y;
    }
    point.result()
}

#[derive(Clone, Copy)]
pub(super) struct Transform {
    xx: f64,
    xy: f64,
    tx: f64,
    yx: f64,
    yy: f64,
    ty: f64,
}

impl Transform {
    fn identity() -> Self {
        Self {
            xx: 1.0,
            xy: 0.0,
            tx: 0.0,
            yx: 0.0,
            yy: 1.0,
            ty: 0.0,
        }
    }

    pub(super) fn apply(self, p: Point) -> Result<Point, Error> {
        Point {
            x: arithmetic::dot(self.xx, self.xy, p.x, p.y) + self.tx,
            y: arithmetic::dot(self.yx, self.yy, p.x, p.y) + self.ty,
        }
        .result()
    }

    // Native rotation about a point composes two full 3x3 products. Keeping
    // the zero-initialized accumulation (including zero products) preserves
    // signed zeros and rounding; a simplified translation is not equivalent.
    pub(super) fn rotate(center: Point, angle: f64) -> Result<Self, Error> {
        center.input()?;
        finite(angle)?;
        fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> Result<[[f64; 3]; 3], Error> {
            let mut result = [[0.0; 3]; 3];
            for (row, output) in a.iter().zip(result.iter_mut()) {
                for (column, cell) in output.iter_mut().enumerate() {
                    for (left, right) in row.iter().zip(b.iter()) {
                        *cell = arithmetic::multiply_add(
                            *left,
                            *right.get(column).ok_or(Error::Numeric)?,
                            *cell,
                        );
                    }
                    finite(*cell)?;
                }
            }
            Ok(result)
        }
        let (sine, cosine) = arithmetic::sin_cos(angle);
        let rotation = [[cosine, -sine, 0.0], [sine, cosine, 0.0], [0.0, 0.0, 1.0]];
        let before = [
            [1.0, 0.0, -center.x],
            [0.0, 1.0, -center.y],
            [0.0, 0.0, 1.0],
        ];
        let after = [[1.0, 0.0, center.x], [0.0, 1.0, center.y], [0.0, 0.0, 1.0]];
        let [[xx, xy, tx], [yx, yy, ty], _] = multiply(after, multiply(rotation, before)?)?;
        Ok(Self {
            xx,
            xy,
            tx,
            yx,
            yy,
            ty,
        })
    }

    // Preserve Transform2D::SetTransform's acos/sign/cos/sin operation order.
    pub(super) fn align(ref1: Point, ref2: Point, p1: Point, p2: Point) -> Result<Self, Error> {
        let r = Point {
            x: ref2.x - ref1.x,
            y: ref2.y - ref1.y,
        }
        .result()?;
        let p = Point {
            x: p2.x - p1.x,
            y: p2.y - p1.y,
        }
        .result()?;
        let dot = finite(arithmetic::dot(r.x, r.y, p.x, p.y))?;
        let length = finite(r.length() * p.length())?;
        if length <= 0.0 {
            return Ok(Self::identity());
        }
        let cosine = finite(dot / length)?.clamp(-1.0, 1.0);
        let mut angle = cosine.acos();
        let cross = finite(arithmetic::cross(p.x, p.y, r.x, r.y))?;
        if cross < 0.0 {
            angle *= -1.0;
        }
        let (sine, cosine) = arithmetic::sin_cos(angle);
        let mut transform = Self {
            xx: cosine,
            xy: -sine,
            tx: 0.0,
            yx: sine,
            yy: cosine,
            ty: 0.0,
        };
        let rotated = transform.apply(p1)?;
        transform.tx = finite(ref1.x - rotated.x)?;
        transform.ty = finite(ref1.y - rotated.y)?;
        Ok(transform)
    }
}

/// Reflect using the native two-transform algorithm, without rescaling.
///
/// Coincident axis endpoints produce identity transforms, so the native result
/// reflects about the global x axis. This also applies to underflowed axes.
pub fn reflect_point(point: Point, first: Point, second: Point) -> Result<Point, Error> {
    point.input()?;
    first.input()?;
    second.input()?;
    let mut center = Point {
        x: first.x + second.x,
        y: first.y + second.y,
    }
    .result()?;
    center.x *= 0.5;
    center.y *= 0.5;
    let origin = Point::default();
    let axis = Point { x: 1.0, y: 0.0 };
    let forward = Transform::align(origin, axis, center, first)?;
    let backward = Transform::align(center, first, origin, axis)?;
    let mut result = forward.apply(point)?;
    result.y = -result.y;
    backward.apply(result)
}

/// Center and rotate a fragment with EmbeddedFrag's canonical orientation.
///
/// Zero/one atom is unchanged. Larger fragments are centered before checking
/// the eigenvector threshold, so a degenerate early return remains centered.
/// All reductions follow increasing atom IDs; input coordinates are unchanged.
pub fn canonical_orientation(points: &Coordinates) -> Result<Coordinates, Error> {
    let (mut centered, transform) = canonical_basis(points)?;
    if let Some(transform) = transform {
        for point in centered.values_mut() {
            *point = transform.apply(*point)?;
        }
    }
    Ok(centered)
}

// Expose the existing centered coordinates and optional rotation to the final
// fragment stage, which must also transform EmbeddedAtom normals. A degenerate
// early return keeps those normals unchanged, exactly as the native method.
pub(super) fn canonical_basis(
    points: &Coordinates,
) -> Result<(Coordinates, Option<Transform>), Error> {
    validate(points)?;
    if points.len() <= 1 {
        return Ok((points.clone(), None));
    }
    let count = u32::try_from(points.len()).map_err(|_| Error::Limit)?;
    let mut center = Point::default();
    for point in points.values() {
        center.x += point.x;
        center.y += point.y;
    }
    center.result()?;
    center.x *= 1.0 / f64::from(count);
    center.y *= 1.0 / f64::from(count);
    let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
    let mut result = Coordinates::new();
    for (&id, point) in points {
        let point = Point {
            x: point.x - center.x,
            y: point.y - center.y,
        }
        .result()?;
        xx = arithmetic::multiply_add(point.x, point.x, xx);
        xy = arithmetic::multiply_add(point.x, point.y, xy);
        yy = arithmetic::multiply_add(point.y, point.y, yy);
        result.insert(id, point);
    }
    finite(xx)?;
    finite(xy)?;
    finite(yy)?;
    let delta = finite(arithmetic::multiply_add(xx - yy, xx - yy, 4.0 * xy * xy))?.sqrt();
    let mut first = Point {
        x: 2.0 * xy,
        y: (yy - xx) + delta,
    }
    .result()?;
    if finite(first.length())? <= 1e-4 {
        return Ok((result, None));
    }
    let first_value = finite((xx + yy + delta) / 2.0)?;
    let length = finite(first.length())?;
    first.x /= length;
    first.y /= length;
    let mut second = Point {
        x: 2.0 * xy,
        y: (yy - xx) - delta,
    }
    .result()?;
    let second_value = finite((xx + yy - delta) / 2.0)?;
    if finite(second.length())? > 1e-4 {
        let length = second.length();
        second.x /= length;
        second.y /= length;
        if second_value > first_value {
            std::mem::swap(&mut first, &mut second);
        }
    }
    let transform = Transform {
        xx: first.x,
        xy: first.y,
        tx: 0.0,
        yx: -first.y,
        yy: first.x,
        ty: 0.0,
    };
    Ok((result, Some(transform)))
}

/// Compute the native signed box extents, including its finite sentinels.
pub fn compute_box(points: &Coordinates) -> Result<Bounds, Error> {
    validate(points)?;
    let mut bounds = Bounds {
        positive_x: -1e8,
        negative_x: 1e8,
        positive_y: -1e8,
        negative_y: 1e8,
    };
    for point in points.values() {
        // Comparisons preserve the first signed zero, as std::min/max do.
        if bounds.positive_x < point.x {
            bounds.positive_x = point.x;
        }
        if point.x < bounds.negative_x {
            bounds.negative_x = point.x;
        }
        if bounds.positive_y < point.y {
            bounds.positive_y = point.y;
        }
        if point.y < bounds.negative_y {
            bounds.negative_y = point.y;
        }
    }
    bounds.negative_x *= -1.0;
    bounds.negative_y *= -1.0;
    Ok(bounds)
}
