use super::*;
fn release(version: &str) -> Release {
    Release {
        version: version.into(),
    }
}
const NIGHTLY: &str = "0.9.1-nightly.20260929.36501221724.1";

#[tokio::test]
#[ignore = "Live official GitHub release discovery; no downloads or preference writes"]
async fn discovers_published_stable_and_nightly_releases() {
    for channel in Channel::ALL {
        let release = fetch(channel).await.unwrap();
        assert_eq!(release.channel(), Some(channel));
        eprintln!("{channel}: {} ({})", release.version, release.url());
    }
}

#[test]
fn compares_versions_within_channels_and_allows_explicit_channel_switches() {
    assert!(release("0.10.0").available_for("0.9.9"));
    assert!(release("0.10.0").available_for("0.10.0-rc.1"));
    assert!(!release("0.10.0").available_for("0.10.0"));
    assert!(!release("0.10.0").available_for("1.0.0"));
    assert!(!release("1.0.0-beta.1").available_for("0.3.0"));
    assert!(release(NIGHTLY).available_for("0.9.1"));
    assert!(release("0.9.0").available_for(NIGHTLY));
    assert!(!release(NIGHTLY).available_for(NIGHTLY));
    assert!(release(NIGHTLY).available_for("0.9.1-nightly.20260928.9999999999.9"));
    assert!(
        release("0.9.1-nightly.20260929.36501221724.10")
            .available_for("0.9.1-nightly.20260929.36501221724.9")
    );
    assert!(!release(NIGHTLY).available_for("0.9.1-nightly.20260929.36501221724.2"));
    assert!(!release(NIGHTLY).available_for("invalid"));
}

