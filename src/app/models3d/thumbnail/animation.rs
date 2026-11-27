//! Worker-owned movie frames through the model thumbnail/export SVG renderer.

use reshiki::document::{Document, Point};
use reshiki::models3d::{self, Display};
use reshiki::palette::{Color, Hue, Palette, Row};
use resvg::tiny_skia;
use std::fmt::Write;

/// Plot coordinates have already been converted to the selected display units.
/// Readout and direction use the same formatted strings as live playback.
pub(in crate::app) struct AnimationPlot<'a> {
    pub points: &'a [[f64; 2]],
    pub marker: [f64; 2],
    pub x_label: &'a str,
    pub y_label: &'a str,
    pub readout: &'a str,
    pub direction: &'a str,
}

/// No canvas handles or UI caches cross the worker boundary. One document copy
/// keeps the selected model's camera, style, atom colors and source identities.
pub(in crate::app) struct AnimationRenderer {
    doc: Document,
    side: u32,
    transparent: bool,
    bounds: (Point, Point),
    size: f32,
    reach: f32,
}

impl AnimationRenderer {
    pub(in crate::app) fn new(
        doc: &Document,
        model_index: usize,
        side: u32,
        transparent: bool,
    ) -> Result<Self, String> {
        if ![480, 720, 1080].contains(&side) {
            return Err("Choose a movie size of 480, 720 or 1080 pixels".into());
        }
        let mut model = doc
            .models
            .get(model_index)
            .cloned()
            .ok_or("The animated 3D model is no longer available")?;
        model.display = Display::ThreeD;
        model.morph = None;
        model.turning = false;
        model.overlay.clear();
        // Orbital surfaces and panels belong to the saved geometry.
        model.surface = None;
        model.panels.clear();
        model.background = models3d::Background::None;
        let mut doc = if transparent {
            doc.clone()
        } else {
            reshiki::export::Background::Color([255; 3])
                .apply(doc)
                .0
                .into_owned()
        };
        doc.models = vec![model];
        let model = doc.models.first().ok_or("Missing animated 3D model")?;
        let points = model.points(&doc).ok_or("The 3D model has missing atoms")?;
        if points.is_empty() || points.iter().flat_map(|(_, p)| p).any(|v| !v.is_finite()) {
            return Err("The 3D model has invalid coordinates".into());
        }
        let size = model.size;
        let reach = model.reach(&doc);
        let bounds = models3d::frame(&doc, model).ok_or("The 3D model has no frame")?;
        if !size.is_finite() || size <= 0. || !reach.is_finite() || reach <= 0. {
            return Err("The 3D model has an invalid size".into());
        }
        Ok(Self {
            doc,
            side,
            transparent,
            bounds,
            size,
            reach,
        })
    }

    /// Reserve one camera envelope before rendering, so moving atoms do not
    /// make the movie zoom in and out or clip its widest phase.
    pub(in crate::app) fn fit(&mut self, frames: &[Vec<[f32; 3]>]) -> Result<(), String> {
        if frames.is_empty() {
            return Err("The animation has no frames".into());
        }
        for positions in frames {
            self.positions(positions)?;
            let model = self.doc.models.first().ok_or("Missing animated 3D model")?;
            let (lo, hi) = models3d::frame(&self.doc, model).ok_or("The 3D model has no frame")?;
            self.bounds.0.x = self.bounds.0.x.min(lo.x);
            self.bounds.0.y = self.bounds.0.y.min(lo.y);
            self.bounds.1.x = self.bounds.1.x.max(hi.x);
            self.bounds.1.y = self.bounds.1.y.max(hi.y);
        }
        Ok(())
    }

    pub(in crate::app) fn set_irc_topology(
        &mut self,
        numbers: &[u8],
        angstrom: &[f64],
    ) -> Result<(), String> {
        let model = self
            .doc
            .models
            .first_mut()
            .ok_or("Missing animated 3D model")?;
        crate::app::animation::set_irc_topology(model, numbers, angstrom.iter().copied())
    }

