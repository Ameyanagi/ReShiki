//! Per-document palette placement, docking, labels and prediction work.
use super::palette::{DEFAULT_SIZE, MAX_SIZE, MIN_SIZE};
use super::{InspectorTab, JobKey, LabelMode, Nucleus, Report, STALE_NOTICE, identity_current};
use iced::{Point, Size};
use reshiki::document::Document;
use std::sync::Arc;

pub(in crate::app) struct State {
    pub(in crate::app) open: bool,
    pub(super) collapsed: bool,
    pub(super) details: bool,
    pub(super) plot: bool,
    pub(super) palette: Palette,
    pub(super) dock: Dock,
    pub(super) labels: Labels,
    pub(super) job: Job,
    checked: Option<Checked>,
}

struct Checked {
    epoch: u64,
    revision: u64,
    pending: Option<JobKey>,
    result: Option<Arc<Report>>,
    result_epoch: Option<u64>,
}

impl Checked {
    fn matches(&self, epoch: u64, revision: u64, job: &Job) -> bool {
        self.epoch == epoch
            && self.revision == revision
            && self.pending == job.pending
            && self.result_epoch == job.result_epoch
            && match (&self.result, &job.result) {
                (Some(checked), Some(current)) => Arc::ptr_eq(checked, current),
                (None, None) => true,
                _ => false,
            }
    }
}

pub(super) struct Palette {
    pub(super) position: Option<Point>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) resized: bool,
    pub(super) rendered_size: Size,
}

#[derive(Default)]
pub(super) struct Dock {
    pub(super) docked: bool,
    pub(super) restore_dock: bool,
    pub(super) dock_return: Option<(bool, InspectorTab)>,
}

#[derive(Default)]
pub(super) struct Labels {
    pub(super) visible: bool,
    pub(super) mode: LabelMode,
    pub(super) menu: bool,
}

pub(super) struct Job {
    pub(super) nucleus: Nucleus,
    pub(super) serial: u64,
    pub(super) pending: Option<JobKey>,
    pub(super) result_epoch: Option<u64>,
    pub(super) result: Option<Arc<Report>>,
    pub(super) presentation: Option<super::presentation::Presentation>,
    pub(super) notice: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            collapsed: false,
            details: false,
            plot: true,
            palette: Palette {
                position: None,
                width: DEFAULT_SIZE.width,
                height: DEFAULT_SIZE.height,
                resized: false,
                rendered_size: DEFAULT_SIZE,
            },
            dock: Dock::default(),
            labels: Labels::default(),
            job: Job {
                nucleus: Nucleus::H1,
                serial: 0,
                pending: None,
                result_epoch: None,
                result: None,
                presentation: None,
                notice: None,
            },
            checked: None,
        }
    }
}

impl State {
    pub(in crate::app) fn is_pending(&self) -> bool {
        self.job.pending.is_some()
    }

    pub(in crate::app) fn clear_pending(&mut self) {
        self.job.pending = None;
    }

    #[cfg(test)]
    pub(in crate::app) fn notice(&self) -> Option<&str> {
        self.job.notice.as_deref()
    }

    #[cfg(test)]
    pub(in crate::app) fn docking_state_for_test(
        &self,
    ) -> (bool, bool, Option<(bool, InspectorTab)>) {
        (
            self.dock.docked,
            self.dock.restore_dock,
            self.dock.dock_return,
        )
    }

    pub(super) fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
        self.labels.menu = false;
    }

    pub(super) fn toggle_details(&mut self) {
        self.details = !self.details;
    }

    pub(super) fn toggle_plot(&mut self) {
        self.plot = !self.plot;
    }

    pub(super) fn toggle_labels(&mut self) {
        if self.job.result.is_some() {
            self.labels.visible = !self.labels.visible;
            self.labels.menu = false;
        }
    }

    pub(super) fn set_label_mode(&mut self, mode: LabelMode) {
        self.labels.mode = mode;
        self.labels.menu = false;
    }

    pub(super) fn set_label_menu(&mut self, open: bool) {
        self.labels.menu = open && self.labels.visible;
    }

    pub(super) fn move_to(&mut self, position: Point) {
        if position.x.is_finite() && position.y.is_finite() {
            self.palette.position = Some(position);
        }
    }

    pub(super) fn resize(&mut self, size: Size) {
        if size.width.is_finite() && size.height.is_finite() {
            self.palette.width = size.width.clamp(MIN_SIZE.width, MAX_SIZE.width);
            self.palette.height = size.height.clamp(MIN_SIZE.height, MAX_SIZE.height);
            self.palette.resized = true;
        }
    }

    pub(super) fn reset_position(&mut self) {
        self.palette.position = None;
    }

    pub(super) fn set_rendered_size(&mut self, size: Size) {
        self.palette.rendered_size = size;
    }

    fn is_stale(&self, document: &Document, epoch: u64) -> bool {
        let stale_pending = self.job.pending.as_ref().is_some_and(|key| {
            !identity_current(document, key.epoch == epoch, &key.ids, &key.fingerprint)
        });
        let stale_result = self.job.result.as_ref().is_some_and(|report| {
            !identity_current(
                document,
                self.job.result_epoch == Some(epoch),
                &report.atom_ids,
                &report.fingerprint,
            )
        });
        stale_pending || stale_result
    }

    pub(in crate::app) fn invalidate_if_changed(&mut self, document: &Document, epoch: u64) {
        // Commit observers run before revision advances and before computed
        // labels are cleared. Never cache the document observed at this point.
        self.checked = None;
        if self.is_stale(document, epoch) {
            self.job.pending = None;
            self.job.result = None;
            self.job.presentation = None;
            self.labels.visible = false;
            self.labels.menu = false;
            self.job.serial = self.job.serial.wrapping_add(1);
            self.job.notice = Some(STALE_NOTICE.into());
        }
    }

    pub(in crate::app) fn invalidate_if_changed_cached(
        &mut self,
        document: &Document,
        epoch: u64,
        revision: u64,
    ) {
        if self
            .checked
            .as_ref()
            .is_some_and(|checked| checked.matches(epoch, revision, &self.job))
        {
            #[cfg(test)]
            debug_assert!(!self.is_stale(document, epoch));
            return;
        }
        let checked = Checked {
            epoch,
            revision,
            pending: self.job.pending.clone(),
            // Holding the report prevents address reuse and also makes direct
            // job.result writes observable without relying on setter methods.
            result: self.job.result.clone(),
            result_epoch: self.job.result_epoch,
        };
        self.invalidate_if_changed(document, epoch);
        if checked.matches(epoch, revision, &self.job) {
            self.checked = Some(checked);
        }
    }
}
