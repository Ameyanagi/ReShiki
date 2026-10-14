use super::*;
use crate::{
    document::Point,
    reactions::{Participant, Reaction},
};

fn molecule() -> Document {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(carbon, oxygen, 1, "plain");
    doc
}

fn rings(arrow: bool) -> Document {
    let mut doc = Document::default();
    crate::editing::ring(&mut doc, Point::default(), 6, arrow, 36.);
    crate::editing::ring(&mut doc, Point::new(240., 0.), 6, false, 36.);
    if arrow {
        doc.arrows.push(crate::document::Arrow::new(
            doc.next_id(),
            Point::new(90., 0.),
            Point::new(150., 0.),
            Default::default(),
            Default::default(),
        ));
    }
    doc
}

#[tokio::test]
async fn chemical_text_copies_all_selected_disconnected_molecules_as_smiles() {
    let mut source = rings(false);
    let ids = source.all_ids();
    source.add_atom("N", Point::new(420., 0.));
    let before = source.clone();
    let snapshot = selection_or_drawing(&source, &ids);
    let (format, chemical) = chemical_text_snapshot(&source, &snapshot).unwrap();
    assert_eq!(format, CopyFormat::Smiles);
    assert_eq!(chemical, snapshot);
    let copy = prepare_chemical_text(Default::default(), chemical, format)
        .await
        .unwrap();
    let smiles = copy.text().unwrap();
    assert_eq!(smiles.split('.').count(), 2);
    let imported = LocalEngine::default()
        .request(crate::engine::Request::import_smiles(smiles))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!((imported.atoms.len(), imported.bonds.len()), (12, 12));
    assert_eq!(source, before);
    assert_eq!(
        copy.representations
            .iter()
            .map(|r| r.kind.as_str())
            .collect::<Vec<_>>(),
        ["org.opensmiles.smiles", TEXT]
    );
}

#[tokio::test]
async fn chemical_text_infers_aromatic_reaction_without_changing_drawing_roles() {
    let source = rings(true);
    let before = source.clone();
    let (format, chemical) = chemical_text_snapshot(&source, &source).unwrap();
    assert_eq!(format, CopyFormat::ChemDoodleReaction);
    assert_eq!(chemical.reactions.len(), 1);
    let copy = prepare_chemical_text(Default::default(), chemical, format)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(copy.text().unwrap()).unwrap();
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
    assert_eq!(source, before);
    assert_eq!(copy.representations.len(), 1);
    assert_eq!(copy.representations[0].kind, TEXT);
}

#[tokio::test]
async fn chemical_text_matches_former_refresh_first_conversion() {
    for source in [rings(false), rings(true)] {
        let engine = LocalEngine::default();
        let (format, chemical) = chemical_text_snapshot(&source, &source).unwrap();
        let direct = prepare_chemical_text(engine.clone(), chemical, format)
            .await
            .unwrap();
        let reaction = crate::reactions::copy_reaction(&source, &source).unwrap();
        let mut refreshed = export::checked_document(&engine, source).await.unwrap();
        refreshed.reactions = reaction.into_iter().collect();
        let former = prepare_as(engine, refreshed, format).await.unwrap();
        assert_eq!(direct.text(), former.text());
        assert_eq!(direct.notices, former.notices);
    }
}

#[tokio::test]
async fn chemical_text_refreshes_stale_attached_forward_arrow_like_former_copy() {
    use crate::arrow_anchors::{Anchor, Target};
    let mut source = rings(true);
    let target = source.atoms[0].id;
    source.arrows[0].start_anchor = Some(Anchor {
        target: Target::Atom { atom: target },
        offset: Point::new(75., 0.),
        direction: Point::new(1., 0.),
        gap: 0.,
    });
    let before = source.clone();
    let engine = LocalEngine::default();
    let (format, chemical) = chemical_text_snapshot(&source, &source).unwrap();
    assert_eq!(format, CopyFormat::ChemDoodleReaction);
    let unrefreshed = prepare_as(engine.clone(), chemical.clone(), format)
        .await
        .unwrap();
    let direct = prepare_chemical_text(engine.clone(), chemical, format)
        .await
        .unwrap();
    let reaction = crate::reactions::copy_reaction(&source, &source).unwrap();
    let mut refreshed = export::checked_document(&engine, source.clone())
        .await
        .unwrap();
    assert_ne!(refreshed.arrows[0].start, source.arrows[0].start);
    refreshed.reactions = reaction.into_iter().collect();
    let former = prepare_as(engine, refreshed, format).await.unwrap();
    assert_ne!(direct.text(), unrefreshed.text());
    assert_eq!(direct.text(), former.text());
    assert_eq!(direct.notices, former.notices);
    assert_eq!(source, before);
}

