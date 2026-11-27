//! Select the saved reaction path, preview its geometries, and export its XYZ trajectory.
use super::super::{palette::Shown, results::View};
use super::{optimized_reactant_energy, saved_search};
#[cfg(not(target_arch = "wasm32"))]
use crate::app::animation::PathSample;
use crate::app::animation::{Playback as Position, PlaybackMode, PlaybackSpeed};
use crate::app::figure_export::{Kind, Prepared};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Frame, IrcEndpoint, IrcPath, KCAL_PER_HARTREE};
use std::sync::{Arc, Weak};

pub(crate) mod controls;
pub(crate) mod focus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Profile {
    Irc,
    Neb,
}

pub(super) fn saved(result: &CalcResult) -> Option<&IrcPath> {
    saved_search(result)?.irc.as_ref()
}

pub(in crate::app::xtb) struct Playback {
    result: Weak<CalcResult>,
    attachment: u64,
    revision: (u64, u64),
    atoms: Vec<Option<u64>>,
    input: Arc<Frame>,
    doc: Document,
    position: Position,
}

impl App {
    pub(in crate::app) fn xtb_irc_leave(&mut self) {
        self.tab.xtb.palette.irc_playback = None;
    }

    pub(super) fn xtb_profile(&self, result: &CalcResult) -> Profile {
        let Some(path) = saved(result) else {
            return Profile::Neb;
        };
        if saved_search(result).is_some_and(|search| search.refined_from.is_some()) {
            return Profile::Irc;
        }
        if let Some(profile) = self.tab.xtb.palette.profile {
            return profile;
        }
        if path.completed == [true; 2]
            || path.reached == [IrcEndpoint::Reactant, IrcEndpoint::Product]
        {
            Profile::Irc
        } else {
            Profile::Neb
        }
    }

    pub(super) fn xtb_set_profile(&mut self, profile: Profile) {
        if self.xtb_shown().is_some_and(|shown| {
            super::is_ts(&shown.stored.result)
                && (profile == Profile::Neb || saved(&shown.stored.result).is_some())
                && (profile != Profile::Neb
                    || !saved_search(&shown.stored.result)
                        .is_some_and(|search| search.refined_from.is_some()))
        }) {
            self.xtb_end_preview();
            self.tab.xtb.palette.profile = Some(profile);
        }
    }

    pub(in crate::app) fn xtb_irc_preview(&self) -> Option<&Document> {
        let playback = self.tab.xtb.palette.irc_playback.as_ref()?;
        if self.tab.xtb.palette.view != View::Path
            || self.tabs.temporary_depth != 0
            || !self.xtb_palette_body_visible()
            || self.xtb_running()
            || self.xtb_failed().is_some()
            || playback.revision != (self.tab.file_epoch, self.tab.revision)
        {
            return None;
        }
        let (_, stored) = self.xtb_shown_entry()?;
        (playback.attachment == stored.attachment && super::is(&playback.result, &stored.result))
            .then_some(&playback.doc)
    }

    /// A paused preview remains visible, but does not subscribe to animation ticks.
    pub(in crate::app) fn xtb_irc_playing(&self) -> bool {
        self.xtb_irc_preview().is_some()
            && self
                .tab
                .xtb
                .palette
                .irc_playback
                .as_ref()
                .is_some_and(|p| p.position.playing)
    }

    pub(super) fn xtb_irc_frame(&self) -> Option<usize> {
        self.xtb_irc_preview()?;
        Some(self.tab.xtb.palette.irc_playback.as_ref()?.position.frame)
    }

    fn xtb_new_irc_playback(&self) -> Option<Playback> {
        if self.xtb_running() {
            return None;
        }
        let shown = self.xtb_shown().filter(|s| !s.stale)?;
        let path = saved(&shown.stored.result)
            .filter(|_| self.xtb_profile(&shown.stored.result) == Profile::Irc)?;
        let frame = path.frame(0)?;
        let doc = shown.atoms.iter().flatten().next().and_then(|&atom| {
            let mut doc = super::super::preview(&self.tab.doc, atom, &frame.positions_angstrom)?;
            super::preview_endpoint_image(&mut doc, shown.atoms, shown.input, &frame).ok()?;
            if let Some(arrow) = shown.arrow {
                let index = doc
                    .models
                    .iter()
                    .position(|m| m.linked_atoms.contains(&atom))?;
                super::show_path_model(&self.tab.doc, &mut doc, index, arrow);
            }
            Some(doc)
        });
        let mut position = Position::default();
        position.playing = path.steps() > 1;
        Some(Playback {
            result: Arc::downgrade(&shown.stored.result),
            attachment: shown.stored.attachment,
            revision: (self.tab.file_epoch, self.tab.revision),
            atoms: shown.atoms.to_vec(),
            input: shown.input.clone(),
            doc: doc?,
            position,
        })
    }

    fn xtb_irc_initialize(&mut self) -> bool {
        if self.xtb_irc_preview().is_some() {
            return true;
        }
        self.tab.xtb.palette.irc_playback = None;
        let Some(playback) = self.xtb_new_irc_playback() else {
            self.xtb_set_palette_status(
                super::super::palette::StatusKind::Palette,
                "Animating the IRC requires a matching 3D model for this reaction.",
                true,
            );
            return false;
        };
        self.xtb_end_preview();
        self.tab.xtb.palette.irc_playback = Some(playback);
        true
    }

