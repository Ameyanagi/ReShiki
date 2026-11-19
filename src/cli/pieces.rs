//! `reshiki --cli pieces`: validate equipment piece files and render previews.
use super::{
    args::PieceRender,
    convert::refuse_binary,
    host::{CliError, Session, labelled},
    render,
};
use reshiki_model::{
    canvas_theme::CanvasTheme,
    document::{Document, Point},
    equipment::{
        format::{self, Definition},
        placed,
        validate::MAX_FILE_BYTES,
    },
};
use serde_json::{Map, Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Prints one JSON line with a result per file; fails when any file fails.
pub(crate) fn check(files: &[PathBuf], out: &mut dyn Write) -> Result<(), CliError> {
    let results: Vec<Value> = files
        .iter()
        .map(|file| match read(file) {
            Ok(def) => json!({
                "file": file.display().to_string(), "ok": true, "id": def.id,
                "revision": def.revision, "hash": format::content_hash(&def),
            }),
            Err(error) => json!({"file": file.display().to_string(), "ok": false, "error": error}),
        })
        .collect();
    let failed = results.iter().filter(|r| r["ok"] == false).count();
    let value = Map::from_iter([("pieces".to_string(), Value::Array(results))]);
    writeln!(out, "{}", labelled(value)).map_err(CliError::Stdout)?;
    match failed {
        0 => Ok(()),
        n => Err(CliError::Failed(format!(
            "{n} of {} piece files did not pass",
            files.len()
        ))),
    }
}

/// Renders one checked piece in a drawing of its own.
pub(crate) async fn render(
    PieceRender {
        file,
        dark,
        preview,
    }: PieceRender,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> Result<(), CliError> {
    refuse_binary(&preview.format, &preview.output, terminal)?;
    let definition = read(&file).map_err(CliError::Failed)?;
    let mut doc = Document {
        canvas_theme: if dark {
            CanvasTheme::Dark
        } else {
            CanvasTheme::Light
        },
        ..Document::default()
    };
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(
        1,
        Arc::new(definition),
        Point::default(),
        scale,
    ));
    let mut session = Session::new();
    let document = session.store_document(doc)?;
    render::write(&mut session, document, preview, out, err).await
}

fn read(path: &Path) -> Result<Definition, String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_FILE_BYTES as u64 + 1)
                .read_to_string(&mut text)
        })
        .map_err(|e| e.to_string())?;
    if text.len() > MAX_FILE_BYTES {
        return Err("The piece file is larger than 1 MB.".into());
    }
    format::parse(&text)
}
