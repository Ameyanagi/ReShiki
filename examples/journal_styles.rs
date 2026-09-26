//! Reproduce a same-geometry comparison and inspect downloaded CDS/CDX styles.
use anyhow::Context;
use reshiki::{
    document::{Document, Point},
    document_styles::{self, Preset},
    editing,
    engine::{LocalEngine, Request},
    exchange, export,
};
use std::{fs, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .context("journal_styles OUTPUT_DIRECTORY | --inspect-style FILE")?,
    );
    if path == std::path::Path::new("--inspect-style") {
        let style =
            document_styles::load(&PathBuf::from(args.next().context("missing style file")?))
                .map_err(anyhow::Error::msg)?;
        println!("{}", serde_json::to_string_pretty(&style)?);
        return Ok(());
    }
    fs::create_dir_all(&path)?;
    let engine = LocalEngine::default();
    let mut sample = Document::default();
    let mut x = 0.;
    for smiles in [
        "CC(=O)Oc1ccccc1C(=O)O",
        "Cn1c(=O)c2c(ncn2C)n(C)c1=O",
        "N[C@@H](C)C(=O)O",
    ] {
        let doc = engine
            .request(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("missing molecule")?;
        let min_x = doc
            .atoms
            .iter()
            .map(|a| a.position.x)
            .reduce(f32::min)
            .unwrap();
        let max_x = doc
            .atoms
            .iter()
            .map(|a| a.position.x)
            .reduce(f32::max)
            .unwrap();
        let min_y = doc
            .atoms
            .iter()
            .map(|a| a.position.y)
            .reduce(f32::min)
            .unwrap();
        editing::append(&mut sample, &doc, Point::new(x - min_x, -min_y));
        x += max_x - min_x + 100.;
    }
    let (sample, notice) = export::figure_document(&engine, sample)
        .await
        .map_err(anyhow::Error::msg)?;
    if let Some(notice) = notice {
        eprintln!("{notice}");
    }
    let styles: Vec<_> = Preset::ALL
        .into_iter()
        .map(|p| (p.id().to_owned(), p.style()))
        .collect();
    let mut manifest = Vec::new();
    for (id, style) in styles {
        let doc = document_styles::apply(&sample, style.clone(), true, true)
            .map_err(anyhow::Error::msg)?;
        let dir = path.join(&id);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("sample.rsk"), serde_json::to_vec_pretty(&doc)?)?;
        fs::write(
            dir.join("style.reshiki-style"),
            serde_json::to_vec_pretty(&style)?,
        )?;
        fs::write(
            dir.join("sample.cdxml"),
            exchange::drawing::write(&doc, Default::default())?,
        )?;
        for format in ["svg", "pdf"] {
            fs::write(
                dir.join(format!("reshiki.{format}")),
                export::figure(&doc, format)
                    .map_err(anyhow::Error::msg)?
                    .bytes,
            )?;
        }
        manifest.push(serde_json::json!({"id":id,"style":style}));
    }
    fs::write(
        path.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
