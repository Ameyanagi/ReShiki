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
