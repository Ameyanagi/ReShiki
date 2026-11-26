//! Admit a previous, verified GFN2 result without guessing legacy provenance.

use super::{bohr, geometry};
use crate::cli::args::Ts;
use serde_json::Value;
use std::fs;

pub(super) fn load(command: &mut Ts) -> Result<Option<Value>, String> {
    let Some(dir) = &command.from else {
        return Ok(None);
    };
    let fallback = "use --refine gxtb --ts TS.xyz with explicit endpoints or bond changes";
    let path = dir.join("report.json");
    let text = match fs::read(&path) {
        Ok(text) => text,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && fs::symlink_metadata(&path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            // Historical --out-dir runs wrote six files without a report.
            // Admit only their geometries, exactly like explicit --ts input.
            // No energy or claimed source method crosses this boundary.
            let reactant = geometry::endpoint(&dir.join("reactant.xyz"))?;
            for name in ["product.xyz", "ts.xyz"] {
                let structure = geometry::endpoint(&dir.join(name))?;
                geometry::check_sequence(&reactant.numbers, &structure.numbers)?;
            }
            command.reactant = Some(dir.join("reactant.xyz"));
            command.product = Some(dir.join("product.xyz"));
            command.ts = Some(dir.join("ts.xyz"));
            return Ok(None);
        }
        Err(error) => {
            return Err(format!(
                "cannot verify the prior GFN2-xTB result in {}: {error}; {fallback}",
                path.display()
            ));
        }
    };
    let report: Value = serde_json::from_slice(&text)
        .map_err(|error| format!("invalid {}: {error}; {fallback}", path.display()))?;
    let text = |path: &str| report.pointer(path).and_then(Value::as_str);
    let flag = |path: &str| report.pointer(path).and_then(Value::as_bool);
    let null = |path: &str| report.pointer(path).is_some_and(Value::is_null);
    let number = |path: &str| {
        report
            .pointer(path)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
    };
    let methods_agree = [
        "energies",
        "barriers",
        "endpoint_relaxation",
        "saddle",
        "frequencies",
        "worker_frequencies",
        "hessian",
        "irc",
        "path",
        "reactive_start",
        "work",
    ]
    .iter()
    .all(|key| text(&format!("/methods/{key}")) == Some("gfn2"))
        && ["energies", "barriers", "structures", "saddle", "irc"]
            .iter()
            .all(|key| text(&format!("/{key}/method")) == Some("gfn2"))
        && ["plus", "minus"]
            .iter()
            .all(|key| text(&format!("/irc/{key}/method")) == Some("gfn2"))
        && text("/structures/position_units") == Some("bohr");
    let verified = report.get("schema_version").and_then(Value::as_u64) == Some(2)
        && methods_agree
        && flag("/verified") == Some(true)
        && text("/settings/method") == Some("gfn2")
        && text("/energies/units") == Some("hartree")
        && text("/barriers/units") == Some("kcal/mol")
        && flag("/barriers/verified") == Some(true)
        && text("/settings/conventions") == Some("axtb")
        && null("/settings/solvent")
        && null("/failure_reason")
        && null("/worker_check_error")
        && flag("/saddle/converged") == Some(true)
        && text("/saddle/reason") == Some("stationary")
        && report.get("imaginary_count").and_then(Value::as_u64) == Some(1)
        && report.get("worker_imaginary_count").and_then(Value::as_u64) == Some(1)
        && flag("/connects") == Some(true)
        && null("/irc_error");
    let connects = matches!(
        (text("/irc/plus/reached"), text("/irc/minus/reached")),
        (Some("Reactant"), Some("Product")) | (Some("Product"), Some("Reactant"))
    );
    let count = |key: &str| {
        report
            .get(key)
            .and_then(Value::as_array)
            .and_then(|frequencies| {
                frequencies.iter().try_fold(0, |count, value| {
                    value
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .map(|value| {
                            count + usize::from(value < -reshiki_optimize::ts::IMAGINARY_CM1)
                        })
                })
            })
            == Some(1)
    };
    if !verified || !connects || !count("frequencies_cm1") || !count("worker_frequencies_cm1") {
        return Err(format!(
            "{} does not prove a verified, gas-phase GFN2-xTB transition state; {fallback}",
            path.display()
        ));
    }
    let numbers: Vec<u8> = report
        .pointer("/structures/numbers")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or_else(|| format!("{} lacks atom provenance; {fallback}", path.display()))?;
    for name in ["reactant", "product", "ts"] {
        let geometry = geometry::endpoint(&dir.join(format!("{name}.xyz")))?;
        let positions: Vec<f64> = report
            .pointer(&format!("/structures/{name}"))
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or_else(|| {
                format!(
                    "{} lacks {name} geometry provenance; {fallback}",
                    path.display()
                )
            })?;
        let xyz = bohr(&geometry);
        if geometry.numbers != numbers
            || positions.len() != xyz.len()
            || positions
                .iter()
                .zip(xyz)
                .any(|(&a, b)| !a.is_finite() || (a - b).abs() > 1e-8)
        {
            return Err(format!(
                "{name}.xyz no longer matches the verified source report; {fallback}"
            ));
        }
        if number(&format!("/energies/{name}")).is_none() {
            return Err(format!(
                "the verified source report has no finite {name} energy"
            ));
        }
    }
    let ts = number("/energies/ts").ok_or("missing TS energy")?;
    for (barrier, endpoint) in [("forward", "reactant"), ("reverse", "product")] {
        let energy = number(&format!("/energies/{endpoint}")).ok_or("missing endpoint energy")?;
        let expected = (ts - energy) * reshiki_optimize::KCAL_PER_HARTREE;
        if !number(&format!("/barriers/{barrier}"))
            .is_some_and(|value| (value - expected).abs() < 1e-8)
        {
            return Err(format!(
                "the source {barrier} barrier does not match its GFN2-xTB endpoint and saddle energies; {fallback}"
            ));
        }
    }
    let charge = report
        .pointer("/settings/charge")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or("the source report has no valid molecular charge")?;
    let uhf = report
        .pointer("/settings/uhf")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or("the source report has no valid unpaired electron count")?;
    if (command.charge_explicit && command.charge != charge)
        || command.uhf.is_some_and(|value| value != uhf)
    {
        return Err("--charge or --uhf disagrees with the verified source report".into());
    }
    command.charge = charge;
    command.uhf = Some(uhf);
    command.reactant = Some(dir.join("reactant.xyz"));
    command.product = Some(dir.join("product.xyz"));
    command.ts = Some(dir.join("ts.xyz"));
    Ok(Some(report))
}
