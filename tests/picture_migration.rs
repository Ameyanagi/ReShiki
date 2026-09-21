use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::pictures::exchange::{self, Budget};
use serde::Deserialize;
use std::{error::Error, path::Path, process::Command};
type TestResult = Result<(), Box<dyn Error>>;

#[derive(Deserialize)]
struct Case {
    name: String,
    format: String,
    opacity: f64,
    data: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[test]
fn pixel_normalization_matches_pillow() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let output = Command::new(python)
        .arg(root.join("tests/picture_reference.py"))
        .env("PYTHONUTF8", "1")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout)?;
    assert!(cases.len() >= 130);
    let mut failures = Vec::new();
    for case in cases {
        let source = STANDARD.decode(&case.data)?;
        for length in [0, 4, 8, source.len() / 2] {
            if let Some(truncated) = source.get(..length) {
                let _ = exchange::import(
                    truncated,
                    &case.format,
                    case.opacity,
                    &mut Budget::default(),
                );
            }
        }
        let picture =
            match exchange::import(&source, &case.format, case.opacity, &mut Budget::default()) {
                Ok(picture) => picture,
                Err(error) => {
                    failures.push(format!("{}: {error}", case.name));
                    continue;
                }
            };
        assert_eq!(
            (picture.width(), picture.height()),
            (case.width, case.height),
            "{}",
            case.name
        );
        let pixels = image::load_from_memory(picture.png())?.to_rgba8();
        let differences: Vec<_> = pixels
            .as_raw()
            .iter()
            .zip(&case.rgba)
            .enumerate()
            // JPEG IDCT/chroma interpolation varies by decoder; alpha and all
            // lossless formats must still match byte for byte.
            .filter_map(|(i, (a, b))| {
                let tolerance = if case.format == "JPEG" && i % 4 != 3 {
                    2
                } else {
                    0
                };
                (a.abs_diff(*b) > tolerance).then_some((i, *a, *b))
            })
            .collect();
        if !differences.is_empty() {
            failures.push(format!(
                "{}: {} changed channels; first {:?}; max {}",
                case.name,
                differences.len(),
                differences.first(),
                differences
                    .iter()
                    .map(|(_, a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

fn raster() -> Result<Vec<u8>, Box<dyn Error>> {
    let pixels = image::RgbaImage::from_fn(7, 5, |x, y| {
        image::Rgba([(x * 30) as u8, (y * 40) as u8, 200, (x * 13 + y * 30) as u8])
    });
    let mut output = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut output, image::ImageFormat::Png)?;
    Ok(output.into_inner())
}

#[test]
fn rejects_invalid_images_and_enforces_budgets_before_committing() -> TestResult {
    let data = raster()?;
    let mut budget = Budget::default();
    for (bytes, format, opacity) in [
        (b"".as_slice(), "PNG", 1.),
        (b"not pixels".as_slice(), "PNG", 1.),
        (data.as_slice(), "TIFF", 1.),
        (data.as_slice(), "JPEG", 1.),
        (data.as_slice(), "PNG", f64::NAN),
        (data.as_slice(), "PNG", 1.1),
        (data.as_slice(), "PNG", -0.1),
        (data.as_slice(), "WEBP", 1.),
    ] {
        assert!(exchange::import(bytes, format, opacity, &mut budget).is_err());
        assert_eq!((budget.bytes, budget.pixels), (0, 0));
    }
    assert!(
        exchange::import(
            &vec![0; reshiki::pictures::MAX_BYTES + 1],
            "PNG",
            1.,
            &mut budget
        )
        .is_err()
    );
    let picture = exchange::import(&data, "PNG", 1., &mut budget)?;
    assert_eq!(budget.pixels, 35);
    let mut limit = Budget {
        pixels: 64_000_000 - 35,
        bytes: 0,
    };
    exchange::import(&data, "PNG", 1., &mut limit)?;
    let saved = (limit.bytes, limit.pixels);
    assert!(exchange::import(&data, "PNG", 1., &mut limit).is_err());
    assert_eq!((limit.bytes, limit.pixels), saved);
    let mut limit = Budget {
        pixels: 0,
        bytes: 64 * 1024 * 1024,
    };
    assert!(exchange::import(&data, "PNG", 1., &mut limit).is_err());
    assert_eq!(limit.pixels, 0);
    let flipped = exchange::export(&picture, true)?;
    let expected = image::load_from_memory(picture.png())?.flipv().to_rgba8();
    assert_eq!(image::load_from_memory(&flipped)?.to_rgba8(), expected);
    Ok(())
}

#[tokio::test]
async fn malformed_picture_cannot_return_a_partial_drawing() -> TestResult {
    use reshiki::engine::{LocalEngine, Request};
    let engine = LocalEngine::default();
    let data = raster()?;
    let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
    let xml = |attributes: &str| {
        format!(
            "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\"/></fragment><embeddedobject id=\"4\" BoundingBox=\"10 20 82 56\" {attributes}/></page></CDXML>"
        )
    };
    for attrs in [
        "PNG=\"00\"".into(),
        format!("TIFF=\"{hex}\""),
        format!("PNG=\"{hex}\" alpha=\"NaN\""),
    ] {
        assert!(
            engine
                .request(Request::import("cdxml", &xml(&attrs)))
                .await
                .is_err()
        );
    }
    let actual = engine
        .request(Request::import(
            "cdxml",
            &xml(&format!(
                "PNG=\"{hex}\" alpha=\"0.5\" RotationAngle=\"2424832\""
            )),
        ))
        .await?;
    let doc = actual
        .document
        .ok_or("Missing drawing after rejected images")?;
    assert_eq!(doc.atoms.len(), 1);
    assert_eq!(doc.graphics.len(), 1);
    doc.validate()?;
    let picture = doc
        .graphics
        .first()
        .and_then(|g| g.picture.as_ref())
        .ok_or("Missing normalized picture")?;
    let expected = exchange::import(&data, "PNG", 0.5, &mut Budget::default())?;
    assert_eq!(picture, &expected);
    Ok(())
}
