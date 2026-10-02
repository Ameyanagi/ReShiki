//! Prepare public manual-handoff fixtures locally. Does not contact CAS or use the clipboard.
use anyhow::{Context, ensure};
use reshiki::engine::{LocalEngine, Request};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("usage: scifinder_handoff_qa NEW_OUTPUT_DIRECTORY")?,
    );
    fs::create_dir(&output).context("Choose a new output directory with an existing parent")?;
    let engine = LocalEngine::default();
    let mut fixtures = Vec::new();
    for (name, input, abbreviate) in [
        ("ethanol", "CCO", false),
        ("lactic-acid-stereo", "C[C@H](O)C(=O)O", false),
        ("difluoroethene-stereo", "F/C=C/F", false),
        ("defined-abbreviations", "COc1ccc(NC(=O)OC(C)(C)C)cc1", true),
    ] {
        let response = engine
            .request(Request::import_smiles(input))
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(
            response.warnings.is_empty(),
            "{name}: {:?}",
            response.warnings
        );
        let original = response.analysis.context("Missing source identity")?;
        ensure!(!original.inchikey.is_empty(), "{name}: empty InChIKey");
        let mut document = response.document.context("Missing source drawing")?;
        if abbreviate {
            let mut request = Request::molecule("abbreviate", document.clone());
            request.selected_ids = Some(document.all_ids());
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            ensure!(
                response.warnings.is_empty(),
                "{name}: {:?}",
                response.warnings
            );
            document = response.document.context("Missing abbreviated drawing")?;
            ensure!(
                !document.abbreviations.is_empty(),
                "{name}: no abbreviations"
            );
        }
        document.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            reshiki::reactions::molecules(&document, &document.all_ids()).len() == 1,
            "{name}: fixture must contain exactly one connected molecule"
        );
        fs::write(
            output.join(format!("{name}.rsk")),
            serde_json::to_vec_pretty(&document)?,
        )?;
        let mut formats = Vec::new();
        for format in ["smiles", "mol", "inchi"] {
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            ensure!(
                response.warnings.is_empty(),
                "{name} {format}: {:?}",
                response.warnings
            );
            let text = response.output.context("Missing exported text")?;
            ensure!(!text.trim().is_empty(), "{name} {format}: empty export");
            let restored = engine
                .request(Request::import(format, &text))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(
                restored.warnings.is_empty(),
                "{name} {format}: {:?}",
                restored.warnings
            );
            let analysis = restored.analysis.context("Missing round-trip identity")?;
            let restored = restored.document.context("Missing round-trip drawing")?;
            ensure!(
                analysis.inchikey == original.inchikey,
                "{name} {format}: identity changed"
            );
            ensure!(
                analysis.formula == original.formula,
                "{name} {format}: formula changed"
            );
            ensure!(
                restored.atoms.len() == document.atoms.len(),
                "{name} {format}: atom count changed"
            );
            ensure!(
                restored.bonds.len() == document.bonds.len(),
                "{name} {format}: bond count changed"
            );
            let filename = format!("{name}.{format}");
            fs::write(output.join(&filename), &text)?;
            formats.push(json!({
                "file": filename,
                "sha256": format!("{:x}", Sha256::digest(text.as_bytes())),
                "local_round_trip": "passed",
                "inchikey": analysis.inchikey,
                "warnings": [],
                "cas_editor_import": "not tested",
                "cas_search": "not tested"
            }));
        }
        println!(
            "{name}: {} · 3 local format round trips passed",
            original.inchikey
        );
        fixtures.push(json!({
            "name": name,
            "input_smiles": input,
            "source_drawing": format!("{name}.rsk"),
            "atoms": document.atoms.len(),
            "bonds": document.bonds.len(),
            "defined_abbreviations": document.abbreviations.len(),
            "analysis": original,
            "formats": formats
        }));
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "reshiki_version": env!("CARGO_PKG_VERSION"),
            "purpose": "Local preparation for a manual CAS SciFinder handoff",
            "scope": "Four public single-molecule fixtures; no network or clipboard access",
            "cas_validation": "Not tested. Requires an authorized CAS SciFinder session.",
            "fixtures": fixtures
        }))?,
    )?;
    Ok(())
}
