//! Distance-derived bonds remain drawing data across engine routes.

use super::*;
use crate::{
    document::{CRYSTALS, DISTANCE_BONDS, Point},
    models3d::{self, Cell, ForceField, Model3d, Placed, crystal},
};

fn distance_pair() -> Document {
    // Chemically valid ethane would pass sanitization. The provenance itself,
    // rather than a fortuitous valence error, must prevent its interpretation.
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(10., 20.));
    let b = doc.add_atom("C", Point::new(66., 20.));
    doc.add_bond(a, b, 1, "plain");
    doc.bonds[0].distance_derived = true;
    doc.require_version(DISTANCE_BONDS);
    doc.validate().unwrap();
    doc
}

fn lone_crystal_atom() -> Document {
    // A live cell is also protected when there are no within-cell bonds to mark.
    let mut doc = Document::default();
    let id = doc.add_atom("He", Point::default());
    let model = Model3d::new(
        &doc,
        vec![Placed { id, xyz: [0.; 3] }],
        vec![],
        ForceField::Uff,
    )
    .with_cell(
        &doc,
        Some(Cell {
            vectors: [[10., 0., 0.], [0., 10., 0.], [0., 0., 10.]],
            crossing: vec![],
        }),
        vec![],
    );
    doc.models.push(model);
    doc.require_version(CRYSTALS);
    doc.validate().unwrap();
    doc
}

fn salt_network(with_cell: bool) -> Document {
    let (mut doc, warnings) = crate::crystal::read(
        "cif",
        include_str!("../../../../../tests/fixtures/crystal/nacl.cif"),
    )
    .unwrap();
    assert!(warnings.is_empty());
    assert!(doc.bonds.iter().any(|b| b.distance_derived));
    if !with_cell {
        doc.models.clear();
    }
    doc.validate().unwrap();
    doc
}

fn export_request(doc: Document, format: &str) -> Request {
    let mut request = Request::molecule("export", doc);
    request.format = Some(format.into());
    request
}

#[tokio::test]
async fn analysis_keeps_distance_drawing_exactly_and_qualifies_its_unavailability() {
    for doc in [
        distance_pair(),
        lone_crystal_atom(),
        salt_network(true),
        salt_network(false),
    ] {
        let expected_notice = crystal::analysis_notice(&doc);
        let response = execute(Request::molecule("analyze", doc.clone()))
            .await
            .unwrap();
        assert_eq!(response.document.as_ref(), Some(&doc));
        assert!(response.analysis.is_none() && response.output.is_none());
        assert_eq!(response.warnings, [expected_notice]);
    }
}

#[tokio::test]
async fn distance_route_validates_protocol_document_and_format_before_returning() {
    let doc = distance_pair();
    let mut bad_protocol = Request::molecule("analyze", doc.clone());
    bad_protocol.protocol += 1;
    assert!(matches!(execute(bad_protocol).await, Err(Error::Protocol)));
    for bad in [
        {
            let mut bad = doc.clone();
            bad.version = DISTANCE_BONDS - 1;
            bad
        },
        {
            let mut bad = doc.clone();
            bad.atoms[0].position.x = f32::NAN;
            bad
        },
    ] {
        assert!(matches!(
            execute(Request::molecule("analyze", bad)).await,
            Err(Error::Chemistry(_))
        ));
    }
    assert!(matches!(
        execute(Request::molecule("export", doc.clone())).await,
        Err(Error::MissingFormat)
    ));
    assert!(matches!(
        execute(export_request(doc, "unknown")).await,
        Err(Error::Export)
    ));
}

#[tokio::test]
async fn molecular_exports_and_attachment_mol_cannot_bypass_distance_provenance() {
    let doc = distance_pair();
    let mut attached = doc.clone();
    let ids = attached.all_ids();
    let point = crate::attachments::add(&mut attached, &ids, crate::attachments::Kind::MultiCenter)
        .unwrap();
    let outside = attached.add_atom("N", Point::new(100., 50.));
    attached.add_bond(point, outside, 1, "plain");
    attached.validate().unwrap();
    for doc in [doc, attached, lone_crystal_atom()] {
        for format in ["mol", "smiles", "inchi"] {
            let error = execute(export_request(doc.clone(), format))
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                Error::Drawing(molecular::Error::DistanceBonds)
            ));
            assert!(error.to_string().contains("crystal atom distances"));
        }
    }
}

#[tokio::test]
async fn caller_supplied_preparation_cannot_reinterpret_a_distance_drawing() {
    let doc = distance_pair();
    let mut ordinary = doc.clone();
    ordinary.bonds[0].distance_derived = false;
    let molecule = molecular::prepare(&ordinary).unwrap();
    let drawing = molecular::for_drawing(&molecule, &ordinary).unwrap();
    let prepared = Arc::new(Prepared { molecule, drawing });
    for (operation, format) in [("analyze", None), ("export", Some("mol"))] {
        let mut request = Request::molecule(operation, doc.clone());
        request.format = format.map(str::to_owned);
        // This file does not exist: a skipped analysis must not start a helper.
        let result = build(
            Arc::new(request),
            Some(Arc::clone(&prepared)),
            Config::new(PathBuf::from("/missing-distance-bond-inchi-helper")),
        )
        .await;
        if operation == "analyze" {
            let response = result.unwrap();
            assert!(response.analysis.is_none());
            assert_eq!(response.document, Some(doc.clone()));
        } else {
            assert!(matches!(
                result,
                Err(Error::Drawing(molecular::Error::DistanceBonds))
            ));
        }
    }
}

