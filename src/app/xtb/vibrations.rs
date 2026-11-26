//! The Vibrations tab: the mode list, the selected mode animated on the 3D
//! model (with displacement arrows), the IR spectrum, thermochemistry at
//! another temperature and pressure, and "Push along mode and re-optimize".
//!
//! Normal modes are not saved (light results); a reopened result
//! recomputes them with the stored settings when the tab opens.
use super::palette::Shown;
use super::results::View;
#[cfg(not(target_arch = "wasm32"))]
use super::state::FailureDetails;
use super::{Page, Request, Start, is, state};
use crate::app::figure_export::{Kind, Prepared};
use crate::app::{App, InspectorTab, Message};
use iced::Task;
use reshiki::document::{Document, Point};
use reshiki::graphics::Graphic;
use reshiki::qcdata::vibrational::{self, BandShape};
use reshiki::qcdata::{CalcResult, Frame, Settings, Thermo};
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Weak};
use std::time::Duration;

mod figure;
mod view;

/// Frames in one period; a period lasts one second at speed 1×.
const FRAMES: usize = 24;
const TICK: Duration = Duration::from_millis(40);
/// "Push along mode": the atom that moves most moves this far (Å).
pub(crate) const PUSH_ANGSTROM: f64 = 0.3;
/// Frames written by "Export mode as XYZ movie".
const MOVIE_FRAMES: usize = 20;
/// Arrows are this many times the animation amplitude, so they read at a glance.
const ARROW_SCALE: f32 = 2.5;
/// Recomputed frequencies further than this from the saved ones (cm⁻¹) are reported.
const DRIFT_CM1: f64 = 0.5;
const MAX_DRIFT_CM1: f64 = 2.;
const GIBBS_TOLERANCE_HARTREE: f64 = 1e-6;
/// Pascal per standard atmosphere.
const PA_PER_ATM: f64 = 101_325.;

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Mode(usize),
    Play(bool),
    Amplitude(f32),
    Speed(f32),
    Arrows(bool),
    /// Add the shown arrows to the drawing, as one Undo step.
    PlaceArrows,
    Shape(BandShape),
    Fwhm(f32),
    Scale(String),
    PlaceSpectrum,
    ExportSpectrum(&'static str),
    Thermo,
    Temperature(String),
    Pressure(String),
    Tick,
    ExportXyz,
    Push,
    #[cfg(not(target_arch = "wasm32"))]
    Recomputed(u64, u64, Result<Arc<super::run::Report>, state::Failure>),
}

/// One document's tab settings and animation.
pub(crate) struct State {
    pub mode: usize,
    pub playing: bool,
    /// Largest atomic excursion of the animation, Å.
    pub amplitude: f32,
    pub speed: f32,
    pub arrows: bool,
    pub shape: BandShape,
    pub fwhm: f32,
    pub scale: String,
    pub thermo: bool,
    pub temperature: String,
    /// Atmospheres, as typed.
    pub pressure: String,
    spectrum: RefCell<Option<(view::SpectrumKey, Arc<view::SpectrumData>)>>,
    animation: Option<Animation>,
    /// Normal modes recomputed for results the file kept light.
    modes: Vec<(Weak<CalcResult>, Arc<Vec<f64>>)>,
    recompute: Option<Recompute>,
    recompute_serial: u64,
    /// Why modes could not be recomputed, for the result it was tried on.
    pub(super) problem: Option<(Weak<CalcResult>, String)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: 0,
            playing: true,
            amplitude: 0.3,
            speed: 1.,
            arrows: false,
            shape: BandShape::Lorentzian,
            fwhm: 15.,
            scale: "1.00".into(),
            thermo: false,
            temperature: "298.15".into(),
            pressure: "1".into(),
            spectrum: RefCell::new(None),
            animation: None,
            modes: Vec::new(),
            recompute: None,
            recompute_serial: 0,
            problem: None,
        }
    }
}

struct Recompute {
    serial: u64,
    epoch: u64,
    result: Weak<CalcResult>,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: reshiki_xtb_worker::client::Cancel,
    _handle: Option<iced::task::Handle>,
}

impl Drop for Recompute {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.cancel.cancel();
    }
}

/// What the frames show; any change builds them again.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    result: *const CalcResult,
    mode: usize,
    amplitude: f32,
    arrows: bool,
    revision: (u64, u64),
}

