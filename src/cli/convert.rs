//! `reshiki --cli convert`: import, then export through the ops tools.
use super::{
    args::{Convert, Output},
    host::{CliError, Session, api},
    io::write_output,
};
use serde_json::{Value, json};
use std::io::Write;

/// Formats never written to a terminal.
const BINARY: &[&str] = &["pdf", "png"];

/// Converts as `convert` says. `terminal` tells whether stdout is one.
///
/// The file goes to `out`, or to the -o file with, given `--receipt`, one
/// `{"receipt": {format, byte_len, detail}, "api": …}` line on `out`.
pub(crate) async fn run(
    convert: Convert,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> Result<(), CliError> {
    let Convert {
        input,
        format,
        output,
        pages,
        force,
        receipt,
    } = convert;
    if output == Output::Stdout && terminal && BINARY.contains(&format.as_str()) {
        return Err(CliError::Usage(
            "refusing to write binary data to a terminal; use -o FILE".into(),
        ));
    }
    let mut session = Session::new();
    let document = session.import(input, err).await?;
    let pages = if pages {
        Value::Bool(true)
    } else {
        Value::Null
    };
    let result = session
        .call(
            "export",
            json!({"document": document, "format": format, "pages": pages}),
            err,
        )
        .await?;
    let Some(file) = result.files.into_iter().next() else {
        return Err(CliError::Failed(
            "internal error: export returned no file".into(),
        ));
    };
    let path = match output {
        Output::Stdout => return out.write_all(&file.bytes).map_err(CliError::Stdout),
        Output::File(path) => path,
    };
    write_output(&path, &file.bytes, force).map_err(CliError::Failed)?;
    if receipt {
        let receipt = result
            .value
            .get("value")
            .and_then(|value| value.get("receipt"))
            .cloned()
            .unwrap_or(Value::Null);
        let line = json!({"receipt": receipt, "api": api()});
        writeln!(out, "{line}").map_err(CliError::Stdout)?;
    }
    Ok(())
}
