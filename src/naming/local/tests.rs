use super::*;
use serde_json::json;

fn reply(smiles: &str) -> serde_json::Value {
    json!({"operation":"parse","protocol":worker::PROTOCOL,"backend":worker::BACKEND,"port_version":worker::PORT_VERSION,"upstream_version":worker::UPSTREAM_VERSION,"upstream_commit":worker::UPSTREAM_COMMIT,"resource_fingerprint":worker::RESOURCE_FINGERPRINT,"options":worker::OPTIONS,"name":"example","status":"SUCCESS","message":"","warnings":[],"cxsmiles":smiles})
}
fn parse(value: serde_json::Value) -> Result<Record, String> {
    parse_reply(&serde_json::to_vec(&value).unwrap(), "example")
}

#[test]
fn strict_adapter_rejects_semantic_loss_and_inconsistent_provenance() {
    assert_eq!(parse(reply("C(C)O")).unwrap().canonical_smiles, "CCO");
    for smiles in [
        "C[C@H](O)C(=O)O |r|",
        "C[C@H](O)C(=O)O |o1:1|",
        "*CC* |$star_e;;;star_e$,Sg:n:1,2::ht|",
        "CCO |unknown:1|",
        "[Na+].[Cl-]",
        "[CH3]",
        "*C",
        "C~C",
        "C->N",
    ] {
        assert!(
            parse(reply(smiles)).is_err(),
            "Simplified unsupported {smiles}"
        );
    }
    assert!(parse(reply(&"C".repeat(513))).is_err());
    for (key, value) in [
        ("protocol", json!(3)),
        ("backend", json!("external-engine")),
        ("port_version", json!("3.0.0")),
        ("upstream_version", json!("3.0.0")),
        ("upstream_commit", json!("different-source")),
        ("resource_fingerprint", json!("different-resources")),
        ("options", json!("plain-smiles")),
        ("name", json!("older-input")),
        ("status", json!("UNKNOWN")),
        ("cxsmiles", json!(null)),
        (
            "warnings",
            json!([{"kind":"APPEARS_AMBIGUOUS","message":"Missing locant"}]),
        ),
        (
            "warnings",
            json!([{"kind":"STEREOCHEMISTRY_IGNORED","message":"Optical rotation cannot assign R/S"}]),
        ),
    ] {
        let mut result = reply("CCO");
        result[key] = value;
        assert!(parse(result).is_err(), "Accepted inconsistent {key}");
    }
    assert!(parse_reply(b"invalid", "example").is_err());
    assert!(parse_reply(&vec![b' '; OUTPUT_LIMIT + 1], "example").is_err());
}

