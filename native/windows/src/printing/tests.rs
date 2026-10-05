use super::*;

// Each case owns GDI+ startup/shutdown and image handles. The Windows
// runtime tears down shared state on shutdown, so these lifetimes must
// not overlap across independent test threads.
static METAFILE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn office_metafile_keeps_vectors_transparency_and_physical_size() {
    let _guard = METAFILE_TEST.lock().unwrap();
    assert_metafile_background(false);
}

#[test]
fn file_metafile_keeps_vector_background_and_physical_size() {
    let _guard = METAFILE_TEST.lock().unwrap();
    assert_metafile_background(true);
}

#[test]
fn file_metafile_intrinsic_size_matches_physical_points() {
    let _guard = METAFILE_TEST.lock().unwrap();
    for (width, height) in [(75.3, 39.7), (0.75, 0.75), (720., 1080.)] {
        let (right, bottom) = (width * 4. / 3., height * 4. / 3.);
        let snapshot = serde_json::json!({
            "version": 1, "width_pt": width, "height_pt": height,
            "pages": [[0., 0.]],
            "primitives": [{"kind": "path", "transform": [1., 0., 0., 1., 0., 0.],
                "commands": [[0., 0., 0.], [1., right, 0.], [1., right, bottom], [1., 0., bottom], [4.]],
                "fill": [255, 255, 255, 255]}]
        });
        let bytes = file_metafile(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let int = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let plus_header = int(4) as usize;
        assert_eq!(int(plus_header + 36), FILE_METAFILE_DPI as u32);
        assert_eq!(int(plus_header + 40), FILE_METAFILE_DPI as u32);
        let _runtime = GdiPlus::new().unwrap();
        // Read the file through GDI+, rather than only checking rclFrame:
        // Office sizes EMF+ pictures using intrinsic bounds / logical DPI.
        unsafe {
            let handle = SetEnhMetaFileBits(&bytes);
            assert!(!handle.is_invalid());
            let mut image = Metafile::default();
            check(gp::GdipCreateMetafileFromEmf(handle, true, &mut image.0)).unwrap();
            let (mut pixel_width, mut pixel_height, mut dpi_x, mut dpi_y) = (0, 0, 0., 0.);
            check(gp::GdipGetImageWidth(image.0.cast(), &mut pixel_width)).unwrap();
            check(gp::GdipGetImageHeight(image.0.cast(), &mut pixel_height)).unwrap();
            check(gp::GdipGetImageHorizontalResolution(
                image.0.cast(),
                &mut dpi_x,
            ))
            .unwrap();
            check(gp::GdipGetImageVerticalResolution(
                image.0.cast(),
                &mut dpi_y,
            ))
            .unwrap();
            for (pixels, dpi, points) in
                [(pixel_width, dpi_x, width), (pixel_height, dpi_y, height)]
            {
                assert!((dpi - FILE_METAFILE_DPI).abs() < 0.01);
                assert!(
                    (pixels as f32 * 25.4 / dpi - points * 25.4 / 72.).abs() <= 0.021,
                    "intrinsic extent differs from physical size: {pixels}px at {dpi}dpi, expected {points}pt"
                );
            }
        }
    }
}

fn assert_metafile_background(opaque: bool) {
    let mut snapshot = serde_json::json!({
        "version": 1, "width_pt": 75., "height_pt": 150., "pages": [[0., 0.]],
        "primitives": [{"kind": "path", "transform": [1., 0., 0., 1., 0., 0.],
            "commands": [[0., 10., 20.], [1., 90., 20.], [1., 10., 180.], [4.]],
            "fill": [20, 150, 220, 255]}]
    });
    if opaque {
        snapshot["primitives"].as_array_mut().unwrap().insert(
            0,
            serde_json::json!({
                "kind": "path", "transform": [1., 0., 0., 1., 0., 0.],
                "commands": [[0., 0., 0.], [1., 100., 0.], [1., 100., 200.], [1., 0., 200.], [4.]],
                "fill": [40, 50, 60, 255]
            }),
        );
    }
    let snapshot = serde_json::to_vec(&snapshot).unwrap();
    let bytes = if opaque {
        file_metafile(&snapshot)
    } else {
        metafile(&snapshot)
    }
    .unwrap();
    let int = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    assert!(
        (int(32) as i32 - 2646).abs() <= 1,
        "incorrect physical width"
    );
    assert!(
        (int(36) as i32 - 5292).abs() <= 1,
        "incorrect physical height"
    );
    let mut offset = 0;
    let mut paths = 0;
    while offset < bytes.len() {
        let (kind, size) = (int(offset), int(offset + 4) as usize);
        assert!(size >= 8 && offset + size <= bytes.len());
        if [3, 8, 59, 86, 91].contains(&kind) {
            paths += 1;
        } // Path or polygon in the GDI fallback.
        assert!(
            ![77, 80, 81, 114, 116].contains(&kind),
            "drawing became a raster"
        );
        if kind == 76 {
            // EMR_BITBLT may delimit the dual EMF, without pixels.
            assert_eq!(int(offset + 84), 0);
            assert_eq!(int(offset + 92), 0);
        }
        offset += size;
    }
    assert!(paths > 0, "missing vector path");
    // Exercise the same GDI fallback used by OLE's presentation cache at a
    // much larger size. Only file exports paint their canvas background.
    let result = replay_metafile(&bytes, 800, 1600);
    let at = |x: usize, y: usize| &result[(y * 800 + x) * 4..(y * 800 + x) * 4 + 3];
    assert_eq!(
        at(760, 1520),
        if opaque { [60, 50, 40] } else { [230; 3] },
        "incorrect metafile background"
    );
    assert_eq!(at(680, 200), [220, 150, 20], "wrong horizontal scale");
    assert_eq!(at(100, 1360), [220, 150, 20], "wrong vertical scale");
}

#[test]
fn file_metafile_replays_raster_alpha_on_canvas_background() {
    let _guard = METAFILE_TEST.lock().unwrap();
    let mut png = Vec::new();
    {
        let pixels: Vec<u8> = (0..64 * 32)
            .flat_map(|i| {
                if i % 64 < 32 {
                    [255, 0, 0, 128]
                } else {
                    [0, 255, 0, 255]
                }
            })
            .collect();
        let mut encoder = png::Encoder::new(&mut png, 64, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    for background in [[255, 255, 255, 255], [0, 0, 0, 255]] {
        let snapshot = serde_json::json!({
            "version": 1, "width_pt": 75., "height_pt": 37.5, "pages": [[0., 0.]],
            "primitives": [
                {"kind": "path", "transform": [1., 0., 0., 1., 0., 0.],
                 "commands": [[0., 0., 0.], [1., 100., 0.], [1., 100., 50.], [1., 0., 50.], [4.]],
                 "fill": background},
                {"kind": "image", "transform": [1., 0., 0., 1., 20., 10.],
                 "width": 60., "height": 30., "data": STANDARD.encode(&png)}
            ]
        });
        let bytes = file_metafile(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let result = replay_metafile(&bytes, 800, 400);
        let at = |x: usize, y: usize| &result[(y * 800 + x) * 4..(y * 800 + x) * 4 + 3];
        assert_eq!(at(40, 40), [background[2], background[1], background[0]]);
        assert_eq!(at(520, 200), [0, 255, 0], "opaque raster placement");
        // GDI+'s existing legacy GDI records omit raster alpha. EMF+
        // readers, including tested Word/PowerPoint, retain it.
        let plus = replay_metafile_plus(&bytes, 800, 400);
        let at = |x: usize, y: usize| &plus[(y * 800 + x) * 4..(y * 800 + x) * 4 + 3];
        assert_eq!(at(40, 40), [background[2], background[1], background[0]]);
        assert_eq!(at(520, 200), [0, 255, 0], "EMF+ raster placement");
        let expected = if background[0] == 255 {
            [127, 127, 255]
        } else {
            [0, 0, 128]
        };
        for (&actual, expected) in at(280, 200).iter().zip(expected) {
            assert!(
                (i32::from(actual) - expected).abs() <= 1,
                "raster alpha changed: actual {:?}, expected {:?}, background {:?}",
                at(280, 200),
                expected,
                background
            );
        }
    }
}

fn replay_metafile_plus(bytes: &[u8], width: usize, height: usize) -> Vec<u8> {
    let _runtime = GdiPlus::new().unwrap();
    let mut pixels = vec![230; width * height * 4];
    unsafe {
        let emf = SetEnhMetaFileBits(bytes);
        assert!(!emf.is_invalid());
        let mut image = Metafile::default();
        check(gp::GdipCreateMetafileFromEmf(emf, true, &mut image.0)).unwrap();
        let mut bitmap = Bitmap::default();
        check(gp::GdipCreateBitmapFromScan0(
            width as i32,
            height as i32,
            width as i32 * 4,
            ARGB32,
            Some(pixels.as_mut_ptr()),
            &mut bitmap.0,
        ))
        .unwrap();
        let mut graphics = Graphics::default();
        check(gp::GdipGetImageGraphicsContext(
            bitmap.0.cast(),
            &mut graphics.0,
        ))
        .unwrap();
        check(gp::GdipSetPageUnit(graphics.0, gp::UnitPixel)).unwrap();
        check(gp::GdipDrawImageRect(
            graphics.0,
            image.0.cast(),
            0.,
            0.,
            width as f32,
            height as f32,
        ))
        .unwrap();
        check(gp::GdipFlush(graphics.0, gp::FlushIntentionSync)).unwrap();
    }
    pixels
}

fn replay_metafile(bytes: &[u8], width: usize, height: usize) -> Vec<u8> {
    unsafe {
        let emf = SetEnhMetaFileBits(bytes);
        assert!(!emf.is_invalid());
        let dc = CreateCompatibleDC(None);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = std::ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, None, 0).unwrap();
        let old = SelectObject(dc, bitmap);
        std::slice::from_raw_parts_mut(pixels.cast::<u8>(), width * height * 4).fill(230);
        let played = PlayEnhMetaFile(
            dc,
            emf,
            &RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
        );
        let _ = GdiFlush();
        let result = std::slice::from_raw_parts(pixels.cast::<u8>(), width * height * 4).to_vec();
        SelectObject(dc, old);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(dc);
        let _ = DeleteEnhMetaFile(emf);
        assert!(played.as_bool());
        result
    }
}
