//! Export a native document without opening a window.
use anyhow::Context;
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .context("usage: export_drawing INPUT.rsk OUTPUT_PREFIX")?;
    let output = std::path::PathBuf::from(args.next().context("missing output prefix")?);
    let document: reshiki::document::Document = serde_json::from_slice(&std::fs::read(input)?)?;
    document.validate().map_err(anyhow::Error::msg)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let document = runtime
        .block_on(reshiki::export::checked_document(
            &Default::default(),
            document,
        ))
        .map_err(anyhow::Error::msg)?;
    for format in ["svg", "pdf", "png"] {
        std::fs::write(
            output.with_extension(format),
            reshiki::export::drawing(&document, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}
