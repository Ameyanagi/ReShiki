use super::{
    RDKIT_VERSION, cdxml, depict, document, inchi::key, molfile, smiles, stereo::cip::label,
    stereo_lint,
};
use crate::{
    document::Document,
    engine::{LocalEngine, Request, native_import, native_response},
    exchange::drawing,
};

fn identity(molecule: &document::Molecule) -> anyhow::Result<String> {
    Ok(smiles::write::write(&molecule.state, Default::default())?.text)
}

async fn round_trip(text: &str) -> anyhow::Result<()> {
    let input = smiles::read(text)?;
    let expected = smiles::write::write(&input.prepared.state, Default::default())?.text;
    let labels = label::assign(&input.prepared.state, &Default::default())?;
    let expected_code = labels
        .state
        .metadata
        .atoms
        .iter()
        .zip(&labels.state.properties.atoms)
        .find(|(meta, _)| meta.chiral_tag == 5)
        .and_then(|(_, properties)| properties.cip_code.as_deref())
        .unwrap();
    let positions = depict::compute(
        &input.prepared.state,
        &vec![None; input.prepared.state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let molecule = document::Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        positions,
        state: input.prepared.state,
    };
    let drawing =
        document::for_generated(&molecule, &input.prepared.dummy_labels, Default::default())?;
    let labels = drawing.labels()?;
    let generated = drawing.finish(labels)?;
    assert!(
        generated
            .atoms
            .iter()
            .any(|atom| atom.cip_label.as_deref() == Some(expected_code)),
        "{text}"
    );
    // Allene winding is stored as terminal stereobonds. Serialization and
    // graph reconstruction must recover it without a central AtomStereo.
    let stored: Document = serde_json::from_str(&serde_json::to_string(&generated)?)?;
    let molecule = document::prepare(&stored)?;
    assert_eq!(identity(&molecule)?, expected, "stored drawing: {text}");
    for force_v3000 in [false, true] {
        let output = molfile::write(&molecule, molfile::Options { force_v3000 })?;
        assert_eq!(
            identity(&molfile::read(&output)?.molecule)?,
            expected,
            "MOL v3000={force_v3000}: {text}"
        );
    }
    let xml = drawing::write(&stored, Default::default())?;
    let reopened = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&xml)?)?.into_document()?;
    assert_eq!(identity(&reopened.molecule)?, expected, "CDXML: {text}");
    assert_eq!(
        identity(&document::prepare(&reopened.document)?)?,
        expected,
        "CDXML drawing: {text}"
    );
    // The actual app import additionally runs the native layout refinements
    // and molecular analysis before publishing its editable document.
    let response = LocalEngine::in_process()
        .request(Request::import("smiles", text))
        .await
        .map_err(anyhow::Error::msg)?;
    let analysis = response.analysis.unwrap();
    assert_eq!(analysis.smiles, expected);
    assert!(analysis.inchi.contains("/t"), "{text}: {}", analysis.inchi);
    assert_eq!(analysis.inchikey, key::from_inchi(&analysis.inchi)?);
    if text.starts_with("CC=") {
        let inversion = if expected_code == "M" { 0 } else { 1 };
        assert_eq!(
            analysis.inchi,
            format!("InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m{inversion}/s1"),
            "{text}"
        );
        assert_eq!(
            analysis.inchikey,
            if expected_code == "M" {
                "PODAMDNJNMAKAZ-YFKPBYRVSA-N"
            } else {
                "PODAMDNJNMAKAZ-RXMQYKEDSA-N"
            }
        );
    }
    assert!(response.warnings.is_empty(), "{:?}", response.warnings);
    assert_eq!(
        identity(&document::prepare(&response.document.unwrap())?)?,
        expected,
        "app import: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn generated_allene_smiles_survives_stored_document_mol_and_cdxml() -> anyhow::Result<()> {
    for text in ["CC=[C@AL1]=CC", "CC=[C@AL2]=CC", "CC=[C@]=CC"] {
        round_trip(text).await?;
    }
    Ok(())
}

#[tokio::test]
async fn generated_allene_and_adjacent_ring_stereocenters_both_survive() -> anyhow::Result<()> {
    for text in [
        "C[C@H]1CCC[C@@H]1C=[C@AL1]=CC",
        "C[C@H]1CCC[C@@H]1C=[C@AL2]=CC",
    ] {
        round_trip(text).await?;
    }
    Ok(())
}

