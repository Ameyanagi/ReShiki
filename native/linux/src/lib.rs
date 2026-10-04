//! Bounded Linux clipboard transport on the GUI's actual display backend.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

mod gui;
mod protocol;
mod x11;

use std::io::{Read, Write};

pub use gui::{gui_clipboard_request, initialize_clipboard};
pub use protocol::{JSON_LIMIT, LIMIT, TRANSFER_TOTAL_TIMEOUT};

/// Prefer the observed GUI backend, including inherited Wayland socket handles.
/// The selected transport still verifies connection/protocol readiness.
pub fn clipboard_available() -> bool {
    gui::available()
}

fn configured(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn respond(response: &protocol::Response) -> Result<(), String> {
    let bytes = serde_json::to_vec(response).map_err(|e| e.to_string())?;
    if bytes.len() > JSON_LIMIT {
        return Err("Clipboard response is too large".into());
    }
    let mut output = std::io::stdout().lock();
    output.write_all(&bytes).map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

/// The standalone helper is X11-only. Standard Wayland clipboard requests use
/// the already-focused GUI's data device through `gui_clipboard_request`.
/// Enters before GUI initialization. Writes a single bounded response line.
/// A successful write then serves selection requests until ownership is lost.
pub fn clipboard_worker() -> Result<(), String> {
    let result = run();
    if let Err(error) = &result {
        // Also works before a display connection exists. After the success line,
        // the parent may have closed stdout; that does not prolong ownership.
        let _ = respond(&protocol::Response::Failure {
            error: error.clone(),
        });
    }
    result
}

fn run() -> Result<(), String> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(JSON_LIMIT as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    let request = protocol::parse_request(&input)?;
    // Decode and validate the complete offer before touching either selection.
    let offer = if request.operation == "write" {
        Some(protocol::prepare_offer(&request.representations)?)
    } else {
        None
    };
    let picture_only = request.operation == "read_picture";
    // A persistent owner needs only decoded shared bytes, not another retained
    // base64 copy of every representation for its entire lifetime.
    drop(request);
    drop(input);
    if configured("DISPLAY") {
        let mut clipboard = x11::Clipboard::connect()?;
        if let Some(offer) = offer {
            clipboard.publish(offer)?;
            respond(&protocol::Response::empty())?;
            clipboard.serve()
        } else {
            let representation = clipboard.read(picture_only)?;
            respond(&protocol::Response::read(representation))
        }
    } else {
        Err("The standalone clipboard worker requires an X11 display. In a Wayland desktop, use Copy or Paste in the focused ReShiki window.".into())
    }
}
