//! GUI-free startup arguments and host edit-session ownership.
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
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
    #[cfg(any(windows, target_os = "linux", test))]
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
    #[cfg(any(windows, target_os = "linux", test))]
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

    #[cfg(any(windows, target_os = "linux"))]
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

    #[cfg(any(windows, target_os = "linux", test))]
    pub(crate) fn has_token(&self, token: [u8; 16]) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.token == token)
    }

    pub(crate) fn preparable(&self) -> bool {
        self.lease
            .as_ref()
            .is_none_or(|lease| lease.alive() && lease.phase() == Phase::Prepared)
    }

    pub(crate) fn same_path(&self, path: &Path) -> bool {
        same_file(&self.path, path)
    }

    pub(crate) fn reject(&self) {
        if let Some(lease) = &self.lease {
            lease.transition(Phase::Prepared, Phase::Rejected);
        }
    }

    pub(crate) fn finish(&self) {
        if let Some(lease) = &self.lease {
            lease.transition(Phase::Open, Phase::Closed);
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Arguments {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) shortcut_examples: bool,
    pub(crate) office_host: Option<Host>,
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Arguments {
    let mut args = args.into_iter();
    let mut parsed = Arguments::default();
    while let Some(arg) = args.next() {
        if arg == "--open" {
            if let Some(path) = args.next() {
                parsed.paths.push(path.into());
            }
        } else if arg == "--shortcut-examples" {
            parsed.shortcut_examples = true;
        }
        if arg == "--office-edit" {
            parsed.office_host = Some(Host::Office);
        } else if arg == "--libreoffice-edit" {
            parsed.office_host = Some(Host::LibreOffice);
        } else if arg == "--office-addin-edit" {
            parsed.office_host = Some(Host::Microsoft365);
        }
    }
    parsed
}

/// Two paths name the same file, also through links or different spellings.
pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .is_some_and(|a| std::fs::canonicalize(b).ok() == Some(a))
}

#[cfg(test)]
mod tests;
