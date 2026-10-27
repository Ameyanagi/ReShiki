use reshiki_io::{engine::LocalEngine, export, formats};
use reshiki_model::{document::Document, scene, style::DrawingStyle};

use crate::styles;

pub struct Options<'a> {
    style: Option<&'a str>,
    width: Option<u32>,
    height: Option<u32>,
    dpi: Option<f64>,
}

impl<'a> Options<'a> {
    pub fn new(
        style: Option<&'a str>,
        width: Option<u32>,
        height: Option<u32>,
        dpi: Option<f64>,
    ) -> Result<Self, String> {
        if width == Some(0) || height == Some(0) {
            return Err("Image dimensions must be positive".into());
        }
        if dpi.is_some_and(|dpi| !dpi.is_finite() || dpi <= 0.) {
            return Err("dpi must be a positive finite number".into());
        }
        if let Some(style) = style {
            styles::merge(&DrawingStyle::default(), style)?;
        }
        Ok(Self {
            style,
            width,
            height,
            dpi,
        })
    }

    fn fit(&self, width: f64, height: f64) -> Option<f64> {
        match (self.width, self.height) {
            (Some(w), Some(h)) => Some((f64::from(w) / width).min(f64::from(h) / height)),
            (Some(w), None) => Some(f64::from(w) / width),
            (None, Some(h)) => Some(f64::from(h) / height),
            (None, None) => None,
        }
    }
}

pub async fn document(
    doc: Document,
    format: &str,
    options: &Options<'_>,
) -> Result<Vec<u8>, String> {
    let doc = styles::apply(doc, options.style)?;
    let engine = LocalEngine::in_process();
    let format = if format == "smiles" && crate::drawing::kind(&doc) == "reaction" {
        "rsmi"
    } else {
        format
    };
    if !matches!(format, "png" | "svg" | "pdf") {
        if format == "sdf" {
            let mut bytes = formats::write_with_engine(doc, "mol", &engine).await?;
            bytes.extend_from_slice(b"\n$$$$\n");
            return Ok(bytes);
        }
        return formats::write_with_engine(doc, format, &engine).await;
    }
    let (doc, _) = export::figure_document(&engine, doc).await?;
    let svg = scene::svg_with_background(&doc);
    // Keep SVG text and physical style dimensions intact unless a box was requested.
    if format == "svg" && options.width.is_none() && options.height.is_none() {
        return Ok(svg.into_bytes());
    }
    let tree = export::parse_svg(svg.clone())?;
    let width = f64::from(tree.size().width());
    let height = f64::from(tree.size().height());
    let fit = options.fit(width, height);
    if matches!(format, "svg" | "pdf") {
        let svg = if let Some(scale) = fit {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {width} {height}\">{svg}</svg>",
                width * scale,
                height * scale
            )
        } else {
            svg
        };
        if format == "svg" {
            return Ok(svg.into_bytes());
        }
        return svg2pdf::to_pdf(
            &export::parse_svg(svg)?,
            svg2pdf::ConversionOptions::default(),
            svg2pdf::PageOptions { dpi: 96. },
        )
        .map_err(|error| error.to_string());
    }
    let dpi = options.dpi.unwrap_or(f64::from(doc.drawing_style.png_dpi));
    let scale = fit.unwrap_or(dpi / 96.);
    let w = ((width * scale).ceil().max(1.) as u32).min(options.width.unwrap_or(u32::MAX));
    let h = ((height * scale).ceil().max(1.) as u32).min(options.height.unwrap_or(u32::MAX));
    if u64::from(w) * u64::from(h) > formats::EXPORT_PNG_PIXELS {
        return Err(
            "PNG exceeds the 16 million pixel budget; reduce dpi or fit width/height".into(),
        );
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("Could not allocate image")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pixmap.as_mut(),
    );
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: (dpi / 0.0254).round() as u32,
            yppu: (dpi / 0.0254).round() as u32,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        // resvg uses premultiplied alpha; PNG stores straight alpha.
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
            .map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}
