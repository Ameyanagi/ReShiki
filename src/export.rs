use crate::{document::Document, scene};

/// Refresh derived chemistry for an export snapshot without touching editing history.
pub async fn checked_document(
    engine: &crate::engine::PythonEngine,
    mut doc: Document,
) -> Result<Document, String> {
    doc.validate()?;
    if doc.atoms.is_empty() {
        return Ok(doc);
    }
    let response = engine
        .request(crate::engine::Request::molecule("analyze", doc.clone()))
        .await?;
    let checked = response
        .document
        .ok_or("Chemistry engine returned no drawing")?;
    crate::atom_labels::refresh_computed(&mut doc, &checked);
    Ok(doc)
}

/// Render at the style's physical size. PDF stays vector; PNG uses line-art resolution.
pub fn drawing(doc: &Document, format: &str) -> Result<Vec<u8>, String> {
    doc.validate()?;
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
            // usvg resolves CSS physical units at 96 px/in.
            svg2pdf::PageOptions { dpi: 96.0 },
        )
        .map_err(|e| e.to_string()),
        "png" => {
            let dpi = crate::style::DEFAULT.png_dpi;
            let scale = dpi as f32 / 96.0;
            let width = (tree.size().width() * scale).ceil() as u32;
            let height = (tree.size().height() * scale).ceil() as u32;
            if u64::from(width) * u64::from(height) > 80_000_000 {
                return Err(format!(
                    "Drawing is too large for a {dpi} dpi PNG; use SVG or PDF."
                ));
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
                    xppu: (dpi as f64 / 0.0254).round() as u32,
                    yppu: (dpi as f64 / 0.0254).round() as u32,
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
/// Export all physical sheets without scaling the drawing. Page gaps and margin
/// guides belong to the editor only; marks beyond a sheet edge are clipped.
pub fn pages_pdf(doc: &Document) -> Result<Vec<u8>, String> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
    use std::collections::HashMap;
    doc.validate()?;
    let layout = doc
        .page_layout
        .as_ref()
        .ok_or("Set up publication pages before exporting a page PDF.")?;
    let svg = scene::svg(doc);
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    let (chunk, root) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default())
        .map_err(|e| e.to_string())?;
    let mut next = Ref::new(1);
    let catalog = next.bump();
    let pages = next.bump();
    let page_ids: Vec<_> = (0..layout.count())
        .map(|_| (next.bump(), next.bump()))
        .collect();
    let mut mapping = HashMap::new();
    let chunk = chunk.renumber(|old| *mapping.entry(old).or_insert_with(|| next.bump()));
    let root = mapping
        .get(&root)
        .copied()
        .ok_or("Could not embed the drawing in the page PDF.")?;
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages)
        .kids(page_ids.iter().map(|(id, _)| *id))
        .count(layout.count() as i32);
    let (drawing_lo, drawing_hi) = scene::bounds(&scene::primitives(doc));
    let scale = crate::style::DEFAULT.points_per_world();
    let width = (drawing_hi.x - drawing_lo.x) * scale;
    let height = (drawing_hi.y - drawing_lo.y) * scale;
    let name = Name(b"Drawing");
    for (index, (id, content_id)) in page_ids.into_iter().enumerate() {
        let (lo, _) = layout.bounds(index).ok_or("Invalid page in layout.")?;
        let mut page = pdf.page(id);
        page.parent(pages)
            .media_box(Rect::new(0., 0., layout.width_pt, layout.height_pt))
            .contents(content_id);
        page.resources().x_objects().pair(name, root);
        page.finish();
        let mut content = Content::new();
        content
            .save_state()
            .rect(0., 0., layout.width_pt, layout.height_pt)
            .clip_nonzero()
            .end_path();
        content
            .transform([
                width,
                0.,
                0.,
                height,
                (drawing_lo.x - lo.x) * scale,
                layout.height_pt - (drawing_lo.y - lo.y) * scale - height,
            ])
            .x_object(name);
        content.restore_state();
        pdf.stream(content_id, &content.finish());
    }
    pdf.extend(&chunk);
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_pdf_and_png_contain_real_drawing_data() {
        let d: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
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
        assert_eq!(reader.info().pixel_dims.unwrap().xppu, 47244);
    }

    #[test]
    fn physical_scale_survives_svg_and_png_export() {
        use crate::document::Point;
        let mut d = Document::default();
        let a = d.add_atom("C", Point::default());
        let b = d.add_atom("C", Point::new(42.0, 0.0));
        d.add_bond(a, b, 1, "plain");
        let svg = scene::svg(&d);
        let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
        // A 14.4 pt bond plus a 4 pt border on each side, independent of screen zoom.
        assert!((tree.size().width() * 72.0 / 96.0 - 22.4).abs() < 0.001);
        let png = drawing(&d, "png").unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .unwrap();
        assert_eq!(
            reader.info().width,
            (22.4_f32 / 72.0 * 1200.0).ceil() as u32
        );
    }
}
