use crate::{canvas_theme::CanvasTheme, document::Document, scene};
use std::borrow::Cow;

/// The paper behind a copied or exported picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    /// No paper. The ink keeps the canvas's colors.
    Transparent,
    /// The drawing's own canvas color.
    Canvas,
    /// This color. Black and white ink swap to stay readable on it.
    Color([u8; 3]),
}

impl Background {
    /// The drawing as painted on this background, and the paper color.
    /// A color background takes the canvas, light or dark, whose ink reads
    /// better on it. Only Ink follows it: palette, element and custom colors
    /// keep how the drawing's own canvas shows them. 3D models keep the
    /// drawing's theme and take its colors for the new canvas.
    pub fn apply(self, doc: &Document) -> (Cow<'_, Document>, Option<[u8; 3]>) {
        use crate::color_contrast::contrast;
        match self {
            Self::Transparent => (Cow::Borrowed(doc), None),
            Self::Canvas => (Cow::Borrowed(doc), Some(doc.canvas_theme.background())),
            Self::Color(paper) => {
                let canvas = if contrast([0; 3], paper) >= contrast([255; 3], paper) {
                    CanvasTheme::Light
                } else {
                    CanvasTheme::Dark
                };
                let doc = if doc.canvas_theme == canvas {
                    Cow::Borrowed(doc)
                } else {
                    let mut flipped = crate::canvas_theme::resolved_document(doc).into_owned();
                    flipped.canvas_theme = canvas;
                    flipped.color_theme = doc.color_theme;
                    flipped.custom_theme.clone_from(&doc.custom_theme);
                    Cow::Owned(flipped)
                };
                (doc, Some(paper))
            }
        }
    }
}

pub fn parse_svg(svg: String) -> Result<resvg::usvg::Tree, String> {
    parse_svg_with_fontdb(&svg, crate::style::fontdb())
}

