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
pub struct OpsinWarning {
    pub kind: WarningKind,
    pub message: String,
}

/// A name interpretation result. Failure never contains a partial structure.
#[derive(Debug, Clone)]
pub struct OpsinResult {
    pub status: Status,
    pub input: String,
    pub normalized_name: Option<String>,
    pub message: String,
    pub warnings: Vec<OpsinWarning>,
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
