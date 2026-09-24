use reshiki::{
    atom_text::{self, Mode},
    document::{Document, Point},
    engine::{LocalEngine, Request},
    scene::{self, Primitive},
};

fn metal_ligand(order: u8) -> (Document, u64) {
    let mut doc = Document::default();
    let pt = doc.add_atom("Pt", Point::default());
    let n = doc.add_atom("N", Point::new(84., 0.));
    doc.add_bond(n, pt, order, "plain");
    (doc, n)
}

#[tokio::test]
async fn platinum_with_two_ammines_counts_all_six_hydrogens()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    let mut doc = Document::default();
    let pt = doc.add_atom("Pt", Point::default());
    let mut ligands = Vec::new();
    for (element, point) in [
        ("N", Point::new(-70., 0.)),
        ("N", Point::new(0., -70.)),
        ("Cl", Point::new(70., 0.)),
        ("Cl", Point::new(0., 70.)),
    ] {
        let id = doc.add_atom(element, point);
        doc.add_bond(pt, id, 1, "plain");
        if element == "N" {
            ligands.push(id);
            doc = atom_text::apply(&doc, id, "NH3", Mode::Auto)?;
        }
    }
    for operation in ["analyze", "clean"] {
        let result = engine
            .request(Request::molecule(operation, doc.clone()))
            .await?;
        // The reference formula convention places H before the other elements.
        assert_eq!(result.analysis.ok_or("Properties")?.formula, "H6Cl2N2Pt");
        let checked = result.document.ok_or("Drawing")?;
        for id in &ligands {
            assert_eq!(checked.atom(*id).ok_or("Ammonia")?.explicit_h, 3);
        }
        for bond in checked.bonds.iter().filter(|b| b.order == 5) {
            assert_eq!(checked.atom(bond.a).ok_or("Donor")?.element, "N");
            assert_eq!(checked.atom(bond.b).ok_or("Acceptor")?.element, "Pt");
        }
        assert_eq!(checked.bonds.iter().filter(|b| b.order == 5).count(), 2);
    }
    Ok(())
}

#[tokio::test]
async fn typed_ammonia_keeps_real_hydrogens_through_check_and_save()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    for order in [1, 5] {
        let (source, n) = metal_ligand(order);
        let mut doc = atom_text::apply(&source, n, "NH3", Mode::Auto)?;
        let atom = doc.atom(n).ok_or("Missing nitrogen")?;
        assert_eq!(atom.element, "N", "A typed hydride is not a dummy label");
        assert_eq!(atom.explicit_h, 3);
        assert!(atom.no_implicit);
        assert_eq!(
            doc.bonds, source.bonds,
            "Typing must not change bond semantics"
        );
        doc.invalidate_chemistry(&[n]);
        assert!(
            scene::primitives(&doc)
                .iter()
                .any(|p| matches!(p, Primitive::Text {text, ..} if text == "3")),
            "Explicit hydrogens remain visible without a successful calculation"
        );
        let before = doc.clone();
        let checked = engine
            .request(Request::molecule("analyze", doc.clone()))
            .await?;
        assert_eq!(checked.analysis.ok_or("Missing analysis")?.formula, "H3NPt");
        assert_eq!(
            checked
                .document
                .ok_or("Missing drawing")?
                .atom(n)
                .ok_or("Missing atom")?
                .label_h,
            3
        );
        assert_eq!(doc, before);
        let saved: Document = serde_json::from_str(&serde_json::to_string(&doc)?)?;
        assert_eq!(saved, doc);
        for format in ["cdxml", "cdx", "mol"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let output = engine
                .request(request)
                .await?
                .output
                .ok_or("Missing export")?;
            let imported = engine.request(Request::import(format, &output)).await?;
            let back = imported.document.ok_or("Missing imported drawing")?;
            let nitrogen = back
                .atoms
                .iter()
                .find(|a| a.element == "N")
                .ok_or("Lost N")?;
            assert_eq!(nitrogen.label_h, 3, "{format} lost hydrogens");
        }
    }
    Ok(())
}