fn parse_svg_with_fontdb(
    svg: &str,
    fontdb: std::sync::Arc<resvg::usvg::fontdb::Database>,
) -> Result<resvg::usvg::Tree, String> {
    let default_selector = resvg::usvg::FontResolver::default_font_selector();
    let options = resvg::usvg::Options {
        fontdb,
        font_resolver: resvg::usvg::FontResolver {
            select_font: Box::new(move |font, db| {
                default_selector(font, db).or_else(|| {
                    let family = font.families().iter().find_map(|family| match family {
                        resvg::usvg::FontFamily::Named(name) => Some(name.as_str()),
                        _ => None,
                    })?;
                    // usvg selects the base face before it exposes characters;
                    // its default glyph fallback handles the rest of the span.
                    crate::style::resolve_font(
                        db,
                        family,
                        resvg::usvg::fontdb::Weight(font.weight()),
                        font.style().into(),
                        'N',
                    )
                })
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    resvg::usvg::Tree::from_str(svg, &options).map_err(|error| error.to_string())
}

/// Office's SVG importer does not honor the editor's text-before-edge baseline.
/// Resolve fonts to paths and retain physical dimensions for picture placement.
fn office_svg(tree: &resvg::usvg::Tree) -> Result<Vec<u8>, String> {
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

/// Outline SVG text with the same parsed-tree glyphs, positions, and sizes as
/// PNG export, retaining physical dimensions in points.
pub fn outlined_svg(svg: String) -> Result<Vec<u8>, String> {
    office_svg(&parse_svg(svg)?)
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

/// A prepared publication export: the file bytes and the status details, the
/// raster receipt before any chemistry review notice.
pub struct Publication {
    pub bytes: Vec<u8>,
    pub details: Vec<String>,
}

/// Prepare a figure snapshot and render it off the async executor, as the
/// app's figure export does. `pages` exports every publication page as a PDF;
/// otherwise PNG exports use `dpi`, or a lower resolution to stay within
/// `max_pixels` (at most [`FILE_PIXELS`]).
/// `editable` includes the prepared drawing in supported figures when it fits the limit.
#[allow(clippy::too_many_arguments)]
pub async fn publication(
    engine: &crate::engine::LocalEngine,
    doc: Document,
    format: &'static str,
    pages: bool,
    editable: bool,
    background: Background,
    dpi: u32,
    max_pixels: u64,
) -> Result<Publication, String> {
    publication_within(
        engine,
        doc,
        format,
        pages,
        editable,
        background,
        dpi,
        max_pixels,
        usize::MAX,
    )
    .await
}

/// [`publication`] for a caller that accepts at most `max_bytes`: an
/// `svg-office` figure certain to be larger fails early with [`OVER_BYTES`].
/// The caller still checks the size of the figure it gets.
#[allow(clippy::too_many_arguments)]
pub async fn publication_within(
    engine: &crate::engine::LocalEngine,
    doc: Document,
    format: &'static str,
    pages: bool,
    editable: bool,
    background: Background,
    dpi: u32,
    max_pixels: u64,
    max_bytes: usize,
) -> Result<Publication, String> {
    if format == "png" && !pages {
        use crate::document::Point;
        doc.validate()?;
        // Atoms too far apart for a PNG are a cheap sign that the figure will
        // not fit. Measuring its scene now saves the chemistry check.
        let budget = RasterBudget::file(dpi, max_pixels);
        let atoms = || doc.atoms.iter().map(|atom| atom.position);
        let lo = atoms().reduce(|lo, at| Point::new(lo.x.min(at.x), lo.y.min(at.y)));
        let hi = atoms().reduce(|hi, at| Point::new(hi.x.max(at.x), hi.y.max(at.y)));
        if lo
            .zip(hi)
            .is_some_and(|span| png_fits(span, budget).is_err())
        {
            png_fits(scene::bounds(&scene::primitives(&doc)), budget)?;
        }
    }
    let (doc, notice) = figure_document(engine, doc).await?;
    let doc = doc.current();
    let attachment_notice = pages
        .then(|| crate::arrow_anchors::export_notice(&doc))
        .flatten();
    let (figure, editable_detail) = crate::task::spawn_blocking(move || {
        let (drawing, detail) = if editable
            && !pages
            && matches!(format, "png" | "svg" | "svg-office" | "pdf")
        {
            let envelope = crate::payload::encode(&doc)?;
            if envelope.len() <= crate::payload::EMBED_LIMIT {
                (
                    Some(envelope),
                    Some("Includes the editable drawing".to_owned()),
                )
            } else {
                (
                    None,
                    Some("Saved without the editable drawing: it is larger than 2 MB".to_owned()),
                )
            }
        } else {
            (None, None)
        };
        let figure = if pages {
            pages_pdf(&doc, background).map(|bytes| Figure {
                bytes,
                detail: None,
            })
        } else {
            with_attachment_notice(
                &doc,
                render_drawing(
                    &doc,
                    format,
                    false,
                    background,
                    RasterBudget::file(dpi, max_pixels),
                    max_bytes,
                    drawing.as_deref(),
                ),
            )
        }?;
        Ok::<_, String>((figure, detail))
    })
    .await
    .map_err(|error| error.to_string())??;
    let details = figure
        .detail
        .into_iter()
        .chain(notice)
        .chain(attachment_notice)
        .chain(editable_detail)
        .collect();
    Ok(Publication {
        bytes: figure.bytes,
        details,
    })
}

/// Render at the style's physical size. Large PNG files use a bounded resolution.
pub fn drawing(doc: &Document, format: &str) -> Result<Vec<u8>, String> {
    Ok(figure(doc, format)?.bytes)
}

/// Include the actual raster dimensions and resolution for the export receipt.
pub fn figure(doc: &Document, format: &str) -> Result<Figure, String> {
    with_attachment_notice(
        doc,
        render_drawing(
            doc,
            format,
            false,
            Background::Canvas,
            RasterBudget::file(crate::style::DEFAULT.png_dpi, FILE_PIXELS),
            usize::MAX,
            None,
        ),
    )
}

/// [`figure`] with PNG resolution chosen to fit `max_pixels`, capped at
/// [`FILE_PIXELS`].
pub fn figure_with_budget(doc: &Document, format: &str, max_pixels: u64) -> Result<Figure, String> {
    figure_with_budget_and_drawing(
        doc,
        format,
        Background::Canvas,
        RasterBudget::file(crate::style::DEFAULT.png_dpi, max_pixels),
        None,
    )
}

fn figure_with_budget_and_drawing(
    doc: &Document,
    format: &str,
    background: Background,
    budget: RasterBudget,
    drawing: Option<&[u8]>,
) -> Result<Figure, String> {
    with_attachment_notice(
        doc,
        render_drawing(doc, format, false, background, budget, usize::MAX, drawing),
    )
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
    clipboard_figure_with_drawing(
        doc,
        format,
        Background::Transparent,
        crate::style::DEFAULT.png_dpi,
        None,
    )
}

/// Render a clipboard figure, optionally carrying an already prepared drawing.
/// PNG uses `dpi`; on Windows a lower one when the picture would not fit the
/// clipboard.
pub fn clipboard_figure_with_drawing(
    doc: &Document,
    format: &str,
    background: Background,
    dpi: u32,
    drawing: Option<&[u8]>,
) -> Result<Figure, String> {
    with_attachment_notice(
        doc,
        render_drawing(
            doc,
            format,
            true,
            background,
            RasterBudget::clipboard(cfg!(windows), dpi),
            usize::MAX,
            drawing,
        ),
    )
}

/// Office's preview of an embedded drawing, rebuilt after it is edited, on
/// the copy background and resolution set at the time of the rebuild.
#[cfg(windows)]
pub fn office_preview(
    doc: &Document,
    background: Background,
    dpi: u32,
) -> Result<reshiki_windows::OfficePreview, String> {
    Ok(reshiki_windows::OfficePreview {
        png: clipboard_figure_with_drawing(doc, "png", background, dpi, None)?.bytes,
        metafile: crate::native_windows::office_metafile(doc, background)?,
    })
}

fn with_attachment_notice(
    doc: &Document,
    result: Result<Figure, String>,
) -> Result<Figure, String> {
    let mut figure = result?;
    if let Some(notice) = crate::arrow_anchors::export_notice(doc) {
        figure.detail = Some(
            figure
                .detail
                .map_or_else(|| notice.clone(), |detail| format!("{detail} · {notice}")),
        );
    }
    Ok(figure)
}

/// The refusal of a figure that is certain to be larger than `max_bytes`.
pub const OVER_BYTES: &str = "The figure is larger than the output limit";

/// `budget` sets the PNG resolution and its limits. An SVG with outlined text
/// that cannot fit `max_bytes` is refused before it is written; the caller
/// still checks the size of the figure it gets.
fn render_drawing(
    doc: &Document,
    format: &str,
    #[cfg_attr(
        not(windows),
        expect(unused_variables, reason = "only EMF tells them apart")
    )]
    clipboard: bool,
    background: Background,
    budget: RasterBudget,
    max_bytes: usize,
    drawing: Option<&[u8]>,
) -> Result<Figure, String> {
    doc.validate()?;
    let (doc, paper) = background.apply(doc);
    let doc = doc.as_ref();
    let primitives = scene::primitives(doc);
    if format == "png" {
        png_fits(scene::bounds(&primitives), budget)?;
    }
    #[cfg(test)]
    SCENES.with(|scenes| scenes.set(scenes.get() + 1));
    let svg = scene::svg_of(doc, primitives, paper);
    if format == "svg" {
        let mut bytes = svg.into_bytes();
        if let Some(drawing) = drawing {
            bytes = crate::payload::embed(format, bytes, drawing)?;
        }
        return Ok(Figure {
            bytes,
            detail: None,
        });
    }
    let tree = parse_svg(svg)?;
    let mut figure = match format {
        "svg-office" => {
            // Before the outlines are written, which costs as much as parsing.
            if outlined_floor(tree.root()) > max_bytes {
                return Err(OVER_BYTES.into());
            }
            office_svg(&tree).map(|bytes| Figure {
                bytes,
                detail: None,
            })
        }
        #[cfg(windows)]
        "emf" => if clipboard {
            crate::native_windows::metafile(&tree, doc)
        } else {
            crate::native_windows::file_metafile(&tree, doc)
        }
        .map(|bytes| Figure {
            bytes,
            detail: None,
        }),
        "pdf" => single_pdf(&tree, drawing).map(|bytes| Figure {
            bytes,
            detail: None,
        }),
        "png" => {
            let (width, height, dpi) =
                png_dimensions(tree.size().width(), tree.size().height(), budget)?;
            let pixmap = raster(&tree, paper, dpi as f32, (width, height), (0., 0.))?;
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
                    .write_image_data(&straight_alpha(pixmap))
                    .map_err(|e| e.to_string())?;
            }
            Ok(Figure {
                bytes,
                detail: Some(format!("PNG: {width} × {height} pixels at {dpi} dpi")),
            })
        }
        _ => Err("Unsupported drawing export".into()),
    }?;
    if let Some(drawing) = drawing
        && matches!(format, "png" | "svg-office")
    {
        figure.bytes = crate::payload::embed(format, figure.bytes, drawing)?;
    }
    Ok(figure)
}

/// The PNG export's pixels at `dpi`: `size` pixels from pixel `origin` of
/// the whole picture.
fn raster(
    tree: &resvg::usvg::Tree,
    paper: Option<[u8; 3]>,
    dpi: f32,
    size: (u32, u32),
    origin: (f32, f32),
) -> Result<resvg::tiny_skia::Pixmap, String> {
    let scale = dpi / 96.0;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size.0, size.1).ok_or("Could not allocate image")?;
    if let Some([r, g, b]) = paper {
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, 255));
    }
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_translate(-origin.0, -origin.1).pre_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// The pixmap's RGBA bytes with straight alpha.
fn straight_alpha(mut pixmap: resvg::tiny_skia::Pixmap) -> Vec<u8> {
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
    pixmap.take()
}

/// Close-ups of a drawing's PNG export at several resolutions.
pub struct CloseUps {
    /// The whole picture's size in CSS pixels, 96 to the inch.
    pub size: (f32, f32),
    /// For each resolution: the close-up's side in pixels and its RGBA pixels.
    pub pixels: Vec<(u32, Vec<u8>)>,
}

/// The PNG export of `doc` at each of `dpis`, cut to a square `side` points
/// across around `focus`, a point of the drawing: the same pixels at each
/// setting as the exported file has there.
pub fn png_close_ups(
    doc: &Document,
    background: Background,
    focus: crate::document::Point,
    side: f32,
    dpis: &[u32],
) -> Result<CloseUps, String> {
    doc.validate()?;
    let (doc, paper) = background.apply(doc);
    let doc = doc.as_ref();
    let tree = parse_svg(scene::svg_on(doc, paper))?;
    let (lo, _) = scene::bounds(&scene::primitives(doc));
    // Inches per drawing unit.
    let scale = crate::style::DEFAULT.points_per_world() / 72.;
    let side = side / 72.;
    let corner = (
        (focus.x - lo.x) * scale - side / 2.,
        (focus.y - lo.y) * scale - side / 2.,
    );
    let pixels = dpis
        .iter()
        .map(|&dpi| {
            let dpi_f = dpi as f32;
            let pixels = (side * dpi_f).round() as u32;
            // Whole pixels, so the close-up shows the file's own pixels.
            let origin = ((corner.0 * dpi_f).round(), (corner.1 * dpi_f).round());
            let pixmap = raster(&tree, paper, dpi_f, (pixels, pixels), origin)?;
            Ok((pixels, straight_alpha(pixmap)))
        })
        .collect::<Result<_, String>>()?;
    Ok(CloseUps {
        size: (tree.size().width(), tree.size().height()),
        pixels,
    })
}

/// A whole PNG export, fitted to a box.
pub struct Fitted {
    /// The whole picture's size in CSS pixels, 96 to the inch.
    pub size: (f32, f32),
    /// Its width and height in pixels, within the box.
    pub pixels: (u32, u32),
    pub rgba: Vec<u8>,
}

/// The whole PNG export of `doc` on `background`, scaled to fit `fit`
/// pixels, but at no more than `dpi`.
pub fn png_preview(
    doc: &Document,
    background: Background,
    fit: (u32, u32),
    dpi: Option<u32>,
) -> Result<Fitted, String> {
    doc.validate()?;
    let (doc, paper) = background.apply(doc);
    let tree = parse_svg(scene::svg_on(doc.as_ref(), paper))?;
    let (width, height) = (tree.size().width(), tree.size().height());
    let fitted = 96. * (fit.0 as f32 / width).min(fit.1 as f32 / height);
    let dpi = dpi.map_or(fitted, |dpi| fitted.min(dpi as f32));
    let side = |css: f32, fit: u32| ((css * dpi / 96.).ceil() as u32).clamp(1, fit);
    let size = (side(width, fit.0), side(height, fit.1));
    let pixmap = raster(&tree, paper, dpi, size, (0., 0.))?;
    Ok(Fitted {
        size: (width, height),
        pixels: size,
        rgba: straight_alpha(pixmap),
    })
}

/// An exported PNG file's width, height and resolution at `dpi`, for a
/// picture `size` CSS pixels across. A large one is written at a lower
/// resolution.
pub fn png_file_size(size: (f32, f32), dpi: u32) -> Result<(u32, u32, u32), String> {
    png_dimensions(size.0, size.1, RasterBudget::file(dpi, FILE_PIXELS))
}

/// A PNG export's width and height in pixels at `dpi`, for a picture
/// `size` CSS pixels across.
fn png_pixels(size: (f32, f32), dpi: u32) -> (u32, u32) {
    let scale = dpi as f32 / 96.;
    (
        (size.0 * scale).ceil() as u32,
        (size.1 * scale).ceil() as u32,
    )
}

/// Embed the SVG as a unit-square XObject scaled to its physical point size.
fn single_pdf(tree: &resvg::usvg::Tree, drawing: Option<&[u8]>) -> Result<Vec<u8>, String> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
    use std::collections::HashMap;
    let (chunk, root) = svg2pdf::to_chunk(tree, svg2pdf::ConversionOptions::default())
        .map_err(|error| error.to_string())?;
    let mut next = Ref::new(1);
    let catalog = next.bump();
    let pages = next.bump();
    let page = next.bump();
    let content_id = next.bump();
    let metadata = drawing.map(|_| next.bump());
    let mut mapping = HashMap::new();
    let chunk = chunk.renumber(|old| *mapping.entry(old).or_insert_with(|| next.bump()));
    let root = mapping
        .get(&root)
        .copied()
        .ok_or("Could not embed the drawing in the PDF.")?;
    let mut pdf = Pdf::new();
    {
        let mut catalog = pdf.catalog(catalog);
        catalog.pages(pages);
        if let Some(metadata) = metadata {
            catalog.metadata(metadata);
        }
    }
    pdf.pages(pages).kids([page]).count(1);
    // usvg resolves CSS physical units at 96 px/in; PDF uses 72 pt/in.
    let width = tree.size().width() * 0.75;
    let height = tree.size().height() * 0.75;
    let name = Name(b"Drawing");
    let mut page = pdf.page(page);
    page.parent(pages)
        .media_box(Rect::new(0., 0., width, height))
        .contents(content_id);
    // Blend translucent ink in RGB, as svg2pdf's own pages do.
    page.group()
        .transparency()
        .isolated(true)
        .knockout(false)
        .color_space()
        .device_rgb();
    page.resources().x_objects().pair(name, root);
    page.finish();
    let mut content = Content::new();
    content
        .save_state()
        .transform([width, 0., 0., height, 0., 0.])
        .x_object(name)
        .restore_state();
    pdf.stream(content_id, &content.finish());
    if let (Some(metadata), Some(drawing)) = (metadata, drawing) {
        pdf.metadata(metadata, crate::payload::xmp(drawing).as_bytes());
    }
    pdf.extend(&chunk);
    Ok(pdf.finish())
}

