//! The built-in catalogue: data in assets/pieces, read the same way as any
//! user's piece.
use super::format::{Definition, parse};
use std::sync::{Arc, OnceLock};

const FILES: [&str; 22] = [
    include_str!("../../../../assets/pieces/rbf-50-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/rbf-100-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/rbf-100-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/rbf-300-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/rbf-500-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/schlenk-flask-100-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/schlenk-tube-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/jy-storage-flask-100.reshiki-piece"),
    include_str!("../../../../assets/pieces/jy-schlenk-tube.reshiki-piece"),
    include_str!("../../../../assets/pieces/liebig-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/liebig-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/stopper-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/stopper-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/septum-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/septum-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/red-29-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/exp-15-29.reshiki-piece"),
    include_str!("../../../../assets/pieces/stand.reshiki-piece"),
    include_str!("../../../../assets/pieces/clamp-3finger.reshiki-piece"),
    include_str!("../../../../assets/pieces/ring.reshiki-piece"),
    include_str!("../../../../assets/pieces/keck-clip-15.reshiki-piece"),
    include_str!("../../../../assets/pieces/keck-clip-29.reshiki-piece"),
];

/// Every built-in piece, in catalogue order. A file that fails its own check
/// is a build error caught by the tests, so it is left out here.
pub fn catalogue() -> &'static [Arc<Definition>] {
    static CATALOGUE: OnceLock<Vec<Arc<Definition>>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        FILES
            .iter()
            .filter_map(|text| parse(text).ok())
            .map(Arc::new)
            .collect()
    })
}

/// A built-in piece by ID.
pub fn find(id: &str) -> Option<Arc<Definition>> {
    catalogue().iter().find(|d| d.id == id).cloned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_built_in_piece_passes_its_check() {
        for (i, text) in super::FILES.iter().enumerate() {
            super::parse(text).unwrap_or_else(|e| panic!("built-in piece {i}: {e}"));
        }
        assert_eq!(super::catalogue().len(), super::FILES.len());
    }
}
