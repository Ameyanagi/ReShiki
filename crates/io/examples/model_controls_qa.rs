//! Build reusable native QA inputs through the real import and geometry paths.
//! `RESHIKI_QA_GEOMETRY_EXE` selects an existing ReShiki desktop executable;
//! the argument is an output directory, kept outside the source tree by default.
use reshiki_io::{
    engine::{LocalEngine, Request},
    geometry::{Client, ForceField, Limits, Prepared},
};
use reshiki_model::{document::Document, models3d::Display};
use std::path::PathBuf;

const PACLITAXEL: &str = "CC1=C2[C@H](C(=O)[C@@]3([C@H](C[C@@H]4[C@]([C@H]3[C@@H]([C@@](C2(C)C)(C[C@@H]1OC(=O)[C@@H]([C@H](C5=CC=CC=C5)NC(=O)C6=CC=CC=C6)O)O)OC(=O)C7=CC=CC=C7)(CO4)OC(=O)C)O)C)OC(=O)C";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("Usage: model_controls_qa OUTPUT_DIRECTORY"))?,
    );
    let executable =
        PathBuf::from(std::env::var_os("RESHIKI_QA_GEOMETRY_EXE").ok_or_else(|| {
            anyhow::anyhow!("Set RESHIKI_QA_GEOMETRY_EXE to an existing desktop ReShiki executable")
        })?);
    let engine = LocalEngine::default();
    let client = Client::new(executable, Limits::default())?;
    std::fs::create_dir_all(&output)?;
    for (name, smiles, formula, model) in [
        ("caffeine", "Cn1c(=O)c2c(ncn2C)n(C)c1=O", "C8H10N4O2", true),
        ("aspirin", "CC(=O)Oc1ccccc1C(=O)O", "C9H8O4", true),
        ("paclitaxel", PACLITAXEL, "C47H51NO14", false),
    ] {
        let response = engine
            .request(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = response
            .analysis
            .ok_or_else(|| anyhow::anyhow!("Missing {name} analysis"))?;
        anyhow::ensure!(
            analysis.formula == formula,
            "Unexpected {name} formula: {}",
            analysis.formula
        );
        let mut doc: Document = response
            .document
            .ok_or_else(|| anyhow::anyhow!("Missing {name} drawing"))?;
        let atoms: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
        reshiki_model::molecule_names::show(
            &mut doc,
            &atoms,
            name.into(),
            analysis.smiles,
            Default::default(),
        )
        .map_err(anyhow::Error::msg)?;
        if model {
            let prepared = Prepared::new(&doc, &atoms)?.without_retained_depth();
            let optimized = client.generate(&prepared, ForceField::Mmff94s).await?;
            doc.models.push(prepared.model(&optimized)?);
            doc.require_version(reshiki_model::document::MODELS_3D);
            reshiki_model::models3d::set_display(&mut doc, 0, Display::ThreeD);
        }
        let path = output.join(format!("{name}.rsk"));
        std::fs::write(&path, doc.file_json().map_err(anyhow::Error::msg)?)?;
        println!("{} · {} atoms · {formula}", path.display(), doc.atoms.len());
    }
    Ok(())
}
