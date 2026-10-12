//! Deterministic agent documentation from the live CLI and operation contracts.
use super::{MCP_USAGE, commands::COMMANDS, help};
use reshiki_agent::{
    access::{AccessError, extensions::FILE_FORMATS},
    envelope::Versions,
    ops::{
        budget::Budgets,
        catalog::{RESULT_CONVENTIONS, RESULTS, catalog_json},
        documents::budgets_json,
        error::ErrorKind,
    },
};
use serde_json::Value;

/// The agent reference, with no application version or host-specific values.
#[doc(hidden)]
pub fn agent_reference() -> String {
    let mut text = String::from(
        "# Agent reference\n\n\
<!-- Generated: cargo run --example agent_reference -- docs/agent-reference.md -->\n\n\
Generated from ReShiki's command help, tool catalog and schemas.\n\n\
## Command line\n\n",
    );
    command_sections(&mut text, "###");
    text.push_str("## MCP\n\n");
    fence(&mut text, "text", &MCP_USAGE);
    text.push_str(&format!("## Tools\n\n{RESULT_CONVENTIONS}\n\n"));
    let catalog = catalog_json();
    for tool in catalog.as_array().into_iter().flatten() {
        tool_section(&mut text, tool, "###");
    }
    budget_section(&mut text);
    error_sections(&mut text);
    file_formats_section(&mut text);
    // Empty app input: only independently versioned contracts are rendered.
    let versions = Versions::current("");
    text.push_str("\n## Versions\n\n| Contract | Version |\n| --- | --- |\n");
    for (name, version) in [
        ("operation_api", versions.operation_api),
        ("engine_protocol", versions.engine_protocol),
        ("document", versions.document),
    ] {
        row(&mut text, &[code(name), version.to_string()]);
    }
    text.push_str("\n## Proposal\n\n<details>\n<summary>Proposal JSON Schema</summary>\n\n");
    fence(&mut text, "json", &format!("{:#}", reshiki_agent::schema()));
    text.push_str("</details>\n");
    text
}

/// The overview and command help, with headings local to each reference.
pub(in crate::cli) fn command_sections(text: &mut String, heading: &str) {
    text.push_str(&format!("{heading} Overview\n\n"));
    let overview = help(None);
    fence(text, "text", &overview);
    for spec in COMMANDS {
        text.push_str(&format!("{heading} reshiki --cli {}\n\n", spec.name));
        let body = help(Some(spec));
        if body == overview {
            text.push_str("Prints the [overview](#overview).\n\n");
        } else {
            fence(text, "text", &body);
        }
    }
}

/// All default budget keys, in the order serialized by `info`.
pub(in crate::cli) fn budget_section(text: &mut String) {
    text.push_str(
        "## Budgets\n\nDefault operation budgets, as reported by `info`.\n\n\
| Budget | Value | Unit |\n| --- | --- | --- |\n",
    );
    budget_rows(text, "", &budgets_json(&Budgets::default()));
}

/// The operation codes and the reasons prefixed to file-error messages.
pub(in crate::cli) fn error_sections(text: &mut String) {
    text.push_str("\n## Error codes\n\n| Code | Meaning |\n| --- | --- |\n");
    for kind in ErrorKind::ALL {
        row(text, &[code(kind.code()), cell(kind.meaning())]);
    }
    text.push_str(
        "\n## File-error reasons\n\n\
A file error's message starts with one of these reasons, followed by a colon.\n\n\
| Reason | Error code | Meaning |\n| --- | --- | --- |\n",
    );
    for reason in AccessError::REASONS {
        row(
            text,
            &[code(reason.code), code(reason.error), cell(reason.meaning)],
        );
    }
}

/// The extensions accepted by MCP file tools, including unsupported sides.
pub(in crate::cli) fn file_formats_section(text: &mut String) {
    text.push_str(
        "\n## File formats\n\n\
Extensions accepted by `file_open` and `file_save`; a dash means unsupported.\n\n\
| Extension | Import format | Export format |\n| --- | --- | --- |\n",
    );
    for format in FILE_FORMATS {
        row(
            text,
            &[
                code(&format!(".{}", format.ext)),
                format.import.map_or_else(|| "—".into(), code),
                format.export.map_or_else(|| "—".into(), code),
            ],
        );
    }
}

