//! Select the saved reaction path, preview its geometries, and export its XYZ trajectory.
use super::super::{palette::Shown, results::View};
use super::{optimized_reactant_energy, saved_search};
use crate::app::figure_export::{Kind, Prepared};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Frame, IrcEndpoint, IrcPath, KCAL_PER_HARTREE};
use std::sync::{Arc, Weak};

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
    step: usize,
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

    pub(super) fn xtb_play_irc(&mut self) {
        let stopping = self.xtb_irc_preview().is_some();
        self.tab.xtb.palette.irc_playback = None;
        if stopping || self.xtb_running() {
            return;
        }
        let Some(shown) = self.xtb_shown().filter(|s| !s.stale) else {
            return;
        };
        let Some(path) = saved(&shown.stored.result)
            .filter(|_| self.xtb_profile(&shown.stored.result) == Profile::Irc)
        else {
            return;
        };
        let Some(frame) = path.frame(0) else { return };
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
        let Some(doc) = doc else {
            self.xtb_set_palette_status(
                super::super::palette::StatusKind::Palette,
                "Animating the IRC requires a matching 3D model for this reaction.",
                true,
            );
            return;
        };
        let playback = Playback {
            result: Arc::downgrade(&shown.stored.result),
            attachment: shown.stored.attachment,
            revision: (self.tab.file_epoch, self.tab.revision),
            atoms: shown.atoms.to_vec(),
            input: shown.input.clone(),
            doc,
            step: 0,
        };
        self.xtb_end_preview();
        self.tab.xtb.palette.irc_playback = Some(playback);
    }

    pub(super) fn xtb_irc_tick(&mut self) {
        if self.xtb_irc_preview().is_none() {
            self.tab.xtb.palette.irc_playback = None;
            return;
        }
        let Some(playback) = self.tab.xtb.palette.irc_playback.as_mut() else {
            return;
        };
        let next = playback.step + 1;
        let frame = playback
            .result
            .upgrade()
            .and_then(|result| saved(&result)?.frame(next));
        if frame.is_some_and(|frame| {
            super::preview_endpoint_image(
                &mut playback.doc,
                &playback.atoms,
                &playback.input,
                &frame,
            )
            .is_ok()
        }) {
            playback.step = next;
        } else {
            self.tab.xtb.palette.irc_playback = None;
        }
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
