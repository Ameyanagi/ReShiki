//! Orbital isosurfaces drawn as vector shapes in a 3D model (see
//! docs/plans/xtb-surfaces.md).
pub mod draw;
mod figure;
pub mod fit;
pub mod look;
pub mod mesh;
pub mod orbital;
mod signing;
pub mod snapshot;
pub mod visible;

pub use draw::demo_on_this_thread;
pub use figure::{Figure, OrbitalRef, Panel, Snapshot};
pub use look::{Fill, Look};
pub(crate) use signing::{keep_mo_signed, keep_signed};

/// Export size above which the export dialog warns (owner: 5 MB).
pub const EXPORT_WARNING_BYTES: usize = 5 * 1024 * 1024;
/// PDF bytes per SVG byte, at most, measured on 3D-model pages with and
/// without surfaces (0.5 to 1.4).
const PDF_PER_SVG: f32 = 1.4;

/// What exporting `doc` would take: its SVG bytes (exact, as written), a
/// PDF estimate, and whether either passes [`EXPORT_WARNING_BYTES`].
pub fn export_estimate(doc: &crate::document::Document) -> (usize, usize, bool) {
    let svg = crate::scene::svg(doc).len();
    let pdf = (svg as f32 * PDF_PER_SVG) as usize;
    (svg, pdf, svg.max(pdf) > EXPORT_WARNING_BYTES)
}

#[cfg(test)]
mod tests;
