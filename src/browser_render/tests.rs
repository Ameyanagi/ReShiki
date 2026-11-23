use super::*;
use crate::document::{Annotation, Point};

fn drawing() -> Document {
    Document {
        annotations: vec![Annotation {
            id: 1,
            position: Point::default(),
            text: "O₃ label".into(),
            format: Default::default(),
            angle: 0.,
        }],
        ..Document::default()
    }
}

fn parity_drawing(format: &str) -> Document {
    if format != "pdf" {
        return drawing();
    }
    // svg2pdf orders multiple Form resources through a HashMap. A single-form
    // geometry fixture keeps full PDF byte parity deterministic; the other
    // formats retain the mixed-style text fixture above.
    use crate::graphics::{Graphic, GraphicKind};
    Document {
        graphics: vec![Graphic::dragged(
            1,
            GraphicKind::Rectangle,
            Point::default(),
            Point::new(84., 42.),
            Default::default(),
            Default::default(),
            false,
        )],
        ..Document::default()
    }
}

fn assert_single_pdf_form(bytes: &[u8]) {
    assert_eq!(
        bytes
            .windows(b"/Subtype /Form".len())
            .filter(|w| *w == b"/Subtype /Form")
            .count(),
        1,
        "PDF byte-parity fixture must have exactly one Form resource"
    );
}

#[tokio::test]
async fn publication_protocol_preserves_existing_bytes_and_details() {
    for format in ["png", "svg", "svg-office", "pdf"] {
        let doc = parity_drawing(format);
        let expected = export::publication(
            &LocalEngine::default(),
            doc.clone(),
            format,
            false,
            true,
            export::Background::Color([250, 248, 244]),
            144,
            1_000_000,
        )
        .await
        .unwrap();
        if format == "pdf" {
            assert_single_pdf_form(&expected.bytes);
        }
        let request = Request::Publication {
            document: Snapshot::owned(doc),
            format: format.into(),
            pages: false,
            editable: true,
            background: Background::Color([250, 248, 244]),
            dpi: 144,
            max_pixels: 1_000_000,
        };
        let bytes = respond(&serde_json::to_vec(&request).unwrap())
            .await
            .unwrap();
        let actual = decode(bytes).unwrap();
        assert_eq!(actual.bytes, expected.bytes, "{format}");
        assert_eq!(actual.details, expected.details, "{format}");
    }
}

