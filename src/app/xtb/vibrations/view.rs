//! The Vibrations tab's widgets: the imaginary-mode banner, the mode list,
//! thermochemistry, the IR spectrum and the animation controls.
use super::super::palette::Shown;
use super::super::state::signed;
use super::super::tone::{Tone, line};
use super::super::{Action as XtbAction, unavailable};
use super::mode_list::mode_list;
use super::{Action, State, thermo_at};
use crate::app::workspace::{accessible_checkbox, accessible_command, control, muted_text};
use crate::app::{App, Message};
use iced::widget::{Column, canvas, column, container, row, slider, text};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::graphics::YAxis;
use reshiki::qcdata::vibrational::{self, BandShape};
use reshiki::qcdata::{CalcResult, Thermo};
use std::sync::{Arc, Weak};

/// cal/mol per hartree.
const CAL_PER_MOL_PER_HARTREE: f64 = 627_509.474;
const LIST_WIDTH: f32 = 258.;
const CHART_WIDTH: f32 = 480.;
const SPECTRUM_CONTROLS_WIDTH: f32 = 280.;
/// Chart height in the inspector; the bottom dock's body is shorter.
const CHART_HEIGHT: f32 = 156.;
const DOCK_CHART_HEIGHT: f32 = 128.;
/// Peaks labelled on the spectrum: at most this many, at least this fraction of the tallest.
const LABELS: usize = 8;
const LABEL_FRACTION: f64 = 0.05;
const CHART_FONT_SIZE: f32 = 10.;
const MAX_TICKS: usize = 40;
const RANGE_ERROR: &str = "Use 0–10000 cm⁻¹ with From at least 200 above To.";
const SCALE_ERROR: &str = "Use a scale factor from 0.5 to 1.5.";
const WAVENUMBER_TITLE: &str = "Wavenumber (cm⁻¹)";

pub(super) fn send(action: Action) -> Message {
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
        let names = self.xtb_motion_names(shown);
        let list = mode_list(
            vibrations,
            settings,
            names.as_deref().unwrap_or_default(),
            settings.recomputing(result),
            selected,
            shown.stale,
            inspector,
        );
        let left = column![list, thermochemistry(settings, shown)]
            .spacing(8)
            .width(if inspector {
                Length::Fill
            } else {
                Length::Fixed(LIST_WIDTH)
            });
        let spectrum = Spectrum::new(result, settings, selected);
        let spectrum_controls = spectrum_controls(settings, spectrum.data.can_plot());
        let chart = canvas(spectrum).width(Length::Fill);
        let controls = self.xtb_animation_controls(shown);
        if inspector {
            body = body
                .push(left)
                .push(chart.height(CHART_HEIGHT))
                .push(spectrum_controls)
                .push(controls);
        } else {
            // Everything fits the dock's short body: list, chart, then the
            // animation and spectrum controls side by side.
            body = body.push(
                row![
                    left,
                    container(chart.height(DOCK_CHART_HEIGHT))
                        .width(Length::FillPortion(5))
                        .max_width(CHART_WIDTH),
                    controls.width(Length::FillPortion(3)),
                    container(spectrum_controls).width(SPECTRUM_CONTROLS_WIDTH),
                ]
                .spacing(18)
                .align_y(Alignment::Start),
            );
        }
        body.into()
    }

    /// Warn about imaginary frequencies and offer
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
            .align_y(Alignment::Center)
            .wrap()
            .vertical_spacing(4),
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

