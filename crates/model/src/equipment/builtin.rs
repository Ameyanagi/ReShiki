//! The built-in catalogue: data in assets/pieces, read the same way as any
//! user's piece.
use super::format::{Definition, parse};
use std::sync::{Arc, OnceLock};

macro_rules! piece_files {
    ($($name:literal),* $(,)?) => {
        &[$(include_str!(concat!("../../../../assets/pieces/", $name, ".reshiki-piece"))),*]
    };
}

const FILES: &[&str] = piece_files![
    "rbf-50-15",
    "rbf-100-15",
    "rbf-100-29",
    "rbf-300-29",
    "rbf-500-29",
    "schlenk-flask-100-15",
    "schlenk-tube-15",
    "jy-storage-flask-100",
    "jy-schlenk-tube",
    "liebig-15",
    "liebig-29",
    "stopper-15",
    "stopper-29",
    "septum-15",
    "septum-29",
    "balloon",
    "red-29-15",
    "exp-15-29",
    "thermometer-adapter-15",
    "thermometer",
    "cow-15",
    "hose-7",
    "stand",
    "clamp-3finger",
    "ring",
    "keck-clip-15",
    "keck-clip-29",
    "rbf-300-15",
    "rbf2-300-29",
    "rbf2-300-15",
    "rbf3-300-29",
    "rbf3-300-15",
    "pear-29",
    "pear-15",
    "schlenk-flask-100-29",
    "schlenk-tube-29",
    "schlenk-tube-29-left",
    "schlenk-tube-15-left",
    "jy-storage-flask-25",
    "jy-storage-flask-25-rising",
    "jy-storage-flask-25-cone",
    "jy-storage-flask-25-cone-rising",
    "jy-storage-flask-50",
    "jy-storage-flask-50-rising",
    "jy-storage-flask-50-cone",
    "jy-storage-flask-50-cone-rising",
    "jy-storage-flask-100-rising",
    "jy-storage-flask-100-cone",
    "jy-storage-flask-100-cone-rising",
    "jy-storage-flask-250",
    "jy-storage-flask-250-rising",
    "jy-storage-flask-250-cone",
    "jy-storage-flask-250-cone-rising",
    "jy-schlenk-tube-rising",
    "jy-schlenk-tube-cone",
    "jy-schlenk-tube-cone-rising",
    "jy-schlenk-tube-small",
    "jy-schlenk-tube-small-rising",
    "jy-schlenk-tube-small-cone",
    "jy-schlenk-tube-small-cone-rising",
    "filter-flask",
    "erlenmeyer",
    "beaker",
    "test-tube",
    "allihn-29",
    "allihn-15",
    "dimroth-29",
    "dimroth-15",
    "still-head-29",
    "still-head-29-socket-arm",
    "still-head-15",
    "still-head-15-socket-arm",
    "bend-120-15",
    "bend-120-29",
    "drip-adapter-15",
    "drip-adapter-29",
    "claisen-adapter-29",
    "claisen-adapter-15",
    "claisen-head-29",
    "claisen-head-15",
    "short-path-15",
    "vigreux-29",
    "vigreux-15",
    "fractionating-29",
    "fractionating-15",
    "thermometer-adapter-29",
    "vacuum-adapter-29",
    "vacuum-adapter-15",
    "kugelrohr-bulb-10",
    "kugelrohr-bulb-15",
    "kugelrohr-flask-10",
    "kugelrohr-flask-15",
    "kugelrohr-oven",
    "kugelrohr-drive-10",
    "kugelrohr-drive-15",
    "red-29-10",
    "red-15-10",
    "exp-10-29",
    "exp-10-15",
    "dropping-funnel-15",
    "dropping-funnel-29",
    "stopper-10",
    "drying-tube-15",
    "gas-inlet-15",
    "gas-inlet-29",
    "bubbler",
    "cold-trap",
    "kiriyama-21-stem",
    "kiriyama-21-joint",
    "kiriyama-40-stem",
    "kiriyama-40-joint",
    "kiriyama-60-stem",
    "kiriyama-60-joint",
    "kiriyama-95-stem",
    "kiriyama-95-joint",
    "kiriyama-flask-100",
    "kiriyama-flask-250",
    "kiriyama-flask-500",
    "rubber-stopper",
    "buchner",
    "rubber-adapter",
    "sep-funnel-15",
    "sep-funnel-29",
    "nmr-tube",
    "jy-nmr-tube",
    "chrom-column",
    "stir-bar",
    "oil-bath",
    "heating-mantle",
    "hotplate",
    "dewar",
    "keck-clip-10",
    "boss-head",
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
        let mut ids = std::collections::HashSet::new();
        for (i, text) in super::FILES.iter().enumerate() {
            let definition =
                super::parse(text).unwrap_or_else(|e| panic!("built-in piece {i}: {e}"));
            assert!(
                ids.insert(definition.id.clone()),
                "duplicate {}",
                definition.id
            );
        }
        assert_eq!(super::catalogue().len(), super::FILES.len());
    }
}
