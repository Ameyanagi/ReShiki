//! The file formats the agent's file tools read and write, by extension: the
//! one table both the tools and the grants' extension checks derive from.
//!
//! Reading follows the app's mapping: native drawings as the app opens a
//! file (src/app/files.rs `prepare`), structures as the Import tab reads
//! them (src/app/import.rs `STRUCTURES`), plus reaction SMILES and InChI.
//! Writing follows the operation export formats, which exclude EMF and CDX,
//! plus native `.rsk` drawings (`Document::file_json`).
use super::{AccessError, echo};
use std::{path::Path, sync::LazyLock};

/// One extension (lower case, without the dot) and the operation formats a
/// file with it opens as and saves as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFormat {
    pub ext: &'static str,
    /// The import format name, when such files can be opened.
    pub import: Option<&'static str>,
    /// The export format name, when such files can be saved.
    pub export: Option<&'static str>,
}

const fn row(
    ext: &'static str,
    import: Option<&'static str>,
    export: Option<&'static str>,
) -> FileFormat {
    FileFormat {
        ext,
        import,
        export,
    }
}

pub const FILE_FORMATS: &[FileFormat] = &[
    row("rsk", Some("reshiki"), Some("reshiki")),
    row("reshiki", Some("reshiki"), None),
    row("moruno", Some("reshiki"), None),
    row("mol", Some("mol"), Some("mol")),
    row("rxn", Some("rxn"), None),
    row("rsmi", Some("rsmi"), None),
    row("cdxml", Some("cdxml"), Some("cdxml")),
    row("cdx", Some("cdx"), None),
    row("smi", Some("smiles"), Some("smiles")),
    row("smiles", Some("smiles"), Some("smiles")),
    row("inchi", Some("inchi"), Some("inchi")),
    row("svg", None, Some("svg")),
    row("pdf", None, Some("pdf")),
    row("png", None, Some("png")),
];

/// The extensions files are opened from: every row with an import format.
pub static READ: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| extensions(|format| format.import.is_some()));

/// The extensions files are saved to: every row with an export format.
pub static WRITE: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| extensions(|format| format.export.is_some()));

fn extensions(keep: fn(&FileFormat) -> bool) -> Vec<&'static str> {
    FILE_FORMATS
        .iter()
        .filter(|format| keep(format))
        .map(|format| format.ext)
        .collect()
}

impl FileFormat {
    /// The row for `path`'s extension, matched ASCII-case-insensitively as
    /// the grants match extensions.
    pub fn of(path: &str) -> Option<&'static Self> {
        let ext = Path::new(path).extension()?.to_str()?;
        FILE_FORMATS
            .iter()
            .find(|format| format.ext.eq_ignore_ascii_case(ext))
    }
}

/// The `extension_not_allowed` error the grants report for `path`.
pub fn extension_not_allowed(path: &str, allowed: &[&str]) -> AccessError {
    AccessError::ExtensionNotAllowed {
        path: echo(path),
        allowed: allowed.iter().map(|&ext| ext.into()).collect(),
    }
}

#[cfg(test)]
mod tests;