/// One preview document; each frame swaps in the model (and its arrows), so
/// a large drawing is copied once, not once per frame.
struct Animation {
    key: Key,
    doc: Document,
    /// The animated model in `doc`, and how many graphics `doc` has without arrows.
    model: usize,
    graphics: usize,
    /// The model and arrows at each phase of one period.
    frames: Vec<(reshiki::models3d::Model3d, Vec<Graphic>)>,
    shown: usize,
    /// Position in the period, 0 to 1.
    phase: f32,
}

impl Animation {
    /// Put the frame for the current phase into the preview document.
    fn show(&mut self) {
        let index = ((self.phase * self.frames.len() as f32) as usize)
            .min(self.frames.len().saturating_sub(1));
        if index == self.shown {
            return;
        }
        let (Some((model, arrows)), Some(slot)) =
            (self.frames.get(index), self.doc.models.get_mut(self.model))
        else {
            return;
        };
        *slot = model.clone();
        self.doc.graphics.truncate(self.graphics);
        self.doc.graphics.extend(arrows.iter().cloned());
        self.shown = index;
    }
}

impl State {
    /// The typed scale factor, if it is a sensible one.
    pub(super) fn scale(&self) -> Option<f64> {
        typed(&self.scale).filter(|s| (0.5..=1.5).contains(s))
    }

    /// Temperature (K) and pressure (Pa) as typed, if valid.
    pub(super) fn conditions(&self) -> Option<(f64, f64)> {
        let kelvin = typed(&self.temperature).filter(|t| (1.0..=10_000.).contains(t))?;
        let atm = typed(&self.pressure).filter(|p| (1e-6..=10_000.).contains(p))?;
        Some((kelvin, atm * PA_PER_ATM))
    }

    pub(super) fn recomputing(&self, result: &Arc<CalcResult>) -> bool {
        self.recompute
            .as_ref()
            .is_some_and(|r| is(&r.result, result))
    }
}

fn typed(text: &str) -> Option<f64> {
    text.trim()
        .replace('−', "-")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn spectrum_method(result: &CalcResult) -> &str {
    result
        .provenance
        .method
        .as_deref()
        .unwrap_or(match &result.settings {
            Settings::Xtb(settings) => settings.method.label(),
            Settings::Imported { .. } => "xTB",
        })
}

/// The selected mode's vector, if the modes are known.
fn vector<'a>(modes: &'a [f64], frame: &Frame, mode: usize) -> Option<&'a [f64]> {
    let width = frame.numbers.len() * 3;
    modes.get(mode * width..(mode + 1) * width)
}

/// Use the rotor type that reproduces the saved Gibbs correction before
/// changing temperature or pressure.
pub(super) fn thermo_at(
    result: &CalcResult,
    frame: &Frame,
    temperature_k: f64,
    pressure_pa: f64,
) -> Option<Thermo> {
    let vibrations = result.vibrations.as_ref()?;
    let saved = result.thermo.as_ref()?;
    // A non-positive rotor cutoff means harmonic-only entropy, which the
    // local formula does not model: such results keep their saved conditions.
    if saved.symmetry_number == 0
        || saved.rotor_cutoff_cm1 <= 0.
        || vibrations.masses_amu.len() != frame.numbers.len()
    {
        return None;
    }
    let mut input = vibrational::ThermoInput {
        frequencies_cm1: &vibrations.frequencies_cm1,
        masses_amu: &vibrations.masses_amu,
        positions_angstrom: &frame.positions_angstrom,
        linear: None,
        symmetry_number: saved.symmetry_number,
        temperature_k: saved.temperature_k,
        pressure_pa: saved.pressure_pa,
        frequency_scale: saved.frequency_scale,
        rotor_cutoff_cm1: saved.rotor_cutoff_cm1,
        imaginary_cutoff_cm1: saved.imaginary_cutoff_cm1,
    };
    let reproduces = |input: &vibrational::ThermoInput<'_>| {
        let thermo = vibrational::thermochemistry(input);
        // Every saved quantity, not G alone: a wrong rotor type can match G
        // by cancellation at an unusual temperature.
        thermo_is_finite(&thermo)
            && [
                (
                    thermo.gibbs_correction_hartree,
                    saved.gibbs_correction_hartree,
                ),
                (
                    thermo.enthalpy_correction_hartree,
                    saved.enthalpy_correction_hartree,
                ),
                (
                    thermo.zero_point_energy_hartree,
                    saved.zero_point_energy_hartree,
                ),
                (
                    thermo.temperature_k * thermo.entropy_hartree_per_k,
                    saved.temperature_k * saved.entropy_hartree_per_k,
                ),
            ]
            .iter()
            .all(|(ours, theirs)| (ours - theirs).abs() <= GIBBS_TOLERANCE_HARTREE)
    };
    if !reproduces(&input) {
        input.linear = Some(!input.is_linear());
        if !reproduces(&input) {
            return None;
        }
    }
    input.temperature_k = temperature_k;
    input.pressure_pa = pressure_pa;
    let mut thermo = vibrational::thermochemistry(&input);
    thermo.point_group = saved.point_group.clone();
    thermo_is_finite(&thermo).then_some(thermo)
}

