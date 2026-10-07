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
