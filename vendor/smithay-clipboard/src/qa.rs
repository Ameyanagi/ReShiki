//! One-shot scheduling delays for real-compositor tests; never supplies results.

use std::time::{Duration, Instant};

pub(crate) fn hold(phase: &str, marker: &str) {
    let Some(directory) = std::env::var_os("RESHIKI_WAYLAND_QA_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    if std::fs::remove_file(directory.join(format!("gate-{phase}.arm"))).is_err() {
        return;
    }
    if let Err(error) = std::fs::write(
        directory.join(format!("gate-{phase}.entered")),
        format!("{phase}\n{marker}\n"),
    ) {
        eprintln!("Could not observe Wayland QA gate: {error}");
        return;
    }
    // Production deadlines keep advancing. A watchdog bounds owner Drop/join
    // even if the external test process fails before releasing the worker.
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        if std::fs::remove_file(directory.join(format!("gate-{phase}.release"))).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    eprintln!("Wayland QA {phase} gate watchdog expired");
}