#[tokio::test]
async fn chemical_text_preserves_explicit_source_roles_and_single_participant_copy() {
    let mut source = rings(true);
    let mut reaction = crate::reactions::copy_reaction(&source, &source)
        .unwrap()
        .unwrap();
    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
    source.reactions = vec![reaction.clone()];
    let before = source.clone();
    let snapshot = selection_or_drawing(&source, &reaction.ids());
    let (format, chemical) = chemical_text_snapshot(&source, &snapshot).unwrap();
    assert_eq!(format, CopyFormat::ChemDoodleReaction);
    assert_eq!(chemical.reactions, source.reactions);
    let copy = prepare_chemical_text(Default::default(), chemical, format)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(copy.text().unwrap()).unwrap();
    assert_eq!(
        json["s"][0]["rs"],
        serde_json::json!([format!("a{}", reaction.reactants[0].atoms[0])])
    );
    assert_eq!(
        json["s"][0]["ps"],
        serde_json::json!([format!("a{}", reaction.products[0].atoms[0])])
    );
    let participant = selection_or_drawing(&source, &reaction.reactants[0].atoms);
    let (format, chemical) = chemical_text_snapshot(&source, &participant).unwrap();
    assert_eq!(format, CopyFormat::Smiles);
    assert_eq!(chemical.atoms.len(), 6);
    assert!(chemical.reactions.is_empty());
    assert!(
        prepare_chemical_text(Default::default(), chemical, format)
            .await
            .unwrap()
            .text()
            .is_some()
    );
    assert_eq!(source, before);
}

#[test]
fn chemical_text_rejects_ambiguous_reactions_and_pruned_explicit_roles() {
    let mut ambiguous = rings(true);
    for atom in &mut ambiguous.atoms[6..] {
        atom.position.x -= 120.;
    }
    assert!(
        chemical_text_snapshot(&ambiguous, &ambiguous)
            .unwrap_err()
            .contains("arrow midpoint")
    );
    let mut source = rings(true);
    let reaction = crate::reactions::copy_reaction(&source, &source)
        .unwrap()
        .unwrap();
    source.reactions = vec![reaction.clone()];
    let selected: Vec<_> = reaction.reactants[0]
        .atoms
        .iter()
        .copied()
        .chain([reaction.arrow])
        .collect();
    let incomplete = selection_or_drawing(&source, &selected);
    assert!(incomplete.reactions.is_empty());
    assert!(chemical_snapshot(&source, &incomplete).is_err());
    assert!(chemical_text_snapshot(&source, &incomplete).is_err());
    let without_arrow = selection_or_drawing(
        &source,
        &source.atoms.iter().map(|a| a.id).collect::<Vec<_>>(),
    );
    assert!(
        chemical_text_snapshot(&source, &without_arrow)
            .unwrap_err()
            .contains("participant")
    );
    let participant = selection_or_drawing(&source, &reaction.reactants[0].atoms);
    assert!(matches!(
        chemical_snapshot(&source, &participant).unwrap(),
        Cow::Borrowed(_)
    ));
}

