//! Copy a native drawing with the same editable/image pipeline as the desktop.
use anyhow::Context;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::io::Write;
    let path = std::env::args_os()
        .nth(1)
        .context("usage: copy_drawing INPUT.rsk")?;
    let document: reshiki::document::Document = serde_json::from_slice(&std::fs::read(path)?)?;
    document.validate().map_err(anyhow::Error::msg)?;
    let result = reshiki::clipboard::copy(Default::default(), document, false)
        .await
        .map_err(anyhow::Error::msg)?;
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "Editable: {} · Image: {}",
        result.external_editable, !result.image_only
    )?;
    for notice in result.notices {
        writeln!(output, "{notice}")?;
    }
    Ok(())
}
