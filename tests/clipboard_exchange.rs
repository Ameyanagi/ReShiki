use reshiki::engine::{LocalEngine, Request};

#[tokio::test]
async fn native_numbered_silicon_abbreviations_keep_the_exact_chemical_graph() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let engine = LocalEngine::default();
    for (name, atoms, oxygens, label) in [
        ("single", 6, 1, "SiMe3"),
        ("two", 7, 2, "SiMe2"),
        ("sparse", 7, 2, "SiMe2"),
    ] {
        let path = format!(
            "{}/tests/fixtures/numbered-attachments/{name}.cdx",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(path).unwrap();
        let response = engine
            .request(Request::import("cdx", &STANDARD.encode(&bytes)))
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let identity = &response
            .analysis
            .as_ref()
            .expect("native fixture must have molecular analysis")
            .inchikey;
        assert!(!identity.is_empty(), "{name}: missing molecular identity");
        let doc = response.document.unwrap();
        assert_eq!((doc.atoms.len(), doc.bonds.len()), (atoms, atoms - 1));
        assert!(
            doc.atoms
                .iter()
                .all(|atom| atom.element != "*" && atom.charge == 0)
        );
        let silicon = doc.atoms.iter().find(|atom| atom.element == "Si").unwrap();
        let adjacent = |id| {
            doc.bonds
                .iter()
                .filter(|bond| bond.a == id || bond.b == id)
                .count()
        };
        assert_eq!(adjacent(silicon.id), 4);
        let oxygen: Vec<_> = doc
            .atoms
            .iter()
            .filter(|atom| atom.element == "O")
            .collect();
        assert_eq!(oxygen.len(), oxygens);
        assert!(oxygen.iter().all(|atom| adjacent(atom.id) == 2));
        assert_eq!(doc.abbreviation(silicon.id).unwrap().label, label);
        assert!(doc.bonds.iter().all(|bond| bond.order == 1));
        // The two authored orderings and ChemDraw's rewritten orderings must
        // describe the same molecule, including the sparse numbered case.
        let xml = std::fs::read_to_string(format!(
            "{}/tests/fixtures/numbered-attachments/{name}-source.cdxml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let authored = engine
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap();
        assert_eq!(identity, &authored.analysis.unwrap().inchikey);
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some("cdx".into());
        let exported = engine.request(request).await.unwrap().output.unwrap();
        let roundtrip = engine
            .request(Request::import("cdx", &exported))
            .await
            .unwrap();
        assert_eq!(identity, &roundtrip.analysis.unwrap().inchikey);
        let back = roundtrip.document.unwrap();
        assert_eq!((back.atoms.len(), back.bonds.len()), (atoms, atoms - 1));
    }
}

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
