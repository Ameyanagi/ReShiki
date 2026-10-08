//! `reshiki --cli render`: import, then render a preview through the ops
//! tools.
use super::{
    args::Render,
    convert::{emit, refuse_binary},
    host::{CliError, Session},
};
use serde_json::json;
use std::io::Write;

/// Renders as `render` says. `terminal` tells whether stdout is one.
///
/// png is the render tool's image and svg its file, as `reshiki --mcp`
/// returns them; either goes to `out` or to the -o file.
pub(crate) async fn run(
    render: Render,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> Result<(), CliError> {
    let Render {
        input,
        format,
        output,
        width,
        height,
        force,
    } = render;
    refuse_binary(&format, &output, terminal)?;
    let mut session = Session::new();
    let document = session.import(input, err).await?;
    let arguments = json!({
        "document": document,
        "format": format,
        "max_width": width,
        "max_height": height,
        "ids": null,
    });
    let result = session.call("render", arguments, err).await?;
    let bytes = if format == "png" {
        result.images.into_iter().next().map(|image| image.bytes)
    } else {
        result.files.into_iter().next().map(|file| file.bytes)
    };
    let Some(bytes) = bytes else {
        return Err(CliError::Failed(
            "internal error: render returned no image".into(),
        ));
    };
    emit(&bytes, &output, force, out)
}
