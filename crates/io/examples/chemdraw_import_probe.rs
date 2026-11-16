//! Batch probe for the production ChemDraw drawing and stationery pipelines.
//!
//! cargo run -p reshiki-io --example chemdraw_import_probe -- \
//!   --output-dir /tmp/reshiki-roundtrips INPUT.cdxml INPUT.cdx INPUT.cds
//!
//! Each input produces one JSONL record. Failed inputs do not stop the batch.
//! Drawings are exported to CDXML, CDX and full CDS stationery and reimported.
//! CDS inputs use the document-style pipeline unless --drawing-cds is passed.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki_io::{
    document_styles,
    engine::{LocalEngine, Request},
    exchange, formats,
};
use reshiki_model::{document::Document, graphics::GraphicKind};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

fn milliseconds(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.
}

fn failed(error: impl std::fmt::Display, started: Instant) -> Value {
    json!({
        "status": "error",
        "error": error.to_string(),
        "warnings": [],
        "duration_ms": milliseconds(started),
    })
}

fn counts(document: &Document) -> Value {
    let mut graphic_kinds = BTreeMap::new();
    for graphic in &document.graphics {
        *graphic_kinds
            .entry(graphic.kind.to_string())
            .or_insert(0usize) += 1;
    }
    json!({
        "atoms": document.atoms.len(),
        "bonds": document.bonds.len(),
        "graphics": document.graphics.len(),
        "graphic_kinds": graphic_kinds,
        "pictures": document.graphics.iter().filter(|g| g.kind == GraphicKind::Picture).count(),
        "text": document.annotations.len(),
        "text_characters": document.annotations.iter().map(|a| a.text.chars().count()).sum::<usize>(),
        "arrows": document.arrows.len(),
        "groups": document.groups.len(),
        // No page_layout means the model has no explicit publication-page grid.
        "pages": document.page_layout.as_ref().map(|layout| layout.count()),
        "abbreviations": document.abbreviations.len(),
        "ring_fills": document.ring_fills.len(),
        "reactions": document.reactions.len(),
        "repeats": document.generic.repeats.len(),
        "alternatives": document.generic.alternatives.len(),
        "links": document.generic.links.len(),
        "arrow_labels": document.arrow_labels.len(),
        "molecule_names": document.molecule_names.len(),
        "property_captions": document.property_captions.len(),
    })
}

fn graphic_bounds(document: &Document) -> Value {
    let mut bounds: Option<[f32; 4]> = None;
    for graphic in &document.graphics {
        let (minimum, maximum) = graphic.bounds();
        let current = [minimum.x, minimum.y, maximum.x, maximum.y];
        bounds = Some(match bounds {
            Some(previous) => [
                previous[0].min(current[0]),
                previous[1].min(current[1]),
                previous[2].max(current[2]),
                previous[3].max(current[3]),
            ],
            None => current,
        });
    }
    if let Some(boundary) = &mut bounds {
        let anchor = document
            .atoms
            .iter()
            .fold(None, |anchor: Option<[f32; 2]>, atom| {
                Some(match anchor {
                    Some(previous) => [
                        previous[0].min(atom.position.x),
                        previous[1].min(atom.position.y),
                    ],
                    None => [atom.position.x, atom.position.y],
                })
            })
            .unwrap_or([boundary[0], boundary[1]]);
        for index in 0..4 {
            boundary[index] -= anchor[index % 2];
        }
    }
    json!(bounds)
}

async fn import_drawing(
    bytes: Vec<u8>,
    format: &str,
    engine: &LocalEngine,
) -> (Value, Option<Document>) {
    let started = Instant::now();
    match formats::read_with_warnings(bytes, format, engine).await {
        Ok((document, warnings)) => (
            json!({
                "status": "ok",
                "warnings": warnings,
                "counts": counts(&document),
                "drawing_style": document.drawing_style,
                "graphic_bounds_relative_to_anchor": graphic_bounds(&document),
                "duration_ms": milliseconds(started),
            }),
            Some(document),
        ),
        Err(error) => (failed(error, started), None),
    }
}

