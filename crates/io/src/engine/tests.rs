//! Custom backends receive unmodified requests without entering native dispatch.
use super::*;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn owned_and_shared_native_imports_preserve_snapshots_and_errors() {
    for text in ["<CDXML><page id=\"1\"/></CDXML>", "<invalid"] {
        let request = Request::import("cdxml", text);
        let expected = serde_json::to_value(&request).unwrap();
        let source = Arc::new(request.clone());
        let owned = native_import::execute(request, None).await;
        let shared = native_import::execute(source.clone(), None).await;
        match (owned, shared) {
            (
                Ok(native_import::Outcome::Complete(owned)),
                Ok(native_import::Outcome::Complete(shared)),
            ) => {
                assert_eq!(
                    serde_json::to_value(owned).unwrap(),
                    serde_json::to_value(shared).unwrap()
                );
            }
            (Err(owned), Err(shared)) => assert_eq!(owned.to_string(), shared.to_string()),
            other => panic!("Owned/shared import outcomes differ: {other:?}"),
        }
        assert_eq!(serde_json::to_value(source.as_ref()).unwrap(), expected);
        assert_eq!(Arc::strong_count(&source), 1);
    }
}

#[test]
#[ignore = "isolated requested-Rust-allocation and transfer timing measurement"]
fn measure_owned_request_transfers() {
    use crate::allocation_metrics;
    use std::{hint::black_box, time::Instant};
    fn request() -> Request {
        let mut request = Request::import("cdxml", &"x".repeat(2 * 1024 * 1024));
        let mut document = Document::default();
        for i in 0..10_000 {
            document.add_atom("C", crate::document::Point::new(i as f32, 0.));
        }
        request.selected_ids = Some(document.atoms.iter().map(|a| a.id).collect());
        request.document = Some(document);
        request
    }
    for mode in [
        "legacy_import",
        "unique_import",
        "shared_import",
        "legacy_export",
        "moved_export",
    ] {
        let mut request = request();
        if mode.ends_with("import") {
            let request = Arc::new(request);
            let caller = (mode == "shared_import").then(|| request.clone());
            let baseline = allocation_metrics::reset();
            let start = Instant::now();
            let prepared = if mode == "legacy_import" {
                (*request).clone()
            } else {
                Arc::unwrap_or_clone(request)
            };
            let elapsed = start.elapsed();
            let snapshot = allocation_metrics::snapshot();
            black_box((&prepared, &caller));
            println!(
                "mode={mode} elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
                snapshot.allocation_count,
                snapshot.allocated_bytes,
                snapshot.peak_bytes.saturating_sub(baseline),
                snapshot.live_bytes.saturating_sub(baseline)
            );
        } else {
            let baseline = allocation_metrics::reset();
            let start = Instant::now();
            let (document, selected) = if mode == "legacy_export" {
                (request.document.clone(), request.selected_ids.clone())
            } else {
                (request.document.take(), request.selected_ids.take())
            };
            let elapsed = start.elapsed();
            let snapshot = allocation_metrics::snapshot();
            black_box((&document, &selected, &request));
            println!(
                "mode={mode} elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
                snapshot.allocation_count,
                snapshot.allocated_bytes,
                snapshot.peak_bytes.saturating_sub(baseline),
                snapshot.live_bytes.saturating_sub(baseline)
            );
        }
    }
}

#[derive(Clone, Default)]
struct DeferredBackend(Arc<Mutex<Option<serde_json::Value>>>);

impl ChemistryEngine for DeferredBackend {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        *self.0.lock().await = Some(serde_json::to_value(request).map_err(|e| e.to_string())?);
        Ok(Response {
            document: None,
            analysis: None,
            output: None,
            engine_version: "deferred test backend".into(),
            warnings: vec![],
        })
    }
}

#[tokio::test]
async fn custom_backends_preserve_the_original_request() -> anyhow::Result<()> {
    let backend = DeferredBackend::default();
    let local = LocalEngine::with_backend(backend.clone());
    for (format, text) in [
        ("smiles", " \t C[C@H](O)F\r\n"),
        ("inchi", " \t InChI=1S/CH4/h1H4\r\n"),
        ("rsmi", " \t C>>O\r\n"),
    ] {
        let mut request = Request::import(format, text);
        request.selected_ids = Some(vec![9_007_199_254_740_993, 4]);
        request.text_layout = Some(std::collections::HashMap::from([(
            7,
            TextMetrics {
                width: 0.1,
                height: -0.2,
                baseline: 0.3,
            },
        )]));
        request.document = Some(Document::default());
        let expected = serde_json::to_value(&request)?;
        let result = local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(result.engine_version == "deferred test backend");
        anyhow::ensure!(backend.0.lock().await.as_ref() == Some(&expected));
        anyhow::ensure!(serde_json::to_value(request)? == expected);
    }
    Ok(())
}

#[tokio::test]
async fn labels_match_full_analysis_for_aromatic_charged_radical_and_stereo_input() {
    let engine = LocalEngine::default();
    for smiles in [
        "CCO",
        "N",
        "[NH4+]",
        "C[O-]",
        "[OH]",
        "[13CH3]O",
        "c1cc[nH]c1",
        "c1ncccc1",
        "OP(=O)(O)O",
        "CS(=O)(=O)O",
        "[2H]O",
        "C[N+](=O)[O-]",
        "C[C@H](O)F",
        "F/C=C/F",
        "F/C=C\\F",
        "C[C@H](O)F.C[C@@H](O)F",
    ] {
        let source = engine
            .request(Request::import_smiles(smiles))
            .await
            .unwrap()
            .document
            .unwrap();
        let checked = engine
            .request(Request::molecule("analyze", source.clone()))
            .await
            .unwrap()
            .document
            .unwrap();
        let result = crate::atom_labels::refresh::Refresh::calculate(
            &source,
            &crate::atom_labels::refresh::Refresh::default(),
        )
        .unwrap();
        assert!(result.notice.is_none(), "{smiles}: {:?}", result.notice);
        let mut actual = source.clone();
        let mut expected = source.clone();
        result.apply(&mut actual);
        crate::atom_labels::refresh_computed(&mut expected, &checked);
        assert_eq!(actual, expected, "{smiles}");
    }
}

/// Mirrors the app's clipboard table (src/clipboard/tests.rs).
#[test]
fn typed_text_formats_follow_cheap_markers() {
    for (text, format) in [
        ("  InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3", "inchi"),
        ("$RXN\n\n  ReShiki\n\n  1  1\n$MOL\nM  END", "rxn"),
        (
            "ethanol\n\n\n  3  2  0  0  0  0  0  0  0  0999 V2000\nM  END",
            "mol",
        ),
        ("\n\n\n  0  0  0     0  0            999 V3000\n", "mol"),
        ("<?xml version=\"1.0\"?><CDXML><page/></CDXML>", "cdxml"),
        ("CCO>>CC=O", "rsmi"),
        ("CCO>O=O>CC=O", "rsmi"),
        ("C->C", "smiles"),
        ("c1ccccc1", "smiles"),
    ] {
        assert_eq!(text_format(text), format, "{text}");
    }
}