/// The spectrum settings and commands.
fn spectrum_controls(settings: &State, enabled: bool) -> Element<'_, Message> {
    let axes = row([
        (YAxis::Transmittance, "transmittance", "Transmittance (%T)"),
        (YAxis::Absorbance, "absorbance", "Absorbance"),
    ]
    .map(|(axis, id, name)| {
        let active = settings.y == axis;
        reshiki::accessibility::button(format!("xtb-ir-{id}"), name, text(name).size(12))
            .padding([2, 6])
            .checked(active)
            .style(control(active))
            .on_press(send(Action::Y(axis)))
            .into()
    }))
    .spacing(4);
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
        .padding([2, 6])
        .checked(active)
        .style(control(active))
        .on_press(send(Action::Shape(shape)))
        .into()
    }))
    .spacing(4);
    let range = row![
        text("From").size(12),
        reshiki::accessibility::text_input(
            "xtb-ir-from",
            "From wavenumber (cm⁻¹)",
            "4000",
            &settings.from
        )
        .on_input(|value| send(Action::From(value)))
        .size(12)
        .padding(2)
        .width(56),
        text("to").size(12),
        reshiki::accessibility::text_input(
            "xtb-ir-to",
            "To wavenumber (cm⁻¹)",
            "400",
            &settings.to
        )
        .on_input(|value| send(Action::To(value)))
        .size(12)
        .padding(2)
        .width(56),
        text("cm⁻¹").size(12).style(muted_text),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let bands = row![
        text("FWHM").size(12),
        slider(5.0..=60., settings.fwhm, |v| send(Action::Fwhm(v)))
            .step(1_f32)
            .width(Length::Fill),
        text(format!("{:.0} cm⁻¹", settings.fwhm))
            .size(12)
            .style(muted_text)
            .width(44),
        text("Scale").size(12),
        reshiki::accessibility::text_input(
            "xtb-ir-scale",
            "Scale factor",
            "1.000",
            &settings.scale
        )
        .on_input(|value| send(Action::Scale(value)))
        .size(12)
        .padding(2)
        .width(56),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let command = |id: &'static str, label: &'static str, action| {
        accessible_command(id, label, label)
            .padding([2, 6])
            .on_press_maybe(enabled.then(|| send(action)))
    };
    let mut controls = column![axes, shapes, range].spacing(4);
    if settings.range().is_none() {
        controls = controls.push(line(RANGE_ERROR, Tone::Error));
    }
    controls = controls.push(bands);
    if settings.scale().is_none() {
        controls = controls.push(line(SCALE_ERROR, Tone::Error));
    }
    controls
        .push(
            column![
                row![
                    command("xtb-ir-place", "Place in drawing", Action::PlaceSpectrum),
                    command("xtb-ir-copy-caption", "Copy caption", Action::CopyCaption),
                ]
                .spacing(4),
                row![
                    command(
                        "xtb-ir-export-svg",
                        "Export SVG…",
                        Action::ExportSpectrum("svg")
                    ),
                    command(
                        "xtb-ir-export-pdf",
                        "Export PDF…",
                        Action::ExportSpectrum("pdf")
                    ),
                ]
                .spacing(4),
            ]
            .spacing(4),
        )
        .into()
}

/// Broadening depends on the result and spectrum controls, independently of
/// which mode is highlighted or where its animation is in the period.
pub(super) struct SpectrumKey {
    result: Weak<CalcResult>,
    shape: BandShape,
    fwhm_bits: u32,
    scale_bits: Option<u64>,
    y: YAxis,
    range_bits: Option<(u64, u64)>,
}

impl PartialEq for SpectrumKey {
    fn eq(&self, other: &Self) -> bool {
        self.result.ptr_eq(&other.result)
            && self.shape == other.shape
            && self.fwhm_bits == other.fwhm_bits
            && self.scale_bits == other.scale_bits
            && self.y == other.y
            && self.range_bits == other.range_bits
    }
}

pub(super) struct SpectrumData {
    /// Scaled real frequencies in the displayed range and their mode indices.
    sticks: Vec<(usize, f64, f64)>,
    curve: Vec<[f64; 2]>,
    peaks: Vec<[f64; 2]>,
    high: f64,
    low: f64,
    shape: BandShape,
    fwhm: f64,
    y: YAxis,
    settings_error: Option<&'static str>,
    has_ir: bool,
    intensities_missing: bool,
    scale: f64,
}

impl SpectrumData {
    pub(super) fn cached(result: &Arc<CalcResult>, settings: &State) -> Arc<Self> {
        let scale = settings.scale();
        let range = settings.range();
        let key = SpectrumKey {
            result: Arc::downgrade(result),
            shape: settings.shape,
            fwhm_bits: settings.fwhm.to_bits(),
            scale_bits: scale.map(f64::to_bits),
            y: settings.y,
            range_bits: range.map(|(high, low)| (high.to_bits(), low.to_bits())),
        };
        let mut cache = settings.spectrum.borrow_mut();
        if let Some((cached_key, data)) = cache.as_ref()
            && *cached_key == key
        {
            return Arc::clone(data);
        }
        let data = Arc::new(Self::new(result, settings, scale, range));
        *cache = Some((key, Arc::clone(&data)));
        data
    }

    fn new(
        result: &CalcResult,
        settings: &State,
        scale: Option<f64>,
        range: Option<(f64, f64)>,
    ) -> Self {
        let (high, low) = range.unwrap_or((4000., 400.));
        let fwhm = f64::from(settings.fwhm);
        let shape = settings.shape;
        let mut data = Self {
            sticks: vec![],
            curve: vec![],
            peaks: vec![],
            high,
            low,
            shape,
            fwhm,
            y: settings.y,
            settings_error: if range.is_none() {
                Some(RANGE_ERROR)
            } else if scale.is_none() {
                Some(SCALE_ERROR)
            } else {
                None
            },
            has_ir: false,
            intensities_missing: true,
            scale: scale.unwrap_or(1.),
        };
        let scale = data.scale;
        let Some(vibrations) = &result.vibrations else {
            return data;
        };
        let frequencies = &vibrations.frequencies_cm1;
        let intensities = &vibrations.ir_intensities_km_mol;
        data.intensities_missing = intensities.is_empty();
        data.sticks = frequencies
            .iter()
            .enumerate()
            .filter(|(_, f)| data.contains(**f * scale))
            .map(|(index, f)| {
                (
                    index,
                    f * scale,
                    intensities.get(index).copied().unwrap_or(0.),
                )
            })
            .collect();
        data.has_ir = intensities.iter().any(|&intensity| intensity > 0.);
        if !data.has_ir {
            return data;
        }
        data.curve = vibrational::ir_spectrum(
            frequencies,
            intensities,
            scale,
            fwhm,
            shape,
            (data.high, data.low),
            900,
        );
        data.peaks =
            vibrational::ir_peaks(frequencies, intensities, scale, fwhm, shape, 0., usize::MAX)
                .into_iter()
                .filter(|[frequency, _]| data.contains(*frequency))
                .collect();
        // Rank and normalize within the visible range, including refined
        // maxima that a uniform grid can miss between samples.
        data.curve.extend_from_slice(&data.peaks);
        data.curve.sort_by(|a, b| b[0].total_cmp(&a[0]));
        data.curve.dedup_by(|a, b| a[0] == b[0]);
        let top = data.top();
        data.peaks
            .retain(|[_, value]| *value >= LABEL_FRACTION * top);
        data.peaks.sort_by(|a, b| b[1].total_cmp(&a[1]));
        data.peaks.truncate(LABELS);
        data.peaks.sort_by(|a, b| b[0].total_cmp(&a[0]));
        data
    }

    fn pickable_sticks(&self) -> impl Iterator<Item = &(usize, f64, f64)> {
        // Results without IR data still show selectable frequency sticks.
        self.sticks
            .iter()
            .filter(|stick| self.intensities_missing || stick.2 > 0.)
    }

    fn message(&self) -> Option<&'static str> {
        if let Some(error) = self.settings_error {
            Some(error)
        } else if !self.has_ir {
            Some("No IR intensities in this result")
        } else if self
            .curve
            .iter()
            .chain(&self.peaks)
            .any(|[_, value]| !value.is_finite())
        {
            Some("IR spectrum cannot be plotted")
        } else if !self.curve.iter().any(|[_, value]| *value > 0.) {
            Some("No IR bands in this range")
        } else {
            None
        }
    }

    pub(super) fn can_plot(&self) -> bool {
        self.message().is_none()
    }

    fn contains(&self, frequency: f64) -> bool {
        (self.low..=self.high).contains(&frequency)
    }

    fn top(&self) -> f64 {
        self.curve
            .iter()
            .map(|[_, value]| *value)
            .fold(0., f64::max)
    }

    fn x(&self, wavenumber: f64) -> f32 {
        ((self.high - wavenumber) / (self.high - self.low)) as f32
    }

    fn ticks(&self) -> Vec<f64> {
        spectrum_ticks(self.high, self.low)
    }

    fn labelled_peaks(
        &self,
        x: impl Fn(f64) -> f32,
        font_size: f32,
        mut placed: Vec<(f32, f32)>,
    ) -> impl Iterator<Item = [f64; 2]> {
        let mut peaks = self.peaks.clone();
        peaks.sort_by(|a, b| b[1].total_cmp(&a[1]));
        let padding = 0.4 * font_size;
        let (left, right) = (x(self.high), x(self.low));
        peaks.into_iter().filter(move |[wavenumber, _]| {
            let x = x(*wavenumber);
            let half = text_extent(&format!("{wavenumber:.0}"), font_size) / 2.;
            let (lo, hi) = (x - half, x + half);
            if lo < left || hi > right {
                return false;
            }
            if !placed
                .iter()
                .all(|&(a, b)| hi + padding < a || lo > b + padding)
            {
                return false;
            }
            placed.push((lo, hi));
            true
        })
    }
}

