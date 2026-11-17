//! `reshiki --cli convert`: import, then export through the ops tools.
use super::{
    args::{Convert, Export, Labels, Output},
    host::{CliError, Session, labelled},
    io::write_output,
};
use serde_json::{Map, Value, json};
use std::io::Write;

/// Formats never written to a terminal.
const BINARY: &[&str] = &["pdf", "png", "cdx"];

/// Converts as `convert` says. `terminal` tells whether stdout is one.
///
/// The file goes to `out`, or to the -o file with, given `--receipt`, one
/// `{"receipt": {format, byte_len, detail}, "experimental": true, "api": …}`
/// line on `out`.
pub(crate) async fn run(
    convert: Convert,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> Result<(), CliError> {
    let Convert {
        input,
        export,
        labels,
    } = convert;
    refuse_binary(&export.format, &export.output, terminal)?;
    let mut session = Session::new();
    let document = session.import(input, err).await?;
    if let Some(labels) = labels {
        label(&mut session, &document, labels, err).await?;
    }
    write(&mut session, document, export, out, err).await
}

/// Predicts the whole drawing's shifts and writes them onto its atoms, at
/// the revision predicted, as `predict_nmr` then `label_atoms`.
async fn label(
    session: &mut Session,
    document: &Value,
    labels: Labels,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let arguments = json!({"document": document, "nucleus": labels.nucleus, "ids": null});
    let predicted = session.call("predict_nmr", arguments, err).await?;
    let value = predicted.value.get("value").cloned().unwrap_or_default();
    let rows: Vec<Value> = value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| json!({"atom": row["atom"], "ppm": row["ppm"]}))
        .collect();
    let arguments = json!({
        "document": document, "rows": rows, "format": labels.format,
        "base_revision": value["revision"], "idempotency_key": null,
    });
    session.call("label_atoms", arguments, err).await?;
    Ok(())
}

/// Refuses to write pdf, png or cdx to stdout when it is a terminal.
pub(crate) fn refuse_binary(format: &str, output: &Output, terminal: bool) -> Result<(), CliError> {
    if *output == Output::Stdout && terminal && BINARY.contains(&format) {
        return Err(CliError::Usage(
            "refusing to write binary data to a terminal; use -o FILE".into(),
        ));
    }
    Ok(())
}

/// Exports the session document `document` and writes the file as `export`
/// says, as convert and compose do.
pub(crate) async fn write(
    session: &mut Session,
    document: Value,
    export: Export,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let Export {
        format,
        svg_text,
        output,
        pages,
        force,
        receipt,
    } = export;
    let pages = if pages {
        Value::Bool(true)
    } else {
        Value::Null
    };
    let result = session
        .call(
            "export",
            json!({
                "document": document,
                "format": format,
                "pages": pages,
                "svg_text": svg_text,
            }),
            err,
        )
        .await?;
    let Some(file) = result.files.into_iter().next() else {
        return Err(CliError::Failed(
            "internal error: export returned no file".into(),
        ));
    };
    emit(&file.bytes, &output, force, out)?;
    if receipt {
        let receipt = result
            .value
            .get("value")
            .and_then(|value| value.get("receipt"))
            .cloned()
            .unwrap_or(Value::Null);
        let line = labelled(Map::from_iter([("receipt".to_owned(), receipt)]));
        writeln!(out, "{line}").map_err(CliError::Stdout)?;
    }
    Ok(())
}

/// Writes `bytes` to `out`, or to the file `output` names.
pub(crate) fn emit(
    bytes: &[u8],
    output: &Output,
    force: bool,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    match output {
        Output::Stdout => out.write_all(bytes).map_err(CliError::Stdout),
        Output::File(path) => write_output(path, bytes, force).map_err(CliError::Failed),
    }
}
