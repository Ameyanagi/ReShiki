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
mod tests {
    use super::*;
    use crate::ops::documents::{EXPORT_FORMATS, IMPORT_FORMATS};
    use std::collections::HashSet;

    #[test]
    fn every_operation_format_has_an_extension() {
        let imports: HashSet<_> = FILE_FORMATS.iter().filter_map(|row| row.import).collect();
        let exports: HashSet<_> = FILE_FORMATS.iter().filter_map(|row| row.export).collect();
        let mut expected_imports: HashSet<_> = IMPORT_FORMATS.iter().copied().collect();
        expected_imports.remove("auto");
        assert_eq!(imports, expected_imports);
        let mut expected_exports: HashSet<_> = EXPORT_FORMATS.iter().copied().collect();
        expected_exports.insert("reshiki");
        assert_eq!(exports, expected_exports);
        for ext in reshiki_io::compatibility::NATIVE_EXTENSIONS {
            assert_eq!(
                FileFormat::of(&format!("/a.{ext}")).and_then(|row| row.import),
                Some("reshiki"),
                "{ext}"
            );
        }
        for excluded in ["emf", "cdx"] {
            assert!(!WRITE.contains(&excluded), "{excluded}");
        }
    }

    #[test]
    fn extensions_are_unique_lower_case_and_used() {
        let mut seen = HashSet::new();
        for row in FILE_FORMATS {
            assert!(seen.insert(row.ext), "{}", row.ext);
            assert_eq!(row.ext, row.ext.to_ascii_lowercase());
            assert!(row.import.is_some() || row.export.is_some(), "{}", row.ext);
        }
    }

    #[test]
    fn read_and_write_derive_from_the_table() {
        assert_eq!(
            *READ,
            [
                "rsk", "reshiki", "moruno", "mol", "rxn", "rsmi", "cdxml", "cdx", "smi", "smiles",
                "inchi"
            ]
        );
        assert_eq!(
            *WRITE,
            [
                "rsk", "mol", "cdxml", "smi", "smiles", "inchi", "svg", "pdf", "png"
            ]
        );
    }

    #[test]
    fn no_executable_or_script_extension_appears() {
        const RUNNABLE: &str = "exe com bat cmd msi msp scr pif cpl dll sys lnk url hta vbs vbe \
            js jse mjs wsf wsh ps1 psm1 sh bash zsh csh fish command app pkg dmg so dylib py \
            pyw pl rb php jar class desktop applescript scpt workflow reg inf";
        for row in FILE_FORMATS {
            assert!(
                RUNNABLE
                    .split_whitespace()
                    .all(|runnable| runnable != row.ext),
                "{}",
                row.ext
            );
        }
    }

    #[test]
    fn rows_are_found_by_extension_case_insensitively() {
        assert_eq!(FileFormat::of("/x/a.MOL").map(|row| row.ext), Some("mol"));
        assert_eq!(
            FileFormat::of("/x/a.b.cdxml").map(|row| row.ext),
            Some("cdxml")
        );
        for missing in ["/x/a.exe", "/x/a", "/x/.mol", ""] {
            assert_eq!(FileFormat::of(missing), None, "{missing}");
        }
        assert_eq!(
            extension_not_allowed("/x/a.exe", &["mol"]),
            AccessError::ExtensionNotAllowed {
                path: "/x/a.exe".into(),
                allowed: vec!["mol".into()],
            }
        );
    }
}
