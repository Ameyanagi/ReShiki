//! Complete field diagnostics for independent reference responses.
use serde_json::Value;

/// Report every changed field, including every missing or extra array entry.
/// Long output strings are described by size; presentation comparators provide
/// their detailed drawing diagnostics separately.
pub fn response_differences(actual: &Value, expected: &Value) -> Vec<String> {
    fn visit(actual: &Value, expected: &Value, path: &str, failures: &mut Vec<String>) {
        match (actual, expected) {
            (Value::Object(a), Value::Object(e)) => {
                let names = a
                    .keys()
                    .chain(e.keys())
                    .collect::<std::collections::BTreeSet<_>>();
                for name in names {
                    let next = format!("{path}/{name}");
                    match (a.get(name), e.get(name)) {
                        (Some(a), Some(e)) => visit(a, e, &next, failures),
                        (a, e) => failures.push(format!(
                            "[response-field] {next}: actual={a:?}, expected={e:?}"
                        )),
                    }
                }
            }
            (Value::Array(a), Value::Array(e)) => {
                if a.len() != e.len() {
                    failures.push(format!(
                        "[response-length] {path}: actual={}, expected={}",
                        a.len(),
                        e.len()
                    ));
                }
                for index in 0..a.len().max(e.len()) {
                    let next = format!("{path}/{index}");
                    match (a.get(index), e.get(index)) {
                        (Some(a), Some(e)) => visit(a, e, &next, failures),
                        (a, e) => failures.push(format!(
                            "[response-entry] {next}: actual={a:?}, expected={e:?}"
                        )),
                    }
                }
            }
            _ if actual != expected => {
                let describe = |value: &Value| match value {
                    Value::String(text) if path == "/output" && text.len() > 160 => {
                        format!("string ({} bytes)", text.len())
                    }
                    _ => value.to_string(),
                };
                failures.push(format!(
                    "[response-field] {path}: actual={}, expected={}",
                    describe(actual),
                    describe(expected)
                ));
            }
            _ => {}
        }
    }
    let mut failures = Vec::new();
    visit(actual, expected, "", &mut failures);
    failures
}
