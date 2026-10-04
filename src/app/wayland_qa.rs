//! Observation only: all drawing and clipboard commands still arrive through
//! the real compositor's input path. Never supplies a clipboard acknowledgement.

use super::{App, Message};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn event(message: &Message) -> Option<&'static str> {
    Some(match message {
        Message::Tab(_, message) => return event(message),
        Message::LinuxClipboardWindow(_) => "window",
        Message::FilePrepared(_) => "opened",
        Message::SelectAll => "selected",
        Message::Copy(false) => "copy",
        Message::Copy(true) => "cut",
        Message::CopyImage => "copy_image",
        Message::ClipboardWritten { .. } => "clipboard_written",
        Message::Paste => "paste",
        Message::ClipboardRead { .. } => "clipboard_read",
        Message::Undo => "undo",
        Message::Redo => "redo",
        Message::Saved(..) => "saved",
        _ => return None,
    })
}

pub(super) fn record(app: &App, event: &str) {
    let Some(directory) = std::env::var_os("RESHIKI_WAYLAND_QA_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let result = (|| {
        let document = app.tab.doc.file_json()?;
        let filename = format!("state-{sequence:04}-{event}.rsk");
        std::fs::write(directory.join(&filename), document).map_err(|error| error.to_string())?;
        let snapshot = serde_json::json!({
            "sequence": sequence,
            "event": event,
            "document": filename,
            "atoms": app.tab.doc.atoms.len(),
            "bonds": app.tab.doc.bonds.len(),
            "selected": app.tab.selected,
            "revision": app.tab.revision,
            "busy": app.clipboard_working(),
            "error": app.error,
            "status": app.status,
        });
        let bytes = serde_json::to_vec_pretty(&snapshot).map_err(|error| error.to_string())?;
        reshiki::storage::write_atomic(&directory.join("state.json"), &bytes)?;
        std::fs::write(directory.join(format!("state-{sequence:04}.json")), bytes)
            .map_err(|error| error.to_string())
    })();
    if let Err(error) = result {
        eprintln!("Wayland QA observation failed: {error}");
    }
}
