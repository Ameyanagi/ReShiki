use super::*;
use std::{
    os::unix::fs::symlink,
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
    let stale = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    drop(stale);
    let listener = fixture.endpoint.listen(&election).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    drop(election);
    let successor = fixture.endpoint.elect().unwrap().unwrap();
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

#[test]
fn write_claim_is_shared_across_private_fallbacks_and_canonical_aliases() {
    let fixture = Fixture::new();
    let path = fixture.path.join("host.rsk");
    std::fs::write(&path, b"owned").unwrap();
    let claim = fixture.endpoint.write_claim(&path).unwrap().unwrap();
    let private = fixture.endpoint.private(random_token().unwrap()).unwrap();
    assert!(
        private
            .write_claim(&fixture.path.join(".").join("host.rsk"))
            .unwrap()
            .is_none()
    );
    assert!(
        private.claim(&path).unwrap().is_some(),
        "launch and writer roles are separate"
    );
    drop(claim);
    assert!(private.write_claim(&path).unwrap().is_some());
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
    let (mut stream, peer) = listener.accept(Duration::from_secs(3)).unwrap();
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
    let (mut stream, _) = listener.accept(Duration::from_secs(2)).unwrap();
    let (_silent, _peer) = silent.join().unwrap();
    let before = Instant::now();
    assert!(stream.read_exact(&mut [0; 1]).is_err());
    assert!(before.elapsed() < Duration::from_secs(4));
    drop(stream);
    drop(_silent);
    let client = fixture.endpoint.clone();
    let next = thread::spawn(move || client.connect(Duration::from_secs(2)).unwrap());
    assert!(listener.accept(Duration::from_secs(2)).is_ok());
    assert!(next.join().is_ok());
}
