//! Labelled reaction-energy figures. Layout is independent of rendering so a
//! Path tab and a multistep diagram can share scales, annotations and selection.
use crate::appearance::EnergyUnit;
use iced::widget::canvas::{self, Frame, Path, Stroke, Text};
use iced::{Color, Point, Rectangle, Size, Theme};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub x: f64,
    pub energy: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    Line,
    // The upcoming multistep diagram uses the same labelled figure as levels.
    #[allow(dead_code)]
    Levels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Above,
    Below,
}

#[derive(Clone, Debug)]
pub(crate) struct Marker {
    pub sample: Sample,
    pub label: String,
    pub side: Side,
    pub transition: bool,
    pub note: Option<String>,
    /// Index into the caller's prepared molecular thumbnails.
    pub thumbnail: Option<usize>,
}

pub(crate) struct Profile {
    pub samples: Vec<Sample>,
    pub overlay: Vec<Sample>,
    pub markers: Vec<Marker>,
    pub selected: Option<usize>,
    pub style: Style,
    pub unit: EnergyUnit,
    pub x_title: String,
    pub x_ticks: bool,
    pub highlight_curve: bool,
    pub stale: bool,
}

#[derive(Debug)]
pub(crate) struct Label<'a> {
    pub text: String,
    pub bounds: Rectangle,
    marker: Option<&'a Marker>,
}

#[derive(Debug)]
pub(crate) struct ModelBox {
    pub thumbnail: usize,
    pub bounds: Rectangle,
    sample: Sample,
}

#[derive(Debug)]
pub(crate) struct ProfileLayout<'a> {
    pub plot: Rectangle,
    pub labels: Vec<Label<'a>>,
    pub models: Vec<ModelBox>,
    pub energy_ticks: Vec<f64>,
    pub coordinate_ticks: Vec<f64>,
    legend_y: Option<f32>,
    footer_legend: bool,
    x_range: (f64, f64),
    energy_range: (f64, f64),
}

const LEFT: f32 = 52.;
const RIGHT: f32 = 16.;
const FONT: f32 = 11.;
const LINE: f32 = 15.;

fn finite_range(values: impl Iterator<Item = f64>, include_zero: bool) -> (f64, f64) {
    let mut range: Option<(f64, f64)> = include_zero.then_some((0., 0.));
    for value in values.filter(|value| value.is_finite()) {
        range = Some(range.map_or((value, value), |(lo, hi)| (lo.min(value), hi.max(value))));
    }
    let (lo, hi) = range.unwrap_or((0., 1.));
    if lo == hi { (lo, lo + 1.) } else { (lo, hi) }
}

/// Round a scale in the displayed unit, rather than making kJ or eV ticks
/// inherit awkward converted kcal intervals.
fn nice_scale(lo: f64, hi: f64) -> (f64, f64, Vec<f64>) {
    let raw = (hi - lo).max(0.000_001) / 4.;
    let magnitude = 10_f64.powf(raw.log10().floor());
    let fraction = raw / magnitude;
    let step = magnitude
        * if fraction <= 1. {
            1.
        } else if fraction <= 2. {
            2.
        } else if fraction <= 5. {
            5.
        } else {
            10.
        };
    let bottom = (lo / step).floor() * step;
    let top = (hi / step).ceil() * step;
    let count = ((top - bottom) / step).round() as usize;
    let ticks = (0..=count.min(12))
        .map(|index| bottom + index as f64 * step)
        .collect();
    (bottom, top, ticks)
}

fn text_width(text: &str) -> f32 {
    // Conservative for the UI font, including the widest digits and TS glyphs.
    text.chars().count() as f32 * FONT * 0.66 + 6.
}

