use super::*;
use crate::chemistry::{cdxml, depict};

fn drawing(text: &str) -> anyhow::Result<Document> {
    let input = smiles::read(text)?;
    let state = input.prepared.state;
    let positions = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let molecule = molecular::Molecule {
        rdkit_version: chemistry::RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        positions,
        state,
    };
    let draft = molecular::for_import(&molecule, false, &input.prepared.dummy_labels)?;
    let labels = draft.labels()?;
    Ok(draft.finish(labels)?)
}

fn groups(doc: &Document) -> Vec<Option<crate::stereo_groups::StereoGroup>> {
    doc.atoms.iter().map(|atom| atom.stereo_group).collect()
}

#[tokio::test]
async fn stereo_groups_analysis_and_inchi_export_need_no_helper() -> anyhow::Result<()> {
    for text in ["C[C@H](N)C(=O)O |&1:1|", "C[C@H](O)[C@H](F)Cl |o1:1,3|"] {
        let doc = drawing(text)?;
        let response = execute(Request::molecule("analyze", doc.clone())).await?;
        assert!(response.warnings.is_empty());
        assert_eq!(groups(response.document.as_ref().unwrap()), groups(&doc));
        let analysis = response.analysis.unwrap();
        assert!(analysis.inchi.is_empty());
        assert!(analysis.inchikey.is_empty());
        assert!(!analysis.formula.is_empty());
        assert!(analysis.mass > 0.);
        assert!(analysis.smiles.contains(" |"));
        assert!(
            response
                .document
                .unwrap()
                .atoms
                .iter()
                .any(|atom| atom.cip_label.is_some())
        );
        let mut request = Request::molecule("export", doc);
        request.format = Some("inchi".into());
        assert_eq!(
            execute(request).await.unwrap_err().to_string(),
            STEREO_GROUP_INCHI_NOTICE
        );
    }
    Ok(())
}

#[tokio::test]
async fn stereo_groups_molecular_exports_roundtrip() -> anyhow::Result<()> {
    for text in [
        "C[C@H](N)C(=O)O |&1:1|",
        "C[C@H](O)[C@H](F)Cl |a:1,&1:3|",
        "C[C@H](O)[C@H](F)Cl |o1:1,3|",
        "C[C@H](O)C(=O)O |r|",
    ] {
        let doc = drawing(text)?;
        let expected = prepare_identifiers(&molecular::prepare(&doc)?, false)?.0;
        for format in ["mol", "smiles", "cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let response = execute(request).await?;
            assert!(
                response.warnings.is_empty(),
                "{text}/{format}: {:?}",
                response.warnings
            );
            let output = response.output.unwrap();
            let restored = match format {
                "smiles" => drawing(&output)?,
                "mol" => {
                    assert!(output.contains("V3000"));
                    assert!(output.contains("MDLV30/STER"));
                    molfile::read(&output)?.display_document()?.0
                }
                _ => {
                    let xml = if format == "cdx" {
                        exchange::from_cdx(&STANDARD.decode(output)?).map_err(anyhow::Error::msg)?
                    } else {
                        output
                    };
                    assert!(xml.contains("EnhancedStereoGroupNum=\"1\""));
                    let imported = cdxml::import_cdxml(&xml)?;
                    assert!(imported.notices.is_empty());
                    imported.document
                }
            };
            if format != "smiles" {
                assert_eq!(groups(&restored), groups(&doc), "{text}/{format}");
            }
            assert_eq!(
                prepare_identifiers(&molecular::prepare(&restored)?, false)?.0,
                expected,
                "{text}/{format}"
            );
        }
    }
    Ok(())
}

#[test]
fn stereo_groups_cdxml_numbers_follow_the_cdx_schema() -> anyhow::Result<()> {
    let mut doc = drawing("C[C@H](O)[C@H](F)Cl |a:1,&1:3|")?;
    doc.atoms[3].stereo_group.as_mut().unwrap().number = 65535;
    let xml = exchange::drawing::write(&doc, Default::default())?;
    assert!(xml.contains("EnhancedStereoType=\"Absolute\""));
    let bytes = exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
    let xml = exchange::from_cdx(&bytes).map_err(anyhow::Error::msg)?;
    assert_eq!(groups(&cdxml::import_cdxml(&xml)?.document), groups(&doc));
    doc.atoms[3].stereo_group.as_mut().unwrap().number = 65536;
    assert!(
        exchange::drawing::write(&doc, Default::default())
            .unwrap_err()
            .to_string()
            .contains("65535")
    );
    Ok(())
}

#[test]
fn stereo_groups_abs_only_identifiers_and_cdxml_stay_plain() -> anyhow::Result<()> {
    let doc = drawing("C[C@H](N)C(=O)O |a:1|")?;
    assert!(groups(&doc).iter().all(Option::is_none));
    let molecule = molecular::prepare(&doc)?;
    assert_eq!(
        prepare_identifiers(&molecule, false)?.0,
        smiles::write::write(&molecule.state, Default::default())?.text
    );
    assert!(!exchange::drawing::write(&doc, Default::default())?.contains("EnhancedStereo"));
    Ok(())
}
