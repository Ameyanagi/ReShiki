//! Select expectations from the original x64 reference process, never Rust math.
use anyhow::Context;
use std::{path::Path, process::Command};

pub fn fixture(stage: &str) -> anyhow::Result<Option<String>> {
    if !cfg!(windows) {
        return Ok(None);
    }
    // A fresh observer supplies exact expectations from the installed wheel's
    // runtime. Recorded fixtures remain cross-runtime audits in this mode.
    let oracle = match stage {
        "geometry" => "RESHIKI_DEPICT_ORACLE".to_owned(),
        "expansion" => "DEPICT_EXPANSION_ORACLE".to_owned(),
        _ => format!("RESHIKI_DEPICT_{}_ORACLE", stage.to_uppercase()),
    };
    if std::env::var_os(oracle).is_some() {
        return Ok(None);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(root.join(".venv/Scripts/python.exe"));
    command.arg(root.join("tests/depict_windows_profile.py"));
    let required = std::env::var("RESHIKI_TEST_WINDOWS_MATH_PROFILE").ok();
    if let Some(profile) = required.as_deref() {
        command.arg("--fma3").arg(match profile {
            "fma3" => "1",
            "no-fma3" => "0",
            _ => anyhow::bail!("Unknown requested Windows CRT profile: {profile}"),
        });
    }
    let output = command.output().context("Original Windows CRT probe")?;
    anyhow::ensure!(
        output.status.success(),
        "Original Windows CRT probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    anyhow::ensure!(result["reference_version"] == "2026.03.6");
    anyhow::ensure!(result["python_platform"] == "win-amd64");
    let profile = result["profile"].as_str().context("Missing CRT profile")?;
    anyhow::ensure!(matches!(profile, "fma3" | "no-fma3" | "server2022"));
    if let Some(required) = required {
        anyhow::ensure!(
            profile == required,
            "Reference CRT profile {profile} != {required}"
        );
    }
    eprintln!("Original x64 reference CRT profile: {profile}");
    // Independent native captures show identical rows in these two stages.
    let invariant = matches!(stage, "seeds" | "finalize");
    let suffix = match profile {
        "no-fma3" if !invariant => "-no-fma3",
        "server2022" if matches!(stage, "geometry" | "rings" | "templates" | "expansion") => {
            "-server2022"
        }
        _ => "",
    };
    Ok(Some(format!(
        "depict-{stage}-windows{suffix}-native.json.gz"
    )))
}
