use super::*;
use crate::app::office::Host as OfficeHost;
use std::{
    io::{Read, Write},
    sync::mpsc as sync,
};

fn isolated() -> native::Endpoint {
    native::Endpoint::current()
        .unwrap()
        .private(native::random_token().unwrap())
        .unwrap()
}
fn test_core(endpoint: &native::Endpoint) -> (Arc<Core>, mpsc::Receiver<Event>, native::Peer) {
    let peer = endpoint.own_process().unwrap();
    let (events, receiver) = mpsc::channel(16);
    (
        Arc::new(Core {
            generation: native::random_token().unwrap(),
            process: Process::from(&peer),
            events,
            records: Mutex::new(Records::default()),
        }),
        receiver,
        peer,
    )
}
fn file(name: &str) -> NativePath {
    NativePath::encode(&std::env::temp_dir().join(name)).unwrap()
}

struct TestChild(std::process::Child);
impl Drop for TestChild {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|exit| exit.is_none()) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn test_child(
    role: &str,
    nonce: [u8; 16],
    source: &std::path::Path,
    marker: &std::path::Path,
) -> TestChild {
    TestChild(
        Child::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", role, "--nocapture"])
            .env(
                "RESHIKI_IPC_TEST_NONCE",
                nonce
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )
            .env("RESHIKI_IPC_TEST_FILE", source)
            .env("RESHIKI_IPC_TEST_MARKER", marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn test_endpoint() -> native::Endpoint {
    let nonce = std::env::var("RESHIKI_IPC_TEST_NONCE").unwrap();
    let token = private_nonce(&["--desktop-private".into(), nonce.into()])
        .unwrap()
        .unwrap();
    native::Endpoint::current().unwrap().private(token).unwrap()
}

#[test]
fn native_path_round_trip_and_bounds_preserve_filename_units() {
    let path = std::env::temp_dir().join("構造 β with spaces.rsk");
    assert_eq!(NativePath::encode(&path).unwrap().decode().unwrap(), path);
    assert!(NativePath::encode(std::path::Path::new("relative.rsk")).is_err());
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let path = std::env::temp_dir().join(std::ffi::OsString::from_wide(&[
            b'x' as u16,
            0xd800,
            b'.' as u16,
            b'r' as u16,
            b's' as u16,
            b'k' as u16,
        ]));
        assert_eq!(NativePath::encode(&path).unwrap().decode().unwrap(), path);
        assert!(NativePath::Windows(vec![0; 10]).decode().is_err());
        assert!(NativePath::Windows(vec![65; 32768]).decode().is_err());
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStringExt;
        let path =
            std::env::temp_dir().join(std::ffi::OsString::from_vec(b"drawing-\xff.rsk".to_vec()));
        assert_eq!(NativePath::encode(&path).unwrap().decode().unwrap(), path);
        assert!(NativePath::Unix(vec![0; 10]).decode().is_err());
        assert!(NativePath::Unix(vec![65; 4097]).decode().is_err());
    }
}

#[test]
fn framed_protocol_rejects_oversized_truncated_and_unknown_fields() {
    let endpoint = isolated();
    let peer = endpoint.own_process().unwrap();
    let request = Request::new(
        [1; 16],
        None,
        Process::from(&peer),
        Command::Open {
            paths: vec![file("a.rsk"), file("b.rsk")],
        },
    );
    let mut bytes = vec![];
    protocol::write(&mut bytes, &request).unwrap();
    let decoded: Request = protocol::read(&mut bytes.as_slice()).unwrap();
    assert!(decoded.valid(&peer));
    assert_eq!(decoded.command, request.command);
    for bytes in [
        vec![],
        vec![1, 0, 0, 0],
        ((protocol::FRAME_LIMIT + 1) as u32).to_le_bytes().to_vec(),
        vec![0, 0, 0, 0],
    ] {
        assert!(protocol::read::<Request>(&mut bytes.as_slice()).is_err());
    }
    let mut json = serde_json::to_value(&request).unwrap();
    json["unexpected"] = true.into();
    assert!(serde_json::from_value::<Request>(json).is_err());
    let mut wrong = request.clone();
    wrong.sender.start += 1;
    assert!(!wrong.valid(&peer));
    let mut many = request;
    many.command = Command::Open {
        paths: (0..65).map(|_| file("a.rsk")).collect(),
    };
    assert!(!many.valid(&peer));
}

#[test]
fn semantic_launch_flags_preserve_workers_and_explicit_gui_actions() {
    let args = |values: &[&str]| {
        values
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    };
    for flag in [
        "--ole-server",
        "--graphics-info",
        "--engine-check",
        "--desktop-host",
    ] {
        assert!(!crate::launch::gui_flag(args(&["--open", flag]), flag));
        assert!(crate::launch::gui_flag(
            args(&["--open", "a.rsk", flag]),
            flag
        ));
    }
    assert!(forwardable(&args(&["--open", "a.rsk", "--open", "b.rsk"])));
    for mode in [
        "--cli",
        "--mcp",
        "--geometry-worker",
        "--inchi-worker",
        "--shortcut-examples",
        "--future-gui-action",
    ] {
        assert!(!forwardable(&args(&[mode])));
    }
    assert!(
        private_nonce(&args(&["--open", "--desktop-private"]))
            .unwrap()
            .is_none()
    );
    assert!(private_nonce(&args(&["--desktop-private", "bad"])).is_err());
    for flag in ["--office-edit", "--libreoffice-edit", "--office-addin-edit"] {
        let supported = args(&["--open", "a.rsk", flag]);
        assert!(
            office_role(&supported, &crate::app::startup::parse(supported.clone()))
                .unwrap()
                .is_some()
        );
        let invalid = args(&["--open", "a.rsk", flag, "--shortcut-examples"]);
        assert!(office_role(&invalid, &crate::app::startup::parse(invalid.clone())).is_err());
        let missing = args(&[flag]);
        assert!(office_role(&missing, &crate::app::startup::parse(missing.clone())).is_err());
    }
}

#[test]
fn abandoned_or_expired_prepare_cannot_be_revived_by_a_late_ui_reply() {
    for expire in [false, true] {
        let endpoint = isolated();
        let (core, mut receiver, peer) = test_core(&endpoint);
        let token = [9; 16];
        let prepare = Request::new(
            token,
            None,
            Process::from(&peer),
            Command::Prepare {
                token,
                path: file("late.rsk"),
                host: OfficeHost::Office,
            },
        );
        let serving = core.clone();
        let request = prepare.clone();
        let owner = peer.clone();
        let waiting = thread::spawn(move || serving.dispatch(request, owner));
        let Event::Prepare(binding, reply) = receiver.blocking_recv().unwrap() else {
            panic!("Prepare");
        };
        let lease = binding.lease.unwrap();
        let command =
            |command| Request::new(token, Some(core.generation), Process::from(&peer), command);
        if expire {
            core.records.lock().unwrap().sessions[0].created =
                Instant::now() - Duration::from_secs(6);
            core.reap();
        } else {
            assert_eq!(
                core.dispatch(command(Command::Abandon { token }), peer.clone()),
                Status::Session(Phase::Rejected)
            );
        }
        reply.set(Status::Prepared);
        assert_eq!(waiting.join().unwrap(), Status::Rejected);
        assert_eq!(lease.phase(), Phase::Rejected);
        assert_eq!(core.dispatch(prepare, peer.clone()), Status::Rejected);
        assert_eq!(
            core.dispatch(command(Command::Commit { token }), peer),
            Status::Rejected
        );
        assert!(receiver.try_recv().is_err());
    }
}

#[test]
fn office_busy_during_startup_can_retry_the_same_launch_after_ui_is_ready() {
    let endpoint = isolated();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let ui = thread::spawn(move || {
        let Event::Prepare(old, reply) = receiver.blocking_recv().unwrap() else {
            panic!("Prepare");
        };
        reply.set(Status::Busy);
        let Event::Prepare(new, reply) = receiver.blocking_recv().unwrap() else {
            panic!("Prepare retry");
        };
        assert_eq!(old.lease.unwrap().phase(), Phase::Rejected);
        assert_eq!(new.lease.as_ref().unwrap().phase(), Phase::Prepared);
        reply.set(Status::Prepared);
        receiver
    });
    let prepare = Request::new(
        [8; 16],
        None,
        Process::from(&peer),
        Command::Prepare {
            token: [8; 16],
            path: file("starting.rsk"),
            host: OfficeHost::Office,
        },
    );
    assert_eq!(core.dispatch(prepare.clone(), peer.clone()), Status::Busy);
    assert!(core.records.lock().unwrap().sessions.is_empty());
    assert_eq!(core.dispatch(prepare, peer), Status::Prepared);
    assert_eq!(core.records.lock().unwrap().sessions.len(), 1);
    assert!(ui.join().unwrap().try_recv().is_err());
}

#[test]
fn wrong_generation_and_full_ui_queue_never_take_office_ownership() {
    let endpoint = isolated();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let prepare = Request::new(
        [4; 16],
        Some([0; 16]),
        Process::from(&peer),
        Command::Prepare {
            token: [4; 16],
            path: file("full.rsk"),
            host: OfficeHost::LibreOffice,
        },
    );
    assert_eq!(
        core.dispatch(prepare.clone(), peer.clone()),
        Status::Rejected
    );
    assert!(receiver.try_recv().is_err());
    for _ in 0..16 {
        core.events
            .try_send(Event::Open(vec![], Reply::new()))
            .unwrap();
    }
    let mut prepare = prepare;
    prepare.generation = Some(core.generation);
    assert_eq!(core.dispatch(prepare, peer), Status::Busy);
    assert!(core.records.lock().unwrap().sessions.is_empty());
}

#[test]
fn source_claim_covers_private_endpoints_and_canonical_aliases() {
    let endpoint = isolated();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("claimed.rsk");
    std::fs::write(&path, b"owned").unwrap();
    let claim = endpoint.claim(&path).unwrap().unwrap();
    let other = endpoint.private(native::random_token().unwrap()).unwrap();
    let alias = directory.path().join(".").join("claimed.rsk");
    assert!(
        !thread::spawn(move || other.claim(&alias).unwrap().is_some())
            .join()
            .unwrap()
    );
    drop(claim);
    assert!(endpoint.claim(&path).unwrap().is_some());
}

#[test]
fn host_write_fence_excludes_same_listener_successor_until_save_worker_finishes() {
    let endpoint = isolated();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("host-owned.rsk");
    std::fs::write(&path, b"owned").unwrap();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let ui = thread::spawn(move || {
        let Event::Prepare(binding, reply) = receiver.blocking_recv().unwrap() else {
            panic!("Prepare");
        };
        reply.set(Status::Prepared);
        (binding, receiver)
    });
    let prepare = |token| {
        Request::new(
            [token; 16],
            None,
            Process::from(&peer),
            Command::Prepare {
                token: [token; 16],
                path: NativePath::encode(&path).unwrap(),
                host: OfficeHost::Office,
            },
        )
    };
    let commit = |token| {
        Request::new(
            [token; 16],
            Some(core.generation),
            Process::from(&peer),
            Command::Commit { token: [token; 16] },
        )
    };
    let mut fences = Fences::default();
    assert_eq!(
        fences.dispatch(&endpoint, &core, prepare(13), peer.clone()),
        Status::Prepared
    );
    assert!(fences.0.is_empty(), "Prepare owns no writer fence");
    let (binding, mut receiver) = ui.join().unwrap();
    assert_eq!(
        fences.dispatch(&endpoint, &core, commit(13), peer.clone()),
        Status::Pending
    );
    let Event::Commit(worker) = receiver.blocking_recv().unwrap() else {
        panic!("Commit");
    };
    let lease = binding.lease.as_ref().unwrap();
    lease.transition(Phase::Prepared, Phase::Open);
    lease.transition(Phase::Open, Phase::Lost);
    core.records.lock().unwrap().sessions.clear(); // dead-proxy reaper
    drop(binding); // UI revocation; worker remains alive
    fences.reap();
    assert_eq!(fences.0.len(), 1);
    let target = path.clone();
    assert!(
        thread::spawn(move || write_target(&target).is_err())
            .join()
            .unwrap(),
        "Save As cannot overwrite the retained worker's path"
    );
    let successor = endpoint.private(native::random_token().unwrap()).unwrap();
    let target = path.clone();
    assert!(
        !thread::spawn(move || successor.write_claim(&target).unwrap().is_some())
            .join()
            .unwrap(),
        "Private successor excludes in-flight writer"
    );
    let ui = thread::spawn(move || {
        let Event::Prepare(_, reply) = receiver.blocking_recv().unwrap() else {
            panic!("Successor Prepare");
        };
        reply.set(Status::Prepared);
        receiver
    });
    assert_eq!(
        fences.dispatch(&endpoint, &core, prepare(14), peer.clone()),
        Status::Prepared
    );
    let mut receiver = ui.join().unwrap();
    assert_eq!(
        fences.dispatch(&endpoint, &core, commit(14), peer.clone()),
        Status::Busy,
        "The SAME listener must not recurse into another lease's mutex"
    );
    assert!(receiver.try_recv().is_err());
    drop(worker); // host acknowledgement/write completed
    fences.reap();
    assert!(fences.0.is_empty());
    assert_eq!(
        fences.dispatch(&endpoint, &core, commit(14), peer),
        Status::Pending
    );
    assert!(matches!(receiver.try_recv(), Ok(Event::Commit(_))));
    assert_eq!(fences.0.len(), 1);
}

#[test]
fn stalled_shared_prepare_leaves_private_fallback_unfenced() {
    let endpoint = isolated();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let mut shared = Fences::default();
    let prepare = Request::new(
        [15; 16],
        None,
        Process::from(&peer),
        Command::Prepare {
            token: [15; 16],
            path: file("fallback.rsk"),
            host: OfficeHost::Office,
        },
    );
    assert_eq!(
        shared.dispatch(&endpoint, &core, prepare.clone(), peer.clone()),
        Status::Pending
    );
    assert!(shared.0.is_empty());
    assert!(matches!(receiver.try_recv(), Ok(Event::Prepare(_, _))));
    let private = endpoint.private(native::random_token().unwrap()).unwrap();
    let (private_core, mut private_ui, _) = test_core(&private);
    let ui = thread::spawn(move || {
        let Event::Prepare(_, reply) = private_ui.blocking_recv().unwrap() else {
            panic!("Private Prepare");
        };
        reply.set(Status::Prepared);
        private_ui
    });
    let mut private_fences = Fences::default();
    assert_eq!(
        private_fences.dispatch(&private, &private_core, prepare, peer.clone()),
        Status::Prepared
    );
    let mut private_ui = ui.join().unwrap();
    let commit = Request::new(
        [15; 16],
        Some(private_core.generation),
        Process::from(&peer),
        Command::Commit { token: [15; 16] },
    );
    assert_eq!(
        private_fences.dispatch(&private, &private_core, commit, peer),
        Status::Pending
    );
    assert!(matches!(private_ui.try_recv(), Ok(Event::Commit(_))));
    assert_eq!(private_fences.0.len(), 1);
}

#[test]
fn queue_full_commit_releases_uncommitted_writer_fence_before_retry() {
    let endpoint = isolated();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let ui = thread::spawn(move || {
        let Event::Prepare(_, reply) = receiver.blocking_recv().unwrap() else {
            panic!("Prepare");
        };
        reply.set(Status::Prepared);
        receiver
    });
    let prepare = Request::new(
        [16; 16],
        None,
        Process::from(&peer),
        Command::Prepare {
            token: [16; 16],
            path: file("queue-full.rsk"),
            host: OfficeHost::Office,
        },
    );
    let mut fences = Fences::default();
    assert_eq!(
        fences.dispatch(&endpoint, &core, prepare, peer.clone()),
        Status::Prepared
    );
    let mut receiver = ui.join().unwrap();
    for _ in 0..16 {
        core.events
            .try_send(Event::Open(vec![], Reply::new()))
            .unwrap();
    }
    let commit = Request::new(
        [16; 16],
        Some(core.generation),
        Process::from(&peer),
        Command::Commit { token: [16; 16] },
    );
    assert_eq!(
        fences.dispatch(&endpoint, &core, commit.clone(), peer.clone()),
        Status::Busy
    );
    assert!(fences.0.is_empty());
    assert!(!core.records.lock().unwrap().sessions[0].committed);
    let path = file("queue-full.rsk").decode().unwrap();
    assert!(
        thread::spawn(move || write_target(&path).is_ok())
            .join()
            .unwrap()
    );
    for _ in 0..16 {
        assert!(matches!(receiver.try_recv(), Ok(Event::Open(_, _))));
    }
    assert_eq!(
        fences.dispatch(&endpoint, &core, commit, peer),
        Status::Pending
    );
    assert_eq!(fences.0.len(), 1);
}

#[test]
fn ordinary_ack_waits_for_ui_and_retry_is_deduplicated() {
    let endpoint = isolated();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let request = Request::new(
        [1; 16],
        None,
        Process::from(&peer),
        Command::Open {
            paths: vec![file("a.rsk")],
        },
    );
    let ui = thread::spawn(move || {
        match receiver.blocking_recv().unwrap() {
            Event::Open(paths, reply) => {
                assert_eq!(paths.len(), 1);
                reply.set(Status::Accepted);
            }
            _ => panic!("Ordinary request"),
        }
        receiver
    });
    assert_eq!(
        core.dispatch(request.clone(), peer.clone()),
        Status::Accepted
    );
    let mut receiver = ui.join().unwrap();
    assert_eq!(core.dispatch(request, peer), Status::Accepted);
    assert!(receiver.try_recv().is_err());
    assert_eq!(core.records.lock().unwrap().ordinary.len(), 1);
}

#[test]
fn office_commit_retries_keep_one_lease_and_completion_survives_until_ack() {
    let endpoint = isolated();
    let (core, mut receiver, peer) = test_core(&endpoint);
    let token = [7; 16];
    let (lease_tx, lease_rx) = sync::channel();
    let ui = thread::spawn(move || {
        match receiver.blocking_recv().unwrap() {
            Event::Prepare(binding, reply) => {
                assert_eq!(binding.lease.as_ref().unwrap().phase(), Phase::Prepared);
                reply.set(Status::Prepared);
            }
            _ => panic!("Prepare"),
        }
        match receiver.blocking_recv().unwrap() {
            Event::Commit(binding) => {
                let lease = binding.lease.unwrap();
                lease.transition(Phase::Prepared, Phase::Open);
                lease_tx.send(lease).unwrap();
            }
            _ => panic!("Commit"),
        }
        receiver
    });
    let prepare = Request::new(
        token,
        None,
        Process::from(&peer),
        Command::Prepare {
            token,
            path: file("a.rsk"),
            host: OfficeHost::Office,
        },
    );
    assert_eq!(
        core.dispatch(prepare.clone(), peer.clone()),
        Status::Prepared
    );
    assert_eq!(core.dispatch(prepare, peer.clone()), Status::Prepared);
    let command =
        |command| Request::new(token, Some(core.generation), Process::from(&peer), command);
    assert!(matches!(
        core.dispatch(command(Command::Commit { token }), peer.clone()),
        Status::Pending | Status::Session(Phase::Open)
    ));
    let lease = lease_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut receiver = ui.join().unwrap();
    assert_eq!(
        core.dispatch(command(Command::Commit { token }), peer.clone()),
        Status::Session(Phase::Open)
    );
    assert!(receiver.try_recv().is_err());
    lease.transition(Phase::Open, Phase::Closed);
    assert_eq!(
        core.dispatch(command(Command::Query { token }), peer.clone()),
        Status::Session(Phase::Closed)
    );
    assert_eq!(core.records.lock().unwrap().sessions.len(), 1);
    assert_eq!(
        core.dispatch(command(Command::Ack { token }), peer),
        Status::Session(Phase::Closed)
    );
    assert!(core.records.lock().unwrap().sessions.is_empty());
}

#[test]
fn native_pipe_or_socket_has_an_elected_authenticated_owner_and_deadlines() {
    let endpoint = isolated();
    let election = endpoint.elect().unwrap().unwrap();
    let other = endpoint.clone();
    assert!(
        !thread::spawn(move || other.elect().unwrap().is_some())
            .join()
            .unwrap()
    );
    let mut listener = endpoint.listen(&election).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, peer) = listener.accept(Duration::from_secs(2)).unwrap();
        assert!(peer.is_alive());
        let mut bytes = [0; 3];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"one");
        stream.write_all(b"two").unwrap();
        drop(stream);
        let (mut silent, _) = listener.accept(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        assert!(silent.read_exact(&mut [0; 1]).is_err());
        assert!(start.elapsed() < Duration::from_secs(4));
    });
    let (mut stream, peer) = endpoint.connect(Duration::from_secs(2)).unwrap();
    assert_eq!(peer.pid, std::process::id());
    stream.write_all(b"one").unwrap();
    let mut bytes = [0; 3];
    stream.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"two");
    drop(stream);
    let (_silent, _) = endpoint.connect(Duration::from_secs(2)).unwrap();
    server.join().unwrap();
}