/// Independently written graphs encode substituent positions, ring topology
/// and known CIP priorities. They are not copied from OPSIN's generated SMILES.
#[tokio::test]
async fn pinned_local_engine_matches_independent_connected_graphs_and_stereo() -> Result<(), String>
{
    for (name, expected) in [
        ("ethanol", "CCO"),
        (
            "ethyl 3-(4-hydroxyphenyl)-2-methylpropanoate",
            "CCOC(=O)C(C)Cc1ccc(O)cc1",
        ),
        ("bicyclo[2.2.1]heptane", "C1CC2CCC1C2"),
        ("spiro[4.5]decane", "C1CCC2(C1)CCCCC2"),
        ("1,3-thiazole", "c1cscn1"),
        ("acetic acid", "CC(=O)O"),
        ("phenol", "Oc1ccccc1"),
        ("(R)-lactic acid", "C[C@@H](O)C(=O)O"),
        ("(S)-lactic acid", "C[C@H](O)C(=O)O"),
        ("(E)-but-2-ene", "C/C=C/C"),
        ("(Z)-but-2-ene", "C/C=C\\C"),
        ("lactic acid", "CC(O)C(=O)O"),
        ("ethan-1-ol-1-13C", "C[13CH2]O"),
        ("ethanoate", "CC(=O)[O-]"),
    ] {
        let record = resolve_name(name, Cancel::default())
            .await
            .map_err(|e| format!("{name}: {e}"))?;
        super::super::verify_identity(expected, &record.smiles)
            .map_err(|e| format!("{name}: {e}"))?;
        assert_eq!(record.provenance, Provenance::OpsinRust);
        assert!(record.warnings.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn actual_parser_rejects_ambiguous_stereo_polymers_salts_and_unknown_names() {
    for name in [
        "bromobutane",
        "(+)-butan-2-ol",
        "(-)-lactic acid",
        "(+)-ethyl lactate",
        "(R)-(+)-lactic acid",
        "(rac)-lactic acid",
        "rel-(1R,2R)-cyclohexane-1,2-diol",
        "poly(ethylene)",
        "sodium chloride",
        "acetic",
        "phenyl",
        "(R)-ethanol",
        "reshiki-no-such-chemical-name-50",
        "aspirin",
    ] {
        let error = resolve_name(name, Cancel::default()).await.expect_err(name);
        assert!(!error.is_empty(), "No explanation for {name}");
    }
    for name in ["", "\nethanol", &"x".repeat(2049)] {
        assert!(resolve_name(name, Cancel::default()).await.is_err());
    }
}

#[test]
fn parser_cannot_accept_a_rule_generation_response() {
    let response = serde_json::json!({"operation":"generate", "protocol":worker::PROTOCOL,
        "profile":super::super::rules::PROFILE,"input":"CCO","result":{"Ok":"ethan-1-ol"}});
    assert!(
        parse_reply(&serde_json::to_vec(&response).unwrap(), "example")
            .unwrap_err()
            .contains("different operation")
    );
}

#[tokio::test]
async fn missing_native_worker_fails_without_a_foreign_runtime_or_service() {
    let _parser_slot = SLOTS.acquire().await.unwrap();
    let path = std::env::temp_dir().join("reshiki-test-no-native-executable-50");
    let error = run_with_executable("ethanol", &Cancel::default(), &path).unwrap_err();
    assert!(error.contains("Could not start bounded local naming worker"));
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_kills_and_reaps_child_and_removes_private_payload() {
    // Fault fixtures obey the same one-parser budget as actual native parser requests.
    let _parser_slot = SLOTS.acquire().await.unwrap();
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("helper");
    let pid = directory.path().join("pid");
    let cwd = directory.path().join("cwd");
    let ready = directory.path().join("ready");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\npwd > '{}'\nprintf ready > '{}'\nexec /bin/sleep 30\n",
            pid.display(),
            cwd.display(),
            ready.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cancel = Cancel::default();
    let _stop = StopOnDrop(cancel.clone());
    let worker_cancel = cancel.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        run_with_executable("ethanol", &worker_cancel, &helper)
    });
    let startup_started = Instant::now();
    let deadline = startup_started + Duration::from_secs(10);
    while !ready.is_file() {
        assert!(
            Instant::now() < deadline,
            "Fixture did not reach its startup marker"
        );
        tokio::select! {
            result = &mut worker => panic!("Fixture exited before readiness: {result:?}"),
            () = tokio::time::sleep(POLL) => {},
        }
    }
    let startup_elapsed = startup_started.elapsed();
    let child_pid = std::fs::read_to_string(&pid).unwrap();
    let work = std::fs::read_to_string(&cwd).unwrap();
    let cancellation_started = Instant::now();
    cancel.stop();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .contains("cancelled")
    );
    let cancellation_elapsed = cancellation_started.elapsed();
    eprintln!(
        "cancellation phases: startup={startup_elapsed:?}; cancellation_before_cleanup_check={cancellation_elapsed:?}"
    );
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", &child_pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    assert!(!Path::new(work.trim()).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn exited_main_child_cannot_leave_a_descendant_holding_response_pipes() {
    let _parser_slot = SLOTS.acquire().await.unwrap();
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("helper");
    let mut response = reply("CCO");
    response["name"] = json!("ethanol");
    let output = serde_json::to_string(&response).unwrap();
    std::fs::write(
        &helper,
        format!("#!/bin/sh\ncat >/dev/null\n/bin/sleep 30 &\nprintf '%s' '{output}'\nexit 0\n"),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let started = Instant::now();
    let result = run_with_executable("ethanol", &Cancel::default(), &helper).unwrap();
    assert_eq!(result.canonical_smiles, "CCO");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "An exited main child left its descendants/pipes alive"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn oversized_actual_child_output_is_bounded_and_worker_is_stopped() {
    let _parser_slot = SLOTS.acquire().await.unwrap();
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("helper");
    std::fs::write(
        &helper,
        "#!/bin/sh\ncat >/dev/null\nhead -c 65537 /dev/zero\nexec /bin/sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let started = Instant::now();
    let error = run_with_executable("ethanol", &Cancel::default(), &helper).unwrap_err();
    assert!(error.contains("output exceeded"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_queued_and_running_futures_recovers_the_worker_permit() {
    // Fault fixtures obey the same one-parser budget as actual native parser requests.
    let _parser_slot = SLOTS.acquire().await.unwrap();
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("helper");
    let pid = directory.path().join("pid");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec /bin/sleep 30\n",
            pid.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let slots: &'static tokio::sync::Semaphore =
        Box::leak(Box::new(tokio::sync::Semaphore::new(1)));
    let held = slots.acquire().await.unwrap();
    let queued_helper = helper.clone();
    let queued = tokio::spawn(resolve_with(
        "ethanol",
        Cancel::default(),
        slots,
        move |name, cancel| run_with_executable(name, cancel, &queued_helper),
    ));
    tokio::time::sleep(POLL).await;
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());
    assert!(
        !pid.exists(),
        "A cancelled queued operation must not launch a child"
    );
    drop(held);
    let running = tokio::spawn(resolve_with(
        "ethanol",
        Cancel::default(),
        slots,
        move |name, cancel| run_with_executable(name, cancel, &helper),
    ));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !pid.exists() {
        assert!(
            Instant::now() < deadline,
            "Fixture did not reach its startup marker"
        );
        tokio::time::sleep(POLL).await;
    }
    let child_pid = std::fs::read_to_string(&pid).unwrap();
    running.abort();
    assert!(running.await.unwrap_err().is_cancelled());
    let permit = tokio::time::timeout(Duration::from_secs(3), slots.acquire())
        .await
        .unwrap()
        .unwrap();
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", &child_pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    drop(permit);
}

#[cfg(unix)]
#[tokio::test]
async fn an_escaped_pipe_holder_cannot_extend_the_completion_deadline() {
    let _parser_slot = SLOTS.acquire().await.unwrap();
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("helper");
    let detached_pid = directory.path().join("detached");
    let ready = directory.path().join("ready");
    let release = directory.path().join("release");
    // This deliberately violates the trusted-direct-executable contract. It
    // verifies bounded IO, without claiming process-group containment of setsid.
    #[cfg(target_os = "macos")]
    let detach = "/usr/bin/perl -MPOSIX -e 'POSIX::setsid() >= 0 or die; open(my $f, \">\", $ARGV[0]) or die; print $f \"$$\"; close($f); sleep 30;'";
    #[cfg(not(target_os = "macos"))]
    let detach =
        "/usr/bin/setsid /bin/sh -c 'printf %s \"$$\" > \"$1\"; exec /bin/sleep 30' ignored";
    // Perl/util-linux are adversarial TEST fixtures only, never app dependencies.
    std::fs::write(&helper, format!("#!/bin/sh\ncat >/dev/null\n{detach} '{}' &\nwhile [ ! -s '{}' ]; do /bin/sleep 0.01; done\nprintf '{{}}'\nprintf ready > '{}'\nwhile [ ! -f '{}' ]; do /bin/sleep 0.01; done\nexit 0\n", detached_pid.display(), detached_pid.display(), ready.display(), release.display())).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Cleanup belongs to the adversarial fixture: a changed process group is
    // expressly outside the production Unix kill-group contract.
    struct Detached(std::path::PathBuf);
    impl Drop for Detached {
        fn drop(&mut self) {
            if let Ok(pid) = std::fs::read_to_string(&self.0)
                && pid.trim().parse::<u32>().is_ok_and(|pid| pid > 1)
            {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", pid.trim()])
                    .status();
            }
        }
    }
    let detached = Detached(detached_pid);
    let cancel = Cancel::default();
    let _stop = StopOnDrop(cancel.clone());
    let startup_started = Instant::now();
    let task = tokio::task::spawn_blocking(move || {
        let result = run_with_executable("ethanol", &cancel, &helper);
        (result, Instant::now())
    });
    let fixture_ready = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if ready.is_file() {
                return true;
            }
            if task.is_finished() {
                return false;
            }
            tokio::time::sleep(POLL).await;
        }
    })
    .await;
    assert_eq!(
        fixture_ready,
        Ok(true),
        "The pipe-holder fixture never became ready"
    );
    let startup_elapsed = startup_started.elapsed();
    // The production post-exit pipe guard starts after the leader exits. Gate
    // that exit so interpreter/spawn/observer startup is measured separately.
    let started = Instant::now();
    std::fs::write(&release, b"exit").unwrap();
    let (result, completed) = task.await.unwrap();
    let completion_elapsed = completed.duration_since(started);
    let cleanup_started = Instant::now();
    drop(detached);
    let cleanup_elapsed = cleanup_started.elapsed();
    eprintln!(
        "escaped-pipe phases: startup={startup_elapsed:?}; completion_before_cleanup={completion_elapsed:?}; cleanup={cleanup_elapsed:?}"
    );
    let error = result.unwrap_err();
    assert!(error.contains("response pipes"), "{error}");
    assert!(
        completion_elapsed < Duration::from_secs(3),
        "{completion_elapsed:?}"
    );
}
#[test]
fn measured_resource_breaches_cannot_enter_the_exit_race_grace_path() {
    // The production check classifies the error before inspecting exit status.
    // A measured RSS breach is Error::other, never one of the two missing-PID
    // errors that may race an exit and receive four retries.
    assert!(!transient_measurement_error(&std::io::Error::other(
        "Local naming worker exceeded its resident-memory limit"
    )));
    for code in [1, 5, 12, 13, 22] {
        assert!(!transient_measurement_error(
            &std::io::Error::from_raw_os_error(code)
        ));
    }
    for code in [2, 3] {
        assert!(transient_measurement_error(
            &std::io::Error::from_raw_os_error(code)
        ));
    }
}

#[test]
fn retained_lactic_name_is_exact_and_requires_its_specific_absolute_graph() {
    for name in ["(+)-lactic acid", "(+)-LACTIC ACID"] {
        assert_eq!(retained_parser_name(name), PLUS_LACTIC_PARSER_NAME);
        for smiles in [
            "C[C@H](O)C(=O)O",
            "C[C@@H](O)C(=O)O",
            "CC(O)C(=O)O",
            "C[C@H](O)C(=O)[O-]",
            "[13CH3][C@H](O)C(=O)O",
            "CC(O)CC(=O)O",
        ] {
            let mut value = reply(smiles);
            value["name"] = json!(PLUS_LACTIC_PARSER_NAME);
            let record = parse_reply(
                &serde_json::to_vec(&value).unwrap(),
                PLUS_LACTIC_PARSER_NAME,
            )
            .unwrap();
            let certified = certify_retained_name(name, record);
            if smiles == PLUS_LACTIC_GRAPH {
                let record = certified.unwrap();
                assert_eq!(record.title, name);
                assert_eq!(record.synonyms, [PLUS_LACTIC_PARSER_NAME]);
                assert_eq!(
                    record.provenance,
                    Provenance::OpsinRustRetainedName {
                        source: "ChEBI:422"
                    }
                );
            } else {
                assert!(
                    certified.is_err(),
                    "retained name lost semantics for {smiles}"
                );
            }
        }
    }
    for name in [
        "(+)-butan-2-ol",
        "(-)-lactic acid",
        "(+)-ethyl lactate",
        "(R)-(+)-lactic acid",
        "(+)-lactic acid sodium salt",
        "(+)-lactic acid extra",
        "(+)-lacticacid",
        "(+)-2-hydroxypropanoic acid",
        "lactic acid",
    ] {
        assert_eq!(retained_parser_name(name), name);
    }
}

#[tokio::test]
async fn retained_plus_lactic_name_uses_local_parser_and_exact_s_reverse() -> Result<(), String> {
    for name in ["(+)-lactic acid", "  (+)-LACTIC ACID  "] {
        let record = resolve_name(name, Cancel::default()).await?;
        super::super::verify_identity(PLUS_LACTIC_GRAPH, &record.smiles)?;
        assert!(super::super::verify_identity("C[C@@H](O)C(=O)O", &record.smiles).is_err());
        assert!(super::super::verify_identity("CC(O)C(=O)O", &record.smiles).is_err());
        assert_eq!(record.title, name.trim());
        assert!(
            record
                .provenance
                .label()
                .contains("ChEBI:422 retained name")
        );
        assert!(record.warnings.is_empty());
        let generated = super::super::generate_name(
            &super::super::Identity {
                smiles: record.smiles,
                warnings: vec![],
            },
            Cancel::default(),
        )
        .await?;
        assert_eq!(
            generated.systematic_name.as_deref(),
            Some(PLUS_LACTIC_PARSER_NAME)
        );
        assert_eq!(generated.provenance, Provenance::LocalRules);
    }
    Ok(())
}