/// One tool under a `heading` (`##`, `###`): title, description, hints,
/// parameters and result shape.
pub(in crate::cli) fn tool_section(text: &mut String, tool: &Value, heading: &str) {
    let name = string(tool, "name");
    text.push_str(&format!(
        "{heading} {name}\n\n**{}**\n\n{}\n\n",
        string(tool, "title"),
        string(tool, "description"),
    ));
    text.push_str("| Hint | Value |\n| --- | --- |\n");
    for (name, value) in tool
        .get("annotations")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        row(text, &[code(name), cell(&value.to_string())]);
    }
    text.push('\n');
    if let Some(schema) = tool.get("inputSchema") {
        parameters(text, schema);
    }
    if let Some((_, shape)) = RESULTS.iter().find(|(tool, _)| *tool == name) {
        // RESULTS entries start with their top-level `{keys}`.
        let (keys, prose) = shape.split_once("} ").unwrap_or((shape, ""));
        text.push_str(&format!("Result: `{keys}}}`. {prose}\n\n"));
    }
}

fn parameters(text: &mut String, schema: &Value) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    if properties.is_empty() {
        text.push_str("Parameters: none; pass `{}`.\n\n");
        return;
    }
    let described = properties
        .values()
        .any(|property| !string(property, "description").is_empty());
    text.push_str("| Parameter | Type | Required | Nullable | Constraints |");
    text.push_str(if described {
        " Description |\n| --- | --- | --- | --- | --- | --- |\n"
    } else {
        "\n| --- | --- | --- | --- | --- |\n"
    });
    for (name, property) in properties {
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|names| names.iter().any(|value| value.as_str() == Some(name)));
        let nullable = branches(property)
            .iter()
            .any(|branch| string(branch, "type") == "null");
        let mut kind = code(&schema_type(property));
        if name == "proposal" {
            kind.push_str(" ([Proposal](#proposal))");
        }
        let mut cells = vec![
            code(name),
            kind,
            required.to_string(),
            nullable.to_string(),
            constraints(property, "").join("; "),
        ];
        if described {
            cells.push(cell(string(property, "description")));
        }
        row(text, &cells);
    }
    text.push('\n');
}

/// Preserve the schema's union order, including its null alternative.
fn branches(schema: &Value) -> Vec<&Value> {
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .map_or_else(|| vec![schema], |values| values.iter().collect())
}

fn schema_type(schema: &Value) -> String {
    branches(schema)
        .into_iter()
        .map(|branch| {
            let kind = string(branch, "type");
            match branch.get("items") {
                Some(items) if kind == "array" => format!("array<{}>", schema_type(items)),
                _ => kind.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

fn constraints(schema: &Value, prefix: &str) -> Vec<String> {
    let mut fields = Vec::new();
    for branch in branches(schema) {
        for key in [
            "enum",
            "minimum",
            "maximum",
            "maxLength",
            "minItems",
            "maxItems",
            "pattern",
        ] {
            if let Some(value) = branch.get(key) {
                fields.push(format!("{prefix}{key}: {}", code(&value.to_string())));
            }
        }
        if let Some(items) = branch.get("items") {
            fields.extend(constraints(items, &format!("{prefix}items.")));
        }
    }
    fields
}

fn budget_rows(text: &mut String, prefix: &str, budgets: &Value) {
    for (key, value) in budgets.as_object().into_iter().flatten() {
        let name = format!("{prefix}{key}");
        if value.is_object() {
            budget_rows(text, &format!("{name}."), value);
        } else {
            let unit = if key.ends_with("_bytes") || key == "max_cdx_base64" {
                "bytes"
            } else if key.ends_with("_seconds") {
                "seconds"
            } else if key.ends_with("_pixels") || prefix == "render." {
                "pixels"
            } else {
                "count"
            };
            row(text, &[code(&name), value.to_string(), unit.into()]);
        }
    }
}

pub(in crate::cli) fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(in crate::cli) fn row(text: &mut String, cells: &[String]) {
    text.push_str(&format!("| {} |\n", cells.join(" | ")));
}

pub(in crate::cli) fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\r', '\n'], " ")
}

pub(in crate::cli) fn code(text: &str) -> String {
    format!("`{}`", cell(text))
}

/// Keep the source bytes inside the fence, including its final newline.
fn fence(text: &mut String, language: &str, body: &str) {
    text.push_str(&format!("```{language}\n{body}"));
    if !body.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("```\n\n");
}

#[cfg(test)]
mod tests;
