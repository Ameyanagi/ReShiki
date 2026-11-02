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

fn assert_drawing_only(outcome: &CopyOutcome, representations: &[Representation]) {
    assert!(representations.iter().all(|r| !matches!(
        r.kind.as_str(),
        "public.utf8-plain-text" | "org.opensmiles.smiles"
    )));
    assert!(
        outcome
            .notices
            .iter()
            .all(|notice| !notice.starts_with("Chemical text"))
    );
}

#[tokio::test]
async fn ordinary_copy_keeps_drawing_formats_in_order_without_chemical_text() {
    for arrow in [false, true] {
        let source = rings(arrow);
        let before = source.clone();
        let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
            .await
            .unwrap();
        assert!(outcome.external_editable);
        assert_drawing_only(&outcome, &representations);
        let mut expected = vec![NATIVE];
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
    assert_drawing_only(&outcome, &representations);
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
    let (outcome, representations) = prepare_copy(Default::default(), snapshot, false)
        .await
        .unwrap();
    assert!(outcome.external_editable);
    assert_drawing_only(&outcome, &representations);
    assert!(outcome.notices.iter().all(|n| !n.contains("participant")));
    let native: Document =
        serde_json::from_slice(&representations.first().unwrap().bytes().unwrap()).unwrap();
    assert_eq!((native.atoms.len(), native.bonds.len()), (12, 12));
}

#[tokio::test]
async fn image_only_copy_has_no_chemical_text_and_retains_picture_formats() {
    let (outcome, representations) = prepare_copy(Default::default(), rings(false), true)
        .await
        .unwrap();
    assert_drawing_only(&outcome, &representations);
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
    let picture: Document = serde_json::from_slice(
        &representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .unwrap()
            .bytes()
            .unwrap(),
    )
    .unwrap();
    assert!(picture.atoms.is_empty());
    assert_eq!(picture.graphics.len(), 1);
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
    // Copy image: the picture alone, which pastes back as a picture.
    let (_, representations) = prepare_copy_for(Default::default(), rings(false), true, true)
        .await
        .unwrap();
    assert_eq!(
        kinds(&representations),
        ["public.png", "public.svg-image", NATIVE]
    );
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
