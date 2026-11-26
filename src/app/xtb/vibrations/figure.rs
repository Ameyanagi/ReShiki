//! An infrared spectrum built from editable drawing objects.
use reshiki::document::{Annotation, Document, Point};
use reshiki::graphics::{
    BracketSides, Frame, Graphic, GraphicKind, GraphicStyle, IrFigure, PathCommand, YAxis,
};
use reshiki::grouping::Group;
use reshiki::qcdata::vibrational;
use reshiki::style::DrawingStyle;
use reshiki::typography::{Script, TextFormat, TextSpan, TextStyle, layout};

#[derive(Debug, PartialEq)]
pub(crate) struct Figure {
    pub(crate) graphics: Vec<Graphic>,
    pub(crate) annotations: Vec<Annotation>,
    pub(crate) group: Group,
    pub(crate) labelled_peaks: Vec<u32>,
}

impl Figure {
    pub(crate) fn add_to(self, doc: &mut Document) {
        doc.graphics.extend(self.graphics);
        doc.annotations.extend(self.annotations);
        doc.groups.push(self.group);
    }
}

const VERTICAL: f32 = -90.;
// The ordinary SVG scene extends four points past the marks on each side.
const EXPORT_PAD_PT: f32 = 4.;

fn annotation(id: u64, text: String, format: TextFormat, centre: Point, angle: f32) -> Annotation {
    let (width, height) = {
        let measured = layout(&text, &format);
        (measured.width, measured.height)
    };
    Annotation {
        id,
        position: centre.offset(-width / 2., -height / 2.),
        text,
        format,
        angle,
    }
}

fn ticks(from: f64, to: f64, step: u32) -> impl Iterator<Item = u32> {
    let first = (to.ceil() as u32).div_ceil(step) * step;
    let last = from.floor() as u32 / step * step;
    (first..=last).rev().step_by(step as usize)
}

fn major_step(figure: &IrFigure, drawing: &DrawingStyle, width: f32, format: &TextFormat) -> u32 {
    // Even with its axis margins, a one-column figure has room for eight ticks.
    let limit = (8. * width / drawing.world(234.)).max(8.).floor() as usize;
    [100, 200, 500, 1000, 2000]
        .into_iter()
        .find(|&step| {
            let widths: Vec<_> = ticks(figure.from, figure.to, step)
                .map(|tick| layout(&tick.to_string(), format).width)
                .collect();
            let spacing = width * (f64::from(step) / (figure.from - figure.to)) as f32;
            widths.len() <= limit
                && widths
                    .iter()
                    .zip(widths.iter().skip(1))
                    .all(|(a, b)| (a + b) / 2. + drawing.world(2.) <= spacing)
        })
        .unwrap_or(2000)
}

fn wavenumber_format(text: &str, format: &TextFormat) -> TextFormat {
    let mut format = format.clone();
    if let Some(unit) = text.rfind("cm−1") {
        format.spans.push(TextSpan {
            start: unit + "cm".len(),
            end: unit + "cm−1".len(),
            style: TextStyle {
                script: Script::Superscript,
                ..format.style.clone()
            },
        });
    }
    format
}

fn method_text(figure: &IrFigure, format: &TextFormat, width: f32) -> (String, TextFormat) {
    let method = figure.method.lines().next().unwrap_or_default().trim();
    let details = format!(
        " · {:?} · FWHM {} cm−1 · scale {:.3}",
        figure.shape,
        figure.fwhm_text(),
        figure.scale,
    );
    let text = format!("{method}{details}");
    let styled = wavenumber_format(&text, format);
    if layout(&text, &styled).width <= width {
        return (text, styled);
    }
    for prefix in method
        .char_indices()
        .rev()
        .filter_map(|(end, _)| method.get(..end))
    {
        let text = format!("{}…{details}", prefix.trim_end());
        let styled = wavenumber_format(&text, format);
        if layout(&text, &styled).width <= width {
            return (text, styled);
        }
    }
    let text = details.trim_start_matches(" · ");
    (text.into(), wavenumber_format(text, format))
}