fn save_artifact(bytes: Result<Vec<u8>, String>, destination: &Path, started: Instant) -> Value {
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(error) => return failed(error, started),
    };
    let write = std::fs::create_dir_all(destination.parent().unwrap_or(Path::new(".")))
        .and_then(|()| std::fs::write(destination, &bytes));
    match write {
        Ok(()) => json!({
            "status": "ok",
            "path": destination,
            "bytes": bytes.len(),
            "duration_ms": milliseconds(started),
        }),
        Err(error) => failed(error, started),
    }
}

async fn drawing_roundtrip(
    document: &Document,
    format: &str,
    destination: &Path,
    engine: &LocalEngine,
) -> Value {
    let started = Instant::now();
    let mut request = Request::molecule("export", document.clone());
    request.format = Some(if format == "cds" { "cdxml" } else { format }.into());
    let response = match engine.request(request).await {
        Ok(response) => response,
        Err(error) => return json!({ "export": failed(error, started) }),
    };
    let result = response
        .output
        .ok_or_else(|| format!("The chemistry engine returned no {format} output"))
        .and_then(|output| {
            if format == "cds" {
                exchange::to_cds(&output)
            } else if format == "cdx" {
                STANDARD.decode(output).map_err(|e| e.to_string())
            } else {
                Ok(output.into_bytes())
            }
        });
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            let mut export = failed(error, started);
            export["warnings"] = json!(response.warnings);
            return json!({ "export": export });
        }
    };
    let write = std::fs::create_dir_all(destination.parent().unwrap_or(Path::new(".")))
        .and_then(|()| std::fs::write(destination, &bytes));
    let mut export = json!({
        "status": "ok",
        "warnings": response.warnings,
        "path": destination,
        "bytes": bytes.len(),
        "duration_ms": milliseconds(started),
    });
    if let Err(error) = write {
        export["status"] = json!("error");
        export["write_error"] = json!(error.to_string());
        return json!({ "export": export });
    }
    let (reimport, reimported) =
        import_drawing(bytes, if format == "cds" { "cdx" } else { format }, engine).await;
    let mut result = json!({
        "export": export,
        "reimport": reimport,
        "counts_match": reimported.as_ref().map(|drawing| counts(drawing) == counts(document)),
        "drawing_style_match": reimported.as_ref().map(|drawing| drawing.drawing_style == document.drawing_style),
    });
    if let Some(reimported) = reimported {
        let started = Instant::now();
        result["snapshot"] = save_artifact(
            reimported.file_json(),
            &destination.with_extension(format!("{format}.rsk")),
            started,
        );
    }
    result
}

fn style_probe(path: &Path, bytes: Vec<u8>, destination: &Path) -> Value {
    let started = Instant::now();
    let style = match document_styles::load_bytes(path, bytes) {
        Ok(style) => style,
        Err(error) => return json!({ "import": failed(error, started) }),
    };
    let import = json!({
        "status": "ok",
        "style": style,
        "warnings": [],
        "duration_ms": milliseconds(started),
    });
    let started = Instant::now();
    let bytes = match document_styles::to_bytes(destination, &style) {
        Ok(bytes) => bytes,
        Err(error) => return json!({ "import": import, "export": failed(error, started) }),
    };
    let write = std::fs::create_dir_all(destination.parent().unwrap_or(Path::new(".")))
        .and_then(|()| std::fs::write(destination, &bytes));
    let mut export = json!({
        "status": "ok",
        "warnings": [],
        "path": destination,
        "bytes": bytes.len(),
        "duration_ms": milliseconds(started),
    });
    if let Err(error) = write {
        export["status"] = json!("error");
        export["write_error"] = json!(error.to_string());
        return json!({ "import": import, "export": export });
    }
    let started = Instant::now();
    match document_styles::load_bytes(destination, bytes) {
        Ok(mut restored) => {
            // The filename supplies the style's display name on each load.
            restored.name.clone_from(&style.name);
            json!({
                "import": import,
                "export": export,
                "reimport": {
                    "status": "ok",
                    "style": restored,
                    "warnings": [],
                    "duration_ms": milliseconds(started),
                },
                "settings_match": restored == style,
            })
        }
        Err(error) => json!({
            "import": import,
            "export": export,
            "reimport": failed(error, started),
        }),
    }
}