#[tokio::test]
async fn allene_app_analysis_and_exports_keep_available_identifiers() -> anyhow::Result<()> {
    let outcome = native_import::execute(
        Request::import("smiles", "CC=[C@AL1]=CC"),
        Some(native_response::Config::in_process()),
    )
    .await?;
    let native_import::Outcome::Complete(imported) = outcome else {
        panic!("Allene import unexpectedly deferred");
    };
    let analysis = imported.analysis.unwrap();
    let expected = analysis.smiles;
    let expected_inchi = "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1";
    assert_eq!(analysis.inchi, expected_inchi);
    let expected_key = key::from_inchi(expected_inchi)?;
    assert_eq!(expected_key, "PODAMDNJNMAKAZ-YFKPBYRVSA-N");
    assert_eq!(analysis.inchikey, expected_key);
    let source = imported.document.unwrap();
    let engine = LocalEngine::in_process();
    for format in [
        None,
        Some("smiles"),
        Some("mol"),
        Some("cdxml"),
        Some("inchi"),
    ] {
        let mut request = Request::molecule(
            if format.is_some() {
                "export"
            } else {
                "analyze"
            },
            source.clone(),
        );
        request.format = format.map(str::to_owned);
        let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
        let analysis = response.analysis.unwrap();
        assert_eq!(analysis.smiles, expected);
        assert_eq!(analysis.formula, "C5H8");
        assert_eq!(analysis.inchi, expected_inchi);
        assert_eq!(analysis.inchikey, expected_key);
        assert!(response.warnings.is_empty(), "{:?}", response.warnings);
        if let Some(format) = format {
            let output = response.output.unwrap();
            match format {
                "smiles" => assert_eq!(output.trim(), expected),
                "inchi" => assert_eq!(output.trim(), expected_inchi),
                "mol" => assert_eq!(identity(&molfile::read(&output)?.molecule)?, expected),
                "cdxml" => {
                    let imported =
                        cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&output)?)?.into_document()?;
                    assert_eq!(identity(&imported.molecule)?, expected);
                }
                _ => unreachable!(),
            }
        }
    }
    let rejected = native_import::execute(
        Request::import("smiles", "CC=[C@AL1]=CC"),
        Some(native_response::Config::new(std::path::PathBuf::new())),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        rejected,
        native_import::Error::Analysis(native_response::Error::Helper(_))
    ));
    Ok(())
}

#[tokio::test]
async fn projected_allene_keeps_analysis_without_inventing_inchi_geometry() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let response = engine
        .request(Request::import("smiles", "CC=[C@AL1]=CC"))
        .await
        .map_err(anyhow::Error::msg)?;
    let expected = response.analysis.unwrap().smiles;
    let mut source = response.document.unwrap();
    source
        .bonds
        .iter_mut()
        .find(|bond| bond.order == 2)
        .unwrap()
        .projection = true;
    let response = engine
        .request(Request::molecule("analyze", source.clone()))
        .await
        .map_err(anyhow::Error::msg)?;
    let analysis = response.analysis.unwrap();
    assert_eq!(analysis.smiles, expected);
    assert!(analysis.inchi.is_empty());
    assert!(analysis.inchikey.is_empty());
    assert_eq!(response.warnings, [native_response::ALLENE_INCHI_NOTICE]);
    let mut request = Request::molecule("export", source);
    request.format = Some("inchi".into());
    let error = engine.request(request).await.unwrap_err();
    assert!(
        error.contains(native_response::ALLENE_INCHI_NOTICE),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn allene_wedges_at_both_ends_do_not_acquire_an_inchi_configuration() -> anyhow::Result<()> {
    let xml = r#"<CDXML BondLength="28" ShowAtomStereo="yes">
<page id="1"><fragment id="2">
<n id="3" p="44 72" Element="6"/>
<n id="4" p="72 100" Element="6"/>
<n id="5" p="100 100" Element="6"/>
<n id="6" p="128 100" Element="6"/>
<n id="7" p="156 72" Element="6"/>
<n id="8" p="44 128" Element="1"/>
<n id="9" p="156 128" Element="1"/>
<b id="10" B="4" E="3" Order="1" Display="WedgeBegin"/>
<b id="11" B="4" E="5" Order="2"/>
<b id="12" B="5" E="6" Order="2"/>
<b id="13" B="6" E="7" Order="1" Display="WedgedHashBegin"/>
<b id="14" B="4" E="8" Order="1" Display="WedgedHashBegin"/>
<b id="15" B="6" E="9" Order="1"/>
</fragment></page></CDXML>"#;
    let engine = LocalEngine::in_process();
    let imported = engine
        .request(Request::import("cdxml", xml))
        .await
        .map_err(anyhow::Error::msg)?;
    let source = imported.document.unwrap();
    assert!(source.atoms.iter().all(|atom| atom.cip_label.is_none()));
    assert_eq!(
        document::prepare(&source)?.state.metadata.atoms[2].chiral_tag,
        0
    );
    assert!(
        stereo_lint::check(&source, false)
            .iter()
            .any(|note| note.kind == stereo_lint::Kind::AlleneBothEnds)
    );
    let analyzed = engine
        .request(Request::molecule("analyze", source))
        .await
        .map_err(anyhow::Error::msg)?;
    for analysis in [imported.analysis.unwrap(), analyzed.analysis.unwrap()] {
        assert!(!analysis.smiles.contains('@'), "{}", analysis.smiles);
        assert_eq!(analysis.inchi, "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3");
        assert!(!analysis.inchi.contains("/t"));
    }
    assert!(
        analyzed
            .document
            .unwrap()
            .atoms
            .iter()
            .all(|atom| atom.cip_label.is_none())
    );
    Ok(())
}
