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
        "../../../../../../tests/fixtures/cleanup-windows-trigonometry.json"
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
