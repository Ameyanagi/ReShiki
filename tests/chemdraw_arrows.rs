use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::{
    arrows::{Head, HeadShape, Preset},
    chemistry::cdxml::{ArrowReader, presentation},
    engine::{LocalEngine, Request},
    exchange::{from_cdx, to_cdx},
};

#[test]
fn binary_arrows_use_chemdraw_code_and_migrate_legacy_files() -> anyhow::Result<()> {
    let xml = r#"<CDXML><page id="1"><arrow id="2" Tail3D="10 20 0" Head3D="70 20 0" ArrowheadHead="Full" ArrowheadType="Solid"/></page></CDXML>"#;
    let binary = to_cdx(xml).map_err(anyhow::Error::msg)?;
    // File header (22), document object (6), page object (6), then the arrow.
    assert_eq!(&binary[34..40], &[0x21, 0x80, 2, 0, 0, 0]);
    let mut legacy = binary.clone();
    legacy[34] = 0x27;
    let decoded = from_cdx(&legacy).map_err(anyhow::Error::msg)?;
    assert_eq!(to_cdx(&decoded).map_err(anyhow::Error::msg)?, binary);
    let tree = presentation::parse(&decoded)?;
    let arrow = tree
        .descendants()
        .find(|n| n.has_tag_name("arrow"))
        .context("Arrow was lost")?;
    assert_eq!(arrow.attribute("Tail3D"), Some("10 20 0"));
    assert_eq!(arrow.attribute("Head3D"), Some("70 20 0"));
    Ok(())
}