    /// Exactly `side × side` straight-alpha RGBA pixels, ready for an encoder.
    /// Positions are in Å, in model-atom order followed by added hydrogens.
    pub(in crate::app) fn render(
        &mut self,
        positions: &[[f32; 3]],
        plot: Option<&AnimationPlot<'_>>,
    ) -> Result<Vec<u8>, String> {
        self.positions(positions)?;
        let artwork = models3d::primitives(&self.doc);
        if artwork.is_empty() {
            return Err("The animated 3D model has no visible artwork".into());
        }
        let (lo, hi) = self.bounds;
        let span = (hi.x - lo.x).max(hi.y - lo.y);
        if !span.is_finite() || span <= 0. {
            return Err("The animated 3D model has an invalid frame".into());
        }
        let padding = span * 0.035;
        let (lo, hi) = (lo.offset(-padding, -padding), hi.offset(padding, padding));
        // This is the same primitive → SVG → resvg path as ModelImage tiles.
        let model_svg = reshiki::scene::svg_part_on(
            &artwork,
            lo,
            hi,
            self.doc.canvas_theme,
            (!self.transparent).then_some([255; 3]),
        );
        let (_, body) = model_svg.split_once('>').ok_or("Invalid model SVG")?;
        let body = body.strip_suffix("</svg>").ok_or("Invalid model SVG")?;
        let mut svg = String::from(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"720\" height=\"720\" viewBox=\"0 0 720 720\">",
        );
        let (top, height) = if plot.is_some() {
            (20., 412.)
        } else {
            (24., 672.)
        };
        let _ = write!(
            svg,
            "<svg x=\"24\" y=\"{top}\" width=\"672\" height=\"{height}\" viewBox=\"{} {} {} {}\">{body}</svg>",
            lo.x,
            lo.y,
            hi.x - lo.x,
            hi.y - lo.y
        );
        if let Some(plot) = plot {
            plot_svg(
                &mut svg,
                plot,
                Palette::in_mode(&self.doc, self.doc.canvas_theme),
            )?;
        }
        svg.push_str("</svg>");
        raster_animation_frame(svg, self.side, 720., self.transparent)
    }

    fn positions(&mut self, positions: &[[f32; 3]]) -> Result<(), String> {
        let model = self
            .doc
            .models
            .first_mut()
            .ok_or("Missing animated 3D model")?;
        if positions.len() != model.atoms.len() + model.hydrogens.len()
            || positions.iter().flatten().any(|value| !value.is_finite())
        {
            return Err("The animation coordinates do not match the 3D model".into());
        }
        for (target, &position) in model
            .atoms
            .iter_mut()
            .map(|a| &mut a.xyz)
            .chain(model.hydrogens.iter_mut().map(|h| &mut h.xyz))
            .zip(positions)
        {
            *target = position;
        }
        let model = self.doc.models.first().ok_or("Missing animated 3D model")?;
        let reach = model.reach(&self.doc);
        if !reach.is_finite() || reach <= 0. {
            return Err("The animated 3D model has an invalid size".into());
        }
        let model = self
            .doc
            .models
            .first_mut()
            .ok_or("Missing animated 3D model")?;
        model.size = self.size * reach / self.reach;
        Ok(())
    }
}

/// Render a square SVG into straight-alpha RGBA for a movie encoder.
pub(in crate::app) fn raster_animation_frame(
    svg: String,
    side: u32,
    svg_side: f32,
    transparent: bool,
) -> Result<Vec<u8>, String> {
    let tree = reshiki::export::parse_svg(svg)?;
    let mut pixels =
        tiny_skia::Pixmap::new(side, side).ok_or("Could not allocate an animation frame")?;
    if !transparent {
        pixels.fill(tiny_skia::Color::WHITE);
    }
    let scale = side as f32 / svg_side;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    Ok(reshiki::export::straight_alpha(pixels))
}