fn thermo_is_finite(thermo: &Thermo) -> bool {
    [
        thermo.temperature_k,
        thermo.pressure_pa,
        thermo.zero_point_energy_hartree,
        thermo.enthalpy_correction_hartree,
        thermo.entropy_hartree_per_k,
        thermo.gibbs_correction_hartree,
        thermo.frequency_scale,
        thermo.rotor_cutoff_cm1,
        thermo.imaginary_cutoff_cm1,
    ]
    .into_iter()
    .chain(thermo.heat_capacity_hartree_per_k)
    .all(f64::is_finite)
}

impl App {
    fn xtb_mode(&self, count: usize) -> usize {
        self.tab.xtb.vibrations.mode.min(count.saturating_sub(1))
    }

    fn xtb_model_at_result(&self, shown: &Shown<'_>) -> bool {
        super::model_shows(&self.tab.doc, shown.atoms, &shown.frame.positions_angstrom)
    }

    /// The normal modes of `result`: stored, or recomputed this session.
    pub(super) fn xtb_modes<'a>(&'a self, result: &'a Arc<CalcResult>) -> Option<&'a [f64]> {
        let vibrations = result.vibrations.as_ref()?;
        if !vibrations.modes.is_empty() {
            return Some(&vibrations.modes);
        }
        self.tab
            .xtb
            .vibrations
            .modes
            .iter()
            .find(|(weak, _)| is(weak, result))
            .map(|(_, modes)| modes.as_slice())
    }

    /// The document showing the current animation frame, while it belongs
    /// to this document state.
    pub(super) fn xtb_vibration_preview(&self) -> Option<&Document> {
        Some(&self.xtb_current_animation()?.doc)
    }

    fn xtb_current_animation(&self) -> Option<&Animation> {
        self.tab
            .xtb
            .vibrations
            .animation
            .as_ref()
            .filter(|animation| {
                self.xtb_vibrations_visible()
                    && animation.key.revision == (self.tab.file_epoch, self.tab.revision)
            })
    }

    /// Ticks while a mode animates.
    pub(in crate::app) fn xtb_subscription(&self) -> iced::Subscription<Message> {
        let vibrations = &self.tab.xtb.vibrations;
        if vibrations.playing && self.xtb_vibration_preview().is_some() {
            iced::time::every(TICK).map(|_| Message::Xtb(super::Action::Vibrations(Action::Tick)))
        } else {
            iced::Subscription::none()
        }
    }

    pub(in crate::app) fn xtb_vibrations_visible(&self) -> bool {
        let palette = &self.tab.xtb.palette;
        self.tabs.temporary_depth == 0
            && palette.open
            && palette.view == View::Vibrations
            && if self.xtb.dock.inspector {
                self.inspector_open
                    && self.inspector_tab == InspectorTab::Predict
                    && self.xtb.page == Page::Xtb
            } else {
                !self.xtb.dock.collapsed
            }
    }

