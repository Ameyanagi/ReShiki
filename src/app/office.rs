//! An Office save target belongs to its document tab, not to the whole window.
use super::{App, Message, document_tab::TabId, files};
use iced::Task;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Host {
    Office,
    LibreOffice,
    Microsoft365,
}

impl Host {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Office => "Office",
            Self::LibreOffice => "LibreOffice",
            Self::Microsoft365 => "Microsoft 365",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub(crate) enum Phase {
    Prepared,
    Open,
    Closed,
    Rejected,
    Lost,
}

/// The watched launcher owns the host's temporary file. Its lease stays open
/// until this particular tab and its explicit file writes have finished.
#[derive(Clone)]
pub(crate) struct Lease {
    pub(crate) token: [u8; 16],
    phase: Arc<AtomicU8>,
    alive: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OfficeLease")
            .field("phase", &self.phase())
            .finish_non_exhaustive()
    }
}

impl Lease {
    pub(crate) fn new(token: [u8; 16], alive: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            token,
            phase: Arc::new(AtomicU8::new(Phase::Prepared as u8)),
            alive: Arc::new(alive),
        }
    }

    pub(crate) fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Acquire) {
            0 => Phase::Prepared,
            1 => Phase::Open,
            2 => Phase::Closed,
            3 => Phase::Rejected,
            _ => Phase::Lost,
        }
    }

    pub(crate) fn transition(&self, from: Phase, to: Phase) -> bool {
        self.phase
            .compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn alive(&self) -> bool {
        (self.alive)()
    }

    pub(crate) fn references(&self) -> usize {
        Arc::strong_count(&self.phase)
    }

    pub(crate) fn writable(&self) -> bool {
        self.phase() == Phase::Open && self.alive()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) path: PathBuf,
    pub(crate) host: Host,
    pub(crate) lease: Option<Lease>,
}

impl Binding {
    pub(crate) fn standalone(path: PathBuf, host: Host) -> Self {
        Self {
            path,
            host,
            lease: None,
        }
    }

    pub(crate) fn writable(&self) -> bool {
        self.lease.as_ref().is_none_or(Lease::writable)
    }

    pub(crate) fn same_path(&self, path: &Path) -> bool {
        self.path == path
            || std::fs::canonicalize(&self.path)
                .ok()
                .is_some_and(|source| std::fs::canonicalize(path).ok() == Some(source))
    }

    pub(crate) fn finish(&self) {
        if let Some(lease) = &self.lease {
            lease.transition(Phase::Open, Phase::Closed);
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    // Keep the tab while its write settles. A failed/cancelled save cancels this
    // close, so its Office binding and drawing remain available for retry.
    pub(super) closing: Vec<TabId>,
}

impl App {
    pub(super) fn detach_office(&mut self) {
        if let Some(binding) = self.tab.office.take() {
            binding.finish();
        }
    }

    pub(super) fn finish_office_closes(&mut self) -> Task<Message> {
        if self.exit.frozen() || self.pending.is_some() || self.file_io.saving {
            return Task::none();
        }
        let Some(id) = self.office.closing.first().copied() else {
            return Task::none();
        };
        self.office.closing.remove(0);
        self.tab_action(super::tabs::Action::Close(Some(id)))
    }

    pub(super) fn office_prepared(
        &mut self,
        binding: Binding,
        opened: files::Opened,
    ) -> Task<Message> {
        let Some((path, result)) = opened else {
            if let Some(lease) = &binding.lease {
                lease.transition(Phase::Prepared, Phase::Rejected);
            }
            return Task::none();
        };
        let result = match result {
            Ok(files::Prepared::Native(doc)) => Ok(doc),
            Ok(_) => Err("Office editing requires a native ReShiki drawing".into()),
            Err(error) => Err(error),
        };
        let conflict = self.strip().any(|tab| {
            tab.path
                .as_ref()
                .is_some_and(|path| binding.same_path(path))
                && (tab.office.is_some()
                    || self.edited(tab)
                    || self.file_io.saving && self.file_io.saving_tab == Some(tab.id))
        });
        if self.exit.frozen()
            || self.updates.restarting
            || conflict
            || !binding
                .lease
                .as_ref()
                .is_none_or(|lease| lease.alive() && lease.phase() == Phase::Prepared)
        {
            if let Some(lease) = &binding.lease {
                lease.transition(Phase::Prepared, Phase::Rejected);
            }
            self.status =
                "This drawing already belongs to another edit session, or the editor is closing"
                    .into();
            self.error = true;
            return Task::none();
        }
        match result {
            Ok(doc) => {
                if path != binding.path {
                    if let Some(lease) = &binding.lease {
                        lease.transition(Phase::Prepared, Phase::Rejected);
                    }
                    self.status = "Office edit source does not match the opened file".into();
                    self.error = true;
                    return Task::none();
                }
                if let Some(lease) = &binding.lease
                    && !lease.transition(Phase::Prepared, Phase::Open)
                {
                    return Task::none();
                }
                let task = self.file_prepared(Some((path, Ok(files::Prepared::Native(doc)))));
                self.tab.path = Some(binding.path.clone());
                self.tab.office = Some(binding);
                task
            }
            Err(error) => {
                if let Some(lease) = &binding.lease {
                    lease.transition(Phase::Prepared, Phase::Rejected);
                }
                self.status = error;
                self.error = true;
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests;