fn text_extent(text: &str, font_size: f32) -> f32 {
    crate::app::workspace::text_width(text, font_size)
}

/// A bounded axis generator also tolerates ranges much larger than the chart
/// permits, without a floating-point subtraction loop that can stop advancing.
fn spectrum_ticks(high: f64, low: f64) -> Vec<f64> {
    let span = high - low;
    if !high.is_finite() || !low.is_finite() || !span.is_finite() || span < 0. {
        return Vec::new();
    }
    let base_step = if span > 2000. {
        500.
    } else if span > 500. {
        250.
    } else {
        100.
    };
    let step = (span / (MAX_TICKS - 1) as f64 / base_step).ceil().max(1.) * base_step;
    let first = (high / step).floor() * step;
    (0..MAX_TICKS)
        .map(|index| first - index as f64 * step)
        .take_while(|tick| tick.is_finite() && *tick >= low && *tick <= high)
        .collect()
}

/// The broadened IR spectrum, wavenumber decreasing to the right; a click
/// picks the mode contributing most to the band.
struct Spectrum {
    data: Arc<SpectrumData>,
    left: f32,
    right: f32,
    /// The selected mode's scaled frequency when it is in the visible range.
    selected: Option<f64>,
    imaginary_selected: bool,
}

impl Spectrum {
    const LEFT: f32 = 14.;
    const RIGHT: f32 = 10.;
    const TOP: f32 = 18.;
    const BOTTOM: f32 = 30.;

