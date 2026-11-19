//! `reshiki --cli pieces check FILE…`: validate equipment piece files, the
//! check authors and CI run before sharing a piece.
use super::host::{CliError, labelled};
use reshiki_model::equipment::{
    format::{self, Definition},
    validate::MAX_FILE_BYTES,
};
use serde_json::{Map, Value, json};
use std::{io::Write, path::Path, path::PathBuf};

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