async fn probe(path: &Path, destination: &Path, engine: &LocalEngine, drawing_cds: bool) -> Value {
    let started = Instant::now();
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut record = json!({ "path": path, "format": format });
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record["import"] = failed(
                format!("Could not read {}: {error}", path.display()),
                started,
            );
            record["duration_ms"] = json!(milliseconds(started));
            return record;
        }
    };
    record["bytes"] = json!(bytes.len());
    if format == "cds" && !drawing_cds {
        record["mode"] = json!("style");
        record["style"] = style_probe(path, bytes, &destination.join("roundtrip.cds"));
    } else if matches!(format.as_str(), "cdxml" | "cdx" | "cds") {
        record["mode"] = json!("drawing");
        let (import, document) =
            import_drawing(bytes, if format == "cds" { "cdx" } else { &format }, engine).await;
        record["import"] = import;
        if let Some(document) = document {
            let started = Instant::now();
            record["artifacts"]["snapshot"] = save_artifact(
                document.file_json(),
                &destination.join("imported.rsk"),
                started,
            );
            if format == "cdxml" {
                let started = Instant::now();
                record["artifacts"]["preview"] = save_artifact(
                    formats::write_with_engine(document.clone(), "svg", engine).await,
                    &destination.join("imported.svg"),
                    started,
                );
            }
            for target in ["cdxml", "cdx", "cds"] {
                record["roundtrips"][target] = drawing_roundtrip(
                    &document,
                    target,
                    &destination.join(format!("roundtrip.{target}")),
                    engine,
                )
                .await;
            }
        }
    } else {
        record["import"] = failed("Expected a .cdxml, .cdx or .cds input", started);
    }
    record["duration_ms"] = json!(milliseconds(started));
    record
}

async fn run() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let mut output = None;
    let mut drawing_cds = false;
    let mut paths = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--output-dir" {
            output = Some(PathBuf::from(
                arguments.next().ok_or("--output-dir requires a path")?,
            ));
        } else if argument == "--drawing-cds" {
            drawing_cds = true;
        } else if argument == "--help" || argument == "-h" {
            println!(
                "Usage: chemdraw_import_probe --output-dir DIRECTORY [--drawing-cds] INPUT ...\n\
                 Outputs one JSONL record per input. CDS files probe styles by default;\n\
                 --drawing-cds imports CDS artwork through the production CDX pipeline.\n\
                 Exported files are saved under DIRECTORY/INDEX-INPUT_NAME/.\n\
                 A null pages count means the imported model has no explicit publication-page grid."
            );
            return Ok(());
        } else {
            paths.push(PathBuf::from(argument));
        }
    }
    let output = output.ok_or("Specify --output-dir DIRECTORY")?;
    if paths.is_empty() {
        return Err("Specify at least one .cdxml, .cdx or .cds input".into());
    }
    std::fs::create_dir_all(&output).map_err(|error| error.to_string())?;
    let engine = LocalEngine::in_process();
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    for (index, path) in paths.iter().enumerate() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let destination = output.join(format!("{index:04}-{name}"));
        let record = probe(path, &destination, &engine, drawing_cds).await;
        serde_json::to_writer(&mut stdout, &record).map_err(|error| error.to_string())?;
        writeln!(stdout).map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("chemdraw_import_probe: {error}");
        std::process::exit(1);
    }
}