#[tokio::test]
async fn dark_depth_copy_as_cdxml_and_cdx_preserves_exact_source_ink() {
    use crate::{canvas_theme::CanvasTheme, palette::Color};
    for frozen in [false, true] {
        let mut original = Document {
            canvas_theme: CanvasTheme::Dark,
            ..Document::default()
        };
        for (i, element) in ["O", "C", "O"].into_iter().enumerate() {
            let id = original.add_atom(element, Point::new(i as f32 * 42., 0.));
            original.atom_mut(id).unwrap().depth = i as f32 * 20. - 20.;
            if i > 0 {
                original.add_bond(id - 1, id, 1, "plain");
            }
        }
        let ids = original.all_ids();
        crate::depth_appearance::enable(&mut original, &ids, 0.6).unwrap();
        if frozen {
            crate::depth_appearance::freeze(&mut original, &ids);
        }
        let before = original.clone();
        for format in [CopyFormat::Cdxml, CopyFormat::Cdx] {
            let copy = prepare_as(Default::default(), original.clone(), format)
                .await
                .unwrap();
            let xml = if format == CopyFormat::Cdxml {
                copy.text().unwrap().to_owned()
            } else {
                let representation = copy
                    .representations
                    .iter()
                    .find(|r| r.kind == CDX_TYPES[0])
                    .unwrap();
                crate::exchange::from_cdx(&representation.bytes().unwrap()).unwrap()
            };
            let restored = crate::chemistry::cdxml::import_cdxml(&xml)
                .unwrap()
                .document;
            let rear = restored
                .atoms
                .iter()
                .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
                .unwrap();
            assert_eq!(
                rear.text_style.as_ref().unwrap().color.rgb(),
                [102; 3],
                "{format:?}/{frozen}"
            );
            let bond = restored
                .bonds
                .iter()
                .find(|b| b.a == rear.id || b.b == rear.id)
                .unwrap();
            assert_eq!(bond.color.rgb(), [140; 3], "{format:?}/{frozen}");
            assert!(restored.depth_appearance.is_empty());
            assert_eq!(original.depth_appearance, before.depth_appearance);
            assert_eq!(original.bonds[0].color, Color::Ink);
            assert_eq!(original, before);
        }
    }
}

#[test]
fn selection_does_not_expand_to_other_molecules_or_replace_the_source() {
    let mut doc = molecule();
    let selected = doc.all_ids();
    doc.add_atom("N", Point::new(200., 0.));
    let before = doc.clone();
    let part = selection_or_drawing(&doc, &selected);
    assert_eq!(part.atoms.len(), 2);
    assert_eq!(part.bonds.len(), 1);
    assert_eq!(selection_or_drawing(&doc, &[]), before);
    assert_eq!(doc, before);
}

