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
    for (fail, missing_licenses, missing_binary) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
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
        if missing_binary {
            std::fs::remove_file(payload.join("reshiki")).unwrap();
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
        let mut script = include_str!("../install.sh").to_owned();
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
            !fail && !missing_binary,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(target.join("user drawing.rsk")).unwrap(),
            "preserve"
        );
        if fail || missing_binary {
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
                    std::fs::read_to_string(target.join("Licenses/rust/legacy/LICENSE")).unwrap(),
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
        }
        // The helper removes its stage once nothing of the app is left in it.
        assert!(!stage.exists());
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
                && (license_layout == "compact" || (!self_process && license_layout == "legacy"))
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

/// S14: an update can wait between its checks and the restart. A staged
/// installer that changed in that time is not started.
#[tokio::test]
async fn a_staged_program_that_changed_is_not_installed() {
    let directory = tempfile::tempdir().unwrap();
    let stage = directory.path().to_owned();
    let program = stage.join("payload");
    std::fs::write(&program, b"verified").unwrap();
    let prepared = Arc::new(Prepared {
        folder: stage.clone(),
        target: stage.join("installed"),
        payload: program.clone(),
        digest: tree_digest(&program).unwrap(),
        handed_off: std::sync::atomic::AtomicBool::new(false),
    });
    std::fs::write(&program, b"replaced").unwrap();
    assert_eq!(
        handoff(Arc::clone(&prepared), vec![]).await.unwrap_err(),
        CHANGED
    );
    // Nothing was started: no installer script, log or acknowledgement.
    assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 1);
    std::fs::remove_file(&program).unwrap();
    assert!(handoff(Arc::clone(&prepared), vec![]).await.is_err());
    assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
}
const CHANGED: &str = "The downloaded update changed after it was checked. Nothing was installed; check for updates again.";

/// S14: the helper installs the whole staged application, so a change to
/// any part of it stops the installation, with the executable untouched.
#[cfg(unix)]
#[tokio::test]
async fn a_staged_application_that_changed_outside_its_executable_is_not_installed() {
    let directory = tempfile::tempdir().unwrap();
    let stage = directory.path().to_owned();
    let app = stage.join("ReShiki.app");
    let executable = app.join("Contents/MacOS/reshiki");
    let plist = app.join("Contents/Info.plist");
    let resource = app.join("Contents/Resources/helper");
    for file in [&executable, &plist, &resource] {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, b"verified").unwrap();
    }
    let link = app.join("Contents/Resources/link");
    std::os::unix::fs::symlink("helper", &link).unwrap();
    let digest = tree_digest(&app).unwrap();
    assert_eq!(tree_digest(&app).unwrap(), digest);
    let prepared = Arc::new(Prepared {
        folder: stage.clone(),
        target: stage.join("installed"),
        payload: app.clone(),
        digest,
        handed_off: std::sync::atomic::AtomicBool::new(false),
    });
    let refused = async || {
        assert_eq!(
            handoff(Arc::clone(&prepared), vec![]).await.unwrap_err(),
            CHANGED
        );
        // Nothing was started: no installer script, log or acknowledgement.
        assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 1);
        assert_eq!(std::fs::read(&executable).unwrap(), b"verified");
    };
    // A changed file, of the same size.
    std::fs::write(&plist, b"replaced").unwrap();
    refused().await;
    std::fs::write(&plist, b"verified").unwrap();
    // An added file.
    let added = app.join("Contents/Frameworks");
    std::fs::write(&added, b"").unwrap();
    refused().await;
    std::fs::remove_file(&added).unwrap();
    // A removed file.
    std::fs::remove_file(&resource).unwrap();
    refused().await;
    std::fs::write(&resource, b"verified").unwrap();
    // A link that points somewhere else.
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("/bin/sh", &link).unwrap();
    refused().await;
    // Bytes that move from one file to the next.
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("helper", &link).unwrap();
    assert_eq!(tree_digest(&app).unwrap(), prepared.digest);
    std::fs::write(&plist, b"verifiedv").unwrap();
    std::fs::write(&resource, b"erified").unwrap();
    refused().await;
}