#[allow(clippy::indexing_slicing)] // Each coordinate has both axes, selected only by [0, 1].
fn plot_svg(svg: &mut String, plot: &AnimationPlot<'_>, palette: Palette) -> Result<(), String> {
    if plot.points.is_empty()
        || plot
            .points
            .iter()
            .flatten()
            .chain(&plot.marker)
            .any(|v| !v.is_finite())
    {
        return Err("The animation energy plot has invalid coordinates".into());
    }
    let ranges = [0, 1].map(|axis| {
        let mut lo = plot
            .points
            .iter()
            .map(|p| p[axis])
            .fold(f64::INFINITY, f64::min);
        let mut hi = plot
            .points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        lo = lo.min(plot.marker[axis]);
        hi = hi.max(plot.marker[axis]);
        if axis == 1 {
            lo = lo.min(0.);
            hi = hi.max(0.);
        }
        let padding = if hi > lo {
            (hi - lo) * 0.06
        } else {
            lo.abs().max(1.) * 0.06
        };
        [lo - padding, hi + padding]
    });
    if ranges.iter().flatten().any(|v| !v.is_finite()) {
        return Err("The animation energy plot has an invalid range".into());
    }
    let [(xmin, xmax, x_ticks), (ymin, ymax, y_ticks)] =
        ranges.map(|[lo, hi]| crate::app::energy_plot::scale::nice_scale(lo, hi));
    if ![(xmax - xmin), (ymax - ymin)]
        .into_iter()
        .all(|span| span.is_finite() && span > 0.)
    {
        return Err("The animation energy plot has an invalid range".into());
    }
    let project = |[x, y]: [f64; 2]| {
        [
            88. + (x - xmin) / (xmax - xmin) * 588.,
            638. - (y - ymin) / (ymax - ymin) * 142.,
        ]
    };
    let ink = rgb(palette.rgb(Color::Ink));
    let curve = rgb(palette.rgb(Color::Palette(Hue::Blue, Row::Strong)));
    let marker = rgb(palette.rgb(Color::Palette(Hue::Red, Row::Strong)));
    let readout_width = reshiki::style::styled_text_width(plot.readout, 18., &Default::default());
    let readout_size = if readout_width > 652. {
        18. * 652. / readout_width
    } else {
        18.
    };
    text(svg, 34., 456., readout_size, &ink, plot.readout, "start");
    text(svg, 34., 480., 15., &ink, plot.direction, "start");
    for y in y_ticks {
        let [_, py] = project([xmin, y]);
        let _ = write!(
            svg,
            "<path d=\"M88 {py}H676\" fill=\"none\" stroke=\"{ink}\" stroke-opacity=\"0.14\"/>"
        );
        text(
            svg,
            78.,
            py + 4.,
            13.,
            &ink,
            &tick_label(y, ymax - ymin),
            "end",
        );
    }
    for x in x_ticks {
        let [px, _] = project([x, ymin]);
        let _ = write!(svg, "<path d=\"M{px} 638v4\" stroke=\"{ink}\"/>");
        text(
            svg,
            px,
            658.,
            13.,
            &ink,
            &tick_label(x, xmax - xmin),
            "middle",
        );
    }
    let _ = write!(
        svg,
        "<path d=\"M88 496V638H676\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"1.4\"/><polyline fill=\"none\" stroke=\"{curve}\" stroke-width=\"3\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\""
    );
    for &point in plot.points {
        let [x, y] = project(point);
        let _ = write!(svg, "{x},{y} ");
    }
    svg.push_str("\"/>");
    let [x, y] = project(plot.marker);
    let _ = write!(
        svg,
        "<path d=\"M{x} 496V638\" fill=\"none\" stroke=\"{marker}\" stroke-opacity=\"0.5\" stroke-dasharray=\"4 4\"/><circle cx=\"{x}\" cy=\"{y}\" r=\"6\" fill=\"{marker}\" stroke=\"white\" stroke-width=\"2\"/>"
    );
    text(svg, 382., 690., 16., &ink, plot.x_label, "middle");
    svg.push_str("<g transform=\"translate(24 567) rotate(-90)\">");
    text(svg, 0., 0., 16., &ink, plot.y_label, "middle");
    svg.push_str("</g>");
    Ok(())
}

fn text(svg: &mut String, x: f64, y: f64, size: f32, color: &str, value: &str, anchor: &str) {
    let value = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let _ = write!(
        svg,
        "<text x=\"{x}\" y=\"{y}\" font-family=\"Arial\" font-size=\"{size}\" text-anchor=\"{anchor}\" fill=\"{color}\">{value}</text>"
    );
}

fn rgb([r, g, b]: [u8; 3]) -> String {
    format!("rgb({r},{g},{b})")
}

fn tick_label(value: f64, range: f64) -> String {
    let digits = (1. - (range / 4.).log10().floor()).clamp(0., 6.) as usize;
    let value = if value.abs() < 0.5 * 10_f64.powi(-(digits as i32)) {
        0.
    } else {
        value
    };
    let label = format!("{value:.digits$}");
    if digits == 0 {
        label
    } else {
        label.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

#[cfg(test)]
#[path = "animation/tests.rs"]
mod tests;