    pub(super) fn xtb_play_irc(&mut self) {
        let existing = self.xtb_irc_preview().is_some();
        if !self.xtb_irc_initialize() {
            return;
        }
        if existing && let Some(playback) = &mut self.tab.xtb.palette.irc_playback {
            let len = playback
                .result
                .upgrade()
                .and_then(|r| saved(&r).map(IrcPath::steps));
            if let Some(len) = len {
                playback.position.toggle(self.appearance.playback_mode, len);
                if !playback.show() {
                    self.tab.xtb.palette.irc_playback = None;
                }
            }
        }
    }

    pub(super) fn xtb_seek_irc(&mut self, frame: usize) {
        if !self.xtb_irc_initialize() {
            return;
        }
        if let Some(playback) = &mut self.tab.xtb.palette.irc_playback {
            let len = playback
                .result
                .upgrade()
                .and_then(|r| saved(&r).map(IrcPath::steps));
            if let Some(len) = len {
                playback.position.seek(frame, len);
                if !playback.show() {
                    self.tab.xtb.palette.irc_playback = None;
                }
            }
        }
    }

    pub(super) fn xtb_step_irc(&mut self, direction: i32) {
        if self.xtb_irc_playing() {
            return;
        }
        let current = self.xtb_irc_frame().unwrap_or(0);
        self.xtb_seek_irc(current.saturating_add_signed(direction as isize));
    }

    pub(super) fn xtb_irc_mode(&mut self, mode: PlaybackMode) {
        self.appearance.playback_mode = mode;
        let _ = self.appearance.save();
    }

    pub(super) fn xtb_irc_speed(&mut self, speed: PlaybackSpeed) {
        self.appearance.playback_speed = speed;
        let _ = self.appearance.save();
    }

    pub(super) fn xtb_irc_tick(&mut self) {
        if self.xtb_irc_preview().is_none() {
            self.tab.xtb.palette.irc_playback = None;
            return;
        }
        let Some(playback) = self.tab.xtb.palette.irc_playback.as_mut() else {
            return;
        };
        let len = playback
            .result
            .upgrade()
            .and_then(|r| saved(&r).map(IrcPath::steps));
        if let Some(len) = len
            && playback
                .position
                .advance(self.appearance.playback_mode, len)
            && !playback.show()
        {
            self.tab.xtb.palette.irc_playback = None;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::app) fn xtb_irc_gif_source(&self) -> Option<crate::app::gif_export::Source> {
        use crate::app::gif_export::{Motion, Source};
        let shown = self.xtb_shown().filter(|shown| !shown.stale)?;
        let result = &shown.stored.result;
        let path = saved(result).filter(|path| path.has_frames())?;
        let reference = optimized_reactant_energy(result)?;
        let source = self.xtb_ts_export_source(result, shown.arrow)?;
        let doc = self
            .xtb_irc_preview()
            .cloned()
            .or_else(|| self.xtb_new_irc_playback().map(|p| p.doc))?;
        let atom = shown.atoms.iter().flatten().next()?;
        let model = doc
            .models
            .iter()
            .position(|model| model.linked_atoms.contains(atom))?;
        let samples = (0..path.steps())
            .map(|frame| PathSample::from_irc(path, reference, frame))
            .collect::<Option<_>>()?;
        Some(Source {
            name: source.stem(reshiki::qc_input::Job::Irc),
            doc,
            model,
            motion: Motion::Irc {
                trajectory: Box::new(path.trajectory.clone()),
                samples,
                ts_index: path.ts_index,
                unit: self.appearance.energy_unit,
            },
            mode: self.appearance.playback_mode,
            speed: self.appearance.playback_speed.factor(),
        })
    }

    pub(super) fn xtb_prepare_irc(&self, shown: &Shown<'_>) -> Option<Result<Prepared, String>> {
        let result = &shown.stored.result;
        let path = saved(result).filter(|p| p.has_frames())?;
        let reference = optimized_reactant_energy(result)?;
        let source = self.xtb_ts_export_source(result, shown.arrow)?;
        let stem = source.stem(reshiki::qc_input::Job::Irc);
        let unit = self.appearance.energy_unit;
        Some(
            reshiki::qc_input::write_xyz_trajectory_with_comments(
                &path.trajectory,
                path.arc_lengths_sqrt_amu_bohr
                    .iter()
                    .zip(&path.trajectory.energies_hartree)
                    .map(|(s, energy)| {
                        let delta = unit.convert_kcal((energy - reference) * KCAL_PER_HARTREE);
                        format!(
                            "{} · E = {energy} Eh · s = {s:.6} amu½·bohr · ΔE = {delta:.6} {}",
                            super::view::method_name(result),
                            unit.label()
                        )
                    }),
            )
            .map(|xyz| Prepared::new(xyz.into_bytes(), "xyz", Some(stem), vec![], Kind::Irc))
            .map_err(|error| error.to_string()),
        )
    }

    pub(super) fn xtb_export_irc(&mut self) -> Task<Message> {
        match self
            .xtb_shown()
            .and_then(|shown| self.xtb_prepare_irc(&shown))
        {
            Some(Ok(export)) => self.export_ready(export, cfg!(target_arch = "wasm32")),
            Some(Err(error)) => Task::done(Message::Xtb(super::super::Action::Palette(
                super::super::palette::Action::IrcExported(Err(error)),
            ))),
            None => Task::none(),
        }
    }
}

impl Playback {
    fn show(&mut self) -> bool {
        self.result
            .upgrade()
            .and_then(|result| saved(&result)?.frame(self.position.frame))
            .is_some_and(|frame| {
                super::preview_endpoint_image(&mut self.doc, &self.atoms, &self.input, &frame)
                    .is_ok()
            })
    }
}
