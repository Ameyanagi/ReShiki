use reshiki::{
    document::{Document, Point},
    editing,
    rings::Preset,
};

#[test]
fn bundled_gallery_is_one_editable_document_and_samples_copy_independently() -> anyhow::Result<()> {
    let doc: Document =
        serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk"))?;
    doc.validate().map_err(anyhow::Error::msg)?;
    assert!(
        doc.page_layout.is_none(),
        "The gallery uses the ordinary unbounded canvas"
    );
    assert_eq!(
        doc.annotations
            .iter()
            .filter(|a| a.text.starts_with("Section "))
            .count(),
        9
    );
    let mut molecules = 0;
    for ids in editing::groups(&doc, &doc.all_ids()) {
        if !ids.iter().any(|id| doc.atom(*id).is_some()) {
            continue;
        }
        let part = editing::selection(&doc, &ids);
        assert!(
            part.annotations.is_empty(),
            "A structure must copy without its caption"
        );
        part.validate().map_err(anyhow::Error::msg)?;
        let encoded = serde_json::to_string(&part)?;
        let decoded: Document = serde_json::from_str(&encoded)?;
        let mut destination = Document::default();
        assert!(!editing::append(&mut destination, &decoded, Point::new(12., 34.)).is_empty());
        destination.validate().map_err(anyhow::Error::msg)?;
        assert_eq!(destination.atoms.len(), part.atoms.len());
        assert_eq!(destination.bonds.len(), part.bonds.len());
        assert_eq!(destination.abbreviations.len(), part.abbreviations.len());
        assert!(
            destination.page_layout.is_none(),
            "Pasting must not replace the destination's page setup"
        );
        molecules += 1;
    }
    assert!(
        molecules >= 90,
        "The gallery should cover every structure-producing key"
    );
    for label in ["Me", "Boc", "MgBr", "N3", "Cp*"] {
        assert!(
            doc.abbreviations.iter().any(|g| g.label == label),
            "Missing {label}"
        );
    }
    assert!(doc.atoms.iter().any(|a| a.element == "N" && a.label_h > 0));
    Ok(())
}

#[test]
fn atom_drag_adds_the_element_and_preserves_existing_endpoints() -> Result<(), String> {
    for element in ["C", "N", "O", "S", "Cl", "Fe", "*"] {
        let mut source = Document::default();
        let start = source.add_atom("C", Point::default());
        let (drawing, id) =
            editing::add_bonded_atom(&source, start, Point::new(42., 0.), None, element)?;
        assert_eq!(source.atoms.len(), 1);
        assert_eq!(drawing.atoms.len(), 2);
        assert_eq!(drawing.atom(id).map(|a| a.element.as_str()), Some(element));
        assert_eq!(drawing.atom(start).map(|a| a.element.as_str()), Some("C"));
        assert_eq!(drawing.bonds.first().map(|b| b.order), Some(1));
        let (same, _) =
            editing::add_bonded_atom(&drawing, start, Point::new(42., 0.), Some(id), "N")?;
        assert_eq!(
            same, drawing,
            "Connecting to an existing atom must not replace it"
        );
        let mut triple = drawing.clone();
        triple.bonds.first_mut().ok_or("Bond")?.order = 3;
        let (same, _) =
            editing::add_bonded_atom(&triple, start, Point::new(42., 0.), Some(id), "N")?;
        assert_eq!(
            same, triple,
            "Dragging across an existing bond must retain its order"
        );
        assert!(
            editing::add_bonded_atom(&source, start, Point::default(), Some(start), element)
                .is_err()
        );
        assert!(
            editing::add_bonded_atom(&source, 999, Point::new(42., 0.), None, element).is_err()
        );
    }
    Ok(())
}

#[test]
fn benzene_preset_has_alternating_bonds_and_toggles_without_changing_identity() -> anyhow::Result<()>
{
    for alternate in [false, true] {
        let doc = Preset::Benzene.document(42., alternate);
        assert_eq!(doc.atoms.len(), 6);
        assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 3);
        assert_eq!(doc.bonds.iter().filter(|b| b.order == 1).count(), 3);
        assert!(reshiki::aromatic::circles(&doc).is_empty());
        let circle = reshiki::chemistry::document::aromatic_display(&doc, &doc.all_ids())?
            .finish()?
            .document;
        assert_eq!(reshiki::aromatic::circles(&circle).len(), 1);
        let alternating =
            reshiki::chemistry::document::aromatic_display(&circle, &circle.all_ids())?
                .finish()?
                .document;
        assert_eq!(alternating.bonds.iter().filter(|b| b.order == 2).count(), 3);
    }
    Ok(())
}