/// S15: a link in the update archive could point a later entry outside the
/// folder it is unpacked into, so any entry but a file or a folder is unsafe.
/// This reads the listing of the system's own tar: GNU tar or bsdtar.
#[cfg(unix)]
#[test]
fn archive_links_are_unsafe_entries() {
    let folder = tempfile::tempdir().unwrap();
    let package = folder.path().join("package");
    std::fs::create_dir_all(package.join("Licenses")).unwrap();
    std::fs::write(package.join("reshiki"), b"app").unwrap();
    std::fs::write(package.join("Licenses/NOTICE"), b"notice").unwrap();
    let listing = |name: &str| {
        let archive = folder.path().join(name);
        let packed = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(folder.path())
            .arg("package")
            .status()
            .unwrap();
        assert!(packed.success());
        let listed = Command::new("tar")
            .arg("-tvzf")
            .arg(&archive)
            .output()
            .unwrap();
        assert!(listed.status.success());
        String::from_utf8(listed.stdout).unwrap()
    };
    let plain = listing("plain.tar.gz");
    assert_eq!(plain.lines().count(), 4, "{plain}");
    assert!(only_files_and_folders(&plain), "{plain}");
    std::os::unix::fs::symlink("/etc", package.join("escape")).unwrap();
    let symbolic = listing("symbolic.tar.gz");
    assert!(!only_files_and_folders(&symbolic), "{symbolic}");
    std::fs::remove_file(package.join("escape")).unwrap();
    std::fs::hard_link(package.join("reshiki"), package.join("twin")).unwrap();
    let hard = listing("hard.tar.gz");
    assert!(!only_files_and_folders(&hard), "{hard}");
}

/// S5: a signed Windows build installs only an installer signed by the same
/// publisher. The reports are as PowerShell prints them on Windows 11.
#[test]
fn a_signed_app_needs_an_installer_from_its_publisher() {
    const PUBLISHER: &str = "CN=ReShiki Publisher, O=ReShiki, C=US";
    const REFUSED: &str = "The downloaded installer is not signed by the publisher of this ReShiki. Nothing was installed.";
    const UNCHECKED: &str = "The signature of this ReShiki is not valid, so the installer cannot be checked against it. Nothing was installed; download the installer from the releases page.";
    let report = |lines: [&str; 4]| lines.join("\r\n") + "\r\n";
    // Only a build with no signature has no publisher to compare.
    for installer in [["NotSigned", ""], ["Valid", PUBLISHER]] {
        assert_eq!(
            same_publisher(&report(["NotSigned", "", installer[0], installer[1]])),
            Ok(())
        );
    }
    // A signature that is broken, not trusted or could not be checked is
    // not an unsigned build, with or without a signer, whatever the
    // installer's signature is.
    for status in [
        "HashMismatch",
        "NotTrusted",
        "UnknownError",
        "NotSupportedFileFormat",
        "Incompatible",
        "NotSigned",
    ] {
        for publisher in [PUBLISHER, ""] {
            if (status, publisher) == ("NotSigned", "") {
                continue;
            }
            for installer in [["NotSigned", ""], ["Valid", PUBLISHER]] {
                assert_eq!(
                    same_publisher(&report([status, publisher, installer[0], installer[1]])),
                    Err(UNCHECKED.into()),
                    "{status} {publisher:?} {installer:?}"
                );
            }
        }
    }
    assert_eq!(
        same_publisher(&report(["Valid", PUBLISHER, "Valid", PUBLISHER])),
        Ok(())
    );
    for installer in [
        ["NotSigned", ""],
        ["HashMismatch", PUBLISHER],
        ["NotTrusted", PUBLISHER],
        ["Valid", "CN=Someone Else, O=Other, C=US"],
        ["Valid", ""],
    ] {
        assert_eq!(
            same_publisher(&report(["Valid", PUBLISHER, installer[0], installer[1]])),
            Err(REFUSED.into()),
            "{installer:?}"
        );
    }
    assert_eq!(
        same_publisher(&report(["Valid", "", "Valid", ""])),
        Err(REFUSED.into())
    );
    // PowerShell did not run, or printed something else.
    for broken in ["", "Valid\r\n", "Valid\r\na\r\nValid\r\na\r\nextra\r\n"] {
        assert_eq!(
            same_publisher(broken),
            Err("The installer's signature could not be checked. Nothing was installed.".into()),
            "{broken:?}"
        );
    }
}