#[tokio::test]
async fn save_protocol_preserves_existing_formats_and_operation_errors() {
    for format in ["svg", "png", "pdf", "cdxml", "cdx"] {
        let doc = parity_drawing(format);
        let expected = formats::write(doc.clone(), format).await.unwrap();
        if format == "pdf" {
            assert_single_pdf_form(&expected);
        }
        let request = Request::Save {
            document: Snapshot::owned(doc),
            format: format.into(),
        };
        let actual = decode(
            respond(&serde_json::to_vec(&request).unwrap())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(actual.bytes, expected, "{format}");
    }
    let doc = drawing();
    let expected = formats::write(doc.clone(), "not-a-format")
        .await
        .unwrap_err();
    let request = Request::Save {
        document: Snapshot::owned(doc),
        format: "not-a-format".into(),
    };
    let reply = respond(&serde_json::to_vec(&request).unwrap())
        .await
        .unwrap();
    assert_eq!(decode(reply).err().unwrap(), expected);
}

#[test]
fn font_envelope_keeps_job_bytes_and_rejects_truncated_headers() {
    let request = br#"{"job":"clipboard"}"#;
    let envelope = request_fonts(request).unwrap();
    let (fonts, body) = split(&envelope, MAX_REQUEST_BYTES).unwrap();
    let fonts: Vec<String> = serde_json::from_slice(fonts).unwrap();
    assert!(fonts.len() <= crate::style::font_generation());
    assert_eq!(body, request);
    assert!(install_fonts(&0_u32.to_le_bytes()).is_err());
    assert!(install_fonts(&[0, 0, 0]).is_err());
    assert!(install_fonts(&[255; 4]).is_err());
    assert!(decode(vec![0, 0, 0]).is_err());
    assert!(decode(vec![255; 4]).is_err());
}

fn marked_centroid_drawing(xi: bool) -> (Document, u64, [u64; 2]) {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for (element, position) in [
        ("F", Point::new(-40., 0.)),
        ("Cl", Point::new(25., -35.)),
        ("Br", Point::new(25., 35.)),
    ] {
        let atom = doc.add_atom(element, position);
        doc.add_bond(center, atom, 1, "plain");
    }
    let a = doc.add_atom("C", Point::new(200., 0.));
    let b = doc.add_atom("C", Point::new(242., 0.));
    doc.add_bond(a, b, 2, "plain");
    let f = doc.add_atom("F", Point::new(158., 0.));
    let cl = doc.add_atom("Cl", Point::new(284., 0.));
    doc.add_bond(a, f, 1, "plain");
    doc.add_bond(b, cl, 1, "plain");
    let c = doc.add_atom("C", Point::new(200., 140.));
    let d = doc.add_atom("C", Point::new(242., 140.));
    doc.add_bond(c, d, 1, "plain");
    doc.atom_labels.stereo = true;
    doc.atom_labels.mark_undefined_stereocentres = true;
    doc.atom_labels.undefined_xi = xi;
    crate::atom_labels::refresh::refresh_optional_stereo(&mut doc);
    assert!(doc.atom(center).unwrap().undefined_stereo);
    assert!(doc.bond_between(a, b).unwrap().1.undefined_stereo);
    // Adding a bond invalidates computed labels, so finish every molecular
    // component before refreshing. Adding only the centroid preserves them
    // and makes checked_document retain the incoming visible snapshot.
    crate::projection::add_centroid(&mut doc, &[c, d]).unwrap();
    assert!(doc.atom(center).unwrap().undefined_stereo);
    assert!(doc.bond_between(a, b).unwrap().1.undefined_stereo);
    (doc, center, [c, d])
}

#[tokio::test]
async fn centroid_exports_preserve_refreshed_atom_and_bond_markers() {
    for xi in [false, true] {
        let (doc, _, _) = marked_centroid_drawing(xi);
        assert!(
            crate::atom_labels::indicators(&doc)
                .iter()
                .any(|i| i.text == if xi { "(Ξ)" } else { "(?)" })
        );
        let file: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_ne!(crate::scene::svg(&file), crate::scene::svg(&doc));
        for format in ["png", "svg-office"] {
            let expected = export::publication(
                &LocalEngine::default(),
                doc.clone(),
                format,
                false,
                false,
                export::Background::Transparent,
                96,
                1_000_000,
            )
            .await
            .unwrap();
            let request = Request::Publication {
                document: Snapshot::borrowed(&doc),
                format: format.into(),
                pages: false,
                editable: false,
                background: Background::Transparent,
                dpi: 96,
                max_pixels: 1_000_000,
            };
            let actual = decode(
                respond(&serde_json::to_vec(&request).unwrap())
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(actual.bytes, expected.bytes, "{format}, xi={xi}");
            assert_eq!(actual.details, expected.details);
        }
        let expected = formats::write(doc.clone(), "svg").await.unwrap();
        let request = Request::Save {
            document: Snapshot::borrowed(&doc),
            format: "svg".into(),
        };
        let actual = decode(
            respond(&serde_json::to_vec(&request).unwrap())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(actual.bytes, expected);
    }
}

#[test]
fn shared_snapshot_preserves_every_skipped_visible_field_without_changing_file_json() {
    use crate::models3d::{ForceField, Model3d, Placed};
    let (mut doc, center, members) = marked_centroid_drawing(true);
    doc.atom_mut(center).unwrap().undefined_pseudoasymmetric = true;
    doc.on_other_canvas.insert(center);
    let mut model = Model3d::new(
        &doc,
        vec![
            Placed {
                id: members[0],
                xyz: [0., 0., 0.],
            },
            Placed {
                id: members[1],
                xyz: [1.5, 0., 0.],
            },
        ],
        vec![],
        ForceField::Mmff94s,
    );
    model.morph = Some(0.45);
    model.turning = true;
    doc.models.push(model);
    let doc = doc.current();
    assert!(doc.atom(center).unwrap().undefined_stereo);
    assert!(doc.atom(center).unwrap().undefined_pseudoasymmetric);
    assert!(doc.bonds.iter().any(|b| b.undefined_stereo));
    let file = serde_json::to_value(&doc).unwrap();
    assert!(
        file["atoms"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a.get("undefined_stereo").is_none())
    );
    assert!(file.get("_render_visible").is_none());
    let bytes = serde_json::to_vec(&Snapshot::borrowed(&doc)).unwrap();
    let restored = serde_json::from_slice::<Snapshot<'_>>(&bytes)
        .unwrap()
        .into_document();
    assert!(restored.atom(center).unwrap().undefined_stereo);
    assert!(restored.atom(center).unwrap().undefined_pseudoasymmetric);
    assert_eq!(
        restored
            .bonds
            .iter()
            .map(|b| b.undefined_stereo)
            .collect::<Vec<_>>(),
        doc.bonds
            .iter()
            .map(|b| b.undefined_stereo)
            .collect::<Vec<_>>()
    );
    assert_eq!(restored.on_other_canvas, doc.on_other_canvas);
    assert_eq!(restored.models[0].morph, Some(0.45));
    assert!(restored.models[0].turning);
    assert_eq!(crate::scene::svg(&restored), crate::scene::svg(&doc));
    // An older sender's ordinary file-shaped document remains accepted.
    let legacy = serde_json::from_value::<Snapshot<'_>>(file)
        .unwrap()
        .into_document();
    assert!(!legacy.atom(center).unwrap().undefined_stereo);
}

#[tokio::test]
async fn failed_chemistry_keeps_the_same_visible_snapshot_across_the_worker_protocol() {
    let (mut doc, center, _) = marked_centroid_drawing(true);
    doc.atoms.retain(|atom| atom.centroid.is_empty());
    // A separate invalid molecule does not prevent the editor's per-component
    // refresh from marking the valid molecules. The whole-document exporter
    // then fails analysis and must preserve that genuinely refreshed snapshot.
    let invalid = doc.add_atom("C", Point::new(400., 140.));
    for i in 0..5 {
        let hydrogen = doc.add_atom("H", Point::new(360. + 20. * i as f32, 190.));
        doc.add_bond(invalid, hydrogen, 1, "plain");
    }
    crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default())
        .unwrap()
        .apply(&mut doc);
    assert!(doc.atom(center).unwrap().undefined_stereo);
    assert!(doc.bonds.iter().any(|b| b.undefined_stereo));
    let expected = export::publication(
        &LocalEngine::default(),
        doc.clone(),
        "svg-office",
        false,
        false,
        export::Background::Transparent,
        96,
        1_000_000,
    )
    .await
    .unwrap();
    assert!(
        expected
            .details
            .iter()
            .any(|detail| detail.contains("Drawing preserved; chemistry needs review"))
    );
    let request = Request::Publication {
        document: Snapshot::borrowed(&doc),
        format: "svg-office".into(),
        pages: false,
        editable: false,
        background: Background::Transparent,
        dpi: 96,
        max_pixels: 1_000_000,
    };
    let actual = decode(
        respond(&serde_json::to_vec(&request).unwrap())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual.bytes, expected.bytes);
    assert_eq!(actual.details, expected.details);
}
