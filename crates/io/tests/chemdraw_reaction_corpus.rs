//! Reaction roles in the independent SDK corpus, including explicit roles
//! that the drawing's geometry would not infer.
use reshiki_io::{compatibility, engine::LocalEngine, exchange, formats};
use reshiki_model::{document::Document, reactions};
use std::path::Path;

fn fixture(name: &str, format: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/chemdraw-import-corpus/supported")
            .join(format!("{name}.{format}")),
    )
    .unwrap()
}

fn atoms(document: &Document, range: std::ops::Range<usize>) -> Vec<u64> {
    document.atoms[range].iter().map(|atom| atom.id).collect()
}

fn sorted(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids
}

fn binary(xml: &str) -> Vec<u8> {
    let root = xml.find("<CDXML").unwrap();
    exchange::to_cdx(&xml[root..]).unwrap()
}

#[tokio::test]
async fn supported_corpus_steps_keep_explicit_molecule_roles_and_condition_captions() {
    let engine = LocalEngine::in_process();
    for (name, step_count) in [
        ("037-reaction-scheme-1", 1),
        ("073-multischeme-2-schemes-1-steps-1", 2),
    ] {
        for format in ["cdxml", "cdx"] {
            let (document, warnings) =
                formats::read_with_warnings(fixture(name, format), format, &engine)
                    .await
                    .unwrap_or_else(|error| panic!("{name}.{format}: {error}"));
            assert!(warnings.is_empty(), "{name}.{format}: {warnings:?}");
            assert_eq!(document.reactions.len(), step_count, "{name}.{format}");
            assert_eq!(document.arrows.len(), step_count, "{name}.{format}");
            reactions::validate(&document).unwrap();
            for (index, reaction) in document.reactions.iter().enumerate() {
                assert_eq!(reaction.arrow, document.arrows[index].id);
                assert!(reaction.agents.is_empty());
                assert_eq!(reaction.products.len(), 1);
                let expected_captions = if step_count == 1 {
                    assert_eq!(reaction.reactants.len(), 1);
                    assert_eq!(
                        sorted(reaction.reactants[0].atoms.clone()),
                        sorted(atoms(&document, 0..3))
                    );
                    assert_eq!(
                        sorted(reaction.products[0].atoms.clone()),
                        sorted(atoms(&document, 3..6))
                    );
                    vec!["catalyst 1".to_owned(), "solvent, 24 h".to_owned()]
                } else {
                    // The two-atom co-reactant is 65 pt below the arrow,
                    // outside the geometric preview's reaction row.
                    let offset = index * 10;
                    assert_eq!(reaction.reactants.len(), 2);
                    assert_eq!(
                        sorted(reaction.reactants[0].atoms.clone()),
                        sorted(atoms(&document, offset..offset + 4))
                    );
                    assert_eq!(
                        sorted(reaction.reactants[1].atoms.clone()),
                        sorted(atoms(&document, offset + 8..offset + 10))
                    );
                    assert_eq!(
                        sorted(reaction.products[0].atoms.clone()),
                        sorted(atoms(&document, offset + 4..offset + 8))
                    );
                    vec![
                        format!("cat. {}.1", index + 1),
                        ["61%", "28%"][index].to_owned(),
                    ]
                };
                assert!(
                    reaction
                        .reactants
                        .iter()
                        .chain(&reaction.products)
                        .all(|participant| participant.coefficient == 1)
                );
                let captions: Vec<_> = reaction
                    .annotations
                    .iter()
                    .map(|id| {
                        document
                            .annotations
                            .iter()
                            .find(|annotation| annotation.id == *id)
                            .unwrap()
                            .text
                            .clone()
                    })
                    .collect();
                assert_eq!(captions, expected_captions, "{name}.{format}, step {index}");
            }
        }
    }
}

#[tokio::test]
async fn a_corpus_caption_used_as_a_reactant_keeps_the_metadata_loss_notice() {
    let source = String::from_utf8(fixture("037-reaction-scheme-1", "cdxml"))
        .unwrap()
        .replace(
            "ReactionStepReactants=\"2\"",
            "ReactionStepReactants=\"17\"",
        );
    // Reuse the existing catalyst caption as a textual starting material.
    // It remains visible, but a native participant requires atoms.
    let binary = binary(&source);
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let contents = compatibility::import_text(format, bytes.clone()).unwrap();
        let loss = formats::chemdraw_loss(format, &contents);
        assert_eq!((loss.schemes, loss.steps), (1, 1), "{format}");
        let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap();
        assert_eq!((document.atoms.len(), document.bonds.len()), (6, 4));
        assert_eq!((document.arrows.len(), document.annotations.len()), (1, 2));
        assert!(document.reactions.is_empty(), "{format}");
        assert!(document.annotations.iter().any(|a| a.text == "catalyst 1"));
        assert!(
            matches!(warnings.as_slice(), [warning] if warning.contains("external reaction roles and condition references are not retained")),
            "{format}: {warnings:?}"
        );
    }
}

#[tokio::test]
async fn a_complete_step_resolves_the_captured_group_and_superseded_arrow() {
    // The original capture has no reactant or product references. Give its
    // existing group (six-atom ring) and outside fragment (water) those roles.
    // The source step still names legacy graphic 17, superseded by arrow 23.
    let complete = |xml: &str| {
        xml.replace(
            "ReactionStepArrows=\"17\"",
            "ReactionStepArrows=\"17\" ReactionStepReactants=\"20\" ReactionStepProducts=\"19\"",
        )
        .replace(
            "ReactionStepObjectsBelowArrow=\"20 16\"",
            "ReactionStepObjectsBelowArrow=\"16\"",
        )
    };
    let source = complete(include_str!(
        "../../../tests/fixtures/chemdraw-arrows/numeric.cdxml"
    ));
    let captured = exchange::from_cdx_tolerant(include_bytes!(
        "../../../tests/fixtures/chemdraw-arrows/numeric.cdx"
    ))
    .unwrap();
    let binary = binary(&complete(&captured));
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{format}: {warnings:?}");
        assert_eq!(document.reactions.len(), 1, "{format}");
        let reaction = &document.reactions[0];
        assert_eq!(reaction.arrow, document.arrows[0].id);
        assert_eq!(reaction.reactants.len(), 1);
        assert_eq!(reaction.products.len(), 1);
        assert!(reaction.agents.is_empty());
        let ring = document
            .atoms
            .iter()
            .filter(|atom| atom.element != "O")
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        assert_eq!(ring.len(), 6);
        assert_eq!(sorted(reaction.reactants[0].atoms.clone()), sorted(ring));
        let water = document
            .atoms
            .iter()
            .find(|atom| atom.element == "O")
            .unwrap();
        assert_eq!(reaction.products[0].atoms, [water.id]);
        assert_eq!(reaction.annotations, [document.annotations[0].id]);
        assert_eq!(document.annotations[0].text, "Fixed text");
        reactions::validate(&document).unwrap();
    }
}
