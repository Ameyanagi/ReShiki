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
mod tests;
