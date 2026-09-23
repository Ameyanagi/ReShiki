use reshiki::{
    attachments::{self, Kind},
    chemistry::cdxml,
    document::{Document, Point},
    editing, exchange,
};

#[test]
fn files_resaved_by_chemdraw_26_retain_typed_targets() -> anyhow::Result<()> {
    for (source, kind, sizes) in [
        (
            include_str!("fixtures/chemdraw-attachments/eta3-allyl.cdxml"),
            Kind::MultiCenter,
            vec![3],
        ),
        (
            include_str!("fixtures/chemdraw-attachments/eta6-arene.cdxml"),
            Kind::MultiCenter,
            vec![6],
        ),
        (
            include_str!("fixtures/chemdraw-attachments/ferrocene.cdxml"),
            Kind::MultiCenter,
            vec![5, 5],
        ),
        (
            include_str!("fixtures/chemdraw-attachments/variable-arene.cdxml"),
            Kind::Variable,
            vec![6],
        ),
    ] {
        let doc = cdxml::import_cdxml(source)?.document;
        let points: Vec<_> = doc
            .atoms
            .iter()
            .filter(|a| a.attachment.is_some())
            .collect();
        assert_eq!(
            points.iter().map(|a| a.centroid.len()).collect::<Vec<_>>(),
            sizes
        );
        assert!(
            points
                .iter()
                .all(|a| a.attachment == Some(kind) && a.element == "*")
        );
        let xml = exchange::drawing::write(&doc, Default::default())?;
        let roundtrip = cdxml::import_cdxml(&xml)?.document;
        assert_eq!(roundtrip.atoms.len(), doc.atoms.len());
        assert_eq!(
            roundtrip
                .atoms
                .iter()
                .filter_map(|a| a.attachment.map(|k| (k, a.centroid.clone())))
                .collect::<Vec<_>>(),
            points
                .iter()
                .filter_map(|a| a.attachment.map(|k| (k, a.centroid.clone())))
                .collect::<Vec<_>>()
        );
    }
    let xml = exchange::from_cdx(include_bytes!(
        "fixtures/chemdraw-attachments/eta6-arene.cdx"
    ))
    .map_err(anyhow::Error::msg)?;
    let doc = cdxml::import_cdxml(&xml)?.document;
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.attachment == Some(Kind::MultiCenter))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn chemdraw_expand_label_keeps_all_twelve_common_group_graphs() -> anyhow::Result<()> {
    let collapsed = cdxml::import_cdxml(include_str!(
        "fixtures/chemdraw-attachments/common-groups.cdxml"
    ))?
    .document;
    let expanded = cdxml::import_cdxml(include_str!(
        "fixtures/chemdraw-attachments/common-groups-expanded.cdxml"
    ))?
    .document;
    assert_eq!(collapsed.abbreviations.len(), 12);
    assert!(expanded.abbreviations.is_empty());
    let identity = |doc: &Document| -> anyhow::Result<String> {
        let molecule = reshiki::chemistry::document::prepare(doc)?;
        Ok(reshiki::chemistry::smiles::write::write(&molecule.state, Default::default())?.text)
    };
    assert_eq!(identity(&collapsed)?, identity(&expanded)?);
    Ok(())
}

fn allyl(kind: &str, members: &str) -> String {
    format!(
        r#"<CDXML BondLength="14.4"><page id="1"><fragment id="2">
      <n id="10" p="0 0"/><n id="20" p="14.4 0"/><n id="30" p="21.6 12.47"/>
      <n id="40" p="12 4" NodeType="{kind}" Attachments="{members}"/>
      <n id="50" p="12 24" Element="46" Charge="2"/>
      <b id="60" B="10" E="20" Order="2"/><b id="70" B="20" E="30"/>
      <b id="80" B="40" E="50"/>
    </fragment></page></CDXML>"#
    )
}

#[test]
fn long_v3000_target_lists_use_continuation_records() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let members: Vec<_> = (0..24)
        .map(|i| doc.add_atom("C", Point::new(i as f32 * 42., 0.)))
        .collect();
    for pair in members.windows(2) {
        if let [a, b] = pair {
            doc.add_bond(*a, *b, 1, "plain");
        }
    }
    let point =
        attachments::add(&mut doc, &members, Kind::MultiCenter).map_err(anyhow::Error::msg)?;
    let metal = doc.add_atom("Fe", Point::new(300., 84.));
    doc.add_bond(point, metal, 1, "plain");
    let block = reshiki::chemistry::molfile::write_document(&doc)?;
    assert!(block.lines().any(|line| line.ends_with('-')));
    let imported = reshiki::chemistry::molfile::read(&block)?;
    assert_eq!(
        imported.annotations.attachments.first().map(|a| &a.members),
        Some(&members)
    );
    Ok(())
}

