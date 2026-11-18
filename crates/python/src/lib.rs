//! Thin Python bindings over the same headless import, export and analysis API
//! used by the browser. Python owns the public objects and integrations.
#![forbid(unsafe_code)]

mod drawing;
mod render;
mod styles;

use pyo3::{exceptions::PyValueError, prelude::*, types::PyBytes};

/// `reshiki.ReShikiError(message, code)`, the package's coded `ValueError`.
/// If the Python class cannot be loaded, a plain `ValueError` still carries
/// the message.
fn coded(py: Python<'_>, code: &str, message: String) -> PyErr {
    py.import("reshiki._errors")
        .and_then(|module| module.getattr("ReShikiError"))
        .and_then(|class| class.call1((message.as_str(), code)))
        .map_or_else(|_| PyValueError::new_err(message), PyErr::from_value)
}

/// The structure could not be read, drawn or written.
fn failed(py: Python<'_>) -> impl Fn(String) -> PyErr {
    move |message| coded(py, "failed", message)
}

/// A format, option or style is not accepted.
fn invalid(py: Python<'_>) -> impl Fn(String) -> PyErr {
    move |message| coded(py, "invalid_arguments", message)
}
use rayon::prelude::*;
use reshiki_model::document::Document;

fn bytes(py: Python<'_>, data: Vec<u8>) -> Py<PyBytes> {
    PyBytes::new(py, &data).unbind()
}

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction(signature = (data, format="auto"))]
fn parse(
    py: Python<'_>,
    data: Vec<u8>,
    format: &str,
) -> PyResult<(Py<PyBytes>, String, Vec<String>)> {
    if !drawing::is_input_format(format) {
        return Err(invalid(py)(format!("Unsupported input format: '{format}'")));
    }
    let (data, kind, warnings) = py
        .detach(|| {
            drawing::run(async {
                let (document, warnings) = drawing::read(data, format).await?;
                let (data, kind) = drawing::serialize(document)?;
                Ok((data, kind, warnings))
            })
        })
        .map_err(failed(py))?;
    Ok((bytes(py, data), kind, warnings))
}

#[pyfunction]
fn from_name(py: Python<'_>, name: &str) -> PyResult<(Py<PyBytes>, String, Vec<String>)> {
    let (data, kind, warnings) = py
        .detach(|| drawing::run(drawing::from_name(name)))
        .map_err(failed(py))?;
    Ok((bytes(py, data), kind, warnings))
}

#[pyfunction]
fn parse_online_smiles(
    py: Python<'_>,
    smiles: &str,
) -> PyResult<(Py<PyBytes>, String, Vec<String>)> {
    let (data, kind, warnings) = py
        .detach(|| drawing::run(drawing::parse_online_smiles(smiles)))
        .map_err(failed(py))?;
    Ok((bytes(py, data), kind, warnings))
}

#[pyfunction]
fn clean(py: Python<'_>, data: Vec<u8>) -> PyResult<Py<PyBytes>> {
    let data = py
        .detach(|| drawing::run(drawing::clean(&data)))
        .map_err(failed(py))?;
    Ok(bytes(py, data))
}

#[pyfunction]
fn with_style(py: Python<'_>, data: Vec<u8>, style_json: &str) -> PyResult<Py<PyBytes>> {
    let data = py
        .detach(|| styles::apply(Document::from_native_file(&data)?, Some(style_json))?.file_json())
        .map_err(invalid(py))?;
    Ok(bytes(py, data))
}

#[pyfunction(name = "styles")]
fn list_styles(py: Python<'_>) -> PyResult<String> {
    styles::presets().map_err(failed(py))
}

#[pyfunction(signature = (name, overrides_json="{}"))]
fn style(py: Python<'_>, name: &str, overrides_json: &str) -> PyResult<String> {
    styles::named(name, overrides_json).map_err(invalid(py))
}

// The arguments mirror the Python keywords.
#[allow(clippy::too_many_arguments)]
#[pyfunction(name = "render", signature = (data, format="png", style_json=None, width=None, height=None, dpi=None, svg_text="outline"))]
fn render_drawing(
    py: Python<'_>,
    data: Vec<u8>,
    format: &str,
    style_json: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    dpi: Option<f64>,
    svg_text: &str,
) -> PyResult<Py<PyBytes>> {
    let options =
        render::Options::new(style_json, width, height, dpi, svg_text).map_err(invalid(py))?;
    let data = py
        .detach(|| {
            drawing::run(async {
                render::document(Document::from_native_file(&data)?, format, &options).await
            })
        })
        .map_err(failed(py))?;
    Ok(bytes(py, data))
}

#[pyfunction]
fn properties(py: Python<'_>, data: Vec<u8>) -> PyResult<String> {
    py.detach(|| drawing::run(drawing::properties(&data)))
        .map_err(failed(py))
}

#[pyfunction]
fn predict_nmr(py: Python<'_>, data: Vec<u8>, nucleus: &str) -> PyResult<String> {
    let nucleus = reshiki_io::nmr::Nucleus::from_code(nucleus)
        .ok_or_else(|| invalid(py)("nucleus must be 1H or 13C".into()))?;
    py.detach(|| drawing::predict_nmr(&data, nucleus))
        .map_err(failed(py))
}

