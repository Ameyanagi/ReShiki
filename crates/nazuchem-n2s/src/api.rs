// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/NameToStructureConfig.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinResult.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinWarning.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/NameToStructureConfig.java | b91b610af5ab07560fedb20730d7aef46bb2bca0
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinResult.java | b91b610af5ab07560fedb20730d7aef46bb2bca0
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinWarning.java | b91b610af5ab07560fedb20730d7aef46bb2bca0
// Copyright 2017 Daniel Lowe
// Changes: Rust ownership, arena/container and error-handling adaptation; integrated native changes are documented in PORTING.md.

//! Public result and configuration types, corresponding to OPSIN 2.9.0.

use std::fmt;

/// All five options in upstream `NameToStructureConfig`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOptions {
    pub allow_radicals: bool,
    pub output_radicals_as_wildcard_atoms: bool,
    pub detailed_failure_analysis: bool,
    pub interpret_acids_without_the_word_acid: bool,
    pub warn_rather_than_fail_on_uninterpretable_stereochemistry: bool,
}

impl ParseOptions {
    pub const fn strict() -> Self {
        Self {
            allow_radicals: false,
            output_radicals_as_wildcard_atoms: false,
            detailed_failure_analysis: false,
            interpret_acids_without_the_word_acid: false,
            warn_rather_than_fail_on_uninterpretable_stereochemistry: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Success,
    Warning,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    AppearsAmbiguous,
    StereochemistryIgnored,
}

impl WarningKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppearsAmbiguous => "APPEARS_AMBIGUOUS",
            Self::StereochemistryIgnored => "STEREOCHEMISTRY_IGNORED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning {
    pub kind: WarningKind,
    pub message: String,
}

/// A name interpretation result. Failure never contains a partial structure.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub status: Status,
    pub input: String,
    pub normalized_name: Option<String>,
    pub message: String,
    pub warnings: Vec<ParseWarning>,
    pub structure: Option<crate::Structure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationError(pub(crate) String);

impl fmt::Display for InitializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for InitializationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsingError(pub String);

impl fmt::Display for ParsingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ParsingError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationError(pub String);

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SerializationError {}
