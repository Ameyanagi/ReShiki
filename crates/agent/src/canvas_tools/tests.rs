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
