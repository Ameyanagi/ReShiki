// SPDX-License-Identifier: MIT
// Origin: original nazuchem/ReShiki code; source history and separately attributed data/API context follow.
//   ReShiki, crates/opsin/tests/resource_pins.rs @ 8022c4129f19cde7a8a953cdda46900f799a1277.

//! The embedded resources match provenance/manifest.json and RESOURCE_FINGERPRINT.
//! After a grammar change or a conflicted merge, run
//! `uv run --no-project python scripts/regenerate_opsin_grammar.py`
//! (see provenance/README.md).
use sha2::{Digest, Sha256};
use std::path::Path;

#[test]
fn resources_match_manifest_and_fingerprint() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../provenance/manifest.json")).unwrap();
    let resources = manifest["resources"].as_object().unwrap();
    let mut paths: Vec<_> = resources.keys().collect();
    paths.sort();
    let mut fingerprint = Sha256::new();
    for path in paths {
        let bytes = std::fs::read(root.join(path)).unwrap();
        assert_eq!(
            resources[path],
            format!("{:x}", Sha256::digest(&bytes)),
            "{path} is not the pinned resource; run scripts/regenerate_opsin_grammar.py"
        );
        for part in [path.as_bytes(), b"\0", &bytes, b"\0"] {
            fingerprint.update(part);
        }
    }
    let fingerprint = format!("{:x}", fingerprint.finalize());
    assert_eq!(manifest["resource_fingerprint"]["sha256"], fingerprint);
    assert_eq!(opsin::RESOURCE_FINGERPRINT, fingerprint);
}