#[test]
fn genuine_chemdraw_file_retains_arrow_geometry_style_and_references() -> anyhow::Result<()> {
    let decoded = from_cdx(include_bytes!("fixtures/chemdraw-arrows/numeric.cdx"))
        .map_err(anyhow::Error::msg)?;
    let saved_xml = include_str!("fixtures/chemdraw-arrows/numeric.cdxml");
    for xml in [decoded.as_str(), saved_xml] {
        let tree = presentation::parse(xml)?;
        let elements: Vec<_> = tree.descendants().filter(|n| n.is_element()).collect();
        let source = elements
            .iter()
            .position(|n| n.has_tag_name("arrow"))
            .context("Missing arrow")?;
        assert_eq!(
            elements.iter().filter(|n| n.has_tag_name("arrow")).count(),
            1
        );
        assert_eq!(elements.iter().filter(|n| n.has_tag_name("n")).count(), 7);
        assert_eq!(
            elements
                .iter()
                .filter(|n| n.has_tag_name("embeddedobject"))
                .count(),
            0
        );
        let palette = presentation::palette(tree.root_element())?;
        let arrow = ArrowReader::new(xml)?.read(source, 1., &palette.colors, 1)?;
        assert_eq!(arrow.kind, Preset::Forward);
        assert_eq!(arrow.style.head, Head::Full);
        assert_eq!(arrow.style.tail, Head::None);
        assert_eq!(arrow.style.shape, HeadShape::Solid);
        assert_eq!(arrow.style.head_length_pt, 6.);
        assert_eq!(arrow.style.head_width_pt, 1.5);
        assert_eq!(arrow.style.head_notch, 0.125);
        assert!((arrow.start.x - 73.327911).abs() < 0.0051);
        assert!((arrow.end.x - 104.18506).abs() < 0.0051);
        assert!((arrow.start.y - 52.304535).abs() < 0.0051);
        assert!((arrow.end.y - arrow.start.y).abs() < 0.0001);
        let step = elements
            .iter()
            .find(|n| n.has_tag_name("step"))
            .context("Lost reaction references")?;
        assert_eq!(step.attribute("ReactionStepArrows"), Some("17"));
        assert_eq!(
            step.attribute("ReactionStepObjectsBelowArrow"),
            Some("20 16")
        );
        let fallback = elements
            .iter()
            .find(|n| n.attribute("id") == Some("17"))
            .context("Missing linked graphic")?;
        assert_eq!(fallback.attribute("SupersededBy"), Some("23"));
    }
    // Real ChemDraw previously discarded the invalid-tag arrow altogether.
    let before = presentation::parse(include_str!(
        "fixtures/chemdraw-arrows/numeric-before.cdxml"
    ))?;
    assert_eq!(
        before
            .descendants()
            .filter(|n| n.has_tag_name("arrow"))
            .count(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn native_fill_codes_preserve_unfilled_arrows_and_reject_unsupported_fills()
-> anyhow::Result<()> {
    let engine = LocalEngine::default();
    for (fill, code) in [
        ("Unspecified", 0u8),
        ("None", 1),
        ("Solid", 2),
        ("Shaded", 4),
    ] {
        let xml = format!(
            r#"<CDXML><page id="1"><arrow id="2" Tail3D="10 20 0" Head3D="70 20 0" ArrowheadHead="Full" ArrowheadType="Solid" FillType="{fill}"/></page></CDXML>"#
        );
        let binary = to_cdx(&xml).map_err(anyhow::Error::msg)?;
        assert!(binary.windows(6).any(|p| p == [0x37, 0x0a, 2, 0, code, 0]));
        let decoded = from_cdx(&binary).map_err(anyhow::Error::msg)?;
        let tree = presentation::parse(&decoded)?;
        let arrow = tree
            .descendants()
            .find(|n| n.has_tag_name("arrow"))
            .context("Lost arrow")?;
        assert_eq!(arrow.attribute("FillType"), Some(fill));
        let result = engine
            .request(Request::import("cdx", &STANDARD.encode(binary)))
            .await;
        if code <= 1 {
            assert_eq!(
                result
                    .map_err(anyhow::Error::msg)?
                    .document
                    .context("Missing document")?
                    .arrows
                    .len(),
                1
            );
        } else {
            assert!(result.unwrap_err().contains("Filled or faded CDXML arrows"));
        }
        if code == 0 {
            let mut legacy = to_cdx(&xml).map_err(anyhow::Error::msg)?;
            legacy[34] = 0x27;
            let result = engine
                .request(Request::import("cdx", &STANDARD.encode(legacy)))
                .await
                .map_err(anyhow::Error::msg)?;
            assert_eq!(
                result
                    .document
                    .context("Missing legacy drawing")?
                    .arrows
                    .len(),
                1
            );
        }
    }
    let faded = r#"<CDXML><page id="1"><arrow id="2" Tail3D="10 20 0" Head3D="70 20 0" ArrowheadHead="Full" FillType="None" FadePercent="90"/></page></CDXML>"#;
    assert!(
        engine
            .request(Request::import("cdxml", faded))
            .await
            .unwrap_err()
            .contains("Filled or faded CDXML arrows")
    );
    Ok(())
}

#[tokio::test]
async fn actual_chemdraw_reaction_imports_complete_graph_and_both_arrows() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    let binary = STANDARD.encode(include_bytes!("fixtures/chemdraw-arrows/reaction.cdx"));
    for (format, input) in [
        ("cdx", binary.as_str()),
        (
            "cdxml",
            include_str!("fixtures/chemdraw-arrows/reaction.cdxml"),
        ),
    ] {
        let response = engine
            .request(Request::import(format, input))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = response.document.context("Missing reaction drawing")?;
        assert_eq!(
            (doc.atoms.len(), doc.bonds.len(), doc.arrows.len()),
            (9, 6, 2)
        );
        assert!(doc.graphics.is_empty());
        assert!(doc.annotations.is_empty());
        let analysis = response.analysis.context("Missing chemistry analysis")?;
        assert_eq!(analysis.smiles, "CCN.CCN.CCO");
        assert_eq!(analysis.formula, "C6H20N2O");
        assert_eq!(analysis.inchikey, "VOQVNJWELYGUNW-UHFFFAOYSA-N");
        for arrow in &doc.arrows {
            assert_eq!(arrow.kind, "forward");
            let style = arrow.appearance();
            assert_eq!(style.head, Head::Full);
            assert_eq!(style.head_length_pt, 6.);
            assert_eq!(style.head_width_pt, 1.5);
            assert!((style.width_pt - 0.6).abs() < 0.00002);
            assert!((arrow.end.x - arrow.start.x - 84.).abs() < 0.03);
            assert!((arrow.end.y - arrow.start.y).abs() < 0.0001);
        }
        // The codec retains scheme references. The existing native scene
        // importer does not restore external reaction roles; do not infer them.
        assert!(doc.reactions.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn actual_mixed_drawing_imports_scheme_with_explicit_metadata_warning() -> anyhow::Result<()>
{
    for xml in [
        include_str!("fixtures/chemdraw-arrows/numeric.cdxml"),
        include_str!("fixtures/chemdraw-arrows/numeric-final.cdxml"),
    ] {
        let response = LocalEngine::default()
            .request(Request::import("cdxml", xml))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = response.document.context("Missing drawing")?;
        assert_eq!(
            (
                doc.atoms.len(),
                doc.bonds.len(),
                doc.arrows.len(),
                doc.annotations.len(),
                doc.graphics.len(),
                doc.groups.len()
            ),
            (7, 6, 1, 1, 1, 1)
        );
        assert_eq!(doc.groups[0].members.len(), 9);
        assert_eq!(doc.annotations[0].text, "Fixed text");
        assert!(doc.reactions.is_empty());
        assert_eq!(response.warnings.len(), 1);
        assert!(
            response.warnings[0]
                .contains("external reaction roles and condition references are not retained")
        );
        assert_eq!(
            response.analysis.context("Missing identity")?.inchikey,
            "IZWQYPFXJMTLHZ-UHFFFAOYSA-N"
        );
    }
    Ok(())
}

#[tokio::test]
async fn scheme_fields_require_appropriate_drawing_targets() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    let original = include_str!("fixtures/chemdraw-arrows/numeric.cdxml");
    // These IDs come from the immutable ChemDraw capture: atom 3, bond 10,
    // fragment 2, caption 16, legacy arrow 17, closed path 18, group 20,
    // modern arrow 23. Font 20 uses the same ID in its separate namespace.
    for fields in [
        r#"ReactionStepArrows="3""#,
        r#"ReactionStepArrows="10""#,
        r#"ReactionStepArrows="16""#,
        r#"ReactionStepArrows="18""#,
        r#"ReactionStepArrows="20""#,
        r#"ReactionStepReactants="3""#,
        r#"ReactionStepProducts="23""#,
        r#"ReactionStepPlusses="17""#,
        r#"ReactionStepPlusses="16""#,
        r#"ReactionStepAtomMap="3 23""#,
        r#"ReactionStepAtomMapManual="2 19""#,
        r#"ReactionStepAtomMapAuto="16 20""#,
        r#"ReactionStepObjectsAboveArrow="3""#,
        r#"ReactionStepObjectsAboveArrow="10""#,
    ] {
        let xml = original.replace(r#"ReactionStepArrows="17""#, fields);
        let error = engine
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap_err();
        assert!(
            error.contains("reaction scheme references"),
            "{fields}: {error}"
        );
    }
    for (from, to) in [
        (r#"GraphicType="Line""#, r#"GraphicType="Rectangle""#),
        (r#"ArrowType="FullHead""#, r#"ArrowType="Unknown""#),
        (r#"SupersededBy="23""#, r#"SupersededBy="3""#),
        (r#"SupersededBy="23""#, r#"SupersededBy="17""#),
    ] {
        let error = engine
            .request(Request::import("cdxml", &original.replace(from, to)))
            .await
            .unwrap_err();
        assert!(
            error.contains("reaction scheme references"),
            "{to}: {error}"
        );
    }
    let fields = r#"ReactionStepArrows="17 23" ReactionStepReactants="2 20 16" ReactionStepProducts="19" ReactionStepAtomMap="3 9" ReactionStepAtomMapManual="3 9" ReactionStepAtomMapAuto="3 9" ReactionStepObjectsAboveArrow="2 16 18 20""#;
    let positive = original.replace(r#"ReactionStepArrows="17""#, fields);
    let plus_caption = positive
        .replace("Fixed text", "+")
        .replace("<step", "<step ReactionStepPlusses=\"16\"");
    let plus_symbol = positive.replace("<scheme", "<graphic id=\"50\" GraphicType=\"Symbol\" SymbolType=\"Plus\" BoundingBox=\"140 20 145 25\"/><scheme")
        .replace("<step", "<step ReactionStepPlusses=\"50\"");
    for xml in [positive, plus_caption, plus_symbol] {
        let response = engine
            .request(Request::import("cdxml", &xml))
            .await
            .map_err(anyhow::Error::msg)?;
        let document = response.document.context("Missing drawing")?;
        assert_eq!(
            (
                document.atoms.len(),
                document.bonds.len(),
                document.arrows.len()
            ),
            (7, 6, 1)
        );
        assert!(document.reactions.is_empty());
        assert_eq!(response.warnings.len(), 1);
    }
    // The vendor Arrow_Type definition includes NoHead and treats an absent
    // property as headless. The existing native reader keeps these as editable
    // line graphics, so metadata references must not require a visible head.
    for style in [r#"ArrowType="NoHead""#, ""] {
        let xml = original.replace(r#"ReactionStepArrows="17""#, r#"ReactionStepArrows="50""#)
            .replace("<scheme", &format!(r#"<graphic id="50" GraphicType="Line" {style} BoundingBox="140 20 165 20"/><scheme"#));
        let response = engine
            .request(Request::import("cdxml", &xml))
            .await
            .map_err(anyhow::Error::msg)?;
        let document = response.document.context("Missing headless drawing")?;
        assert_eq!(
            (
                document.atoms.len(),
                document.arrows.len(),
                document.graphics.len()
            ),
            (7, 1, 2)
        );
        assert!(document.reactions.is_empty());
        assert_eq!(response.warnings.len(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn malformed_scheme_metadata_is_rejected_without_partial_drawing() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    for body in [
        r#"<scheme id="2"><step id="3" ReactionStepArrows="99"/></scheme>"#,
        r#"<scheme id="2"><step id="3" Unknown="1"/></scheme>"#,
        r#"<scheme id="2"><n id="3" p="0 0" Element="6"/></scheme>"#,
        r#"<step id="3" ReactionStepArrows="4"/>"#,
        r#"<scheme id="2"><step id="3" ReactionStepAtomMap="4"/></scheme>"#,
        r#"<scheme id="2"><step id="3" ReactionStepArrows="0"/></scheme>"#,
        r#"<scheme id="2"><step id="3" ReactionStepArrows="-1"/></scheme>"#,
        r#"<scheme id="2"><step id="3" ReactionStepArrows="4294967296"/></scheme>"#,
        r#"<scheme id="2"><step id="3"><unsupported/></step></scheme>"#,
        r#"<scheme><step id="3"/></scheme>"#,
        r#"<scheme id="2"><step id="4"/></scheme>"#,
    ] {
        let xml = format!(
            r#"<CDXML><page id="1"><arrow id="4" Tail3D="0 0 0" Head3D="42 0 0" ArrowheadHead="Full"/>{body}</page></CDXML>"#
        );
        let error = engine
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap_err();
        assert!(
            error.contains("reaction scheme references"),
            "{body}: {error}"
        );
    }
    let xml = format!(
        r#"<CDXML><page id="1"><arrow id="4" Tail3D="0 0 0" Head3D="42 0 0" ArrowheadHead="Full"/><scheme id="2"><step id="3" ReactionStepArrows="{}"/></scheme></page></CDXML>"#,
        "4 ".repeat(100_001)
    );
    assert!(
        engine
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap_err()
            .contains("limit")
    );
    Ok(())
}
