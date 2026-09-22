//! Detached, bounded two-dimensional depiction using the pinned native order.

mod arithmetic;
mod compute;

pub use compute::{Error, Options, compute, compute_with_rank_properties};

pub mod attachment;
pub mod collision;
pub mod expansion;
pub mod finalize;
pub mod geometry;
pub mod ranks;
pub mod rings;
pub mod seeds;
pub mod templates;
