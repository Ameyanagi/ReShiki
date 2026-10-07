use super::*;
use crate::document::Point;

#[test]
fn free_labels_retain_connections_style_coordinates_and_roundtrip()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Cu", Point::new(0., 0.));
    let ligand = doc.add_atom("N", Point::new(60., 0.));
    doc.add_bond(ligand, metal, 5, "plain");
    let original = doc.clone();
    for label in ["M", "L", "X", "R₁", "custom ligand", "Boc"] {
        let mode = if label == "Boc" {
            Mode::Text
        } else {
            Mode::Auto
        };
        let named = apply(&doc, metal, label, mode)?;
        assert_eq!(named.bonds, original.bonds);
        assert_eq!(
            named.atom(metal).ok_or("Missing atom")?.position,
            Point::new(0., 0.)
        );
        assert_eq!(named.atom(metal).ok_or("Missing atom")?.element, "*");
        assert_eq!(
            named
                .atom(metal)
                .ok_or("Missing atom")?
                .display
                .variable
                .as_deref(),
            Some(label)
        );
        assert!(named.atom(metal).ok_or("Missing atom")?.no_implicit);
        // A label such as R₁ can span multiple font runs when the requested
        // face lacks a glyph. Check visible text, not contiguous XML bytes.
        let svg = crate::scene::svg(&named);
        let xml = roxmltree::Document::parse(&svg)?;
        let rendered: String = xml
            .descendants()
            .filter(|node| node.has_tag_name("text"))
            .filter_map(|node| node.text())
            .collect();
        assert!(rendered.contains(label), "Missing label {label}: {svg}");
        let saved = serde_json::to_string(&named)?;
        let loaded: Document = serde_json::from_str(&saved)?;
        loaded.validate()?;
        assert_eq!(loaded, named);
        let copy = crate::editing::selection(&named, &[metal, ligand]);
        assert_eq!(
            copy.atom(metal)
                .ok_or("Missing atom")?
                .display
                .variable
                .as_deref(),
            Some(label)
        );
        let restored = apply(&named, metal, "Fe", Mode::Auto)?;
        assert_eq!(restored.atom(metal).ok_or("Missing atom")?.element, "Fe");
        assert!(
            restored
                .atom(metal)
                .ok_or("Missing atom")?
                .display
                .variable
                .is_none()
        );
        assert_eq!(restored.bonds, original.bonds);
    }
    assert_eq!(doc, original);
    Ok(())
}

#[test]
fn boc_is_a_real_expandable_group_and_invalid_input_is_atomic()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    let endpoint = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(n, endpoint, 1, "plain");
    let before = doc.clone();
    let boc = apply(&doc, endpoint, "Boc", Mode::Auto)?;
    assert!(boc.atoms.len() > 2);
    let group = boc.abbreviation(endpoint).ok_or("Missing abbreviation")?;
    assert_eq!(group.label, "Boc");
    assert_eq!(
        boc.atom(endpoint).ok_or("Missing atom")?.position,
        doc.atom(endpoint).ok_or("Missing atom")?.position
    );
    assert!(boc.bonds.contains(doc.bonds.first().ok_or("Missing bond")?));
    crate::chemistry::document::prepare(&boc)?;
    let mut expanded = boc.clone();
    assert_eq!(expanded.expand_abbreviations(&[endpoint]), 1);
    assert_eq!(expanded.atoms, boc.atoms);
    assert_eq!(expanded.bonds, boc.bonds);
    assert!(apply(&boc, endpoint, "M", Mode::Auto).is_err());
    for label in ["", "  ", "X\nY", "abcdefghijklmnopqrstuvwxyz1234567"] {
        assert!(apply(&doc, endpoint, label, Mode::Text).is_err());
    }
    assert_eq!(doc, before);
    doc.add_bond(n, endpoint, 2, "plain");
    assert!(apply(&doc, endpoint, "Boc", Mode::Auto).is_err());
    assert!(apply(&doc, endpoint, "Boc", Mode::Text).is_ok());
    Ok(())
}
