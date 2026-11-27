//! Safe, cancellable MP4 export with atomic destination replacement.

use super::{CANCELED, FPS, Options, RENDERING, SAVING, Work};
use std::path::Path;
use std::sync::atomic::Ordering::Relaxed;

pub(super) fn save(
    path: &Path,
    options: Options,
    sequence: &[usize],
    work: &Work,
    render: impl FnMut(usize, usize) -> Result<Vec<u8>, String>,
) -> Result<bool, String> {
    if work.state.load(Relaxed) == CANCELED {
        return Ok(false);
    }
    if sequence.is_empty() {
        return Err("There are no frames to export.".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = tempfile::Builder::new()
        .prefix(".reshiki-mp4-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    let movie = temporary.path().join("animation.mp4");
    let encoded = reshiki_macos::mp4::encode(
        &movie,
        reshiki_macos::mp4::Settings {
            size: options.size,
            fps: FPS as u16,
            per_frame: options.plot,
        },
        sequence,
        || work.state.load(Relaxed) == CANCELED,
        |done| work.completed.store(done, Relaxed),
        render,
    )
    .map_err(|e| e.to_string())?;
    if !encoded {
        return Ok(false);
    }
    // A cancel before this transition wins, including during native finishing.
    if work
        .state
        .compare_exchange(RENDERING, SAVING, Relaxed, Relaxed)
        .is_err()
    {
        return Ok(false);
    }
    std::fs::rename(&movie, path).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options() -> Options {
        Options {
            size: 32,
            transparent: true,
            plot: false,
        }
    }

    #[test]
    fn cancel_and_render_failure_preserve_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("animation.mp4");
        std::fs::write(&path, b"old destination").unwrap();
        let work = Work::default();
        assert!(
            !save(&path, options(), &[0], &work, |_, _| {
                work.cancel();
                Ok(vec![255; 32 * 32 * 4])
            })
            .unwrap()
        );
        let failed = save(&path, options(), &[0], &Work::default(), |_, _| {
            Err("render failed".into())
        });
        assert_eq!(failed.unwrap_err(), "render failed");
        assert_eq!(std::fs::read(&path).unwrap(), b"old destination");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn empty_sequence_preserves_destination_without_rendering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("animation.mp4");
        std::fs::write(&path, b"old destination").unwrap();
        assert!(
            save(&path, options(), &[], &Work::default(), |_, _| panic!(
                "unexpected render"
            ))
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"old destination");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