#[test]
#[ignore = "Internal child role; the parent test owns the fake host"]
fn office_proxy_child() {
    let endpoint = test_endpoint();
    let source = std::env::var_os("RESHIKI_IPC_TEST_FILE").unwrap();
    let startup = crate::app::startup::parse(["--open".into(), source, "--office-edit".into()]);
    std::process::exit(office_proxy(endpoint, &startup, OfficeHost::Office));
}

#[test]
#[ignore = "Internal non-GUI host child; the parent owns its process lifetime"]
fn office_fake_host_child() {
    let endpoint = test_endpoint();
    let election = endpoint.elect().unwrap().unwrap();
    let mut listener = endpoint.listen(&election).unwrap();
    let (core, mut receiver, _) = test_core(&endpoint);
    let marker = std::path::PathBuf::from(std::env::var_os("RESHIKI_IPC_TEST_MARKER").unwrap());
    std::fs::write(marker.with_extension("ready"), b"bound").unwrap();
    thread::spawn(move || {
        while let Some(event) = receiver.blocking_recv() {
            match event {
                Event::Prepare(_, reply) => reply.set(Status::Prepared),
                Event::Commit(binding) => {
                    binding
                        .lease
                        .unwrap()
                        .transition(Phase::Prepared, Phase::Open);
                    std::fs::write(&marker, b"open").unwrap();
                }
                Event::Open(_, reply) => reply.set(Status::Accepted),
            }
        }
    });
    let mut fences = Fences::default();
    loop {
        if let Ok((mut stream, peer)) = listener.accept(Duration::from_millis(100))
            && let Ok(request) = protocol::read::<Request>(&mut stream)
        {
            let status = fences.dispatch(&endpoint, &core, request, peer);
            let _ = protocol::write(
                &mut stream,
                &Response::new(core.generation, core.process, status),
            );
        }
    }
}