#[test]
fn boc_can_be_added_to_a_drawing_with_semantic_attachments() -> anyhow::Result<()> {
    let mut doc = fixture(Kind::MultiCenter)?;
    let n = doc.add_atom("N", Point::new(200., 0.));
    let c = doc.add_atom("C", Point::new(242., 0.));
    doc.add_bond(n, c, 1, "plain");
    let doc = reshiki::atom_text::apply(&doc, c, "Boc", reshiki::atom_text::Mode::Auto)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        doc.atom(4).and_then(|a| a.attachment),
        Some(Kind::MultiCenter)
    );
    assert_eq!(
        doc.abbreviations.first().map(|a| a.label.as_str()),
        Some("Boc")
    );
    let xml = exchange::drawing::write(&doc, Default::default())?;
    let restored = cdxml::import_cdxml(&xml)?.document;
    assert!(attachments::present(&restored));
    assert_eq!(
        restored.abbreviations.first().map(|a| a.label.as_str()),
        Some("Boc")
    );
    Ok(())
}

#[test]
fn chemdraw_attachment_must_not_become_carbon() -> anyhow::Result<()> {
    for kind in ["MultiAttachment", "VariableAttachment"] {
        let imported = cdxml::import_cdxml(&allyl(kind, "10 20 30"))?;
        assert_eq!(
            imported
                .document
                .atoms
                .iter()
                .filter(|a| a.element == "C")
                .count(),
            3,
            "{kind}"
        );
        assert_eq!(
            imported
                .document
                .atoms
                .iter()
                .filter(|a| a.element == "*")
                .count(),
            1,
            "{kind}"
        );
        let point = imported
            .document
            .atom(4)
            .ok_or_else(|| anyhow::anyhow!("attachment"))?;
        assert_eq!(point.attachment, Kind::from_cdxml(kind));
        assert_eq!(point.centroid, vec![1, 2, 3]);
        assert!(point.no_implicit);
    }
    Ok(())
}

fn fixture(kind: Kind) -> anyhow::Result<Document> {
    Ok(cdxml::import_cdxml(&allyl(kind.cdxml(), "10 20 30"))?.document)
}

#[test]
fn chemdraw_cdxml_and_binary_cdx_keep_targets_and_distributed_charge() -> anyhow::Result<()> {
    for kind in [Kind::MultiCenter, Kind::Variable] {
        let mut doc = fixture(kind)?;
        let a = doc.atom_mut(4).ok_or_else(|| anyhow::anyhow!("point"))?;
        a.charge = -1;
        a.radical_electrons = 1;
        let xml = exchange::drawing::write(&doc, Default::default())?;
        let parsed = roxmltree::Document::parse(&xml)?;
        let point = parsed
            .descendants()
            .find(|n| n.attribute("NodeType") == Some(kind.cdxml()))
            .ok_or_else(|| anyhow::anyhow!("missing ChemDraw node"))?;
        assert_eq!(point.attribute("Attachments"), Some("3 4 5"));
        assert_eq!(point.attribute("Element"), None);
        assert!(!point.children().any(|n| n.has_tag_name("t")));
        let binary = exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
        let binary_xml = exchange::from_cdx(&binary).map_err(anyhow::Error::msg)?;
        for xml in [&xml, &binary_xml] {
            let restored = cdxml::import_cdxml(xml)?.document;
            let a = restored
                .atom(4)
                .ok_or_else(|| anyhow::anyhow!("restored point"))?;
            assert_eq!(a.attachment, Some(kind));
            assert_eq!(a.centroid, vec![1, 2, 3]);
            assert_eq!(a.charge, -1);
            assert_eq!(a.radical_electrons, 1);
            assert_eq!(restored.bonds.len(), 3);
        }
    }
    Ok(())
}

