use super::*;
use crate::{
    canvas_theme::CanvasTheme,
    document::{Annotation, Point},
    engine::LocalEngine,
};

#[test]
fn clipboard_request_limit_falls_back_only_for_pre_dispatch_size_admission() {
    for bytes in [0, REQUEST_BYTES - 1, REQUEST_BYTES] {
        assert_eq!(
            worker_request(request_length(bytes, REQUEST_BYTES)),
            Ok(Some(()))
        );
    }
    for bytes in [REQUEST_BYTES + 1, usize::MAX] {
        assert_eq!(
            worker_request(request_length(bytes, REQUEST_BYTES)),
            Ok(None)
        );
    }
    let error = "Browser clipboard request exceeds 128 MiB";
    assert_eq!(
        worker_request::<()>(Err(RequestError::Invalid(error.into()))),
        Err(error.into()),
        "a serialization error must not become fallback based on its wording"
    );
}

#[test]
fn clipboard_request_encoding_preserves_local_fallback_for_every_operation() {
    let document = drawing();
    let pictures = Pictures::default();
    for operation in [
        Operation::Copy { image_only: false },
        Operation::Copy { image_only: true },
        Operation::CopyAs {
            format: CopyFormat::Svg,
        },
        Operation::ChemicalText {
            format: CopyFormat::Smiles,
        },
    ] {
        let bytes = request_operation(&document, operation, pictures).unwrap();
        assert!(!bytes.is_empty() && bytes.len() < 1024 * 1024);
        assert_eq!(
            worker_request(request_operation_with_limit(
                &document,
                operation,
                pictures,
                bytes.len(),
            )),
            Ok(Some(bytes.clone()))
        );
        assert_eq!(
            worker_request(request_operation_with_limit(
                &document,
                operation,
                pictures,
                bytes.len() - 1,
            )),
            Ok(None),
            "the caller must retain its local preparation path before dispatch"
        );
    }
}

fn drawing() -> Document {
    let mut doc = Document {
        annotations: vec![Annotation {
            id: 1,
            position: Point::default(),
            text: "O₃ label".into(),
            format: Default::default(),
            angle: 0.,
        }],
        recent_colors: vec![[1, 2, 3]],
        ..Document::default()
    };
    let mut theme = crate::theme_files::bundled().unwrap().remove(0);
    theme.author = "Private author".into();
    theme.source = "Private source".into();
    theme.license = "Private license".into();
    doc.custom_theme = Some(Box::new(theme));
    doc
}

async fn pair(doc: Document, image_only: bool, pictures: Pictures) -> (Prepared, Prepared) {
    let expected = prepare_copy_with_limit(
        LocalEngine::default(),
        doc.clone(),
        image_only,
        true,
        crate::payload::EMBED_LIMIT,
        pictures,
    )
    .await
    .unwrap();
    let bytes = request(&doc, image_only, pictures).unwrap();
    let actual = copy_result(decode(&respond(&bytes).await.unwrap()).unwrap()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        super::super::browser::web_items(&actual.1).unwrap(),
        super::super::browser::web_items(&expected.1).unwrap()
    );
    (expected, actual)
}

