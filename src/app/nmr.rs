//! Atom-linked offline NMR results; detached work and chemical identity gates.
use super::{App, InspectorTab, Message, files};
use crate::canvas::nmr::LabelMode;
use iced::{Point, Size, Task};
use reshiki::document::Document;
use reshiki::nmr::{self as engine, Nucleus, Report};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobKey {
    serial: u64,
    epoch: u64,
    ids: Vec<u64>,
    fingerprint: String,
}
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Close,
    Dock,
    Undock,
    Predict(Nucleus),
    ToggleCollapsed,
    ToggleDetails,
    TogglePlot,
    ToggleLabels,
    LabelMode(LabelMode),
    LabelMenu(bool),
    Move(Point),
    Resize(Size),
    ResetPosition,
    RenderedSize(Size),
    SelectRow(usize),
    SelectMarker(Vec<usize>),
    Copy,
    Export,
    Finished(JobKey, Result<Arc<Report>, String>),
    Exported(Result<Option<std::path::PathBuf>, String>),
}

pub(super) use palette::OVERLAY_INSETS;
pub(super) use state::State;

const STALE_NOTICE: &str = "Molecular chemistry changed; run prediction again";
/// The status while nothing predictable is selected: an empty state, not an error.
const EMPTY_STATUS: &str = "NMR · Select a molecule";

/// The status summary of an accepted report, e.g. `¹H NMR · 3 of 3 atoms predicted`.
fn summary(nucleus: Nucleus, supported: usize, total: usize) -> String {
    format!(
        "{} NMR · {supported} of {total} atoms predicted",
        nucleus.label()
    )
}

fn identity_current(
    document: &Document,
    epoch_matches: bool,
    ids: &[u64],
    fingerprint: &str,
) -> bool {
    epoch_matches && engine::fingerprint(document, ids).ok().as_deref() == Some(fingerprint)
}

mod palette;
mod panel;
mod plot;
mod presentation;
mod state;
mod style;
mod view;
mod worker;

