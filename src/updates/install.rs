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
    /// The [`tree_digest`] of `payload` from when it was verified;
    /// [`handoff`] checks it again.
    digest: String,
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
/// The lowercase hexadecimal SHA-256 of everything staged at `path`: a
/// file's bytes, or for a folder the name, kind and bytes of every entry in
/// it (the target of a symbolic link), in name order.
fn tree_digest(path: &Path) -> Result<String, String> {
    use std::io::BufRead;
    fn add(hash: &mut Sha256, root: &Path, path: &Path) -> std::io::Result<()> {
        let name = path.strip_prefix(root).unwrap_or(path);
        hash.update(name.as_os_str().as_encoded_bytes());
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            hash.update(b"\0d");
            let mut entries = std::fs::read_dir(path)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            entries.iter().try_for_each(|entry| add(hash, root, entry))
        } else if metadata.is_symlink() {
            hash.update(b"\0l");
            hash.update(std::fs::read_link(path)?.as_os_str().as_encoded_bytes());
            hash.update(b"\0");
            Ok(())
        } else {
            hash.update(b"\0f");
            hash.update(metadata.len().to_le_bytes());
            let mut file = std::io::BufReader::new(std::fs::File::open(path)?);
            loop {
                let chunk = file.fill_buf()?;
                if chunk.is_empty() {
                    return Ok(());
                }
                hash.update(chunk);
                let read = chunk.len();
                file.consume(read);
            }
        }
    }
    let mut hash = Sha256::new();
    add(&mut hash, path, path).map_err(|e| e.to_string())?;
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
        let digest = tree_digest(&payload)?;
        Ok(Arc::new(Prepared {
            directory,
            target,
            payload,
            digest,
            handed_off: std::sync::atomic::AtomicBool::new(false),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Checks that every part of the bundle at `app` has a valid signature of
/// ReShiki's publisher.
#[cfg(target_os = "macos")]
fn verify_app(app: &Path) -> Result<String, String> {
    command(Command::new("/usr/bin/codesign").args(["--verify","--deep","--strict","-R","=anchor apple generic and identifier \"dev.reshiki.editor\" and certificate leaf[subject.OU] = \"XXN44W8X56\""]).arg(app))
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
        verify_app(&app)?;
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
    use std::os::windows::process::CommandExt;
    let report = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SIGNERS])
        .env(
            "RESHIKI_UPDATE_CURRENT",
            std::env::current_exe().map_err(|e| e.to_string())?,
        )
        .env("RESHIKI_UPDATE_INSTALLER", asset)
        .creation_flags(crate::workers::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    same_publisher(&String::from_utf8_lossy(&report.stdout))?;
    Ok(asset.into())
}
/// Prints the Authenticode status and signer subject of the running
/// application, then of the downloaded installer: four lines.
#[cfg(target_os = "windows")]
const SIGNERS: &str = "$ErrorActionPreference = 'Stop'
foreach ($path in $env:RESHIKI_UPDATE_CURRENT, $env:RESHIKI_UPDATE_INSTALLER) {
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    [string]$signature.Status
    if ($signature.SignerCertificate) { $signature.SignerCertificate.Subject } else { '' }
}";
/// Checks the report of the `SIGNERS` script. A signed application installs only an
/// installer with a valid signature from the same publisher, so a release
/// whose files and checksums were both replaced is not run. Only a build
/// that has no signature at all has no publisher to compare and relies on
/// the checksum alone: an application whose own signature is broken, not
/// trusted or could not be checked installs nothing.
#[cfg(any(target_os = "windows", test))]
fn same_publisher(report: &str) -> Result<(), String> {
    let lines: Vec<_> = report.lines().map(str::trim).collect();
    match *lines.as_slice() {
        ["NotSigned", "", _, _] => Ok(()),
        ["Valid", publisher, "Valid", signer] if !publisher.is_empty() && signer == publisher => {
            Ok(())
        }
        ["Valid", _, _, _] => Err(
            "The downloaded installer is not signed by the publisher of this ReShiki. Nothing was installed."
                .into(),
        ),
        [_, _, _, _] => Err(
            "The signature of this ReShiki is not valid, so the installer cannot be checked against it. Nothing was installed; download the installer from the releases page."
                .into(),
        ),
        _ => Err("The installer's signature could not be checked. Nothing was installed.".into()),
    }
}
/// Whether a `tar -tv` listing holds only regular files and folders. A
/// symbolic or hard link could point a later entry outside the folder the
/// archive is unpacked into.
#[cfg(any(target_os = "linux", all(test, unix)))]
fn only_files_and_folders(listing: &str) -> bool {
    listing.lines().all(|line| {
        matches!(line.as_bytes().first(), Some(b'-' | b'd')) && !line.contains(" link to ")
    })
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
    }) || !only_files_and_folders(&command(Command::new("tar").arg("-tvzf").arg(asset))?)
    {
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
/// Nothing starts when anything staged no longer has its verified digest.
pub async fn handoff(prepared: Arc<Prepared>, drawings: Vec<PathBuf>) -> Result<(), String> {
    // The update can wait a long time for the user between its checks and
    // this handoff, so everything staged is checked again: the helper
    // installs the whole payload, not just its executable. On macOS the
    // bundle's signature is checked again as well.
    let staged = Arc::clone(&prepared);
    let unchanged = tokio::task::spawn_blocking(move || {
        let unchanged = tree_digest(&staged.payload).is_ok_and(|digest| digest == staged.digest);
        #[cfg(target_os = "macos")]
        let unchanged = unchanged && verify_app(&staged.payload).is_ok();
        unchanged
    })
    .await
    .map_err(|e| e.to_string())?;
    if !unchanged {
        return Err("The downloaded update changed after it was checked. Nothing was installed; check for updates again.".into());
    }
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
mod tests;
