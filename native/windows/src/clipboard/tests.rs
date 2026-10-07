use super::*;
#[test]
fn explicit_structure_formats_keep_native_bytes_and_unicode_text() {
    for (kind, name, data) in [
        (
            "com.mdli.molfile",
            "chemical/x-mdl-molfile",
            "MOL\nM  END\n",
        ),
        ("org.opensmiles.smiles", "SMILES", "CO"),
        (
            "chemical/x-cdxml",
            "chemical/x-cdxml",
            "<CDXML>日本語</CDXML>",
        ),
    ] {
        let representations = [kind, "public.utf8-plain-text"]
            .into_iter()
            .map(|kind| Representation {
                kind: kind.into(),
                data: STANDARD.encode(data),
            })
            .collect();
        let formats = prepare_formats(representations, false).unwrap();
        assert_eq!(formats.len(), 2);
        assert_eq!(formats[&format(name).unwrap()], data.as_bytes());
        let unicode: Vec<_> = data
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(formats[&UNICODE], unicode);
    }
}

#[test]
fn editable_office_copy_omits_standalone_bitmap_but_copy_image_keeps_it() {
    let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([30, 70, 150, 255]));
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 20, 10);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(image.as_raw())
        .unwrap();
    let reps = || {
        vec![
            Representation {
                kind: "public.png".into(),
                data: STANDARD.encode(&png),
            },
            Representation {
                kind: "dev.reshiki.drawing".into(),
                data: STANDARD.encode(b"{\"version\":15}"),
            },
        ]
    };
    let office = prepare_formats(reps(), true).unwrap();
    assert!(!office.contains_key(&DIB));
    assert_eq!(office[&format("PNG").unwrap()], png);
    assert!(office.contains_key(&format("dev.reshiki.drawing").unwrap()));
    let picture = prepare_formats(reps(), false).unwrap();
    assert_eq!(bitmap(&from_dib(&picture[&DIB]).unwrap()).unwrap(), image);
}
#[test]
fn real_clipboard_unicode_native_priority_and_invalid_write() {
    let _clipboard = CLIPBOARD_TEST_LOCK.lock().unwrap();
    let rep = |kind: &str, bytes: &[u8]| Representation {
        kind: kind.into(),
        data: STANDARD.encode(bytes),
    };
    let text = "日本語 CCO".as_bytes();
    write(
        vec![
            rep("public.utf8-plain-text", text),
            rep("dev.reshiki.drawing", b"{\"version\":15}"),
        ],
        false,
    )
    .unwrap();
    let packet = read_packet(false).unwrap();
    assert_eq!(packet.representations[0].kind, "dev.reshiki.drawing");
    write(vec![rep("public.utf8-plain-text", text)], false).unwrap();
    assert_eq!(
        STANDARD
            .decode(&read_packet(false).unwrap().representations[0].data)
            .unwrap(),
        text
    );
    assert!(
        write(
            vec![Representation {
                kind: "public.png".into(),
                data: "!".into()
            }],
            false
        )
        .is_err()
    );
    assert_eq!(
        STANDARD
            .decode(&read_packet(false).unwrap().representations[0].data)
            .unwrap(),
        text
    );
    write(vec![rep("com.cambridgesoft.cdx", b"VjCD0100\0")], false).unwrap();
    assert_eq!(
        read_packet(false).unwrap().representations[0].kind,
        "com.revvity.chemdraw.cdx-clipboard"
    );
}
#[test]
fn dib_roundtrip_keeps_pixels_and_resolution() {
    let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([30, 70, 150, 255]));
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 20, 10);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: 11811,
        yppu: 11811,
        unit: png::Unit::Meter,
    }));
    encoder
        .write_header()
        .unwrap()
        .write_image_data(image.as_raw())
        .unwrap();
    let result = from_dib(&to_dib(&png).unwrap()).unwrap();
    assert_eq!(bitmap(&result).unwrap(), image);
    assert_eq!(
        png::Decoder::new(Cursor::new(result))
            .read_info()
            .unwrap()
            .info()
            .pixel_dims
            .unwrap()
            .xppu,
        11811
    );
    assert!(from_dib(&[0; 40]).is_err());
    for length in 0..40 {
        assert!(from_dib(&vec![0; length]).is_err());
    }
}
