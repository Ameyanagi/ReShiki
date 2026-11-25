//! Reaction roles in the independent SDK corpus, including explicit roles
//! that the drawing's geometry would not infer.
use crate::{
    compatibility, cycles,
    document::{Annotation, Document, Point},
    engine::LocalEngine,
    exchange, formats, reactions,
};
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
async fn a_complete_step_resolves_the_captured_group_and_deduplicates_superseded_arrows() {
    // The original capture has no reactant or product references. Give its
    // existing group (six-atom ring) and outside fragment (water) those roles.
    // The source step still names legacy graphic 17, superseded by arrow 23.
    let complete = |xml: &str| {
        xml.replace(
            "ReactionStepArrows=\"17\"",
            "ReactionStepArrows=\"17 23\" ReactionStepReactants=\"20\" ReactionStepProducts=\"19\"",
        )
        .replace(
            "ReactionStepObjectsBelowArrow=\"20 16\"",
            "ReactionStepObjectsBelowArrow=\"16\"",
        )
    };
    let source = complete(include_str!(
        "../../../../../tests/fixtures/chemdraw-arrows/numeric.cdxml"
    ));
    let captured = exchange::from_cdx_tolerant(include_bytes!(
        "../../../../../tests/fixtures/chemdraw-arrows/numeric.cdx"
    ))
    .unwrap();
    let binary = binary(&complete(&captured));
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let contents = compatibility::import_text(format, bytes.clone()).unwrap();
        assert_eq!(
            formats::chemdraw_loss(format, &contents),
            formats::ChemdrawLoss::default(),
            "{format}"
        );
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

fn map_numbers(document: &Document, ids: &[u64]) -> Vec<u32> {
    ids.iter()
        .map(|id| document.atom(*id).unwrap().map_num)
        .collect()
}

#[tokio::test]
async fn shared_intermediates_propagate_atom_maps_in_either_step_order() {
    let source =
        String::from_utf8(fixture("080-multischeme-5-schemes-2-steps-8", "cdxml")).unwrap();
    let mut reversed: Vec<_> = source.lines().collect();
    let step_lines: Vec<_> = reversed
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.contains("<step ").then_some(index))
        .collect();
    for &[first, second] in step_lines.as_chunks::<2>().0 {
        reversed.swap(first, second);
    }
    let reversed = reversed.join("\n");
    let engine = LocalEngine::in_process();
    for (order, source) in [("forward", source), ("reverse", reversed)] {
        let binary = binary(&source);
        for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
            let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
                .await
                .unwrap();
            assert!(warnings.is_empty(), "{order}.{format}: {warnings:?}");
            assert_eq!(document.reactions.len(), 10, "{order}.{format}");
            for [first, second] in document.reactions.as_chunks::<2>().0 {
                let (first, second) = if order == "forward" {
                    (first, second)
                } else {
                    (second, first)
                };
                let starting = map_numbers(&document, &first.reactants[0].atoms);
                let intermediate = map_numbers(&document, &first.products[0].atoms);
                let final_product = map_numbers(&document, &second.products[0].atoms);
                assert!(starting.iter().all(|map| *map > 0), "{order}.{format}");
                assert_eq!(starting.len(), 4);
                assert_eq!(starting, intermediate, "{order}.{format}");
                assert_eq!(intermediate, final_product, "{order}.{format}");
                assert_eq!(
                    sorted(first.products[0].atoms.clone()),
                    sorted(second.reactants[0].atoms.clone())
                );
            }
        }
    }
}

#[tokio::test]
async fn atom_map_fields_use_aggregate_then_manual_then_auto() {
    let source = String::from_utf8(fixture("040-reaction-scheme-4", "cdxml")).unwrap();
    let manual = "ReactionStepAtomMapManual=\"3 12 4 13 5 14 6 15\"";
    let reversed = "3 15 4 14 5 13 6 12";
    let engine = LocalEngine::in_process();
    for (name, maps, expected) in [
        ("manual", manual.to_owned(), vec![1, 2, 3, 4]),
        ("auto", manual.replace("Manual", "Auto"), vec![1, 2, 3, 4]),
        (
            "manual before auto",
            format!("ReactionStepAtomMapAuto=\"{reversed}\" {manual}"),
            vec![1, 2, 3, 4],
        ),
        (
            "disjoint manual and auto",
            "ReactionStepAtomMapAuto=\"5 14 6 15\" ReactionStepAtomMapManual=\"3 12 4 13\"".into(),
            vec![1, 2, 3, 4],
        ),
        (
            "aggregate before manual",
            format!("{manual} ReactionStepAtomMap=\"{reversed}\""),
            vec![4, 3, 2, 1],
        ),
        (
            "empty aggregate",
            format!("{manual} ReactionStepAtomMap=\"\""),
            vec![0, 0, 0, 0],
        ),
    ] {
        let source = source.replace(manual, &maps);
        let binary = binary(&source);
        for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
            let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
                .await
                .unwrap();
            assert!(warnings.is_empty(), "{name}.{format}: {warnings:?}");
            assert_eq!(document.reactions.len(), 1, "{name}.{format}");
            let reaction = &document.reactions[0];
            assert_eq!(
                map_numbers(&document, &reaction.products[0].atoms),
                expected,
                "{name}.{format}"
            );
        }
    }
}