#[test]
fn stable_metadata_stays_strict_and_urls_are_constructed_locally() {
    let parsed = parse_release(br#"{"tag_name":"v0.4.0","draft":false,"prerelease":false,"html_url":"https://untrusted.example"}"#).unwrap();
    assert_eq!(parsed.url(), format!("{RELEASES}/tag/v0.4.0"));
    for input in [
        br#"{"tag_name":"v0.4.0","draft":true,"prerelease":false}"#.as_slice(),
        br#"{"tag_name":"v0.4.0-beta.1","draft":false,"prerelease":false}"#,
        br#"{"tag_name":"v0.4.0","draft":false,"prerelease":true}"#,
        br#"{"tag_name":"../../evil","draft":false,"prerelease":false}"#,
        b"not json",
    ] {
        assert!(parse_release(input).is_err());
    }
    assert_eq!(release("../bad").url(), RELEASES);
}

#[test]
fn nightly_discovery_rejects_other_prereleases_and_sorts_numeric_identifiers() {
    let data = serde_json::json!([
        {"tag_name":"v99.0.0","draft":false,"prerelease":false},
        {"tag_name":"v99.0.0-rc.1","draft":false,"prerelease":true},
        {"tag_name":"nightly-0.9.1-nightly.20260929.20.9","draft":false,"prerelease":true},
        {"tag_name":"nightly-0.9.1-nightly.20260929.20.10","draft":false,"prerelease":true},
        {"tag_name":"nightly-0.9.1-nightly.20260930.20.1","draft":true,"prerelease":true},
        {"tag_name":"nightly-0.9.1-nightly.20260930.21.1","draft":false,"prerelease":false},
        {"tag_name":"nightly-../../evil","draft":false,"prerelease":true},
        {"tag_name":"nightly-0.9.1-nightly.20260930.22.1+other","draft":false,"prerelease":true},
        {"tag_name":"nightly-0.9.1-nightly.20260930.22","draft":false,"prerelease":true}
    ]);
    let result = parse_nightlies(&serde_json::to_vec(&data).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(result.version, "0.9.1-nightly.20260929.20.10");
    assert_eq!(
        result.url(),
        format!("{RELEASES}/tag/nightly-{}", result.version)
    );
    assert_eq!(parse_nightlies(b"[]").unwrap(), None);
    assert!(parse_nightlies(b"not json").is_err());
}

#[test]
fn preferences_preserve_legacy_opt_out_and_round_trip_channel_selection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    assert_eq!(Preferences::load(path), Preferences::default());
    std::fs::write(path.join("update-preferences.json"), b"false").unwrap();
    assert_eq!(
        Preferences::load(path),
        Preferences {
            automatic: false,
            channel: Channel::Stable
        }
    );
    let preferences = Preferences {
        automatic: false,
        channel: Channel::Nightly,
    };
    preferences.save(path).unwrap();
    assert_eq!(Preferences::load(path), preferences);
    // Old stable builds still read the opt-out as a JSON boolean.
    assert!(
        !serde_json::from_slice::<bool>(
            &std::fs::read(path.join("update-preferences.json")).unwrap()
        )
        .unwrap()
    );
    std::fs::write(path.join("update-channel.json"), b"invalid").unwrap();
    assert_eq!(
        Preferences::load(path),
        Preferences {
            automatic: false,
            channel: Channel::Stable
        }
    );
}

#[test]
fn portable_downloads_cover_the_six_published_nightly_packages() {
    for (os, arch, suffix) in [
        ("macos", "aarch64", "macos-arm64.zip"),
        ("macos", "x86_64", "macos-x64.zip"),
        ("windows", "aarch64", "windows-arm64.zip"),
        ("windows", "x86_64", "windows-x64.zip"),
        ("linux", "aarch64", "linux-arm64.tar.gz"),
        ("linux", "x86_64", "linux-x64.tar.gz"),
    ] {
        assert_eq!(
            release(NIGHTLY).portable_url(os, arch).unwrap(),
            format!("{RELEASES}/download/nightly-{NIGHTLY}/reshiki-{NIGHTLY}-{suffix}")
        );
    }
    assert!(release(NIGHTLY).portable_url("macos", "riscv64").is_err());
    assert!(release(NIGHTLY).portable_url("unknown", "x86_64").is_err());
    assert!(release("../bad").portable_url("linux", "x86_64").is_err());
    assert!(release("0.9.1").portable_url("linux", "x86_64").is_err());
}

#[test]
fn nightly_downloads_prefer_published_installers_and_fall_back_to_legacy_archives() {
    for (os, arch, platform, installer, portable) in [
        ("macos", "aarch64", "macos-arm64", ".dmg", ".zip"),
        ("macos", "x86_64", "macos-x64", ".dmg", ".zip"),
        ("windows", "aarch64", "windows-arm64", "-setup.exe", ".zip"),
        ("windows", "x86_64", "windows-x64", "-setup.exe", ".zip"),
        ("linux", "aarch64", "linux-arm64", ".tar.gz", ".tar.gz"),
        ("linux", "x86_64", "linux-x64", ".tar.gz", ".tar.gz"),
    ] {
        let stem = format!("reshiki-{NIGHTLY}-{platform}");
        let mut response = serde_json::json!({
            "tag_name": format!("nightly-{NIGHTLY}"), "draft": false, "prerelease": true,
            "assets": [{"name": format!("{stem}{portable}"), "browser_download_url": "https://untrusted.example/archive"}]
        });
        let bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            nightly_download_url(&release(NIGHTLY), &bytes, os, arch).unwrap(),
            release(NIGHTLY).portable_url(os, arch).unwrap()
        );
        response["assets"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name": format!("{stem}{installer}")}));
        let bytes = serde_json::to_vec(&response).unwrap();
        assert!(
            nightly_download_url(&release(NIGHTLY), &bytes, os, arch)
                .unwrap()
                .ends_with(installer)
        );
        response["draft"] = true.into();
        assert!(
            nightly_download_url(
                &release(NIGHTLY),
                &serde_json::to_vec(&response).unwrap(),
                os,
                arch
            )
            .is_err()
        );
        response["draft"] = false.into();
        response["assets"] = serde_json::json!([]);
        assert!(
            nightly_download_url(
                &release(NIGHTLY),
                &serde_json::to_vec(&response).unwrap(),
                os,
                arch
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn cached_nightly_downloads_open_trusted_archives_when_asset_lookup_is_unavailable() {
    for (os, arch) in [
        ("macos", "aarch64"),
        ("macos", "x86_64"),
        ("windows", "aarch64"),
        ("windows", "x86_64"),
        ("linux", "aarch64"),
        ("linux", "x86_64"),
    ] {
        let opened = std::cell::RefCell::new(None);
        open_nightly_download_with(
            release(NIGHTLY),
            os,
            arch,
            async |url| {
                assert_eq!(url, format!("{API}/tags/nightly-{NIGHTLY}"));
                Err(FetchError::Unavailable("Offline".into()))
            },
            async |url| {
                opened.replace(Some(url));
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(
            opened.into_inner(),
            Some(release(NIGHTLY).portable_url(os, arch).unwrap())
        );
    }
}

#[tokio::test]
async fn nightly_downloads_only_fall_back_for_transient_http_responses() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    for status in [302, 400, 401, 403, 404, 408, 410, 429, 500, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_url = format!("http://{}/release", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(10), async move {
                    let (stream, _) = listener.accept().await.unwrap();
                    let mut stream = tokio::io::BufReader::new(stream);
                    loop {
                        let mut line = String::new();
                        assert_ne!(stream.read_line(&mut line).await.unwrap(), 0);
                        if line == "\r\n" {
                            break;
                        }
                    }
                    stream
                        .get_mut()
                        .write_all(
                            format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                                .as_bytes(),
                        )
                        .await
                        .unwrap();
                })
                .await
                .expect("Local release response must finish promptly");
        });
        let opened = std::cell::RefCell::new(None);
        let result = open_nightly_download_with(
            release(NIGHTLY),
            "windows",
            "x86_64",
            async |url| {
                assert_eq!(url, format!("{API}/tags/nightly-{NIGHTLY}"));
                fetch_bytes(&client, &local_url).await
            },
            async |url| {
                opened.replace(Some(url));
                Ok(())
            },
        )
        .await;
        server.await.unwrap();
        if matches!(status, 403 | 408 | 429 | 500 | 503) {
            result.unwrap();
            assert_eq!(
                opened.into_inner(),
                Some(release(NIGHTLY).portable_url("windows", "x86_64").unwrap())
            );
        } else {
            let error = result.unwrap_err();
            if matches!(status, 404 | 410) {
                assert!(error.contains("release is no longer available"));
            } else {
                assert!(error.contains(&status.to_string()));
            }
            assert_eq!(opened.into_inner(), None, "HTTP {status} opened a browser");
        }
    }
}

#[tokio::test]
async fn nightly_download_fallback_cannot_bypass_version_or_metadata_validation() {
    for (version, os, arch) in [
        ("0.9.1", "macos", "aarch64"),
        ("../bad", "windows", "x86_64"),
        ("0.9.1-nightly.20260929.1.1/evil", "linux", "x86_64"),
        (NIGHTLY, "unknown", "x86_64"),
        (NIGHTLY, "macos", "riscv64"),
    ] {
        assert!(
            open_nightly_download_with(
                release(version),
                os,
                arch,
                async |_| panic!("Invalid target must be rejected before lookup"),
                async |_| panic!("Invalid target must never open a browser"),
            )
            .await
            .is_err()
        );
    }
    let valid = serde_json::json!({
        "tag_name": format!("nightly-{NIGHTLY}"), "draft": false, "prerelease": true,
        "assets": [{"name": format!("reshiki-{NIGHTLY}-macos-arm64.dmg")}]
    });
    let mut invalid = vec![b"not JSON".to_vec()];
    for (field, value) in [
        (
            "tag_name",
            serde_json::json!("nightly-0.9.1-nightly.20260928.1.1"),
        ),
        ("draft", serde_json::json!(true)),
        ("prerelease", serde_json::json!(false)),
        ("assets", serde_json::json!([])),
    ] {
        let mut response = valid.clone();
        response[field] = value;
        invalid.push(serde_json::to_vec(&response).unwrap());
    }
    for result in invalid
        .into_iter()
        .map(Ok)
        .chain([Err(FetchError::TooLarge)])
    {
        assert!(
            open_nightly_download_with(
                release(NIGHTLY),
                "macos",
                "aarch64",
                async |_| result,
                async |_| panic!("Invalid metadata must not trigger an archive fallback"),
            )
            .await
            .is_err()
        );
    }
    let opened = std::cell::RefCell::new(None);
    open_nightly_download_with(
        release(NIGHTLY),
        "macos",
        "aarch64",
        async |_| Ok(serde_json::to_vec(&valid).unwrap()),
        async |url| {
            opened.replace(Some(url));
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(
        opened.into_inner(),
        Some(format!(
            "{RELEASES}/download/nightly-{NIGHTLY}/reshiki-{NIGHTLY}-macos-arm64.dmg"
        ))
    );
}

#[test]
fn caches_are_channel_specific_and_manual_checks_bypass_them() {
    let mut cache = Cache {
        checked_at: 100,
        result: Ok(release("0.4.0")),
    };
    assert_ne!(Channel::Stable.cache_name(), Channel::Nightly.cache_name());
    assert!(cache.usable(Channel::Stable, false, 101));
    assert!(!cache.usable(Channel::Nightly, false, 101));
    assert!(!cache.usable(Channel::Stable, true, 101));
    assert!(!cache.fresh(99));
    assert!(!cache.fresh(86500));
    cache.result = Err("Offline".into());
    assert!(cache.fresh(3699));
    assert!(!cache.fresh(3700));
}
