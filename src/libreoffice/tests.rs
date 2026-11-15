use super::*;

#[test]
fn preview_preserves_native_bytes_and_physical_extent() {
    let bytes = include_bytes!("../../tests/fixtures/ui-drawn-ethanol.reshiki");
    let packet = preview(bytes).unwrap();
    assert_eq!(STANDARD.decode(packet.native).unwrap(), bytes);
    let png = STANDARD.decode(packet.png).unwrap();
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let info = reader.info();
    let resolution = info.pixel_dims.unwrap();
    assert!(
        (info.width as f64 * 100_000.0 / resolution.xppu as f64 - packet.extent[0] as f64).abs()
            < 4.0
    );
    assert!(
        (info.height as f64 * 100_000.0 / resolution.yppu as f64 - packet.extent[1] as f64).abs()
            < 4.0
    );
}

#[test]
fn malformed_drawings_are_not_given_a_preview() {
    assert!(preview(b"not a drawing").is_err());
    assert!(preview(br#"{"version":999999}"#).is_err());
}

/// S10: a drawing over the agent import's object limit is refused before
/// it is rendered, which took minutes and gigabytes.
#[test]
fn a_drawing_over_the_object_limit_is_refused_before_rendering() {
    let mut doc = Document::default();
    doc.add_atom("C", crate::document::Point::default());
    let atom = doc.atoms[0].clone();
    let limit = Budgets::default().max_objects as u64;
    doc.atoms.extend((1..=limit).map(|i| crate::document::Atom {
        id: i + 1,
        position: crate::document::Point::new((i % 300) as f32 * 42., (i / 300) as f32 * 42.),
        ..atom.clone()
    }));
    let error = preview(&serde_json::to_vec(&doc).unwrap()).err().unwrap();
    assert_eq!(error, "The drawing has 100001 objects; the limit is 100000");
}
