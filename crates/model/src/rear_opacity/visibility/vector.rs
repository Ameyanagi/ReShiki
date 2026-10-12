use super::{Document, Point};

pub(super) type V = [f64; 3];

pub(super) fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(super) fn dot(a: V, b: V) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
pub(super) fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn unit(v: V) -> Option<V> {
    let n = dot(v, v).sqrt();
    (n.is_finite() && n > 1e-9).then(|| v.map(|x| x / n))
}
pub(super) fn xy(v: V) -> Point {
    Point::new(v[0] as f32, v[1] as f32)
}
pub(super) fn xyz(doc: &Document, id: u64) -> Option<V> {
    let a = doc.atom(id)?;
    Some([a.position.x.into(), a.position.y.into(), a.depth.into()])
}
pub(super) fn cross2(a: Point, b: Point, c: Point) -> f64 {
    (f64::from(b.x) - f64::from(a.x)) * (f64::from(c.y) - f64::from(a.y))
        - (f64::from(b.y) - f64::from(a.y)) * (f64::from(c.x) - f64::from(a.x))
}
pub(super) fn cross_xy(a: V, b: V, c: V) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

pub(super) fn mix_v(a: V, b: V, t: f64) -> V {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}
