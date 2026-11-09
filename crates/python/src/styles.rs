use reshiki_io::document_styles::{self, Preset};
use reshiki_model::{document::Document, style::DrawingStyle};
use serde_json::{Map, Value};

fn complete_style_value(style: &DrawingStyle) -> Result<Value, String> {
    // Preserve the native serializer's f32 dimension representation.
    let serialized = serde_json::to_string(style).map_err(|error| error.to_string())?;
    let mut value: Value = serde_json::from_str(&serialized).map_err(|error| error.to_string())?;
    // Native documents omit defaults; a saved Python style replaces settings
    // on existing drawings and must carry those defaults explicitly.
    value["center_chain_double_bonds"] = style.center_chain_double_bonds.into();
    value["two_wedge_stereocentres"] = style.two_wedge_stereocentres.into();
    value["ring_fusion_hydrogens"] = style.ring_fusion_hydrogens.into();
    value["wavy_unknown_alkenes"] = style.wavy_unknown_alkenes.into();
    value["charge_before_radical"] = style.charge_before_radical.into();
    value["hydrogen_isotopes"] =
        serde_json::to_value(style.hydrogen_isotopes).map_err(|error| error.to_string())?;
    Ok(value)
}

pub fn presets() -> Result<String, String> {
    let mut values = Map::new();
    for preset in Preset::ALL.into_iter().chain([Preset::Presentation]) {
        values.insert(preset.to_string(), complete_style_value(&preset.style())?);
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
    let style = merge(&preset.style(), overrides)?;
    serde_json::to_string(&complete_style_value(&style)?).map_err(|error| error.to_string())
}

pub fn merge(base: &DrawingStyle, overrides: &str) -> Result<DrawingStyle, String> {
    let overrides: Value =
        serde_json::from_str(overrides).map_err(|error| format!("Invalid style: {error}"))?;
    let overrides = overrides
        .as_object()
        .ok_or("Style overrides must be an object")?;
    let mut value = complete_style_value(base)?;
    let fields = value.as_object_mut().ok_or("Invalid drawing style")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_omitted_default_stereochemistry_options_and_resets_them() {
        let custom = merge(
            &DrawingStyle::default(),
            r#"{"two_wedge_stereocentres":true,"ring_fusion_hydrogens":false,"wavy_unknown_alkenes":true}"#,
        ).unwrap();
        assert!(custom.two_wedge_stereocentres);
        assert!(!custom.ring_fusion_hydrogens);
        assert!(custom.wavy_unknown_alkenes);
        let reset = merge(
            &custom,
            r#"{"two_wedge_stereocentres":false,"ring_fusion_hydrogens":true,"wavy_unknown_alkenes":false}"#,
        ).unwrap();
        assert!(reset.is_default());
        assert!(merge(&reset, r#"{"two_wedge_stereocentres":1}"#).is_err());
        assert!(merge(&reset, r#"{"unknown_style_option":true}"#).is_err());
    }
}
