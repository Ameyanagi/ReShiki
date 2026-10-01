//! Download only official stable artifacts; stage before touching the running app.
use super::{CURRENT_VERSION, Channel, RELEASES, Release};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct Prepared {
    directory: tempfile::TempDir,
    target: PathBuf,
    payload: PathBuf,
    handed_off: std::sync::atomic::AtomicBool,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if self.handed_off.load(std::sync::atomic::Ordering::Acquire) {
            self.directory.disable_cleanup(true);
        }
    }
}
#[derive(Debug, Clone)]
pub struct Progress(pub String);

pub fn asset_name(version: &str, os: &str, arch: &str) -> Result<String, String> {
    let v = semver::Version::parse(version).map_err(|_| "Invalid release version")?;
    if !v.pre.is_empty() || !v.build.is_empty() {
        return Err("Only stable releases can be installed".into());
    }
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return Err("No update is available for this architecture".into()),
    };
    let suffix = match (os, arch) {
        ("macos", _) => format!("macos-{arch}.dmg"),
        ("windows", _) => format!("windows-{arch}-setup.exe"),
        ("linux", _) => format!("linux-{arch}.tar.gz"),
        _ => return Err("No installer is available for this platform".into()),
    };
    Ok(format!("reshiki-{v}-{suffix}"))
}
fn expected_digest(manifest: &str, name: &str) -> Result<String, String> {
    let values: Vec<_> = manifest
        .lines()
        .filter_map(|line| {
            let (digest, file) = line.split_once(char::is_whitespace)?;
            (file.trim().trim_start_matches('*') == name).then_some(digest)
        })
        .collect();
    match values.as_slice() {
        [digest] if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(digest.to_ascii_lowercase())
        }
        _ => Err("The release checksum is missing or ambiguous. Nothing was installed.".into()),
    }
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .user_agent(concat!("ReShiki/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && matches!(
                    attempt.url().host_str(),
                    Some(
                        "github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.follow()
            } else {
                attempt.error("Unexpected update download redirect")
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}
async fn download(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    limit: u64,
    progress: &tokio::sync::mpsc::Sender<Progress>,
) -> Result<String, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Could not download the update: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Release download unavailable: {e}"))?;
    if response.content_length().is_some_and(|n| n > limit) {
        return Err("Release asset exceeds the download limit".into());
    }
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut last_mb = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download interrupted: {e}"))?
    {
        total = total.saturating_add(chunk.len() as u64);
        if total > limit {
            return Err("Release asset exceeds the download limit".into());
        }
        hash.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        if total / 1_048_576 > last_mb {
            last_mb = total / 1_048_576;
            let _ = progress.try_send(Progress(format!("Downloading update · {last_mb} MB")));
        }
    }
    file.sync_all().await.map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hash.finalize()))
}
#[cfg(not(windows))]
fn command(command: &mut Command) -> Result<String, String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Update preparation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
fn install_target(exe: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let macos = exe.parent().ok_or("Missing app bundle")?;
        let contents = macos.parent().ok_or("Missing app bundle")?;
        let app = contents.parent().ok_or("Missing app bundle")?;
        if macos.file_name().is_none_or(|s| s != "MacOS")
            || contents.file_name().is_none_or(|s| s != "Contents")
            || app.extension().is_none_or(|s| s != "app")
        {
            return Err("Run the installed ReShiki.app to use automatic updates.".into());
        }
        Ok(app.into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let folder = exe.parent().ok_or("Missing installation folder")?;
        if !folder.join("build.json").is_file() && !folder.join("unins000.exe").is_file() {
            return Err("Run an installed or portable release to use automatic updates.".into());
        }
        Ok(folder.into())
    }
}
pub async fn prepare(
    release: Release,
    progress: tokio::sync::mpsc::Sender<Progress>,
) -> Result<Arc<Prepared>, String> {
    if release.channel() != Some(Channel::Stable) {
        return Err("Nightly builds must be downloaded and installed manually.".into());
    }
    if !release.available_for(CURRENT_VERSION) {
        return Err("You already have this release or a newer version.".into());
    }
    let name = asset_name(
        &release.version,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = install_target(&exe)?;
    let directory=tempfile::Builder::new().prefix(".reshiki-update-").tempdir_in(target.parent().ok_or("Missing installation parent")?)
        .map_err(|_|"ReShiki cannot write to its installation folder. Move the app to a writable folder and try again.".to_owned())?;
    let base = format!("{RELEASES}/download/v{}", release.version);
    let client = client()?;
    let manifest = directory.path().join("SHA256SUMS");
    download(
        &client,
        &format!("{base}/SHA256SUMS"),
        &manifest,
        1024 * 1024,
        &progress,
    )
    .await?;
    let expected = expected_digest(
        &tokio::fs::read_to_string(manifest)
            .await
            .map_err(|e| e.to_string())?,
        &name,
    )?;
    let asset = directory.path().join(&name);
    let actual = download(
        &client,
        &format!("{base}/{name}"),
        &asset,
        512 * 1024 * 1024,
        &progress,
    )
    .await?;
    if actual != expected {
        return Err(
            "The update checksum did not match. Nothing was installed; please retry.".into(),
        );
    }
    let _ = progress
        .send(Progress("Verifying and preparing installation…".into()))
        .await;
    tokio::task::spawn_blocking(move || {
        let payload = stage(&asset, directory.path(), &release.version)?;
        Ok(Arc::new(Prepared {
            directory,
            target,
            payload,
            handed_off: std::sync::atomic::AtomicBool::new(false),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn stage(asset: &Path, directory: &Path, version: &str) -> Result<PathBuf, String> {
    // Verify the signed disk image before mounting; verify the app again afterward.
    command(
        Command::new("/usr/bin/codesign")
            .args([
                "--verify",
                "--strict",
                "-R",
                "=anchor apple generic and certificate leaf[subject.OU] = \"XXN44W8X56\"",
            ])
            .arg(asset),
    )?;
    let mount = directory.join("mount");
    std::fs::create_dir(&mount).map_err(|e| e.to_string())?;
    command(
        Command::new("/usr/bin/hdiutil")
            .args(["attach", "-readonly", "-nobrowse", "-mountpoint"])
            .arg(&mount)
            .arg(asset),
    )?;
    let result = (|| {
        let app = mount.join("ReShiki.app");
        command(Command::new("/usr/bin/codesign").args(["--verify","--deep","--strict","-R","=anchor apple generic and identifier \"dev.reshiki.editor\" and certificate leaf[subject.OU] = \"XXN44W8X56\""]).arg(&app))?;
        command(
            Command::new("/usr/sbin/spctl")
                .args(["--assess", "--type", "execute"])
                .arg(&app),
        )?;
        let actual = command(
            Command::new("/usr/libexec/PlistBuddy")
                .args(["-c", "Print CFBundleShortVersionString"])
                .arg(app.join("Contents/Info.plist")),
        )?;
        if actual != version {
            return Err("The downloaded app version does not match the release".into());
        }
        let payload = directory.join("ReShiki.app");
        command(Command::new("/usr/bin/ditto").arg(&app).arg(&payload))?;
        Ok(payload)
    })();
    let detached = command(Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount));
    if detached.is_err() {
        let _ = command(
            Command::new("/usr/bin/hdiutil")
                .args(["detach", "-force"])
                .arg(&mount),
        );
    }
    result
}
#[cfg(target_os = "windows")]
fn stage(asset: &Path, _directory: &Path, _version: &str) -> Result<PathBuf, String> {
    Ok(asset.into())
}
#[cfg(target_os = "linux")]
fn stage(asset: &Path, directory: &Path, version: &str) -> Result<PathBuf, String> {
    let extract = directory.join("extracted");
    std::fs::create_dir(&extract).map_err(|e| e.to_string())?;
    let listing = command(Command::new("tar").arg("-tzf").arg(asset))?;
    if listing.lines().any(|s| {
        Path::new(s).is_absolute()
            || Path::new(s)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
    }) {
        return Err("Unsafe archive entry".into());
    }
    command(
        Command::new("tar")
            .args(["--no-same-owner", "--no-same-permissions", "-xzf"])
            .arg(asset)
            .arg("-C")
            .arg(&extract),
    )?;
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let payload = extract.join(format!("reshiki-{version}-linux-{arch}"));
    let data: serde_json::Value = serde_json::from_slice(
        &std::fs::read(payload.join("build.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let self_process =
        data.pointer("/inchi/runtime").and_then(|v| v.as_str()) == Some("self-process");
    let licenses = payload.join("Licenses");
    let complete_licenses = ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "NOTICE"]
        .iter()
        .all(|name| licenses.join(name).is_file())
        && (licenses.join("THIRD-PARTY-NOTICES.txt").is_file()
            || (!self_process
                && licenses.join("rust").is_dir()
                && licenses.join("sources").is_dir()
                && licenses.join("rust-dependencies.json").is_file()));
    if data.get("version").and_then(|v| v.as_str()) != Some(version)
        || !payload.join("reshiki").is_file()
        || (!self_process && !payload.join("reshiki-inchi-helper").is_file())
        || !complete_licenses
    {
        return Err("Incomplete update package".into());
    }
    Ok(payload)
}

/// The helper acknowledges startup before the application exits. It waits for
/// this process to terminate, then installs and reopens the saved tabs.
pub async fn handoff(prepared: Arc<Prepared>, drawings: Vec<PathBuf>) -> Result<(), String> {
    let dir = prepared.directory.path();
    let ready = dir.join("ready");
    // A retry must receive an acknowledgement from the new helper, not a
    // marker left behind by an earlier failed handoff.
    match tokio::fs::remove_file(&ready).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Could not reset installer acknowledgement: {error}"
            ));
        }
    }
    let script = dir.join(if cfg!(windows) {
        "install.ps1"
    } else {
        "install.sh"
    });
    tokio::fs::write(
        &script,
        if cfg!(windows) {
            include_str!("install.ps1")
        } else {
            include_str!("install.sh")
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let log = std::fs::File::create(dir.join("install.log")).map_err(|e| e.to_string())?;
    let error = log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = handoff_command(
        &script,
        std::process::id(),
        &prepared.target,
        &prepared.payload,
        dir,
        &drawings,
        std::env::consts::OS,
    );
    cmd.stdin(Stdio::null()).stdout(log).stderr(error);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start the installer: {e}"))?;
    for _ in 0..50 {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("The installer could not start. Your app is unchanged.".into());
        }
        if ready.exists() {
            prepared
                .handed_off
                .store(true, std::sync::atomic::Ordering::Release);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("The installer did not respond. Your app is unchanged.".into())
}

fn handoff_command(
    script: &Path,
    parent: u32,
    target: &Path,
    payload: &Path,
    stage: &Path,
    drawings: &[PathBuf],
    platform: &str,
) -> Command {
    let mut cmd = if platform == "windows" {
        let mut c = Command::new("powershell.exe");
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
        c
    } else {
        Command::new("/bin/sh")
    };
    // The first six arguments retain the single-drawing helper format.
    cmd.arg(script)
        .arg(parent.to_string())
        .arg(target)
        .arg(payload)
        .arg(stage)
        .arg(
            drawings
                .first()
                .map_or_else(|| Path::new(""), PathBuf::as_path),
        )
        .arg(platform)
        .args(drawings.iter().skip(1));
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_keeps_each_path_as_an_argument_and_preserves_the_legacy_prefix() {
        use std::ffi::OsString;
        let drawings = [
            PathBuf::from("drawings/first drawing.rsk"),
            "資料/構造 β.rsk".into(),
            "drawings/first drawing.rsk".into(),
        ];
        for platform in ["macos", "linux", "windows"] {
            for count in [0, 1, drawings.len()] {
                let paths = &drawings[..count];
                let script = Path::new("staging directory/install script");
                let command = handoff_command(
                    script,
                    123,
                    Path::new("installed app"),
                    Path::new("update payload"),
                    Path::new("staging directory"),
                    paths,
                    platform,
                );
                let mut expected: Vec<OsString> = if platform == "windows" {
                    [
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                    ]
                    .map(OsString::from)
                    .to_vec()
                } else {
                    vec![]
                };
                expected.extend(
                    [
                        script.as_os_str(),
                        "123".as_ref(),
                        "installed app".as_ref(),
                        "update payload".as_ref(),
                        "staging directory".as_ref(),
                        paths.first().map_or_else(|| "".as_ref(), |p| p.as_os_str()),
                        platform.as_ref(),
                    ]
                    .map(OsString::from),
                );
                expected.extend(paths.iter().skip(1).map(|p| p.as_os_str().to_owned()));
                assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
                assert_eq!(
                    command.get_program(),
                    if platform == "windows" {
                        "powershell.exe"
                    } else {
                        "/bin/sh"
                    }
                );
            }
        }
    }

    #[tokio::test]
    async fn nightly_cannot_enter_verified_stable_installation() {
        let (sender, _) = tokio::sync::mpsc::channel(1);
        let result = prepare(
            Release {
                version: "99.0.0-nightly.20260929.20.1".into(),
            },
            sender,
        )
        .await;
        assert!(result.unwrap_err().contains("manually"));
    }

    #[cfg(unix)]
    #[test]
    fn helper_replaces_owned_files_and_rolls_back_an_interrupted_install() {
        use std::os::unix::fs::PermissionsExt;
        for (fail, missing_licenses) in [(false, false), (true, false), (false, true)] {
            let root = tempfile::tempdir().unwrap();
            let target = root.path().join("installed app");
            let stage = root.path().join("stage");
            let payload = stage.join("payload");
            std::fs::create_dir_all(&target).unwrap();
            std::fs::create_dir_all(&payload).unwrap();
            let old = "#!/bin/sh\nexit 0\n";
            for (path, body) in [
                (target.join("reshiki"), old),
                (payload.join("reshiki"), "#!/bin/sh\n# new\nexit 0\n"),
            ] {
                std::fs::write(&path, body).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            std::fs::write(target.join("reshiki-inchi-helper"), "old helper").unwrap();
            std::fs::create_dir_all(target.join("Licenses/rust/legacy")).unwrap();
            std::fs::write(target.join("Licenses/rust/legacy/LICENSE"), "old license").unwrap();
            if !missing_licenses {
                std::fs::create_dir_all(payload.join("Licenses")).unwrap();
                std::fs::write(
                    payload.join("Licenses/THIRD-PARTY-NOTICES.txt"),
                    "complete notices",
                )
                .unwrap();
            }
            std::fs::write(target.join("user drawing.rsk"), "preserve").unwrap();
            let mut script = include_str!("install.sh").to_owned();
            if fail {
                let wrapper = root.path().join("fail-move");
                let counter = root.path().join("count");
                std::fs::write(&wrapper,format!("#!/bin/sh\nn=0\n[ ! -f '{0}' ] || n=$(cat '{0}')\nn=$((n+1))\necho $n > '{0}'\n[ $n -ne 4 ] || exit 33\nexec /bin/mv \"$@\"\n",counter.display())).unwrap();
                std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
                script = script.replace("/bin/mv", &format!("'{}'", wrapper.display()));
            }
            let file = stage.join("install.sh");
            std::fs::write(&file, script).unwrap();
            let result = Command::new("/bin/sh")
                .arg(file)
                .arg("2147483647")
                .arg(&target)
                .arg(&payload)
                .arg(&stage)
                .arg(target.join("user drawing.rsk"))
                .arg("linux")
                .output()
                .unwrap();
            assert_eq!(
                result.status.success(),
                !fail,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                std::fs::read_to_string(target.join("user drawing.rsk")).unwrap(),
                "preserve"
            );
            if fail {
                assert_eq!(
                    std::fs::read_to_string(target.join("reshiki-inchi-helper")).unwrap(),
                    "old helper"
                );
                assert_eq!(
                    std::fs::read_to_string(target.join("Licenses/rust/legacy/LICENSE")).unwrap(),
                    "old license"
                );
                assert_eq!(
                    std::fs::read_to_string(target.join("reshiki")).unwrap(),
                    old
                );
            } else {
                assert!(!target.join("reshiki-inchi-helper").exists());
                if missing_licenses {
                    assert_eq!(
                        std::fs::read_to_string(target.join("Licenses/rust/legacy/LICENSE"))
                            .unwrap(),
                        "old license"
                    );
                } else {
                    assert!(!target.join("Licenses/rust").exists());
                    assert_eq!(
                        std::fs::read_to_string(target.join("Licenses/THIRD-PARTY-NOTICES.txt"))
                            .unwrap(),
                        "complete notices"
                    );
                }
                assert_eq!(
                    std::fs::read_to_string(stage.join("previous/reshiki-inchi-helper")).unwrap(),
                    "old helper"
                );
            }
        }
    }
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires a downloaded signed release disk image"]
    fn verifies_and_stages_signed_macos_release_without_touching_installed_app() {
        let disk = std::env::var_os("RESHIKI_TEST_UPDATE_DMG").expect("signed DMG path");
        let root = tempfile::tempdir().unwrap();
        let app = stage(Path::new(&disk), root.path(), "0.6.1").unwrap();
        assert!(app.join("Contents/MacOS/reshiki").is_file());
        assert!(!root.path().join("mount/ReShiki.app").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stages_single_executable_and_legacy_linux_updates_but_rejects_incomplete_packages() {
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        };
        for (self_process, old_helper, main_binary, license_layout) in [
            (true, false, true, "compact"),
            (false, true, true, "legacy"),
            (false, false, true, "compact"),
            (true, false, false, "compact"),
            (true, false, true, "missing"),
            (true, false, true, "empty"),
            (true, false, true, "project-only"),
            (true, false, true, "legacy"),
            (false, true, true, "project-only"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let name = format!("reshiki-0.10.0-linux-{arch}");
            let payload = root.path().join(&name);
            std::fs::create_dir(&payload).unwrap();
            let mut metadata = serde_json::json!({"version": "0.10.0"});
            if self_process {
                metadata["inchi"] = serde_json::json!({"runtime": "self-process"});
            }
            std::fs::write(
                payload.join("build.json"),
                serde_json::to_vec(&metadata).unwrap(),
            )
            .unwrap();
            if main_binary {
                std::fs::write(payload.join("reshiki"), "application").unwrap();
            }
            if old_helper {
                std::fs::write(payload.join("reshiki-inchi-helper"), "legacy helper").unwrap();
            }
            if license_layout != "missing" {
                let licenses = payload.join("Licenses");
                std::fs::create_dir(&licenses).unwrap();
                if license_layout != "empty" {
                    for name in ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "NOTICE"] {
                        std::fs::write(licenses.join(name), "project notice").unwrap();
                    }
                }
                if license_layout == "compact" {
                    std::fs::write(
                        licenses.join("THIRD-PARTY-NOTICES.txt"),
                        "dependency notices",
                    )
                    .unwrap();
                } else if license_layout == "legacy" {
                    std::fs::create_dir(licenses.join("rust")).unwrap();
                    std::fs::create_dir(licenses.join("sources")).unwrap();
                    std::fs::write(licenses.join("rust-dependencies.json"), "{}").unwrap();
                }
            }
            let archive = root.path().join("package.tar.gz");
            assert!(
                Command::new("tar")
                    .arg("-czf")
                    .arg(&archive)
                    .arg("-C")
                    .arg(root.path())
                    .arg(&name)
                    .status()
                    .unwrap()
                    .success()
            );
            let staging = root.path().join("staging");
            std::fs::create_dir(&staging).unwrap();
            assert_eq!(
                stage(&archive, &staging, "0.10.0").is_ok(),
                main_binary
                    && (self_process || old_helper)
                    && (license_layout == "compact"
                        || (!self_process && license_layout == "legacy"))
            );
        }
    }

    #[test]
    fn checksums_require_one_exact_asset_and_cannot_accept_malformed_hashes() {
        let name = "reshiki-1.0.0-macos-arm64.dmg";
        let digest = "ab".repeat(32);
        assert_eq!(
            expected_digest(&format!("{digest}  {name}\n"), name).unwrap(),
            digest
        );
        for manifest in [
            format!("{digest}  other"),
            format!("bad  {name}"),
            format!("{digest}  {name}\n{digest}  {name}"),
        ] {
            assert!(expected_digest(&manifest, name).is_err());
        }
    }
    #[test]
    fn only_supported_stable_assets_can_be_requested() {
        assert_eq!(
            asset_name("1.2.3", "macos", "aarch64").unwrap(),
            "reshiki-1.2.3-macos-arm64.dmg"
        );
        assert_eq!(
            asset_name("1.2.3", "macos", "x86_64").unwrap(),
            "reshiki-1.2.3-macos-x64.dmg"
        );
        assert!(asset_name("1.2.3", "macos", "riscv64").is_err());
        assert!(asset_name("../bad", "linux", "x86_64").is_err());
        assert!(asset_name("1.2.3-beta", "windows", "aarch64").is_err());
        assert_eq!(
            asset_name("1.2.3", "windows", "aarch64").unwrap(),
            "reshiki-1.2.3-windows-arm64-setup.exe"
        );
    }
}