#[tokio::test]
async fn editable_drawing_exports_preserve_edges_and_report_provenance_loss() {
    for doc in [
        distance_pair(),
        lone_crystal_atom(),
        salt_network(true),
        salt_network(false),
    ] {
        for format in ["cdxml", "cdx"] {
            let response = execute(export_request(doc.clone(), format)).await.unwrap();
            assert_eq!(response.document, Some(doc.clone()));
            assert!(response.analysis.is_none());
            assert_eq!(response.warnings.len(), 2);
            assert_eq!(response.warnings[0], crystal::analysis_notice(&doc));
            let loss = &response.warnings[1];
            assert!(loss.contains("without") && loss.contains("distance-derived bond provenance"));
            assert!(loss.contains("Reimported bonds may be interpreted as chemical bonds"));
            assert_eq!(loss.contains("unit cell"), crystal::present(&doc));
            let output = response.output.unwrap();
            let xml = if format == "cdx" {
                exchange::from_cdx(&STANDARD.decode(output).unwrap()).unwrap()
            } else {
                output
            };
            let parsed = chemistry::cdxml::read(&xml).unwrap();
            let atoms = parsed
                .fragments
                .iter()
                .map(|f| f.graph.atoms.len())
                .sum::<usize>();
            let bonds = parsed
                .fragments
                .iter()
                .map(|f| f.graph.bonds.len())
                .sum::<usize>();
            assert_eq!((atoms, bonds), (doc.atoms.len(), doc.bonds.len()));
            assert!(!xml.contains("distance_derived"));
            if doc.atoms.len() == 2 {
                let positions = &parsed.fragments[0].positions;
                assert!((positions[1].y - positions[0].y).abs() < 1e-6);
                assert!(positions[1].x > positions[0].x);
            }
        }
    }
}

#[tokio::test]
async fn drawing_fallback_keeps_strict_appearance_refusals() {
    let mut ring_arc = distance_pair();
    ring_arc.bonds[0].ring_arc = true;
    let mut variable = distance_pair();
    variable.atoms[0].element = "*".into();
    variable.atoms[0].display.variable = Some("R".into());
    for (doc, ring_arc) in [(ring_arc, true), (variable, false)] {
        doc.validate().unwrap();
        for format in ["cdxml", "cdx"] {
            let error = execute(export_request(doc.clone(), format))
                .await
                .unwrap_err();
            let Error::DrawingExport(ref drawing_error) = error else {
                panic!("{format}: expected a drawing appearance refusal, got {error}");
            };
            // The appearance blocker remains a drawing-export refusal when
            // its inner classification changes from Invalid to Unsupported.
            assert!(
                !matches!(
                    drawing_error,
                    exchange::drawing::Error::Chemistry(_) | exchange::drawing::Error::Limit
                ),
                "{format}: expected a strict appearance refusal, got {error}"
            );
            let message = drawing_error.to_string();
            let identifies_appearance = if ring_arc {
                message.contains("inner ring curves")
            } else {
                message.contains("variable atom labels")
                    || message.contains("text label") && message.contains('R')
            };
            assert!(identifies_appearance, "{format}: {error}");
            assert!(
                message.contains("Save as") && message.contains(".rsk"),
                "{format}: native-save guidance must be retained: {error}"
            );
        }
    }
}

#[tokio::test]
async fn native_and_figure_exports_keep_distance_drawing_without_chemistry() {
    let engine = crate::engine::LocalEngine::default();
    for doc in [
        distance_pair(),
        lone_crystal_atom(),
        salt_network(true),
        salt_network(false),
    ] {
        let saved = doc.file_json().unwrap();
        let reopened = Document::from_json(&saved).unwrap();
        assert_eq!(reopened.bonds, doc.bonds);
        assert_eq!(reopened.models, doc.models);
        let (figure, notice) = crate::export::figure_document(&engine, doc.clone())
            .await
            .unwrap();
        assert_eq!(figure, doc);
        assert!(notice.is_none());
        for format in ["svg", "png", "pdf"] {
            let bytes = crate::export::drawing(&figure, format).unwrap();
            assert!(!bytes.is_empty(), "{format}");
        }
    }
}

#[tokio::test]
async fn disconnected_ordinary_component_still_receives_real_molecular_analysis() {
    if std::env::var_os("RESHIKI_INCHI_HELPER").is_none() {
        assert!(std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none());
        eprintln!("Set RESHIKI_INCHI_HELPER for ordinary-component analysis coverage");
        return;
    }
    let mut mixed = distance_pair();
    let oxygen = mixed.add_atom("O", Point::new(200., 0.));
    assert!(!crystal::touches(&mixed, &[oxygen]));
    let ordinary = crate::editing::analysis_document(&mixed, &[oxygen]);
    let response = execute(Request::molecule("analyze", ordinary))
        .await
        .unwrap();
    let analysis = response.analysis.unwrap();
    assert_eq!(analysis.smiles, "O");
    assert_eq!(analysis.formula, "H2O");
    assert_eq!(analysis.inchi, "InChI=1S/H2O/h1H2");
    assert!(response.warnings.is_empty());
    assert!(models3d::crystal::touches(&mixed, &[]));
}
