//! Uncapped differential diagnostics. Categories describe fields, not exemptions.
use serde_json::Value;
use std::collections::BTreeMap;

#[allow(dead_code)] // Shared by targets with different historical numeric comparisons.
pub fn differences(actual: &Value, expected: &Value, path: &str) -> Vec<String> {
    collect(actual, expected, path, false)
}

#[allow(dead_code)] // Preparation and scene historically compare f64 bit patterns.
pub fn numeric_differences(actual: &Value, expected: &Value, path: &str) -> Vec<String> {
    collect(actual, expected, path, true)
}

fn collect(actual: &Value, expected: &Value, path: &str, numeric_bits: bool) -> Vec<String> {
    let mut result = Vec::new();
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in b {
                let path = format!("{path}.{key}");
                if let Some(actual) = a.get(key) {
                    result.extend(collect(actual, value, &path, numeric_bits));
                } else {
                    result.push(format!("{path}: missing, expected={value}"));
                }
            }
            for (key, value) in a {
                if !b.contains_key(key) {
                    result.push(format!("{path}.{key}: unexpected={value}"));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                result.push(format!("{path}.length: {} != {}", a.len(), b.len()));
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                result.extend(collect(a, b, &format!("{path}[{i}]"), numeric_bits));
            }
            for (i, value) in a.iter().enumerate().skip(b.len()) {
                result.push(format!("{path}[{i}]: unexpected={value}"));
            }
            for (i, value) in b.iter().enumerate().skip(a.len()) {
                result.push(format!("{path}[{i}]: missing, expected={value}"));
            }
        }
        (Value::Number(a), Value::Number(b)) if numeric_bits && (a.is_f64() || b.is_f64()) => {
            if a.as_f64().map(f64::to_bits) != b.as_f64().map(f64::to_bits) {
                result.push(format!("{path}: {actual} != {expected}"));
            }
        }
        _ if actual == expected => (),
        _ => result.push(format!("{path}: {actual} != {expected}")),
    }
    result
}

pub fn report(label: &str, failures: &[String]) -> anyhow::Result<()> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for failure in failures.iter().flat_map(|failure| failure.lines()) {
        let category = if failure.contains(".projection:") {
            "projection"
        } else if failure.contains(".directions[") {
            "chemical direction"
        } else if failure.contains(".bond_cfg[") {
            "chemical CFG"
        } else if failure.contains(".a:") || failure.contains(".b:") {
            "endpoint"
        } else if failure.contains(".display:") {
            "paint"
        } else if failure.contains(".position:") || failure.contains(".double_position:") {
            "paint position"
        } else if failure.contains(".explicit_valence:") {
            "explicit valence"
        } else if failure.contains(".implicit_hydrogens:") {
            "implicit hydrogens"
        } else if failure.contains(".label_h:") {
            "atom hydrogen label"
        } else if failure.contains("document rejected:") {
            "document acceptance"
        } else {
            "other (requires classification)"
        };
        *counts.entry(category).or_default() += 1;
    }
    eprintln!(
        "{label}: {} failure records, {} field/outcome differences; category counts {counts:?}",
        failures.len(),
        counts.values().sum::<usize>()
    );
    anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
