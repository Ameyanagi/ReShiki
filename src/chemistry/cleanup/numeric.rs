//! CPython 3.12.12 builtin_sum (Python/bltinmodule.c).
//! Copyright Python Software Foundation; PSF license, see licenses/cpython.
//! Keep the native compensated additions and expression order; no reassociation.
use super::{Error, Result};
use crate::chemistry::{depict::geometry::Coordinates, stereo::Point3};

fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut value, mut correction): (f64, f64) = (0., 0.);
    for next in values {
        let combined = value + next;
        correction += if value.abs() >= next.abs() {
            (value - combined) + next
        } else {
            (next - combined) + value
        };
        value = combined;
    }
    if correction != 0. && correction.is_finite() {
        value += correction;
    }
    value
}

pub(super) fn orient(
    old: &[Point3],
    new: &mut [Point3],
    fixed: &Coordinates,
    keep: bool,
) -> Result<()> {
    if old.len() != new.len() || new.is_empty() {
        return Err(Error::Layout);
    }
    if fixed.len() >= 2 {
        return Ok(());
    }
    let (bx, by, ax, ay) = if let Some((&pivot, _)) = fixed.first_key_value() {
        let before = old.get(pivot).ok_or(Error::Layout)?;
        let after = new.get(pivot).ok_or(Error::Layout)?;
        (before.x, before.y, after.x, after.y)
    } else {
        let n = f64::from(u32::try_from(new.len()).map_err(|_| Error::Limit)?);
        (
            sum(old.iter().map(|p| p.x)) / n,
            sum(old.iter().map(|p| p.y)) / n,
            sum(new.iter().map(|p| p.x)) / n,
            sum(new.iter().map(|p| p.y)) / n,
        )
    };
    let mut angle = 0.;
    if keep {
        let dot = sum(new
            .iter()
            .zip(old)
            .map(|(p, q)| (p.x - ax) * (q.x - bx) + (p.y - ay) * (q.y - by)));
        let cross = sum(new
            .iter()
            .zip(old)
            .map(|(p, q)| (p.x - ax) * (q.y - by) - (p.y - ay) * (q.x - bx)));
        if dot.abs() + cross.abs() > 1e-12 {
            angle = cross.atan2(dot);
        }
    }
    let c = angle.cos();
    let s = angle.sin();
    for point in new {
        let x = point.x - ax;
        let y = point.y - ay;
        *point = Point3 {
            x: bx + c * x - s * y,
            y: by + s * x + c * y,
            z: 0.,
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let python = root.join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        });
        let output = std::process::Command::new(python)
            .arg(root.join("tests/cleanup_numeric_reference.py"))
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
}
