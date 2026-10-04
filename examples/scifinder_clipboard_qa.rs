//! Public clipboard-handoff fixtures; never contacts CAS or publishes clipboard data.
use anyhow::{Context, ensure};
use reshiki::{
    clipboard::{CopyFormat, chemical_snapshot, prepare_as},
    editing::{self, Transform},
    engine::{LocalEngine, Request},
};
use std::{future::Future, path::PathBuf, pin::Pin};

#[inline(never)]
fn heap_future<F: Future>(make: impl FnOnce() -> F) -> Pin<Box<F>> {
    Box::pin(make())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("Pass an output directory")?,
    );
    std::fs::create_dir_all(&output)?;
    let engine = LocalEngine::default();
    let mut scheme = heap_future(|| engine.request(Request::import("rsmi", "c1ccccc1>>C1CCCCC1")))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reaction drawing")?;
    scheme.reactions.clear();
    let mut vertical = scheme.clone();
    let ids = vertical.all_ids();
    editing::transform(&mut vertical, &ids, Transform::Rotate(90.));
    let mut reversed = scheme.clone();
    let arrow = &mut reversed.arrows[0];
    std::mem::swap(&mut arrow.start, &mut arrow.end);
    let mut no_arrow = scheme.clone();
    no_arrow.arrows.clear();
    let single = heap_future(|| engine.request(Request::import_smiles("C1CCCCC1")))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing molecular drawing")?;
    let mut results = Vec::new();
    for (name, source) in [
        ("benzene-hydrogenation", scheme),
        ("benzene-hydrogenation-vertical", vertical),
        ("cyclohexane-dehydrogenation-reversed", reversed),
        ("two-rings-no-arrow", no_arrow),
        ("cyclohexane", single),
    ] {
        let before = source.clone();
        let chemical = chemical_snapshot(&source, &source)
            .map_err(anyhow::Error::msg)?
            .into_owned();
        let format = if chemical.reactions.is_empty() {
            CopyFormat::Smiles
        } else {
            CopyFormat::ChemDoodleReaction
        };
        let copy = heap_future(|| prepare_as(engine.clone(), chemical, format))
            .await
            .map_err(anyhow::Error::msg)?;
        let text = copy.text().context("Missing chemical text")?;
        let extension = if format == CopyFormat::Smiles {
            "smiles"
        } else {
            "json"
        };
        std::fs::write(
            output.join(format!("{name}.rsk")),
            serde_json::to_vec_pretty(&source)?,
        )?;
        std::fs::write(output.join(format!("{name}.{extension}")), text)?;
        std::fs::write(
            output.join(format!("{name}.svg")),
            reshiki::scene::svg(&source),
        )?;
        std::fs::write(
            output.join(format!("{name}.png")),
            reshiki::export::drawing(&source, "png").map_err(anyhow::Error::msg)?,
        )?;
        ensure!(source == before, "Chemical copy changed the source");
        results.push(serde_json::json!({"fixture": name, "format": format.code(),
            "atoms": source.atoms.len(), "bonds": source.bonds.len(),
            "source_has_explicit_roles": !source.reactions.is_empty(),
            "copy_notices": copy.notices, "receiver_import": "not tested"}));
    }
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
