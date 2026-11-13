use super::*;
use crate::document::{Arrow, Point};

fn rings(arrow: bool) -> Document {
    let mut doc = Document::default();
    editing::ring(&mut doc, Point::default(), 6, arrow, 36.);
    editing::ring(&mut doc, Point::new(240., 0.), 6, false, 36.);
    if arrow {
        doc.arrows.push(Arrow::new(
            doc.next_id(),
            Point::new(90., 0.),
            Point::new(150., 0.),
            Default::default(),
            Default::default(),
        ));
    }
    doc
}

fn assert_chemical_text(
    outcome: &CopyOutcome,
    representations: &[Representation],
    smiles: Option<&str>,
) {
    let text: Vec<_> = representations
        .iter()
        .filter(|r| {
            matches!(
                r.kind.as_str(),
                "public.utf8-plain-text" | "org.opensmiles.smiles"
            )
        })
        .collect();
    if let Some(smiles) = smiles {
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].kind, "public.utf8-plain-text");
        assert_eq!(text[0].bytes().unwrap(), smiles.as_bytes());
    } else {
        assert!(text.is_empty());
    }
    assert!(
        outcome
            .notices
            .iter()
            .all(|notice| !notice.starts_with("Chemical text"))
    );
}

#[tokio::test]
async fn ordinary_copy_keeps_drawing_formats_in_order_with_smiles_only_on_linux() {
    for arrow in [false, true] {
        let engine = LocalEngine::default();
        let source = rings(arrow);
        let before = source.clone();
        let smiles = if cfg!(target_os = "linux") && !arrow {
            let (format, snapshot) = chemical_text_snapshot(&source, &source).unwrap();
            assert_eq!(format, CopyFormat::Smiles);
            Some(
                prepare_chemical_text(engine.clone(), snapshot, CopyFormat::Smiles)
                    .await
                    .unwrap()
                    .text()
                    .unwrap()
                    .to_owned(),
            )
        } else {
            None
        };
        let (outcome, representations) = prepare_copy(engine, source.clone(), false).await.unwrap();
        assert!(outcome.external_editable);
        assert_chemical_text(&outcome, &representations, smiles.as_deref());
        let mut expected = vec![NATIVE];
        if smiles.is_some() {
            expected.push("public.utf8-plain-text");
        }
        expected.extend(CDX_TYPES);
        expected.extend(["com.adobe.pdf", "public.png", "public.svg-image"]);
        if cfg!(windows) {
            expected.push("dev.reshiki.office-metafile");
        }
        assert_eq!(
            representations
                .iter()
                .map(|r| r.kind.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        let native: Document =
            serde_json::from_slice(&representations.first().unwrap().bytes().unwrap()).unwrap();
        assert!(native.reactions.is_empty());
        assert_eq!(native.arrows, source.arrows);
        assert_eq!((native.atoms.len(), native.bonds.len()), (12, 12));
        assert_eq!(source, before);
    }
}

#[tokio::test]
async fn ambiguous_reaction_copy_keeps_drawing_without_chemical_notices() {
    let mut source = rings(true);
    for atom in &mut source.atoms[6..] {
        atom.position.x -= 120.;
    }
    assert!(crate::reactions::copy_reaction(&source, &source).is_err());
    let (outcome, representations) = prepare_copy(Default::default(), source, false)
        .await
        .unwrap();
    assert!(outcome.external_editable);
    assert_chemical_text(&outcome, &representations, None);
    assert!(
        outcome
            .notices
            .iter()
            .all(|n| !n.contains("arrow midpoint"))
    );
    assert!(representations.iter().any(|r| r.kind == NATIVE));
    assert!(representations.iter().any(|r| r.kind == "public.png"));
}

#[tokio::test]
async fn ordinary_copy_keeps_atoms_when_explicit_role_selection_omits_arrow() {
    let mut source = rings(true);
    source.reactions = vec![
        crate::reactions::copy_reaction(&source, &source)
            .unwrap()
            .unwrap(),
    ];
    let ids: Vec<_> = source.atoms.iter().map(|a| a.id).collect();
    let snapshot = editing::selection(&source, &ids);
    assert!(crate::reactions::copy_reaction(&source, &snapshot).is_err());
    let smiles = if cfg!(target_os = "linux") {
        Some(
            prepare_chemical_text(Default::default(), snapshot.clone(), CopyFormat::Smiles)
                .await
                .unwrap()
                .text()
                .unwrap()
                .to_owned(),
        )
    } else {
        None
    };
    let (outcome, representations) = prepare_copy(Default::default(), snapshot, false)
        .await
        .unwrap();
    assert!(outcome.external_editable);
    assert_chemical_text(&outcome, &representations, smiles.as_deref());
    assert!(outcome.notices.iter().all(|n| !n.contains("participant")));
    let native: Document =
        serde_json::from_slice(&representations.first().unwrap().bytes().unwrap()).unwrap();
    assert_eq!((native.atoms.len(), native.bonds.len()), (12, 12));
}

#[tokio::test]
async fn ordinary_copy_without_chemistry_has_no_chemical_text() {
    let mut source = Document::default();
    source.graphics.push(crate::graphics::Graphic::dragged(
        source.next_id(),
        crate::graphics::GraphicKind::Rectangle,
        Point::default(),
        Point::new(60., 40.),
        Default::default(),
        Default::default(),
        false,
    ));
    let (outcome, representations) = prepare_copy(Default::default(), source, false)
        .await
        .unwrap();
    assert_chemical_text(&outcome, &representations, None);
    assert!(representations.iter().any(|r| r.kind == NATIVE));
    assert!(representations.iter().any(|r| r.kind == "public.png"));
}

#[tokio::test]
async fn image_only_copy_has_no_chemical_text_and_retains_picture_formats() {
    let (outcome, representations) = prepare_copy(Default::default(), rings(false), true)
        .await
        .unwrap();
    assert_chemical_text(&outcome, &representations, None);
    assert!(outcome.image_only);
    let mut expected = vec!["com.adobe.pdf"];
    expected.extend(CDX_TYPES);
    expected.extend(["public.png", "public.svg-image", NATIVE]);
    assert_eq!(
        representations
            .iter()
            .map(|r| r.kind.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    let drawing: Document = serde_json::from_slice(
        &representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .unwrap()
            .bytes()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(drawing.all_ids(), rings(false).all_ids());
    assert!(drawing.graphics.is_empty());
}

#[tokio::test]
async fn a_browser_copy_has_the_drawing_pictures_and_smiles_but_no_pdf_or_cdx() {
    let kinds = |representations: &[Representation]| {
        representations
            .iter()
            .map(|r| r.kind.clone())
            .collect::<Vec<_>>()
    };
    let (outcome, representations) =
        prepare_copy_for(Default::default(), rings(false), false, true)
            .await
            .unwrap();
    assert!(!outcome.external_editable);
    assert_eq!(
        kinds(&representations),
        [
            NATIVE,
            "public.utf8-plain-text",
            "public.png",
            "public.svg-image"
        ]
    );
    let smiles = String::from_utf8(representations[1].bytes().unwrap()).unwrap();
    assert_eq!(smiles.matches(['c', 'C']).count(), 12, "{smiles}");
    let items = browser::web_items(&representations).unwrap();
    assert_eq!(
        items.iter().map(|(mime, _)| *mime).collect::<Vec<_>>(),
        ["text/html", "image/png", "image/svg+xml", "text/plain"]
    );
    // Copy Image also restores the drawing when pasted back into ReShiki.
    let (_, representations) = prepare_copy_for(Default::default(), rings(false), true, true)
        .await
        .unwrap();
    assert_eq!(
        kinds(&representations),
        ["public.png", "public.svg-image", NATIVE]
    );
}

#[tokio::test]
async fn a_browser_copy_says_when_its_smiles_drops_an_axis() {
    use crate::engine::native_response::SMILES_AXIAL_NOTICE;
    let engine = LocalEngine::default();
    for (annotation, notice) in [(" |wU:0.9|", true), ("", false)] {
        let source = format!("c1(-c2c(Br)cccc2Cl)c([C@H](O)C)cccc1[C@@H](O)C{annotation}");
        let doc = engine
            .request(Request::import_smiles(&source))
            .await
            .unwrap()
            .document
            .unwrap();
        let (outcome, _) = prepare_copy_for(engine.clone(), doc, false, true)
            .await
            .unwrap();
        assert_eq!(
            outcome.notices,
            notice
                .then(|| SMILES_AXIAL_NOTICE.to_owned())
                .into_iter()
                .collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn a_browser_svg_only_pick_restores_the_editable_drawing() {
    let source = rings(false);
    let (_, representations) =
        prepare_copy_for(LocalEngine::in_process(), source.clone(), false, true)
            .await
            .unwrap();
    let svg = representations
        .iter()
        .find(|item| item.kind == "public.svg-image")
        .unwrap()
        .bytes()
        .unwrap();
    let items = vec![("image/svg+xml".into(), svg)];
    let formats: Vec<_> = priority::READABLE
        .iter()
        .map(|kind| (*kind).into())
        .collect();
    let item = browser::pick(&formats, &items).unwrap();
    assert_eq!(item.kind, "public.svg-image");
    let restored = paste_representation(LocalEngine::in_process(), item, false)
        .await
        .unwrap();
    assert!(restored.native);
    assert_eq!(restored.document.all_ids(), source.all_ids());
    assert_eq!(restored.warnings, [RESTORED_DRAWING_NOTICE]);
    let pictures: Vec<_> = priority::PICTURES
        .iter()
        .map(|kind| (*kind).into())
        .collect();
    assert!(browser::pick(&pictures, &items).is_none());
}

#[test]
fn a_browser_name_copy_keeps_html_and_plain_text_without_rtf() {
    let text = "(1R)-N6-methyl";
    let html = r#"<meta charset="utf-8">(1<i>R</i>)-<i>N</i><sup>6</sup>-methyl"#;
    let representations = [
        Representation::new("public.utf8-plain-text", text.as_bytes()),
        Representation::new("public.html", html.as_bytes()),
        Representation::new("public.rtf", br"{\rtf1 unsupported on the web}"),
    ];
    assert_eq!(
        browser::web_items(&representations).unwrap(),
        vec![
            ("text/html", html.as_bytes().to_vec()),
            ("text/plain", text.as_bytes().to_vec()),
        ]
    );
}
