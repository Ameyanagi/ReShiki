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
    read_with_warnings(data, format, engine)
        .await
        .map(|(doc, _)| doc)
}

/// [`read_with_engine`] with import warnings in the order the engine reports them.
pub async fn read_with_warnings(
    data: Vec<u8>,
    format: &str,
    engine: &LocalEngine,
) -> Result<(Document, Vec<String>), String> {
    if matches!(format, "rsk" | "reshiki") {
        return Document::from_native_file(&data).map(|doc| (doc, Vec::new()));
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
    Ok((finalize(&document)?, response.warnings))
}

/// The engine's drawing as the editor keeps it, as the `import` tool stores it.
pub use crate::chemistry::cdxml::schemes::Loss as ChemdrawLoss;

/// What a save in ChemDraw format loses from the ChemDraw file `contents`
/// (`cdxml`, or `cdx` as base64, as [`compatibility::import_text`] gives it).
/// Nothing for another format or a file that cannot be read.
pub fn chemdraw_loss(format: &str, contents: &str) -> ChemdrawLoss {
    use crate::exchange::cycles::{self, Found};
    let read = match format {
        "cdxml" => {
            let (xml, found) = cycles::take_cdxml(contents);
            Ok((xml.into_owned(), found))
        }
        "cdx" => STANDARD
            .decode(contents)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                let (bytes, found) = cycles::take_cdx(&bytes);
                Ok((crate::exchange::from_cdx_tolerant(&bytes)?, found))
            }),
        _ => return ChemdrawLoss::default(),
    };
    let Ok((xml, found)) = read else {
        return ChemdrawLoss::default();
    };
    let mut loss = crate::chemistry::cdxml::schemes::loss(&xml).unwrap_or_default();
    // The cycles that come back write their own scheme and steps again.
    if let Some(Found::Fresh(payload)) = found {
        for n in payload
            .cycles
            .iter()
            .filter_map(|saved| saved.graphic.cycle.as_deref())
            .map(|c| c.intermediates.len())
            .filter(|n| *n > 0)
        {
            loss.schemes = loss.schemes.saturating_sub(1);
            loss.steps = loss.steps.saturating_sub(n);
        }
    }
    loss
}

fn finalize(response: &Document) -> Result<Document, String> {
    response.validate()?;
    let mut doc = Document::default();
    transaction::replace(&mut doc, response.clone()).commit(
        &mut doc,
        &mut History::default(),
        false,
    );
    atom_labels::copy_computed(&mut doc, response);
    Ok(doc)
}

/// Whether SVG output turns text into paths or keeps live text elements.
/// Every surface names the option `svg_text` (`svgText` in JavaScript) and
/// outlines by default, for the same appearance in every viewer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SvgText {
    #[default]
    Outline,
    Live,
}

impl SvgText {
    /// `outline` or `live`; `None` for anything else.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "outline" => Some(Self::Outline),
            "live" => Some(Self::Live),
            _ => None,
        }
    }
}

/// The `format` names [`read`] accepts.
pub const INPUT: &[&str] = &[
    "auto", "rsk", "reshiki", "mol", "smiles", "inchi", "cdxml", "cdx", "rxn", "rsmi",
];

/// The `format` names [`write`] accepts.
pub const OUTPUT: &[&str] = &[
    "rsk", "reshiki", "svg", "png", "pdf", "mol", "smiles", "inchi", "cdxml", "cdx", "rxn", "rsmi",
];

/// Write `doc` as `format`: `rsk`, a figure (`svg`, `png` or `pdf`, as the
/// app's Export makes it) or a chemistry format (`mol`, `smiles`, `inchi`,
/// `cdxml`, `cdx`, `rxn` or `rsmi`).
pub async fn write(doc: Document, format: &str) -> Result<Vec<u8>, String> {
    write_with_engine(doc, format, &LocalEngine::default()).await
}

/// [`write`] with an explicitly configured headless chemistry engine. SVG
/// keeps live text, as the app's plain SVG export does.
pub async fn write_with_engine(
    doc: Document,
    format: &str,
    engine: &LocalEngine,
) -> Result<Vec<u8>, String> {
    write_with(doc, format, engine, SvgText::Live).await
}

/// [`write_with_engine`] with the SVG text mode: outlined SVG is the app's
/// SVG for Office, as the `export` tool makes it.
pub async fn write_with(
    doc: Document,
    format: &str,
    engine: &LocalEngine,
    svg_text: SvgText,
) -> Result<Vec<u8>, String> {
    if let Some(figure) = ["svg", "png", "pdf"].into_iter().find(|f| *f == format) {
        let figure = if figure == "svg" && svg_text == SvgText::Outline {
            "svg-office"
        } else {
            figure
        };
        return export::publication(
            engine,
            doc,
            figure,
            false,
            false,
            export::Background::Canvas,
            crate::style::DEFAULT.png_dpi,
            EXPORT_PNG_PIXELS,
        )
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