#[tokio::test]
async fn browser_clipboard_protocol_preserves_flavors_order_options_and_provenance() {
    for theme in [CanvasTheme::Light, CanvasTheme::Dark] {
        for image_only in [false, true] {
            for editable in [false, true] {
                let mut doc = drawing();
                doc.canvas_theme = theme;
                let pictures = Pictures {
                    background: export::Background::Color([250, 248, 244]),
                    dpi: 144,
                    editable,
                };
                let (_, actual) = pair(doc, image_only, pictures).await;
                let web = super::super::browser::web_items(&actual.1).unwrap();
                assert_eq!(
                    web.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
                    ["text/html", "image/png", "image/svg+xml"]
                );
                for r in actual
                    .1
                    .iter()
                    .filter(|r| matches!(r.kind.as_str(), "public.png" | "public.svg-image"))
                {
                    let restored = crate::payload::read(&r.bytes().unwrap());
                    if editable {
                        let restored = restored.unwrap().unwrap();
                        assert!(restored.recent_colors.is_empty());
                        let theme = restored.custom_theme.unwrap();
                        assert!(
                            theme.author.is_empty()
                                && theme.source.is_empty()
                                && theme.license.is_empty()
                        );
                    } else {
                        assert!(restored.is_none());
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn browser_clipboard_protocol_keeps_partial_picture_failures_and_native_equipment() {
    let mut far = drawing();
    let mut distant = far.annotations[0].clone();
    distant.id = 2;
    distant.position = Point::new(200_000., 200_000.);
    far.annotations.push(distant);
    let (_, actual) = pair(far, false, Pictures::default()).await;
    assert!(!actual.1.iter().any(|r| r.kind == "public.png"));
    assert!(actual.1.iter().any(|r| r.kind == "public.svg-image"));
    assert!(
        actual
            .0
            .notices
            .iter()
            .any(|notice| notice.starts_with("PNG unavailable:"))
    );

    let mut equipment = Document::default();
    let scale = crate::equipment::placed::world_per_mm(&equipment, 0.4);
    equipment.graphics.push(crate::equipment::placed::graphic(
        1,
        crate::equipment::builtin::find("reshiki/compression-tube-60-1-4").unwrap(),
        Point::default(),
        scale,
    ));
    pair(
        equipment,
        false,
        Pictures {
            dpi: 144,
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn browser_clipboard_protocol_preserves_transient_undefined_stereo_markers() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("N", Point::new(42., 0.));
    doc.add_bond(a, b, 2, "plain");
    crate::projection::add_centroid(&mut doc, &[a, b]).unwrap();
    doc.atom_labels.stereo = true;
    doc.atom_labels.mark_undefined_stereocentres = true;
    doc.atoms[0].undefined_stereo = true;
    doc.atoms[1].undefined_stereo = true;
    doc.atoms[1].undefined_pseudoasymmetric = true;
    doc.bonds[0].undefined_stereo = true;
    let pictures = Pictures {
        dpi: 144,
        ..Default::default()
    };
    for xi in [false, true] {
        doc.atom_labels.undefined_xi = xi;
        let (expected, _) = pair(doc.clone(), false, pictures).await;
        let mut without = doc.clone();
        for a in &mut without.atoms {
            a.undefined_stereo = false;
            a.undefined_pseudoasymmetric = false;
        }
        for b in &mut without.bonds {
            b.undefined_stereo = false;
        }
        let plain = prepare_copy_with_limit(
            LocalEngine::default(),
            without,
            false,
            true,
            crate::payload::EMBED_LIMIT,
            pictures,
        )
        .await
        .unwrap();
        let svg = |prepared: Prepared| {
            prepared
                .1
                .into_iter()
                .find(|r| r.kind == "public.svg-image")
                .unwrap()
                .data
        };
        assert_ne!(
            svg(expected),
            svg(plain),
            "fixture must visibly exercise the transient marker"
        );
    }
}

#[tokio::test]
async fn browser_copy_as_protocol_preserves_prepared_formats_and_failures() {
    for editable in [false, true] {
        for format in [CopyFormat::Png, CopyFormat::Svg, CopyFormat::Cdxml] {
            let doc = drawing();
            let pictures = Pictures {
                dpi: 144,
                editable,
                background: export::Background::Color([250, 248, 244]),
            };
            let expected = super::super::copy_as::prepare_as_local(
                LocalEngine::default(),
                doc.clone(),
                format,
                pictures,
            )
            .await;
            let bytes = request_operation(&doc, Operation::CopyAs { format }, pictures).unwrap();
            let actual = decode(&respond(&bytes).await.unwrap()).and_then(as_result);
            assert_eq!(actual, expected, "{format:?}, editable={editable}");
        }
    }

    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    for format in [
        CopyFormat::Mol,
        CopyFormat::Smiles,
        CopyFormat::ReactionSmiles,
    ] {
        let expected = super::super::copy_as::prepare_as_local(
            LocalEngine::default(),
            doc.clone(),
            format,
            Pictures::default(),
        )
        .await;
        let bytes =
            request_operation(&doc, Operation::CopyAs { format }, Pictures::default()).unwrap();
        let actual = decode(&respond(&bytes).await.unwrap()).and_then(as_result);
        assert_eq!(actual, expected, "{format:?}");
    }
    let expected = super::super::copy_as::prepare_chemical_text_local(
        LocalEngine::default(),
        doc.clone(),
        CopyFormat::Smiles,
    )
    .await;
    let bytes = request_operation(
        &doc,
        Operation::ChemicalText {
            format: CopyFormat::Smiles,
        },
        Pictures::default(),
    )
    .unwrap();
    assert_eq!(
        decode(&respond(&bytes).await.unwrap()).and_then(as_result),
        expected
    );
}

#[tokio::test]
async fn clipboard_operation_errors_are_normal_replies_and_bad_versions_are_rejected() {
    let bytes = request(&Document::default(), false, Pictures::default()).unwrap();
    let reply = respond(&bytes).await.unwrap();
    assert_eq!(decode(&reply).unwrap_err(), "There is nothing to copy");
    assert!(decode(&respond(b"invalid request").await.unwrap()).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["version"] = serde_json::json!(VERSION + 1);
    let reply = respond(&serde_json::to_vec(&value).unwrap()).await.unwrap();
    assert_eq!(
        decode(&reply).unwrap_err(),
        "Unsupported browser clipboard request version"
    );
    let mut value: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    value["version"] = serde_json::json!(VERSION + 1);
    assert_eq!(
        decode(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        "Unsupported browser clipboard response version"
    );
}
