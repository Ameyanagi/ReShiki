//! Export a native document without opening a window.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .ok_or("usage: export_drawing INPUT.moruno OUTPUT_PREFIX")?;
    let output = std::path::PathBuf::from(args.next().ok_or("missing output prefix")?);
    let document: moruno::document::Document = serde_json::from_slice(&std::fs::read(input)?)?;
    document.validate()?;
    for format in ["svg", "pdf", "png"] {
        std::fs::write(
            output.with_extension(format),
            moruno::export::drawing(&document, format)?,
        )?;
    }
    Ok(())
}
