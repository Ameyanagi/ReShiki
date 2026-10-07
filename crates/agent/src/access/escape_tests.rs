//! The filesystem escape suite (gate G3): adversarial layouts around one
//! granted root, run in the rust CI job on macOS, Windows and Linux.
//!
//! Every case keeps an `outside/secret.mol` sentinel beside the root and
//! shows that no request returns its bytes or changes the outside folder.
//! Race and FIFO cases are stress tests: they show that no escape happens
//! under load, not that none can.
//!
//! The crate forbids unsafe code, so links, FIFOs and privilege checks use
//! std APIs or bounded subprocesses, never libc calls.
use super::{AccessError, Grants, WriteReceipt};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

const FORMATS: &[&str] = &["rsk", "mol"];
const LIMIT: usize = 4096;
/// The outside file's bytes: no granted request may ever return them.
const SENTINEL: &[u8] = b"RESHIKI ESCAPE SENTINEL: this file is outside the granted root";
/// The bytes of files inside the root.
const INSIDE: &[u8] = b"inside the granted root";
/// The [`read_outcome`] of a read that returned [`INSIDE`].
const INSIDE_READ: &str = "ok (inside bytes)";

/// A canonical temporary folder holding `root/` and `outside/secret.mol`.
struct Sandbox {
    _dir: tempfile::TempDir,
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base = super::root::normalize(fs::canonicalize(dir.path()).unwrap()).unwrap();
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("secret.mol"), SENTINEL).unwrap();
        Self {
            _dir: dir,
            base,
            root,
            outside,
        }
    }

    fn secret(&self) -> PathBuf {
        self.outside.join("secret.mol")
    }

    /// The root, granted for both reading and writing.
    fn grants(&self) -> Grants {
        Grants::open(
            std::slice::from_ref(&self.root),
            std::slice::from_ref(&self.root),
        )
        .unwrap()
    }

    /// The fully qualified request for `relative` (`/`-separated) under the root.
    fn path(&self, relative: &str) -> String {
        text(
            &relative
                .split('/')
                .fold(self.root.clone(), |path, part| path.join(part)),
        )
    }

    /// The sentinel keeps its bytes and nothing else appeared beside it.
    fn assert_outside_untouched(&self) {
        assert_eq!(fs::read(self.secret()).unwrap(), SENTINEL);
        let names: Vec<_> = fs::read_dir(&self.outside)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["secret.mol"], "the outside folder changed");
    }
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// Temporary files a write left in `dir`.
fn temps(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".reshiki-") && name.ends_with(".tmp"))
        .collect()
}

fn assert_never_sentinel(bytes: &[u8]) {
    assert!(
        !bytes
            .windows(SENTINEL.len())
            .any(|window| window == SENTINEL),
        "a granted read returned the outside sentinel"
    );
}

/// A read's outcome for the table; fails on sentinel bytes.
fn read_outcome(result: &Result<Vec<u8>, AccessError>) -> String {
    match result {
        Ok(bytes) => {
            assert_never_sentinel(bytes);
            if bytes == INSIDE {
                INSIDE_READ.into()
            } else {
                format!("ok ({} bytes)", bytes.len())
            }
        }
        Err(error) => error.code().into(),
    }
}

fn write_outcome(result: &Result<WriteReceipt, AccessError>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(error) => error.code().into(),
    }
}

/// Print a two-column outcome table; shown with `--nocapture`.
fn print_outcomes(header: [&str; 2], title: &str, rows: &[(String, String)]) {
    let [left, right] = header;
    let width = rows
        .iter()
        .map(|(first, _)| first.chars().count())
        .chain([left.chars().count()])
        .max()
        .unwrap_or(0);
    println!("escape suite: {title}");
    println!("  {left:<width$}  {right}");
    for (first, second) in rows {
        println!("  {first:<width$}  {second}");
    }
}

/// Run `f` on its own thread and fail if it takes longer than `limit`. A
/// hung call stays parked on its thread; the test still fails promptly.
fn run_with_timeout<T, F>(limit: Duration, f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(f());
    });
    match receiver.recv_timeout(limit) {
        Ok(value) => value,
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("the call did not finish within {limit:?}"),
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("the call panicked"),
    }
}

/// Run a short helper command, killing it if it outlives `limit`.
fn run_bounded(command: &mut Command, limit: Duration) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot run {command:?}: {error}"));
    let deadline = Instant::now() + limit;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{command:?} did not finish within {limit:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

/// Create a FIFO with the POSIX `mkfifo` tool, under a 5 s watchdog.
#[cfg(unix)]
fn make_fifo(path: &Path) {
    let output = run_bounded(Command::new("mkfifo").arg(path), Duration::from_secs(5));
    assert!(output.status.success(), "mkfifo failed: {output:?}");
}

/// Whether permission bits do not bind this process (root, or a similar
/// capability): a chmod-000 file in `dir` can still be read.
#[cfg(unix)]
fn running_privileged(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    let probe = dir.join("privilege-probe");
    fs::write(&probe, b"probe").unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o000)).unwrap();
    let privileged = fs::read(&probe).is_ok();
    fs::remove_file(&probe).unwrap();
    privileged
}
