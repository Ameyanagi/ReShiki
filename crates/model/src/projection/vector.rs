//! f32 3D vector helpers over retained XYZ (position x/y plus depth).
use crate::document::Atom;

// rear_opacity/visibility.rs keeps its own f64 set on purpose (f64 precision, 1e-9 unit
// threshold).
pub(crate) type V = [f32; 3];
pub(crate) fn xyz(a: &Atom) -> V {
    [a.position.x, a.position.y, a.depth]
}
pub(crate) fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn dot(a: V, b: V) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub(crate) fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn unit(a: V) -> Option<V> {
    let length = dot(a, a).sqrt();
    (length.is_finite() && length > 0.001).then(|| a.map(|v| v / length))
}

#[cfg(test)]
mod tests;