#[tokio::test]
async fn conflicting_or_out_of_role_atom_pairs_do_not_erase_other_maps() {
    let source = String::from_utf8(fixture("040-reaction-scheme-4", "cdxml"))
        .unwrap()
        .replace(
            "ReactionStepAtomMapManual=\"3 12 4 13 5 14 6 15\"",
            // The third pair conflicts with two established maps; the fourth
            // has its roles reversed. The final pairs still restore normally.
            "ReactionStepAtomMapManual=\"3 12 4 13 3 13 14 5 5 14 6 15\"",
        );
    let binary = binary(&source);
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let document = formats::read_with_engine(bytes, format, &engine)
            .await
            .unwrap();
        let reaction = &document.reactions[0];
        assert_eq!(
            map_numbers(&document, &reaction.reactants[0].atoms),
            [1, 2, 3, 4],
            "{format}"
        );
        assert_eq!(
            map_numbers(&document, &reaction.products[0].atoms),
            [1, 2, 3, 4],
            "{format}"
        );
    }
}

#[tokio::test]
async fn repeated_atom_map_endpoints_keep_only_the_first_pair_in_a_step() {
    let source = String::from_utf8(fixture("040-reaction-scheme-4", "cdxml"))
        .unwrap()
        .replace(
            "ReactionStepAtomMapManual=\"3 12 4 13 5 14 6 15\"",
            // Reuse reactant 3, then product 12. Neither later pair may map
            // another atom; the remaining independent pairs still restore.
            "ReactionStepAtomMapManual=\"3 12 3 13 4 12 5 14 6 15\"",
        );
    let binary = binary(&source);
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{format}: {warnings:?}");
        assert_eq!(document.reactions.len(), 1, "{format}");
        let reaction = &document.reactions[0];
        for participant in [&reaction.reactants[0], &reaction.products[0]] {
            assert_eq!(
                map_numbers(&document, &participant.atoms),
                [1, 0, 2, 3],
                "{format}"
            );
        }
        reactions::validate(&document).unwrap();
    }
}

#[tokio::test]
async fn an_invalid_step_keeps_the_other_scheme_and_counts_only_its_own_loss() {
    let source = String::from_utf8(fixture("073-multischeme-2-schemes-1-steps-1", "cdxml"))
        .unwrap()
        .replacen(
            "ReactionStepProducts=\"12\"",
            "ReactionStepProducts=\"3\"",
            1,
        );
    let binary = binary(&source);
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let contents = compatibility::import_text(format, bytes.clone()).unwrap();
        let loss = formats::chemdraw_loss(format, &contents);
        assert_eq!((loss.schemes, loss.steps), (1, 1), "{format}");
        let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap();
        assert_eq!(document.reactions.len(), 1, "{format}");
        assert_eq!(document.reactions[0].arrow, document.arrows[1].id);
        assert_eq!(document.reactions[0].reactants.len(), 2);
        assert_eq!(document.reactions[0].products.len(), 1);
        assert!(
            matches!(warnings.as_slice(), [warning] if warning.contains("external reaction roles and condition references are not retained")),
            "{format}: {warnings:?}"
        );
        reactions::validate(&document).unwrap();
    }
}

