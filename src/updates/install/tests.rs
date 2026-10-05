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