#[test]
fn watched_proxy_reports_failure_only_after_its_exact_host_process_dies() {
    let nonce = native::random_token().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("owned.rsk");
    let marker = directory.path().join("session");
    std::fs::write(&source, b"fake-host-fixture").unwrap();
    let mut host = test_child(
        "desktop::tests::office_fake_host_child",
        nonce,
        &source,
        &marker,
    );
    let wait_marker = |path: &std::path::Path| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "Child did not publish {}",
                path.display()
            );
            thread::sleep(Duration::from_millis(25));
        }
    };
    wait_marker(&marker.with_extension("ready"));
    let mut proxy = test_child(
        "desktop::tests::office_proxy_child",
        nonce,
        &source,
        &marker,
    );
    wait_marker(&marker);
    assert!(proxy.0.try_wait().unwrap().is_none());
    host.0.kill().unwrap();
    host.0.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = loop {
        if let Some(exit) = proxy.0.try_wait().unwrap() {
            break exit;
        }
        assert!(
            Instant::now() < deadline,
            "Proxy did not detect the dead owner"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert!(
        !exit.success(),
        "A crashed GUI is not normal session completion"
    );
}

#[test]
fn watched_office_proxy_process_stays_alive_until_its_session_closes() {
    let nonce = native::random_token().unwrap();
    let endpoint = native::Endpoint::current().unwrap().private(nonce).unwrap();
    let election = endpoint.elect().unwrap().unwrap();
    let mut listener = endpoint.listen(&election).unwrap();
    let (core, mut receiver, _) = test_core(&endpoint);
    let serving = core.clone();
    let serving_endpoint = endpoint.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = thread::spawn(move || {
        let mut fences = Fences::default();
        let mut dropped_commit_reply = false;
        let mut dropped_queries = 0;
        while !stopping.load(Ordering::Acquire) {
            if let Ok((mut stream, peer)) = listener.accept(Duration::from_millis(100))
                && let Ok(request) = protocol::read::<Request>(&mut stream)
            {
                let commit = matches!(request.command, Command::Commit { .. });
                let query = matches!(request.command, Command::Query { .. });
                let status = fences.dispatch(&serving_endpoint, &serving, request, peer);
                // A committed Open with a lost acknowledgement, followed
                // by transient IPC loss, is still a live editing session.
                if commit && !dropped_commit_reply {
                    dropped_commit_reply = true;
                    continue;
                }
                if query && dropped_queries < 3 {
                    dropped_queries += 1;
                    continue;
                }
                let _ = protocol::write(
                    &mut stream,
                    &Response::new(serving.generation, serving.process, status),
                );
            }
        }
    });
    let (lease_tx, lease_rx) = sync::channel();
    let ui = thread::spawn(move || {
        while let Some(event) = receiver.blocking_recv() {
            match event {
                Event::Prepare(_, reply) => reply.set(Status::Prepared),
                Event::Commit(binding) => {
                    let lease = binding.lease.unwrap();
                    lease.transition(Phase::Prepared, Phase::Open);
                    lease_tx.send(lease).unwrap();
                }
                Event::Open(_, reply) => reply.set(Status::Accepted),
            }
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("drawing.rsk");
    std::fs::write(&source, b"fake-host-fixture").unwrap();
    let mut child = Child::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "desktop::tests::office_proxy_child",
            "--nocapture",
        ])
        .env(
            "RESHIKI_IPC_TEST_NONCE",
            nonce
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .env("RESHIKI_IPC_TEST_FILE", &source)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let lease = match lease_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(lease) => lease,
        Err(error) => {
            let _ = child.kill();
            panic!("Proxy did not open its lease: {error}");
        }
    };
    assert!(
        child.try_wait().unwrap().is_none(),
        "Office's watched child must still be running after handoff"
    );
    thread::sleep(Duration::from_millis(900));
    assert!(
        child.try_wait().unwrap().is_none(),
        "Lost commit ACK/temporary IPC loss is not completion"
    );
    assert_eq!(core.records.lock().unwrap().sessions.len(), 1);
    let client = Client::new(endpoint).unwrap();
    assert!(client.open(vec![], Instant::now() + HANDOFF));
    assert!(
        child.try_wait().unwrap().is_none(),
        "Unrelated ordinary activation is not completion"
    );
    lease.transition(Phase::Open, Phase::Closed);
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("Proxy did not exit after its lease completed");
        }
        thread::sleep(Duration::from_millis(25));
    };
    assert!(exit.success());
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    drop(core);
    ui.join().unwrap();
    drop(election);
}
