//! Import EMF through a disposable native playback process on Windows.
use crate::pictures::Picture;
use std::path::Path;

pub async fn open(path: &Path) -> Result<Picture, String> {
    #[cfg(windows)]
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("emf"))
    {
        use std::{io::Read, process::Stdio, time::Duration};
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            process::Command,
        };
        let path = path.to_owned();
        let bytes = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.is_file() || metadata.len() > crate::pictures::MAX_BYTES as u64 {
                return Err("Choose an EMF file no larger than 16 MB".into());
            }
            let mut bytes = Vec::new();
            file.take(crate::pictures::MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            // from_emf validates again when accepting the completed preview.
            crate::pictures::emf_dimensions(&bytes)?;
            Ok::<_, String>(bytes)
        })
        .await
        .map_err(|error| error.to_string())??;
        let mut command = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
        command
            .arg("--emf-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command.creation_flags(0x0800_0000);
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
            Picture::from_emf(&bytes, &preview)
        };
        return tokio::time::timeout(Duration::from_secs(30), work)
            .await
            .map_err(|_| "EMF playback exceeded its 30-second limit".to_owned())?;
    }
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || Picture::open(&path))
        .await
        .map_err(|error| error.to_string())?
}
