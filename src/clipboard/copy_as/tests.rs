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
