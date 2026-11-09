// SPDX-License-Identifier: BSD-3-Clause AND (MIT OR Apache-2.0)
// Origin: adapted
// Source: ReShiki | crates/chemistry/src/lib.rs | be63a799aa03a83dc6ec74bb294f4d42b89298d3
// Source: RDKit | Code/GraphMol/atomic_data.cpp | 0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985
// Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.

//! Molecular preparation and shared atomic records in safe Rust.
//!
//! The Element record stores RDKit-derived element properties supplied by
//! atomic_data.rs; ELEMENTS and ISOTOPES re-export those BSD-3-Clause tables.
//! Module wiring and AtomFacts are local ReShiki declarations. The former
//! mass/formula calculation operations were removed during kernel extraction.
//! See licenses/rdkit/LICENSE and NOTICE for the retained atomic-data attribution.
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
