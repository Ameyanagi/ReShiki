use reshiki::{
    document::{Annotation, Document, Point},
    typography::{self, Script, StyleChange, TextAlign, TextFormat, TextSpan, TextStyle},
};

#[test]
fn automatic_formula_recognition_excludes_captions_units_and_malformed_input() {
    for formula in [
        "C2H2",
        "H2O",
        "C2H5OH",
        "Ca(OH)2",
        "Fe2(SO4)3",
        "[CH2]6",
        "C60",
    ] {
        assert!(typography::is_formula(formula), "{formula}");
    }
    for caption in [
        "",
        "2026",
        "Figure 2",
        "Sample C2H2",
        "25 C",
        "H2O 10 mL",
        "NH4+",
        "Cu2+",
        "Xx2",
        "C(2)",
        "C2()",
        "(CH2",
        "CH0",
        "2H2O",
    ] {
        assert!(!typography::is_formula(caption), "{caption}");
    }
}

#[test]
fn utf8_ranges_survive_insert_replace_and_repeated_character_deletion() {
    let mut f = TextFormat::default();
    f.apply("αAAA", Some(3..4), &StyleChange::Bold(true));
    // Delete the first identical A, preserving bold on the second original A.
    f.edited("αAAA", "αAA", 2..3);
    assert!(f.at(2).bold);
    assert!(!f.at(3).bold);
    f.edited("αAA", "αβAA", 2..2);
    assert!(f.at(2).bold);
    assert!(f.at(4).bold);
    assert!(!f.at(5).bold);
    f.validate("αβAA").unwrap();

    let mut f = TextFormat::default();
    f.apply("AAA", Some(0..1), &StyleChange::Color([180, 50, 55]));
    f.edited("AAA", "AA", 0..2);
    assert_eq!(f.at(0).color, [180, 50, 55]);
    assert_eq!(f.at(1).color, [0, 0, 0]);
}

#[test]
fn formatting_validates_utf8_and_keeps_scripts_exclusive() {
    let mut f = TextFormat::default();
    f.apply("H2O", Some(1..2), &StyleChange::Script(Script::Subscript));
    f.apply("H2O", None, &StyleChange::Formula(true));
    assert!(f.style.formula);
    assert!(f.spans.is_empty());
    f.apply("H2O", Some(1..2), &StyleChange::Script(Script::Superscript));
    assert!(!f.at(1).formula);
    assert_eq!(f.at(1).script, Script::Superscript);
    f.spans = vec![TextSpan {
        start: 1,
        end: 2,
        style: TextStyle::default(),
    }];
    assert!(f.validate("α").is_err());
    f.spans.clear();
    f.style.size_pt = f32::NAN;
    assert!(f.validate("x").is_err());
}

#[test]
fn formula_layout_wraps_and_aligns_without_changing_text() {
    let mut f = TextFormat::default();
    f.apply("2 H2O + NH4+", None, &StyleChange::Formula(true));
    let l = typography::layout("2 H2O + NH4+", &f);
    assert_eq!(
        l.fragments
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>(),
        "2 H2O + NH4+"
    );
    assert_eq!(l.fragments[0].style.script, Script::Normal);
    assert!(
        l.fragments
            .iter()
            .any(|r| r.text == "2" && r.style.script == Script::Subscript)
    );
    assert!(
        l.fragments
            .iter()
            .any(|r| r.text == "+" && r.style.script == Script::Superscript)
    );
    assert!(l.fragments.iter().all(|r| r.position.y >= 0.0));
    f.width_pt = Some(50.0);
    f.alignment = TextAlign::Right;
    let l = typography::layout("alpha beta gamma delta", &f);
    assert!(l.height > f.style.size() * 2.0);
    assert!(l.fragments.last().unwrap().position.x > 0.0);
}

