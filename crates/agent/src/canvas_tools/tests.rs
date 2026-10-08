use crate::document::Document;

#[test]
fn reported_wedge_junctions_have_shared_atoms_and_complete_outlines() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/bond-join-regression.rsk"
    ))
    .unwrap();
    doc.validate().unwrap();
    assert_eq!(doc.atoms.len(), 8);
    assert_eq!(doc.bonds.len(), 10);
    assert_eq!(doc.bonds.iter().filter(|b| b.a == 7 || b.b == 7).count(), 2);
    assert_eq!(doc.bonds.iter().filter(|b| b.a == 8 || b.b == 8).count(), 2);
    let scene = crate::scene::primitives(&doc);
    assert!(
        scene
            .iter()
            .any(|p| matches!(p, crate::scene::Primitive::Path { filled: true, .. }))
    );
    assert!(
        scene
            .iter()
            .any(|p| matches!(p, crate::scene::Primitive::Path { filled: false, .. }))
    );
    assert!(super::image(&doc).is_ok());
}

/// Width and height from a PNG IHDR chunk.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let size = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (size(16), size(20))
}

#[test]
fn image_budget_invariants() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/bond-join-regression.rsk"
    ))
    .unwrap();
    let (width, height) = png_size(&super::image(&doc).unwrap());
    assert!(width <= 1600 && height <= 1000);
    let tree = crate::export::parse_svg(crate::scene::svg(&doc)).unwrap();
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = (1600. / w).min(1000. / h).min(3.);
    assert_eq!(width, (w * scale).ceil().clamp(1., 1600.) as u32);
    assert_eq!(height, (h * scale).ceil().clamp(1., 1000.) as u32);
}

/// Prints canvas image sizes and hashes for a same-machine before/after
/// comparison; the bytes depend on system fonts, so nothing is asserted.
#[test]
#[ignore = "same-machine canvas image parity dump"]
fn canvas_image_parity_dump() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    for (name, bytes) in [
        (
            "bond-join-regression.rsk",
            include_bytes!("../../../../tests/fixtures/bond-join-regression.rsk").as_slice(),
        ),
        (
            "coordination-layout.rsk",
            include_bytes!("../../../../tests/fixtures/coordination-layout.rsk").as_slice(),
        ),
        (
            "adjustable-arcs.rsk",
            include_bytes!("../../../../tests/fixtures/adjustable-arcs.rsk").as_slice(),
        ),
    ] {
        let doc = Document::from_json(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let png = super::image(&doc).unwrap();
        let (width, height) = png_size(&png);
        let mut hasher = DefaultHasher::new();
        png.hash(&mut hasher);
        println!("{name} {width}x{height} {:016x}", hasher.finish());
    }
}

#[test]
fn image_matches_image_within_default() {
    for bytes in [
        include_bytes!("../../../../tests/fixtures/bond-join-regression.rsk").as_slice(),
        include_bytes!("../../../../tests/fixtures/coordination-layout.rsk").as_slice(),
        include_bytes!("../../../../tests/fixtures/adjustable-arcs.rsk").as_slice(),
    ] {
        let doc = Document::from_json(bytes).unwrap();
        assert_eq!(
            super::image(&doc).unwrap(),
            super::image_within(&doc, 1600, 1000).unwrap()
        );
    }
}

#[test]
fn image_within_rejects_an_empty_budget() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/bond-join-regression.rsk"
    ))
    .unwrap();
    assert_eq!(
        super::image_within(&doc, 0, 1000).unwrap_err(),
        "Preview size must be at least 1 × 1 pixels"
    );
    assert!(super::image_within(&doc, 1600, 0).is_err());
    assert!(super::image_within(&doc, 1600, 1000).is_ok());
}
