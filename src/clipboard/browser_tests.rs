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

fn text(representations: &[Representation]) -> Option<String> {
    representations
        .iter()
        .find(|r| r.kind == "public.utf8-plain-text")
        .map(|r| String::from_utf8(r.bytes().unwrap()).unwrap())
}

#[tokio::test]
async fn ordinary_copy_adds_smiles_for_every_disconnected_molecule() {
    let source = rings(false);
    let before = source.clone();
    let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
        .await
        .unwrap();
    assert_eq!(outcome.chemical_format, Some(CopyFormat::Smiles));
    let smiles = text(&representations).unwrap();
    assert_eq!(smiles.split('.').count(), 2);
    let imported = LocalEngine::default()
        .request(Request::import_smiles(&smiles))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!((imported.atoms.len(), imported.bonds.len()), (12, 12));
    assert!(source.reactions.is_empty());
    assert_eq!(source, before);
    for kind in [
        NATIVE,
        CDX_TYPES[0],
        "public.png",
        "public.svg-image",
        "com.adobe.pdf",
    ] {
        assert!(representations.iter().any(|r| r.kind == kind), "{kind}");
    }
}

#[tokio::test]
async fn ordinary_copy_infers_aromatic_reaction_text_only() {
    let source = rings(true);
    let before = source.clone();
    let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
        .await
        .unwrap();
    assert_eq!(
        outcome.chemical_format,
        Some(CopyFormat::ChemDoodleReaction),
        "{:?}",
        outcome.notices
    );
    let json: serde_json::Value = serde_json::from_str(&text(&representations).unwrap()).unwrap();
    assert_eq!(json["m"].as_array().unwrap().len(), 2);
    assert_eq!(json["m"][0]["a"].as_array().unwrap().len(), 6);
    assert_eq!(json["m"][1]["a"].as_array().unwrap().len(), 6);
    assert_eq!(
        json["m"][0]["b"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["o"] == 2)
            .count(),
        3
    );
    assert_eq!(json["s"][0]["rs"].as_array().unwrap().len(), 1);
    assert_eq!(json["s"][0]["ps"].as_array().unwrap().len(), 1);
    let native: Document = serde_json::from_slice(
        &representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .unwrap()
            .bytes()
            .unwrap(),
    )
    .unwrap();
    assert!(native.reactions.is_empty());
    assert_eq!(native.arrows, source.arrows);
    assert_eq!(source, before);
}

#[tokio::test]
async fn ambiguous_reaction_keeps_drawing_copy_without_molecular_text() {
    let mut source = rings(true);
    for atom in &mut source.atoms[6..] {
        atom.position.x -= 120.;
    }
    let (outcome, representations) = prepare_copy(Default::default(), source, false)
        .await
        .unwrap();
    assert!(outcome.external_editable);
    assert!(outcome.chemical_format.is_none());
    assert!(text(&representations).is_none());
    assert!(outcome.notices.iter().any(|n| n.contains("arrow midpoint")));
    assert!(representations.iter().any(|r| r.kind == NATIVE));
    assert!(representations.iter().any(|r| r.kind == "public.png"));
}

#[tokio::test]
async fn ordinary_copy_does_not_flatten_explicit_roles_when_arrow_is_omitted() {
    let mut source = rings(true);
    source.reactions = vec![
        crate::reactions::copy_reaction(&source, &source)
            .unwrap()
            .unwrap(),
    ];
    let ids: Vec<_> = source.atoms.iter().map(|a| a.id).collect();
    let snapshot = editing::selection(&source, &ids);
    let reaction = crate::reactions::copy_reaction(&source, &snapshot);
    assert!(reaction.is_err());
    let (outcome, representations) =
        prepare_copy_with_reaction(Default::default(), snapshot, false, reaction)
            .await
            .unwrap();
    assert!(outcome.chemical_format.is_none());
    assert!(text(&representations).is_none());
    assert!(outcome.notices.iter().any(|n| n.contains("participant")));
    let native: Document = serde_json::from_slice(
        &representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .unwrap()
            .bytes()
            .unwrap(),
    )
    .unwrap();
    assert_eq!((native.atoms.len(), native.bonds.len()), (12, 12));
}

#[tokio::test]
async fn image_only_copy_has_no_chemical_text() {
    let (outcome, representations) = prepare_copy(Default::default(), rings(false), true)
        .await
        .unwrap();
    assert!(outcome.chemical_format.is_none());
    assert!(text(&representations).is_none());
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

#[test]
fn text_budget_counts_aliases_padding_and_retains_existing_bytes_at_the_boundary() {
    let mut drawing = vec![Representation::new(NATIVE, &[42])];
    drawing.extend(Representation::aliases(
        &CDX_TYPES,
        STANDARD.encode([1, 2, 3]).into(),
    ));
    let original: Vec<_> = drawing
        .iter()
        .map(|r| (r.kind.clone(), r.bytes().unwrap()))
        .collect();
    let text_size = if cfg!(windows) { 8 } else { 3 };
    let mut rejected = drawing.clone();
    assert!(!append_chemical_text(
        &mut rejected,
        "CCO",
        10 + text_size - 1
    ));
    assert_eq!(
        rejected
            .iter()
            .map(|r| (r.kind.clone(), r.bytes().unwrap()))
            .collect::<Vec<_>>(),
        original
    );
    assert!(append_chemical_text(&mut drawing, "CCO", 10 + text_size));
    assert_eq!(text(&drawing).as_deref(), Some("CCO"));
    assert_eq!(
        drawing[..original.len()]
            .iter()
            .map(|r| (r.kind.clone(), r.bytes().unwrap()))
            .collect::<Vec<_>>(),
        original
    );
}

#[test]
fn windows_dib_reservation_uses_header_and_rgb_row_padding() {
    let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    header.extend_from_slice(&5u32.to_be_bytes());
    header.extend_from_slice(&7u32.to_be_bytes());
    let png = Representation::new("public.png", &header);
    assert_eq!(clipboard_dib_size(&png), Some(40 + 16 * 7));
    assert!(clipboard_dib_size(&Representation::new("public.png", b"not PNG")).is_none());
    #[cfg(windows)]
    {
        let mut packet = vec![png];
        let baseline = header.len() + 40 + 16 * 7;
        assert!(!append_chemical_text(&mut packet, "C", baseline + 3));
        assert!(append_chemical_text(&mut packet, "C", baseline + 4));
    }
}

#[test]
fn chemical_snapshot_rejects_pruned_explicit_roles_and_preserves_participant_copy() {
    let mut source = rings(true);
    source.reactions = vec![
        crate::reactions::copy_reaction(&source, &source)
            .unwrap()
            .unwrap(),
    ];
    let selected: Vec<_> = source.atoms[..6]
        .iter()
        .map(|a| a.id)
        .chain([source.arrows[0].id])
        .collect();
    let incomplete = editing::selection(&source, &selected);
    assert!(incomplete.reactions.is_empty());
    assert!(chemical_snapshot(&source, &incomplete).is_err());
    let molecule = editing::selection(&source, &selected[..6]);
    let chemical = chemical_snapshot(&source, &molecule).unwrap();
    assert!(CopyFormat::Smiles.unavailable_reason(&chemical).is_none());
    assert!(matches!(chemical, std::borrow::Cow::Borrowed(_)));
}
