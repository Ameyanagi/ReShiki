//! InChI strings and checked molecular input preparation.

pub mod generator;
pub mod helper;
pub mod input;
pub mod key;
pub mod output;

/// Version of the reference implementation used for these operations.
pub const INCHI_VERSION: &str = "1.07.3";
