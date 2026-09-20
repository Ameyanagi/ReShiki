//! Copy a native drawing with the same editable/image pipeline as the desktop.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: copy_drawing INPUT.reshiki")?;
    let document: reshiki::document::Document = serde_json::from_slice(&std::fs::read(path)?)?;
    document.validate()?;
    let result = reshiki::clipboard::copy(Default::default(), document, false).await?;
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
