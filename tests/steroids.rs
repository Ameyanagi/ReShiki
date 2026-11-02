//! Steroid identities are checked on the actual stored/placed drawing, using
//! independently fetched PubChem standard InChIKeys rather than source strings.
use anyhow::{Context, Result};
use reshiki::{
    document::{Document, Point},
    editing::{self, Transform},
    engine::{Analysis, ChemistryEngine, LocalEngine, Request},
    templates::{self, LIBRARY},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    name: String,
    smiles: String,
    formula: String,
    inchikey: String,
    cid: u64,
    source: String,
}

fn references() -> Result<Vec<Reference>> {
    Ok(serde_json::from_str(include_str!(
        "../assets/steroids.json"
    ))?)
}

fn template(name: &str, implicit: bool) -> Result<&'static templates::Template> {
    let name = if implicit {
        format!("{name} · implicit 8,9,14-H")
    } else {
        name.into()
    };
    LIBRARY
        .iter()
        .find(|t| t.group == "Steroids" && t.name == name)
        .with_context(|| format!("Missing steroid template {name}"))
}

async fn analyze(doc: Document) -> Result<Analysis> {
    LocalEngine::default()
        .execute(Request::molecule("analyze", doc))
        .await
        .map_err(anyhow::Error::msg)?
        .analysis
        .context("Missing steroid analysis")
}

#[tokio::test]
async fn all_sixteen_actual_steroid_drawings_match_fetched_pubchem_identities() -> Result<()> {
    let references = references()?;
    assert_eq!(references.len(), 8);
    assert_eq!(LIBRARY.iter().filter(|t| t.group == "Steroids").count(), 16);
    for reference in references {
        assert_eq!(
            reference.source,
            format!(
                "https://pubchem.ncbi.nlm.nih.gov/compound/{}",
                reference.cid
            )
        );
        let imported = LocalEngine::default()
            .execute(Request::import_smiles(&reference.smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Missing source analysis")?;
        assert_eq!(
            imported.formula, reference.formula,
            "{} source formula",
            reference.name
        );
        assert_eq!(
            imported.inchikey, reference.inchikey,
            "{} source identity",
            reference.name
        );
        for implicit in [false, true] {
            let item = template(&reference.name, implicit)?;
            item.document.validate().map_err(anyhow::Error::msg)?;
            let analysis = analyze(item.document.clone()).await?;
            assert_eq!(analysis.formula, reference.formula, "{} formula", item.name);
            assert_eq!(
                analysis.inchikey, reference.inchikey,
                "{} identity",
                item.name
            );
            let native =
                Document::from_native_file(&item.document.file_json().map_err(anyhow::Error::msg)?)
                    .map_err(anyhow::Error::msg)?;
            assert_eq!(
                analyze(native).await?.inchikey,
                reference.inchikey,
                "{} native round trip",
                item.name
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn placement_rotation_copy_and_chemical_exports_keep_steroid_stereochemistry() -> Result<()> {
    for reference in references()? {
        for implicit in [false, true] {
            let item = template(&reference.name, implicit)?;
            let (mut placed, ids) = templates::place(
                &Document::default(),
                &item.document,
                Point::new(150., 100.),
                None,
                5.,
            )
            .map_err(anyhow::Error::msg)?;
            editing::transform(&mut placed, &ids, Transform::Rotate(37.));
            let mut copied = Document::default();
            editing::append(&mut copied, &placed, Point::new(120., -65.));
            assert_eq!(
                analyze(copied.clone()).await?.inchikey,
                reference.inchikey,
                "{} placed/rotated/copied",
                item.name
            );
            // CDXML/CDX carry no atom parity, so the implicit variant's
            // undrawn 8/9/14 configurations do not survive there (as in ChemDraw).
            let formats: &[&str] = if implicit {
                &["mol", "smiles"]
            } else {
                &["mol", "smiles", "cdxml", "cdx"]
            };
            for &format in formats {
                let mut request = Request::molecule("export", copied.clone());
                request.format = Some(format.into());
                let output = LocalEngine::default()
                    .execute(request)
                    .await
                    .map_err(anyhow::Error::msg)?
                    .output
                    .context("Missing steroid export")?;
                let restored = LocalEngine::default()
                    .execute(Request::import(format, &output))
                    .await
                    .map_err(anyhow::Error::msg)?;
                assert_eq!(
                    restored
                        .analysis
                        .context("Missing reimported steroid analysis")?
                        .inchikey,
                    reference.inchikey,
                    "{} {format}",
                    item.name
                );
            }
        }
    }
    Ok(())
}