#[test]
fn styled_labels_survive_native_save_and_vector_and_raster_exports() {
    let mut doc = Document::default();
    let mut format = TextFormat::default();
    for change in [
        StyleChange::Family("Helvetica".into()),
        StyleChange::Size(18.0),
        StyleChange::Bold(true),
        StyleChange::Italic(true),
        StyleChange::Underline(true),
        StyleChange::Color([180, 50, 55]),
    ] {
        format.apply("α < H2O", None, &change);
    }
    doc.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "α < H2O".into(),
        format,
    });
    doc.validate().unwrap();
    let roundtrip: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(roundtrip, doc);
    let svg = reshiki::scene::svg(&doc);
    for value in [
        "font-weight=\"bold\"",
        "font-style=\"italic\"",
        "text-decoration=\"underline\"",
        "rgb(180,50,55)",
        "α &lt; H2O",
    ] {
        assert!(svg.contains(value), "{value}");
    }
    // The document preserves Helvetica, while rendering uses installed faces.
    // Helvetica is normally absent on Windows; validate every resolved glyph.
    for glyph in "α < H2O".chars() {
        let (family, _) = reshiki::style::glyph_metrics(glyph, &doc.annotations[0].format.style);
        assert!(svg.contains(&format!("font-family=\"{family}\"")));
    }
    let bytes = reshiki::export::drawing(&doc, "png").unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert!(
        pixels[..info.buffer_size()]
            .chunks_exact(4)
            .filter(|p| i16::from(p[0]) - i16::from(p[1]) > 30
                && i16::from(p[0]) - i16::from(p[2]) > 30)
            .count()
            > 100
    );
    assert!(
        reshiki::export::drawing(&doc, "pdf")
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let old: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki")).unwrap();
    assert_eq!(old.annotations[0].format, TextFormat::default());
}

#[tokio::test]
async fn cdxml_uses_rendered_text_metrics_and_keeps_caption_position() {
    use reshiki::engine::{ChemistryEngine, LocalEngine, Request};
    let engine = LocalEngine::default();
    let mut doc = engine
        .execute(Request::import_smiles("CCO"))
        .await
        .unwrap()
        .document
        .unwrap();
    doc.annotations.push(Annotation {
        id: 10,
        position: Point::new(150.0, 90.0),
        text: "Centered\ncaption".into(),
        format: TextFormat {
            alignment: TextAlign::Center,
            ..Default::default()
        },
    });
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some("cdxml".into());
    let metrics = &request.text_layout.as_ref().unwrap()[&10];
    let expected = typography::layout(&doc.annotations[0].text, &doc.annotations[0].format);
    assert!(
        (metrics.width - expected.width * reshiki::style::DEFAULT.points_per_world()).abs() < 0.001
    );
    let xml = engine.execute(request).await.unwrap().output.unwrap();
    assert!(xml.contains("CaptionJustification=\"Center\""));
    let restored = engine
        .execute(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    let before = doc.annotations[0].position;
    let after = restored.annotations[0].position;
    assert!(
        (before.x - doc.atoms[0].position.x - after.x + restored.atoms[0].position.x).abs() < 0.001
    );
    assert!(
        (before.y - doc.atoms[0].position.y - after.y + restored.atoms[0].position.y).abs() < 0.001
    );
}

#[test]
fn japanese_fallback_uses_real_advances_and_keeps_document_styles() {
    let style = TextStyle::default();
    let (family, advance) = reshiki::style::glyph_metrics('水', &style);
    let (latin_family, _) = reshiki::style::glyph_metrics('H', &style);
    if family == "Arial" {
        return;
    } // Minimal CI images may not install any CJK font.
    assert!(advance > 0.9 && advance < 1.1, "{family}: {advance}");
    let format = TextFormat::default();
    let layout = typography::layout("水素化 H₂O", &format);
    assert_eq!(format.style.family, "Arial");
    assert!(
        layout
            .fragments
            .iter()
            .any(|r| r.style.family == family && r.text.contains("水素化"))
    );
    assert!(
        layout
            .fragments
            .iter()
            // Arial is not installed on every platform. Rendering resolves a
            // real Latin face while the document keeps its requested family.
            .any(|r| r.style.family == latin_family && r.text.contains("H"))
    );
    let measured = reshiki::style::styled_text_width("水素化 H₂O", style.size(), &style);
    assert!((layout.width - measured).abs() < 0.01);
    let mut doc = Document::default();
    doc.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "水素化 H₂O".into(),
        format,
    });
    let svg = reshiki::scene::svg(&doc);
    assert!(svg.contains(&format!("font-family=\"{family}\"")));
    assert!(svg.contains("水素化"));
    assert!(svg.contains("xml:space=\"preserve\""));
    assert_eq!(doc.annotations[0].format.style.family, "Arial");
    let wrapped = typography::layout(
        "水素化反応",
        &TextFormat {
            width_pt: Some(20.),
            ..Default::default()
        },
    );
    assert!(wrapped.height > style.size() * 2.);
}
