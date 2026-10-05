use crate::{document::Document, scene};

pub fn parse_svg(svg: String) -> Result<resvg::usvg::Tree, String> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    resvg::usvg::Tree::from_str(&svg, &options).map_err(|error| error.to_string())
}

/// Office's SVG importer does not honor the text-before-edge baseline used by
/// the editor. Resolve fonts and outlines before putting a Windows picture on
/// the clipboard, retaining the physical size and vector quality.
#[cfg(any(windows, test))]
pub fn clipboard_svg(doc: &Document) -> Result<Vec<u8>, String> {
    doc.validate()?;
    let tree = parse_svg(scene::svg(doc))?;
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

/// Clipboard figures retain visible canvas ink with a transparent background.
pub fn clipboard_png(doc: &Document) -> Result<Vec<u8>, String> {
    clipboard_drawing(doc, "png")
}

pub fn clipboard_drawing(doc: &Document, format: &str) -> Result<Vec<u8>, String> {
    Ok(clipboard_figure(doc, format)?.bytes)
}

/// Preserve format details (including bounded PNG dimensions/DPI) in an
/// explicit format-copy receipt as well as in file-export receipts.
pub fn clipboard_figure(doc: &Document, format: &str) -> Result<Figure, String> {
    render_drawing(doc, format, true)
}

#[cfg(windows)]
pub fn office_preview(doc: &Document) -> Result<reshiki_windows::OfficePreview, String> {
    Ok(reshiki_windows::OfficePreview {
        png: clipboard_png(doc)?,
        metafile: crate::native_windows::office_metafile(doc)?,
    })
}

fn render_drawing(doc: &Document, format: &str, clipboard: bool) -> Result<Figure, String> {
    doc.validate()?;
    let svg = if clipboard {
        scene::svg(doc)
    } else {
        scene::svg_with_background(doc)
    };
    if format == "svg" {
        return Ok(Figure {
            bytes: svg.into_bytes(),
            detail: None,
        });
    }
    let tree = parse_svg(svg)?;
    match format {
        #[cfg(windows)]
        "emf" => if clipboard {
            crate::native_windows::metafile(&tree)
        } else {
            crate::native_windows::file_metafile(&tree)
        }
        .map(|bytes| Figure {
            bytes,
            detail: None,
        }),
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
            let budget = if clipboard {
                RasterBudget::clipboard(cfg!(windows))
            } else {
                RasterBudget::FILE
            };
            let (width, height, dpi) =
                png_dimensions(tree.size().width(), tree.size().height(), budget)?;
            let scale = dpi as f32 / 96.0;
            let mut pixmap =
                resvg::tiny_skia::Pixmap::new(width, height).ok_or("Could not allocate image")?;
            if !clipboard {
                let [r, g, b] = doc.canvas_theme.background();
                pixmap.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, 255));
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
                for index in 0..pixmap.pixels().len() {
                    if let Some(pixel) = pixmap.pixels().get(index).copied()
                        && let Some(bytes) = pixmap.data_mut().get_mut(index * 4..index * 4 + 4)
                    {
                        let color = pixel.demultiply();
                        let rgba = [color.red(), color.green(), color.blue(), color.alpha()];
                        bytes.copy_from_slice(&rgba);
                    }
                }
                writer
                    .write_image_data(&pixmap.take())
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

#[derive(Clone, Copy)]
struct RasterBudget {
    adaptive: bool,
    pixels: u64,
    side: u32,
}
impl RasterBudget {
    const FILE: Self = Self {
        adaptive: true,
        pixels: 80_000_000,
        side: u32::MAX,
    };

    fn clipboard(windows: bool) -> Self {
        if windows {
            // RGBA Office previews and CF_DIB must fit the native 64 MiB
            // limit. Also keep Copy Image pasteable as a native picture.
            Self {
                adaptive: true,
                pixels: crate::pictures::MAX_PIXELS,
                side: 8192,
            }
        } else {
            Self {
                adaptive: false,
                ..Self::FILE
            }
        }
    }
}

/// Choose resolution before allocating pixels. PNG resolution metadata keeps
/// the publication size independent of an adaptive clipboard preview's DPI.
fn png_dimensions(
    width: f32,
    height: f32,
    budget: RasterBudget,
) -> Result<(u32, u32, u32), String> {
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err("Invalid PNG dimensions".into());
    }
    let preferred = crate::style::DEFAULT.png_dpi;
    for dpi in [preferred, 600, 300, 150, 96, 72] {
        if !budget.adaptive && dpi != preferred {
            break;
        }
        let scale = dpi as f32 / 96.;
        let w = (width * scale).ceil() as u32;
        let h = (height * scale).ceil() as u32;
        if w <= budget.side && h <= budget.side && u64::from(w) * u64::from(h) <= budget.pixels {
            return Ok((w, h, dpi));
        }
    }
    Err(if budget.adaptive {
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
    let svg = scene::svg_with_background(doc);
    let tree = parse_svg(svg)?;
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
        let [r, g, b] = doc.canvas_theme.background().map(|c| f32::from(c) / 255.);
        content
            .set_fill_rgb(r, g, b)
            .rect(0., 0., layout.width_pt, layout.height_pt)
            .fill_nonzero();
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
mod tests;
