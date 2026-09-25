//! Export an interchange fixture through the desktop engine for visual review.
use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::{
    engine::{LocalEngine, Request},
    export,
};
use std::{fs, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().context("Input CDX or CDXML file required")?);
    let output = PathBuf::from(args.next().context("Output prefix required")?);
    let bytes = fs::read(&source)?;
    let format = source
        .extension()
        .and_then(|s| s.to_str())
        .context("Missing extension")?;
    let text = if format == "cdx" {
        STANDARD.encode(bytes)
    } else {
        String::from_utf8(bytes)?
    };
    let engine = LocalEngine::default();
    let response = engine
        .request(Request::import(format, &text))
        .await
        .map_err(anyhow::Error::msg)?;
    let doc = response.document.context("Missing imported drawing")?;
    fs::write(
        output.with_extension("rsk"),
        serde_json::to_vec_pretty(&doc)?,
    )?;
    for format in ["svg", "pdf", "png"] {
        fs::write(
            output.with_extension(format),
            export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let result = engine
            .request(request)
            .await
            .map_err(anyhow::Error::msg)?
            .output
            .context("Missing exported drawing")?;
        let bytes = if format == "cdx" {
            STANDARD.decode(result)?
        } else {
            result.into_bytes()
        };
        fs::write(output.with_extension(format), bytes)?;
    }
    println!(
        "{} atoms, {} bonds, {} graphics; warnings: {:?}",
        doc.atoms.len(),
        doc.bonds.len(),
        doc.graphics.len(),
        response.warnings
    );
    Ok(())
}