/// The pixel budget of a file-export PNG.
pub const FILE_PIXELS: u64 = 80_000_000;
/// The longest side of an exported PNG. Many image readers refuse a longer
/// one, and a single line of text can be that long within the pixel budget.
pub const FILE_SIDE: u32 = 32_767;

#[derive(Clone, Copy)]
struct RasterBudget {
    /// The resolution to use when the picture fits.
    dpi: u32,
    /// Whether a lower resolution may be used instead.
    adaptive: bool,
    pixels: u64,
    side: u32,
}
impl RasterBudget {
    fn file(dpi: u32, pixels: u64) -> Self {
        Self {
            dpi,
            adaptive: true,
            pixels: pixels.min(FILE_PIXELS),
            side: FILE_SIDE,
        }
    }

    fn clipboard(windows: bool, dpi: u32) -> Self {
        if windows {
            // RGBA Office previews and CF_DIB must fit the native 64 MiB
            // limit. Picture-only paste uses the same pixel budget.
            Self {
                dpi,
                adaptive: true,
                pixels: crate::pictures::MAX_PIXELS,
                side: 8192,
            }
        } else {
            Self {
                adaptive: false,
                ..Self::file(dpi, FILE_PIXELS)
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Scenes written as SVG for a figure on this thread.
    static SCENES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Refuse a PNG of a scene with these bounds that cannot fit `budget`. The
/// bounds of a scene are the size of its figure, so this is exact before the
/// scene is written and parsed.
fn png_fits(
    (lo, hi): (crate::document::Point, crate::document::Point),
    budget: RasterBudget,
) -> Result<(), String> {
    // The SVG gives its size in points, and the parser turns points into CSS
    // pixels. Round in the same order so the size matches to the pixel.
    let pixels = |span: f32| (span * crate::style::DEFAULT.points_per_world() * 96. / 72.).max(1.);
    png_dimensions(pixels(hi.x - lo.x), pixels(hi.y - lo.y), budget).map(|_| ())
}

/// At least this many bytes of path data are written for `group` with its
/// text outlined. Each segment is a letter and its numbers, each followed by
/// a space, and a number has at least the digits of its whole part.
fn outlined_floor(group: &resvg::usvg::Group) -> usize {
    use resvg::{tiny_skia::PathSegment, usvg::Node};
    let point = |p: resvg::tiny_skia::Point| {
        [p.x, p.y]
            .map(|v| 2 + (v.abs() as u32).checked_ilog10().unwrap_or(0) as usize)
            .iter()
            .sum::<usize>()
    };
    group
        .children()
        .iter()
        .map(|node| match node {
            Node::Group(group) => outlined_floor(group),
            Node::Text(text) => outlined_floor(text.flattened()),
            Node::Path(path) => path
                .data()
                .segments()
                .map(|segment| {
                    2 + match segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => point(p),
                        PathSegment::QuadTo(a, p) => point(a) + point(p),
                        PathSegment::CubicTo(a, b, p) => point(a) + point(b) + point(p),
                        PathSegment::Close => 0,
                    }
                })
                .sum(),
            Node::Image(_) => 0,
        })
        .sum()
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
    let lower = [600, 300, 150, 96, 72]
        .into_iter()
        .filter(|&dpi| budget.adaptive && dpi < budget.dpi);
    for dpi in std::iter::once(budget.dpi).chain(lower) {
        let (w, h) = png_pixels((width, height), dpi);
        if w <= budget.side && h <= budget.side && u64::from(w) * u64::from(h) <= budget.pixels {
            return Ok((w, h, dpi));
        }
    }
    Err(if budget.adaptive {
        "Drawing is too large for a PNG even at 72 dpi; use SVG or PDF.".into()
    } else {
        format!(
            "Drawing is too large for a {} dpi PNG; use SVG or PDF.",
            budget.dpi
        )
    })
}
/// Export all physical sheets without scaling the drawing. Page gaps and margin
/// guides belong to the editor only; marks beyond a sheet edge are clipped.
pub fn pages_pdf(doc: &Document, background: Background) -> Result<Vec<u8>, String> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
    use std::collections::HashMap;
    doc.validate()?;
    let (doc, paper) = background.apply(doc);
    let doc = doc.as_ref();
    let layout = doc
        .page_layout
        .as_ref()
        .ok_or("Set up publication pages before exporting a page PDF.")?;
    let svg = scene::svg_on(doc, paper);
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
        if let Some(paper) = paper {
            let [r, g, b] = paper.map(|c| f32::from(c) / 255.);
            content
                .set_fill_rgb(r, g, b)
                .rect(0., 0., layout.width_pt, layout.height_pt)
                .fill_nonzero();
        }
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