    fn xtb_vibrations_live(&self) -> Option<Shown<'_>> {
        if !self.xtb_vibrations_visible()
            || self.tab.xtb.run.is_some()
            || self.xtb_step_preview().is_some()
        {
            return None;
        }
        self.xtb_shown()
            .filter(|shown| !shown.stale && self.xtb_model_at_result(shown))
    }

    pub(in crate::app) fn xtb_vibrations_leave(&mut self) {
        let settings = &mut self.tab.xtb.vibrations;
        settings.animation = None;
        settings.recompute = None;
    }

    /// Build the animation frames for what the tab now shows, or drop them.
    pub(super) fn xtb_vibrations_sync(&mut self) {
        let revision = (self.tab.file_epoch, self.tab.revision);
        let settings = &self.tab.xtb.vibrations;
        let Some(shown) = self.xtb_vibrations_live() else {
            self.xtb_vibrations_leave();
            return;
        };
        let result = &shown.stored.result;
        let count = result
            .vibrations
            .as_ref()
            .map_or(0, |v| v.frequencies_cm1.len());
        let key = Key {
            result: Arc::as_ptr(result),
            mode: self.xtb_mode(count),
            amplitude: settings.amplitude,
            arrows: settings.arrows,
            revision,
        };
        if settings.animation.as_ref().is_some_and(|a| a.key == key) {
            return;
        }
        let phase = settings.animation.as_ref().map_or(0., |a| a.phase);
        let mut animation = self
            .xtb_modes(result)
            .and_then(|modes| vector(modes, shown.frame, key.mode))
            .and_then(|mode| self.xtb_mode_frames(&shown, mode, key, phase));
        if let Some(animation) = &mut animation {
            animation.show();
        }
        self.tab.xtb.vibrations.animation = animation;
    }

    /// One period of `mode` on the model, with arrows if asked. The model
    /// keeps its drawn size as atoms move (its frame follows its reach).
    fn xtb_mode_frames(
        &self,
        shown: &Shown<'_>,
        mode: &[f64],
        key: Key,
        phase: f32,
    ) -> Option<Animation> {
        let atom = *shown.atoms.iter().flatten().next()?;
        let equilibrium = &shown.frame.positions_angstrom;
        let positions =
            vibrational::mode_frames(equilibrium, mode, f64::from(key.amplitude), FRAMES);
        let mut doc = super::preview(&self.tab.doc, atom, equilibrium)?;
        let index = doc
            .models
            .iter()
            .position(|m| m.linked_atoms.contains(&atom))?;
        let rest = doc.models.get(index)?.clone();
        let (size, reach) = (rest.size, rest.reach(&doc));
        let mut frames = Vec::with_capacity(positions.len());
        for positions in &positions {
            let model = doc.models.get_mut(index)?;
            let mut xyz = positions.iter().map(|p| p.map(|x| x as f32));
            for (atom, p) in model.atoms.iter_mut().zip(xyz.by_ref()) {
                atom.xyz = p;
            }
            for (hydrogen, p) in model.hydrogens.iter_mut().zip(xyz) {
                hydrogen.xyz = p;
            }
            let reach_now = doc.models.get(index)?.reach(&doc);
            doc.models.get_mut(index)?.size = size * reach_now / reach.max(1e-3);
            let arrows = if key.arrows {
                arrows(&doc, index, positions, mode, key.amplitude)?
            } else {
                vec![]
            };
            frames.push((doc.models.get(index)?.clone(), arrows));
        }
        *doc.models.get_mut(index)? = rest;
        Some(Animation {
            key,
            graphics: doc.graphics.len(),
            doc,
            model: index,
            frames,
            // Nothing shown yet: `show` puts in the frame for `phase`.
            shown: usize::MAX,
            phase,
        })
    }

    pub(super) fn xtb_vibrations_action(&mut self, action: Action) -> Task<Message> {
        let tick = matches!(action, Action::Tick);
        let settings = &mut self.tab.xtb.vibrations;
        match action {
            Action::Mode(mode) => settings.mode = mode,
            Action::Play(on) => settings.playing = on,
            Action::Amplitude(value) => settings.amplitude = value,
            Action::Speed(value) => settings.speed = value,
            Action::Arrows(on) => settings.arrows = on,
            Action::PlaceArrows => {
                self.xtb_place_arrows();
                self.xtb_vibrations_sync();
                return Task::none();
            }
            Action::Shape(shape) => settings.shape = shape,
            Action::Fwhm(value) => settings.fwhm = value,
            Action::Scale(value) => settings.scale = value,
            Action::PlaceSpectrum => {
                self.xtb_place_spectrum();
                self.xtb_vibrations_sync();
                return Task::none();
            }
            Action::ExportSpectrum(format) => {
                return self
                    .xtb_prepare_spectrum(format)
                    .map(|result| match result {
                        Ok(export) => Message::ExportReady(export),
                        Err(error) => Message::Xtb(super::Action::Palette(
                            super::palette::Action::Exported(Err(error)),
                        )),
                    });
            }
            Action::Thermo => settings.thermo = !settings.thermo,
            Action::Temperature(value) => settings.temperature = value,
            Action::Pressure(value) => settings.pressure = value,
            Action::Tick => {
                if let Some(animation) = &mut settings.animation {
                    let step = settings.speed * TICK.as_secs_f32();
                    animation.phase = (animation.phase + step).fract();
                    animation.show();
                }
            }
            Action::ExportXyz => return self.xtb_export_movie(),
            Action::Push => return self.xtb_push(),
            #[cfg(not(target_arch = "wasm32"))]
            Action::Recomputed(epoch, serial, outcome) => {
                // Its status (a frequency drift) stays in front of the mode line.
                self.xtb_recomputed(epoch, serial, outcome);
                self.xtb_vibrations_sync();
                return Task::none();
            }
        }
        self.xtb_vibrations_sync();
        // Ticks leave the status bar to whatever else it says.
        if !tick {
            self.xtb_vibrations_status();
        }
        Task::none()
    }

    /// "Bend · 1590 cm⁻¹ · animating" for the selected mode.
    fn xtb_vibrations_status(&mut self) {
        if self.tab.xtb.vibrations.animation.is_none() {
            return;
        }
        let Some(shown) = self.xtb_shown() else {
            return;
        };
        let Some(vibrations) = &shown.stored.result.vibrations else {
            return;
        };
        let mode = self.xtb_mode(vibrations.frequencies_cm1.len());
        let Some(frequency) = vibrations.frequencies_cm1.get(mode) else {
            return;
        };
        let state = if self.tab.xtb.vibrations.playing {
            "animating"
        } else {
            "paused"
        };
        self.status = format!(
            "Mode {} · {} · {state}",
            mode + 1,
            view::wavenumber(*frequency)
        );
        self.error = false;
    }

    /// The tab opened: recompute missing modes, then animate.
    pub(super) fn xtb_vibrations_opened(&mut self) -> Task<Message> {
        if let Some(result) = self.xtb_shown().map(|shown| shown.stored.result.clone()) {
            self.tab
                .xtb
                .vibrations
                .problem
                .take_if(|(weak, _)| is(weak, &result));
        }
        let task = self.xtb_vibrations_resume();
        self.xtb_vibrations_status();
        task
    }

    pub(in crate::app) fn xtb_vibrations_resume(&mut self) -> Task<Message> {
        self.xtb_vibrations_sync();
        self.xtb_recompute_modes()
    }

    /// Recompute the shown result's normal modes with its stored settings,
    /// unless they are known, being recomputed, or cannot be.
    fn xtb_recompute_modes(&mut self) -> Task<Message> {
        #[cfg(target_arch = "wasm32")]
        return Task::none();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(shown) = self.xtb_vibrations_live() else {
                return Task::none();
            };
            let result = shown.stored.result.clone();
            let vibrations = &self.tab.xtb.vibrations;
            if self.xtb_modes(&result).is_some()
                || vibrations.recomputing(&result)
                || vibrations
                    .problem
                    .as_ref()
                    .is_some_and(|(w, _)| is(w, &result))
                || super::unavailable().is_some()
                || !matches!(result.settings, Settings::Xtb(_))
            {
                return Task::none();
            }
            let Some(mut config) = super::results::config(
                &result,
                shown.frame,
                shown.atoms,
                self.xtb.settings.threads(),
            ) else {
                return Task::none();
            };
            config.task = state::Task::Frequencies;
            config.fixed.clear();
            let job = super::job(&config, shown.frame, vec![]);
            let epoch = self.tab.file_epoch;
            let settings = &mut self.tab.xtb.vibrations;
            settings.recompute_serial = settings.recompute_serial.wrapping_add(1);
            let serial = settings.recompute_serial;
            let cancel = reshiki_xtb_worker::client::Cancel::default();
            // The worker starts when the task first runs, never in a test that drops it.
            let worker = cancel.clone();
            let events =
                iced::futures::StreamExt::flatten(iced::futures::stream::once(async move {
                    super::run::start(job, worker)
                }));
            let (task, handle) = Task::run(events, move |event| match event {
                super::run::Event::Done(outcome) => Some(Message::Xtb(super::Action::Vibrations(
                    Action::Recomputed(epoch, serial, outcome),
                ))),
                _ => None,
            })
            .abortable();
            settings.recompute = Some(Recompute {
                serial,
                epoch,
                result: Arc::downgrade(&result),
                cancel,
                _handle: Some(handle.abort_on_drop()),
            });
            task.then(|message| message.map_or_else(Task::none, Task::done))
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn xtb_recomputed(
        &mut self,
        epoch: u64,
        serial: u64,
        outcome: Result<Arc<super::run::Report>, state::Failure>,
    ) {
        let Some(recompute) = self
            .tab
            .xtb
            .vibrations
            .recompute
            .take_if(|r| r.epoch == epoch && r.serial == serial)
        else {
            return;
        };
        if recompute.epoch != self.tab.file_epoch {
            return;
        }
        let Some(shown) = self
            .xtb_vibrations_live()
            .filter(|shown| is(&recompute.result, &shown.stored.result))
        else {
            return;
        };
        let atoms = shown.frame.numbers.len();
        let result = shown.stored.result.clone();
        let vibrations = &mut self.tab.xtb.vibrations;
        let saved = result
            .vibrations
            .as_ref()
            .map_or(&[][..], |v| &v.frequencies_cm1);
        let report = match outcome {
            Ok(report) => report,
            Err(failure) => {
                let (reason, _) = failure.explain(reshiki::qcdata::XtbMethod::Gfn2Xtb, &[]);
                vibrations.problem = Some((
                    Arc::downgrade(&result),
                    format!("The normal modes could not be recomputed: {reason}"),
                ));
                return;
            }
        };
        let fresh = report
            .computed
            .vibrations
            .as_ref()
            .map_or(&[][..], |v| &v.frequencies_cm1);
        let modes = &report.computed.normal_modes;
        let drift = fresh
            .iter()
            .zip(saved)
            .map(|(a, b)| (a - b).abs())
            .fold(0., f64::max);
        if fresh.len() != saved.len()
            || drift > MAX_DRIFT_CM1
            || fresh
                .iter()
                .zip(saved)
                .any(|(a, b)| a.partial_cmp(&0.) != b.partial_cmp(&0.))
        {
            vibrations.problem = Some((
                Arc::downgrade(&result),
                format!(
                    "The recomputed frequencies differ from the saved ones (up to {drift:.1} cm⁻¹); the modes are not shown."
                ),
            ));
            return;
        }
        if modes.len() != saved.len() * atoms * 3 {
            vibrations.problem = Some((
                Arc::downgrade(&result),
                "The recomputed modes do not match the saved frequencies.".into(),
            ));
            return;
        }
        if drift > DRIFT_CM1 {
            self.status = format!("Recomputed modes differ by up to {drift:.1} cm⁻¹");
            self.error = false;
        }
        vibrations.modes.retain(|(weak, _)| weak.strong_count() > 0);
        vibrations
            .modes
            .push((Arc::downgrade(&result), Arc::new(modes.clone())));
    }

    /// "Show arrows" → "Add arrows to drawing": the arrows of the selected
    /// mode at the model's own geometry, as one Undo step.
    fn xtb_place_arrows(&mut self) {
        let Some(shown) = self.xtb_vibrations_live() else {
            return;
        };
        let Some(vibrations) = &shown.stored.result.vibrations else {
            return;
        };
        let mode = self.xtb_mode(vibrations.frequencies_cm1.len());
        let amplitude = self.tab.xtb.vibrations.amplitude;
        let Some(mode) = self
            .xtb_modes(&shown.stored.result)
            .and_then(|modes| vector(modes, shown.frame, mode))
        else {
            return;
        };
        let Some(atom) = shown.atoms.iter().flatten().next().copied() else {
            return;
        };
        let positions = &shown.frame.positions_angstrom;
        let Some(mut doc) = super::preview(&self.tab.doc, atom, positions) else {
            return;
        };
        let Some(index) = doc
            .models
            .iter()
            .position(|m| m.linked_atoms.contains(&atom))
        else {
            return;
        };
        let Some(arrows) = arrows(&doc, index, positions, mode, amplitude) else {
            return;
        };
        let count = arrows.len();
        doc.graphics.extend(arrows);
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
        if !self.error {
            self.status = format!("Added {count} displacement arrows · ⌘Z undoes");
            // The drawing has them now at equilibrium: stop there, so the
            // preview neither draws them twice nor moves atoms out from under them.
            let settings = &mut self.tab.xtb.vibrations;
            settings.arrows = false;
            settings.playing = false;
            if let Some(animation) = &mut settings.animation {
                animation.phase = 0.;
                animation.show();
            }
        }
    }

    fn xtb_place_spectrum(&mut self) {
        let Some(shown) = self.xtb_shown() else {
            return;
        };
        let result = &shown.stored.result;
        let spectrum = view::SpectrumData::cached(result, &self.tab.xtb.vibrations);
        if !spectrum.can_plot() {
            return;
        }
        let doc = &self.tab.doc;
        let mut component_atoms = HashSet::new();
        for &atom in shown.atoms.iter().flatten() {
            if !component_atoms.contains(&atom) {
                component_atoms.extend(reshiki::molecule_names::component(doc, atom));
            }
        }
        let atoms: Vec<_> = component_atoms.iter().copied().collect();
        // Selection bounds include the committed model frame when it is shown.
        let bounds = reshiki::scene::selection_bounds(doc, &atoms);
        // Each model frame covers every rotation at that phase. Reserve all
        // phases since the frame itself can grow as the mode displaces atoms.
        let animation_bounds = self
            .xtb_current_animation()
            .into_iter()
            .flat_map(|animation| {
                animation.frames.iter().filter_map(move |(model, _)| {
                    (model.display != reshiki::models3d::Display::TwoD)
                        .then(|| reshiki::models3d::frame(&animation.doc, model))
                        .flatten()
                })
            });
        let Some((lo, hi)) =
            bounds
                .into_iter()
                .chain(animation_bounds)
                .reduce(|(lo, hi), (frame_lo, frame_hi)| {
                    (
                        Point::new(lo.x.min(frame_lo.x), lo.y.min(frame_lo.y)),
                        Point::new(hi.x.max(frame_hi.x), hi.y.max(frame_hi.y)),
                    )
                })
        else {
            return;
        };
        let at = Point::new(hi.x + doc.drawing_style.bond_length_world, lo.y);
        let figure = figure::spectrum(doc, &spectrum, spectrum_method(result), at);
        let before = doc.clone();
        let members = figure.group.members.clone();
        figure.add_to(&mut self.tab.doc);
        self.changed(before);
        if !self.error {
            self.set_selection(members);
            self.tool = crate::canvas::Tool::Select;
            self.status = "Placed the IR spectrum · ⌘Z undoes".into();
        }
    }

    fn xtb_prepare_spectrum(&self, format: &'static str) -> Task<Result<Prepared, String>> {
        let Some(shown) = self.xtb_shown() else {
            return Task::none();
        };
        let result = &shown.stored.result;
        let spectrum = view::SpectrumData::cached(result, &self.tab.xtb.vibrations);
        if !spectrum.can_plot() {
            return Task::none();
        }
        let source = &self.tab.doc;
        let mut doc = Document {
            drawing_style: source.drawing_style.clone(),
            canvas_theme: source.canvas_theme,
            color_theme: source.color_theme,
            custom_theme: source.custom_theme.clone(),
            ..Document::default()
        };
        figure::spectrum(&doc, &spectrum, spectrum_method(result), Point::default())
            .add_to(&mut doc);
        let engine = self.engine.clone();
        let name = self.export_stem().unwrap_or_else(|| self.document_name());
        let stem = Some(format!("{name} IR spectrum"));
        let appearance = &self.appearance;
        let render_format = if format == "svg" && !appearance.export_svg_live_text {
            "svg-office"
        } else {
            format
        };
        let editable = appearance.export_editable_drawing;
        let background = appearance
            .export_background
            .paper(appearance.custom_background);
        let dpi = appearance.export_dpi.get();
        Task::perform(
            async move {
                #[cfg(target_arch = "wasm32")]
                crate::web::after_paint().await;
                #[cfg(target_arch = "wasm32")]
                use reshiki::browser_render::publication;
                #[cfg(not(target_arch = "wasm32"))]
                use reshiki::export::publication;
                let exported = publication(
                    &engine,
                    doc,
                    render_format,
                    false,
                    editable,
                    background,
                    dpi,
                    reshiki::export::FILE_PIXELS,
                )
                .await?;
                Ok(Prepared::new(
                    exported.bytes,
                    format,
                    stem,
                    vec![],
                    Kind::Xtb,
                ))
            },
            std::convert::identity,
        )
    }

    /// Export mode as XYZ movie: one period, [`MOVIE_FRAMES`] frames.
    fn xtb_export_movie(&mut self) -> Task<Message> {
        let Some(shown) = self.xtb_shown() else {
            return Task::none();
        };
        let Some(vibrations) = &shown.stored.result.vibrations else {
            return Task::none();
        };
        let mode = self.xtb_mode(vibrations.frequencies_cm1.len());
        let amplitude = f64::from(self.tab.xtb.vibrations.amplitude);
        let Some(vector) = self
            .xtb_modes(&shown.stored.result)
            .and_then(|modes| vector(modes, shown.frame, mode))
        else {
            self.status = "The normal modes are not available yet".into();
            self.error = true;
            return Task::none();
        };
        let frequency = vibrations
            .frequencies_cm1
            .get(mode)
            .copied()
            .unwrap_or_default();
        let frames = vibrational::mode_frames(
            &shown.frame.positions_angstrom,
            vector,
            amplitude,
            MOVIE_FRAMES,
        );
        let formula = state::formula(&shown.frame.numbers);
        let movie = vibrational::xyz_movie(
            &shown.frame.numbers,
            &frames,
            &format!("{formula} mode {} {frequency:.2} cm-1", mode + 1),
        );
        let stem = std::path::Path::new(&self.document_name())
            .file_stem()
            .map(|stem| format!("{} mode {}", stem.to_string_lossy(), mode + 1));
        self.export_ready(
            crate::app::figure_export::Prepared::new(
                movie.into_bytes(),
                "xyz",
                stem,
                vec![],
                crate::app::figure_export::Kind::Xtb,
            ),
            cfg!(target_arch = "wasm32"),
        )
    }

    /// Push along the selected imaginary mode (the first one if a real mode
    /// is selected) and run Opt + freq from there. The result replaces the
    /// accepted structure only if the optimization converges.
    fn xtb_push(&mut self) -> Task<Message> {
        if !self.xtb_can_start() {
            return Task::none();
        }
        let Some(shown) = self.xtb_shown().filter(|s| !s.stale) else {
            return Task::none();
        };
        if !self.xtb_model_at_result(&shown) {
            self.status = "The 3D model moved after this calculation".into();
            self.error = true;
            return Task::none();
        }
        let result = shown.stored.result.clone();
        let Some(vibrations) = result.vibrations.as_ref() else {
            return Task::none();
        };
        let selected = self.xtb_mode(vibrations.frequencies_cm1.len());
        let mode = if vibrations
            .frequencies_cm1
            .get(selected)
            .is_some_and(|f| *f < 0.)
        {
            selected
        } else if vibrations.imaginary_count() > 0 {
            0
        } else {
            return Task::none();
        };
        let Some(vector) = self
            .xtb_modes(&result)
            .and_then(|modes| vector(modes, shown.frame, mode))
        else {
            self.status = "Computing normal modes first; push again after".into();
            self.error = false;
            return self.xtb_recompute_modes();
        };
        let Some(mut config) = super::results::config(
            &result,
            shown.frame,
            shown.atoms,
            self.xtb.settings.threads(),
        ) else {
            self.status = "The Calculate card cannot repeat this calculation".into();
            self.error = true;
            return Task::none();
        };
        config.task = state::Task::OptFreq;
        config.fixed.clear();
        let from = shown.frame.positions_angstrom.clone();
        let positions = vibrational::displaced(&from, vector, PUSH_ANGSTROM);
        let Some(first) = shown
            .atoms
            .iter()
            .flatten()
            .copied()
            .find(|id| self.tab.doc.atom(*id).is_some())
        else {
            return Task::none();
        };
        let request = Request {
            atoms: reshiki::molecule_names::component(&self.tab.doc, first),
            config,
            keep_full: shown.stored.keep_full,
            start: Some(Start { positions, from }),
            search: None,
        };
        self.xtb_begin(request)
    }
}

