use super::sin_cos;
use anyhow::Context;

#[test]
fn bounded_rotations_match_independent_windows_crt() -> anyhow::Result<()> {
    let data = include_bytes!("../../../../tests/fixtures/abbreviation-trigonometry-windows.bin");
    let header: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/abbreviation-trigonometry-windows.json"
    ))?;
    anyhow::ensure!(header["fma3"] == false, "Wrong native math algorithm");
    anyhow::ensure!(data.len().is_multiple_of(24), "Truncated native corpus");
    let mut count = 0_u64;
    let mut differences = Vec::new();
    for record in data.as_chunks::<24>().0.iter() {
        let mut values = record.as_chunks::<8>().0.iter();
        let mut next = || -> anyhow::Result<u64> {
            Ok(u64::from_be_bytes(
                *values.next().context("Missing native value")?,
            ))
        };
        let angle = f64::from_bits(next()?);
        let sine = next()?;
        let cosine = next()?;
        let (actual_sine, actual_cosine) = sin_cos(angle).context("Valid angle rejected")?;
        if (actual_sine.to_bits(), actual_cosine.to_bits()) != (sine, cosine)
            && differences.len() < 20
        {
            differences.push(format!(
                "angle={angle:.17e}/{:016x} sin={:016x}/{sine:016x} cos={:016x}/{cosine:016x}",
                angle.to_bits(),
                actual_sine.to_bits(),
                actual_cosine.to_bits()
            ));
        }
        count += 1;
    }
    anyhow::ensure!(differences.is_empty(), "{}", differences.join("\n"));
    anyhow::ensure!(
        count > 40_000 && header["cases"] == count,
        "Incomplete native corpus"
    );
    eprintln!("Verified {count} exact native non-FMA rotation pairs");
    Ok(())
}

#[test]
fn rejects_values_outside_the_attachment_domain() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 7.0, -7.0] {
        assert!(sin_cos(value).is_none());
    }
}
