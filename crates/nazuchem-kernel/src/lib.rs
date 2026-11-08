// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: ReShiki kernel-root extraction; inherited RDKit attribution is retained pending per-type origin confirmation.
//   ReShiki, crates/chemistry/src/lib.rs @ be63a799aa03a83dc6ec74bb294f4d42b89298d3.
//   RDKit 2026.03.6, Code/GraphMol/MolProps.cpp @ 0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985.
//   RDKit 2026.03.6, Code/GraphMol/Atom.cpp @ 0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985.
// Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
// Unresolved: Inherited BSD attribution is stale/overbroad for the reduced root; author should confirm intended per-type origin before replacing it with an original-code header.

//! Molecular preparation, drawing reconstruction and properties in safe Rust.
//!
//! Migrated operations use local sanitization and stereo perception. Remaining
//! imports, full CIP labels and identifiers still use the backend. Never use cached
//! drawing labels: they may predate the latest edit.
//! Mass/formula semantics adapted from RDKit MolProps.cpp and Atom::getMass.
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
// Copied from crates/chemistry/src/lib.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: retain required modules, atomic exports, AtomFacts and Element only; add local budget module and narrow unused-field allowance
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
pub mod aromaticity;
mod atomic_data;
pub mod budget;
pub mod cx;
pub mod electronic;
pub mod graph;
pub mod hydrogens;
pub mod kekulize;
mod native_order;
pub mod normalize;
pub mod ranking;
pub mod rings;
pub mod sanitize;
pub mod smiles;
pub mod stereo;
pub use atomic_data::{ELEMENTS, ISOTOPES, RDKIT_VERSION};
use serde::Deserialize;
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomFacts {
    pub atomic_number: u8,
    pub isotope: u16,
    pub charge: i8,
    /// Explicit attached + implicit H, excluding H represented by graph atoms.
    pub hydrogens: u8,
    pub radical_electrons: u8,
}
pub struct Element {
    pub symbol: &'static str,
    average: f64,
    #[allow(dead_code, reason = "Preserve the copied atomic-data record layout")]
    exact: f64,
    pub common_isotope: u16,
    outer_electrons: i32,
    pub valences: &'static [i32],
}
