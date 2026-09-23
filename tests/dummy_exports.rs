use reshiki::{
    attachments,
    document::{Document, Point},
    scene::{self, Primitive},
};

#[tokio::test]
async fn tracked_drawing_centroids_can_pass_the_figure_export_preparation() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let ring = reshiki::editing::ring(&mut doc, Point::default(), 6, true, 5.);
    let point = reshiki::projection::add_centroid(&mut doc, &ring).map_err(anyhow::Error::msg)?;
    let metal = doc.add_atom("Fe", Point::new(0., 100.));
    doc.add_bond(point, metal, 5, "dashed");
    let engine = reshiki::engine::LocalEngine::default();
    let prepared = reshiki::export::checked_document(&engine, doc.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(prepared, doc);
    assert!(
        !reshiki::scene::primitives(&prepared)
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "*"))
    );
    for format in ["svg", "png", "pdf"] {
        assert!(
            !reshiki::export::drawing(&prepared, format)
                .map_err(anyhow::Error::msg)?
                .is_empty()
        );
    }
    doc.atom_mut(point)
        .ok_or_else(|| anyhow::anyhow!("centroid"))?
        .centroid
        .push(u64::MAX);
    assert!(
        reshiki::export::checked_document(&engine, doc)
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn dummy_handles_are_editor_only_and_never_leave_gaps_in_exported_bonds() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let dummy = doc.add_atom("*", Point::new(42., 0.));
    let b = doc.add_atom("C", Point::new(84., 0.));
    doc.add_bond(a, dummy, 1, "plain");
    doc.add_bond(dummy, b, 1, "plain");
    assert_eq!(
        attachments::editor_markers(&doc)
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![dummy]
    );
    assert!(
        scene::primitives(&doc)
            .iter()
            .all(|p| !matches!(p, Primitive::Text { .. }))
    );
    let mut carbon_reference = doc.clone();
    carbon_reference
        .atom_mut(dummy)
        .ok_or_else(|| anyhow::anyhow!("dummy"))?
        .element = "C".into();
    // The figure has exactly the same continuous stroke as an unlabeled junction.
    assert_eq!(scene::svg(&doc), scene::svg(&carbon_reference));
    for format in ["svg", "png", "pdf"] {
        assert_eq!(
            reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
            reshiki::export::drawing(&carbon_reference, format).map_err(anyhow::Error::msg)?,
            "{format}"
        );
    }
    assert_eq!(
        reshiki::export::clipboard_png(&doc).map_err(anyhow::Error::msg)?,
        reshiki::export::clipboard_png(&carbon_reference).map_err(anyhow::Error::msg)?
    );
    assert_eq!(doc.atom(dummy).map(|a| a.element.as_str()), Some("*"));
    let xml = reshiki::exchange::drawing::write(&doc, Default::default())?;
    let parsed = roxmltree::Document::parse(&xml)?;
    let node = parsed
        .descendants()
        .find(|n| n.has_tag_name("n") && n.attribute("Element") == Some("0"))
        .ok_or_else(|| anyhow::anyhow!("wildcard"))?;
    assert_eq!(node.attribute("Visible"), Some("no"));
    assert_eq!(node.attribute("NodeType"), Some("Unspecified"));
    let restored = reshiki::chemistry::cdxml::import_cdxml(&xml)?.document;
    assert_eq!(
        restored.atoms.iter().filter(|a| a.element == "*").count(),
        1
    );
    assert_eq!(restored.bonds.len(), 2);
    Ok(())
}

#[test]
fn attachment_markers_remain_editable_while_named_labels_remain_in_figures() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    for kind in [attachments::Kind::MultiCenter, attachments::Kind::Variable] {
        let anchor = attachments::add(&mut doc, &[a, b], kind).map_err(anyhow::Error::msg)?;
        assert!(attachments::editor_markers(&doc).any(|a| a.id == anchor));
        let fe = doc.add_atom("Fe", Point::new(21., 70.));
        doc.add_bond(anchor, fe, 1, "plain");
        assert!(attachments::editor_markers(&doc).any(|a| a.id == anchor));
    }
    assert!(
        scene::primitives(&doc)
            .iter()
            .all(|p| !matches!(p, Primitive::Text { text, .. } if text == "*"))
    );
    for label in ["M", "L", "X", "E", "Boc", "Cp*"] {
        let end = doc.add_atom("C", Point::new(150., 150.));
        let named = reshiki::atom_text::apply(&doc, end, label, reshiki::atom_text::Mode::Text)
            .map_err(anyhow::Error::msg)?;
        assert!(!attachments::editor_markers(&named).any(|a| a.id == end));
        assert!(
            scene::primitives(&named)
                .iter()
                .any(|p| matches!(p, Primitive::Text { text, .. } if text == label))
        );
    }
    Ok(())
}

#[test]
fn chemdraw_preserves_hidden_dummy_connectivity() -> anyhow::Result<()> {
    let doc = reshiki::chemistry::cdxml::import_cdxml(include_str!(
        "fixtures/chemdraw-attachments/hidden-dummy.cdxml"
    ))?
    .document;
    assert_eq!(doc.atoms.len(), 3);
    assert_eq!(doc.atoms.iter().filter(|a| a.element == "*").count(), 1);
    assert_eq!(doc.bonds.len(), 2);
    assert_eq!(attachments::editor_markers(&doc).count(), 1);
    assert!(
        scene::primitives(&doc)
            .iter()
            .all(|p| !matches!(p, Primitive::Text { text, .. } if text == "*"))
    );
    Ok(())
}
