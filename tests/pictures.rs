use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use moruno::{
    document::{Document, History, Point},
    editing::{self, Transform},
    engine::{ChemistryEngine, PythonEngine, Request},
    graphics::{Graphic, GraphicKind},
    pictures::Picture,
};
use std::io::Cursor;

fn encoded(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

fn picture() -> Picture {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_fn(12, 8, |x, y| match (x < 6, y < 4) {
        (true, true) => Rgba([255, 0, 0, 255]),
        (false, true) => Rgba([0, 255, 0, 255]),
        (true, false) => Rgba([0, 0, 255, 255]),
        (false, false) => Rgba([0, 0, 0, 0]),
    }));
    Picture::import(&encoded(&image, ImageFormat::Png)).unwrap()
}

#[test]
fn formats_normalize_to_portable_png_and_small_images_keep_aspect_ratio() {
    let rgb = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(3, 2, image::Rgb([250, 30, 40])));
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Tiff,
        ImageFormat::WebP,
    ] {
        let imported = Picture::import(&encoded(&rgb, format)).unwrap();
        assert_eq!((imported.width(), imported.height()), (3, 2));
        assert!(imported.png().starts_with(b"\x89PNG\r\n\x1a\n"));
        let doc = imported.document();
        doc.validate().unwrap();
        assert_eq!(
            doc,
            serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap()
        );
    }
    let narrow = DynamicImage::new_rgba8(1, 10);
    let g = Picture::import(&encoded(&narrow, ImageFormat::Png))
        .unwrap()
        .graphic(1, Point::new(50., 60.));
    assert!((g.axis_y.y / g.axis_x.x - 10.).abs() < 0.0001);
    assert!((g.origin.x + g.axis_x.x / 2. - 50.).abs() < 0.0001);
    assert!((g.origin.y + g.axis_y.y / 2. - 60.).abs() < 0.0001);
}

#[test]
fn invalid_data_and_picture_frames_are_rejected_without_panicking() {
    for bytes in [&b""[..], &b"not a picture"[..], &b"\x89PNG\r\n\x1a\n"[..]] {
        assert!(Picture::import(bytes).is_err());
    }
    assert!(Picture::import(&vec![0; moruno::pictures::MAX_BYTES + 1]).is_err());
    assert!(
        Picture::import(&encoded(
            &DynamicImage::new_rgba8(8193, 1),
            ImageFormat::Png
        ))
        .is_err()
    );
    assert!(serde_json::from_str::<Picture>("\"invalid-base64\"").is_err());
    let mut g = picture().graphic(1, Point::default());
    g.picture = None;
    assert!(g.validate().is_err());
    g.picture = Some(picture());
    g.kind = GraphicKind::Rectangle;
    assert!(g.validate().is_err());
    g.kind = GraphicKind::Picture;
    g.axis_y.x = 1.;
    assert!(g.validate().is_err());
    g.axis_y.x = f32::NAN;
    assert!(g.validate().is_err());
}

#[test]
fn picture_transforms_selection_copy_and_undo_preserve_embedded_data() {
    let mut doc = picture().document();
    let before = doc.clone();
    let mut history = History::default();
    editing::transform(&mut doc, &[1], Transform::Rotate(37.));
    editing::transform(&mut doc, &[1], Transform::FlipHorizontal);
    doc.validate().unwrap();
    let g = &doc.graphics[0];
    assert!(g.hit(
        g.origin.offset(
            (g.axis_x.x + g.axis_y.x) / 2.,
            (g.axis_x.y + g.axis_y.y) / 2.
        ),
        0.
    ));
    assert!(!g.hit(g.origin.offset(500., 500.), 1.));
    let selected = editing::selection(&doc, &[1]);
    assert_eq!(
        editing::append(&mut doc, &selected, Point::new(100., 50.)),
        vec![2]
    );
    assert_eq!(doc.graphics[0].picture, doc.graphics[1].picture);
    assert_eq!(doc.graphics[0].picture, before.graphics[0].picture);
    let after = doc.clone();
    history.commit(before.clone(), &doc);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, after);
    assert_eq!(
        doc,
        serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap()
    );
    let mut unchanged = doc.graphics[0].clone();
    unchanged.edit_point(0, Point::new(1000., 1000.));
    assert_eq!(unchanged, doc.graphics[0]);
}

fn sample_quadrants(doc: &Document) -> Vec<[u8; 4]> {
    let png = moruno::export::drawing(doc, "png").unwrap();
    let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
    // Scene exports have fixed outer padding; sample within each colored area.
    let center_x = pixels.width() / 2;
    let center_y = pixels.height() / 2;
    let radius = (moruno::style::DEFAULT.points_per_world() * 10. * 1200. / 72.) as u32;
    [
        (center_x - radius, center_y - radius),
        (center_x + radius, center_y - radius),
        (center_x - radius, center_y + radius),
        (center_x + radius, center_y + radius),
    ]
    .into_iter()
    .map(|(x, y)| pixels.get_pixel(x, y).0)
    .collect()
}

#[test]
fn exported_pictures_preserve_pixels_transparency_rotation_and_layer_order() {
    let mut doc = picture().document();
    let g = &mut doc.graphics[0];
    g.origin = Point::new(-30., -30.);
    g.axis_x = Point::new(60., 0.);
    g.axis_y = Point::new(0., 60.);
    assert_eq!(
        sample_quadrants(&doc),
        vec![
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255; 4]
        ]
    );
    editing::transform(&mut doc, &[1], Transform::FlipHorizontal);
    assert_eq!(
        sample_quadrants(&doc),
        vec![
            [0, 255, 0, 255],
            [255, 0, 0, 255],
            [255; 4],
            [0, 0, 255, 255]
        ]
    );
    editing::transform(&mut doc, &[1], Transform::Rotate(90.));
    assert_eq!(
        sample_quadrants(&doc),
        vec![
            [255; 4],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 0, 0, 255]
        ]
    );
    let pdf = moruno::export::drawing(&doc, "pdf").unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
    doc.page_layout = Some(Default::default());
    let pdf = moruno::export::pages_pdf(&doc).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
    let mut cover = Graphic::dragged(
        2,
        GraphicKind::Rectangle,
        Point::new(-30., -30.),
        Point::new(30., 30.),
        Default::default(),
        Default::default(),
        false,
    );
    cover.style.fill = Some([200, 100, 50]);
    cover.layer = 1;
    doc.graphics.push(cover);
    assert_eq!(sample_quadrants(&doc), vec![[200, 100, 50, 255]; 4]);
    doc.graphics[1].layer = -2;
    assert_eq!(
        sample_quadrants(&doc),
        vec![
            [200, 100, 50, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 0, 0, 255]
        ]
    );
}

#[tokio::test]
async fn chemistry_retains_pictures_and_unsupported_exchange_fails_explicitly() {
    let engine = PythonEngine::default();
    let mut doc = engine
        .execute(Request::import_smiles("CCO"))
        .await
        .unwrap()
        .document
        .unwrap();
    doc.graphics
        .push(picture().graphic(10, Point::new(100., 100.)));
    for op in ["analyze", "clean"] {
        let response = engine
            .execute(Request::molecule(op, doc.clone()))
            .await
            .unwrap();
        assert_eq!(response.document.unwrap().graphics, doc.graphics);
        assert_eq!(response.analysis.unwrap().smiles, "CCO");
    }
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let error = engine.execute(request).await.unwrap_err();
        assert!(error.contains("embedded picture exchange"), "{error}");
    }
}
