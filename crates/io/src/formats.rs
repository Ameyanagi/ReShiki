//! Drawings read from and written to named formats, as the CLI and the MCP
//! tools exchange them. The browser library and editor use these for their
//! JavaScript API, so their results match the desktop's.
use crate::{
    compatibility,
    engine::{LocalEngine, Request, text_format},
    export::{self, Publication},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki_model::{
    atom_labels,
    document::{Document, History},
    transaction,
};

/// The most PNG pixels a figure export may have, as the MCP `export` tool's
/// default budget allows.
pub const EXPORT_PNG_PIXELS: u64 = 16_000_000;

/// Read `data` as `format`: `rsk` (a native drawing), `mol`, `smiles`, `inchi`,
/// `cdxml`, `cdx` (the binary file), `rxn`, `rsmi` or `auto`, which tells the
/// text formats apart.
pub async fn read(data: Vec<u8>, format: &str) -> Result<Document, String> {
    read_with_engine(data, format, &LocalEngine::default()).await
}

/// [`read`] with an explicitly configured headless chemistry engine.
pub async fn read_with_engine(
    data: Vec<u8>,
    format: &str,
    engine: &LocalEngine,
) -> Result<Document, String> {
    if matches!(format, "rsk" | "reshiki") {
        return Document::from_native_file(&data);
    }
    let text = compatibility::import_text(format, data)?;
    let format = match format {
        "auto" => text_format(&text),
        format => format,
    };
    let mut request = Request::import(format, "");
    request.text = Some(text);
    let response = engine.request(request).await?;
    let document = response.document.ok_or("Import returned no drawing")?;
    finalize(&document)
}

/// The engine's drawing as the editor keeps it, as the `import` tool stores it.
fn finalize(response: &Document) -> Result<Document, String> {
    let mut doc = response.clone();
    transaction::reconcile(&mut doc, Document::default())
        .map_err(|rejection| rejection.message().to_owned())?
        .commit(&mut doc, &mut History::default(), false);
    atom_labels::refresh_computed(&mut doc, response);
    Ok(doc)
}

/// Write `doc` as `format`: `rsk`, a figure (`svg`, `png` or `pdf`, as the
/// app's Export makes it) or a chemistry format (`mol`, `smiles`, `inchi`,
/// `cdxml`, `cdx`, `rxn` or `rsmi`).
pub async fn write(doc: Document, format: &str) -> Result<Vec<u8>, String> {
    write_with_engine(doc, format, &LocalEngine::default()).await
}

/// [`write`] with an explicitly configured headless chemistry engine.
pub async fn write_with_engine(
    doc: Document,
    format: &str,
    engine: &LocalEngine,
) -> Result<Vec<u8>, String> {
    if let Some(figure) = ["svg", "png", "pdf"].into_iter().find(|f| *f == format) {
        return export::publication(engine, doc, figure, false, EXPORT_PNG_PIXELS)
            .await
            .map(|Publication { bytes, .. }| bytes);
    }
    match format {
        "rsk" | "reshiki" => doc.file_json(),
        "mol" | "smiles" | "inchi" | "cdxml" | "cdx" | "rxn" | "rsmi" => {
            let mut request = Request::molecule("export", doc);
            request.format = Some(format.into());
            let output = engine
                .request(request)
                .await?
                .output
                .ok_or_else(|| format!("The chemistry engine returned no {format} output"))?;
            if format == "cdx" {
                STANDARD
                    .decode(output)
                    .map_err(|_| "Invalid binary drawing base64".to_owned())
            } else {
                Ok(output.into_bytes())
            }
        }
        format => Err(format!("Unknown output format {format}")),
    }
}
