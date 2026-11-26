//! Shared xTB calculations and conformer searches for desktop and Python clients.
#![forbid(unsafe_code)]

pub mod conformers;
mod failure;
#[cfg(not(target_arch = "wasm32"))]
pub mod run;
pub mod scf;

pub use failure::Failure;

/// One SCF the worker ran.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evaluation {
    pub energy: f64,
    pub scf_iterations: usize,
    pub seconds: f64,
}

/// Resolve the Calculate card's Auto thread setting.
pub fn threads(count: u32) -> u32 {
    if count == 0 {
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8) as u32)
    } else {
        count
    }
}

/// Warning shown by the xTB calculation interfaces for a small electronic gap.
pub const XTB_NEAR_DEGENERATE: &str = "Near-degenerate electronic structure: results can differ between computers. Try a higher electronic temperature or more unpaired electrons.";
