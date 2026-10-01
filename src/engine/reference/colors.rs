//! Translate current color tags at the optional historical worker boundary.
//! The worker still receives RGB bytes; its chemistry and exports are unchanged.
use crate::palette::Color;
use serde_json::Value;

#[derive(Default)]
pub(super) struct Colors {
    values: Vec<(String, Value, Value)>,
    imported: bool,
}

fn visit(
    value: &mut Value,
    path: &str,
    f: &mut impl FnMut(&mut Value, &str) -> Result<(), String>,
) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                if matches!(key.as_str(), "color" | "stroke" | "fill" | "hydrogen_color") {
                    f(value, &next)?;
                } else {
                    visit(value, &next, f)?;
                }
            }
        }
        Value::Array(array) => {
            for (i, value) in array.iter_mut().enumerate() {
                visit(value, &format!("{path}/{i}"), f)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn prepare(message: &mut Value) -> Result<Colors, String> {
    let mut colors = Colors {
        imported: message["operation"] == "import"
            && matches!(message["format"].as_str(), Some("cdxml" | "cdx")),
        ..Default::default()
    };
    if let Some(document) = message.get_mut("document") {
        visit(document, "/document", &mut |value, path| {
            if let Some(text) = value.as_str() {
                let color: Color = text.parse()?;
                // The old worker has no document palette. Restrict this bridge
                // to explicit colors rather than silently resolving swatches.
                let Color::Custom(rgb) = color else {
                    return Err("The historical reference requires explicit RGB colors".into());
                };
                let original = value.clone();
                *value = serde_json::json!(rgb);
                colors.values.push((path.into(), original, value.clone()));
            }
            Ok(())
        })?;
    }
    Ok(colors)
}

impl Colors {
    pub(super) fn restore(self, result: &mut Value) -> Result<(), String> {
        for (path, original, rgb) in self.values {
            if let Some(value) = result.pointer_mut(&path)
                && *value == rgb
            {
                // Restore the tag only after checking the worker's actual RGB;
                // changed/missing colors remain visible to comparisons.
                *value = original;
            }
        }
        if self.imported
            && let Some(document) = result.get_mut("document")
        {
            // An RGB value returned by this external reader is explicit, even
            // when it happens to equal an old ReShiki palette swatch.
            visit(document, "/document", &mut |value, _| {
                if value.is_array() {
                    let rgb = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                    *value =
                        serde_json::to_value(Color::imported(rgb)).map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bridge_preserves_explicit_colors_and_does_not_rewrite_text_or_changed_bytes() {
        let original = json!({"document": {"bonds": [{"color": "#B43237"}], "annotations": [{"text": "#B43237"}]}});
        let mut message = original.clone();
        let colors = prepare(&mut message).unwrap();
        assert_eq!(
            message["document"]["bonds"][0]["color"],
            json!([180, 50, 55])
        );
        assert_eq!(message["document"]["annotations"][0]["text"], "#B43237");
        colors.restore(&mut message).unwrap();
        assert_eq!(message, original);

        let mut changed = original.clone();
        let colors = prepare(&mut changed).unwrap();
        changed["document"]["bonds"][0]["color"] = json!([180, 50, 56]);
        colors.restore(&mut changed).unwrap();
        assert_eq!(
            changed["document"]["bonds"][0]["color"],
            json!([180, 50, 56])
        );
        assert_ne!(changed, original);
    }

    #[test]
    fn imported_swatch_bytes_remain_explicit_and_palette_requests_are_rejected() {
        let mut request = json!({"operation": "import", "format": "cdxml"});
        let colors = prepare(&mut request).unwrap();
        let mut response = json!({"document": {"bonds": [{"color": [180, 50, 55]}]}});
        colors.restore(&mut response).unwrap();
        assert_eq!(response["document"]["bonds"][0]["color"], "#B43237");

        let mut palette = json!({"document": {"bonds": [{"color": "red.strong"}]}});
        assert!(prepare(&mut palette).is_err());
        let mut black = json!({"document": {"bonds": [{"color": "#000000"}]}});
        let colors = prepare(&mut black).unwrap();
        colors.restore(&mut black).unwrap();
        assert_eq!(black["document"]["bonds"][0]["color"], "#000000");
    }
}
