//! Thin Python bindings over the same headless import, export and analysis API
//! used by the browser. Python owns the public objects and integrations.
#![forbid(unsafe_code)]

mod drawing;
mod render;
mod styles;

use pyo3::{
    exceptions::{PyNotImplementedError, PyValueError},
    prelude::*,
    types::PyBytes,
};
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
fn parse(py: Python<'_>, data: Vec<u8>, format: &str) -> PyResult<(Py<PyBytes>, String)> {
    let (data, kind) = py
        .detach(|| drawing::run(async { drawing::serialize(drawing::read(data, format).await?) }))
        .map_err(PyValueError::new_err)?;
    Ok((bytes(py, data), kind))
}

#[pyfunction(signature = (name, online=false))]
fn from_name(py: Python<'_>, name: &str, online: bool) -> PyResult<(Py<PyBytes>, String)> {
    if online {
        return Err(PyNotImplementedError::new_err(
            "Online PubChem name lookup requires a headless client in a follow-up; use offline names with online=False",
        ));
    }
    let (data, kind) = py
        .detach(|| drawing::run(drawing::from_name(name)))
        .map_err(PyValueError::new_err)?;
    Ok((bytes(py, data), kind))
}

#[pyfunction]
fn clean(py: Python<'_>, data: Vec<u8>) -> PyResult<Py<PyBytes>> {
    let data = py
        .detach(|| drawing::run(drawing::clean(&data)))
        .map_err(PyValueError::new_err)?;
    Ok(bytes(py, data))
}

#[pyfunction]
fn with_style(py: Python<'_>, data: Vec<u8>, style_json: &str) -> PyResult<Py<PyBytes>> {
    let data = py
        .detach(|| styles::apply(Document::from_native_file(&data)?, Some(style_json))?.file_json())
        .map_err(PyValueError::new_err)?;
    Ok(bytes(py, data))
}

#[pyfunction(name = "styles")]
fn list_styles() -> PyResult<String> {
    styles::presets().map_err(PyValueError::new_err)
}

#[pyfunction(signature = (name, overrides_json="{}"))]
fn style(name: &str, overrides_json: &str) -> PyResult<String> {
    styles::named(name, overrides_json).map_err(PyValueError::new_err)
}

#[pyfunction(name = "render", signature = (data, format="png", style_json=None, width=None, height=None, dpi=None))]
fn render_drawing(
    py: Python<'_>,
    data: Vec<u8>,
    format: &str,
    style_json: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    dpi: Option<f64>,
) -> PyResult<Py<PyBytes>> {
    let options =
        render::Options::new(style_json, width, height, dpi).map_err(PyValueError::new_err)?;
    let data = py
        .detach(|| {
            drawing::run(async {
                render::document(Document::from_native_file(&data)?, format, &options).await
            })
        })
        .map_err(PyValueError::new_err)?;
    Ok(bytes(py, data))
}

#[pyfunction]
fn properties(py: Python<'_>, data: Vec<u8>) -> PyResult<String> {
    py.detach(|| drawing::run(drawing::properties(&data)))
        .map_err(PyValueError::new_err)
}

type BatchOutput = (Vec<Option<Py<PyBytes>>>, Vec<(usize, String)>);

#[pyfunction(signature = (items, format="png", style_json=None, width=None, height=None, dpi=None))]
fn render_many(
    py: Python<'_>,
    items: Vec<(Vec<u8>, String)>,
    format: &str,
    style_json: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    dpi: Option<f64>,
) -> PyResult<BatchOutput> {
    let options =
        render::Options::new(style_json, width, height, dpi).map_err(PyValueError::new_err)?;
    let results: Vec<_> = py.detach(|| {
        items
            .into_par_iter()
            .map(|(data, input_format)| {
                drawing::run(async {
                    render::document(drawing::read(data, &input_format).await?, format, &options)
                        .await
                })
            })
            .collect()
    });
    let mut failures = Vec::new();
    let values = results
        .into_iter()
        .enumerate()
        .map(|(index, result)| match result {
            Ok(data) => Some(bytes(py, data)),
            Err(error) => {
                failures.push((index, error));
                None
            }
        })
        .collect();
    Ok((values, failures))
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(version, module)?)?;
    module.add_function(wrap_pyfunction!(parse, module)?)?;
    module.add_function(wrap_pyfunction!(from_name, module)?)?;
    module.add_function(wrap_pyfunction!(clean, module)?)?;
    module.add_function(wrap_pyfunction!(with_style, module)?)?;
    module.add_function(wrap_pyfunction!(list_styles, module)?)?;
    module.add_function(wrap_pyfunction!(style, module)?)?;
    module.add_function(wrap_pyfunction!(render_drawing, module)?)?;
    module.add_function(wrap_pyfunction!(properties, module)?)?;
    module.add_function(wrap_pyfunction!(render_many, module)?)?;
    Ok(())
}
