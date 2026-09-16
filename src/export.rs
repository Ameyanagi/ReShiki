use crate::{document::Document, scene};

/// Render the shared vector scene. PDF stays vector; PNG is rendered at 300 dpi.
pub fn drawing(doc: &Document, format: &str) -> Result<Vec<u8>, String> {
    let svg = scene::svg(doc);
    if format == "svg" {
        return Ok(svg.into_bytes());
    }
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    match format {
        "pdf" => svg2pdf::to_pdf(
            &tree,
            svg2pdf::ConversionOptions::default(),
            svg2pdf::PageOptions::default(),
        )
        .map_err(|e| e.to_string()),
        "png" => {
            let scale = 300.0 / 96.0;
            let width = (tree.size().width() * scale).ceil() as u32;
            let height = (tree.size().height() * scale).ceil() as u32;
            if u64::from(width) * u64::from(height) > 80_000_000 {
                return Err("Drawing is too large for a 300 dpi PNG; use SVG or PDF.".into());
            }
            let mut pixmap =
                resvg::tiny_skia::Pixmap::new(width, height).ok_or("Could not allocate image")?;
            pixmap.fill(resvg::tiny_skia::Color::WHITE);
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            let mut bytes = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut bytes, width, height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.set_pixel_dims(Some(png::PixelDimensions {
                    xppu: 11811,
                    yppu: 11811,
                    unit: png::Unit::Meter,
                }));
                let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
                writer
                    .write_image_data(pixmap.data())
                    .map_err(|e| e.to_string())?;
            }
            Ok(bytes)
        }
        _ => Err("Unsupported drawing export".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_pdf_and_png_contain_real_drawing_data() {
        let d: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.moruno"))
                .unwrap();
        let pdf = drawing(&d, "pdf").unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 500);
        let png = drawing(&d, "png").unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.len() > 1000);
        let reader = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().pixel_dims.unwrap().xppu, 11811);
    }
}
