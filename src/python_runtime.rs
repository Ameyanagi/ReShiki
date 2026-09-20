//! Locate the packaged worker project and prepare its user-owned uv environment.
use std::{
    ffi::OsStr,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

const INSTALL_UV: &str = "ReShiki needs uv to set up local chemistry packages. Install uv from https://docs.astral.sh/uv/getting-started/installation/, then retry. A separate Python installation is not required.";

fn python_request(os: &str, arch: &str) -> &'static str {
    // RDKit's Windows wheels are x64. The ARM UI uses a separate x64 worker
    // through Windows 11's built-in emulation; the drawing process stays native.
    if os == "windows" && arch == "aarch64" {
        "cpython-3.12-windows-x86_64-none"
    } else {
        "3.12"
    }
}

pub(crate) fn packaged_project(executable: &Path) -> Option<PathBuf> {
    let directory = executable.parent()?;
    let project = if cfg!(target_os = "macos") {
        directory.parent()?.join("Resources/chemistry")
    } else {
        directory.join("chemistry")
    };
    ["pyproject.toml", "uv.lock", "engine/worker.py"]
        .iter()
        .all(|name| project.join(name).is_file())
        .then_some(project)
}

fn uv_candidates(path: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let name = if cfg!(windows) { "uv.exe" } else { "uv" };
    let mut candidates = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .collect::<Vec<_>>();
    if let Some(home) = home {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join(".cargo/bin").join(name));
    }
    // Finder launches often have a minimal PATH without Homebrew or uv's installer path.
    #[cfg(target_os = "macos")]
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/uv"),
        PathBuf::from("/usr/local/bin/uv"),
    ]);
    #[cfg(windows)]
    if let Some(home) = home {
        candidates.push(home.join("AppData/Local/Microsoft/WinGet/Links/uv.exe"));
    }
    candidates
}

fn find_uv() -> Result<PathBuf, String> {
    if let Some(value) = crate::compatibility::environment("UV") {
        let path = PathBuf::from(value);
        return path
            .is_file()
            .then_some(path)
            .ok_or_else(|| format!("RESHIKI_UV does not point to a uv executable. {INSTALL_UV}"));
    }
    let directories = directories_next::BaseDirs::new();
    uv_candidates(
        std::env::var_os("PATH").as_deref(),
        directories.as_ref().map(|dirs| dirs.home_dir()),
    )
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| INSTALL_UV.into())
}

fn environment_path(project: &Path, cache: &Path) -> Result<PathBuf, String> {
    let lock = std::fs::read(project.join("uv.lock"))
        .map_err(|error| format!("Could not read chemistry dependencies: {error}"))?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    lock.hash(&mut hash);
    std::env::consts::ARCH.hash(&mut hash);
    std::env::consts::OS.hash(&mut hash);
    python_request(std::env::consts::OS, std::env::consts::ARCH).hash(&mut hash);
    Ok(cache.join(format!(
        "{}-{:016x}",
        env!("CARGO_PKG_VERSION"),
        hash.finish()
    )))
}

pub(crate) async fn prepare(project: &Path) -> Result<PathBuf, String> {
    let uv = find_uv()?;
    let cache = if let Some(path) = crate::compatibility::environment("RUNTIME_DIR") {
        PathBuf::from(path)
    } else {
        directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
            .ok_or("Could not find the local chemistry cache directory")?
            .cache_dir()
            .join("chemistry")
    };
    if !cache.is_absolute() {
        return Err("RESHIKI_RUNTIME_DIR must be an absolute path".into());
    }
    let environment = environment_path(project, &cache)?;
    std::fs::create_dir_all(&cache)
        .map_err(|error| format!("Could not create the local chemistry environment: {error}"))?;
    let mut command = Command::new(uv);
    command
        .args([
            "sync",
            "--locked",
            "--no-dev",
            "--python",
            python_request(std::env::consts::OS, std::env::consts::ARCH),
            "--project",
        ])
        .arg(project)
        .env("UV_PROJECT_ENVIRONMENT", &environment)
        .env_remove("VIRTUAL_ENV")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(600), command.output())
        .await
        .map_err(|_| "Local chemistry setup timed out. Check your internet connection and retry; existing drawings are unchanged.".to_owned())?
        .map_err(|error| format!("Could not start uv: {error}. {INSTALL_UV}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not prepare local chemistry packages. First use requires an internet connection. Check uv and retry.\n{}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(4000)
                .collect::<String>()
        ));
    }
    let python = environment.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    python.is_file().then_some(python).ok_or_else(|| {
        "uv completed without creating the chemistry interpreter. Retry setup.".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_arm_requests_an_x64_worker_and_other_platforms_remain_native() {
        assert_eq!(
            python_request("windows", "aarch64"),
            "cpython-3.12-windows-x86_64-none"
        );
        for (os, arch) in [
            ("windows", "x86_64"),
            ("linux", "aarch64"),
            ("linux", "x86_64"),
            ("macos", "aarch64"),
        ] {
            assert_eq!(python_request(os, arch), "3.12");
        }
    }

    #[test]
    fn detects_a_relocated_worker_project_only_when_complete() {
        let directory = tempfile::tempdir().unwrap();
        let (executable, project) = if cfg!(target_os = "macos") {
            (
                directory.path().join("ReShiki.app/Contents/MacOS/reshiki"),
                directory
                    .path()
                    .join("ReShiki.app/Contents/Resources/chemistry"),
            )
        } else {
            (
                directory.path().join("reshiki"),
                directory.path().join("chemistry"),
            )
        };
        std::fs::create_dir_all(project.join("engine")).unwrap();
        assert_eq!(packaged_project(&executable), None);
        for name in ["pyproject.toml", "uv.lock", "engine/worker.py"] {
            std::fs::write(project.join(name), b"fixture").unwrap();
        }
        assert_eq!(packaged_project(&executable), Some(project));
    }

    #[test]
    fn environments_are_separate_for_changed_dependencies_and_outside_the_app() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("read only app");
        let cache = directory.path().join("user cache");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("uv.lock"), b"first").unwrap();
        let first = environment_path(&project, &cache).unwrap();
        assert!(first.starts_with(&cache));
        assert!(!first.starts_with(&project));
        std::fs::write(project.join("uv.lock"), b"second").unwrap();
        assert_ne!(environment_path(&project, &cache).unwrap(), first);
    }

    #[test]
    fn discovers_installer_location_with_a_minimal_gui_path() {
        let home = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "uv.exe" } else { "uv" };
        assert!(
            uv_candidates(None, Some(home.path()))
                .contains(&home.path().join(".local/bin").join(name))
        );
    }
}