/// Every path under `root`, relative and in order, as `find` lists them.
#[cfg(unix)]
fn tree(root: &Path) -> Vec<String> {
    fn walk(root: &Path, folder: &Path, paths: &mut Vec<String>) {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            paths.push(path.strip_prefix(root).unwrap().display().to_string());
            if path.symlink_metadata().unwrap().is_dir() {
                walk(root, &path, paths);
            }
        }
    }
    let mut paths = vec![];
    walk(root, root, &mut paths);
    paths.sort();
    paths
}
#[cfg(unix)]
fn program(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A check, a download, an install and the next launch, in a data folder and
/// an applications folder of their own, for the app folder of Linux and the
/// bundle of macOS. The update is installed from its version folder (same
/// volume) and from a copy beside the app (another volume), and once fails
/// after the swap. `--nocapture` prints the folders after each step.
#[cfg(unix)]
#[test]
fn an_update_stays_in_its_version_folder_and_is_gone_after_the_next_launch() {
    use crate::updates::Cache;
    const OLD: &str = "#!/bin/sh\n# old\nexit 0\n";
    const NEW: &str = "#!/bin/sh\n# new\nexit 0\n";
    for platform in ["linux", "macos"] {
        for another_volume in [false, true] {
            for fails in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let (data, apps) = (root.path().join("data"), root.path().join("Applications"));
                let updates = data.join("updates");
                let show = |step: &str| {
                    eprintln!(
                        "\n## {platform}, {}, {}: {step}",
                        if another_volume {
                            "updates folder on another volume"
                        } else {
                            "updates folder on the app's volume"
                        },
                        if fails {
                            "install fails"
                        } else {
                            "install succeeds"
                        },
                    );
                    for (name, folder) in [("data", &data), ("Applications", &apps)] {
                        for path in tree(folder) {
                            eprintln!("{name}/{path}");
                        }
                    }
                };
                // The app, with what earlier versions left behind.
                let (target, executable, app) = if platform == "macos" {
                    ("ReShiki.app", "Contents/MacOS/reshiki", "ReShiki.app")
                } else {
                    ("ReShiki", "reshiki", "extracted/reshiki-9.9.9-linux-arm64")
                };
                let target = apps.join(target);
                program(&target.join(executable), OLD);
                program(
                    &apps.join(".reshiki-update-Ab12Cd/previous.app/reshiki"),
                    OLD,
                );
                std::fs::write(apps.join(".reshiki-update-Ab12Cd/install.log"), "").unwrap();
                std::fs::create_dir(&data).unwrap();
                std::fs::write(data.join("update-preferences.json"), "true").unwrap();
                for name in ["update-cache.json", "update-cache-nightly.json"] {
                    std::fs::write(data.join(name), "{}").unwrap();
                }
                show("before, as earlier versions left it");

                // Launch, then a check.
                clean_in(&updates, Some(&apps));
                let cache = Cache {
                    checked_at: 100,
                    result: Ok(Release {
                        version: "9.9.9".into(),
                    }),
                };
                cache.save(&updates, Channel::Stable).unwrap();
                assert!(Cache::load(&updates, Channel::Stable).is_some());
                assert!(Cache::load(&updates, Channel::Nightly).is_none());
                assert_eq!(
                    tree(&data),
                    ["update-preferences.json", "updates", "updates/stable.json"]
                );
                assert_eq!(tree(&apps).len(), tree(&target).len() + 1);
                show("after launch and a check");

                // A download: the checksums, the archive and the unpacked app.
                let folder = updates.join("9.9.9");
                claim(&folder).unwrap();
                std::fs::write(folder.join("SHA256SUMS"), "sums").unwrap();
                std::fs::write(folder.join("reshiki-9.9.9.archive"), "archive").unwrap();
                let payload = folder.join(app);
                program(&payload.join(executable), NEW);
                let prepared = Prepared {
                    digest: tree_digest(&payload).unwrap(),
                    folder: folder.clone(),
                    target: target.clone(),
                    payload,
                    handed_off: std::sync::atomic::AtomicBool::new(false),
                };
                let mut expected = [
                    "9.9.9",
                    &format!("9.9.9/{}", app.split('/').next().unwrap()),
                    "9.9.9/SHA256SUMS",
                    "9.9.9/pid",
                    "9.9.9/reshiki-9.9.9.archive",
                    "stable.json",
                ]
                .map(str::to_owned);
                expected.sort();
                let mut found = tree(&updates);
                found.retain(|path| path.matches('/').count() < 2);
                assert_eq!(found, expected);
                assert_eq!(tree(&apps).len(), tree(&target).len() + 1);
                show("after the download");

                // Restart: the helper works where a rename reaches the app.
                let (stage, payload) = if another_volume {
                    prepared.beside_app().unwrap()
                } else {
                    assert!(!on_another_volume(&folder, &target));
                    prepared.stage().unwrap()
                };
                assert_eq!(
                    stage,
                    if another_volume {
                        apps.join(".reshiki-update-9.9.9")
                    } else {
                        folder.clone()
                    }
                );
                assert!(payload.starts_with(&stage));
                assert_eq!(tree_digest(&payload).unwrap(), prepared.digest);
                show("at restart, before the helper runs");
                let script = include_str!("../install.sh").replace(
                    "/usr/bin/open",
                    if fails {
                        "/usr/bin/false"
                    } else {
                        "/usr/bin/true"
                    },
                );
                if fails && platform == "linux" {
                    std::fs::remove_file(payload.join(executable)).unwrap();
                }
                std::fs::write(stage.join("install.sh"), script).unwrap();
                let helper = Command::new("/bin/sh")
                    .arg(stage.join("install.sh"))
                    .arg("2147483647")
                    .arg(&target)
                    .arg(&payload)
                    .arg(&stage)
                    .arg("")
                    .arg(platform)
                    .output()
                    .unwrap();
                assert_eq!(
                    helper.status.success(),
                    !fails,
                    "{}",
                    String::from_utf8_lossy(&helper.stderr)
                );
                prepared
                    .handed_off
                    .store(true, std::sync::atomic::Ordering::Release);
                // Installed or rolled back, the app is whole and the helper's
                // stage, with the previous app in it, is gone.
                assert_eq!(
                    std::fs::read_to_string(target.join(executable)).unwrap(),
                    if fails { OLD } else { NEW }
                );
                assert!(!stage.exists());
                assert_eq!(folder.exists(), another_volume);
                show("after the helper");

                // The next launch, when the ReShiki that downloaded is gone.
                if another_volume {
                    std::fs::write(folder.join("pid"), "2147483647\n").unwrap();
                }
                clean_in(&updates, Some(&apps));
                assert_eq!(
                    tree(&data),
                    ["update-preferences.json", "updates", "updates/stable.json"]
                );
                assert_eq!(tree(&apps).len(), tree(&target).len() + 1);
                assert_eq!(tree(&target).last().unwrap(), executable);
                show("after the next launch");
            }
        }
    }
}

