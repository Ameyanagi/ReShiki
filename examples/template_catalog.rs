//! Generate drawing coordinates from frozen molecular facts with the native engine.
use anyhow::Context;
use reshiki::{
    engine::{LocalEngine, Request},
    templates::Template,
};
use std::{fs, path::PathBuf};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(
        args.next()
            .context("Usage: template_catalog FACTS_JSON OUTPUT_JSON")?,
    );
    let output = PathBuf::from(args.next().context("Missing output file")?);
    let facts: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(input)?)?;
    let engine = LocalEngine::default();
    let mut templates = vec![];
    for fact in facts {
        let get = |key| {
            fact.get(key)
                .and_then(|v| v.as_str())
                .context("Missing molecular fact")
        };
        let smiles = get("smiles")?;
        let response = engine
            .request(Request::import("smiles", smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = response.analysis.context("Missing chemical identity")?;
        anyhow::ensure!(
            analysis.formula == get("formula")? && analysis.inchikey == get("inchikey")?,
            "Chemical identity differs for {}",
            get("name")?
        );
        templates.push(Template {
            id: String::new(),
            name: get("name")?.into(),
            group: get("group")?.into(),
            smiles: smiles.into(),
            keywords: serde_json::from_value(fact.get("keywords").cloned().unwrap_or_default())?,
            note: get("note")?.into(),
            document: response.document.context("Missing drawing")?,
            anchor: Default::default(),
        });
    }
    fs::write(output, serde_json::to_vec_pretty(&templates)?)?;
    println!("Generated {} validated templates", templates.len());
    Ok(())
}
