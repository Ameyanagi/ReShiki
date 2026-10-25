//! Reusable captioned caffeine/glucose inputs from the real Name importer and
//! geometry worker. Set RESHIKI_NAMING_HELPER to this source's built executable.
use anyhow::{Context, Result, ensure};
use reshiki::{
    document::Document,
    engine::{LocalEngine, Request},
    geometry::{Client, ForceField, Limits, Prepared},
    models3d::Display,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let output = PathBuf::from(std::env::args_os().nth(1).context("Output directory")?);
    let executable = PathBuf::from(
        std::env::var_os("RESHIKI_NAMING_HELPER")
            .context("Set RESHIKI_NAMING_HELPER to this source's built ReShiki executable")?,
    );
    let engine = LocalEngine::default();
    let geometry = Client::new(executable, Limits::default())?;
    std::fs::create_dir_all(&output)?;
    let mut receipts = vec![];
    for (slug, name, formula, stereocenters) in [
        ("caffeine", "caffeine", "C8H10N4O2", 0),
        ("glucose", "β-D-glucopyranose", "C6H12O6", 5),
    ] {
        let parsed = reshiki::naming::resolve_name(name, Default::default()).await?;
        let imported = engine
            .request(Request::import_smiles(&parsed.smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let before = imported.analysis.context("Imported chemistry analysis")?;
        ensure!(before.formula == formula, "Unexpected {name} formula");
        let mut document: Document = imported.document.context("Imported drawing")?;
        let atoms: Vec<_> = document.atoms.iter().map(|atom| atom.id).collect();
        ensure!(
            document
                .atoms
                .iter()
                .filter(|atom| atom.stereo.is_some())
                .count()
                == stereocenters,
            "Unexpected {name} stereocenter count"
        );
        reshiki::molecule_names::show(
            &mut document,
            &atoms,
            name.into(),
            before.smiles.clone(),
            Default::default(),
        )
        .map_err(anyhow::Error::msg)?;
        std::fs::write(
            output.join(format!("{slug}-2d.rsk")),
            document.file_json().map_err(anyhow::Error::msg)?,
        )?;
        let chemical_atoms = document.atoms.clone();
        let chemical_bonds = document.bonds.clone();
        let before_model = document.clone();
        let prepared = Prepared::new(&document, &atoms)?.without_retained_depth();
        let optimized = geometry.generate(&prepared, ForceField::Mmff94s).await?;
        document.models.push(prepared.model(&optimized)?);
        document.require_version(reshiki::document::MODELS_3D);
        reshiki::models3d::set_display(&mut document, 0, Display::Both);
        let _ = reshiki::transaction::reconcile(&mut document, before_model)
            .map_err(|error| anyhow::anyhow!("Model transaction: {error:?}"))?;
        let after = engine
            .request(Request::molecule("analyze", document.clone()))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Model chemistry analysis")?;
        ensure!(
            document.atoms == chemical_atoms && document.bonds == chemical_bonds,
            "Generating a model changed {name}'s drawing chemistry"
        );
        ensure!(
            before.smiles == after.smiles
                && before.inchi == after.inchi
                && before.inchikey == after.inchikey,
            "Generating a model changed {name}'s chemical identifiers"
        );
        std::fs::write(
            output.join(format!("{slug}-model.rsk")),
            document.file_json().map_err(anyhow::Error::msg)?,
        )?;
        receipts.push(serde_json::json!({
            "name": name, "parser": parsed.provenance.label(), "formula":formula,
            "stereocenters":stereocenters, "before":before, "after":after,
            "original_atom_count": optimized.conformer.original_atom_count,
            "generated_hydrogens":optimized.conformer.hydrogen_parents.len(),
        }));
        println!("{slug}: {formula}, {stereocenters} stored stereocenters, identifiers unchanged");
    }
    std::fs::write(
        output.join("chemistry-receipts.json"),
        serde_json::to_vec_pretty(&receipts)?,
    )?;
    Ok(())
}
