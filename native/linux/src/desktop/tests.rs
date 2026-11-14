use super::*;
use std::{
    os::{fd::OwnedFd, unix::fs::symlink},
    process::{Child, Command, Stdio},
    thread,
};

struct Fixture {
    endpoint: Endpoint,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let token = random_token()
            .unwrap()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("reshiki-ipc-test-{token}"));
        let uid = process::getuid().as_raw();
        let directory = Arc::new(private_directory(&path, uid).unwrap());
        Self {
            endpoint: Endpoint {
                root: directory.clone(),
                directory,
                uid,
            },
            path,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|exit| exit.is_none()) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// Another test thread can spawn while a claim is held. A pre-exec child
/// temporarily shares the flock's open-file description even with CLOEXEC;
/// closing the parent's File need not immediately close the last reference.
/// This finite poll tolerates that window, not errors or persistent leaks.
fn reacquire_after_drop(mut acquire: impl FnMut() -> io::Result<Option<Election>>) -> Election {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        assert!(
            Instant::now() < deadline,
            "Dropped election was not released within two seconds"
        );
        match acquire().expect("Reacquiring a dropped election must not fail") {
            Some(election) => return election,
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "Dropped election was not released within two seconds"
                );
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        }
    }
}

#[test]
fn private_runtime_rejects_owner_mode_and_symlink_surprises() {
    let fixture = Fixture::new();
    assert!(metadata(&fixture.path, fixture.endpoint.uid + 1, 0o700).is_err());
    std::fs::set_permissions(&fixture.path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(private_directory(&fixture.path, fixture.endpoint.uid).is_err());
    std::fs::set_permissions(&fixture.path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let alias = fixture.path.join("alias");
    symlink(&fixture.path, &alias).unwrap();
    assert!(private_directory(&alias, fixture.endpoint.uid).is_err());
    let socket = fixture.endpoint.path("desktop.sock");
    symlink(&alias, &socket).unwrap();
    let election = fixture.endpoint.elect().unwrap().unwrap();
    assert!(fixture.endpoint.listen(&election).is_err());
    assert!(fixture.endpoint.connect(Duration::from_millis(20)).is_err());
    assert!(
        std::fs::symlink_metadata(socket)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn flock_election_and_stale_socket_cleanup_keep_live_unknown_endpoints() {
    let fixture = Fixture::new();
    let election = fixture.endpoint.elect().unwrap().unwrap();
    assert!(fixture.endpoint.elect().unwrap().is_none());
    let path = fixture.endpoint.path("desktop.sock");
    // A bound, non-listening socket refuses connections like a dead host's socket;
    // a concurrent child's pre-exec copy cannot accept connections either.
    let stale = net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::CLOEXEC,
        None,
    )
    .unwrap();
    net::bind(&stale, &SocketAddrUnix::new(&path).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    drop(stale);
    let listener = fixture.endpoint.listen(&election).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    drop(election);
    let successor = reacquire_after_drop(|| fixture.endpoint.elect());
    assert!(
        fixture.endpoint.elect().unwrap().is_none(),
        "Successor still excludes another host"
    );
    assert_eq!(
        fixture.endpoint.listen(&successor).err().unwrap().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_socket()
    );
    drop(listener);
    assert!(!path.exists());
    assert!(fixture.endpoint.listen(&successor).is_ok());
}

/// Model a host that crashes while a child is between fork and exec: passing
/// its lock and listener through sleep's stdio keeps both open in that child
/// only. The ghost accepts connections, but it also excludes any successor.
#[test]
fn crashed_host_fork_window_ghost_keeps_the_lock_until_it_closes() {
    let fixture = Fixture::new();
    let Election { _lock: lock, .. } = fixture.endpoint.elect().unwrap().unwrap();
    let path = fixture.endpoint.path("desktop.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut ghost = ChildGuard(
        Command::new("sleep")
            .arg("30")
            .stdin(Stdio::from(OwnedFd::from(listener)))
            .stdout(Stdio::from(lock))
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(
        fixture.endpoint.connect(Duration::from_millis(50)).is_ok(),
        "A pre-exec child keeps the crashed host's listener connectable"
    );
    assert!(
        fixture.endpoint.elect().unwrap().is_none(),
        "The same child also keeps the host lock, so no successor can probe it"
    );
    ghost.0.kill().unwrap();
    ghost.0.wait().unwrap();
    reacquire_after_drop(|| fixture.endpoint.elect());
}

#[test]
fn write_claim_is_shared_across_private_fallbacks_and_canonical_aliases() {
    let fixture = Fixture::new();
    let path = fixture.path.join("host.rsk");
    std::fs::write(&path, b"owned").unwrap();
    let dotted = fixture.path.join(".").join("host.rsk");
    let alias = fixture.path.join("host-alias.rsk");
    symlink(&path, &alias).unwrap();
    let claim = fixture.endpoint.write_claim(&path).unwrap().unwrap();
    let private = fixture.endpoint.private(random_token().unwrap()).unwrap();
    for endpoint in [&fixture.endpoint, &private] {
        for candidate in [&path, &dotted, &alias] {
            assert!(
                endpoint.write_claim(candidate).unwrap().is_none(),
                "Held writer excludes private and canonical aliases"
            );
        }
    }
    let launch = private
        .claim(&path)
        .unwrap()
        .expect("launch and writer roles are separate");
    assert!(
        fixture.endpoint.claim(&alias).unwrap().is_none(),
        "Launch claims also share the canonical source identity"
    );
    drop(claim);
    let successor = reacquire_after_drop(|| private.write_claim(&alias));
    for endpoint in [&fixture.endpoint, &private] {
        for candidate in [&path, &dotted, &alias] {
            assert!(
                endpoint.write_claim(candidate).unwrap().is_none(),
                "Successor writer retains exclusion across aliases"
            );
        }
    }
    assert!(
        fixture.endpoint.claim(&path).unwrap().is_none(),
        "Releasing the writer does not release the launch claim"
    );
    drop(successor);
    drop(launch);
}

#[test]
#[ignore = "Internal authenticated non-GUI client process"]
fn peer_child() {
    let path = PathBuf::from(std::env::var_os("RESHIKI_NATIVE_IPC_TEST_DIR").unwrap());
    let uid = process::getuid().as_raw();
    let directory = Arc::new(private_directory(&path, uid).unwrap());
    let endpoint = Endpoint {
        root: directory.clone(),
        directory,
        uid,
    };
    let (mut stream, peer) = endpoint.connect(Duration::from_secs(2)).unwrap();
    assert!(peer.is_alive());
    stream.write_all(b"ping").unwrap();
    let mut answer = [0; 4];
    stream.read_exact(&mut answer).unwrap();
    assert_eq!(&answer, b"pong");
    thread::sleep(Duration::from_secs(20));
}

#[test]
fn native_peer_credentials_identify_exact_child_and_detect_process_death() {
    let fixture = Fixture::new();
    let election = fixture.endpoint.elect().unwrap().unwrap();
    let mut listener = fixture.endpoint.listen(&election).unwrap();
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "desktop::tests::peer_child"])
            .env("RESHIKI_NATIVE_IPC_TEST_DIR", &fixture.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let (mut stream, peer) = listener
        .accept(Duration::from_secs(3), Duration::from_secs(2))
        .unwrap();
    assert_eq!(peer.pid, child.0.id());
    assert!(peer.is_alive());
    let mut bytes = [0; 4];
    stream.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"ping");
    stream.write_all(b"pong").unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(!peer.is_alive());
}

#[test]
fn silent_peer_is_bounded_and_does_not_prevent_the_next_connection() {
    let fixture = Fixture::new();
    let election = fixture.endpoint.elect().unwrap().unwrap();
    let mut listener = fixture.endpoint.listen(&election).unwrap();
    let client = fixture.endpoint.clone();
    let silent = thread::spawn(move || client.connect(Duration::from_secs(2)).unwrap());
    let (mut stream, _) = listener
        .accept(Duration::from_secs(2), Duration::from_secs(2))
        .unwrap();
    let (_silent, _peer) = silent.join().unwrap();
    let before = Instant::now();
    assert!(stream.read_exact(&mut [0; 1]).is_err());
    assert!(before.elapsed() < Duration::from_secs(4));
    drop(stream);
    drop(_silent);
    let client = fixture.endpoint.clone();
    let next = thread::spawn(move || client.connect(Duration::from_secs(2)).unwrap());
    assert!(
        listener
            .accept(Duration::from_secs(2), Duration::from_secs(2))
            .is_ok()
    );
    assert!(next.join().is_ok());
}

/// S17: without `XDG_RUNTIME_DIR` the endpoint used a predictable name in
/// `/tmp`, which any user could create first. It now stays in folders only
/// this user can write.
#[test]
fn the_endpoint_folder_is_never_a_shared_temporary_folder() {
    let fixture = Fixture::new();
    let (uid, root) = (fixture.endpoint.uid, &fixture.path);
    let run_user = root.join("run");
    let home = root.join("home");
    let folder = |runtime: Option<&Path>, cache: Option<&Path>| {
        let (runtime, cache) = (runtime.map(Path::to_owned), cache.map(Path::to_owned));
        base(
            uid,
            |name| match name {
                "XDG_RUNTIME_DIR" => runtime.clone().map(OsString::from),
                "XDG_CACHE_HOME" => cache.clone().map(OsString::from),
                "HOME" => Some(home.clone().into()),
                _ => None,
            },
            &run_user,
        )
    };
    let mode = |path: &Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    // No runtime folder at all: the user's cache folder, made when missing.
    assert_eq!(folder(None, None).unwrap(), home.join(".cache"));
    assert!(home.join(".cache").is_dir());
    let cache = root.join("cache");
    assert_eq!(folder(None, Some(&cache)).unwrap(), cache);
    assert_eq!(
        folder(None, Some(Path::new("relative"))).unwrap(),
        home.join(".cache")
    );
    // systemd's folder counts only when it is the user's own, mode 0700.
    let runtime = run_user.join(uid.to_string());
    std::fs::create_dir_all(&runtime).unwrap();
    mode(&runtime, 0o755);
    assert_eq!(folder(None, None).unwrap(), home.join(".cache"));
    mode(&runtime, 0o700);
    assert_eq!(folder(None, None).unwrap(), runtime);
    // XDG_RUNTIME_DIR comes first, and an unsafe one is an error, not a
    // reason to look elsewhere.
    let named = root.join("named");
    std::fs::create_dir(&named).unwrap();
    mode(&named, 0o700);
    assert_eq!(folder(Some(&named), None).unwrap(), named);
    mode(&named, 0o755);
    assert!(folder(Some(&named), None).is_err());
    assert!(folder(Some(&root.join("missing")), None).is_err());
    // Nothing to go by: an error, never /tmp.
    assert!(base(uid, |_| None, &root.join("no-run")).is_err());
}

/// S17: a cache folder that already exists is used only when it is the
/// user's own and no one else can write to it. `XDG_CACHE_HOME=/tmp`, or a
/// link into such a folder, would put the endpoint's predictable name where
/// another user can create it first.
#[test]
fn a_cache_folder_others_can_write_to_is_not_used() {
    let fixture = Fixture::new();
    let (uid, root) = (fixture.endpoint.uid, &fixture.path);
    let home = root.join("home");
    let folder = |uid: u32, cache: &Path| {
        let cache = cache.to_owned();
        base(
            uid,
            |name| match name {
                "XDG_CACHE_HOME" => Some(cache.clone().into()),
                "HOME" => Some(home.clone().into()),
                _ => None,
            },
            &root.join("no-run"),
        )
    };
    let mode = |path: &Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    let shared = root.join("shared");
    std::fs::create_dir(&shared).unwrap();
    // As /tmp, then writable by a group, then by anyone without the sticky bit.
    for bits in [0o1777, 0o770, 0o777] {
        mode(&shared, bits);
        assert_eq!(
            folder(uid, &shared).unwrap(),
            home.join(".cache"),
            "{bits:o}"
        );
    }
    // A link into the shared folder is that folder.
    let link = root.join("link");
    symlink(&shared, &link).unwrap();
    assert_eq!(folder(uid, &link).unwrap(), home.join(".cache"));
    // A folder of another user.
    let private = root.join("private");
    std::fs::create_dir(&private).unwrap();
    mode(&private, 0o755);
    assert!(folder(uid + 1, &private).is_err());
    // The user's own folder is used, through a link too.
    assert_eq!(folder(uid, &private).unwrap(), private);
    std::fs::remove_file(&link).unwrap();
    symlink(&private, &link).unwrap();
    assert_eq!(folder(uid, &link).unwrap(), private);
    // No private folder at all: an error.
    mode(&home.join(".cache"), 0o777);
    assert!(folder(uid, &shared).is_err());
    // The endpoint is made in the folder that was checked.
    mode(&home.join(".cache"), 0o700);
    let endpoint = folder(uid, &shared).unwrap().join("reshiki-desktop-test");
    private_directory(&endpoint, uid).unwrap();
    assert!(home.join(".cache/reshiki-desktop-test").is_dir());
    assert_eq!(std::fs::read_dir(&shared).unwrap().count(), 0);
}
