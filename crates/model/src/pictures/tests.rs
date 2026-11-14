use super::*;

fn raster(width: u32, height: u32) -> Picture {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::new_rgba8(width, height)
        .write_to(&mut png, ImageFormat::Png)
        .unwrap();
    Picture::import(png.get_ref()).unwrap()
}

fn emf() -> Vec<u8> {
    let mut bytes = vec![0; 108];
    for (offset, value) in [
        (0, 1),
        (4, 88),
        (24, 100),
        (28, 200),
        (32, 2640),
        (36, 1470),
        (40, 0x464d4520),
        (44, 0x10000),
        (48, 108),
        (52, 2),
        (56, 1),
        (88, 14),
        (92, 20),
        (104, 20),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
    }
    bytes
}

#[test]
fn fit_within_preserves_preview_pixel_aspect_ratio() {
    let picture = raster(4, 2);
    assert_eq!(picture.fit_within(10., 10.), (10., 5.));
    assert_eq!(picture.fit_within(3., 10.), (3., 1.5));
    assert_eq!(picture.fit_within(10., 3.), (6., 3.));
}

#[test]
fn intrinsic_emf_size_uses_the_validated_frame_but_fit_uses_preview_pixels() {
    let source = emf();
    let preview = raster(4, 3);
    let picture = Picture::from_emf(&source, preview.png()).unwrap();
    let dimensions = emf_dimensions(&source).unwrap();
    assert_eq!(picture.0.emf.as_ref().unwrap().1, dimensions);
    let expected = (
        crate::style::DEFAULT.world(72.),
        crate::style::DEFAULT.world(36.),
    );
    assert_eq!(picture.intrinsic_size(), expected);
    let graphic = picture.graphic(1, Point::default());
    assert_eq!(graphic.axis_x, Point::new(expected.0, 0.));
    assert_eq!(graphic.axis_y, Point::new(0., expected.1));
    assert_eq!(picture.fit_within(12., 12.), (12., 9.));
}

#[test]
fn retained_dimensions_do_not_change_byte_equality_or_serialization() {
    let source = emf();
    let preview = raster(4, 3);
    let original = Picture::from_emf(&source, preview.png()).unwrap();
    let mut equivalent = Picture::from_emf(&source, preview.png()).unwrap();
    let (_, dimensions) = Arc::get_mut(&mut equivalent.0)
        .unwrap()
        .emf
        .as_mut()
        .unwrap();
    *dimensions = reshiki_metafile::Dimensions {
        width_pt: 99.,
        height_pt: 17.,
    };
    assert_eq!(original, equivalent);
    assert_eq!(equivalent.emf(), Some(source.as_slice()));
    assert_eq!(
        equivalent.stored_bytes(),
        equivalent.png().len() + source.len()
    );
    let expected = format!(
        "{{\"png\":\"{}\",\"emf\":\"{}\"}}",
        STANDARD.encode(original.png()),
        STANDARD.encode(&source)
    );
    assert_eq!(serde_json::to_string(&original).unwrap(), expected);
    assert_eq!(serde_json::to_string(&equivalent).unwrap(), expected);
    let reopened: Picture = serde_json::from_str(&expected).unwrap();
    assert_eq!(reopened, original);
    assert_eq!(reopened.intrinsic_size(), original.intrinsic_size());
    assert_eq!(
        serde_json::to_string(&preview).unwrap(),
        serde_json::to_string(&STANDARD.encode(preview.png())).unwrap()
    );
}

#[test]
fn base64_limits_and_metafile_error_order_are_preserved() {
    let oversized = "!".repeat(MAX_BYTES.div_ceil(3) * 4 + 1);
    assert_eq!(
        decode_base64_limited(&oversized).unwrap_err(),
        "Picture exceeds 16 MB"
    );
    let invalid = "!";
    let expected = STANDARD.decode(invalid).unwrap_err().to_string();
    assert_eq!(decode_base64_limited(invalid).unwrap_err(), expected);
    let stored = serde_json::json!({ "png": invalid, "emf": oversized });
    assert_eq!(
        serde_json::from_value::<Picture>(stored)
            .unwrap_err()
            .to_string(),
        expected
    );
    assert_eq!(
        Picture::from_emf(&[], &vec![0; MAX_BYTES + 1]).unwrap_err(),
        "EMF must be a complete file no larger than 16 MB"
    );
}

#[test]
fn bounded_file_reads_keep_the_limit_and_caller_error() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"picture bytes").unwrap();
    assert_eq!(
        read_bounded(file.path(), "caller limit").unwrap(),
        b"picture bytes".to_vec()
    );
    file.as_file().set_len(MAX_BYTES as u64).unwrap();
    assert_eq!(
        read_bounded(file.path(), "caller limit").unwrap().len(),
        MAX_BYTES
    );
    file.as_file().set_len(MAX_BYTES as u64 + 1).unwrap();
    assert_eq!(
        read_bounded(file.path(), "caller limit").unwrap_err(),
        "caller limit"
    );
    assert_eq!(
        Picture::open(file.path()).unwrap_err(),
        "Choose a picture file no larger than 16 MB"
    );
    let missing = file.path().with_extension("missing");
    let expected = std::fs::File::open(&missing).unwrap_err().to_string();
    assert_eq!(
        read_bounded(&missing, "caller limit").unwrap_err(),
        expected
    );
}

/// A drawing with `count` pictures that are only a PNG header claiming
/// 4000 × 4000 pixels: decoding any of them fails.
fn drawing_of_headers(count: u64) -> Vec<u8> {
    let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    header.extend([4000_u32.to_be_bytes(), 4000_u32.to_be_bytes()].concat());
    let mut doc = Document::default();
    for id in 1..=count {
        doc.graphics
            .push(raster(1, 1).graphic(id, Point::default()));
    }
    let mut json = serde_json::to_value(&doc).unwrap();
    for graphic in json["graphics"].as_array_mut().unwrap() {
        graphic["picture"] = STANDARD.encode(&header).into();
    }
    serde_json::to_vec(&json).unwrap()
}

#[test]
fn stored_pictures_over_the_pixel_budget_are_refused_before_any_is_decoded() {
    // Five headers claim 80 million pixels. The refusal names the budget, not
    // the first picture's missing pixel data, so nothing was decoded.
    assert_eq!(
        Document::from_json(&drawing_of_headers(5)).unwrap_err(),
        "Drawing pictures exceed 64 MB or 64 million pixels"
    );
    // Within the budget each picture is decoded as before.
    assert_ne!(
        Document::from_json(&drawing_of_headers(4)).unwrap_err(),
        "Drawing pictures exceed 64 MB or 64 million pixels"
    );
    // A picture over its own limits keeps the error decoding gives it.
    let mut oversized =
        serde_json::from_slice::<serde_json::Value>(&drawing_of_headers(1)).unwrap();
    let header = STANDARD.decode(oversized["graphics"][0]["picture"].as_str().unwrap());
    let mut header = header.unwrap();
    header[16..24].copy_from_slice(&[30_000_u32.to_be_bytes(), 30_000_u32.to_be_bytes()].concat());
    oversized["graphics"][0]["picture"] = STANDARD.encode(&header).into();
    assert_ne!(
        Document::from_json(&serde_json::to_vec(&oversized).unwrap()).unwrap_err(),
        "Drawing pictures exceed 64 MB or 64 million pixels"
    );
    let mut doc = Document::default();
    doc.graphics.push(raster(4, 2).graphic(1, Point::default()));
    let reopened = Document::from_json(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(reopened.graphics, doc.graphics);
}
