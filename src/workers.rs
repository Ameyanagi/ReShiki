//! Worker modes: the flags that relaunch this executable as a disposable
//! worker, and the dispatch `main` (and the benchmark example) run first.
use std::{ffi::OsStr, io::Write};

#[cfg(windows)]
pub use reshiki_windows::{EMF_FILE_WORKER, EMF_IMPORT_WORKER, EMF_OFFICE_WORKER};

pub const NAMING_WORKER: &str = "--naming-worker";
pub const GEOMETRY_WORKER: &str = "--geometry-worker";
pub const INCHI_WORKER: &str = "--inchi-worker";
pub const CLIPBOARD_WORKER: &str = "--clipboard-worker";
pub const PRINT_WORKER: &str = "--print-worker";

/// Windows process flag shared by facade spawners. Lower-crate copies remain in
/// io geometry/client.rs, io engine/reference.rs, chemistry inchi/generator.rs,
/// and native/windows metafile_worker.rs to avoid a dependency on this facade.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A worker selected by one of the flags above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Worker {
    #[cfg(windows)]
    Emf(&'static str),
    Naming,
    Geometry,
    Inchi,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Clipboard,
    #[cfg(target_os = "macos")]
    Print,
}

/// The worker `first` selects, in dispatch order.
fn worker(first: &OsStr) -> Option<Worker> {
    const WORKERS: &[(&str, Worker)] = &[
        #[cfg(windows)]
        (EMF_IMPORT_WORKER, Worker::Emf(EMF_IMPORT_WORKER)),
        #[cfg(windows)]
        (EMF_FILE_WORKER, Worker::Emf(EMF_FILE_WORKER)),
        #[cfg(windows)]
        (EMF_OFFICE_WORKER, Worker::Emf(EMF_OFFICE_WORKER)),
        (NAMING_WORKER, Worker::Naming),
        (GEOMETRY_WORKER, Worker::Geometry),
        // LibreOffice owns its flags; `run` checks them when this table has no match.
        (INCHI_WORKER, Worker::Inchi),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        (CLIPBOARD_WORKER, Worker::Clipboard),
        #[cfg(target_os = "macos")]
        (PRINT_WORKER, Worker::Print),
    ];
    WORKERS
        .iter()
        .find_map(|(flag, mode)| (first == OsStr::new(flag)).then_some(*mode))
}

impl Worker {
    fn run(self) -> i32 {
        match self {
            #[cfg(windows)]
            Self::Emf(mode) => report(reshiki_windows::emf_worker(mode), "EMF playback failed: "),
            Self::Naming => {
                crate::naming::worker::run();
                0
            }
            Self::Geometry => {
                crate::geometry::worker::run();
                0
            }
            Self::Inchi => {
                crate::chemistry::inchi::worker::run();
                0
            }
            #[cfg(target_os = "linux")]
            Self::Clipboard => report(reshiki_linux::clipboard_worker(), ""),
            #[cfg(target_os = "macos")]
            Self::Clipboard => report(reshiki_macos::workers::clipboard(), ""),
            #[cfg(target_os = "macos")]
            Self::Print => report(reshiki_macos::workers::print(), ""),
        }
    }
}

/// 0, or 1 after printing `{prefix}{error}` to stderr.
fn report(result: Result<(), String>, prefix: &str) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "{prefix}{error}");
            1
        }
    }
}

/// Runs the worker `first` selects and returns its exit code: 0 on success,
/// 1 after printing one stderr line. Any other argument returns `None`.
///
/// The order is today's: EMF, naming, geometry, LibreOffice, InChI, then the
/// clipboard and print workers. The flags are distinct, so checking the table
/// before LibreOffice's own flags cannot change which worker runs.
pub fn run(first: &OsStr) -> Option<i32> {
    match worker(first) {
        Some(worker) => Some(worker.run()),
        None => crate::libreoffice::run(Some(first))
            .map(|result| report(result, "LibreOffice integration failed: ")),
    }
}

#[cfg(test)]
mod tests;
