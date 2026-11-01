use reshiki_io::document_styles::{self, Preset};
use reshiki_model::{document::Document, style::DrawingStyle};
use serde_json::{Map, Value};

pub fn presets() -> Result<String, String> {
    let mut values = Map::new();
    for preset in Preset::ALL.into_iter().chain([Preset::Presentation]) {
        values.insert(
            preset.to_string(),
            serde_json::to_value(preset.style()).map_err(|error| error.to_string())?,
        );
    }
    serde_json::to_string(&values).map_err(|error| error.to_string())
}

pub fn named(name: &str, overrides: &str) -> Result<String, String> {
    let preset = Preset::ALL
        .into_iter()
        .chain([Preset::Presentation])
        .find(|preset| {
            preset.to_string().eq_ignore_ascii_case(name) || preset.id().eq_ignore_ascii_case(name)
        })
        .ok_or_else(|| format!("Unknown drawing style {name:?}"))?;
    serde_json::to_string(&merge(&preset.style(), overrides)?).map_err(|error| error.to_string())
}

pub fn merge(base: &DrawingStyle, overrides: &str) -> Result<DrawingStyle, String> {
    let overrides: Value =
        serde_json::from_str(overrides).map_err(|error| format!("Invalid style: {error}"))?;
    let overrides = overrides
        .as_object()
        .ok_or("Style overrides must be an object")?;
    let mut value = serde_json::to_value(base).map_err(|error| error.to_string())?;
    let fields = value.as_object_mut().ok_or("Invalid drawing style")?;
    // Omitted from serialized styles while false.
    fields
        .entry("center_chain_double_bonds")
        .or_insert(false.into());
    let mut bond_length_changed = false;
    for (key, value) in overrides {
        let field = match key.as_str() {
            "font_size" => "font_size_pt",
            "bond_length" => "bond_length_pt",
            "line_width" => "line_width_pt",
            "bold_width" => "bold_width_pt",
            "margin_width" => "margin_width_pt",
            "hash_spacing" => "hash_spacing_pt",
            "bond_spacing" => "bond_spacing_ratio",
            other => other,
        };
        if !fields.contains_key(field) {
            return Err(format!("Unknown style option {key:?}"));
        }
        if field == "bond_length_pt" {
            bond_length_changed = true;
        }
        fields.insert(field.into(), value.clone());
    }
    let mut style: DrawingStyle =
        serde_json::from_value(value).map_err(|error| format!("Invalid style: {error}"))?;
    if bond_length_changed && !overrides.contains_key("bond_length_world") {
        style.set_bond_length(style.bond_length_pt);
    }
    style.validate()?;
    Ok(style)
}

pub fn apply(document: Document, overrides: Option<&str>) -> Result<Document, String> {
    match overrides {
        Some(overrides) => {
            let style = merge(&document.drawing_style, overrides)?;
            document_styles::apply(&document, style, true, true)
        }
        None => Ok(document),
    }
}
