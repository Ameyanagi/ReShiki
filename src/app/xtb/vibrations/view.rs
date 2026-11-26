//! The Vibrations tab's widgets: the imaginary-mode banner, the mode list,
//! thermochemistry, the IR spectrum and the animation controls.
use super::super::palette::Shown;
use super::super::state::signed;
use super::super::tone::{Tone, line};
use super::super::{Action as XtbAction, unavailable};
use super::{Action, State, thermo_at};
use crate::app::workspace::{accessible_checkbox, accessible_command, control, muted_text};
use crate::app::{App, Message};
use iced::widget::{Column, canvas, column, container, row, scrollable, slider, text};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::qcdata::vibrational::{self, BandShape};
use reshiki::qcdata::{CalcResult, Thermo};
use std::sync::{Arc, Weak};

/// cal/mol per hartree.
const CAL_PER_MOL_PER_HARTREE: f64 = 627_509.474;
const LIST_WIDTH: f32 = 250.;
const CHART_WIDTH: f32 = 480.;
/// Chart height in the inspector; the bottom dock's body is shorter.
const CHART_HEIGHT: f32 = 156.;
const DOCK_CHART_HEIGHT: f32 = 128.;
/// Peaks labelled on the spectrum: at most this many, at least this fraction of the tallest.
const LABELS: usize = 8;
const LABEL_FRACTION: f64 = 0.05;
const MAX_WAVENUMBER: f64 = 10_000.;
const MAX_TICKS: usize = 40;
const UNPLOTTABLE: &str = "Frequencies outside 0–10000 cm⁻¹ cannot be plotted";

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Vibrations(action))
}

/// "1590 cm⁻¹", or "−212i cm⁻¹" for an imaginary mode.
pub(super) fn wavenumber(frequency: f64) -> String {
    if frequency < 0. {
        format!("{}i cm⁻¹", signed(frequency, 0))
    } else {
        format!("{frequency:.0} cm⁻¹")
    }
}

