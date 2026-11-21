//! Editable generic drawing export must not require a concrete molecular identity.
use reshiki_io::{
    engine::{LocalEngine, Request},
    exchange, formats,
};
use reshiki_model::{
    arrow_labels::{ArrowLabel, Side},
    arrows::{ArrowStyle, Preset},
    atom_text,
    document::{Annotation, Arrow, Document, Point},
    generic::{self, Generic, RepeatCount},
    graphics::{GraphicKind, LinePattern},
};
use std::{collections::HashMap, path::Path};

async fn imported(bytes: Vec<u8>, format: &str) -> Document {
    let (document, warnings) =
        formats::read_with_warnings(bytes, format, &LocalEngine::in_process())
            .await
            .unwrap();
    assert!(warnings.iter().any(|notice| notice == generic::NOTICE));
    assert!(generic::present(&document));
    document
}

async fn fixture(name: &str, format: &str) -> Document {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
        .with_extension(format);
    imported(std::fs::read(path).unwrap(), format).await
}

fn canonical_generic(document: &Document) -> Generic {
    let ids = document
        .atoms
        .iter()
        .map(|atom| atom.id)
        .chain(document.graphics.iter().map(|graphic| graphic.id))
        .enumerate()
        .map(|(index, id)| (id, index as u64 + 1))
        .collect();
    let mut generic = document.generic.clone();
    generic.remap(&ids).unwrap();
    generic
}

async fn editable_bytes(source: &Document, format: &str) -> Result<Vec<u8>, String> {
    let drawing_format = if format == "cds" { "cdxml" } else { format };
    let bytes =
        formats::write_with_engine(source.clone(), drawing_format, &LocalEngine::in_process())
            .await?;
    if format == "cds" {
        exchange::to_cds(std::str::from_utf8(&bytes).unwrap())
    } else {
        Ok(bytes)
    }
}

