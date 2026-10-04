//! A safe command bridge to the clipboard instance Iced already owns.

use std::sync::Mutex;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use smithay_clipboard::rich::Client;

use crate::protocol::{self, Response};

enum Desktop {
    Unknown,
    X11,
    Wayland(usize),
    Unavailable(String),
}

static DESKTOP: Mutex<Desktop> = Mutex::new(Desktop::Unknown);

pub(crate) fn available() -> bool {
    let Ok(desktop) = DESKTOP.lock() else {
        return false;
    };
    match &*desktop {
        Desktop::Wayland(_) | Desktop::X11 => true,
        Desktop::Unknown => crate::configured("WAYLAND_DISPLAY") || crate::configured("DISPLAY"),
        Desktop::Unavailable(_) => false,
    }
}

/// Observe the actual window backend after Iced creates its clipboard owner.
/// A Wayland address is only a lookup key, never dereferenced or retained as an
/// owning display handle. Iced's owner stops the worker before its window drops.
pub fn initialize_clipboard(window: &(impl HasDisplayHandle + ?Sized)) {
    let desktop = match window.display_handle().map(|handle| handle.as_raw()) {
        Ok(RawDisplayHandle::Wayland(display)) => {
            Desktop::Wayland(display.display.as_ptr() as usize)
        }
        Ok(RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)) => Desktop::X11,
        Ok(_) => {
            Desktop::Unavailable("This window has no supported Linux clipboard backend".into())
        }
        Err(error) => {
            Desktop::Unavailable(format!("Could not inspect the window clipboard: {error}"))
        }
    };
    if let Ok(mut current) = DESKTOP.lock() {
        *current = desktop;
    }
}

fn client() -> Result<Option<Client>, String> {
    let desktop = DESKTOP
        .lock()
        .map_err(|_| "The desktop clipboard state is unavailable")?;
    match &*desktop {
        Desktop::Wayland(key) => Client::for_display(*key).map(Some).ok_or_else(|| {
            "The window clipboard is not ready. Focus ReShiki and try again.".into()
        }),
        Desktop::X11 => Ok(None),
        Desktop::Unknown if !crate::configured("WAYLAND_DISPLAY") => Ok(None),
        Desktop::Unknown => Err(
            "The Wayland window clipboard has not initialized. Try again after the window opens."
                .into(),
        ),
        Desktop::Unavailable(error) => Err(error.clone()),
    }
}

/// Performs a bounded request through the GUI owner. `None` selects the existing
/// X11 worker; a Wayland error never falls back to a different display or global
/// clipboard protocol. JSON keeps the native module's existing validation and
/// MIME mapping authoritative for both transports.
pub async fn gui_clipboard_request(input: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Some(client) = client()? else {
        return Ok(None);
    };
    let request = protocol::parse_request(input)?;
    let response = if request.operation == "write" {
        let write = client.write(protocol::prepare_offer(&request.representations)?);
        drop(request);
        #[cfg(feature = "wayland-qa")]
        let result = crate::wayland_qa::write(write).await;
        #[cfg(not(feature = "wayland-qa"))]
        let result = write.await;
        result.map_err(|error| error.to_string())?;
        Response::empty()
    } else {
        let picture_only = request.operation == "read_picture";
        let mime_types = protocol::READABLE
            .iter()
            .filter(|(_, _, picture)| !picture_only || *picture)
            .flat_map(|(_, types, _)| types.iter().map(|mime| (*mime).to_owned()))
            .collect();
        let item = client
            .read(mime_types)
            .await
            .map_err(|error| error.to_string())?;
        let representation = item
            .map(|item| {
                let (kind, _) = protocol::choose_type(|mime| mime == item.mime_type, picture_only)
                    .ok_or("The clipboard returned an unrequested MIME representation")?;
                protocol::representation(kind, &item.bytes)
            })
            .transpose()?;
        Response::read(representation)
    };
    let output = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
    if output.len() > protocol::JSON_LIMIT {
        return Err("Clipboard response is too large".into());
    }
    Ok(Some(output))
}