impl App {
    pub(in crate::app::xtb) fn xtb_vibrations<'a>(
        &'a self,
        shown: &Shown<'a>,
        inspector: bool,
    ) -> Element<'a, Message> {
        let result = &shown.stored.result;
        let Some(vibrations) = &result.vibrations else {
            return column![].into();
        };
        let settings = &self.tab.xtb.vibrations;
        let selected = self.xtb_mode(vibrations.frequencies_cm1.len());
        let mut body = column![].spacing(8);
        if let Some(banner) = self.xtb_imaginary_banner(shown) {
            body = body.push(banner);
        }
        let list = mode_list(result, selected, shown.stale);
        let left = column![
            container(scrollable(list).id("xtb-vibration-modes"))
                .height(if inspector {
                    Length::Fixed(160.)
                } else {
                    Length::Fixed(120.)
                })
                .width(Length::Fill),
            thermochemistry(settings, shown),
        ]
        .spacing(8)
        .width(if inspector {
            Length::Fill
        } else {
            Length::Fixed(LIST_WIDTH)
        });
        let chart = canvas(Spectrum::new(result, settings, selected));
        let controls = self.xtb_animation_controls(shown);
        if inspector {
            body = body
                .push(left)
                .push(chart.width(Length::Fill).height(CHART_HEIGHT))
                .push(spectrum_controls(settings))
                .push(controls);
        } else {
            // Everything fits the dock's short body: list, chart, then the
            // animation and spectrum controls side by side.
            body = body.push(
                row![
                    left,
                    chart
                        .width(Length::Fixed(CHART_WIDTH))
                        .height(DOCK_CHART_HEIGHT),
                    controls.width(Length::FillPortion(3)),
                    container(spectrum_controls(settings)).width(Length::FillPortion(2)),
                ]
                .spacing(18)
                .align_y(Alignment::Start),
            );
        }
        body.into()
    }

    /// "1 imaginary frequency (−212 cm⁻¹): this is not a minimum." with
    /// "Push along mode and re-optimize".
    fn xtb_imaginary_banner(&self, shown: &Shown<'_>) -> Option<Element<'_, Message>> {
        let vibrations = shown.stored.result.vibrations.as_ref()?;
        let count = vibrations.imaginary_count();
        let lowest = *vibrations.frequencies_cm1.first().filter(|f| **f < 0.)?;
        let title = if count == 1 {
            format!(
                "1 imaginary frequency ({} cm⁻¹): this is not a minimum.",
                signed(lowest, 0)
            )
        } else {
            format!(
                "{count} imaginary frequencies (lowest {} cm⁻¹): this is not a minimum.",
                signed(lowest, 0)
            )
        };
        let can_push = !shown.stale
            && self.xtb_model_at_result(shown)
            && unavailable().is_none()
            && self.tab.xtb.run.is_none()
            && super::super::results::config(&shown.stored.result, shown.frame, shown.atoms, 1)
                .is_some();
        // The dock's body is short: the explanation goes in the inspector only.
        let detail = if can_push && self.xtb.dock.inspector {
            "Push moves the structure 0.3 Å along the mode, optimizes and recomputes frequencies; \
             the structure changes only if the optimization converges."
        } else {
            ""
        };
        let push = reshiki::accessibility::button(
            "xtb-push-mode",
            "Push 0.3 Å along the imaginary mode and re-optimize; the structure changes only if the optimization converges",
            text("Push along mode and re-optimize").size(12),
        )
        .padding([6, 12])
        .style(crate::appearance::secondary)
        .on_press_maybe(can_push.then(|| send(Action::Push)));
        Some(super::super::palette::alert(
            true,
            title,
            detail.into(),
            vec![push.into()],
        ))
    }

    fn xtb_animation_controls<'a>(&'a self, shown: &Shown<'a>) -> Column<'a, Message> {
        let settings = &self.tab.xtb.vibrations;
        let result = &shown.stored.result;
        let modes = self.xtb_modes(result).is_some();
        let model_matches = self.xtb_model_at_result(shown);
        let live = modes && !shown.stale && model_matches;
        let mut controls = column![].spacing(7);
        // Why nothing moves, when nothing can.
        let note = if shown.stale {
            Some("Recalculate to show the vibrations.".to_owned())
        } else if !model_matches {
            Some("The 3D model is not at this result's geometry; use the result's geometry (Steps tab) to animate.".to_owned())
        } else if settings.recomputing(result) {
            Some("Recomputing the normal modes…".to_owned())
        } else if let Some((_, problem)) = settings
            .problem
            .as_ref()
            .filter(|(weak, _)| super::is(weak, result))
        {
            Some(problem.clone())
        } else if !modes && unavailable().is_some() {
            Some("Normal modes are not saved in the file; this build cannot recompute them.".into())
        } else {
            None
        };
        if let Some(note) = note {
            controls = controls.push(line(note, Tone::Muted));
        }
        let play = reshiki::accessibility::button(
            "xtb-vibration-play",
            if settings.playing {
                "Pause the animation"
            } else {
                "Play the animation"
            },
            text(if settings.playing { "Pause" } else { "Play" }).size(12),
        )
        .padding([6, 12])
        .style(crate::appearance::secondary)
        .on_press_maybe(live.then(|| send(Action::Play(!settings.playing))));
        controls = controls.push(
            row![
                play,
                accessible_checkbox(
                    "xtb-vibration-arrows",
                    "Show arrows",
                    settings.arrows,
                    live.then(|| send(Action::Arrows(!settings.arrows))),
                ),
                accessible_command(
                    "xtb-vibration-place-arrows",
                    "Add the displacement arrows to the drawing",
                    "Add to drawing",
                )
                .on_press_maybe((live && settings.arrows).then(|| send(Action::PlaceArrows))),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
        controls = controls.push(labelled_slider(
            "Amplitude",
            format!("{:.2} Å", settings.amplitude),
            slider(0.1..=0.6, settings.amplitude, |v| {
                send(Action::Amplitude(v))
            })
            .step(0.05_f32),
        ));
        controls = controls.push(labelled_slider(
            "Speed",
            format!("{:.2}×", settings.speed),
            slider(0.25..=2., settings.speed, |v| send(Action::Speed(v))).step(0.25_f32),
        ));
        controls.push(
            accessible_command(
                "xtb-vibration-export-xyz",
                "Export the selected mode as a multi-frame XYZ movie",
                "Export mode as XYZ movie…",
            )
            .on_press_maybe(modes.then(|| send(Action::ExportXyz))),
        )
    }
}

fn labelled_slider<'a>(
    name: &'static str,
    value: String,
    slider: iced::widget::Slider<'a, f32, Message>,
) -> Element<'a, Message> {
    row![
        text(name).size(12).width(76),
        slider.width(Length::Fill),
        text(value).size(12).style(muted_text).width(52),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// "1590 cm⁻¹ · IR 127.7", imaginary modes first in red with a chip.
fn mode_list<'a>(result: &CalcResult, selected: usize, stale: bool) -> Column<'a, Message> {
    let Some(vibrations) = &result.vibrations else {
        return column![];
    };
    Column::with_children(vibrations.frequencies_cm1.iter().enumerate().map(
        |(index, &frequency)| {
            let imaginary = frequency < 0.;
            let mut label = wavenumber(frequency);
            if imaginary {
                label.push_str(" · imaginary");
            } else if let Some(ir) = vibrations.ir_intensities_km_mol.get(index) {
                label.push_str(&format!(" · IR {ir:.1}"));
            }
            let chip: Element<'a, Message> = if imaginary {
                container(text("not a minimum").size(10))
                    .padding([0, 4])
                    .style(|theme: &Theme| container::Style {
                        text_color: Some(theme.palette().danger),
                        background: Some(
                            Color {
                                a: 0.1,
                                ..theme.palette().danger
                            }
                            .into(),
                        ),
                        border: iced::Border {
                            radius: 3.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
            } else {
                text(format!("{}", index + 1))
                    .size(11)
                    .style(muted_text)
                    .into()
            };
            let active = index == selected;
            reshiki::accessibility::button(
                format!("xtb-mode-{}", index + 1),
                format!("Mode {}, {label}", index + 1),
                row![
                    text(label)
                        .size(12)
                        .width(Length::Fill)
                        .style(move |theme: &Theme| text::Style {
                            color: Some(if imaginary {
                                theme.palette().danger
                            } else if stale {
                                Color {
                                    a: 0.55,
                                    ..crate::appearance::muted(theme)
                                }
                            } else {
                                theme.palette().text
                            }),
                        }),
                    chip,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([3, 8])
            .width(Length::Fill)
            .checked(active)
            .style(control(active))
            .on_press(send(Action::Mode(index)))
            .into()
        },
    ))
    .spacing(2)
    .padding(iced::Padding::ZERO.right(10))
}

/// The collapsed Thermochemistry block: T and p fields and the values at them.
fn thermochemistry<'a>(settings: &'a State, shown: &Shown<'a>) -> Element<'a, Message> {
    let toggle = accessible_command(
        "xtb-thermochemistry",
        "Show or hide the thermochemistry",
        if settings.thermo {
            "Thermochemistry ▾"
        } else {
            "Thermochemistry ▸"
        },
    )
    .expanded(settings.thermo)
    .on_press(send(Action::Thermo));
    let result = &shown.stored.result;
    let Some(saved) = result.thermo.as_ref().filter(|_| settings.thermo) else {
        return toggle.into();
    };
    let field = |id: &'static str,
                 name: &'static str,
                 unit: &'static str,
                 value: &str,
                 on: fn(String) -> Action| {
        row![
            text(name).size(12).width(24),
            reshiki::accessibility::text_input(id, name, "", value)
                .on_input(move |value| send(on(value)))
                .size(12)
                .padding(4)
                .width(80),
            text(unit).size(12).style(muted_text),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    let mut block = column![
        toggle,
        field(
            "xtb-thermo-temperature",
            "T",
            "K",
            &settings.temperature,
            Action::Temperature
        ),
        field(
            "xtb-thermo-pressure",
            "p",
            "atm",
            &settings.pressure,
            Action::Pressure
        ),
    ]
    .spacing(5);
    let thermo = settings.conditions().and_then(|(kelvin, pascal)| {
        let same = (kelvin - saved.temperature_k).abs() < 1e-9
            && (pascal - saved.pressure_pa).abs() < 1e-6;
        if same {
            Some(saved.clone())
        } else {
            thermo_at(result, shown.frame, kelvin, pascal)
        }
    });
    let Some(thermo) = thermo else {
        let why = if settings.conditions().is_none() {
            "Enter a positive temperature and pressure.".to_owned()
        } else {
            format!(
                "This result's thermochemistry can be shown only at its saved conditions ({:.2} K, {:.3} atm).",
                saved.temperature_k,
                saved.pressure_pa / super::PA_PER_ATM,
            )
        };
        return block.push(line(why, Tone::Warning)).into();
    };
    for (name, value) in thermo_rows(&thermo, result.energy_hartree) {
        block = block.push(
            row![
                text(name).size(12).style(muted_text).width(104),
                text(value).size(12),
            ]
            .spacing(8),
        );
    }
    block
        .push(line(
            format!(
                "Quasi-RRHO (Grimme 2012){}: modes below {} cm⁻¹ blend into free \
                 rotors; imaginary modes above −{} cm⁻¹ count as real, the others are left out.",
                if super::super::results::thermo_defaults(&thermo) {
                    ", xtb's defaults"
                } else {
                    ""
                },
                thermo.rotor_cutoff_cm1,
                thermo.imaginary_cutoff_cm1.abs(),
            ),
            Tone::Muted,
        ))
        .into()
}

/// ZPVE, H, S, G, Cv, point group and symmetry number.
pub(super) fn thermo_rows(thermo: &Thermo, energy: Option<f64>) -> Vec<(&'static str, String)> {
    let total = |correction: f64| match energy {
        Some(energy) => format!("{} Eh", signed(energy + correction, 6)),
        None => format!("E + {} Eh", signed(correction, 6)),
    };
    let molar = |per_k: f64| format!("{:.2} cal/(mol·K)", per_k * CAL_PER_MOL_PER_HARTREE);
    let mut rows = vec![
        (
            "ZPVE",
            format!("{} Eh", signed(thermo.zero_point_energy_hartree, 6)),
        ),
        ("H", total(thermo.enthalpy_correction_hartree)),
        ("S", molar(thermo.entropy_hartree_per_k)),
        ("G", total(thermo.gibbs_correction_hartree)),
    ];
    if let Some(cv) = thermo.heat_capacity_hartree_per_k {
        rows.push(("Cp", molar(cv)));
    }
    if let Some(group) = &thermo.point_group {
        rows.push(("Point group", super::super::results::point_group(group)));
    }
    rows.push(("Symmetry number", thermo.symmetry_number.to_string()));
    rows
}

/// Band shape, FWHM and scale factor under the spectrum.
fn spectrum_controls(settings: &State) -> Element<'_, Message> {
    let shapes = row([
        (BandShape::Lorentzian, "Lorentzian"),
        (BandShape::Gaussian, "Gaussian"),
    ]
    .map(|(shape, name)| {
        let active = settings.shape == shape;
        reshiki::accessibility::button(
            format!("xtb-ir-{}", name.to_lowercase()),
            format!("{name} bands"),
            text(name).size(12),
        )
        .padding([4, 8])
        .checked(active)
        .style(control(active))
        .on_press(send(Action::Shape(shape)))
        .into()
    }))
    .spacing(4);
    let scale = row![
        text("Scale factor").size(12),
        reshiki::accessibility::text_input("xtb-ir-scale", "Scale factor", "1.00", &settings.scale)
            .on_input(|value| send(Action::Scale(value)))
            .size(12)
            .padding(4)
            .width(56),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    column![
        shapes,
        labelled_slider(
            "FWHM",
            format!("{:.0} cm⁻¹", settings.fwhm),
            slider(5.0..=60., settings.fwhm, |v| send(Action::Fwhm(v))).step(1_f32),
        ),
        scale,
    ]
    .spacing(7)
    .into()
}

/// Broadening depends on the result and spectrum controls, independently of
/// which mode is highlighted or where its animation is in the period.
pub(super) struct SpectrumKey {
    result: Weak<CalcResult>,
    shape: BandShape,
    fwhm_bits: u32,
    scale_bits: u64,
}

impl PartialEq for SpectrumKey {
    fn eq(&self, other: &Self) -> bool {
        self.result.ptr_eq(&other.result)
            && self.shape == other.shape
            && self.fwhm_bits == other.fwhm_bits
            && self.scale_bits == other.scale_bits
    }
}

pub(super) struct SpectrumData {
    /// Scaled real frequencies and their mode indices.
    sticks: Vec<(usize, f64, f64)>,
    curve: Vec<[f64; 2]>,
    peaks: Vec<[f64; 2]>,
    high: f64,
    low: f64,
    unplottable: bool,
}

impl SpectrumData {
    fn new(result: &CalcResult, shape: BandShape, fwhm: f32, scale: f64) -> Self {
        let mut data = Self {
            sticks: vec![],
            curve: vec![],
            peaks: vec![],
            high: 4000.,
            low: 400.,
            unplottable: false,
        };
        let Some(vibrations) = &result.vibrations else {
            return data;
        };
        let frequencies = &vibrations.frequencies_cm1;
        let intensities = &vibrations.ir_intensities_km_mol;
        let highest = frequencies
            .iter()
            .map(|frequency| frequency * scale)
            .filter(|frequency| frequency.is_finite())
            .fold(0., f64::max);
        data.high = if highest > 3900. {
            (((highest.min(MAX_WAVENUMBER) + 100.) / 500.).ceil() * 500.).min(MAX_WAVENUMBER)
        } else {
            4000.
        };
        data.low = 400_f64
            .min(
                frequencies
                    .iter()
                    .map(|f| f * scale)
                    .filter(|f| f.is_finite() && *f >= 0.)
                    .fold(f64::INFINITY, f64::min)
                    - 50.,
            )
            .max(0.);
        data.unplottable = frequencies.iter().any(|frequency| {
            let scaled = frequency * scale;
            !scaled.is_finite() || scaled > MAX_WAVENUMBER
        });
        if data.unplottable {
            return data;
        }
        let fwhm = f64::from(fwhm);
        data.curve = vibrational::ir_spectrum(
            frequencies,
            intensities,
            scale,
            fwhm,
            shape,
            (data.high, data.low),
            900,
        );
        data.peaks = vibrational::ir_peaks(
            frequencies,
            intensities,
            scale,
            fwhm,
            shape,
            LABEL_FRACTION,
            LABELS,
        );
        data.sticks = frequencies
            .iter()
            .enumerate()
            .filter(|(_, f)| f.is_finite() && **f >= 0.)
            .map(|(index, f)| {
                (
                    index,
                    f * scale,
                    intensities.get(index).copied().unwrap_or(0.),
                )
            })
            .collect();
        data
    }

    fn message(&self) -> Option<&'static str> {
        if self.unplottable {
            Some(UNPLOTTABLE)
        } else if !self.curve.iter().any(|[_, value]| *value > 0.) {
            Some("No IR intensities in this result")
        } else {
            None
        }
    }
}

/// A bounded axis generator also tolerates ranges much larger than the chart
/// permits, without a floating-point subtraction loop that can stop advancing.
fn spectrum_ticks(high: f64, low: f64) -> Vec<f64> {
    let span = high - low;
    if !high.is_finite() || !low.is_finite() || !span.is_finite() || span < 0. {
        return Vec::new();
    }
    let base_step = if span > 2000. { 500. } else { 250. };
    let step = (span / (MAX_TICKS - 1) as f64 / base_step).ceil().max(1.) * base_step;
    let first = (high / step).floor() * step;
    (0..MAX_TICKS)
        .map(|index| first - index as f64 * step)
        .take_while(|tick| tick.is_finite() && *tick >= low && *tick <= high)
        .collect()
}

/// The broadened IR spectrum, wavenumber decreasing to the right; a click
/// picks the nearest mode.
struct Spectrum {
    data: Arc<SpectrumData>,
    /// The selected mode's scaled frequency, or `None` for an imaginary one.
    selected: Option<f64>,
    imaginary_selected: bool,
}

impl Spectrum {
    const LEFT: f32 = 14.;
    const RIGHT: f32 = 10.;
    const TOP: f32 = 18.;
    const BOTTOM: f32 = 30.;

    fn new(result: &Arc<CalcResult>, settings: &State, selected: usize) -> Self {
        let scale = settings.scale().unwrap_or(1.);
        let key = SpectrumKey {
            result: Arc::downgrade(result),
            shape: settings.shape,
            fwhm_bits: settings.fwhm.to_bits(),
            scale_bits: scale.to_bits(),
        };
        let data = {
            let mut cache = settings.spectrum.borrow_mut();
            match cache.as_ref() {
                Some((cached_key, data)) if *cached_key == key => Arc::clone(data),
                _ => {
                    let data = Arc::new(SpectrumData::new(
                        result,
                        settings.shape,
                        settings.fwhm,
                        scale,
                    ));
                    *cache = Some((key, Arc::clone(&data)));
                    data
                }
            }
        };
        let frequency = result
            .vibrations
            .as_ref()
            .and_then(|vibrations| vibrations.frequencies_cm1.get(selected))
            .copied();
        Self {
            data,
            selected: frequency
                .filter(|f| f.is_finite() && *f >= 0.)
                .map(|f| f * scale),
            imaginary_selected: frequency.is_some_and(|f| f < 0.),
        }
    }

    fn x(&self, wavenumber: f64, bounds: Rectangle) -> f32 {
        let width = bounds.width - Self::LEFT - Self::RIGHT;
        Self::LEFT
            + ((self.data.high - wavenumber) / (self.data.high - self.data.low)) as f32 * width
    }
}

impl canvas::Program<Message> for Spectrum {
    type State = ();

    fn update(
        &self,
        _: &mut (),
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event else {
            return None;
        };
        let point = cursor.position_in(bounds)?;
        let (index, _, _) = self.data.sticks.iter().min_by(|a, b| {
            (self.x(a.1, bounds) - point.x)
                .abs()
                .total_cmp(&(self.x(b.1, bounds) - point.x).abs())
        })?;
        Some(canvas::Action::publish(send(Action::Mode(*index))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if !self.data.sticks.is_empty() && cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let muted = crate::appearance::muted(theme);
        let ink = theme.palette().text;
        let accent = theme.palette().primary;
        let base = bounds.height - Self::BOTTOM;
        let height = base - Self::TOP;
        let width = bounds.width - Self::LEFT - Self::RIGHT;
        frame.stroke(
            &canvas::Path::rectangle(
                Point::new(Self::LEFT, Self::TOP),
                iced::Size::new(width, height),
            ),
            canvas::Stroke::default()
                .with_color(Color { a: 0.4, ..muted })
                .with_width(1.),
        );
        let small = |content: String,
                     position: Point,
                     color: Color,
                     align: iced::alignment::Horizontal| canvas::Text {
            content,
            position,
            color,
            size: 10.into(),
            align_x: align.into(),
            ..Default::default()
        };
        // A narrow chart labels fewer ticks: each label needs its width
        // (about 6 px a digit at 10 px) clear of the previous one.
        let mut last_right = f32::NEG_INFINITY;
        for tick in spectrum_ticks(self.data.high, self.data.low) {
            let x = self.x(tick, bounds);
            frame.stroke(
                &canvas::Path::line(Point::new(x, base), Point::new(x, base + 4.)),
                canvas::Stroke::default().with_color(muted).with_width(1.),
            );
            let half = format!("{tick:.0}").len() as f32 * 3.;
            let major = (tick / 1000.).fract() == 0. || self.data.high - self.data.low <= 2000.;
            if major && x - half > last_right + 6. {
                last_right = x + half;
                frame.fill_text(small(
                    format!("{tick:.0}"),
                    Point::new(x, base + 6.),
                    muted,
                    iced::alignment::Horizontal::Center,
                ));
            }
        }
        frame.fill_text(small(
            "Wavenumber (cm⁻¹)".into(),
            Point::new(Self::LEFT + width / 2., bounds.height - 12.),
            muted,
            iced::alignment::Horizontal::Center,
        ));
        if let Some(message) = self.data.message() {
            frame.fill_text(small(
                message.into(),
                Point::new(Self::LEFT + width / 2., Self::TOP + height / 2.),
                muted,
                iced::alignment::Horizontal::Center,
            ));
            return vec![frame.into_geometry()];
        }
        let top = self
            .data
            .curve
            .iter()
            .map(|[_, value]| *value)
            .fold(0., f64::max);
        let y = |value: f64| base - (value / top * 0.92) as f32 * height;
        let tallest = self
            .data
            .sticks
            .iter()
            .map(|s| s.2)
            .fold(0., f64::max)
            .max(1e-12);
        for &(_, frequency, intensity) in &self.data.sticks {
            let chosen = self.selected == Some(frequency);
            let x = self.x(frequency, bounds);
            let stick_top = base - (intensity / tallest * 0.92) as f32 * height;
            frame.stroke(
                &canvas::Path::line(Point::new(x, base), Point::new(x, stick_top)),
                canvas::Stroke::default()
                    .with_color(if chosen {
                        accent
                    } else {
                        Color { a: 0.35, ..muted }
                    })
                    .with_width(if chosen { 2. } else { 1. }),
            );
        }
        frame.stroke(
            &canvas::Path::new(|path| {
                for (index, [wavenumber, value]) in self.data.curve.iter().enumerate() {
                    let point = Point::new(self.x(*wavenumber, bounds), y(*value));
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            }),
            canvas::Stroke::default().with_color(ink).with_width(1.4),
        );
        // The selected mode's label and the imaginary note win; then the
        // tallest peaks, each clear of the text already placed (horizontal
        // extents at the chart's 10 px text, about 6 px a character).
        let extent = |text: &str| text.chars().count() as f32 * 6.;
        let mut placed: Vec<(f32, f32)> = Vec::new();
        let free = |placed: &[(f32, f32)], (lo, hi): (f32, f32)| {
            placed.iter().all(|&(a, b)| hi + 4. < a || lo > b + 4.)
        };
        if let Some(frequency) = self.selected {
            let x = self.x(frequency, bounds);
            let label = format!("{frequency:.0} cm⁻¹");
            let width = extent(&label);
            let left = x + 4. + width < bounds.width - Self::RIGHT;
            let (lo, hi) = if left {
                (x + 4., x + 4. + width)
            } else {
                (x - 4. - width, x - 4.)
            };
            frame.fill_text(small(
                label,
                Point::new(if left { lo } else { hi }, Self::TOP + 2.),
                accent,
                if left {
                    iced::alignment::Horizontal::Left
                } else {
                    iced::alignment::Horizontal::Right
                },
            ));
            placed.push((lo.min(x), hi.max(x)));
        }
        if self.imaginary_selected {
            let note = "imaginary mode: not plotted";
            // Top left: high wavenumbers rarely carry peak labels.
            frame.fill_text(small(
                note.into(),
                Point::new(Self::LEFT + 4., Self::TOP + 2.),
                theme.palette().danger,
                iced::alignment::Horizontal::Left,
            ));
            placed.push((Self::LEFT + 4., Self::LEFT + 4. + extent(note)));
        }
        let mut peaks: Vec<&[f64; 2]> = self.data.peaks.iter().collect();
        peaks.sort_by(|a, b| b[1].total_cmp(&a[1]));
        for [wavenumber, value] in peaks {
            let x = self.x(*wavenumber, bounds);
            let label = format!("{wavenumber:.0}");
            let half = extent(&label) / 2.;
            if !free(&placed, (x - half, x + half)) {
                continue;
            }
            placed.push((x - half, x + half));
            frame.fill_text(small(
                label,
                Point::new(x, (y(*value) - 12.).max(Self::TOP + 2.)),
                ink,
                iced::alignment::Horizontal::Center,
            ));
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::xtb::tests::{START, optfreq};
    use reshiki::qcdata::{Frame, Task};

    fn spectrum_result() -> CalcResult {
        let frame = Frame {
            numbers: vec![8, 1, 1],
            positions_angstrom: START.to_vec(),
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        };
        let report = optfreq(&START, &START);
        report
            .computed
            .to_calc_result(&frame, Task::Hessian, &report.engine, None)
            .unwrap()
    }

    #[test]
    fn enormous_or_nonfinite_frequencies_construct_a_bounded_spectrum_without_a_curve() {
        for frequency in [1e20, f64::INFINITY, f64::NAN] {
            let mut result = spectrum_result();
            let vibrations = result.vibrations.as_mut().unwrap();
            vibrations.frequencies_cm1 = vec![100., frequency];
            vibrations.ir_intensities_km_mol = vec![1., 1.];
            let spectrum = Spectrum::new(&Arc::new(result), &State::default(), 0);
            assert!(spectrum.data.curve.is_empty());
            assert!(spectrum.data.peaks.is_empty());
            assert_eq!(
                spectrum.data.message(),
                Some("Frequencies outside 0–10000 cm⁻¹ cannot be plotted")
            );
            assert!(spectrum.data.high.is_finite());
            assert!(spectrum.data.low.is_finite());
            assert!(spectrum.data.high <= MAX_WAVENUMBER);
            assert!(spectrum_ticks(spectrum.data.high, spectrum.data.low).len() <= MAX_TICKS);
        }
    }

    #[test]
    fn the_plot_limit_applies_to_scaled_frequencies_and_includes_ten_thousand() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![0., MAX_WAVENUMBER];
        vibrations.ir_intensities_km_mol = vec![1., 1.];
        let result = Arc::new(result);
        let mut settings = State::default();
        let at_limit = Spectrum::new(&result, &settings, 1);
        assert_eq!(at_limit.data.high, MAX_WAVENUMBER);
        assert_eq!(at_limit.data.low, 0.);
        assert_eq!(at_limit.data.message(), None);
        assert!(!at_limit.data.curve.is_empty());
        settings.scale = "1.01".into();
        let over_limit = Spectrum::new(&result, &settings, 1);
        assert!(over_limit.data.curve.is_empty());
        assert_eq!(over_limit.data.message(), Some(UNPLOTTABLE));
    }

    #[test]
    fn axis_ticks_are_bounded_even_when_five_hundred_no_longer_changes_the_frequency() {
        let ticks = spectrum_ticks(1e20, 0.);
        assert!(!ticks.is_empty());
        assert!(ticks.len() <= MAX_TICKS);
        assert!(ticks.iter().all(|tick| tick.is_finite()));
        assert!(ticks.windows(2).all(|pair| {
            let [left, right] = pair else {
                return false;
            };
            left > right
        }));
        assert!(spectrum_ticks(f64::INFINITY, 0.).is_empty());
    }

    #[test]
    fn spectrum_cache_reuses_broadening_for_modes_and_rebuilds_for_spectrum_controls() {
        let result = Arc::new(spectrum_result());
        let mut settings = State::default();
        let first = Spectrum::new(&result, &settings, 0);
        let other_mode = Spectrum::new(&result, &settings, 1);
        assert!(Arc::ptr_eq(&first.data, &other_mode.data));
        assert_ne!(first.selected, other_mode.selected);

        settings.shape = BandShape::Gaussian;
        let gaussian = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&first.data, &gaussian.data));
        settings.fwhm = 20.;
        let wider = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&gaussian.data, &wider.data));
        settings.scale = "0.95".into();
        let scaled = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&wider.data, &scaled.data));
        let other_result = Arc::new(spectrum_result());
        let different = Spectrum::new(&other_result, &settings, 1);
        assert!(!Arc::ptr_eq(&scaled.data, &different.data));
    }
}
