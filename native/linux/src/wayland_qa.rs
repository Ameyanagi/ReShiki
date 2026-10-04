//! Test-only cancellation of an actual GUI-owner request.

use std::{io, path::PathBuf, time::Duration};

pub(crate) async fn write(request: smithay_clipboard::rich::Request<()>) -> io::Result<()> {
    let Some(directory) = std::env::var_os("RESHIKI_WAYLAND_QA_DIR").map(PathBuf::from) else {
        return request.await;
    };
    let cancellation = async {
        loop {
            if std::fs::remove_file(directory.join("cancel-write")).is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    tokio::pin!(request);
    tokio::select! {
        result = &mut request => result,
        () = cancellation => {
            // Returning drops the real Request, triggering its normal atomic
            // cancellation and maintenance wake. Never reports publication Ok.
            Err(io::Error::new(io::ErrorKind::Interrupted, "clipboard request cancelled by QA"))
        }
    }
}
