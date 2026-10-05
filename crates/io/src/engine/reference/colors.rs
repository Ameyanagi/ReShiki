//! Translate current color tags at the optional historical worker boundary.
//! The worker still receives RGB bytes; its chemistry and exports are unchanged.
use crate::palette::Color;
use serde_json::Value;

#[derive(Default)]
pub(super) struct Colors {
    values: Vec<(Vec<Step>, Value, Value)>,
    imported: bool,
}

#[derive(Clone)]
enum Step {
    Key(String),
    Index(usize),
    Id(u64),
    Bond(u64, u64),
}

fn bond(value: &Value) -> Option<(u64, u64)> {
    let (a, b) = (value["a"].as_u64()?, value["b"].as_u64()?);
    Some((a.min(b), a.max(b)))
}

fn locate<'a>(mut value: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get_mut(key)?,
            Step::Index(index) => value.get_mut(*index)?,
            Step::Id(id) => value
                .as_array_mut()?
                .iter_mut()
                .find(|v| v["id"].as_u64() == Some(*id))?,
            Step::Bond(a, b) => value
                .as_array_mut()?
                .iter_mut()
                .find(|v| bond(v) == Some((*a, *b)))?,
        };
    }
    Some(value)
}

fn visit(
    value: &mut Value,
    path: &mut Vec<Step>,
    f: &mut impl FnMut(&mut Value, &[Step]) -> Result<(), String>,
) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                path.push(Step::Key(key.clone()));
                if matches!(key.as_str(), "color" | "stroke" | "fill" | "hydrogen_color") {
                    f(value, path)?;
                } else {
                    visit(value, path, f)?;
                }
                path.pop();
            }
        }
        Value::Array(array) => {
            for (i, value) in array.iter_mut().enumerate() {
                // Abbreviation replacement can remove/reorder atoms and bonds.
                // Preserve identity across that edit; nested ordered spans and
                // auxiliary drawing parts still use their own array positions.
                path.push(if let Some(id) = value["id"].as_u64() {
                    Step::Id(id)
                } else if let Some((a, b)) = bond(value) {
                    Step::Bond(a, b)
                } else {
                    Step::Index(i)
                });
                visit(value, path, f)?;
                path.pop();
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
    for field in ["document", "atom_indicators", "graphic_parts"] {
        if let Some(payload) = message.get_mut(field) {
            visit(
                payload,
                &mut vec![Step::Key(field.into())],
                &mut |value, path| {
                    if let Some(text) = value.as_str() {
                        let color: Color = text.parse()?;
                        // The old worker has no document palette. Restrict this bridge
                        // to explicit colors rather than silently resolving swatches.
                        let Color::Custom(rgb) = color else {
                            return Err(
                                "The historical reference requires explicit RGB colors".into()
                            );
                        };
                        let original = value.clone();
                        *value = serde_json::json!(rgb);
                        colors.values.push((path.to_vec(), original, value.clone()));
                    }
                    Ok(())
                },
            )?;
        }
    }
    Ok(colors)
}

impl Colors {
    pub(super) fn restore(self, result: &mut Value) -> Result<(), String> {
        for (path, original, rgb) in self.values {
            if let Some(value) = locate(result, &path)
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
            visit(document, &mut vec![], &mut |value, _| {
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

    #[test]
    fn auxiliary_export_payloads_translate_without_rewriting_labels() {
        let mut request = json!({
            "atom_indicators": [{"text": "#B43237", "style": {"color": "#B43237"}}],
            "graphic_parts": {"71": [{"style": {"stroke": "#117E6C", "fill": "#000000"}}]}
        });
        prepare(&mut request).unwrap();
        assert_eq!(request["atom_indicators"][0]["text"], "#B43237");
        assert_eq!(
            request["atom_indicators"][0]["style"]["color"],
            json!([180, 50, 55])
        );
        assert_eq!(
            request["graphic_parts"]["71"][0]["style"]["stroke"],
            json!([17, 126, 108])
        );
        assert_eq!(
            request["graphic_parts"]["71"][0]["style"]["fill"],
            json!([0, 0, 0])
        );
    }

    #[test]
    fn removed_and_reordered_objects_restore_by_identity_and_preserve_changed_rgb() {
        let mut request = json!({"document": {
            "atoms": [
                {"id": 1, "text_style": {"color": "#B43237"}},
                {"id": 2, "text_style": {"color": "#000000"}},
                {"id": 3, "text_style": {"color": "#B43237"}}
            ],
            "bonds": [{"a": 1, "b": 2, "color": "#117E6C"}, {"a": 2, "b": 3, "color": "#B43237"}],
            "annotations": [{"id": 10, "format": {"style": {"color": "#000000"}}}, {"id": 11, "text": "label"}]
        }});
        let colors = prepare(&mut request).unwrap();
        let mut response = json!({"document": {
            "atoms": [
                {"id": 3, "text_style": {"color": [180, 50, 56]}},
                {"id": 2, "text_style": {"color": [0, 0, 0]}},
                {"id": 4, "text_style": {"color": [0, 0, 0]}}
            ],
            "bonds": [{"a": 3, "b": 2, "color": [180, 50, 55]}, {"a": 1, "b": 2, "color": [17, 126, 108]}],
            "annotations": [{"id": 11, "text": "label"}, {"id": 10, "format": {"style": {"color": [0, 0, 0]}}}]
        }});
        colors.restore(&mut response).unwrap();
        assert_eq!(
            response["document"]["atoms"][0]["text_style"]["color"],
            json!([180, 50, 56])
        );
        assert_eq!(
            response["document"]["atoms"][1]["text_style"]["color"],
            "#000000"
        );
        assert_eq!(
            response["document"]["atoms"][2]["text_style"]["color"],
            json!([0, 0, 0])
        );
        assert_eq!(response["document"]["bonds"][0]["color"], "#B43237");
        assert_eq!(response["document"]["bonds"][1]["color"], "#117E6C");
        assert_eq!(
            response["document"]["annotations"][1]["format"]["style"]["color"],
            "#000000"
        );
    }
}
