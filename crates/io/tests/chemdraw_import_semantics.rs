//! Semantic controls and intended-success regressions for synthetic SDK inputs.
use reshiki_io::{engine::LocalEngine, formats};
use reshiki_model::{document::Document, graphics::GraphicKind, palette::Color};
use std::path::Path;

async fn fixture(name: &str, format: &str) -> Document {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/chemdraw-import-semantics/inputs")
        .join(format!("{name}.{format}"));
    let (document, warnings) = formats::read_with_warnings(
        std::fs::read(&path).unwrap(),
        if format == "cds" { "cdx" } else { format },
        &LocalEngine::in_process(),
    )
    .await
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    if name.starts_with("01-")
        || name.starts_with("02-")
        || name.starts_with("03-")
        || name.starts_with("04-")
        || name.starts_with("05-")
    {
        assert_eq!(warnings.len(), 1, "{}: {warnings:?}", path.display());
        assert!(warnings[0].starts_with("Hidden drawing objects were omitted."));
    } else {
        assert!(warnings.is_empty(), "{}: {warnings:?}", path.display());
    }
    document
}

async fn svg(document: Document) -> String {
    String::from_utf8(
        formats::write_with_engine(document, "svg", &LocalEngine::in_process())
            .await
            .unwrap(),
    )
    .unwrap()
}

fn two_fragments(bond_length: f32) -> String {
    format!(
        r#"<CDXML BondLength="{bond_length}"><page id="1">
        <fragment id="2"><n id="3" Element="6" p="10 20"/>
        <n id="4" Element="8" p="{} 20"/>
        <b id="5" B="3" E="4" Order="1"/></fragment>
        <fragment id="6"><n id="7" Element="6" p="80 20"/>
        <n id="8" Element="7" p="{} 20"/>
        <b id="9" B="7" E="8" Order="1"/></fragment>
        </page></CDXML>"#,
        10. + bond_length,
        80. + bond_length,
    )
}

#[tokio::test]
async fn explicit_oxygen_text_inherits_document_label_typography() {
    for format in ["cdxml", "cdx", "cds"] {
        let document = fixture("08-label-defaults-with-text", format).await;
        let oxygen = document.atoms.iter().find(|a| a.element == "O").unwrap();
        let style = oxygen.text_style.as_ref().unwrap();
        assert_eq!(style.family, "Times New Roman", "{format}");
        assert_eq!(style.size_pt, 18., "{format}");
        assert!(style.bold && style.italic, "{format}: {style:?}");
        let output = svg(document).await;
        assert!(output.contains("font-weight=\"bold\""));
        assert!(output.contains("font-style=\"italic\""));
    }
}

#[tokio::test]
async fn unstyled_caption_inherits_document_font_size_and_face() {
    for format in ["cdxml", "cdx", "cds"] {
        let document = fixture("06-caption-defaults", format).await;
        assert_eq!(document.annotations.len(), 1);
        let caption = document.annotations.first().unwrap();
        assert_eq!(caption.text, "Default caption");
        let style = &caption.format.style;
        assert_eq!(style.family, "Times New Roman", "{format}");
        assert_eq!(style.size_pt, 18., "{format}");
        assert!(style.bold && style.italic, "{format}: {style:?}");
    }
}

#[tokio::test]
async fn ordinary_visible_graphics_keep_colors_and_document_or_object_stroke_widths() {
    for format in ["cdxml", "cdx", "cds"] {
        let document = fixture("09-document-line-width", format).await;
        assert_eq!(document.graphics.len(), 2);
        for (graphic, (color, width)) in document.graphics.iter().zip([
            (Color::Custom([255, 0, 0]), 2.),
            (Color::Custom([0, 0, 255]), 0.6),
        ]) {
            assert_eq!(graphic.kind, GraphicKind::Rectangle, "{format}");
            assert_eq!(graphic.style.stroke, color, "{format}");
            assert!((graphic.style.width_pt - width).abs() < 0.0001, "{format}");
        }
        let output = svg(document).await;
        assert!(output.contains("rgb(255,0,0)"));
        assert!(output.contains("rgb(0,0,255)"));
    }
}

