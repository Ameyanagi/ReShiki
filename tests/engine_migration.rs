//! Differential chemistry/figure tests guard the boundary between local Rust
//! conversion and the retained RDKit backend. Neither oracle uses the new codec.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

type TestResult = Result<(), Box<dyn Error>>;

fn assert_response_matches(actual: Response, expected: Response) -> TestResult {
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    if let (Some(a), Some(e)) = (
        actual["analysis"].as_object_mut(),
        expected["analysis"].as_object_mut(),
    ) {
        for field in ["mass", "exact_mass"] {
            let av = a
                .remove(field)
                .and_then(|v| v.as_f64())
                .ok_or("Missing mass")?;
            let ev = e
                .remove(field)
                .and_then(|v| v.as_f64())
                .ok_or("Missing reference mass")?;
            // RDKit wheels may fuse floating-point operations on some targets.
            assert!(
                (av - ev).abs() <= ev.abs().max(1.) * 1e-12,
                "{field}: {av} != {ev}"
            );
        }
    }
    assert_eq!(actual, expected);
    Ok(())
}

#[tokio::test]
async fn rust_properties_match_reference_across_editor_operations() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for smiles in [
        "CN",
        "[2H]O[3H]",
        "[NH4+]",
        "[Fe+3].[Cl-].[Cl-].[Cl-]",
        "[CH3]",
        "*CC",
    ] {
        let request = Request::import_smiles(smiles);
        assert_response_matches(
            local.execute(request.clone()).await?,
            reference.execute(request).await?,
        )?;
    }
    let document = reference
        .execute(Request::import_smiles("COc1ccccc1"))
        .await?
        .document
        .ok_or("Missing reference document")?;
    for operation in ["analyze", "clean", "aromatic", "abbreviate"] {
        let mut request = Request::molecule(operation, document.clone());
        request.selected_ids = Some(document.atoms.iter().map(|a| a.id).collect());
        if operation == "abbreviate" {
            request.text = Some("OMe".into());
        }
        assert_response_matches(
            local.execute(request.clone()).await?,
            reference.execute(request).await?,
        )?;
    }
    for format in ["smiles", "inchi", "mol", "cdxml", "cdx"] {
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        assert_response_matches(
            local.execute(request.clone()).await?,
            reference.execute(request).await?,
        )?;
    }
    let request = Request::import("rsmi", "[CH3:1][OH:2]>>[CH2:1]=[O:2]");
    assert_response_matches(
        local.execute(request.clone()).await?,
        reference.execute(request).await?,
    )?;
    // Cached labels must not affect properties following a topology edit.
    let mut document = reference
        .execute(Request::import_smiles("CC"))
        .await?
        .document
        .ok_or("Missing reference document")?;
    for atom in &mut document.atoms {
        atom.label_h = 99;
    }
    for bond in &mut document.bonds {
        bond.order = 2;
    }
    let request = Request::molecule("analyze", document);
    let actual = local.execute(request.clone()).await?;
    assert_eq!(
        actual.analysis.as_ref().map(|a| a.formula.as_str()),
        Some("C2H4")
    );
    assert_response_matches(actual, reference.execute(request).await?)?;
    Ok(())
}

#[tokio::test]
async fn native_drawings_match_the_original_python_importer() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for data in [
        include_bytes!("fixtures/native-ethyl-clipboard.cdx").as_slice(),
        include_bytes!("fixtures/abbreviations-native.cdx").as_slice(),
        include_bytes!("fixtures/picture-group-native.cdx").as_slice(),
    ] {
        let request = Request::import("cdx", &STANDARD.encode(data));
        let expected = reference.execute(request.clone()).await?;
        let actual = local.execute(request).await?;
        assert_response_matches(actual, expected)?;
    }
    Ok(())
}

#[tokio::test]
async fn exports_and_reimports_preserve_rdkit_chemistry_and_document_metadata() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for smiles in [
        "CCO",
        "N[C@@H](C)C(=O)O",
        "N[C@H](C)C(=O)O",
        "[13CH3][NH3+]",
        "[Na+].[Cl-]",
        "F/C=C/F",
        "F/C=C\\F",
        "[CH3]",
        "c1ccc2occc2c1",
        "C1CC2CCC1C2",
    ] {
        let initial = reference.execute(Request::import_smiles(smiles)).await?;
        let analysis = initial.analysis.ok_or("Missing reference analysis")?;
        let mut document = initial.document.ok_or("Missing reference drawing")?;
        for bond in &mut document.bonds {
            bond.color = [180, 50, 55];
        }
        let mut request = Request::molecule("export", document);
        request.format = Some("cdx".into());
        let expected = reference.execute(request.clone()).await?;
        let actual = local.execute(request).await?;
        assert_response_matches(actual.clone(), expected)?;
        let data = actual.output.ok_or("Missing binary export")?;
        let request = Request::import("cdx", &data);
        let back = local.execute(request.clone()).await?;
        let oracle = reference.execute(request).await?;
        assert_response_matches(back.clone(), oracle)?;
        let identity = back.analysis.ok_or("Missing round-trip analysis")?;
        assert_eq!(analysis.smiles, identity.smiles, "{smiles}");
        assert_eq!(analysis.inchikey, identity.inchikey, "{smiles}");
        assert_eq!(analysis.formula, identity.formula, "{smiles}");
        assert_eq!(analysis.exact_mass, identity.exact_mass, "{smiles}");
    }
    Ok(())
}

