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
    "septum-hose-7",
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
    "schlenk-tube-straight-15",
    "schlenk-tube-straight-15-left",
    "schlenk-tube-straight-29",
    "schlenk-tube-straight-29-left",
    "stopcock-three-way-hose",
    "stopcock-three-way-15",
    "stopcock-three-way-29",
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
    "mass-flow-controller",
    "compression-tube-60-1-4",
    "compression-tube-120-1-4",
    "compression-tube-bend-90-1-4",
    "compression-union-1-4",
    "compression-elbow-1-4",
    "compression-tee-1-4",
    "compression-cap-1-4",
    "compression-bulkhead-union-1-4",
    "compression-bulkhead-union-bored-through-1-4",
    "compression-tube-60-1-16",
    "compression-tube-120-1-16",
    "compression-tube-bend-90-1-16",
    "compression-union-1-16",
    "compression-elbow-1-16",
    "compression-tee-1-16",
    "compression-cap-1-16",
    "compression-bulkhead-union-1-16",
    "compression-bulkhead-union-bored-through-1-16",
    "compression-tube-60-1-8",
    "compression-tube-120-1-8",
    "compression-tube-bend-90-1-8",
    "compression-union-1-8",
    "compression-elbow-1-8",
    "compression-tee-1-8",
    "compression-cap-1-8",
    "compression-bulkhead-union-1-8",
    "compression-bulkhead-union-bored-through-1-8",
    "compression-tube-60-3-8",
    "compression-tube-120-3-8",
    "compression-tube-bend-90-3-8",
    "compression-union-3-8",
    "compression-elbow-3-8",
    "compression-tee-3-8",
    "compression-cap-3-8",
    "compression-bulkhead-union-3-8",
    "compression-bulkhead-union-bored-through-3-8",
    "compression-tube-60-6-mm",
    "compression-tube-120-6-mm",
    "compression-tube-bend-90-6-mm",
    "compression-union-6-mm",
    "compression-elbow-6-mm",
    "compression-tee-6-mm",
    "compression-cap-6-mm",
    "compression-bulkhead-union-6-mm",
    "compression-bulkhead-union-bored-through-6-mm",
    "compression-reducing-union-1-8-1-16",
    "compression-reducing-union-1-4-1-8",
    "compression-reducing-union-3-8-1-4",
    "compression-reducing-union-6-mm-1-4",
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

    #[test]
    fn the_straight_schlenk_sidearms_keep_the_s_type_valve_controls_and_ports() {
        for suffix in ["15", "15-left", "29", "29-left"] {
            let straight = super::find(&format!("reshiki/schlenk-tube-straight-{suffix}")).unwrap();
            let bent = super::find(&format!("reshiki/schlenk-tube-{suffix}")).unwrap();
            let straight_valve = straight.params.iter().find(|p| p.id() == "stopcock");
            let bent_valve = bent.params.iter().find(|p| p.id() == "stopcock");
            assert!(straight_valve.is_some(), "{suffix}");
            assert_eq!(straight_valve, bent_valve);
            for port in ["hose1", "hose2"] {
                let port_of = |definition: &super::Definition| {
                    definition
                        .joints
                        .iter()
                        .find(|joint| joint.id() == port)
                        .map(|joint| (joint.gender(), joint.dir()))
                };
                assert!(port_of(&straight).is_some(), "{suffix} {port}");
                assert_eq!(port_of(&straight), port_of(&bent));
            }
        }
    }

    #[test]
    fn compression_sizes_and_reducers_keep_exact_nominal_interfaces() {
        use crate::equipment::format::{Category, Gender, HoldKind, Joint, Param, PlugFamily};

        let sizes = [
            ("1-16", 1.5875, 7.),
            ("1-8", 3.175, 10.5),
            ("1-4", 6.35, 13.5),
            ("3-8", 9.525, 15.),
            ("6-mm", 6., 13.),
        ];
        let families = [
            "compression-tube-60",
            "compression-tube-120",
            "compression-tube-bend-90",
            "compression-union",
            "compression-elbow",
            "compression-tee",
            "compression-cap",
            "compression-bulkhead-union",
            "compression-bulkhead-union-bored-through",
        ];
        for (size, od, depth) in sizes {
            for family in families {
                let id = format!("reshiki/{family}-{size}");
                let piece = super::find(&id).unwrap_or_else(|| panic!("missing {id}"));
                assert_eq!(piece.category, Category::Other, "{id}");
                assert!(piece.keywords.iter().any(|word| word == "joint"), "{id}");
                assert!(piece.names.keys().all(|language| language == "en"), "{id}");
                assert!(
                    piece.params.iter().any(|param| matches!(
                        param,
                        Param::States { id, default, states, .. }
                            if id == "view" && default == "exterior"
                                && states.iter().any(|state| state.id == "section")
                    )),
                    "{id}"
                );
                let tube = family.starts_with("compression-tube");
                let through = family.ends_with("bored-through");
                if through {
                    assert!(piece.joints.is_empty(), "{id}");
                    let bore = piece.holds.iter().find(|hold| hold.id == "bore").unwrap();
                    assert_eq!(bore.kind, HoldKind::ThroughBore, "{id}");
                    assert_eq!(bore.diameter_mm.range(), [od; 2], "{id}");
                    assert!(bore.to[0] - bore.from[0] <= 60., "{id}");
                    assert!(bore.when.is_empty() && bore.part.is_none(), "{id}");
                } else {
                    assert!(!piece.joints.is_empty(), "{id}");
                }
                for joint in &piece.joints {
                    let Joint::Plug {
                        family,
                        gender,
                        size_mm,
                        insertion_mm,
                        ..
                    } = joint
                    else {
                        panic!("non-compression interface on {id}");
                    };
                    assert_eq!(*family, PlugFamily::Compression, "{id}");
                    assert_eq!(*size_mm, od, "{id}");
                    assert_eq!(*gender, if tube { Gender::M } else { Gender::F }, "{id}");
                    assert_eq!(*insertion_mm, if tube { None } else { Some(depth) }, "{id}");
                }
                if tube {
                    assert!(!piece.holds.is_empty(), "{id}");
                    for span in &piece.holds {
                        assert_eq!(span.kind, HoldKind::TubeSpan, "{id}");
                        assert_eq!(span.diameter_mm.range(), [od; 2], "{id}");
                    }
                }
            }
        }
        for (name, left, right) in [
            ("1-8-1-16", (3.175, 10.5), (1.5875, 7.)),
            ("1-4-1-8", (6.35, 13.5), (3.175, 10.5)),
            ("3-8-1-4", (9.525, 15.), (6.35, 13.5)),
            ("6-mm-1-4", (6., 13.), (6.35, 13.5)),
        ] {
            let id = format!("reshiki/compression-reducing-union-{name}");
            let piece = super::find(&id).unwrap();
            assert_eq!(piece.category, Category::Other, "{id}");
            assert_eq!(piece.joints.len(), 2, "{id}");
            for (end, (od, depth)) in [("left", left), ("right", right)] {
                let joint = piece.joints.iter().find(|joint| joint.id() == end).unwrap();
                assert!(
                    matches!(
                        joint,
                        Joint::Plug { family: PlugFamily::Compression, gender: Gender::F, size_mm, insertion_mm: Some(insertion), .. }
                            if *size_mm == od && *insertion == depth
                    ),
                    "{id} {end}"
                );
            }
        }
    }
}
