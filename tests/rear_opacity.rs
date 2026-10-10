use reshiki::{
    depth_appearance as depth,
    document::{Document, Point},
    export, scene,
};

fn drawing(alpha: f32) -> Document {
    let mut doc = Document::from_json(include_bytes!(
        "fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
    doc
}
fn rgba(doc: &Document) -> image::RgbaImage {
    image::load_from_memory(&export::clipboard_png(doc).unwrap())
        .unwrap()
        .into_rgba8()
}
fn ink_alpha(image: &image::RgbaImage, point: Point, doc: &Document) -> u8 {
    let drawing = scene::primitives(doc);
    let (lo, hi) = scene::bounds(&drawing);
    let x = ((point.x - lo.x) / (hi.x - lo.x) * image.width() as f32).round() as u32;
    let y = ((point.y - lo.y) / (hi.y - lo.y) * image.height() as f32).round() as u32;
    (x.saturating_sub(3)..=(x + 3).min(image.width() - 1))
        .flat_map(|x| {
            (y.saturating_sub(3)..=(y + 3).min(image.height() - 1))
                .map(move |y| image.get_pixel(x, y).0[3])
        })
        .max()
        .unwrap()
}

#[test]
fn transparent_raster_and_vector_exports_have_real_alpha_and_front_ink_on_each_theme() {
    for theme in reshiki::canvas_theme::CanvasTheme::ALL {
        let mut doc = drawing(0.5);
        doc.canvas_theme = theme;
        let original = doc.clone();
        let image = rgba(&doc);
        let rear = ink_alpha(&image, Point::new(-8.102754, -18.966398), &doc);
        let front = ink_alpha(&image, Point::new(-69.3732, 73.96892), &doc);
        assert!((126..=129).contains(&rear), "rear alpha {rear}");
        assert_eq!(front, 255);
        let expected = theme.color([0; 3]);
        assert!(
            image
                .pixels()
                .any(|p| p.0[3] >= 126 && p.0[3] <= 129 && p.0[..3] == expected)
        );
        assert!(scene::svg(&doc).contains("opacity=\"0.5\""));
        let pdf = export::clipboard_drawing(&doc, "pdf").unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let pdf = String::from_utf8_lossy(&pdf);
        assert!(
            pdf.contains("/ca 0.5") || pdf.contains("/CA 0.5"),
            "PDF lacks real opacity state"
        );
        for format in ["png", "svg", "pdf"] {
            assert!(!export::figure(&doc, format).unwrap().bytes.is_empty());
        }
        assert_eq!(doc, original);
    }
}

#[test]
fn hidden_cage_ink_disappears_whole_rim_stays_solid_and_100_restores_original_figure() {
    let mut doc = drawing(1.);
    let original = doc.clone();
    let svg = scene::svg(&doc);
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    assert!(!scene::svg(&doc).contains("opacity="));
    let image = rgba(&doc);
    assert_eq!(ink_alpha(&image, Point::new(-69.3732, 73.96892), &doc), 255);
    assert_eq!(
        ink_alpha(&image, Point::new(-8.102754, -18.966398), &doc),
        0
    );
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
    assert_eq!(scene::svg(&doc), svg);
}

#[test]
fn native_preserves_opacity_and_external_editable_copy_reports_presentation_loss() {
    let doc = drawing(0.25);
    let original = doc.clone();
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.depth_appearance, doc.depth_appearance);
    assert_eq!(scene::svg(&reopened), scene::svg(&doc));
    let error = reshiki::exchange::drawing::write(&doc, Default::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("rear opacity"));
    assert!(error.contains("SVG"));
    let (_, warnings) = reshiki::exchange::drawing::write_clipboard(&doc).unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("rear opacity"))
    );
    assert_eq!(doc, original);
}

fn crossing_highlights(flat: bool) -> Document {
    use reshiki::palette::Color;
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    let a = doc.add_atom("C", Point::new(0., -42.));
    let b = doc.add_atom("C", Point::new(30., -21.));
    doc.add_bond(center, a, 1, "plain");
    doc.add_bond(center, b, 1, "plain");
    doc.atom_mut(center).unwrap().display.highlight = Some(Color::Custom([0, 0, 255]));
    let left = doc.add_atom("C", Point::new(-45., 0.));
    let right = doc.add_atom("C", Point::new(45., 0.));
    doc.add_bond(left, right, 1, "plain");
    doc.bonds.last_mut().unwrap().highlight = Some(Color::Custom([255, 0, 0]));
    for atom in &mut doc.atoms {
        atom.depth = if flat { 8. } else { -30. };
    }
    if !flat {
        doc.atom_mut(center).unwrap().depth = 30.;
    }
    doc
}

fn pixel_at(image: &image::RgbaImage, point: Point, doc: &Document) -> [u8; 4] {
    let (lo, hi) = scene::bounds(&scene::primitives(doc));
    let x = ((point.x - lo.x) / (hi.x - lo.x) * image.width() as f32).round() as u32;
    let y = ((point.y - lo.y) / (hi.y - lo.y) * image.height() as f32).round() as u32;
    image.get_pixel(x, y).0
}

#[test]
fn flat_retained_rear_setting_keeps_distinct_color_highlight_overlap_and_exact_output() {
    let mut doc = crossing_highlights(true);
    let svg = scene::svg(&doc);
    let image = rgba(&doc);
    assert_eq!(pixel_at(&image, Point::new(0., 4.), &doc), [0, 0, 255, 255]);
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    assert_eq!(scene::svg(&doc), svg);
    assert_eq!(
        rgba(&doc),
        image,
        "flat RGB overlap must remain byte-identical"
    );
}

#[test]
fn opaque_front_atom_highlight_stays_above_different_color_bond_with_rear_alpha_elsewhere() {
    let mut doc = crossing_highlights(false);
    let before = rgba(&doc);
    let sample = Point::new(0., 4.);
    assert_eq!(pixel_at(&before, sample, &doc), [0, 0, 255, 255]);
    let ids = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    assert!(scene::svg(&doc).contains("opacity=\"0.25\""));
    assert_eq!(pixel_at(&rgba(&doc), sample, &doc), [0, 0, 255, 255]);
}