/// Displacement arrows for each atom that moves visibly in `mode`, drawn
/// from the atoms of model `index` in `doc` (at `positions`, frame order).
fn arrows(
    doc: &Document,
    index: usize,
    positions: &[[f64; 3]],
    mode: &[f64],
    amplitude: f32,
) -> Option<Vec<Graphic>> {
    use reshiki::graphics::{PathCommand, PathHeads};
    let model = doc.models.get(index)?;
    let (directions, _) = mode.as_chunks::<3>();
    let largest = directions
        .iter()
        .map(|d| d.iter().map(|x| x * x).sum::<f64>().sqrt())
        .fold(0., f64::max);
    if largest <= 0. {
        return Some(vec![]);
    }
    let length = f64::from(amplitude * ARROW_SCALE) / largest;
    let tips: Vec<[f32; 3]> = positions
        .iter()
        .zip(directions)
        .flat_map(|(p, &[dx, dy, dz])| {
            let [x, y, z] = *p;
            [
                p.map(|x| x as f32),
                [x + dx * length, y + dy * length, z + dz * length].map(|x| x as f32),
            ]
        })
        .collect();
    let points = reshiki::models3d::project(doc, model, &tips)?;
    // Arrows shorter than a fifth of a bond would be all head.
    let shortest = doc.drawing_style.bond_length_world * 0.2;
    let style = reshiki::graphics::GraphicStyle {
        stroke: reshiki::palette::Color::Palette(
            reshiki::palette::Hue::Teal,
            reshiki::palette::Row::Strong,
        ),
        width_pt: 1.2,
        ..Default::default()
    };
    let heads = PathHeads {
        head: reshiki::arrows::Head::Full,
        head_length_pt: 5.,
        head_width_pt: 2.,
        ..Default::default()
    };
    let mut id = doc.next_id();
    Some(
        points
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|[start, end]| start.distance(*end) >= shortest)
            .map(|&[start, end]| {
                // A plain pen path: no reaction-arrow meaning, drawn in front
                // of the model it annotates.
                let mut arrow = Graphic::pen_curve(id, start, end, style.clone());
                arrow.path = vec![PathCommand::Move(start), PathCommand::Line(end)];
                arrow.node_types.clear();
                arrow.heads = Some(heads);
                arrow.layer = 1;
                id += 1;
                arrow
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
