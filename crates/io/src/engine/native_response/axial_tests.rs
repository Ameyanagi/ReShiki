use super::*;
use crate::engine::LocalEngine;

const INPUT: &str = "c1(-c2c(Br)cccc2Cl)c([C@H](O)C)cccc1[C@@H](O)C";
const OUTPUT: &str = "C[C@H](O)c1cccc([C@@H](C)O)c1-c1c(Cl)cccc1Br";

#[tokio::test]
async fn axial_smiles_notice_survives_analysis_and_export() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    for annotation in [" |wU:0.9|", " |wD:0.9|", ""] {
        let expected_notice = (!annotation.is_empty()).then_some(SMILES_AXIAL_NOTICE);
        let imported = engine
            .request(Request::import_smiles(&format!("{INPUT}{annotation}")))
            .await
            .map_err(anyhow::Error::msg)?;
        assert!(imported.warnings.is_empty());
        let document = imported.document.unwrap();
        for format in [None, Some("smiles"), Some("inchi"), Some("mol")] {
            let mut request = Request::molecule(
                if format.is_some() {
                    "export"
                } else {
                    "analyze"
                },
                document.clone(),
            );
            request.format = format.map(str::to_owned);
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            let analysis = response.analysis.unwrap();
            assert_eq!(analysis.smiles, OUTPUT, "{annotation}/{format:?}");
            assert_eq!(analysis.smiles_notice.as_deref(), expected_notice);
            let expected_warnings: Vec<_> = expected_notice
                .filter(|_| format == Some("smiles"))
                .into_iter()
                .collect();
            assert_eq!(response.warnings, expected_warnings);
            if format == Some("smiles") {
                assert_eq!(response.output.as_deref(), Some(OUTPUT));
            }
        }
        // Older cached analyses deserialize with None; no-axis JSON omits the field.
        let mut json = serde_json::to_value(imported.analysis.unwrap())?;
        assert_eq!(
            json.get("smiles_notice").is_some(),
            expected_notice.is_some()
        );
        json.as_object_mut().unwrap().remove("smiles_notice");
        assert!(
            serde_json::from_value::<Analysis>(json)?
                .smiles_notice
                .is_none()
        );
    }
    Ok(())
}

#[tokio::test]
async fn reaction_smiles_warns_only_for_exported_axial_participants() -> anyhow::Result<()> {
    use crate::{document::Point, reactions::Participant};

    let engine = LocalEngine::in_process();
    for annotation in [" |wU:0.9|", " |wD:0.9|", ""] {
        let mut document = engine
            .request(Request::import("rsmi", &format!("{INPUT}>>O{annotation}")))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .unwrap();
        document.reactions[0].reactants[0].coefficient = 2;
        for format in ["rsmi", "rxn"] {
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            assert_eq!(
                response
                    .warnings
                    .iter()
                    .filter(|notice| notice.as_str() == SMILES_AXIAL_NOTICE)
                    .count(),
                usize::from(format == "rsmi" && !annotation.is_empty())
            );
            if format == "rsmi" {
                assert_eq!(response.output, Some(format!("{OUTPUT}.{OUTPUT}>>O")));
            }
        }
        // An axis elsewhere in the drawing is not part of this reaction export.
        let reactant = document.add_atom("C", Point::new(350., 0.));
        document.reactions[0].reactants = vec![Participant {
            atoms: vec![reactant],
            coefficient: 1,
        }];
        let mut request = Request::molecule("export", document);
        request.format = Some("rsmi".into());
        let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
        assert_eq!(response.output.as_deref(), Some("C>>O"));
        assert!(
            !response
                .warnings
                .iter()
                .any(|notice| notice == SMILES_AXIAL_NOTICE)
        );
    }
    Ok(())
}
