#![forbid(unsafe_code)]

//! Keep the paired trigonometric call in one optimized code-generation unit.
//!
//! RDKit's macOS ARM64 wheel uses `__sincos_stret` and its Linux wheels use
//! `sincos`; separate libc sin/cos calls can return different bits. The workspace
//! compiles this crate at opt-level 1 in development and tests, without
//! optimizing the application as a whole.
//! Native differential tests check the resulting coordinates, including the
//! inputs that distinguish paired and separate calls. No FFI is declared here.

#[inline(never)]
pub fn sin_cos(angle: f64) -> (f64, f64) {
    angle.sin_cos()
}
