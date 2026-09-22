//! Arithmetic contractions observed in the pinned native wheel.
//!
//! The macOS ARM64 compiler contracts these source expressions. The Linux
//! x86_64 wheel keeps their multiply and add separate. This policy is private
//! to depiction: it does not change the application's general floating math.
//! Exact instruction sites and native probes are recorded in the numeric audit.

/// A source multiply followed by an add at a verified contraction site.
pub(super) fn multiply_add(first: f64, second: f64, addend: f64) -> f64 {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        first.mul_add(second, addend)
    } else {
        first * second + addend
    }
}

/// The second product rounds before the first product is accumulated.
pub(super) fn dot(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    multiply_add(ax, bx, ay * by)
}

pub(super) fn squared_length(x: f64, y: f64) -> f64 {
    dot(x, y, x, y)
}

/// Native ARM64 fnmul of the second product, then fmadd of the first.
pub(super) fn multiply_subtract(a: f64, b: f64, c: f64, d: f64) -> f64 {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        a.mul_add(b, -(c * d))
    } else {
        a * b - c * d
    }
}

pub(super) fn cross(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    multiply_subtract(ax, by, ay, bx)
}

/// The pinned Mac ARM64 and Linux native functions request sin and cos
/// together. glibc 2.35 can round its paired call differently from sin alone.
pub(super) fn sin_cos(angle: f64) -> (f64, f64) {
    if cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )) {
        reshiki_depict_math::sin_cos(angle)
    } else {
        (angle.sin(), angle.cos())
    }
}