/// Startup removes what an interrupted update left, and nothing else: not
/// the check files, an update another running ReShiki is using, what a link
/// points to, or anything of the user's beside the app.
#[cfg(unix)]
#[test]
fn startup_removes_leftovers_but_not_an_update_in_use() {
    let root = tempfile::tempdir().unwrap();
    let (data, apps) = (root.path().join("data"), root.path().join("Applications"));
    let updates = data.join("updates");
    let mut other = Command::new("sleep").arg("60").spawn().unwrap();
    let file = |path: PathBuf, body: &str| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    };
    for name in ["stable.json", "nightly.json", "half.tmp"] {
        file(updates.join(name), "{}");
    }
    // A finished update, a half download and one in use.
    file(updates.join("1.0.0/pid"), "2147483647\n2147483646\n");
    file(updates.join("1.0.0/previous.app/reshiki"), "old");
    file(updates.join("1.1.0/reshiki-1.1.0.archive"), "half");
    file(
        updates.join("1.2.0/pid"),
        &format!("2147483647\n{}\n", other.id()),
    );
    file(updates.join("1.2.0/SHA256SUMS"), "sums");
    file(root.path().join("elsewhere/kept"), "kept");
    std::os::unix::fs::symlink(root.path().join("elsewhere"), updates.join("link")).unwrap();
    for name in ["update-cache.json", "update-cache-nightly.json"] {
        file(data.join(name), "{}");
    }
    file(data.join("update-channel.json"), "\"stable\"");
    file(data.join("recovery/1-draft.rsk"), "draft");
    file(apps.join("ReShiki/reshiki"), "app");
    file(apps.join(".reshiki-update-Ab12Cd/previous/reshiki"), "old");
    file(
        apps.join(".reshiki-update-1.2.0/pid"),
        &format!("{}\n", other.id()),
    );
    file(
        apps.join(".reshiki-updates/mine"),
        "not an update folder's name",
    );
    file(apps.join("Other.app/reshiki-update"), "other");

    clean_in(&updates, Some(&apps));
    let kept_data = [
        "recovery",
        "recovery/1-draft.rsk",
        "update-channel.json",
        "updates",
        "updates/nightly.json",
        "updates/stable.json",
    ];
    assert_eq!(
        tree(&data),
        [
            "recovery",
            "recovery/1-draft.rsk",
            "update-channel.json",
            "updates",
            "updates/1.2.0",
            "updates/1.2.0/SHA256SUMS",
            "updates/1.2.0/pid",
            "updates/nightly.json",
            "updates/stable.json",
        ]
    );
    let kept_apps = [
        ".reshiki-updates",
        ".reshiki-updates/mine",
        "Other.app",
        "Other.app/reshiki-update",
        "ReShiki",
        "ReShiki/reshiki",
    ];
    assert_eq!(
        tree(&apps),
        [
            &[".reshiki-update-1.2.0", ".reshiki-update-1.2.0/pid"],
            &kept_apps[..]
        ]
        .concat()
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("elsewhere/kept")).unwrap(),
        "kept"
    );
    // Another ReShiki cannot take over the update in use either.
    assert!(
        claim(&updates.join("1.2.0"))
            .unwrap_err()
            .contains("Another ReShiki")
    );
    assert!(updates.join("1.2.0/SHA256SUMS").exists());

    // Once that ReShiki is gone, its update is a leftover like the others.
    other.kill().unwrap();
    other.wait().unwrap();
    claim(&updates.join("1.2.0")).unwrap();
    assert_eq!(tree(&updates.join("1.2.0")), ["pid"]);
    // This process's own update is in use; it is a leftover to the next
    // launch, which is another process.
    clean_in(&updates, None);
    assert_eq!(tree(&updates.join("1.2.0")), ["pid"]);
    std::fs::write(updates.join("1.2.0/pid"), "2147483647\n").unwrap();
    clean_in(&updates, Some(&apps));
    assert_eq!(tree(&data), kept_data);
    assert_eq!(tree(&apps), kept_apps);
    // Nothing to clean, no app folder and no updates folder are all fine.
    clean_in(&updates, None);
    clean_in(
        &root.path().join("missing/updates"),
        Some(&root.path().join("missing")),
    );
    assert_eq!(tree(&data), kept_data);
}

/// An update that is given up before it installs takes its folder with it;
/// one the helper took over is the helper's to remove.
#[test]
fn an_update_given_up_before_it_installs_removes_its_folder() {
    let root = tempfile::tempdir().unwrap();
    for handed_off in [false, true] {
        let folder = root.path().join("updates/9.9.9");
        claim(&folder).unwrap();
        assert_eq!(
            std::fs::read_to_string(folder.join("pid")).unwrap(),
            format!("{}\n", std::process::id())
        );
        std::fs::write(folder.join("payload"), b"verified").unwrap();
        drop(Prepared {
            target: root.path().join("installed"),
            payload: folder.join("payload"),
            digest: String::new(),
            handed_off: std::sync::atomic::AtomicBool::new(handed_off),
            folder: folder.clone(),
        });
        assert_eq!(folder.exists(), handed_off);
        assert!(root.path().join("updates").exists());
    }
}