#[tokio::test]
async fn save_loss_credits_the_restored_step_when_an_invalid_step_shares_its_arrow() {
    let source = String::from_utf8(fixture("073-multischeme-2-schemes-1-steps-1", "cdxml"))
        .unwrap()
        .replacen(
            "ReactionStepProducts=\"12\"",
            "ReactionStepProducts=\"3\"",
            1,
        )
        .replace("ReactionStepArrows=\"57\"", "ReactionStepArrows=\"27\"")
        .replace(
            "<step id=\"61\"",
            "<step id=\"100\" ReactionStepArrows=\"57\" />\n      <step id=\"61\"",
        );
    let binary = binary(&source);
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let contents = compatibility::import_text(format, bytes.clone()).unwrap();
        let loss = formats::chemdraw_loss(format, &contents);
        // Neither scheme is complete: the first is invalid and the second
        // keeps its valid step while losing the added incomplete step.
        assert_eq!((loss.schemes, loss.steps), (2, 2), "{format}");
        let document = formats::read_with_engine(bytes, format, &engine)
            .await
            .unwrap();
        assert_eq!(document.reactions.len(), 1, "{format}");
        assert_eq!(document.reactions[0].arrow, document.arrows[0].id);
    }
}

async fn mixed_cycle_document(engine: &LocalEngine) -> Document {
    let mut document = formats::read_with_engine(
        fixture("073-multischeme-2-schemes-1-steps-1", "cdxml"),
        "cdxml",
        engine,
    )
    .await
    .unwrap();
    // Reverse the first step's explicit roles so geometry cannot recover them.
    let first = &mut document.reactions[0];
    std::mem::swap(&mut first.reactants, &mut first.products);
    let mut intermediates = Vec::new();
    for (x, y, text) in [
        (800., -200., "Pd(0)L2"),
        (970., 90., "ArPdX"),
        (570., 90., "ArPdAr"),
    ] {
        let id = document.next_id();
        document.annotations.push(Annotation {
            id,
            angle: 0.,
            position: Point::new(x, y),
            text: text.into(),
            format: Default::default(),
        });
        intermediates.push(id);
    }
    cycles::make(&mut document, &intermediates).unwrap();
    document
}

#[tokio::test]
async fn a_fresh_cycle_keeps_unrelated_explicit_steps_and_their_save_loss_credit() {
    let engine = LocalEngine::in_process();
    let document = mixed_cycle_document(&engine).await;
    for format in ["cdxml", "cdx"] {
        let bytes = formats::write_with_engine(document.clone(), format, &engine)
            .await
            .unwrap();
        let contents = compatibility::import_text(format, bytes.clone()).unwrap();
        assert_eq!(
            formats::chemdraw_loss(format, &contents),
            formats::ChemdrawLoss::default(),
            "{format}"
        );
        let (restored, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{format}: {warnings:?}");
        assert_eq!(
            restored
                .graphics
                .iter()
                .filter(|g| g.cycle.is_some())
                .count(),
            1
        );
        assert_eq!(restored.reactions.len(), 2, "{format}");
        assert_eq!(restored.reactions[0].reactants.len(), 1);
        assert_eq!(restored.reactions[0].products.len(), 2);
        assert_eq!(restored.reactions[1].reactants.len(), 2);
        assert_eq!(restored.reactions[1].products.len(), 1);
        reactions::validate(&restored).unwrap();
    }
}

#[tokio::test]
async fn an_edited_cycle_keeps_unrelated_explicit_steps_and_their_save_loss_credit() {
    let engine = LocalEngine::in_process();
    let document = mixed_cycle_document(&engine).await;
    let xml = String::from_utf8(
        formats::write_with_engine(document, "cdxml", &engine)
            .await
            .unwrap(),
    )
    .unwrap();
    // Change an ordinary condition caption while leaving the cycle seal.
    let source = xml.replace("cat. 1.1", "cat. edited");
    assert_ne!(source, xml);
    let binary = binary(&source);
    for (format, bytes) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let contents = compatibility::import_text(format, bytes.clone()).unwrap();
        let loss = formats::chemdraw_loss(format, &contents);
        // Only the text-intermediate cycle's unrepresentable steps are lost.
        assert_eq!((loss.schemes, loss.steps), (1, 3), "{format}");
        let (restored, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap();
        assert_eq!(warnings.len(), 2, "{format}: {warnings:?}");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("catalytic cycle was edited"))
        );
        assert!(warnings.iter().any(|w| {
            w.contains("external reaction roles and condition references are not retained")
        }));
        assert!(restored.graphics.iter().all(|g| g.cycle.is_none()));
        assert_eq!(restored.reactions.len(), 2, "{format}");
        assert_eq!(restored.reactions[0].reactants.len(), 1);
        assert_eq!(restored.reactions[0].products.len(), 2);
        assert_eq!(restored.reactions[1].reactants.len(), 2);
        assert_eq!(restored.reactions[1].products.len(), 1);
        assert!(restored.annotations.iter().any(|a| a.text == "cat. edited"));
        reactions::validate(&restored).unwrap();
    }
}
