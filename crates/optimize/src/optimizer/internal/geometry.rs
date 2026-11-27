//! Three-dimensional Cartesian geometry used by the internal coordinate charts.

pub(super) type Vector = [f64; 3];

pub(super) fn length(vector: Vector) -> f64 {
    let squared = dot(vector, vector);
    if squared.is_normal() {
        squared.sqrt()
    } else {
        // Preserve the scaled norm for subnormal or overflowing squares.
        vector[0].hypot(vector[1]).hypot(vector[2])
    }
}

pub(super) fn dot(a: Vector, b: Vector) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn subtract(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|k| a[k] - b[k])
}

pub(super) fn point(x: &[f64], atom: usize) -> Option<Vector> {
    if !x.len().is_multiple_of(3) {
        return None;
    }
    let start = atom.checked_mul(3)?;
    let xyz = x.get(start..start.checked_add(3)?)?;
    let result = [xyz[0], xyz[1], xyz[2]];
    result
        .iter()
        .all(|value| value.is_finite())
        .then_some(result)
}
