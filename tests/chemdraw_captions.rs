use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::engine::{LocalEngine, Request};
use reshiki::{
    document::{Annotation, Document, Point},
    exchange::{drawing, from_cdx, to_cdx},
    typography::{TextFormat, TextStyle},
};

#[test]
fn explicit_nonchemical_text_survives_binary_encoding() -> anyhow::Result<()> {
    let xml = r#"<CDXML><page id="1"><t id="2" p="0 0" InterpretChemically="no"><s>C18H24Si</s></t></page></CDXML>"#;
    let binary = to_cdx(xml).map_err(anyhow::Error::msg)?;
    // Property 0x0708, one-byte payload, explicit false. Omitting this property
    // changes its meaning: ChemDraw interprets standalone captions chemically.
    assert!(
        binary
            .windows(5)
            .any(|bytes| bytes == [0x08, 0x07, 1, 0, 0])
    );
    let back = from_cdx(&binary).map_err(anyhow::Error::msg)?;
    let tree = roxmltree::Document::parse(&back)?;
    let caption = tree.descendants().find(|n| n.has_tag_name("t")).unwrap();
    assert_eq!(caption.attribute("InterpretChemically"), Some("no"));
    Ok(())
}

#[test]
fn fixed_line_heights_use_twentieth_points_and_preserve_special_values() -> anyhow::Result<()> {
    for (property, code) in [
        ("LineHeight", 0x0702u16),
        ("LabelLineHeight", 0x0706),
        ("CaptionLineHeight", 0x0707),
    ] {
        for (value, encoded, decoded) in [
            ("12", 240u16, "12"),
            ("10.75", 215, "10.75"),
            ("10.8", 216, "10.8"),
            ("0", 0, "variable"),
            ("1", 1, "automatic"),
        ] {
            let xml = format!(
                r#"<CDXML><page id="1"><t id="2" {property}="{value}"><s>caption</s></t></page></CDXML>"#
            );
            let binary = to_cdx(&xml).map_err(anyhow::Error::msg)?;
            let [lo, hi] = code.to_le_bytes();
            let [v0, v1] = encoded.to_le_bytes();
            assert!(
                binary
                    .windows(6)
                    .any(|bytes| bytes == [lo, hi, 2, 0, v0, v1]),
                "{property}/{value}"
            );
            let back = from_cdx(&binary).map_err(anyhow::Error::msg)?;
            let tree = roxmltree::Document::parse(&back)?;
            let caption = tree.descendants().find(|n| n.has_tag_name("t")).unwrap();
            assert_eq!(caption.attribute(property), Some(decoded));
        }
    }
    Ok(())
}

#[test]
fn formula_and_plain_captions_remain_text_in_both_exchange_formats() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    for (index, (text, formula)) in [("C18H24Si", true), ("C", false)].into_iter().enumerate() {
        doc.annotations.push(Annotation {
            id: 10 + index as u64,
            position: Point::new(0., 80. + 60. * index as f32),
            text: text.into(),
            format: TextFormat {
                style: TextStyle {
                    formula,
                    ..Default::default()
                },
                ..Default::default()
            },
        });
    }
    let xml = drawing::write(&doc, Default::default())?;
    let binary = to_cdx(&xml).map_err(anyhow::Error::msg)?;
    let decoded = from_cdx(&binary).map_err(anyhow::Error::msg)?;
    for text in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(text)?;
        let captions: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("t") && n.parent().is_some_and(|p| p.has_tag_name("page")))
            .collect();
        assert_eq!(captions.len(), 2);
        for caption in captions {
            assert_eq!(caption.attribute("InterpretChemically"), Some("no"));
        }
        // Chemical atom labels retain the atom's Element and normal semantics.
        let oxygen = tree
            .descendants()
            .find(|n| n.has_tag_name("n") && n.attribute("Element") == Some("8"))
            .unwrap();
        assert!(
            oxygen
                .descendants()
                .filter(|n| n.has_tag_name("t"))
                .all(|n| n.attribute("InterpretChemically").is_none())
        );
    }
    Ok(())
}

#[tokio::test]
async fn real_chemdraw_captions_keep_graphs_and_text_in_xml_and_binary() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    let binary = STANDARD.encode(include_bytes!("fixtures/chemdraw-captions/gallery.cdx"));
    for (format, input) in [
        (
            "cdxml",
            include_str!("fixtures/chemdraw-captions/gallery.cdxml"),
        ),
        ("cdx", binary.as_str()),
    ] {
        let result = engine
            .request(Request::import(format, input))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = result.document.unwrap();
        assert_eq!(
            (doc.atoms.len(), doc.bonds.len(), doc.annotations.len()),
            (117, 123, 12),
            "{format}"
        );
        assert_eq!(doc.abbreviations.len(), 4);
        assert!(doc.graphics.is_empty());
        assert!(doc.atoms.iter().all(|a| a.element != "*"));
        let analysis = result.analysis.unwrap();
        assert_eq!(analysis.formula, "C108H144O3Si6");
        // Independently calculated from three copies each of the two explicit
        // reference SMILES documented in the fixture provenance.
        assert_eq!(analysis.inchikey, "BZUKRNRSTKNNSD-UHFFFAOYSA-N");
        for group in &doc.abbreviations {
            let oxygen = group.label == "OTBDPS";
            assert!(oxygen || group.label == "TBDPS");
            assert_eq!(group.members.len(), if oxygen { 18 } else { 17 });
            assert_eq!(
                doc.atom(group.anchor).unwrap().element,
                if oxygen { "O" } else { "Si" }
            );
        }
        for caption in &doc.annotations {
            let is_formula = caption.text.starts_with("C18H24");
            let expected_height = if is_formula { 10.75 } else { 12. };
            let height = caption.format.style.size_pt * caption.format.line_spacing;
            assert!(
                (height - expected_height).abs() < 0.001,
                "{format}: {} = {height}",
                caption.text
            );
        }
    }
    // The original malformed interpretation must continue to fail rather than
    // silently discard the chemical objects manufactured from formula captions.
    let before = include_str!("fixtures/chemdraw-captions/gallery-before.cdxml");
    let error = engine
        .request(Request::import("cdxml", before))
        .await
        .unwrap_err();
    assert!(
        error.contains("Molecular objects outside parsed fragments"),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn binary_automatic_and_variable_line_heights_import_as_captions() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    for value in ["0", "1", "variable", "auto", "automatic"] {
        let xml = format!(
            r#"<CDXML><page id="1"><t id="2" p="0 0" CaptionLineHeight="{value}"><s font="3" size="10">caption</s></t></page></CDXML>"#
        );
        let data = to_cdx(&xml).map_err(anyhow::Error::msg)?;
        let decoded = from_cdx(&data).map_err(anyhow::Error::msg)?;
        assert_eq!(to_cdx(&decoded).map_err(anyhow::Error::msg)?, data);
        let result = engine
            .request(Request::import("cdx", &STANDARD.encode(data)))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = result.document.unwrap();
        assert_eq!(doc.annotations.len(), 1);
        assert_eq!(doc.annotations[0].text, "caption");
        assert_eq!(doc.annotations[0].format.line_spacing, 1.2);
    }
    Ok(())
}