#[test]
fn malformed_attachment_targets_fail_atomically() {
    for kind in ["MultiAttachment", "VariableAttachment"] {
        for members in ["", "10", "10 10", "10 40", "10 999", "10 twenty", "10 -1"] {
            assert!(
                cdxml::import_cdxml(&allyl(kind, members)).is_err(),
                "{kind}: {members}"
            );
        }
        let source = allyl(kind, "10 20 30");
        assert!(cdxml::import_cdxml(&source.replace(" Attachments=\"10 20 30\"", "")).is_err());
        assert!(
            cdxml::import_cdxml(&source.replace("B=\"40\" E=\"50\"", "B=\"40\" E=\"10\"")).is_err()
        );
        assert!(cdxml::import_cdxml(&source.replace("<n id=\"20\"", "<n id=\"10\"")).is_err());
    }
}

#[test]
fn native_save_copy_selection_and_delete_keep_valid_membership() -> anyhow::Result<()> {
    for kind in [Kind::MultiCenter, Kind::Variable] {
        let doc = fixture(kind)?;
        let json = serde_json::to_string(&doc)?;
        let restored: Document = serde_json::from_str(&json)?;
        assert_eq!(restored, doc);
        let selected = editing::selection(&doc, &[4, 5]);
        assert_eq!(selected.atoms.len(), 5);
        let mut target = Document::default();
        target.add_atom("O", Point::new(-100., -100.));
        let pasted = editing::append(&mut target, &selected, Point::new(200., 100.));
        assert_eq!(pasted.len(), 5);
        let point = target
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .ok_or_else(|| anyhow::anyhow!("pasted point"))?;
        assert_eq!(point.attachment, Some(kind));
        assert_eq!(point.centroid, vec![2, 3, 4]);
        assert_eq!(editing::groups(&doc, &doc.all_ids()).len(), 1);
        assert_eq!(
            reshiki::reactions::molecules(&doc, &[5]),
            vec![doc.all_ids()]
        );
        target.delete(&[2]);
        assert!(!attachments::present(&target));
        target.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

#[test]
fn attachment_positions_are_editable_and_do_not_snap_on_unrelated_edits() -> anyhow::Result<()> {
    let mut doc = fixture(Kind::Variable)?;
    let before = doc
        .atom(4)
        .ok_or_else(|| anyhow::anyhow!("point"))?
        .position;
    doc.translate(&[4], 12., -9.);
    reshiki::projection::sync_centroids(&mut doc);
    assert_eq!(
        doc.atom(4)
            .ok_or_else(|| anyhow::anyhow!("point"))?
            .position,
        before.offset(12., -9.)
    );
    let point = doc.atom(4).ok_or_else(|| anyhow::anyhow!("point"))?;
    assert!(attachments::hidden(point, &doc));
    doc.bonds.retain(|b| b.a != 4 && b.b != 4);
    assert!(attachments::hidden(
        doc.atom(4).ok_or_else(|| anyhow::anyhow!("point"))?,
        &doc
    ));
    Ok(())
}

#[test]
fn drawing_centroids_are_not_silently_promoted_to_chemical_attachments() -> anyhow::Result<()> {
    let mut doc = fixture(Kind::MultiCenter)?;
    assert!(reshiki::chemistry::document::prepare(&doc).is_err());
    doc.atom_mut(4)
        .ok_or_else(|| anyhow::anyhow!("point"))?
        .attachment = None;
    assert!(exchange::drawing::write(&doc, Default::default()).is_err());
    let before = doc.clone();
    assert!(attachments::add(&mut doc, &[1, 999], Kind::MultiCenter).is_err());
    assert_eq!(doc, before);
    Ok(())
}

#[test]
fn v3000_all_and_any_preserve_targets_through_cdxml() -> anyhow::Result<()> {
    for (kind, mode) in [(Kind::MultiCenter, "ALL"), (Kind::Variable, "ANY")] {
        let doc = fixture(kind)?;
        let block = reshiki::chemistry::molfile::write_document(&doc)?;
        assert!(block.contains(&format!("ENDPTS=(3 1 2 3) ATTACH={mode}")));
        let imported = reshiki::chemistry::molfile::read(&block)?;
        let draft = imported.drawing()?;
        let labels = draft.labels()?;
        let restored = draft.finish(labels)?;
        let point = restored.atom(4).ok_or_else(|| anyhow::anyhow!("point"))?;
        assert_eq!(point.attachment, Some(kind));
        assert_eq!(point.centroid, vec![1, 2, 3]);
        assert_eq!(point.element, "*");
        let xml = exchange::drawing::write(&restored, Default::default())?;
        assert_eq!(
            cdxml::import_cdxml(&xml)?
                .document
                .atom(4)
                .and_then(|a| a.attachment),
            Some(kind)
        );
        for (from, to) in [
            ("ENDPTS=(3 1 2 3)", "ENDPTS=(2 1 2 3)"),
            ("ENDPTS=(3 1 2 3)", "ENDPTS=(3 1 2 99)"),
            ("ENDPTS=(3 1 2 3)", "ENDPTS=(3 1 2 4)"),
            ("ENDPTS=(3 1 2 3)", "ENDPTS=(3 1 2 2)"),
            ("ENDPTS=(3 1 2 3) ", ""),
        ] {
            assert!(
                reshiki::chemistry::molfile::read(&block.replace(from, to)).is_err(),
                "{to}"
            );
        }
    }
    Ok(())
}

#[test]
fn common_groups_keep_the_real_graph_in_chemdraw_interchange() -> anyhow::Result<()> {
    for label in reshiki::abbreviations::PRESETS {
        let mut doc = Document::default();
        let n = doc.add_atom("N", Point::new(0., 0.));
        let c = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(n, c, 1, "plain");
        let result = reshiki::atom_text::apply(&doc, c, label, reshiki::atom_text::Mode::Group)
            .map_err(anyhow::Error::msg)?;
        assert!(result.atoms.iter().all(|a| a.element != "*"), "{label}");
        assert_eq!(result.abbreviations.len(), 1, "{label}");
        let xml = exchange::drawing::write(&result, Default::default())?;
        for xml in [
            xml.clone(),
            exchange::from_cdx(&exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?,
        ] {
            let restored = cdxml::import_cdxml(&xml)?.document;
            assert_eq!(restored.atoms.len(), result.atoms.len(), "{label} atoms");
            assert_eq!(restored.bonds.len(), result.bonds.len(), "{label} bonds");
            assert!(
                restored.abbreviations.iter().any(|a| a.label == *label),
                "{label} label"
            );
            let before = reshiki::chemistry::document::prepare(&result)?;
            let after = reshiki::chemistry::document::prepare(&restored)?;
            let facts = |mol: &reshiki::chemistry::document::Molecule| {
                let mut atoms: Vec<_> = mol
                    .state
                    .graph
                    .atoms
                    .iter()
                    .map(|a| (a.atomic_number, a.charge, a.isotope))
                    .collect();
                atoms.sort_unstable();
                atoms
            };
            assert_eq!(facts(&before), facts(&after), "{label} composition");
            assert_eq!(
                reshiki::chemistry::smiles::write::write(&before.state, Default::default())?.text,
                reshiki::chemistry::smiles::write::write(&after.state, Default::default())?.text,
                "{label} chemical graph"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn public_import_export_preserves_semantics_without_false_molecular_analysis()
-> anyhow::Result<()> {
    use reshiki::engine::{ChemistryEngine, LocalEngine, Request};
    let engine = LocalEngine::default();
    for kind in [Kind::MultiCenter, Kind::Variable] {
        let imported = engine
            .execute(Request::import("cdxml", &allyl(kind.cdxml(), "10 20 30")))
            .await
            .map_err(anyhow::Error::msg)?;
        assert!(imported.analysis.is_none());
        assert!(!imported.warnings.is_empty());
        let doc = imported
            .document
            .ok_or_else(|| anyhow::anyhow!("imported drawing"))?;
        assert_eq!(
            reshiki::export::checked_document(&engine, doc.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            doc
        );
        for format in ["cdxml", "cdx", "mol"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let exported = engine.execute(request).await.map_err(anyhow::Error::msg)?;
            assert!(exported.analysis.is_none());
            let output = exported.output.ok_or_else(|| anyhow::anyhow!("export"))?;
            let restored = engine
                .execute(Request::import(format, &output))
                .await
                .map_err(anyhow::Error::msg)?;
            let restored = restored
                .document
                .ok_or_else(|| anyhow::anyhow!("roundtrip"))?;
            assert_eq!(
                restored.atom(4).and_then(|a| a.attachment),
                Some(kind),
                "{format}"
            );
        }
        assert!(
            engine
                .execute(Request::molecule("analyze", doc))
                .await
                .is_err()
        );
    }
    Ok(())
}