#[tokio::test]
async fn formula_entry_and_contraction_keep_counts_and_expansion()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    let mut source = Document::default();
    let n = source.add_atom("N", Point::default());
    let end = source.add_atom("C", Point::new(42., 0.));
    source.add_bond(n, end, 1, "plain");
    for (label, expected) in [
        ("C2H5", "C2H7N"),
        ("C₂H₅", "C2H7N"),
        ("CH2CH3", "C2H7N"),
        ("OCH3", "CH5NO"),
        ("OC2H5", "C2H7NO"),
    ] {
        let doc = atom_text::apply(&source, end, label, Mode::Auto)?;
        assert_eq!(doc.abbreviation(end).ok_or("Missing group")?.label, label);
        let analysis = engine
            .request(Request::molecule("analyze", doc.clone()))
            .await?
            .analysis
            .ok_or("No analysis")?;
        assert_eq!(analysis.formula, expected, "{label}");
        let renamed = atom_text::apply(&doc, end, "MyGroup", Mode::Group)?;
        assert_eq!(renamed.atoms, doc.atoms);
        assert_eq!(renamed.bonds, doc.bonds);
        let mut expanded = renamed;
        expanded.expand_abbreviations(&[end]);
        let analysis = engine
            .request(Request::molecule("analyze", expanded))
            .await?
            .analysis
            .ok_or("No analysis")?;
        assert_eq!(analysis.formula, expected);
    }
    Ok(())
}

#[tokio::test]
async fn explicit_hydrogens_contribute_to_properties_without_hiding_valence_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    for (label, expected) in [
        ("NH3", "H3N"),
        ("NH4+", "H4N+"),
        ("OH2", "H2O"),
        ("CH4", "CH4"),
        ("SiH4", "H4Si"),
    ] {
        let mut source = Document::default();
        let id = source.add_atom("C", Point::default());
        let doc = atom_text::apply(&source, id, label, Mode::Auto)?;
        assert_eq!(
            engine
                .request(Request::molecule("analyze", doc.clone()))
                .await?
                .analysis
                .ok_or("No analysis")?
                .formula,
            expected
        );
        let clean = engine
            .request(Request::molecule("clean", doc))
            .await?
            .document
            .ok_or("No cleaned drawing")?;
        assert_eq!(
            atom_text::entry(clean.atoms.first().ok_or("Missing atom")?),
            label
        );
    }
    let (source, n) = metal_ligand(1);
    let unusual = atom_text::apply(&source, n, "NH5", Mode::Auto)?;
    assert!(
        engine
            .request(Request::molecule("analyze", unusual.clone()))
            .await
            .is_err()
    );
    assert!(scene::svg(&unusual).contains(">5</text>"));
    for label in ["R-X", "ligand-1", "A+B", "Hf", "He"] {
        assert!(
            atom_text::apply(&source, n, label, Mode::Auto).is_ok(),
            "{label}"
        );
    }
    for label in ["NH256", "NH3+9", "NH3+foo"] {
        assert!(
            atom_text::apply(&source, n, label, Mode::Auto).is_err(),
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn explicit_labels_are_reversible_and_invalid_chemistry_is_still_drawable() -> Result<(), String> {
    let (source, n) = metal_ligand(1);
    for input in ["NH3", "NH₃", "H3N"] {
        let doc = atom_text::apply(&source, n, input, Mode::Auto)?;
        assert_eq!(doc.atom(n).ok_or("Missing N")?.explicit_h, 3);
        let automatic = atom_text::apply(&doc, n, "N", Mode::Auto)?;
        assert_eq!(automatic.atom(n).ok_or("Missing N")?.explicit_h, 0);
        assert!(!automatic.atom(n).ok_or("Missing N")?.no_implicit);
    }
    let ammonium = atom_text::apply(&source, n, "NH4+", Mode::Auto)?;
    assert_eq!(ammonium.atom(n).ok_or("Missing N")?.charge, 1);
    let unusual = atom_text::apply(&source, n, "NH5", Mode::Auto)?;
    unusual.validate()?;
    assert_eq!(unusual.atom(n).ok_or("Missing N")?.explicit_h, 5);
    assert!(scene::svg(&unusual).contains(">5</text>"));
    assert_eq!(source.atom(n).ok_or("Missing N")?.explicit_h, 0);
    Ok(())
}
