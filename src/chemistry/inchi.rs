//! InChI strings and checked molecular input preparation.

pub mod generator;
pub mod helper;
pub mod input;
pub mod kernel;
pub mod key;
pub mod output;
pub mod wire;

/// Version of the reference implementation used for these operations.
pub const INCHI_VERSION: &str = kernel::VERSION;
