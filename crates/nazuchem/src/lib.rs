// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: mixed
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs | Phase A step 5 (local extraction)
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

pub use nazuchem_ir::{AlternativeName, IsotopeStyle, NameReason, NameStatus};
pub use nazuchem_kernel::budget::Budget;

/// Name-to-structure engine: a Rust port of OPSIN by Daniel Lowe (MIT).
#[cfg(feature = "n2s")]
pub mod n2s {
    pub use nazuchem_n2s::preprocess::preprocess;
    pub use nazuchem_n2s::{
        Axis, InitializationError, PORT_VERSION, ParseOptions, ParseResult, ParseWarning, Parser,
        ParsingError, RESOURCE_FINGERPRINT, SerializationError, SerializedStructure, Status,
        Structure, UPSTREAM_COMMIT, UPSTREAM_VERSION, WarningKind,
    };
}

/// Structure-to-name engine begun as an openclatura 0.4.3 port, extended in ReShiki.
#[cfg(feature = "s2n")]
pub mod s2n {
    pub use nazuchem_s2n::{
        Name, PROFILE, Verification, canonical_smiles, name_smiles, name_smiles_with_style,
        recover_radical_ion, verify_alternative_identity, verify_identity,
    };
}

/// The stage at which a facade operation failed.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The name-to-structure parser could not initialize its resources.
    Init,
    /// The name could not be interpreted as a structure.
    Parse,
    /// Strict name interpretation rejected a parser warning.
    Warning,
    /// The parsed structure could not be serialized without losing information.
    Serialize,
    /// The structure-to-name engine rejected the input or could not name it.
    Naming,
}

/// A parser, serializer, or naming-engine error, retaining the engine's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

// Provenance: impl Error | original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs::Error
impl Error {
    /// Classify the failure without inspecting the engine's message.
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
}

// Provenance: impl <Error as std::fmt::Display> | original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs::<Error as std::fmt::Display>
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

// Provenance: mod n2s_errors | original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs::n2s_errors
#[cfg(feature = "n2s")]
mod n2s_errors {
    use super::{Error, ErrorKind, n2s};

    impl From<n2s::InitializationError> for Error {
        fn from(error: n2s::InitializationError) -> Self {
            Self {
                kind: ErrorKind::Init,
                message: error.to_string(),
            }
        }
    }

    impl From<n2s::ParsingError> for Error {
        fn from(error: n2s::ParsingError) -> Self {
            Self {
                kind: ErrorKind::Parse,
                message: error.to_string(),
            }
        }
    }

    impl From<n2s::SerializationError> for Error {
        fn from(error: n2s::SerializationError) -> Self {
            Self {
                kind: ErrorKind::Serialize,
                message: error.to_string(),
            }
        }
    }
}

/// Interpret a name with strict OPSIN options and return semantic CXSMILES.
///
/// Uses an unbounded default budget. Parser warnings and failures are returned
/// as errors with the parser's message. Semantic CXSMILES preserves supported
/// labels, enhanced stereochemistry and polymers; locants are excluded. Axial
/// configurations require the lower-level API that returns axes separately.
// Provenance: fn name_to_smiles | original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs::name_to_smiles
#[cfg(feature = "n2s")]
pub fn name_to_smiles(name: &str) -> Result<String, Error> {
    let parser = n2s::Parser::with_budget(Budget::default())?;
    let result = parser.parse(name, &n2s::ParseOptions::strict());
    let kind = match result.status {
        n2s::Status::Failure => Some(ErrorKind::Parse),
        n2s::Status::Warning => Some(ErrorKind::Warning),
        n2s::Status::Success if !result.warnings.is_empty() => Some(ErrorKind::Warning),
        n2s::Status::Success => None,
    };
    if let Some(kind) = kind {
        return Err(Error {
            kind,
            message: result.message,
        });
    }
    result
        .structure
        .ok_or(Error {
            kind: ErrorKind::Parse,
            message: result.message,
        })?
        .semantic_cxsmiles()
        .map_err(Error::from)
}

/// Generate a name using IUPAC isotope notation and an unbounded default budget.
///
/// The returned [`s2n::Name`] includes the naming engine's preference status,
/// alternatives, omitted stereochemistry and optional recovered neutral target.
// Provenance: fn smiles_to_name | original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs::smiles_to_name
#[cfg(feature = "s2n")]
pub fn smiles_to_name(smiles: &str) -> Result<s2n::Name, Error> {
    s2n::name_smiles_with_style(smiles, IsotopeStyle::Iupac, &Budget::default()).map_err(
        |message| Error {
            kind: ErrorKind::Naming,
            message,
        },
    )
}

// Provenance: mod tests | original
// Source: Nazuchem/ReShiki | crates/nazuchem/src/lib.rs::tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_and_message_are_available_without_engines() {
        for kind in [
            ErrorKind::Init,
            ErrorKind::Parse,
            ErrorKind::Warning,
            ErrorKind::Serialize,
            ErrorKind::Naming,
        ] {
            let error = Error {
                kind,
                message: "engine message".into(),
            };
            let source: &dyn std::error::Error = &error;
            assert_eq!(error.kind(), kind);
            assert_eq!(source.to_string(), "engine message");
            assert!(source.source().is_none());
        }
    }

    #[cfg(feature = "n2s")]
    #[test]
    fn invalid_name_is_a_parse_error() {
        let result = n2s::Parser::new()
            .unwrap()
            .parse("not a chemical name", &n2s::ParseOptions::strict());
        assert_eq!(result.status, n2s::Status::Failure);
        let error = name_to_smiles(&result.input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Parse);
        assert_eq!(error.to_string(), result.message);
    }

    #[cfg(feature = "n2s")]
    #[test]
    fn parser_warning_is_a_warning_error() {
        let result = n2s::Parser::new()
            .unwrap()
            .parse("(+)-lactic acid", &n2s::ParseOptions::strict());
        assert_eq!(result.status, n2s::Status::Warning);
        let error = name_to_smiles(&result.input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Warning);
        assert_eq!(error.to_string(), result.message);
    }

    #[cfg(feature = "n2s")]
    #[test]
    fn axial_structure_is_a_serialization_error() {
        let input = "(1M)-[1,1'-binaphthalene]-2,2'-diol";
        let result = n2s::Parser::new()
            .unwrap()
            .parse(input, &n2s::ParseOptions::strict());
        assert_eq!(result.status, n2s::Status::Success);
        let serialization_error = result.structure.unwrap().semantic_cxsmiles().unwrap_err();
        let error = name_to_smiles(input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Serialize);
        assert_eq!(error.to_string(), serialization_error.to_string());
    }

    #[cfg(feature = "n2s")]
    #[test]
    fn typed_parsing_error_preserves_its_message() {
        let error = Error::from(n2s::ParsingError("parse message".into()));
        assert_eq!(error.kind(), ErrorKind::Parse);
        assert_eq!(error.to_string(), "parse message");
    }

    #[cfg(feature = "s2n")]
    #[test]
    fn unsupported_smiles_is_a_naming_error() {
        let input = "C>>O";
        let message = s2n::name_smiles(input, &Budget::default()).unwrap_err();
        let error = smiles_to_name(input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Naming);
        assert_eq!(error.to_string(), message);
    }
}