#[derive(Clone, Default)]
struct RecordingBackend {
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
}
impl ChemistryEngine for RecordingBackend {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        self.requests
            .lock()
            .map_err(|_| "Poisoned test lock")?
            .push(serde_json::to_value(&request).map_err(|e| e.to_string())?);
        Ok(Response {
            document: None,
            analysis: None,
            output: Some("<CDXML><page id=\"1\"/></CDXML>".into()),
            engine_version: "reference".into(),
            warnings: vec!["kept".into()],
        })
    }
}
#[tokio::test]
async fn backend_receives_cdxml_and_unchanged_chemistry_requests() -> TestResult {
    let backend = RecordingBackend::default();
    let local = LocalEngine::with_backend(backend.clone());
    let input = "<CDXML><page id=\"1\"/></CDXML>";
    let bytes = reshiki::exchange::to_cdx(input)?;
    local
        .execute(Request::import("cdx", &STANDARD.encode(&bytes)))
        .await?;
    let mut export = Request::molecule("export", Default::default());
    export.format = Some("cdx".into());
    let out = local.execute(export).await?;
    assert_eq!(out.output, Some(STANDARD.encode(bytes)));
    assert_eq!(out.warnings, vec!["kept"]);
    let request = Request::import_smiles("N[C@@H](C)C(=O)O");
    local.execute(request.clone()).await?;
    let requests = backend.requests.lock().map_err(|_| "Poisoned test lock")?;
    assert_eq!(
        requests
            .first()
            .and_then(|r| r.get("format"))
            .and_then(|v| v.as_str()),
        Some("cdxml")
    );
    assert_eq!(
        requests
            .get(1)
            .and_then(|r| r.get("format"))
            .and_then(|v| v.as_str()),
        Some("cdxml")
    );
    assert_eq!(requests.get(2), Some(&serde_json::to_value(request)?));
    Ok(())
}

#[tokio::test]
async fn malformed_binary_never_reaches_backend_and_next_request_succeeds() -> TestResult {
    let backend = RecordingBackend::default();
    let local = LocalEngine::with_backend(backend.clone());
    for text in ["", "!!!", "VmpDRDAxMDA=", "A"] {
        assert!(local.execute(Request::import("cdx", text)).await.is_err());
    }
    let mut invalid = Request::import_smiles("CCO");
    invalid.protocol = 2;
    assert!(local.execute(invalid).await.is_err());
    assert!(
        backend
            .requests
            .lock()
            .map_err(|_| "Poisoned test lock")?
            .is_empty()
    );
    local.execute(Request::import_smiles("CCO")).await?;
    assert_eq!(
        backend
            .requests
            .lock()
            .map_err(|_| "Poisoned test lock")?
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn supported_figure_exports_are_byte_identical_to_python() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for name in [
        "bond-styles-chemdraw",
        "formatted-label-chemdraw",
        "graphics-chemdraw",
        "symbols-chemdraw",
        "arrows-chemdraw",
        "atom-labels-chemdraw",
        "attached-symbols-chemdraw",
        "grouped-aspirin-chemdraw",
        "ring-presets-chemdraw",
        "aromatic-circle-native",
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}.cdxml",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let doc = reference
            .execute(Request::import("cdxml", &xml))
            .await?
            .document
            .ok_or("Missing drawing")?;
        let mut request = Request::molecule("export", doc);
        request.format = Some("cdx".into());
        let expected = reference.execute(request.clone()).await?;
        let actual = local.execute(request).await?;
        assert_response_matches(actual, expected)?;
    }
    Ok(())
}

#[tokio::test]
async fn query_predicates_still_fail_chemistry_validation() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for (name, value) in [
        ("RingBondCount", "NoRingBonds"),
        ("SubstituentsExactly", "0"),
        ("RxnStereo", "Inversion"),
    ] {
        let xml = format!(
            "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\" Element=\"6\" {name}=\"{value}\"/></fragment></page></CDXML>"
        );
        let bytes = reshiki::exchange::to_cdx(&xml)?;
        let request = Request::import("cdx", &STANDARD.encode(bytes));
        let expected = reference.execute(request.clone()).await;
        let actual = local.execute(request).await;
        assert!(expected.is_err());
        assert!(actual.is_err());
        assert_eq!(expected.err(), actual.err());
    }
    Ok(())
}
