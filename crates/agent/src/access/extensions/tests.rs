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
