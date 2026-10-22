//! Import EMF through a disposable native playback process on Windows.
use crate::pictures::Picture;
use std::path::Path;

#[cfg(windows)]
pub const PICTURE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "webp", "emf"];
#[cfg(not(windows))]
pub const PICTURE_EXTENSIONS: &[&str] = crate::pictures::RASTER_EXTENSIONS;

pub const EMF_IMPORT_NOTICE: &str = "EMF imported at physical size. Save .rsk or export EMF on Windows to retain vectors; other formats use a preview.";
pub const EMF_PANEL_NOTICE: &str =
    "Windows EMF export keeps the original vectors. PNG, SVG, PDF and print use a preview.";

pub async fn open(path: &Path) -> Result<Picture, String> {
    let source = path.to_owned();
    let bytes = crate::task::spawn_blocking(move || {
        crate::pictures::read_bounded(&source, "Choose a picture file no larger than 16 MB")
    })
    .await
    .map_err(|error| error.to_string())??;
    open_bytes(path, bytes).await
}

/// A picture from a file's contents; the extension of `name`, a file name or
/// path, tells EMF apart.
pub async fn open_bytes(name: &Path, bytes: Vec<u8>) -> Result<Picture, String> {
    #[cfg(windows)]
    if name
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("emf"))
    {
        return open_emf(bytes).await;
    }
    #[cfg(not(windows))]
    let _ = name;
    crate::task::spawn_blocking(move || Picture::import(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(windows)]
async fn open_emf(bytes: Vec<u8>) -> Result<Picture, String> {
    use crate::workers::{CREATE_NO_WINDOW, EMF_IMPORT_WORKER};
    use reshiki_windows::PLAYBACK_LIMIT;
    use std::process::Stdio;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        process::Command,
    };
    if bytes.len() > crate::pictures::MAX_BYTES {
        return Err("Choose an EMF file no larger than 16 MB".into());
    }
    let bytes = crate::task::spawn_blocking(move || {
        // from_emf validates again when accepting the completed preview.
        crate::pictures::emf_dimensions(&bytes)?;
        Ok::<_, String>(bytes)
    })
    .await
    .map_err(|error| error.to_string())??;
    let mut command = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
    command
        .arg(EMF_IMPORT_WORKER)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let mut input = child.stdin.take().ok_or("Missing EMF worker input")?;
    let output = child.stdout.take().ok_or("Missing EMF worker output")?;
    let work = async {
        let write = async {
            input.write_all(&bytes).await?;
            input.flush().await?;
            // Tokio's Windows process pipe shutdown is a no-op. Owning
            // and dropping this handle sends the EOF the worker needs.
            drop(input);
            Ok::<_, std::io::Error>(())
        };
        let read = async {
            let mut preview = Vec::new();
            output
                .take(crate::pictures::MAX_BYTES as u64 + 1)
                .read_to_end(&mut preview)
                .await?;
            Ok::<_, std::io::Error>(preview)
        };
        let (_, preview) = tokio::try_join!(write, read).map_err(|error| error.to_string())?;
        let status = child.wait().await.map_err(|error| error.to_string())?;
        if !status.success() {
            return Err("Windows could not render this EMF picture".into());
        }
        Ok::<_, String>(preview)
    };
    let preview = tokio::time::timeout(PLAYBACK_LIMIT, work)
        .await
        .map_err(|_| "EMF playback exceeded its 30-second limit".to_owned())??;
    crate::task::spawn_blocking(move || Picture::from_emf(&bytes, &preview))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests;
