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
    orient_with(old, new, fixed, keep, rotation)
}

fn rotation(angle: f64) -> Result<(f64, f64)> {
    // The Windows ARM reference is x64 CPython with the non-FMA UCRT path.
    // Its sine differs from ARM's host CRT at some cleanup angles. Keep the
    // same bounded, audited implementation used by abbreviation placement.
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        crate::chemistry::windows_trigonometry::sin_cos(angle).ok_or(Error::Layout)
    }
    #[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
    {
        let c = angle.cos();
        let s = angle.sin();
        Ok((s, c))
    }
}

fn orient_with(
    old: &[Point3],
    new: &mut [Point3],
    fixed: &Coordinates,
    keep: bool,
    rotation: impl FnOnce(f64) -> Result<(f64, f64)>,
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
    let (s, c) = rotation(angle)?;
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

#[cfg(all(test, feature = "rdkit-reference"))]
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

#[cfg(test)]
mod windows_tests {
    use super::*;
    use anyhow::Context;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Stage {
        old: Vec<Point3>,
        new: Vec<Point3>,
        fixed: Coordinates,
        keep: bool,
        scalars: Scalars,
        expected: Vec<Point3>,
    }
    #[derive(Deserialize)]
    struct Scalars {
        angle: f64,
        sine: f64,
        cosine: f64,
    }
    #[derive(Deserialize)]
    struct Profile {
        fma3: u8,
        stage: Stage,
        analysis: Vec<Point3>,
    }
    #[derive(Deserialize)]
    struct Capture {
        header: serde_json::Value,
        profiles: Vec<Profile>,
    }

    #[test]
    fn windows_emulated_cleanup_preserves_the_original_sine_rounding() -> anyhow::Result<()> {
        let capture: Capture = serde_json::from_value(serde_json::from_str(include_str!(
            "../../../tests/fixtures/cleanup-windows-trigonometry.json"
        ))?)?;
        anyhow::ensure!(capture.header["rdkit"] == crate::chemistry::RDKIT_VERSION);
        let original = capture
            .profiles
            .iter()
            .find(|p| p.fma3 == 0)
            .context("Missing non-FMA reference")?;
        let fused = capture
            .profiles
            .iter()
            .find(|p| p.fma3 == 1)
            .context("Missing FMA reference")?;
        for (a, b) in [
            (&original.stage.old, &fused.stage.old),
            (&original.stage.new, &fused.stage.new),
        ] {
            anyhow::ensure!(a.len() == b.len());
            for (a, b) in a.iter().zip(b) {
                anyhow::ensure!(
                    a.x.to_bits() == b.x.to_bits()
                        && a.y.to_bits() == b.y.to_bits()
                        && a.z.to_bits() == b.z.to_bits(),
                    "CRT profiles changed the source positions"
                );
            }
        }
        let expected_angle = 0xbfe806a29303c80d;
        anyhow::ensure!(
            original.stage.scalars.angle.to_bits() == expected_angle
                && fused.stage.scalars.angle.to_bits() == expected_angle
        );
        anyhow::ensure!(
            original.stage.scalars.sine.to_bits() == 0xbfe5d4d6729eadb0
                && fused.stage.scalars.sine.to_bits() == 0xbfe5d4d6729eadaf
        );
        anyhow::ensure!(
            original.stage.scalars.cosine.to_bits() == fused.stage.scalars.cosine.to_bits()
        );
        // This one native primitive difference exactly reproduces both values
        // from the Windows ARM CI failure, without using Rust as the oracle.
        anyhow::ensure!(
            original
                .analysis
                .get(2)
                .context("Missing native atom")?
                .y
                .to_bits()
                == (-1.9335777144834954_f64).to_bits()
        );
        anyhow::ensure!(
            fused
                .analysis
                .get(2)
                .context("Missing native atom")?
                .y
                .to_bits()
                == (-1.9335777144834956_f64).to_bits()
        );
        let mut actual = original.stage.new.clone();
        let mut observed_angle = None;
        orient_with(
            &original.stage.old,
            &mut actual,
            &original.stage.fixed,
            original.stage.keep,
            |angle| {
                observed_angle = Some(angle.to_bits());
                crate::chemistry::windows_trigonometry::sin_cos(angle).ok_or(Error::Layout)
            },
        )?;
        anyhow::ensure!(observed_angle == Some(expected_angle));
        anyhow::ensure!(actual.len() == original.stage.expected.len());
        for (index, (a, e)) in actual.iter().zip(&original.stage.expected).enumerate() {
            anyhow::ensure!(
                a.x.to_bits() == e.x.to_bits()
                    && a.y.to_bits() == e.y.to_bits()
                    && a.z.to_bits() == e.z.to_bits(),
                "Native Windows cleanup atom {index}: {a:?} != {e:?}"
            );
        }
        Ok(())
    }
}
