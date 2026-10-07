use super::*;
#[test]
fn planar_coordination_scheme_is_readable_and_ownership_is_not_an_overlap() {
    // A visual regression fixture, not a validated chemical assignment.
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/coordination-layout.rsk"
    ))
    .unwrap();
    assert!(doc.atoms.iter().all(|a| a.depth == 0.));
    assert!(internal_overlaps(&doc).is_empty());
    let issues = quality(&doc, &Composition::default());
    assert!(
        issues
            .iter()
            .any(|s| s.starts_with("Chemical assignments need review:"))
    );
    assert!(!issues.iter().any(|s| s.contains(" overlaps ")));
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.display.variable.as_deref() == Some("E"))
            .count(),
        2
    );
    assert_eq!(crate::ring_arcs::render(&doc).primitives.len(), 4);
    assert_eq!(doc.abbreviations.len(), 4);
    assert!(
        crate::exchange::drawing::write(&doc, Default::default())
            .unwrap_err()
            .to_string()
            .contains("CDXML cannot yet preserve")
    );
}

#[test]
fn detects_internal_collapse_even_in_one_connected_complex() {
    let mut doc = Document::default();
    let cu = doc.add_atom("Cu", Point::default());
    let n = doc.add_atom("N", Point::new(1., 1.));
    doc.add_bond(n, cu, 5, "plain");
    assert!(!internal_overlaps(&doc).is_empty());
    doc.atom_mut(n).unwrap().position = Point::new(0., -doc.drawing_style.bond_length_world);
    assert!(internal_overlaps(&doc).is_empty());
    crate::projection::add_centroid(&mut doc, &[n, cu]).unwrap();
    assert!(internal_overlaps(&doc).is_empty());
}

#[test]
fn haworth_sugar_templates_have_no_internal_overlaps() -> anyhow::Result<()> {
    for template in crate::haworth::templates().map_err(anyhow::Error::msg)? {
        let overlaps = internal_overlaps(&template.document);
        assert!(overlaps.is_empty(), "{}: {overlaps:?}", template.name);
    }
    Ok(())
}
