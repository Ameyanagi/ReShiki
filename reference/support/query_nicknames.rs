//! ReShiki has no query atoms. Its molecular CDXML parser refuses an exact A,
//! a or Q generic nickname, which RDKit 2026.09 reads as an atom query
//! (rdkit#9567); the application imports them as labelled dummy atoms with a
//! warning. See "Deliberate differences" in reference/README.md.

/// Whether the CDXML holds such a generic nickname.
pub fn present(cdxml: &str) -> bool {
    cdxml
        .split("GenericNickname=\"")
        .skip(1)
        .any(|rest| matches!(rest.split('"').next(), Some("A" | "a" | "Q")))
}
