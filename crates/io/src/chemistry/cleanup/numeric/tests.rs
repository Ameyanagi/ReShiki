use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
struct Orientation {
    old: Vec<Point3>,
    new: Vec<Point3>,
    fixed: Coordinates,
    keep: bool,
    expected: Vec<Point3>,
}
#[derive(Deserialize)]
struct Sum {
    values: Vec<f64>,
    expected: f64,
}
#[derive(Deserialize)]
struct Hypot {
    x: f64,
    y: f64,
    expected: f64,
}
#[derive(Deserialize)]
struct Cases {
    orientations: Vec<Orientation>,
    sums: Vec<Sum>,
    hypots: Vec<Hypot>,
}

#[test]
fn original_python_orientation_and_numeric_primitives_match_exactly() -> anyhow::Result<()> {
    let root = &crate::repository_root();
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let output = std::process::Command::new(python)
        .arg(root.join("reference/cleanup_numeric_reference.py"))
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Cases = serde_json::from_value(serde_json::from_slice(&output.stdout)?)?;
    for (index, mut case) in cases.orientations.into_iter().enumerate() {
        orient(&case.old, &mut case.new, &case.fixed, case.keep)?;
        anyhow::ensure!(case.new.len() == case.expected.len());
        for (atom, (actual, expected)) in case.new.iter().zip(case.expected).enumerate() {
            for (a, e) in [
                (actual.x, expected.x),
                (actual.y, expected.y),
                (actual.z, expected.z),
            ] {
                anyhow::ensure!(
                    a.to_bits() == e.to_bits(),
                    "Orientation {index}/{atom}: {a:?} != {e:?}"
                );
            }
        }
    }
    for case in cases.sums {
        anyhow::ensure!(
            sum(case.values.into_iter()).to_bits() == case.expected.to_bits(),
            "Native sum changed"
        );
    }
    for (index, case) in cases.hypots.into_iter().enumerate() {
        anyhow::ensure!(
            crate::chemistry::cdxml::native_hypot(case.x, case.y).to_bits()
                == case.expected.to_bits(),
            "Hypot {index} changed"
        );
    }
    Ok(())
}
