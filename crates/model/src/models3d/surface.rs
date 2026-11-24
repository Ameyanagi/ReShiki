//! Orbital isosurfaces drawn as vector shapes in a 3D model (prototype; see
//! docs/plans/xtb-surfaces.md).
pub mod draw;
pub mod mesh;
pub mod visible;

pub use draw::{demo_on_this_thread, enable_demo};

#[cfg(test)]
mod tests;