// Clip to the label's x-span before comparing y.
pub(super) fn hits_box(a: Point, b: Point, lo: Point, hi: Point) -> bool {
    let (a, b) = if a.x <= b.x { (a, b) } else { (b, a) };
    if a.x > hi.x || b.x < lo.x {
        return false;
    }
    let (first, last) = if a.x == b.x {
        (a.y, b.y)
    } else {
        let y = |x| a.y + (b.y - a.y) * ((x - a.x) / (b.x - a.x));
        (y(lo.x.max(a.x)), y(hi.x.min(b.x)))
    };
    first.min(last) <= hi.y && first.max(last) >= lo.y
}

pub(super) fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let separated = |a: f32, b: f32, c: f32, d: f32| a.max(b) < c.min(d) || c.max(d) < a.min(b);
    if separated(a.x, b.x, c.x, d.x) || separated(a.y, b.y, c.y, d.y) {
        return false;
    }
    let side = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let straddles = |a, b, c, d| side(a, b, c) * side(a, b, d) <= 0.;
    straddles(a, b, c, d) && straddles(c, d, a, b)
}

pub(super) struct PeakLabel {
    pub(super) peak: [f64; 2],
    pub(super) required: bool,
    partner: Option<f64>,
}

// Peaks are ordered by descending wavenumber, as in the drawn curve.
pub(super) fn select_labels(
    peaks: &[[f64; 2]],
    maximum: f64,
    limit: usize,
    drawn: impl Fn(usize) -> bool,
) -> Vec<PeakLabel> {
    let strength = |&(_, [a_number, a_value]): &(usize, [f64; 2]),
                    &(_, [b_number, b_value]): &(usize, [f64; 2])| {
        a_value
            .total_cmp(&b_value)
            .then(a_number.total_cmp(&b_number))
    };
    let bond_regions = [2000. ..2500., 1500. ..2000.];
    let in_bond_region = |number| bond_regions.iter().any(|range| range.contains(&number));
    let visible: Vec<_> = peaks
        .iter()
        .copied()
        .enumerate()
        .map(|(i, peak)| drawn(i).then_some((i, peak)))
        .collect();
    let candidates = || visible.iter().copied().flatten();
    let stretches: Vec<_> = candidates()
        .filter(|&(_, [number, value])| {
            (2500. ..=4000.).contains(&number) && value >= maximum * 0.01
        })
        .collect();
    let mut reserved: Vec<_> = stretches
        .chunk_by(|&(_, [a, _]), &(_, [b, _])| a - b < 150.)
        .filter_map(|group| group.iter().copied().max_by(strength))
        .collect();
    reserved.sort_unstable_by(|a, b| strength(b, a));
    reserved.truncate(3);
    for range in &bond_regions {
        reserved.extend(
            candidates()
                .filter(|&(_, [number, value])| range.contains(&number) && value >= maximum * 0.02)
                .max_by(strength),
        );
    }
    reserved.sort_unstable_by(|a, b| strength(b, a));
    let mut fill: Vec<_> = candidates()
        .filter(|&(_, [_, value])| value >= maximum * 0.05)
        .collect();
    fill.sort_unstable_by(|a, b| strength(b, a));
    let neighbour = |i: usize, [number, value]: [f64; 2], selected: &[usize]| {
        let distance = |(_, [other, _]): (usize, [f64; 2])| (number - other).abs();
        [i.checked_sub(1), Some(i + 1)]
            .into_iter()
            .flatten()
            .filter_map(|j| visible.get(j).copied().flatten())
            .filter(|(j, _)| !selected.contains(j))
            .filter(|&candidate @ (_, [_, height])| {
                distance(candidate) <= 60. && height >= value * 0.30
            })
            .min_by(|&a, &b| distance(a).total_cmp(&distance(b)).then(strength(&b, &a)))
    };
    let mut selected = Vec::new();
    reserved
        .into_iter()
        .map(|i| (i, true))
        .chain(fill.into_iter().map(|i| (i, false)))
        .flat_map(|((i, peak), reserved)| {
            // A doublet takes consecutive slots at its anchor's priority.
            let pair = if selected.contains(&i) {
                [None, None]
            } else {
                selected.push(i);
                let partner = if reserved && !in_bond_region(peak[0]) {
                    None
                } else {
                    neighbour(i, peak, &selected)
                };
                [
                    Some(PeakLabel {
                        peak,
                        required: reserved || partner.is_some(),
                        partner: partner.map(|(_, [number, _])| number),
                    }),
                    partner.map(|(j, other)| {
                        selected.push(j);
                        PeakLabel {
                            peak: other,
                            required: true,
                            partner: Some(peak[0]),
                        }
                    }),
                ]
            };
            pair.into_iter().flatten()
        })
        .take(limit)
        .collect()
}