async fn roundtrip(source: &Document, format: &str) -> Document {
    let bytes = editable_bytes(source, format)
        .await
        .unwrap_or_else(|error| panic!("{format} generic drawing export: {error}"));
    let restored = imported(bytes, if format == "cds" { "cdx" } else { format }).await;
    assert_eq!(
        canonical_generic(&restored),
        canonical_generic(source),
        "{format}"
    );
    assert_eq!(restored.atoms.len(), source.atoms.len(), "{format}");
    assert_eq!(restored.bonds.len(), source.bonds.len(), "{format}");
    assert_eq!(restored.graphics.len(), source.graphics.len(), "{format}");
    let source_origin = source.atoms.first().unwrap().position;
    let restored_origin = restored.atoms.first().unwrap().position;
    for (actual, expected) in restored.atoms.iter().zip(&source.atoms) {
        assert_eq!(actual.element, expected.element, "{format}");
        assert_eq!(actual.charge, expected.charge, "{format}");
        assert_eq!(actual.isotope, expected.isotope, "{format}");
        assert!(
            actual
                .position
                .offset(-restored_origin.x, -restored_origin.y)
                .distance(expected.position.offset(-source_origin.x, -source_origin.y))
                < 0.002,
            "{format}: atom geometry changed"
        );
    }
    let endpoints = |document: &Document| {
        let indices: HashMap<_, _> = document
            .atoms
            .iter()
            .enumerate()
            .map(|(index, atom)| (atom.id, index))
            .collect();
        document
            .bonds
            .iter()
            .map(|bond| {
                (
                    indices[&bond.a],
                    indices[&bond.b],
                    bond.order,
                    bond.display.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(endpoints(&restored), endpoints(source), "{format}");
    for (actual, expected) in restored.graphics.iter().zip(&source.graphics) {
        assert_eq!(actual.kind, expected.kind, "{format}");
        assert_eq!(actual.style.stroke, expected.style.stroke, "{format}");
        assert!(
            (actual.style.width_pt - expected.style.width_pt).abs() < 0.0001,
            "{format}: bracket stroke width changed"
        );
        assert!(
            actual
                .origin
                .offset(-restored_origin.x, -restored_origin.y)
                .distance(expected.origin.offset(-source_origin.x, -source_origin.y))
                < 0.002,
            "{format}: bracket position changed: actual {actual:?}, expected {expected:?}; atom origins {restored_origin:?} / {source_origin:?}"
        );
        assert!(actual.axis_x.distance(expected.axis_x) < 0.002, "{format}");
        assert!(actual.axis_y.distance(expected.axis_y) < 0.002, "{format}");
    }
    restored
}

#[tokio::test]
async fn all_four_polymer_variants_roundtrip_from_both_source_formats() {
    for name in [
        "041-polymer-repeat-1",
        "042-polymer-repeat-2",
        "043-polymer-repeat-3",
        "044-polymer-repeat-4",
    ] {
        for input in ["cdxml", "cdx"] {
            let document =
                fixture(&format!("chemdraw-import-corpus/supported/{name}"), input).await;
            assert_eq!(document.generic.repeats.len(), 1, "{name}.{input}");
            for output in ["cdxml", "cdx"] {
                let once = roundtrip(&document, output).await;
                roundtrip(&once, output).await;
            }
        }
    }
}

#[tokio::test]
async fn paired_semantic_bracket_frames_do_not_grow_with_thick_strokes_or_curves() {
    let source = fixture(
        "chemdraw-import-corpus/supported/041-polymer-repeat-1",
        "cdxml",
    )
    .await;
    for kind in [
        GraphicKind::Brackets,
        GraphicKind::Parentheses,
        GraphicKind::Braces,
    ] {
        let mut document = source.clone();
        let bracket = document.graphics.first_mut().unwrap();
        bracket.kind = kind;
        bracket.style.width_pt = 2.4;
        document.validate().unwrap();
        for format in ["cdxml", "cdx", "cds"] {
            let once = roundtrip(&document, format).await;
            roundtrip(&once, format).await;
        }
    }
}

#[tokio::test]
async fn transformed_semantic_bracket_frames_reject_editable_export_without_changing_the_source() {
    let source = fixture(
        "chemdraw-import-corpus/supported/041-polymer-repeat-1",
        "cdxml",
    )
    .await;
    for (axis_x, axis_y) in [
        (Point::new(84., 84.), Point::new(-84., 84.)),
        (Point::new(84., 20.), Point::new(0., 84.)),
        (Point::new(84., 0.), Point::new(20., 84.)),
        (Point::new(-84., 0.), Point::new(0., 84.)),
        (Point::new(84., 0.), Point::new(0., -84.)),
    ] {
        for kind in [
            GraphicKind::Brackets,
            GraphicKind::Parentheses,
            GraphicKind::Braces,
        ] {
            let mut document = source.clone();
            let bracket = document.graphics.first_mut().unwrap();
            bracket.kind = kind;
            bracket.axis_x = axis_x;
            bracket.axis_y = axis_y;
            document.validate().unwrap();
            let before = document.clone();
            for format in ["cdxml", "cdx", "cds"] {
                let error = editable_bytes(&document, format).await.unwrap_err();
                assert!(
                    error.contains("cannot preserve rotated, skewed or mirrored chemical brackets"),
                    "{format}: {error}"
                );
                assert!(error.contains("native ReShiki (.rsk)"), "{format}: {error}");
                assert_eq!(document, before, "{format}");
            }
        }
    }
}

#[tokio::test]
async fn non_solid_semantic_brackets_reject_editable_export_without_changing_the_source() {
    let source = fixture(
        "chemdraw-import-corpus/supported/041-polymer-repeat-1",
        "cdxml",
    )
    .await;
    for pattern in [LinePattern::Dashed, LinePattern::Dotted] {
        for kind in [
            GraphicKind::Brackets,
            GraphicKind::Parentheses,
            GraphicKind::Braces,
        ] {
            let mut document = source.clone();
            let bracket = document.graphics.first_mut().unwrap();
            bracket.kind = kind;
            bracket.style.pattern = pattern;
            document.validate().unwrap();
            let before = document.clone();
            for format in ["cdxml", "cdx", "cds"] {
                let error = editable_bytes(&document, format).await.unwrap_err();
                assert!(
                    error.contains("cannot preserve dashed or dotted chemical brackets"),
                    "{format}: {error}"
                );
                assert!(error.contains("native ReShiki (.rsk)"), "{format}: {error}");
                assert_eq!(document, before, "{format}");
            }
        }
    }
}

#[tokio::test]
async fn element_lists_not_lists_group_choices_isotopes_and_link_nodes_stay_editable() {
    let mut chain = Document::default();
    for (index, element) in ["O", "C", "C", "N"].iter().enumerate() {
        let id = chain.add_atom(element, Point::new(index as f32 * 42., 0.));
        if id > 1 {
            chain.add_bond(id - 1, id, 1, "plain");
        }
    }
    let mut documents = Vec::new();
    for label in ["[Cl,Br,I]", "[NOT Cl,Br,I]", "[OMe,OPh]", "[D,T]"] {
        documents.push(atom_text::apply(&chain, 1, label, atom_text::Mode::Auto).unwrap());
    }
    documents.push(fixture("generic-structures/anonymous-mixed", "cdxml").await);
    documents.push(fixture("generic-structures/link-chain", "cdxml").await);
    for document in documents {
        for format in ["cdxml", "cdx"] {
            roundtrip(&document, format).await;
        }
    }
}

#[tokio::test]
async fn generic_drawing_export_keeps_the_snapshot_and_other_drawing_loss_notices() {
    let mut document = fixture("generic-structures/link-chain", "cdxml").await;
    let arrow = document.next_id();
    document.arrows.push(Arrow::new(
        arrow,
        Point::new(0., 100.),
        Point::new(120., 100.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let caption = document.next_id();
    document.annotations.push(Annotation {
        angle: 0.,
        id: caption,
        position: Point::new(30., 70.),
        text: "heat".into(),
        format: Default::default(),
    });
    document.arrow_labels.push(ArrowLabel {
        arrow,
        annotation: caption,
        side: Side::Above,
        gap: None,
    });
    document.validate().unwrap();
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        let response = LocalEngine::in_process().request(request).await.unwrap();
        assert_eq!(response.document.as_ref(), Some(&document), "{format}");
        assert!(response.analysis.is_none(), "{format}");
        assert!(response.output.is_some(), "{format}");
        assert_eq!(
            response.warnings,
            [generic::NOTICE, reshiki_model::arrow_labels::EXPORT_NOTICE]
        );
    }
}

#[tokio::test]
async fn generic_choices_do_not_acquire_identifiers_or_molecular_properties() {
    for name in [
        "generic-structures/anonymous-mixed",
        "generic-structures/link-chain",
        "chemdraw-import-corpus/supported/041-polymer-repeat-1",
    ] {
        let document = fixture(name, "cdxml").await;
        let error = LocalEngine::in_process()
            .request(Request::molecule("analyze", document.clone()))
            .await
            .unwrap_err();
        assert!(error.contains(generic::NOTICE), "{error}");
        for format in ["smiles", "inchi", "mol"] {
            let error =
                formats::write_with_engine(document.clone(), format, &LocalEngine::in_process())
                    .await
                    .unwrap_err();
            assert!(error.contains(generic::NOTICE), "{name}/{format}: {error}");
        }
    }
}

#[tokio::test]
async fn malformed_native_generic_definitions_are_still_rejected() {
    let source = fixture(
        "chemdraw-import-corpus/supported/041-polymer-repeat-1",
        "cdxml",
    )
    .await;
    let mut missing_member = source.clone();
    missing_member.generic.repeats[0].members.push(u64::MAX);
    let mut missing_crossing = source.clone();
    missing_crossing.generic.repeats[0].crossings.pop();
    let mut invalid_range = source.clone();
    invalid_range.generic.repeats[0].count = RepeatCount::Finite { min: 5, max: 2 };
    for document in [missing_member, missing_crossing, invalid_range] {
        let validation_error = document.validate().unwrap_err();
        for format in ["cdxml", "cdx"] {
            let error =
                formats::write_with_engine(document.clone(), format, &LocalEngine::in_process())
                    .await
                    .unwrap_err();
            assert!(error.contains(&validation_error), "{format}: {error}");
        }
    }
}
