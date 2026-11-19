//! Lab apparatus: the `.reshiki-piece` format, its validator, the evaluator
//! and the paint plan. Apparatus is always illustration, never chemistry.
pub mod builtin;
pub mod eval;
pub mod format;
pub mod geometry;
pub mod placed;
pub mod plan;
pub mod validate;

#[cfg(test)]
mod placed_tests;
#[cfg(test)]
mod tests;
