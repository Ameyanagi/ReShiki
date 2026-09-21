//! Differential chemistry/figure tests guard the boundary between local Rust
//! conversion and the retained RDKit backend. Neither oracle uses the new codec.
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response};
use std::sync::{Arc, Mutex};

type TestResult = anyhow::Result<()>;

fn assert_response_matches(actual: Response, expected: Response) -> TestResult {
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    if let (Some(a), Some(e)) = (
        actual["analysis"].as_object_mut(),
        expected["analysis"].as_object_mut(),
    ) {
        for field in ["mass", "exact_mass", "logp", "tpsa"] {
            let av = a
                .remove(field)
                .and_then(|v| v.as_f64())
                .context("Missing mass")?;
            let ev = e
                .remove(field)
                .and_then(|v| v.as_f64())
                .context("Missing reference mass")?;
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
        "C12C3C4C1C5C2C3C45", // Cubane: six symmetric rings, five basis rings.
        "C1CCC2(CC1)CCCC2",   // Spiro junction.
        "c1ccc2occc2c1",      // Fused aromatic rings.
        "CC(=O)N",            // Exclude the amide nitrogen from acceptors.
        "O1CC1",              // Three-membered-ring polar surface contribution.
        "CS(=O)(=O)N",        // Sulfur and nitrogen descriptor rules.
    ] {
        let request = Request::import_smiles(smiles);
        assert_response_matches(
            local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
    }
    let document = reference
        .execute(Request::import_smiles("COc1ccccc1"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reference document")?;
    for operation in ["analyze", "clean", "aromatic", "abbreviate"] {
        let mut request = Request::molecule(operation, document.clone());
        request.selected_ids = Some(document.atoms.iter().map(|a| a.id).collect());
        if operation == "abbreviate" {
            request.text = Some("OMe".into());
        }
        assert_response_matches(
            local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
    }
    for format in ["smiles", "inchi", "mol", "cdxml", "cdx"] {
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        assert_response_matches(
            local
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?,
            reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?,
        )?;
    }
    let request = Request::import("rsmi", "[CH3:1][OH:2]>>[CH2:1]=[O:2]");
    assert_response_matches(
        local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    // Cached labels must not affect properties following a topology edit.
    let mut document = reference
        .execute(Request::import_smiles("CC"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reference document")?;
    for atom in &mut document.atoms {
        atom.label_h = 99;
    }
    for bond in &mut document.bonds {
        bond.order = 2;
    }
    let request = Request::molecule("analyze", document);
    let actual = local
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        actual.analysis.as_ref().map(|a| a.formula.as_str()),
        Some("C2H4")
    );
    assert_response_matches(
        actual,
        reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    Ok(())
}

#[tokio::test]
async fn rust_ring_display_preserves_selected_scope_and_complete_response() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    for text in [
        "c1ccccc1.CCO",
        "c1cc[nH]c1",
        "c1ncc[nH]1",
        "c1ccc2ccccc2c1",
        "c1ccc2[nH]ccc2c1",
        "[13cH:90]1ccccc1.[2H]O[3H]",
        "C[C@H](O)c1ccccc1",
        "c1ccccc1/C=C/Cl",
    ] {
        let mut original = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing ring drawing")?;
        for bond in &mut original.bonds {
            bond.color = [25, 60, 190];
        }
        original.annotations.push(reshiki::document::Annotation {
            id: 987654,
            position: reshiki::document::Point::new(10., 100.),
            text: "試料".into(),
            format: Default::default(),
        });
        let chemical = reshiki::chemistry::document::prepare(&original)?;
        let ring = chemical
            .state
            .rings
            .atoms
            .first()
            .context("Missing aromatic ring")?
            .iter()
            .map(|&i| chemical.ids.get(i).copied().context("Missing ring atom"))
            .collect::<anyhow::Result<Vec<_>>>()?;
        for selected in [original.atoms.iter().map(|a| a.id).collect(), ring] {
            let mut document = original.clone();
            for _ in 0..2 {
                let mut request = Request::molecule("aromatic", document);
                request.selected_ids = Some(selected.clone());
                let expected = reference
                    .execute(request.clone())
                    .await
                    .map_err(anyhow::Error::msg)?;
                let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
                document = actual.document.clone().context("Missing ring display")?;
                assert_response_matches(actual, expected)?;
            }
        }
        for selected in [vec![], vec![987654]] {
            let mut request = Request::molecule("aromatic", original.clone());
            request.selected_ids = Some(selected);
            assert!(reference.execute(request.clone()).await.is_err());
            assert!(local.execute(request).await.is_err());
        }
    }
    Ok(())
}

#[tokio::test]
async fn rust_prepared_drawings_preserve_full_analysis_and_molecular_exports() -> TestResult {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let templates: serde_json::Value =
        serde_json::from_str(include_str!("../assets/templates.json"))?;
    let mut texts: Vec<_> = templates
        .as_array()
        .context("Missing templates")?
        .iter()
        .filter_map(|t| t["smiles"].as_str())
        .collect();
    texts.extend([
        "[2H]O[3H]",
        "[13CH3][NH3+]",
        "[CH3]",
        "[CH2]",
        "[O]",
        "[Na+].[Cl-]",
        "F/C=C/F",
        "F/C=C\\F",
        "F/C=C/C=C/Cl",
        "F[C@](Cl)(Br)I",
        "C[C@H]1CCC[C@@H](C)C1",
        "C[S@](=O)CC",
        "C1CCC2(CC1)CCCC2",
        "C12C3C4C1C5C2C3C45",
        "[NH3]->[Cu+2]<-[NH3]",
        "[CH3:1][OH:9]",
    ]);
    for text in texts {
        let mut doc = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reference drawing")?;
        // Reordering changes toolkit indices; stable IDs and explicit winding
        // must survive, while stale display labels must have no chemical effect.
        if text == "[CH3:1][OH:9]" {
            doc.atoms.last_mut().context("Missing mapped atom")?.map_num = i32::MAX as u32;
        }
        doc.atoms.reverse();
        doc.bonds.reverse();
        for a in &mut doc.atoms {
            a.position.x = -a.position.x;
            a.label_h = 99;
        }
        for b in &mut doc.bonds {
            b.color = [75, 125, 160];
        }
        for format in [None, Some("smiles"), Some("mol")] {
            let mut request = Request::molecule(
                if format.is_some() {
                    "export"
                } else {
                    "analyze"
                },
                doc.clone(),
            );
            request.format = format.map(String::from);
            let expected = reference.execute(request.clone()).await;
            let actual = local.execute(request).await;
            match (actual, expected) {
                (Ok(a), Ok(e)) => {
                    assert_response_matches(a, e).with_context(|| format!("{text}/{format:?}"))?
                }
                (Err(_), Err(_)) => anyhow::bail!("Unexpected rejection of {text}/{format:?}"),
                (a, e) => anyhow::bail!("Response mismatch for {text}/{format:?}: {a:?} vs {e:?}"),
            }
        }
    }
    // A chemistry failure must leave the input unchanged and the next request usable.
    let mut invalid = reshiki::document::Document::default();
    let center = invalid.add_atom("C", Default::default());
    for i in 0..5 {
        let other = invalid.add_atom("F", reshiki::document::Point::new(i as f32 * 28., 28.));
        invalid.add_bond(center, other, 1, "plain");
    }
    let before = invalid.clone();
    assert!(
        local
            .execute(Request::molecule("analyze", invalid.clone()))
            .await
            .is_err()
    );
    assert_eq!(before, invalid);
    let valid = reference
        .execute(Request::import_smiles("CO"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing valid drawing")?;
    let req = Request::molecule("analyze", valid);
    assert_response_matches(
        local
            .execute(req.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        reference.execute(req).await.map_err(anyhow::Error::msg)?,
    )?;
    Ok(())
}

#[tokio::test]
async fn ambiguous_ring_pruning_keeps_the_complete_reference_analysis() -> TestResult {
    use reshiki::{chemistry::graph::Graph, document::Document};
    let graph: Graph = serde_json::from_str(include_str!("fixtures/ring-order-dependent.json"))?;
    let mut document = Document::default();
    let ids = (0..graph.atoms.len())
        .map(|i| {
            document.add_atom(
                "*",
                reshiki::document::Point {
                    x: i as f32 * 42.,
                    y: 0.,
                },
            )
        })
        .collect::<Vec<_>>();
    for bond in &graph.bonds {
        document.add_bond(ids[bond.a], ids[bond.b], 1, "plain");
    }
    let request = Request::molecule("analyze", document);
    assert_response_matches(
        LocalEngine::default()
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?,
        PythonEngine::default()
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
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
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
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
        let initial = reference
            .execute(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = initial.analysis.context("Missing reference analysis")?;
        let mut document = initial.document.context("Missing reference drawing")?;
        for bond in &mut document.bonds {
            bond.color = [180, 50, 55];
        }
        let mut request = Request::molecule("export", document);
        request.format = Some("cdx".into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
        assert_response_matches(actual.clone(), expected)?;
        let data = actual.output.context("Missing binary export")?;
        let request = Request::import("cdx", &data);
        let back = local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let oracle = reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?;
        assert_response_matches(back.clone(), oracle)?;
        let identity = back.analysis.context("Missing round-trip analysis")?;
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
    let bytes = reshiki::exchange::to_cdx(input).map_err(anyhow::Error::msg)?;
    local
        .execute(Request::import("cdx", &STANDARD.encode(&bytes)))
        .await
        .map_err(anyhow::Error::msg)?;
    let mut export = Request::molecule("export", Default::default());
    export.format = Some("cdx".into());
    let out = local.execute(export).await.map_err(anyhow::Error::msg)?;
    assert_eq!(out.output, Some(STANDARD.encode(bytes)));
    assert_eq!(out.warnings, vec!["kept"]);
    let request = Request::import_smiles("N[C@@H](C)C(=O)O");
    local
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let requests = backend
        .requests
        .lock()
        .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?;
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
            .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?
            .is_empty()
    );
    local
        .execute(Request::import_smiles("CCO"))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        backend
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?
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
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing drawing")?;
        if !doc.atoms.is_empty() {
            let analyze = Request::molecule("analyze", doc.clone());
            assert_response_matches(
                local
                    .execute(analyze.clone())
                    .await
                    .map_err(anyhow::Error::msg)?,
                reference
                    .execute(analyze)
                    .await
                    .map_err(anyhow::Error::msg)?,
            )?;
        }
        let mut request = Request::molecule("export", doc);
        request.format = Some("cdx".into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
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
        let bytes = reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
        let request = Request::import("cdx", &STANDARD.encode(bytes));
        let expected = reference.execute(request.clone()).await;
        let actual = local.execute(request).await;
        assert!(expected.is_err());
        assert!(actual.is_err());
        assert_eq!(expected.err(), actual.err());
    }
    Ok(())
}
