use reshiki::engine::{LocalEngine, Request};

#[tokio::test]
async fn binary_exchange_keeps_supported_structure_and_figure_objects() {
    let engine = LocalEngine::default();
    for name in [
        "bond-styles-chemdraw",
        "formatted-label-chemdraw",
        "graphics-chemdraw",
        "symbols-chemdraw",
        "arrows-chemdraw",
        "atom-labels-chemdraw",
        "attached-symbols-chemdraw",
        "grouped-aspirin-chemdraw",
        "ring-presets-chemdraw",
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}.cdxml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let original = engine
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap();
        let doc = original.document.unwrap();
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some("cdxml".into());
        let xml = engine
            .request(request.clone())
            .await
            .unwrap()
            .output
            .unwrap();
        // Scientific graphics intentionally exchange as editable vector groups.
        // Compare those objects with the XML path, not their ReShiki-only presets.
        let exchange = engine
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap()
            .document
            .unwrap();
        request.format = Some("cdx".into());
        let binary = engine
            .request(request)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let restored = engine
            .request(Request::import("cdx", &binary.output.unwrap()))
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            original.analysis.as_ref().map(|a| &a.inchikey),
            restored.analysis.as_ref().map(|a| &a.inchikey),
            "{name}"
        );
        let back = restored.document.unwrap();
        assert_eq!(doc.atoms.len(), back.atoms.len(), "{name}");
        assert_eq!(doc.bonds.len(), back.bonds.len(), "{name}");
        assert_eq!(doc.arrows.len(), back.arrows.len(), "{name}");
        assert_eq!(exchange.graphics.len(), back.graphics.len(), "{name}");
        assert_eq!(exchange.groups.len(), back.groups.len(), "{name}");
        for (a, b) in exchange.graphics.iter().zip(&back.graphics) {
            assert_eq!(a.path.len(), b.path.len(), "{name}");
            assert_eq!(
                (a.style.stroke, a.style.fill, a.style.pattern),
                (b.style.stroke, b.style.fill, b.style.pattern),
                "{name}"
            );
            assert!(
                (a.style.width_pt - b.style.width_pt).abs() < 0.0001,
                "{name}"
            );
        }
        for (a, b) in doc.bonds.iter().zip(&back.bonds) {
            assert_eq!(
                (a.order, &a.display, a.color),
                (b.order, &b.display, b.color),
                "{name}"
            );
        }
        for (a, b) in doc.annotations.iter().zip(&back.annotations) {
            assert_eq!(
                (&a.text, &a.format.style),
                (&b.text, &b.format.style),
                "{name}"
            );
        }
    }
}