#[tokio::test]
async fn two_fragments_at_default_14_4pt_scale_import_together() {
    let (document, warnings) = formats::read_with_warnings(
        two_fragments(14.4).into_bytes(),
        "cdxml",
        &LocalEngine::in_process(),
    )
    .await
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(document.atoms.len(), 4);
    assert_eq!(document.bonds.len(), 2);
    for bond in &document.bonds {
        let a = document
            .atoms
            .iter()
            .find(|atom| atom.id == bond.a)
            .unwrap();
        let b = document
            .atoms
            .iter()
            .find(|atom| atom.id == bond.b)
            .unwrap();
        assert!((a.position.distance(b.position) - 42.).abs() < 0.001);
    }
}

#[tokio::test]
async fn malformed_xml_and_truncated_binary_are_rejected() {
    for (format, bytes) in [
        ("cdxml", b"<CDXML><page>".to_vec()),
        ("cdx", b"VjCD0100\x04\x03\x02\x01".to_vec()),
    ] {
        assert!(
            formats::read_with_warnings(bytes, format, &LocalEngine::in_process())
                .await
                .is_err(),
            "{format}"
        );
    }
}

#[tokio::test]
async fn hidden_drawing_objects_stay_absent_from_visible_figure_output() {
    for name in [
        "01-hidden-graphic",
        "02-hidden-caption",
        "03-hidden-arrow",
        "04-hidden-modern-spline",
        "05-grouped-hidden-children",
    ] {
        for format in ["cdxml", "cdx", "cds"] {
            let output = svg(fixture(name, format).await).await;
            assert!(
                !output.contains("rgb(255,0,0)") && !output.contains("HIDDEN"),
                "{name}.{format}: hidden source content became visible: {output}"
            );
        }
    }
}

#[tokio::test]
async fn oxygen_without_text_child_inherits_document_bold_italic_label_face() {
    for format in ["cdxml", "cdx", "cds"] {
        let document = fixture("07-label-defaults-no-text", format).await;
        let oxygen = document.atoms.iter().find(|a| a.element == "O").unwrap();
        let style = oxygen
            .text_style
            .clone()
            .unwrap_or_else(|| document.drawing_style.text_style());
        assert_eq!(style.family, "Times New Roman", "{format}");
        assert_eq!(style.size_pt, 18., "{format}");
        assert!(style.bold && style.italic, "{format}: {style:?}");
    }
}

#[tokio::test]
async fn two_fragments_with_nondefault_24pt_bonds_import_together() {
    let (document, warnings) = formats::read_with_warnings(
        two_fragments(24.).into_bytes(),
        "cdxml",
        &LocalEngine::in_process(),
    )
    .await
    .expect("valid two-fragment CDXML should retain its 24pt drawing scale");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(document.atoms.len(), 4);
    assert_eq!(document.bonds.len(), 2);
}

#[tokio::test]
async fn imported_polymer_repeat_exports_as_an_editable_drawing() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/chemdraw-import-corpus/supported/041-polymer-repeat-1.cdxml");
    let engine = LocalEngine::in_process();
    let document = formats::read_with_engine(std::fs::read(path).unwrap(), "cdxml", &engine)
        .await
        .unwrap();
    assert_eq!(document.generic.repeats.len(), 1);
    for format in ["cdxml", "cdx"] {
        let output = formats::write_with_engine(document.clone(), format, &engine)
            .await
            .expect("editable drawing export should preserve an imported repeating unit");
        let reopened = formats::read_with_engine(output, format, &engine)
            .await
            .unwrap();
        assert_eq!(reopened.generic.repeats.len(), 1);
    }
}