fn wrap_note(text: &str, width: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.into()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && text_width(&candidate) > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

impl Profile {
    fn coordinate_range(&self) -> (f64, f64) {
        let (lo, hi) = finite_range(
            self.samples
                .iter()
                .chain(self.overlay.iter())
                .map(|sample| sample.x)
                .chain(self.markers.iter().map(|marker| marker.sample.x)),
            false,
        );
        if self.style == Style::Levels {
            let margin = (hi - lo) / self.samples.len().saturating_sub(1).max(1) as f64 / 2.;
            (lo - margin, hi + margin)
        } else {
            (lo, hi)
        }
    }

    pub(crate) fn energy_bounds(&self) -> (f64, f64) {
        finite_range(
            self.samples
                .iter()
                .chain(self.overlay.iter())
                .map(|sample| sample.energy)
                .chain(self.markers.iter().map(|marker| marker.sample.energy)),
            true,
        )
    }

    pub(crate) fn x(&self, coordinate: f64, bounds: Rectangle) -> f32 {
        let (lo, hi) = self.coordinate_range();
        LEFT + ((coordinate - lo) / (hi - lo)) as f32 * (bounds.width - LEFT - RIGHT).max(1.)
    }

    /// Selection follows the actual coordinate spacing, including irregular IRC
    /// samples. A partial/nonfinite sample never becomes a selectable point.
    pub(crate) fn nearest(&self, x: f32, bounds: Rectangle) -> Option<usize> {
        self.samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.x.is_finite() && sample.energy.is_finite())
            .min_by(|(_, a), (_, b)| {
                (self.x(a.x, bounds) - x)
                    .abs()
                    .total_cmp(&(self.x(b.x, bounds) - x).abs())
            })
            .map(|(index, _)| index)
    }

    pub(crate) fn layout(&self, bounds: Rectangle) -> ProfileLayout<'_> {
        let width = (bounds.width - LEFT - RIGHT).max(1.);
        let x_range = self.coordinate_range();
        let (lo, hi) = self.energy_bounds();
        let factor = self.unit.convert_kcal(1.);
        let (lo, hi, ticks) = nice_scale(lo * factor, hi * factor);
        let energy_range = (lo / factor, hi / factor);
        let energy_ticks = ticks.into_iter().map(|tick| tick / factor).collect();
        let coordinate_ticks = if self.x_ticks {
            let (_, _, ticks) = nice_scale(x_range.0, x_range.1);
            ticks
                .into_iter()
                .filter(|tick| *tick >= x_range.0 && *tick <= x_range.1)
                .collect()
        } else {
            vec![]
        };
        let show_models = bounds.width >= 360. && bounds.height >= 200.;
        let model_markers: Vec<_> = self
            .markers
            .iter()
            .filter(|_| show_models)
            .filter_map(|marker| Some((marker, marker.thumbnail?)))
            .collect();
        let mut models = vec![];
        let model_height = if model_markers.is_empty() { 0. } else { 44. };
        if !model_markers.is_empty() {
            let slot_width = width / model_markers.len() as f32;
            for (slot, (marker, thumbnail)) in model_markers.into_iter().enumerate() {
                let size = Size::new((slot_width - 6.).min(88.), 40.);
                let ideal = self.x(marker.sample.x, bounds);
                let center = ideal.clamp(
                    LEFT + slot as f32 * slot_width + size.width / 2.,
                    LEFT + (slot + 1) as f32 * slot_width - size.width / 2.,
                );
                models.push(ModelBox {
                    thumbnail,
                    bounds: Rectangle::new(Point::new(center - size.width / 2., 1.), size),
                    sample: marker.sample,
                });
            }
        }
        // Marker text is packed into separate lanes outside the data rectangle.
        // This remains safe for exothermic, endothermic and almost flat paths.
        let mut labels = vec![];
        let mut rows = [0_usize; 2];
        for (row_count, side) in rows.iter_mut().zip([Side::Above, Side::Below]) {
            let mut markers: Vec<_> = self
                .markers
                .iter()
                .filter(|marker| marker.side == side)
                .collect();
            markers.sort_by(|a, b| a.sample.x.total_cmp(&b.sample.x));
            let mut row = 0;
            let mut right = LEFT;
            for marker in markers {
                let text = format!(
                    "{} {}",
                    marker.label,
                    self.unit.number(marker.sample.energy)
                );
                let label_width = text_width(&text).min(width);
                let preferred = (self.x(marker.sample.x, bounds) - label_width / 2.)
                    .clamp(LEFT, LEFT + width - label_width);
                let mut left = preferred.max(right);
                if left + label_width > LEFT + width + 0.1 {
                    row += 1;
                    left = preferred;
                }
                right = left + label_width + 8.;
                *row_count = row + 1;
                labels.push(Label {
                    text,
                    bounds: Rectangle::new(
                        Point::new(left, row as f32 * LINE),
                        Size::new(label_width, LINE),
                    ),
                    marker: Some(marker),
                });
            }
        }
        let mut note_lines = vec![];
        for marker in &self.markers {
            if let Some(note) = &marker.note {
                let x = self.x(marker.sample.x, bounds);
                note_lines.extend(wrap_note(note, width).into_iter().map(|line| (line, x)));
            }
        }
        let footer_legend = !self.overlay.is_empty() && bounds.height < 200. && width >= 240.;
        let legend_y = (!self.overlay.is_empty()).then_some(if footer_legend {
            bounds.height - 14.
        } else {
            3. + model_height
        });
        let legend_height = if legend_y.is_some() && !footer_legend {
            LINE
        } else {
            0.
        };
        let top = 3. + model_height + legend_height + rows[0] as f32 * LINE;
        let tick_height = if self.x_ticks { 15. } else { 0. };
        let footer = 22. + tick_height + rows[1] as f32 * LINE;
        // Leave a readable trace when a plan has many forming distances. The
        // accessible figure name retains the entire annotation.
        let max_lines = ((bounds.height - footer - top - 40.) / LINE)
            .floor()
            .max(1.) as usize;
        if note_lines.len() > max_lines {
            note_lines.truncate(max_lines);
            if let Some((last, _)) = note_lines.last_mut() {
                last.push('…');
            }
        }
        let plot_top = top + note_lines.len() as f32 * LINE + 10.;
        let plot = Rectangle::new(
            Point::new(LEFT, plot_top),
            Size::new(width, (bounds.height - footer - plot_top).max(1.)),
        );
        for label in &mut labels {
            label.bounds.y += match label.marker.map(|m| m.side) {
                None => continue,
                Some(Side::Above) => 3. + model_height + legend_height,
                Some(Side::Below) => plot.y + plot.height + tick_height + 5.,
            };
        }
        for (index, (text, anchor)) in note_lines.into_iter().enumerate() {
            let label_width = text_width(&text).min(width);
            let left = (anchor - label_width / 2.).clamp(LEFT, LEFT + width - label_width);
            labels.push(Label {
                text,
                bounds: Rectangle::new(
                    Point::new(left, top + index as f32 * LINE),
                    Size::new(label_width, LINE),
                ),
                marker: None,
            });
        }
        ProfileLayout {
            plot,
            labels,
            models,
            energy_ticks,
            coordinate_ticks,
            legend_y,
            footer_legend,
            x_range,
            energy_range,
        }
    }

    pub(crate) fn draw(&self, frame: &mut Frame, theme: &Theme, layout: &ProfileLayout<'_>) {
        let muted = crate::appearance::muted(theme);
        let ink = if self.stale {
            muted
        } else {
            theme.palette().text
        };
        let accent = crate::appearance::readable(
            theme.palette().primary,
            &[theme.palette().background],
            reshiki::color_contrast::TEXT_TARGET,
        );
        let amber = crate::appearance::readable(
            theme.palette().warning,
            &[theme.palette().background],
            reshiki::color_contrast::TEXT_TARGET,
        );
        let grid = Color { a: 0.22, ..muted };
        let label = |frame: &mut Frame, content: String, at: Point, color, size, align| {
            frame.fill_text(Text {
                content,
                position: at,
                color,
                size: iced::Pixels(size),
                align_x: align,
                ..Text::default()
            });
        };
        frame.with_save(|frame| {
            frame.translate(iced::Vector::new(3., layout.plot.center_y()));
            frame.rotate(-std::f32::consts::FRAC_PI_2);
            label(
                frame,
                format!("ΔE ({})", self.unit.label()),
                Point::ORIGIN,
                muted,
                10.,
                iced::alignment::Horizontal::Center.into(),
            );
        });
        let mut last_tick_y = f32::INFINITY;
        for &energy in &layout.energy_ticks {
            let y = layout.point(Sample { x: 0., energy }).y;
            frame.stroke(
                &Path::line(
                    Point::new(layout.plot.x, y),
                    Point::new(layout.plot.x + layout.plot.width, y),
                ),
                Stroke::default()
                    .with_color(grid)
                    .with_width(if energy.abs() < 1e-9 { 1.2 } else { 0.7 }),
            );
            if last_tick_y - y >= 12. {
                label(
                    frame,
                    self.unit.number(energy),
                    Point::new(LEFT - 7., y - 6.),
                    muted,
                    10.,
                    iced::alignment::Horizontal::Right.into(),
                );
                last_tick_y = y;
            }
        }
        for &coordinate in &layout.coordinate_ticks {
            let x = layout
                .point(Sample {
                    x: coordinate,
                    energy: 0.,
                })
                .x;
            let y = layout.plot.y + layout.plot.height;
            frame.stroke(
                &Path::line(Point::new(x, y), Point::new(x, y + 3.)),
                Stroke::default().with_color(muted).with_width(0.7),
            );
            label(
                frame,
                coordinate_number(coordinate),
                Point::new(x, y + 4.),
                muted,
                10.,
                iced::alignment::Horizontal::Center.into(),
            );
        }
        if let Some(sample) = self
            .selected
            .and_then(|index| self.samples.get(index))
            .filter(|sample| sample.x.is_finite() && sample.energy.is_finite())
        {
            let point = layout.point(*sample);
            frame.stroke(
                &Path::line(
                    Point::new(point.x, layout.plot.y),
                    Point::new(point.x, layout.plot.y + layout.plot.height),
                ),
                Stroke::default()
                    .with_color(Color { a: 0.25, ..accent })
                    .with_width(1.),
            );
        }
        let curve = if self.highlight_curve && !self.stale {
            accent
        } else {
            ink
        };
        match self.style {
            Style::Line => {
                let path = Path::new(|path| {
                    let mut connected = false;
                    for sample in &self.samples {
                        if !sample.x.is_finite() || !sample.energy.is_finite() {
                            connected = false;
                            continue;
                        }
                        let point = layout.point(*sample);
                        if connected {
                            path.line_to(point);
                        } else {
                            path.move_to(point);
                        }
                        connected = true;
                    }
                });
                frame.stroke(&path, Stroke::default().with_color(curve).with_width(1.7));
            }
            Style::Levels => {
                let half_width =
                    (layout.plot.width / self.samples.len().max(1) as f32 * 0.3).min(22.);
                for sample in &self.samples {
                    if sample.x.is_finite() && sample.energy.is_finite() {
                        let point = layout.point(*sample);
                        frame.stroke(
                            &Path::line(
                                Point::new(point.x - half_width, point.y),
                                Point::new(point.x + half_width, point.y),
                            ),
                            Stroke::default().with_color(curve).with_width(2.),
                        );
                    }
                }
            }
        }
        for (index, sample) in self.samples.iter().enumerate() {
            if !sample.x.is_finite() || !sample.energy.is_finite() {
                continue;
            }
            let point = layout.point(*sample);
            let selected = self.selected == Some(index);
            if selected {
                frame.stroke(
                    &Path::circle(point, 6.),
                    Stroke::default().with_color(curve).with_width(1.),
                );
            }
            if self.style == Style::Line {
                frame.fill(
                    &Path::circle(point, if selected { 3.8 } else { 2.3 }),
                    curve,
                );
            }
        }
        for sample in &self.overlay {
            if sample.x.is_finite() && sample.energy.is_finite() {
                frame.fill(&Path::circle(layout.point(*sample), 2.5), muted);
            }
        }
        for model in &layout.models {
            let point = layout.point(model.sample);
            frame.stroke(
                &Path::line(
                    Point::new(
                        model.bounds.center_x(),
                        model.bounds.y + model.bounds.height,
                    ),
                    point,
                ),
                Stroke {
                    line_dash: canvas::LineDash {
                        segments: &[2., 3.],
                        offset: 0,
                    },
                    ..Stroke::default()
                        .with_color(Color { a: 0.45, ..muted })
                        .with_width(0.8)
                },
            );
        }
        for annotation in &layout.labels {
            if let Some(marker) = annotation.marker {
                let point = layout.point(marker.sample);
                let y = if marker.side == Side::Above {
                    annotation.bounds.y + annotation.bounds.height
                } else {
                    annotation.bounds.y
                };
                let end = Point::new(
                    point.x.clamp(
                        annotation.bounds.x,
                        annotation.bounds.x + annotation.bounds.width,
                    ),
                    y,
                );
                frame.stroke(
                    &Path::line(point, end),
                    Stroke::default()
                        .with_color(Color { a: 0.5, ..muted })
                        .with_width(0.8),
                );
            }
        }
        for marker in &self.markers {
            let point = layout.point(marker.sample);
            if marker.transition {
                let diamond = Path::new(|path| {
                    path.move_to(Point::new(point.x, point.y - 5.));
                    path.line_to(Point::new(point.x + 5., point.y));
                    path.line_to(Point::new(point.x, point.y + 5.));
                    path.line_to(Point::new(point.x - 5., point.y));
                    path.close();
                });
                frame.fill(&diamond, theme.palette().background);
                frame.stroke(
                    &diamond,
                    Stroke::default().with_color(amber).with_width(1.5),
                );
            } else {
                frame.fill(&Path::circle(point, 3.8), theme.palette().background);
                frame.stroke(
                    &Path::circle(point, 3.8),
                    Stroke::default().with_color(ink).with_width(1.4),
                );
            }
        }
        for annotation in &layout.labels {
            // Opaque text backing also prevents a thumbnail leader crossing text.
            frame.fill_rectangle(
                annotation.bounds.position(),
                annotation.bounds.size(),
                theme.palette().background,
            );
            let color = annotation
                .marker
                .map_or(muted, |marker| if marker.transition { amber } else { ink });
            label(
                frame,
                annotation.text.clone(),
                annotation.bounds.position(),
                color,
                if annotation.marker.is_some() {
                    FONT
                } else {
                    10.
                },
                iced::alignment::Horizontal::Left.into(),
            );
        }
        let (x, align) = if layout.footer_legend {
            (layout.plot.x, iced::alignment::Horizontal::Left)
        } else {
            (layout.plot.center_x(), iced::alignment::Horizontal::Center)
        };
        label(
            frame,
            self.x_title.clone(),
            Point::new(x, frame.height() - 14.),
            muted,
            10.,
            align.into(),
        );
        if let Some(y) = layout.legend_y {
            // The legend has its own annotation lane, outside the curve.
            let x = layout.plot.x + layout.plot.width - 88.;
            frame.stroke(
                &Path::line(Point::new(x, y + 5.), Point::new(x + 14., y + 5.)),
                Stroke::default().with_color(curve).with_width(1.7),
            );
            label(
                frame,
                "IRC".into(),
                Point::new(x + 18., y),
                muted,
                9.,
                iced::alignment::Horizontal::Left.into(),
            );
            frame.fill(&Path::circle(Point::new(x + 47., y + 5.), 2.3), muted);
            label(
                frame,
                "NEB".into(),
                Point::new(x + 54., y),
                muted,
                9.,
                iced::alignment::Horizontal::Left.into(),
            );
        }
    }
}

impl ProfileLayout<'_> {
    pub(crate) fn point(&self, sample: Sample) -> Point {
        Point::new(
            self.plot.x
                + ((sample.x - self.x_range.0) / (self.x_range.1 - self.x_range.0)) as f32
                    * self.plot.width,
            self.plot.y
                + ((self.energy_range.1 - sample.energy)
                    / (self.energy_range.1 - self.energy_range.0)) as f32
                    * self.plot.height,
        )
    }
}

fn coordinate_number(value: f64) -> String {
    let value = if value.abs() < 1e-9 { 0. } else { value };
    if value.abs() < 0.1 && value != 0. {
        format!("{value:.2}")
    } else if (value - value.round()).abs() < 1e-8 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
    .replace('-', "−")
}

#[cfg(test)]
mod tests;
