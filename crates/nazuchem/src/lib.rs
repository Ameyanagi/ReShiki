// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs | Phase A step 5 (local extraction)
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! Chemical names and structures through today's string-based naming API.
//!
//! The name-to-structure engine is a Rust port of OPSIN by Daniel Lowe under
//! MIT. The structure-to-name engine began as a port of openclatura 0.4.3,
//! extended in ReShiki. Complete applicable terms and attribution travel in
//! this crate's `licenses/` directory and `NOTICE`.
//!
//! Both engines are enabled by default. The `n2s` and `s2n` features expose the
//! corresponding engine modules and convenience functions independently.
//!
//! ```rust
//! # #[cfg(feature = "n2s")]
//! # {
//! // Equivalent to CCO; atom order follows the parser.
//! assert_eq!(nazuchem::name_to_smiles("ethanol")?, "C(C)O");
//! # }
//! # Ok::<(), nazuchem::Error>(())
//! ```
//!
//! ```rust
//! # #[cfg(feature = "s2n")]
//! # {
//! let name = nazuchem::smiles_to_name("CCO")?;
//! assert_eq!(name.text, "ethanol");
//! # }
//! # Ok::<(), nazuchem::Error>(())
//! ```
//!
//! Convenience functions use [`Budget::default()`], which is unbounded. Hosts
//! can supply explicit budgets through `n2s::Parser::with_budget` or
//! `s2n::name_smiles`, when the corresponding feature is enabled. The library
//! performs no I/O, creates no threads, and has no process-wide mutable naming
//! state. The parser shares immutable embedded resources; fusion uses bounded
//! thread-local caches. Worst-case time on adversarial input is unbounded;
//! hosts needing hard limits should supervise requests externally.
#![forbid(unsafe_code)]

pub use nazuchem_ir::*;
pub use nazuchem_kernel::budget::Budget;

/// Name-to-structure engine: a Rust port of OPSIN by Daniel Lowe (MIT).
#[cfg(feature = "n2s")]
pub mod n2s {
    pub use nazuchem_n2s::*;
}

/// Structure-to-name engine begun as an openclatura 0.4.3 port, extended in ReShiki.
#[cfg(feature = "s2n")]
pub mod s2n {
    pub use nazuchem_s2n::*;
}

/// A parser, serializer, or naming-engine error, retaining the engine's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Interpret a name with strict OPSIN options and return semantic CXSMILES.
///
/// Uses an unbounded default budget. Parser warnings and failures are returned
/// as errors with the parser's message. Semantic CXSMILES preserves supported
/// labels, enhanced stereochemistry and polymers; locants are excluded. Axial
/// configurations require the lower-level API that returns axes separately.
#[cfg(feature = "n2s")]
pub fn name_to_smiles(name: &str) -> Result<String, Error> {
    let parser =
        n2s::Parser::with_budget(Budget::default()).map_err(|error| Error(error.to_string()))?;
    let result = parser.parse(name, &n2s::ParseOptions::strict());
    if result.status != n2s::Status::Success || !result.warnings.is_empty() {
        return Err(Error(result.message));
    }
    result
        .structure
        .ok_or(Error(result.message))?
        .semantic_cxsmiles()
        .map_err(|error| Error(error.to_string()))
}

/// Generate a name using IUPAC isotope notation and an unbounded default budget.
///
/// The returned [`s2n::Name`] includes the naming engine's preference status,
/// alternatives, omitted stereochemistry and optional recovered neutral target.
#[cfg(feature = "s2n")]
pub fn smiles_to_name(smiles: &str) -> Result<s2n::Name, Error> {
    s2n::name_smiles_with_style(smiles, IsotopeStyle::Iupac, &Budget::default()).map_err(Error)
}
