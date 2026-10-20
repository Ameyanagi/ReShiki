//! Release checks and links in the browser, where the host page ships updates.
use super::{Channel, Release};

pub async fn check(_channel: Channel, _manual: bool) -> Result<Release, String> {
    Err("The browser editor updates with the page that hosts it.".into())
}

pub async fn open_nightly_download(release: Release) -> Result<(), String> {
    super::open_release(Some(release)).await
}

/// Opens `url` in a new tab.
pub async fn open_url(url: String) -> Result<(), String> {
    web_sys::window()
        .ok_or("The page has no window")?
        .open_with_url_and_target(&url, "_blank")
        .map(|_| ())
        .map_err(|_| "Could not open the link".into())
}
