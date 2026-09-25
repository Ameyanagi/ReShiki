use crate::{document::Document, scene};

/// Office's SVG importer does not honor the text-before-edge baseline used by
/// the editor. Resolve fonts and outlines before putting a Windows picture on
/// the clipboard, retaining the physical size and vector quality.
#[cfg(any(windows, test))]
pub(crate) fn clipboard_svg(doc: &Document) -> Result<Vec<u8>, String> {
    doc.validate()?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&scene::svg(doc), &options)
        .map_err(|error| error.to_string())?;
    let outlined = tree.to_string(&resvg::usvg::WriteOptions::default());
    let (_, contents) = outlined.split_once('>').ok_or("Invalid outlined SVG")?;
    // usvg serializes in CSS pixels. Explicit points plus a matching viewBox
    // keep Office from interpreting the coordinates as point-sized pixels.
    let (width, height) = (tree.size().width(), tree.size().height());
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {width} {height}\">{contents}",
        width * 0.75,
        height * 0.75,
    )
    .into_bytes())
}

/// Refresh derived chemistry for an export snapshot without touching editing history.
pub async fn checked_document(
    engine: &crate::engine::LocalEngine,
    mut doc: Document,
) -> Result<Document, String> {
    doc.validate()?;
    // Tracked drawing anchors and semantic attachments can be rendered without
    // assigning a molecular identity to their contacts or ALL/ANY target sets.
    if doc.atoms.is_empty() || doc.atoms.iter().any(|atom| !atom.centroid.is_empty()) {
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

/// Figure export needs valid drawing geometry, not a resolved molecular identity.
/// Refresh computed labels when possible, otherwise preserve the visible snapshot.
pub async fn figure_document(
    engine: &crate::engine::LocalEngine,
    doc: Document,
) -> Result<(Document, Option<String>), String> {
    doc.validate()?;
    match checked_document(engine, doc.clone()).await {
        Ok(checked) => Ok((checked, None)),
        Err(error) => Ok((
            doc,
            Some(format!(
                "Drawing preserved; chemistry needs review: {error}"
            )),
        )),
    }
}

pub struct Figure {
    pub bytes: Vec<u8>,
    pub detail: Option<String>,
}

/// Render at the style's physical size. Large PNG files use a bounded resolution.
pub fn drawing(doc: &Document, format: &str) -> Result<Vec<u8>, String> {
    Ok(figure(doc, format)?.bytes)
}

/// Include the actual raster dimensions and resolution for the export receipt.
pub fn figure(doc: &Document, format: &str) -> Result<Figure, String> {
    render_drawing(doc, format, false)
}

/// Clipboard figures overlay the destination page or slide without a white box.
pub fn clipboard_png(doc: &Document) -> Result<Vec<u8>, String> {
    Ok(render_drawing(doc, "png", true)?.bytes)
}

#[cfg(windows)]
pub fn office_preview(doc: &Document) -> Result<reshiki_windows::OfficePreview, String> {
    Ok(reshiki_windows::OfficePreview {
        png: clipboard_png(doc)?,
        metafile: crate::native_windows::office_metafile(doc)?,
    })
}

fn render_drawing(doc: &Document, format: &str, transparent: bool) -> Result<Figure, String> {
    doc.validate()?;
    let svg = scene::svg(doc);
    if format == "svg" {
        return Ok(Figure {
            bytes: svg.into_bytes(),
            detail: None,
        });
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
        .map(|bytes| Figure {
            bytes,
            detail: None,
        })
        .map_err(|e| e.to_string()),
        "png" => {
            let (width, height, dpi) =
                png_dimensions(tree.size().width(), tree.size().height(), !transparent)?;
            let scale = dpi as f32 / 96.0;
            let mut pixmap =
                resvg::tiny_skia::Pixmap::new(width, height).ok_or("Could not allocate image")?;
            if !transparent {
                pixmap.fill(resvg::tiny_skia::Color::WHITE);
            }
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
                // tiny-skia stores premultiplied colors; PNG requires straight
                // alpha or antialiased colored edges acquire dark fringes.
                let pixels: Vec<u8> = pixmap
                    .pixels()
                    .iter()
                    .flat_map(|pixel| {
                        let color = pixel.demultiply();
                        [color.red(), color.green(), color.blue(), color.alpha()]
                    })
                    .collect();
                writer
                    .write_image_data(&pixels)
                    .map_err(|e| e.to_string())?;
            }
            Ok(Figure {
                bytes,
                detail: Some(format!("PNG: {width} × {height} pixels at {dpi} dpi")),
            })
        }
        _ => Err("Unsupported drawing export".into()),
    }
}

/// Bound both allocation and encoded pixel work. The clipboard keeps its fixed
/// resolution because its native picture bounds use that resolution too.
fn png_dimensions(width: f32, height: f32, adaptive: bool) -> Result<(u32, u32, u32), String> {
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err("Invalid PNG dimensions".into());
    }
    let preferred = crate::style::DEFAULT.png_dpi;
    for dpi in [preferred, 600, 300, 150, 96, 72] {
        if !adaptive && dpi != preferred {
            break;
        }
        let scale = dpi as f32 / 96.;
        let w = (width * scale).ceil() as u32;
        let h = (height * scale).ceil() as u32;
        if u64::from(w) * u64::from(h) <= 80_000_000 {
            return Ok((w, h, dpi));
        }
    }
    Err(if adaptive {
        "Drawing is too large for a PNG even at 72 dpi; use SVG or PDF."
    } else {
        "Drawing is too large for a 1200 dpi PNG; use SVG or PDF."
    }
    .into())
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
    fn raster_budget_rejects_invalid_and_extreme_sizes_without_allocating() {
        for (width, height) in [
            (0., 10.),
            (10., -1.),
            (f32::NAN, 10.),
            (10., f32::INFINITY),
            (f32::MAX, f32::MAX),
        ] {
            assert!(png_dimensions(width, height, true).is_err());
        }
        assert_eq!(png_dimensions(100., 100., true), Ok((1250, 1250, 1200)));
        assert_eq!(png_dimensions(800., 800., true), Ok((5000, 5000, 600)));
        assert!(png_dimensions(800., 800., false).is_err());
    }
    #[test]
    fn clipboard_png_has_clear_background_and_straight_alpha_on_colored_edges() {
        use crate::{
            document::{Annotation, Point},
            typography::TextFormat,
        };
        let mut format = TextFormat::default();
        format.style.color = [180, 50, 55];
        let mut doc = Document::default();
        doc.annotations.push(Annotation {
            id: 1,
            position: Point::default(),
            text: "O".into(),
            format,
        });
        let bytes = clipboard_png(&doc).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().pixel_dims.unwrap().xppu, 47244);
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        let pixels = &pixels[..frame.buffer_size()];
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        assert!(
            pixels
                .chunks_exact(4)
                .any(|p| p[3] == 255 && p[..3] == [180, 50, 55])
        );
        let edges: Vec<_> = pixels
            .chunks_exact(4)
            .filter(|p| (64..192).contains(&p[3]))
            .collect();
        assert!(!edges.is_empty());
        assert!(edges.iter().all(|p| {
            p[..3]
                .iter()
                .zip([180u8, 50, 55])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        }));
        let opaque = drawing(&doc, "png").unwrap();
        let image = image::load_from_memory(&opaque).unwrap().into_rgba8();
        assert!(
            image.pixels().all(|pixel| pixel[3] == 255),
            "file export remains opaque"
        );
    }
    #[test]
    fn office_clipboard_svg_outlines_text_without_moving_or_resizing_it() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
                .unwrap();
        let svg = String::from_utf8(clipboard_svg(&doc).unwrap()).unwrap();
        let xml = roxmltree::Document::parse(&svg).unwrap();
        assert!(
            xml.root_element()
                .attribute("width")
                .unwrap()
                .ends_with("pt")
        );
        assert!(!xml.descendants().any(|node| node.has_tag_name("text")));
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let original = resvg::usvg::Tree::from_str(&scene::svg(&doc), &options).unwrap();
        // The receiving computer need not have the original fonts installed.
        let restored = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
        assert!((original.size().width() - restored.size().width()).abs() < 0.001);
        assert!((original.size().height() - restored.size().height()).abs() < 0.001);
        let render = |tree: &resvg::usvg::Tree| {
            let mut pixels = resvg::tiny_skia::Pixmap::new(
                (original.size().width() * 3.).ceil() as u32,
                (original.size().height() * 3.).ceil() as u32,
            )
            .unwrap();
            resvg::render(
                tree,
                resvg::tiny_skia::Transform::from_scale(3., 3.),
                &mut pixels.as_mut(),
            );
            pixels
        };
        let expected = render(&original);
        let actual = render(&restored);
        let difference: u64 = expected
            .data()
            .iter()
            .zip(actual.data())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        assert!(difference < expected.data().len() as u64 / 100);
    }

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