#[test]
fn reaction_formats_require_complete_explicit_roles_without_unselected_atoms() {
    let mut doc = molecule();
    assert!(reaction_reason(&doc).is_some());
    let product = doc.add_atom("O", Point::new(240., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(90., 0.),
        Point::new(170., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: vec![1, 2],
        coefficient: 1,
    });
    doc.reactions.push(reaction);
    assert!(reaction_reason(&doc).is_some());
    doc.reactions[0].products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    assert!(reaction_reason(&doc).is_none());
    let complete = doc.reactions[0].ids();
    assert!(reaction_reason(&selection_or_drawing(&doc, &[1, 2])).is_some());
    doc.add_atom("N", Point::new(420., 0.));
    assert!(reaction_reason(&doc).is_some());
    assert!(reaction_reason(&selection_or_drawing(&doc, &complete)).is_none());
    assert!(CopyFormat::Mol.unavailable_reason(&doc).is_some());
}

#[test]
fn chosen_formats_have_native_types_and_exact_text_without_hidden_native_data() {
    for (format, kind) in [
        (CopyFormat::Png, "public.png"),
        (CopyFormat::Svg, "public.svg-image"),
        (CopyFormat::Pdf, "com.adobe.pdf"),
        (CopyFormat::Mol, "com.mdli.molfile"),
        (CopyFormat::Smiles, "org.opensmiles.smiles"),
        (CopyFormat::Cdxml, "chemical/x-cdxml"),
        (CopyFormat::Cdx, CDX_TYPES[0]),
    ] {
        let binary = matches!(format, CopyFormat::Png | CopyFormat::Pdf | CopyFormat::Cdx);
        let data = "exact payload\n";
        let copy = from_output(
            format,
            data.as_bytes().to_vec(),
            (!binary).then(|| data.into()),
            vec!["keep warning".into()],
        )
        .unwrap();
        assert!(copy.representations.iter().any(|r| r.kind == kind));
        assert!(
            copy.representations
                .iter()
                .all(|r| r.kind != super::super::NATIVE)
        );
        if format == CopyFormat::Cdx {
            assert_eq!(copy.representations.len(), CDX_TYPES.len());
            assert!(
                copy.representations.iter().all(|item| {
                    std::sync::Arc::ptr_eq(&copy.representations[0].data, &item.data)
                })
            );
        }
        assert_eq!(copy.text(), (!binary).then_some(data));
        assert_eq!(copy.notices, ["keep warning"]);
        for representation in copy.representations {
            assert_eq!(representation.bytes().unwrap(), data.as_bytes());
        }
    }
    for format in [
        CopyFormat::Inchi,
        CopyFormat::Rxn,
        CopyFormat::ReactionSmiles,
        CopyFormat::ChemDoodleReaction,
    ] {
        let copy = from_output(format, b"text".to_vec(), Some("text".into()), vec![]).unwrap();
        assert_eq!(copy.representations.len(), 1);
        assert_eq!(copy.representations[0].kind, TEXT);
    }
}

#[tokio::test]
async fn molecular_formats_export_selected_graph_and_invalid_conversion_has_no_payload() {
    let engine = LocalEngine::default();
    let doc = molecule();
    let before = doc.clone();
    let smiles = prepare_as(engine.clone(), doc.clone(), CopyFormat::Smiles)
        .await
        .unwrap();
    assert_eq!(smiles.text(), Some("CO"));
    let mol = prepare_as(engine.clone(), doc.clone(), CopyFormat::Mol)
        .await
        .unwrap();
    let imported = crate::chemistry::molfile::read(mol.text().unwrap()).unwrap();
    assert_eq!(imported.molecule.ids.len(), 2);
    assert_eq!(doc, before);
    assert!(
        prepare_as(engine.clone(), doc, CopyFormat::Rxn)
            .await
            .is_err()
    );
    assert!(
        prepare_as(engine, Document::default(), CopyFormat::Mol)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn figure_payloads_are_the_chosen_format_without_a_hidden_editable_drawing() {
    let mut drawing = Document::default();
    drawing.arrows.push(crate::document::Arrow::new(
        1,
        Point::default(),
        Point::new(90., 0.),
        Default::default(),
        Default::default(),
    ));
    for format in [CopyFormat::Png, CopyFormat::Svg, CopyFormat::Pdf] {
        let copy = prepare_as(Default::default(), drawing.clone(), format)
            .await
            .unwrap();
        assert_eq!(copy.format, format);
        let bytes = copy.representations[0].bytes().unwrap();
        match format {
            CopyFormat::Png => {
                let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
                assert_eq!(image.get_pixel(0, 0)[3], 0);
                assert!(copy.notices.iter().any(|notice| notice.contains("dpi")));
                assert!(copy.text().is_none());
            }
            CopyFormat::Svg => {
                let svg = std::str::from_utf8(&bytes).unwrap();
                let tree = roxmltree::Document::parse(svg).unwrap();
                assert!(tree.root_element().has_tag_name("svg"));
                assert_eq!(copy.text(), Some(svg));
            }
            CopyFormat::Pdf => {
                assert!(bytes.starts_with(b"%PDF-"));
                assert!(copy.text().is_none());
            }
            _ => unreachable!(),
        }
        assert!(
            copy.representations
                .iter()
                .all(|rep| rep.kind != super::super::NATIVE)
        );
        assert_eq!(
            copy.representations.len(),
            if format == CopyFormat::Svg { 2 } else { 1 }
        );
    }
}

#[tokio::test]
async fn reaction_payload_retains_participants_and_export_warnings() {
    let mut doc = molecule();
    let product = doc.add_atom("O", Point::new(240., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(90., 0.),
        Point::new(170., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: vec![1, 2],
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    doc.reactions.push(reaction);
    let rxn = prepare_as(Default::default(), doc.clone(), CopyFormat::Rxn)
        .await
        .unwrap();
    let roundtrip = crate::chemistry::reaction::read_rxn(rxn.text().unwrap()).unwrap();
    assert_eq!(roundtrip.reactants.len(), 1);
    assert_eq!(roundtrip.products.len(), 1);
    assert!(
        rxn.notices
            .iter()
            .any(|notice| notice == crate::chemistry::reaction::EXPORT_WARNING)
    );
    let smiles = prepare_as(Default::default(), doc, CopyFormat::ReactionSmiles)
        .await
        .unwrap();
    assert_eq!(smiles.text(), Some("CO>>O"));
    assert_eq!(rxn.notices, smiles.notices);
}