#[pyfunction]
fn with_labels(
    py: Python<'_>,
    data: Vec<u8>,
    rows: Vec<(u64, Option<f64>)>,
    format: &str,
    decimals: Option<i64>,
    nucleus: Option<&str>,
) -> PyResult<Py<PyBytes>> {
    use reshiki_io::nmr::{LabelFormat, MAX_DECIMALS, Nucleus, default_decimals};
    let format = LabelFormat::from_code(format)
        .ok_or_else(|| invalid(py)("format must be value, numbers, ppm or both".into()))?;
    let decimals = match decimals {
        None => default_decimals(nucleus.and_then(Nucleus::from_code)),
        Some(decimals) => u8::try_from(decimals)
            .ok()
            .filter(|decimals| *decimals <= MAX_DECIMALS)
            .ok_or_else(|| invalid(py)(format!("decimals must be 0 to {MAX_DECIMALS}")))?,
    };
    let data = py
        .detach(|| {
            let before = Document::from_native_file(&data)?;
            let mut document = before.clone();
            reshiki_io::nmr::write_labels(&mut document, &rows, format, decimals)?;
            // There is no history to commit to: the reconciled document is
            // the result. As the app and MCP reconcile an atom-number edit,
            // reaction gaps and their signs make room for wider labels.
            if document != before {
                drop(
                    reshiki_model::transaction::reconcile(&mut document, before)
                        .map_err(|rejection| rejection.message().to_owned())?,
                );
            }
            document.file_json()
        })
        .map_err(invalid(py))?;
    Ok(bytes(py, data))
}

/// Sets each `(atom, text)` as that atom's number, keeping its style, for
/// RDKit atom notes; the caller has checked the atoms and texts.
#[pyfunction]
fn with_atom_numbers(
    py: Python<'_>,
    data: Vec<u8>,
    numbers: Vec<(u64, String)>,
) -> PyResult<Py<PyBytes>> {
    use reshiki_model::atom_labels::{Number, number_style};
    let data = py
        .detach(|| {
            let mut document = Document::from_native_file(&data)?;
            for (id, text) in numbers {
                if let Some(atom) = document.atom_mut(id) {
                    let style = atom
                        .display
                        .number
                        .take()
                        .map_or_else(number_style, |number| number.style);
                    atom.display.number = Some(Number {
                        text,
                        offset: None,
                        style,
                    });
                }
            }
            document.validate()?;
            document.file_json()
        })
        .map_err(invalid(py))?;
    Ok(bytes(py, data))
}

type BatchOutput = (
    Vec<Option<Py<PyBytes>>>,
    Vec<(usize, String)>,
    Vec<(usize, String)>,
);

// The arguments mirror the Python keywords.
#[allow(clippy::too_many_arguments)]
#[pyfunction(signature = (items, format="png", style_json=None, width=None, height=None, dpi=None, svg_text="outline"))]
fn render_many(
    py: Python<'_>,
    items: Vec<(Vec<u8>, String)>,
    format: &str,
    style_json: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    dpi: Option<f64>,
    svg_text: &str,
) -> PyResult<BatchOutput> {
    let options =
        render::Options::new(style_json, width, height, dpi, svg_text).map_err(invalid(py))?;
    let results: Vec<_> = py.detach(|| {
        items
            .into_par_iter()
            .map(|(data, input_format)| {
                let mut warnings = Vec::new();
                let result = drawing::run(async {
                    let (document, messages) = drawing::read(data, &input_format).await?;
                    warnings = messages;
                    render::document(document, format, &options).await
                });
                (result, warnings)
            })
            .collect()
    });
    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    let values = results
        .into_iter()
        .enumerate()
        .map(|(index, (result, messages))| {
            warnings.extend(messages.into_iter().map(|message| (index, message)));
            match result {
                Ok(data) => Some(bytes(py, data)),
                Err(error) => {
                    failures.push((index, error));
                    None
                }
            }
        })
        .collect();
    Ok((values, failures, warnings))
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(version, module)?)?;
    module.add_function(wrap_pyfunction!(parse, module)?)?;
    module.add_function(wrap_pyfunction!(from_name, module)?)?;
    module.add_function(wrap_pyfunction!(parse_online_smiles, module)?)?;
    module.add_function(wrap_pyfunction!(clean, module)?)?;
    module.add_function(wrap_pyfunction!(with_style, module)?)?;
    module.add_function(wrap_pyfunction!(list_styles, module)?)?;
    module.add_function(wrap_pyfunction!(style, module)?)?;
    module.add_function(wrap_pyfunction!(render_drawing, module)?)?;
    module.add_function(wrap_pyfunction!(properties, module)?)?;
    module.add_function(wrap_pyfunction!(predict_nmr, module)?)?;
    module.add_function(wrap_pyfunction!(with_labels, module)?)?;
    module.add_function(wrap_pyfunction!(with_atom_numbers, module)?)?;
    module.add_function(wrap_pyfunction!(render_many, module)?)?;
    Ok(())
}
