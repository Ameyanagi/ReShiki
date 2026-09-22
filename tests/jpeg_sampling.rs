use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::pictures::{
    self,
    exchange::{self, Budget},
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    data: String,
    width: u32,
    height: u32,
    rgba: String,
    opacity: f64,
}
#[test]
fn jpeg_chroma_edges_match_independent_libjpeg() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/jpeg_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing JPEG oracle")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing JPEG header")??)?;
    eprintln!("Independent JPEG reference: {header}");
    let (mut cases, mut maximum) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let bytes = STANDARD.decode(case.data)?;
        let expected = STANDARD.decode(case.rgba)?;
        let decode = |fast| -> anyhow::Result<Vec<u8>> {
            let options = zune_core::options::DecoderOptions::new_safe()
                .set_use_unsafe(fast)
                .set_strict_mode(false)
                .set_max_width(8192)
                .set_max_height(8192)
                .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA);
            zune_jpeg::JpegDecoder::new_with_options(
                zune_core::bytestream::ZCursor::new(&bytes),
                options,
            )
            .decode()
            .map_err(|e| anyhow::anyhow!("{e}"))
        };
        assert_eq!(
            decode(false)?,
            decode(true)?,
            "{} scalar/SIMD dispatch",
            case.name
        );
        let picture = exchange::import(&bytes, "JPEG", case.opacity, &mut Budget::default())
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            (picture.width(), picture.height()),
            (case.width, case.height),
            "{}",
            case.name
        );
        let actual = image::load_from_memory(picture.png())?.to_rgba8();
        assert_eq!(actual.as_raw().len(), expected.len());
        let mut changed = Vec::new();
        for (i, (a, b)) in actual.as_raw().iter().zip(&expected).enumerate() {
            maximum = maximum.max(a.abs_diff(*b));
            // Existing JPEG allowance remains 2 color levels; alpha is exact.
            if a.abs_diff(*b) > if i % 4 == 3 { 0 } else { 2 } {
                changed.push((i, *a, *b));
            }
        }
        if !changed.is_empty() {
            failures.push(format!(
                "{}: {} channels; first {:?}; maximum {}",
                case.name,
                changed.len(),
                changed.first(),
                changed
                    .iter()
                    .map(|(_, a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0)
            ));
        }
        if case.opacity == 1. {
            let ordinary = pictures::Picture::import(&bytes).map_err(anyhow::Error::msg)?;
            assert_eq!(ordinary, picture, "{} ordinary picture path", case.name);
        }
        cases += 1;
    }
    assert!(child.wait()?.success());
    assert!(cases > 1200);
    assert!(
        failures.is_empty(),
        "{} failed of {cases}; max difference {maximum}:\n{}",
        failures.len(),
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!("{cases} independent JPEG cases; maximum channel difference {maximum}");
    Ok(())
}

#[test]
fn jpeg_resource_limits_and_truncations_remain_atomic() -> anyhow::Result<()> {
    let bytes = include_bytes!("fixtures/jpeg-subsampled-3x2.jpg");
    for boundary in [0, 2, 7, 32, 128, bytes.len() / 2, bytes.len() - 10] {
        let source = bytes.get(..boundary).context("Truncated source")?;
        let mut budget = Budget::default();
        let result = exchange::import(source, "JPEG", 1., &mut budget);
        if result.is_err() {
            assert_eq!((budget.bytes, budget.pixels), (0, 0));
        }
    }
    for mut budget in [
        Budget {
            pixels: 64_000_000 - 5,
            bytes: 0,
        },
        Budget {
            pixels: 0,
            bytes: 64 * 1024 * 1024,
        },
    ] {
        let before = (budget.bytes, budget.pixels);
        assert!(exchange::import(bytes, "JPEG", 1., &mut budget).is_err());
        assert_eq!((budget.bytes, budget.pixels), before);
    }
    let frame = bytes
        .windows(2)
        .position(|v| v == [0xff, 0xc0])
        .context("JPEG SOF")?;
    for (width, height) in [(8193u16, 2u16), (5000, 5000), (u16::MAX, u16::MAX)] {
        let mut source = bytes.to_vec();
        source
            .get_mut(frame + 5..frame + 7)
            .context("Height")?
            .copy_from_slice(&height.to_be_bytes());
        source
            .get_mut(frame + 7..frame + 9)
            .context("Width")?
            .copy_from_slice(&width.to_be_bytes());
        let mut budget = Budget::default();
        assert!(exchange::import(&source, "JPEG", 1., &mut budget).is_err());
        assert_eq!((budget.bytes, budget.pixels), (0, 0));
    }
    Ok(())
}
