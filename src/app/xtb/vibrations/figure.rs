//! An infrared spectrum built from editable drawing objects.
use super::view::{SpectrumData, WAVENUMBER_TITLE};
use reshiki::document::{Annotation, Document, Point};
use reshiki::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand};
use reshiki::grouping::Group;
use reshiki::qcdata::vibrational::BandShape;
use reshiki::typography::{TextFormat, layout};

const LABEL_INTERVAL_CM1: f64 = 1000.;

#[derive(Debug, PartialEq)]
pub(super) struct Figure {
    pub(super) graphics: Vec<Graphic>,
    pub(super) annotations: Vec<Annotation>,
    pub(super) group: Group,
}

impl Figure {
    pub(super) fn add_to(self, doc: &mut Document) {
        doc.graphics.extend(self.graphics);
        doc.annotations.extend(self.annotations);
        doc.groups.push(self.group);
    }
}

/// Build from a plottable spectrum, with `at` at the top left including labels and strokes.
pub(super) fn spectrum(doc: &Document, spectrum: &SpectrumData, method: &str, at: Point) -> Figure {
    let drawing = &doc.drawing_style;
    let bond = drawing.bond_length_world;
    let width = 9. * bond;
    let height = 3. * bond;
    let gap = 0.12 * bond;
    let tick_length = 0.16 * bond;
    let first_id = doc.next_id();
    let mut next_id = first_id;
    let mut id = || {
        let id = next_id;
        next_id = next_id.saturating_add(1);
        id
    };
    let format = TextFormat {
        style: drawing.text_style(),
        ..Default::default()
    };
    let mut caption_format = format.clone();
    caption_format.style.size_pt = (0.8 * caption_format.style.size_pt).max(4.);
    let shape = match spectrum.shape {
        BandShape::Lorentzian => "Lorentzian",
        BandShape::Gaussian => "Gaussian",
    };
    let scale = spectrum.scale;
    let scale = if (scale * 100.).round() / 100. == scale {
        format!("{scale:.2}")
    } else {
        scale.to_string()
    };
    let caption = Annotation {
        id: id(),
        position: Point::default(),
        text: format!(
            "{method} · scale {scale} · FWHM {:.0} cm⁻¹ · {shape}",
            spectrum.fwhm
        ),
        format: caption_format,
        angle: 0.,
    };
    // Leave space for the peak labels between the tallest peak and the caption.
    let base = caption.size().1 + drawing.line_height() + 2. * gap + height;
    let x = |wavenumber| spectrum.x(wavenumber) * width;
    let top = spectrum
        .curve
        .iter()
        .chain(&spectrum.peaks)
        .map(|p| p[1])
        .fold(0., f64::max);
    let y = |intensity| base - (intensity / top) as f32 * height;
    let style = GraphicStyle {
        width_pt: drawing.line_width_pt,
        ..Default::default()
    };

    // Keep every sampled point as well as the refined maxima used for peak labels.
    let mut points = spectrum.peaks.clone();
    points.extend_from_slice(&spectrum.curve);
    points.sort_by(|a, b| b[0].total_cmp(&a[0]));
    points.dedup_by(|a, b| a[0] == b[0]);
    let point = |[wavenumber, intensity]: [f64; 2]| Point::new(x(wavenumber), y(intensity));
    let start = points.first().copied().map_or(Point::new(0., base), point);
    let end = points
        .last()
        .copied()
        .map_or(Point::new(width, base), point);
    let mut curve = Graphic::pen_curve(id(), start, end, style.clone());
    curve.path = std::iter::once(PathCommand::Move(start))
        .chain(points.into_iter().skip(1).map(point).map(PathCommand::Line))
        .collect();
    curve.node_types.clear();

    let line = |id, start, end| {
        Graphic::dragged(
            id,
            GraphicKind::Line,
            start,
            end,
            style.clone(),
            BracketSides::Both,
            false,
        )
    };
    let mut graphics = vec![
        curve,
        line(id(), Point::new(0., base), Point::new(width, base)),
    ];
    let mut annotations = vec![caption];
    let mut label = |id, text, center: Point, above| {
        let mut annotation = Annotation {
            id,
            position: center,
            text,
            format: format.clone(),
            angle: 0.,
        };
        let (w, h) = annotation.size();
        annotation.position = center.offset(-w / 2., if above { -h } else { 0. });
        annotations.push(annotation);
    };
    let label_top = base + tick_length + gap;
    let ticks = spectrum.ticks();
    for &tick in &ticks {
        let x = x(tick);
        graphics.push(line(
            id(),
            Point::new(x, base),
            Point::new(x, base + tick_length),
        ));
    }
    let tick_labels: Vec<_> = ticks
        .into_iter()
        .filter(|tick| tick % LABEL_INTERVAL_CM1 == 0.)
        .map(|tick| (x(tick), format!("{tick:.0}")))
        .collect();
    let widest_label = tick_labels
        .iter()
        .map(|(_, text)| layout(text, &format).width)
        .fold(0., f32::max);
    let spacing = x(0.) - x(LABEL_INTERVAL_CM1);
    let label_step = ((widest_label + gap) / spacing).ceil().max(1.) as usize;
    for (x, text) in tick_labels.into_iter().step_by(label_step) {
        label(id(), text, Point::new(x, label_top), false);
    }
    label(
        id(),
        WAVENUMBER_TITLE.into(),
        Point::new(width / 2., label_top + drawing.line_height() + gap),
        false,
    );
    for [wavenumber, intensity] in spectrum.labelled_peaks(x, format.style.size(), Vec::new()) {
        label(
            id(),
            format!("{wavenumber:.0}"),
            Point::new(x(wavenumber), y(intensity) - gap),
            true,
        );
    }

    // The first tick label can extend past the start of the axis.
    let left = annotations
        .iter()
        .map(|a| a.position.x)
        .fold(-style.width() / 2., f32::min);
    for graphic in &mut graphics {
        graphic.origin = graphic.origin.offset(at.x - left, at.y);
    }
    for annotation in &mut annotations {
        annotation.position = annotation.position.offset(at.x - left, at.y);
    }
    let group_id = id();
    let group = Group {
        id: group_id,
        members: (first_id..group_id).collect(),
        integral: false,
        layout: None,
    };
    Figure {
        graphics,
        annotations,
        group,
    }
}