    fn new(result: &Arc<CalcResult>, settings: &State, selected: usize) -> Self {
        let data = SpectrumData::cached(result, settings);
        let scale = data.scale;
        let frequency = result
            .vibrations
            .as_ref()
            .and_then(|vibrations| vibrations.frequencies_cm1.get(selected))
            .copied();
        let selected = frequency.map(|f| f * scale).filter(|f| data.contains(*f));
        let half_label = |value| text_extent(&format!("{value:.0}"), CHART_FONT_SIZE) / 2. + 2.;
        let ticks = data.ticks();
        let left = Self::LEFT.max(ticks.first().copied().map_or(0., half_label));
        let right = Self::RIGHT.max(ticks.last().copied().map_or(0., half_label));
        Self {
            data,
            left,
            right,
            selected,
            imaginary_selected: frequency.is_some_and(|f| f < 0.),
        }
    }

    fn plot(&self, bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: self.left,
            y: Self::TOP,
            width: (bounds.width - self.left - self.right).max(0.),
            height: (bounds.height - Self::TOP - Self::BOTTOM).max(0.),
        }
    }

    fn x(&self, wavenumber: f64, bounds: Rectangle) -> f32 {
        let plot = self.plot(bounds);
        plot.x + self.data.x(wavenumber) * plot.width
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
        let plot = self.plot(bounds);
        if plot.width <= 0. {
            return None;
        }
        let frequency = self.data.high
            - f64::from((point.x - plot.x) / plot.width) * (self.data.high - self.data.low);
        // The normalization is shared by every band. Logs keep distant
        // Gaussian tails comparable when their profiles would underflow.
        let log_contribution = |&(_, center, intensity): &(usize, f64, f64)| {
            let offset = (frequency - center) / self.data.fwhm;
            intensity.ln()
                - match self.data.shape {
                    BandShape::Lorentzian => (4. * offset * offset).ln_1p(),
                    BandShape::Gaussian => 4. * std::f64::consts::LN_2 * offset * offset,
                }
        };
        let (index, _, _) = self.data.pickable_sticks().min_by(|a, b| {
            log_contribution(b)
                .total_cmp(&log_contribution(a))
                .then_with(|| {
                    (self.x(a.1, bounds) - point.x)
                        .abs()
                        .total_cmp(&(self.x(b.1, bounds) - point.x).abs())
                })
                .then_with(|| b.2.total_cmp(&a.2))
        })?;
        Some(canvas::Action::publish(send(Action::Mode(*index))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) && self.data.pickable_sticks().next().is_some() {
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
        let axis_text = Color { a: 0.75, ..ink };
        let accent = theme.palette().primary;
        let plot = self.plot(bounds);
        let base = plot.y + plot.height;
        let height = plot.height;
        let width = plot.width;
        if width <= 0. || height <= 0. {
            return vec![frame.into_geometry()];
        }
        frame.stroke(
            &canvas::Path::rectangle(plot.position(), plot.size()),
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
            size: CHART_FONT_SIZE.into(),
            font: iced::Font::with_name(reshiki::style::ui_font_family()),
            align_x: align.into(),
            ..Default::default()
        };
        // A narrow chart labels fewer ticks, measured in the drawn font.
        let mut last_right = f32::NEG_INFINITY;
        for tick in self.data.ticks() {
            let x = self.x(tick, bounds);
            frame.stroke(
                &canvas::Path::line(Point::new(x, base), Point::new(x, base + 4.)),
                canvas::Stroke::default().with_color(muted).with_width(1.),
            );
            let half = text_extent(&format!("{tick:.0}"), CHART_FONT_SIZE) / 2.;
            let major = (tick / 1000.).fract() == 0. || self.data.high - self.data.low <= 2000.;
            if major && x - half >= 0. && x + half <= bounds.width && x - half > last_right + 6. {
                last_right = x + half;
                frame.fill_text(small(
                    format!("{tick:.0}"),
                    Point::new(x, base + 6.),
                    axis_text,
                    iced::alignment::Horizontal::Center,
                ));
            }
        }
        frame.fill_text(small(
            WAVENUMBER_TITLE.into(),
            Point::new(plot.center_x(), bounds.height - 12.),
            axis_text,
            iced::alignment::Horizontal::Center,
        ));
        let transmittance = self.data.y == YAxis::Transmittance;
        frame.fill_text(small(
            if transmittance {
                "100 %T"
            } else {
                "Relative absorbance"
            }
            .into(),
            Point::new(plot.x, plot.y - 14.),
            axis_text,
            iced::alignment::Horizontal::Left,
        ));
        let message = self.data.message();
        if let Some(message) = message.filter(|_| self.data.settings_error.is_none()) {
            frame.fill_text(canvas::Text {
                max_width: (width - 8.).max(1.),
                ..small(
                    message.into(),
                    Point::new(plot.center_x(), plot.center_y()),
                    muted,
                    iced::alignment::Horizontal::Center,
                )
            });
        }
        let baseline = if transmittance {
            plot.y + height * (5. / 105.)
        } else {
            base
        };
        let direction = if transmittance { 1. } else { -1. };
        // Calculated transmittance is relative: the strongest visible band
        // reaches 20 %T rather than an absolute measured transmittance.
        let depth = if transmittance { 80. / 105. } else { 0.92 };
        let top = self.data.top();
        let y = |value: f64| baseline + direction * (value / top * depth) as f32 * height;
        let tallest = self.data.sticks.iter().map(|s| s.2).fold(0., f64::max);
        for &(_, frequency, intensity) in &self.data.sticks {
            let chosen = self.selected == Some(frequency);
            let x = self.x(frequency, bounds);
            let stick_depth = if tallest > 0. {
                (intensity / tallest * depth) as f32 * height
            } else {
                8.
            };
            let stick_end =
                baseline + direction * stick_depth.max(if chosen { 12. } else { 3. }).min(height);
            frame.stroke(
                &canvas::Path::line(Point::new(x, baseline), Point::new(x, stick_end)),
                canvas::Stroke::default()
                    .with_color(if chosen {
                        accent
                    } else {
                        Color { a: 0.35, ..muted }
                    })
                    .with_width(if chosen { 2. } else { 1. }),
            );
        }
        if message.is_none() {
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
        }
        // The selected mode's label and the imaginary note win; then the
        // tallest peaks, each clear of the text already placed.
        let extent = |text: &str| text_extent(text, CHART_FONT_SIZE);
        let mut placed = Vec::new();
        if let Some(frequency) = self.selected {
            let x = self.x(frequency, bounds);
            let label = format!("{frequency:.0} cm⁻¹");
            let label_width = extent(&label);
            if label_width <= width {
                let lo = (x + 4.).min(plot.x + width - label_width).max(plot.x);
                frame.fill_text(small(
                    label,
                    Point::new(lo, plot.y + 2.),
                    accent,
                    iced::alignment::Horizontal::Left,
                ));
                placed.push((lo.min(x), (lo + label_width).max(x)));
            }
        }
        if self.imaginary_selected {
            let note = "imaginary mode: not plotted";
            // Top left: high wavenumbers rarely carry peak labels.
            frame.fill_text(small(
                note.into(),
                Point::new(plot.x + 4., plot.y + 2.),
                theme.palette().danger,
                iced::alignment::Horizontal::Left,
            ));
            placed.push((plot.x + 4., plot.x + 4. + extent(note)));
        }
        for [wavenumber, value] in self
            .data
            .labelled_peaks(
                |wavenumber| self.x(wavenumber, bounds),
                CHART_FONT_SIZE,
                placed,
            )
            .filter(|_| self.data.can_plot())
        {
            frame.fill_text(small(
                format!("{wavenumber:.0}"),
                Point::new(
                    self.x(wavenumber, bounds),
                    if transmittance {
                        (y(value) + 2.).min(base - 12.)
                    } else {
                        (y(value) - 12.).max(plot.y + 2.)
                    },
                ),
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
    fn clicking_coincident_sticks_selects_the_strongest_ir_active_mode() {
        use canvas::Program;

        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![1500.; 3];
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(480., CHART_HEIGHT));
        let click = iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        for (intensities, expected) in [([0., 5., 80.], Some(2)), ([0.; 3], None)] {
            result.vibrations.as_mut().unwrap().ir_intensities_km_mol = intensities.to_vec();
            let spectrum = Spectrum::new(&Arc::new(result.clone()), &State::default(), 0);
            let cursor = mouse::Cursor::Available(Point::new(spectrum.x(1500., bounds), 50.));
            let selected = spectrum
                .update(&mut (), &click, bounds, cursor)
                .map(|action| {
                    let (Some(Message::Xtb(XtbAction::Vibrations(Action::Mode(mode)))), _, _) =
                        action.into_inner()
                    else {
                        panic!("a peak click selects its mode");
                    };
                    mode
                });
            assert_eq!(selected, expected);
        }
    }

    #[test]
    fn clicking_a_broadened_band_selects_a_strong_contributor() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![1500., 1504., 1508.];
        vibrations.ir_intensities_km_mol = vec![100., 0.01, 100.];
        let result = Arc::new(result);
        for shape in [BandShape::Lorentzian, BandShape::Gaussian] {
            for fwhm in [5., 20., 60.] {
                let settings = State {
                    shape,
                    fwhm,
                    ..State::default()
                };
                let spectrum = Spectrum::new(&result, &settings, 1);
                assert!(matches!(picked_mode(&spectrum, 1504.), Some(0 | 2)));
            }
        }
    }

    #[test]
    fn distant_clicks_use_current_shape_and_width() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![1500., 1501.];
        vibrations.ir_intensities_km_mol = vec![1., 1e12];
        let result = Arc::new(result);
        // Both Gaussian profiles underflow at this distance in linear space.
        for (shape, fwhm, expected) in [
            (BandShape::Gaussian, 20., 1),
            (BandShape::Gaussian, 5., 0),
            (BandShape::Lorentzian, 5., 1),
        ] {
            let settings = State {
                shape,
                fwhm,
                ..State::default()
            };
            let spectrum = Spectrum::new(&result, &settings, 0);
            assert_eq!(picked_mode(&spectrum, 400.), Some(expected));
        }
    }

    fn picked_mode(spectrum: &Spectrum, frequency: f64) -> Option<usize> {
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(480., CHART_HEIGHT));
        let cursor = mouse::Cursor::Available(Point::new(spectrum.x(frequency, bounds), 50.));
        canvas::Program::update(
            spectrum,
            &mut (),
            &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .map(|action| {
            let (Some(Message::Xtb(XtbAction::Vibrations(Action::Mode(mode)))), _, _) =
                action.into_inner()
            else {
                panic!("a spectrum click selects its mode");
            };
            mode
        })
    }

    #[test]
    fn enormous_or_nonfinite_frequencies_do_not_expand_the_manual_range() {
        for frequency in [1e20, f64::INFINITY, f64::NAN] {
            let mut result = spectrum_result();
            let vibrations = result.vibrations.as_mut().unwrap();
            vibrations.frequencies_cm1 = vec![600., frequency];
            vibrations.ir_intensities_km_mol = vec![1., 1.];
            let spectrum = Spectrum::new(&Arc::new(result), &State::default(), 0);
            assert!(spectrum.data.can_plot());
            assert_eq!(spectrum.data.high, 4000.);
            assert_eq!(spectrum.data.low, 400.);
            assert_eq!(spectrum.data.sticks, vec![(0, 600., 1.)]);
            assert!(
                spectrum
                    .data
                    .curve
                    .iter()
                    .all(|[x, y]| (400. ..=4000.).contains(x) && y.is_finite())
            );
            assert!(
                spectrum
                    .data
                    .peaks
                    .iter()
                    .all(|[x, _]| (400. ..=4000.).contains(x))
            );
            assert!(spectrum_ticks(spectrum.data.high, spectrum.data.low).len() <= MAX_TICKS);
        }
    }

    #[test]
    fn manual_range_bounds_sticks_and_selection_after_scaling() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![0., 10_000., 20_000.];
        vibrations.ir_intensities_km_mol = vec![1., 1., 1.];
        let result = Arc::new(result);
        let mut settings = State {
            from: "10000".into(),
            to: "0".into(),
            ..State::default()
        };
        let at_limit = Spectrum::new(&result, &settings, 1);
        assert_eq!(at_limit.data.high, 10_000.);
        assert_eq!(at_limit.data.low, 0.);
        assert!(at_limit.data.can_plot());
        assert_eq!(at_limit.selected, Some(10_000.));
        assert_eq!(at_limit.data.sticks, vec![(0, 0., 1.), (1, 10_000., 1.)]);
        settings.scale = "1.01".into();
        let over_limit = Spectrum::new(&result, &settings, 1);
        assert!(over_limit.data.can_plot());
        assert_eq!(over_limit.selected, None);
        assert_eq!(over_limit.data.sticks, vec![(0, 0., 1.)]);
    }

    #[test]
    fn modes_without_ir_intensities_remain_selectable() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![-100., 600., 1600., 4500.];
        vibrations.ir_intensities_km_mol.clear();
        let spectrum = Spectrum::new(&Arc::new(result), &State::default(), 2);
        assert!(!spectrum.data.can_plot());
        assert_eq!(
            spectrum.data.message(),
            Some("No IR intensities in this result")
        );
        assert_eq!(spectrum.data.sticks, vec![(1, 600., 0.), (2, 1600., 0.)]);
        assert_eq!(spectrum.selected, Some(1600.));
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(480., CHART_HEIGHT));
        let cursor = mouse::Cursor::Available(Point::new(spectrum.x(1600., bounds), 50.));
        let (message, _, _) = canvas::Program::update(
            &spectrum,
            &mut (),
            &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap()
        .into_inner();
        assert!(matches!(
            message,
            Some(Message::Xtb(XtbAction::Vibrations(Action::Mode(2))))
        ));
    }

    #[test]
    fn invalid_scale_and_range_keep_preview_sticks_and_mode_picking() {
        let result = Arc::new(spectrum_result());
        for (scale, from, to, error) in [
            ("", "4000", "400", SCALE_ERROR),
            ("1.000", "", "400", RANGE_ERROR),
            ("1.000", "4000", "3900", RANGE_ERROR),
        ] {
            let settings = State {
                scale: scale.into(),
                from: from.into(),
                to: to.into(),
                ..State::default()
            };
            let spectrum = Spectrum::new(&result, &settings, 1);
            let valid = Spectrum::new(&result, &State::default(), 1);
            assert_eq!(spectrum.data.message(), Some(error));
            assert!(!spectrum.data.can_plot());
            assert_eq!(spectrum.data.sticks, valid.data.sticks);
            assert_eq!(spectrum.selected, valid.selected);
            let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(480., CHART_HEIGHT));
            let frequency = spectrum.selected.unwrap();
            let cursor = mouse::Cursor::Available(Point::new(spectrum.x(frequency, bounds), 50.));
            let (message, _, _) = canvas::Program::update(
                &spectrum,
                &mut (),
                &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner();
            assert!(matches!(
                message,
                Some(Message::Xtb(XtbAction::Vibrations(Action::Mode(1))))
            ));
        }
    }

    #[test]
    fn endpoint_tick_labels_fit_and_picking_uses_the_inset_plot() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![10_000., 9800.];
        vibrations.ir_intensities_km_mol = vec![1., 1.];
        let settings = State {
            from: "10000".into(),
            to: "9800".into(),
            ..State::default()
        };
        let spectrum = Spectrum::new(&Arc::new(result), &settings, 1);
        for width in [140., CHART_WIDTH] {
            let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(width, CHART_HEIGHT));
            let plot = spectrum.plot(bounds);
            for frequency in [10_000., 9800.] {
                let x = spectrum.x(frequency, bounds);
                let half = text_extent(&format!("{frequency:.0}"), CHART_FONT_SIZE) / 2.;
                assert!(x - half >= 0. && x + half <= width);
            }
            assert_eq!(spectrum.x(10_000., bounds), plot.x);
            assert_eq!(spectrum.x(9800., bounds), plot.x + plot.width);
            let cursor = mouse::Cursor::Available(Point::new(plot.x + plot.width, 50.));
            let (message, _, _) = canvas::Program::update(
                &spectrum,
                &mut (),
                &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner();
            assert!(matches!(
                message,
                Some(Message::Xtb(XtbAction::Vibrations(Action::Mode(1))))
            ));
        }
    }

    #[test]
    fn visible_refined_peaks_set_the_normalization_and_label_threshold() {
        let mut result = spectrum_result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.frequencies_cm1 = vec![1000.12345, 4500.];
        vibrations.ir_intensities_km_mol = vec![1., 1000.];
        let settings = State {
            shape: BandShape::Gaussian,
            fwhm: 5.,
            from: "1100".into(),
            to: "900".into(),
            ..State::default()
        };
        let spectrum = Spectrum::new(&Arc::new(result), &settings, 0);
        assert_eq!(spectrum.data.peaks.len(), 1);
        let [frequency, intensity] = spectrum.data.peaks[0];
        assert!((frequency - 1000.12345).abs() < 0.1);
        assert!(spectrum.data.curve.contains(&[frequency, intensity]));
        assert_eq!(spectrum.data.top(), intensity);
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
        settings.fwhm = 25.;
        let wider = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&gaussian.data, &wider.data));
        settings.scale = "0.95".into();
        let scaled = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&wider.data, &scaled.data));
        settings.y = YAxis::Absorbance;
        let absorbance = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&scaled.data, &absorbance.data));
        settings.from = "3500".into();
        let lower_start = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&absorbance.data, &lower_start.data));
        settings.to = "600".into();
        let higher_end = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&lower_start.data, &higher_end.data));
        let other_result = Arc::new(spectrum_result());
        let different = Spectrum::new(&other_result, &settings, 1);
        assert!(!Arc::ptr_eq(&higher_end.data, &different.data));

        settings.scale = "invalid".into();
        let invalid_scale = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&higher_end.data, &invalid_scale.data));
        assert_eq!(invalid_scale.data.message(), Some(SCALE_ERROR));
        assert!(!invalid_scale.data.can_plot());
        settings.scale = "0.95".into();
        settings.from = "invalid".into();
        let invalid_range = Spectrum::new(&result, &settings, 1);
        assert!(!Arc::ptr_eq(&invalid_scale.data, &invalid_range.data));
        assert_eq!(invalid_range.data.message(), Some(RANGE_ERROR));
        assert!(!invalid_range.data.can_plot());
    }
}