impl App {
    pub(super) fn nmr_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open => return self.nmr_open(),
            Action::Close => return self.nmr_close(),
            Action::Dock => return self.nmr_dock(),
            Action::Undock => return self.nmr_undock(),
            Action::Predict(nucleus) => return self.nmr_predict(nucleus),
            Action::Finished(key, result) => return self.nmr_finished(key, result),
            Action::SelectRow(index) => return self.nmr_select_row(index),
            Action::SelectMarker(indices) => return self.nmr_select_marker(indices),
            Action::Copy => return self.nmr_copy(),
            Action::Export => return self.nmr_export(),
            Action::Exported(result) => return self.nmr_exported(result),
            Action::ToggleCollapsed => self.tab.nmr.toggle_collapsed(),
            Action::ToggleDetails => self.tab.nmr.toggle_details(),
            Action::TogglePlot => self.tab.nmr.toggle_plot(),
            Action::ToggleLabels => self.tab.nmr.toggle_labels(),
            Action::LabelMode(mode) => self.tab.nmr.set_label_mode(mode),
            Action::LabelMenu(open) => self.tab.nmr.set_label_menu(open),
            Action::Move(position) => self.tab.nmr.move_to(position),
            Action::Resize(size) => self.tab.nmr.resize(size),
            Action::ResetPosition => self.tab.nmr.reset_position(),
            Action::RenderedSize(size) => self.tab.nmr.set_rendered_size(size),
        }
        Task::none()
    }

    fn nmr_open(&mut self) -> Task<Message> {
        self.tab.nmr.open = true;
        self.tab.nmr.collapsed = false;
        self.tab.nmr.details = false;
        if self.tab.nmr.dock.docked {
            self.place_nmr_inspector(true);
        }
        // The Properties entry predicts the currently selected
        // molecule, including when another valid report was retained.
        self.nmr_predict(self.tab.nmr.job.nucleus)
    }

    fn nmr_close(&mut self) -> Task<Message> {
        self.tab.nmr.open = false;
        self.tab.nmr.dock.restore_dock = false;
        self.tab.nmr.labels.visible = false;
        self.tab.nmr.labels.menu = false;
        if self.tab.nmr.dock.docked {
            self.place_nmr_inspector(false);
        }
        Task::none()
    }

    fn nmr_dock(&mut self) -> Task<Message> {
        self.tab.nmr.open = true;
        self.tab.nmr.dock.docked = true;
        self.place_nmr_inspector(true);
        if self.tab.nmr.job.result.is_none() && self.tab.nmr.job.pending.is_none() {
            return self.nmr_predict(self.tab.nmr.job.nucleus);
        }
        Task::none()
    }

    fn nmr_undock(&mut self) -> Task<Message> {
        self.tab.nmr.dock.docked = false;
        self.tab.nmr.dock.restore_dock = false;
        self.place_nmr_inspector(false);
        Task::none()
    }

    fn nmr_select_marker(&mut self, indices: Vec<usize>) -> Task<Message> {
        let Some(report) = &self.tab.nmr.job.result else {
            return Task::none();
        };
        let indices: Vec<_> = indices
            .into_iter()
            .filter(|&index| {
                report
                    .rows
                    .get(index)
                    .is_some_and(engine::Row::has_statistics)
            })
            .collect();
        // Nearby sticks share a label, not a prediction or intensity.
        // Repeated activation cycles their distinct original atom IDs.
        let current = indices.iter().position(|&index| {
            report
                .rows
                .get(index)
                .is_some_and(|row| self.tab.selected.contains(&row.atom_id))
        });
        if let Some(&index) = indices.get(current.map_or(0, |i| (i + 1) % indices.len())) {
            return self.nmr_select_row(index);
        }
        Task::none()
    }

    fn nmr_predict(&mut self, nucleus: Nucleus) -> Task<Message> {
        if self.tab.nmr.job.pending.is_some() {
            return Task::none();
        }
        self.tab.nmr.open = true;
        self.tab.nmr.collapsed = false;
        self.tab.nmr.job.nucleus = nucleus;
        self.tab.nmr.job.result = None;
        self.tab.nmr.job.presentation = None;
        self.tab.nmr.labels.visible = false;
        self.tab.nmr.labels.menu = false;
        let request = match engine::prepare(&self.tab.doc, &self.tab.selected) {
            Ok(r) => r,
            // No atom is selected: the panel shows its empty state. A selection
            // across several molecules keeps its diagnostic below.
            Err(error)
                if error == engine::SELECT_ONE_MOLECULE
                    && !self
                        .tab
                        .selected
                        .iter()
                        .any(|id| self.tab.doc.atom(*id).is_some()) =>
            {
                self.tab.nmr.job.notice = None;
                self.status = EMPTY_STATUS.into();
                self.error = false;
                return Task::none();
            }
            Err(error) => {
                self.tab.nmr.job.notice = Some(error.clone());
                self.status = error;
                self.error = true;
                return Task::none();
            }
        };
        self.tab.nmr.job.serial = self.tab.nmr.job.serial.wrapping_add(1);
        let key = JobKey {
            serial: self.tab.nmr.job.serial,
            epoch: self.tab.file_epoch,
            ids: request.atom_ids.clone(),
            fingerprint: request.fingerprint.clone(),
        };
        self.tab.nmr.job.pending = Some(key.clone());
        self.tab.nmr.job.notice = None;
        self.status = format!(
            "Predicting {} chemical shifts from local observed data…",
            nucleus.label()
        );
        self.error = false;
        Task::perform(worker::calculate(request, nucleus), move |result| {
            Message::Nmr(Action::Finished(key.clone(), result))
        })
    }

    fn nmr_finished(&mut self, key: JobKey, result: Result<Arc<Report>, String>) -> Task<Message> {
        if self.tab.nmr.job.pending.as_ref() != Some(&key) {
            return Task::none();
        }
        self.tab.nmr.job.pending = None;
        if !identity_current(
            &self.tab.doc,
            key.epoch == self.tab.file_epoch,
            &key.ids,
            &key.fingerprint,
        ) {
            self.tab.nmr.job.notice = Some(STALE_NOTICE.into());
            return Task::none();
        }
        match result {
            Ok(report) => {
                let presentation = presentation::Presentation::new(&report, &self.tab.doc);
                self.status = summary(
                    report.nucleus,
                    presentation.supported_count,
                    report.rows.len(),
                );
                self.error = false;
                self.tab.nmr.job.result_epoch = Some(key.epoch);
                self.tab.nmr.job.result = Some(report);
                self.tab.nmr.job.presentation = Some(presentation);
            }
            Err(error) => {
                self.tab.nmr.job.notice = Some(error.clone());
                self.status = error;
                self.error = true;
            }
        }
        Task::none()
    }

    fn nmr_select_row(&mut self, index: usize) -> Task<Message> {
        if let Some(row) = self
            .tab
            .nmr
            .job
            .result
            .as_ref()
            .and_then(|r| r.rows.get(index))
        {
            let mut selected: Vec<_> = std::iter::once(row.atom_id)
                .chain(row.hydrogen_ids.iter().copied())
                .collect();
            let collapsed = self.tab.doc.abbreviation_containing(row.atom_id);
            if let Some(group) = collapsed
                && !selected.contains(&group.anchor)
            {
                selected.push(group.anchor);
            }
            let mut status = format!("Atom #{}", row.atom_id);
            if self.tab.nmr.job.nucleus == Nucleus::H1 {
                status.push_str(&format!(" · {}H", row.hydrogen_count));
            }
            match &row.statistics {
                Some(statistics) => status.push_str(&format!(" · {:.3} ppm", statistics.median)),
                None => status.push_str(" · not predicted"),
            }
            self.set_selection(selected);
            self.status = status;
        }
        Task::none()
    }

    fn nmr_copy(&mut self) -> Task<Message> {
        if let Some(report) = &self.tab.nmr.job.result {
            self.status =
                "Copied predicted NMR shifts with method, conditions and data attribution".into();
            return iced::clipboard::write(engine::to_tsv(report));
        }
        Task::none()
    }

    fn nmr_export(&mut self) -> Task<Message> {
        if let Some(report) = &self.tab.nmr.job.result {
            let contents = engine::to_tsv(report);
            return Task::perform(
                async move {
                    files::save_bytes(
                        "Export predicted NMR shifts",
                        "Predicted NMR shifts.tsv",
                        "tsv",
                        contents.as_bytes(),
                    )
                    .await
                },
                |result| Message::Nmr(Action::Exported(result)),
            );
        }
        Task::none()
    }

    fn nmr_exported(
        &mut self,
        result: Result<Option<std::path::PathBuf>, String>,
    ) -> Task<Message> {
        match result {
            Ok(Some(path)) => {
                self.status = format!("Exported predicted NMR shifts to {}", path.display());
                self.error = false;
            }
            Ok(None) => {}
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
        Task::none()
    }

    pub(super) fn nmr_canvas(&self) -> Option<crate::canvas::nmr::Context<'_>> {
        let state = &self.tab.nmr;
        (state.open && state.labels.visible).then_some(())?;
        let report = state.job.result.as_ref()?;
        let presentation = state
            .job
            .presentation
            .as_ref()
            .filter(|presentation| presentation.matches(report));
        Some(crate::canvas::nmr::Context {
            report,
            mode: state.labels.mode,
            supported: presentation.map(|presentation| presentation.sorted_ids.as_slice()),
        })
    }

    pub(super) fn nmr_leave_tab(&mut self) {
        self.tab.nmr.dock.restore_dock = self.tab.nmr.open
            && self.tab.nmr.dock.docked
            && self.inspector_open
            && self.inspector_tab == InspectorTab::Nmr;
        if self.tab.nmr.dock.restore_dock {
            self.place_nmr_inspector(false);
        }
    }

    pub(super) fn nmr_enter_tab(&mut self) -> bool {
        if !self.tab.nmr.dock.restore_dock {
            return false;
        }
        self.place_nmr_inspector(true);
        // Restoring a report must preserve its saved camera even when the
        // other document already had an equally wide inspector open.
        true
    }

    fn place_nmr_inspector(&mut self, dock: bool) {
        let old_width = self.inspector_width();
        if dock {
            if self.inspector_tab != InspectorTab::Nmr {
                self.tab.nmr.dock.dock_return = Some((self.inspector_open, self.inspector_tab));
            }
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Nmr;
        } else if self.inspector_tab == InspectorTab::Nmr {
            let (open, tab) = self
                .tab
                .nmr
                .dock
                .dock_return
                .take()
                .unwrap_or((true, InspectorTab::Properties));
            self.inspector_open = open;
            self.inspector_tab = tab;
        }
        self.expect_inspector_width_change(old_width);
    }
}

#[cfg(test)]
pub(super) mod test_support;
#[cfg(test)]
mod tests;