fn peak_depth(points: &[[f64; 2]], number: f64) -> f64 {
    let index = points.partition_point(|p| p[0] > number);
    let Some((left, right @ [[_, tip], ..])) = points.get(..=index).zip(points.get(index..)) else {
        return 0.;
    };
    let tip = *tip;
    // Follow each side downhill to the nearest trough, or the plot edge.
    let left = left
        .array_windows::<2>()
        .rev()
        .take_while(|[a, b]| a[1] <= b[1])
        .last()
        .map_or(tip, |[a, _]| a[1]);
    let right = right
        .array_windows::<2>()
        .take_while(|[a, b]| b[1] <= a[1])
        .last()
        .map_or(tip, |[_, b]| b[1]);
    tip - left.max(right)
}

/// Build the drawing with `at` at the top left of the publication box.
pub(crate) fn spectrum(doc: &Document, figure: &IrFigure, at: Point) -> Figure {
    let drawing = &doc.drawing_style;
    let pt = |points| drawing.world(points);
    let (family, label_size, tick_size, line_width) = figure.journal.metrics();
    let text_format = |size_pt| TextFormat {
        style: TextStyle {
            family: family.into(),
            size_pt,
            ..Default::default()
        },
        ..Default::default()
    };
    let tick_format = text_format(tick_size);
    let label_format = text_format(label_size);
    let (width_pt, height_pt) = figure.columns.size_pt();
    let (width, height) = (pt(width_pt), pt(height_pt));
    let pad = pt(EXPORT_PAD_PT);
    let stroke = pt(line_width) / 2.;
    let transmittance = figure.y == YAxis::Transmittance;
    let framed = figure.frame != Frame::Baseline;
    let y_ticks = (0..=5).map(|i| {
        if transmittance {
            (f64::from(i * 20), (i * 20).to_string())
        } else {
            let value = f64::from(i) / 5.;
            (value, format!("{value:.1}"))
        }
    });
    let tick_height = layout("0", &tick_format).height;
    let number_width = layout(&format!("{:.0}", figure.from.ceil()), &tick_format).width;
    let y_title = if transmittance {
        "Transmittance (%)"
    } else {
        "Absorbance (a.u.)"
    };
    let title_height = layout(y_title, &label_format).height;
    let widest_y_tick = y_ticks
        .clone()
        .map(|(_, text)| layout(&text, &tick_format).width)
        .fold(0., f32::max);
    let mut left = if framed {
        pad + title_height + widest_y_tick + pt(7.)
    } else {
        pad + stroke
    };
    let mut right = width - pad - stroke;
    let major = major_step(figure, drawing, right - left - number_width, &tick_format);
    let fraction = |number| ((figure.from - number) / (figure.from - figure.to)) as f32;
    let x_labels: Vec<_> = ticks(figure.from, figure.to, major)
        .map(|tick| {
            let text = tick.to_string();
            (
                fraction(f64::from(tick)),
                layout(&text, &tick_format).width / 2.,
            )
        })
        .collect();
    // A centered endpoint label can reach farther than the curve's stroke.
    // Recompute its overhang after the plot width changes.
    for _ in 0..3 {
        let plot_width = right - left;
        if !framed {
            left = pad
                + x_labels
                    .iter()
                    .map(|&(f, w)| w - f * plot_width)
                    .fold(stroke, f32::max);
        }
        right = width
            - pad
            - x_labels
                .iter()
                .map(|&(f, w)| w - (1. - f) * plot_width)
                .fold(stroke, f32::max);
    }
    let plot_width = right - left;
    let x = |number| left + plot_width * fraction(number);
    let x_title = "Wavenumber (cm−1)";
    let x_format = wavenumber_format(x_title, &label_format);
    let x_title_layout = layout(x_title, &x_format);
    let x_title_bottom = x_title_layout
        .fragments
        .iter()
        .map(|run| run.position.y + run.style.size() * 1.1)
        .fold(0., f32::max);
    let x_title_y = height - pad - x_title_bottom;
    let major_length = pt(3.2);
    let tick_gap = pt(3.)
        + if framed {
            tick_height / 2.
        } else {
            major_length
        };
    let base = x_title_y - pt(2.5) - tick_height - tick_gap;
    let frame_top = pad + stroke;
    let method = figure
        .method_line
        .then(|| method_text(figure, &tick_format, plot_width - pt(4.)));
    let header = method
        .as_ref()
        .map_or(0., |(text, format)| layout(text, format).height);
    let top = frame_top + header;
    let y_max = if transmittance { 105. } else { 1.25 };

    let (frequencies, mut intensities): (Vec<_>, Vec<_>) =
        figure.bands.iter().map(|&[f, i]| (f, i)).unzip();
    // Rescaling every intensity equally keeps the relative spectrum finite.
    let strongest = intensities.iter().copied().fold(0., f64::max);
    if strongest > 0. {
        intensities.iter_mut().for_each(|i| *i /= strongest);
    }
    let step = (figure.fwhm / 8.).min(2.);
    let samples = (((figure.from - figure.to) / step).ceil() as usize + 1).min(4000);
    let mut points = vibrational::ir_spectrum(
        &frequencies,
        &intensities,
        figure.scale,
        figure.fwhm,
        figure.shape,
        (figure.from, figure.to),
        samples,
    );
    // Range and relative intensity cutoffs apply to the visible spectrum.
    let mut peaks = vibrational::ir_peaks(
        &frequencies,
        &intensities,
        figure.scale,
        figure.fwhm,
        figure.shape,
        0.,
        usize::MAX,
    );
    peaks.retain(|&[number, _]| (figure.to..=figure.from).contains(&number));
    points.extend_from_slice(&peaks);
    points.sort_by(|a, b| b[0].total_cmp(&a[0]));
    points.dedup_by(|a, b| a[0] == b[0]);
    let maximum = points.iter().map(|point| point[1]).fold(0., f64::max);
    let ordinate = |intensity| {
        let relative = if maximum > 0. {
            intensity / maximum
        } else {
            0.
        };
        (if transmittance {
            100. - 80. * relative
        } else {
            relative
        }) / y_max
    };
    let depths: Vec<_> = peaks.iter().map(|p| peak_depth(&points, p[0])).collect();
    let select = |height| {
        let scale = f64::from(height) * (ordinate(maximum) - ordinate(0.)).abs();
        select_labels(&peaks, maximum, usize::from(figure.labels), |i| {
            depths
                .get(i)
                .is_some_and(|depth| depth / maximum * scale >= f64::from(pt(0.5)))
        })
    };
    let label_size = |text: &str| {
        let measured = layout(text, &tick_format);
        if figure.vertical_labels {
            (measured.height, measured.width)
        } else {
            (measured.width, measured.height)
        }
    };
    let clearance = pt(1.);
    let (label_left, label_right) = if framed {
        (left + stroke, right - stroke)
    } else {
        (pad, width - pad)
    };
    let offset = pt(3.);
    // An open baseline has no upper frame. Use the highest curve mark to set
    // the top edge while keeping the curve's y scale linear.
    let upper_ordinate = if transmittance && !framed {
        ordinate(points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)) as f32
    } else {
        1.
    };
    let mut plot_height = if framed {
        base - top
    } else if transmittance {
        (base - top) / upper_ordinate
    } else {
        (base - top) * if maximum > 0. { y_max as f32 } else { 1. }
    };
    // Absorbance stays anchored at zero; %T stays anchored at its upper edge,
    // so shrinking the scale makes space below its downward-pointing tips.
    let anchor = if transmittance { upper_ordinate } else { 0. };
    let anchor_y = base - plot_height * anchor;
    let label_top = if framed {
        top + stroke + clearance
    } else {
        pad
    };
    let row_edge = if transmittance {
        base - stroke - 2. * clearance
    } else if framed {
        label_top + clearance
    } else if header > 0. {
        top + clearance
    } else {
        top
    };
    let space = (anchor_y - row_edge).abs();
    let mut labels = select(plot_height);
    loop {
        let row = labels
            .iter()
            .map(|label| label_size(&(label.peak[0].round() as u32).to_string()).1)
            .fold(0., f32::max);
        let reach = labels
            .iter()
            .map(|label| (anchor - ordinate(label.peak[1]) as f32).abs())
            .fold(0., f32::max);
        if reach == 0. {
            break;
        }
        // Leave a row and its leader beyond every selected tip. The extra
        // clearance also keeps exact-fit positions off the floating-point edge.
        let height = ((space - row - offset) / reach).max(0.);
        if height >= plot_height {
            break;
        }
        plot_height = height;
        // A smaller scale can remove a shallow peak and promote another tip.
        // Height only decreases, so drawn-depth eligibility never oscillates.
        labels = select(plot_height);
    }
    let y = |value| anchor_y + plot_height * (anchor - (value / y_max) as f32);
    let tip = |intensity| anchor_y + plot_height * (anchor - ordinate(intensity) as f32);
    let curve_points: Vec<_> = points
        .into_iter()
        .map(|[number, intensity]| Point::new(x(number), tip(intensity)))
        .collect();

    let first_id = doc.next_id();
    let mut next_id = first_id;
    let mut id = || {
        let id = next_id;
        next_id = next_id.saturating_add(1);
        id
    };
    let mut annotations = Vec::new();
    if let Some((text, format)) = method.or_else(|| {
        (!framed && !transmittance && maximum == 0.)
            .then(|| ("No IR intensity in range".into(), tick_format.clone()))
    }) {
        let measured = layout(&text, &format);
        let y = if framed { frame_top + pt(1.5) } else { pad };
        annotations.push(annotation(
            id(),
            text,
            format,
            Point::new(
                left + pt(2.) + measured.width / 2.,
                y + measured.height / 2.,
            ),
            0.,
        ));
    }

    let style = GraphicStyle {
        width_pt: line_width,
        ..Default::default()
    };
    let mut curve = Graphic::pen_curve(
        id(),
        Point::new(left, top),
        Point::new(right, top),
        style.clone(),
    );
    curve.path = curve_points
        .iter()
        .copied()
        .enumerate()
        .map(|(index, point)| {
            if index == 0 {
                PathCommand::Move(point)
            } else {
                PathCommand::Line(point)
            }
        })
        .collect();
    curve.node_types.clear();
    curve.ir = Some(Box::new(figure.clone()));

    let line = |id, start, end, width_pt| {
        Graphic::dragged(
            id,
            GraphicKind::Line,
            start,
            end,
            GraphicStyle {
                width_pt,
                ..style.clone()
            },
            BracketSides::Both,
            false,
        )
    };
    let p = Point::new;
    let mut graphics = vec![curve, line(id(), p(left, base), p(right, base), line_width)];
    if framed {
        graphics.push(line(id(), p(left, frame_top), p(left, base), line_width));
    }
    if figure.frame == Frame::Box {
        graphics.push(line(
            id(),
            p(left, frame_top),
            p(right, frame_top),
            line_width,
        ));
        graphics.push(line(id(), p(right, frame_top), p(right, base), line_width));
    }
    let frame_lines = graphics.len() - 1;
    let tick_step = if figure.minor_ticks { 100 } else { major };
    for tick in ticks(figure.from, figure.to, tick_step) {
        let x = x(f64::from(tick));
        let is_major = tick.is_multiple_of(major);
        let length = if is_major { major_length } else { pt(1.8) };
        graphics.push(line(
            id(),
            p(x, base),
            p(x, base + if framed { -length } else { length }),
            line_width,
        ));
        if is_major {
            annotations.push(annotation(
                id(),
                tick.to_string(),
                tick_format.clone(),
                p(x, base + tick_gap + tick_height / 2.),
                0.,
            ));
        }
    }
    annotations.push(annotation(
        id(),
        x_title.into(),
        x_format,
        p((left + right) / 2., x_title_y + x_title_layout.height / 2.),
        0.,
    ));
    if framed {
        for (value, text) in y_ticks {
            let y = y(value);
            graphics.push(line(
                id(),
                p(left, y),
                p(left + major_length, y),
                line_width,
            ));
            let label_width = layout(&text, &tick_format).width;
            annotations.push(annotation(
                id(),
                text,
                tick_format.clone(),
                p(left - pt(3.) - label_width / 2., y),
                0.,
            ));
        }
        annotations.push(annotation(
            id(),
            y_title.into(),
            label_format,
            p(pad + title_height / 2., (frame_top + base) / 2.),
            VERTICAL,
        ));
    }

    let axes: Vec<_> = graphics
        .iter()
        .skip(1)
        .enumerate()
        .map(|(i, line)| {
            // Inward ticks clear the label itself. Its padding also clears the frame.
            let inset = if i < frame_lines {
                -stroke
            } else {
                clearance - stroke
            };
            let a = line.origin;
            (a, a.offset(line.axis_x.x, line.axis_x.y), inset)
        })
        .collect();
    let axis_labels: Vec<_> = annotations.iter().map(Annotation::bounds).collect();
    let mut occupied = Vec::new();
    let mut leaders = Vec::new();
    let mut labelled_peaks = Vec::new();
    let direction = if transmittance { 1. } else { -1. };
    let angle = if figure.vertical_labels { VERTICAL } else { 0. };
    let curve_between = |left, right| {
        let first = curve_points
            .partition_point(|p| p.x < left)
            .saturating_sub(1);
        curve_points
            .array_windows::<2>()
            .skip(first)
            .take_while(move |[a, _]| a.x <= right)
    };
    // Resolve reserved peaks and doublets before individual fills.
    let placement_order = labels
        .iter()
        .filter(|label| label.required)
        .chain(labels.iter().filter(|label| !label.required));
    let has_doublet = labels.iter().any(|label| label.partner.is_some());
    for &PeakLabel {
        peak: [number, intensity],
        required,
        partner,
    } in placement_order
    {
        let rounded = number.round() as u32;
        let band_tip = p(x(number), tip(intensity));
        let text = rounded.to_string();
        let (w, h) = label_size(&text);
        let x_bounds = label_left + w / 2. ..=label_right - w / 2.;
        let y_bounds = label_top + h / 2. ..=base - stroke - h / 2.;
        let position = |dx, dy| band_tip.offset(dx, direction * (offset + h / 2. + dy));
        let leader = |centre: Point| {
            let end = centre.offset(0., -direction * (h / 2. + pt(1.25)));
            let length = band_tip.distance(end).max(f32::EPSILON);
            (band_tip.lerp(end, pt(0.8) / length), end, length)
        };
        // Leave room for the partner's first position while placing its anchor.
        let partner_box = partner
            .and_then(|number| labels.iter().find(|label| label.peak[0] == number))
            .filter(|label| {
                let rounded = label.peak[0].round() as u32;
                !occupied.iter().any(|&(peak, _)| peak == rounded)
            })
            .map(|label| {
                let [number, intensity] = label.peak;
                let (width, height) = label_size(&(number.round() as u32).to_string());
                let centre = p(
                    x(number),
                    tip(intensity) + direction * (offset + height / 2.),
                );
                (
                    centre.offset(-width / 2., -height / 2.),
                    centre.offset(width / 2., height / 2.),
                )
            });
        let collisions = |centre: Point, start, end, length| {
            let clear_start = band_tip.lerp(end, pt(1.5) / length);
            let (lo, hi) = (
                centre.offset(-w / 2. - clearance, -h / 2. - clearance),
                centre.offset(w / 2. + clearance, h / 2. + clearance),
            );
            let hits_label = |a: Point, b: Point, gap: f32| {
                (lo.x <= b.x && hi.x >= a.x && lo.y <= b.y && hi.y >= a.y)
                    || hits_box(start, end, a.offset(-gap, -gap), b.offset(gap, gap))
            };
            axis_labels
                .iter()
                .filter(|&&(a, b)| hits_label(a, b, 0.))
                .count()
                + axes
                    .iter()
                    .filter(|&&(a, b, inset)| {
                        hits_box(a, b, lo.offset(inset, inset), hi.offset(-inset, -inset))
                    })
                    .count()
                + occupied
                    .iter()
                    .filter(|&&(_, (a, b))| hits_label(a, b, clearance))
                    .count()
                + leaders
                    .iter()
                    .filter(|&&(a, b)| hits_box(a, b, lo, hi) || segments_cross(start, end, a, b))
                    .count()
                + usize::from(curve_between(lo.x, hi.x).any(|&[a, b]| hits_box(a, b, lo, hi)))
                + usize::from(
                    curve_between(clear_start.x.min(end.x), clear_start.x.max(end.x))
                        .any(|&[a, b]| segments_cross(clear_start, end, a, b)),
                )
        };
        let search = |steps: u8, side_steps, pitch: f32, protect_partner, allow_overlaps| {
            let mut shifts: Vec<_> = (0..steps)
                .flat_map(|step| {
                    let dy = f32::from(step) * (h + pt(1.5)) * pitch;
                    std::iter::once(0)
                        .chain((1..=side_steps).flat_map(|n: i16| [-n, n]))
                        .map(move |n| (w * f32::from(n) * pitch / 2., dy))
                })
                .collect();
            shifts.sort_by(|&(ax, ay), &(bx, by)| {
                (ax * ax + ay * ay).total_cmp(&(bx * bx + by * by))
            });
            let mut best = None;
            for (dx, dy) in shifts {
                let centre = position(dx, dy);
                if !x_bounds.contains(&centre.x) || !y_bounds.contains(&centre.y) {
                    continue;
                }
                if protect_partner
                    && partner_box.is_some_and(|(lo, hi)| {
                        let (rx, ry) = (w / 2. + clearance, h / 2. + clearance);
                        centre.x - rx <= hi.x
                            && centre.x + rx >= lo.x
                            && centre.y - ry <= hi.y
                            && centre.y + ry >= lo.y
                    })
                {
                    continue;
                }
                let (start, end, length) = leader(centre);
                let count = collisions(centre, start, end, length);
                let candidate = (centre, start, end);
                if count == 0 {
                    return Some(candidate);
                }
                if allow_overlaps && best.as_ref().is_none_or(|(fewest, _)| count < *fewest) {
                    best = Some((count, candidate));
                }
            }
            best.map(|(_, candidate)| candidate)
        };
        let clean = if partner.is_some() {
            search(32, 24, 0.25, true, false)
                .or_else(|| partner_box.and_then(|_| search(32, 24, 0.25, false, false)))
        } else {
            search(8, 3, 1., false, false).or_else(|| {
                has_doublet
                    .then(|| search(16, 12, 0.5, false, false))
                    .flatten()
            })
        };
        let Some((centre, start, end)) = clean.or_else(|| {
            required.then(|| {
                search(16, 6, 1., false, true).unwrap_or_else(|| {
                    // A band near an edge can have every outward shift outside the plot.
                    let centre = position(0., 0.);
                    let centre = p(
                        centre.x.clamp(*x_bounds.start(), *x_bounds.end()),
                        centre.y.clamp(*y_bounds.start(), *y_bounds.end()),
                    );
                    let (start, end, _) = leader(centre);
                    (centre, start, end)
                })
            })
        }) else {
            continue;
        };
        // Hidden labels still occupy their space in the layout.
        let label = annotation(0, text, tick_format.clone(), centre, angle);
        occupied.push((rounded, label.bounds()));
        leaders.push((start, end));
        if figure.hidden_labels.contains(&rounded) {
            continue;
        }
        graphics.push(line(id(), start, end, line_width / 2.));
        annotations.push(Annotation { id: id(), ..label });
        labelled_peaks.push(rounded);
    }

    for graphic in &mut graphics {
        graphic.origin = graphic.origin.offset(at.x, at.y);
    }
    for annotation in &mut annotations {
        annotation.position = annotation.position.offset(at.x, at.y);
    }
    let group_id = id();
    Figure {
        graphics,
        annotations,
        group: Group {
            id: group_id,
            members: (first_id..group_id).collect(),
            integral: false,
            layout: None,
        },
        labelled_peaks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::graphics::{BandShape, Columns};

    fn tip_at(chart: &Figure, ir: &IrFigure, number: f64) -> Point {
        let curve = chart.graphics[0].commands();
        let left = curve.first().unwrap().points()[0].x;
        let right = curve.last().unwrap().points()[0].x;
        let x = left + (right - left) * ((ir.from - number) / (ir.from - ir.to)) as f32;
        curve
            .iter()
            .map(|command| command.points()[0])
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .unwrap()
    }

    fn assert_outward_label(doc: &Document, chart: &Figure, ir: &IrFigure, number: u32) {
        assert_eq!(chart.labelled_peaks, [number]);
        let label = chart.annotations.last().unwrap();
        assert_eq!(label.text, number.to_string());
        let (lo, hi) = label.bounds();
        let tip = tip_at(chart, ir, f64::from(number));
        if ir.y == YAxis::Transmittance {
            assert!(lo.y > tip.y, "the label stays below its dip");
            assert!(hi.y < chart.graphics[1].origin.y);
        } else {
            assert!(hi.y < tip.y, "the label stays above its peak");
            assert!(lo.y >= doc.drawing_style.world(EXPORT_PAD_PT));
        }
        let clearance = doc.drawing_style.world(1.);
        let (lo, hi) = (
            lo.offset(-clearance, -clearance),
            hi.offset(clearance, clearance),
        );
        let leader = chart.graphics.last().unwrap();
        let start = leader.origin;
        let end = start.offset(leader.axis_x.x, leader.axis_x.y);
        assert!(!hits_box(start, end, lo, hi), "the leader clears its label");
        let clear_start = start.lerp(end, doc.drawing_style.world(0.7) / start.distance(end));
        for pair in chart.graphics[0].commands().windows(2) {
            let (a, b) = (pair[0].points()[0], pair[1].points()[0]);
            assert!(!hits_box(a, b, lo, hi), "the label clears the curve");
            assert!(
                !segments_cross(clear_start, end, a, b),
                "the leader clears the curve after leaving its tip"
            );
        }
    }

    #[test]
    fn baseline_absorbance_reserves_a_row_above_a_selected_near_maximum() {
        let doc = Document::default();
        let mut ir = IrFigure {
            bands: vec![[3400., 99.], [1050., 100.]],
            y: YAxis::Absorbance,
            frame: Frame::Baseline,
            labels: 1,
            ..IrFigure::default()
        };
        for vertical in [false, true] {
            ir.vertical_labels = vertical;
            let chart = spectrum(&doc, &ir, Point::default());
            assert_outward_label(&doc, &chart, &ir, 3400);
            assert!(tip_at(&chart, &ir, 1050.).y < tip_at(&chart, &ir, 3400.).y);

            ir.hidden_labels = vec![3400];
            let hidden = spectrum(&doc, &ir, Point::default());
            assert!(hidden.labelled_peaks.is_empty());
            assert_eq!(hidden.graphics[0].path, chart.graphics[0].path);
            ir.hidden_labels.clear();
        }
    }

    #[test]
    fn transmittance_reserves_a_vertical_label_row_below_a_near_edge_dip() {
        let doc = Document::default();
        let mut ir = IrFigure {
            // This interior maximum rounds to a five-digit label. The long
            // vertical row needs more room than the usual 20% lower margin.
            bands: vec![[9999.6, 100.]],
            from: 10000.,
            to: 9800.,
            fwhm: 2.,
            shape: BandShape::Gaussian,
            labels: 1,
            vertical_labels: true,
            method_line: true,
            ..IrFigure::default()
        };
        for frame in [Frame::Box, Frame::Axes, Frame::Baseline] {
            ir.frame = frame;
            let chart = spectrum(&doc, &ir, Point::default());
            assert_outward_label(&doc, &chart, &ir, 10000);
            let without_labels = spectrum(
                &doc,
                &IrFigure {
                    labels: 0,
                    ..ir.clone()
                },
                Point::default(),
            );
            assert!(
                tip_at(&chart, &ir, 9999.6).y < tip_at(&without_labels, &ir, 9999.6).y,
                "{frame:?}: headroom moves the selected dip away from the bottom"
            );
            ir.hidden_labels = vec![10000];
            let hidden = spectrum(&doc, &ir, Point::default());
            assert!(hidden.labelled_peaks.is_empty());
            assert_eq!(hidden.graphics[0].path, chart.graphics[0].path);
            ir.hidden_labels.clear();
        }
    }

    #[test]
    fn a_selected_neighbour_does_not_block_the_other_doublet_partner() {
        let bands = [
            [2050., 400.],
            [2010., 390.],
            [1980., 300.],
            [1940., 100.],
            [1050., 10000.],
        ];
        let selected = select_labels(&bands, 10000., 6, |_| true);
        let anchor = selected
            .iter()
            .find(|label| label.peak[0] == 1980.)
            .unwrap();
        assert_eq!(anchor.partner, Some(1940.));
        assert_eq!(
            selected
                .iter()
                .map(|label| label.peak[0])
                .collect::<Vec<_>>(),
            [2050., 2010., 1980., 1940., 1050.]
        );
        let ir = IrFigure {
            bands: bands.into(),
            shape: BandShape::Gaussian,
            fwhm: 2.,
            columns: Columns::Two,
            labels: 6,
            ..IrFigure::default()
        };
        let chart = spectrum(&Document::default(), &ir, Point::default());
        assert!(chart.labelled_peaks.contains(&1940));
        assert_eq!(chart.labelled_peaks.len(), 5);
    }
}
