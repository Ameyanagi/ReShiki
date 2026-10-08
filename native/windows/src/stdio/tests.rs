use super::*;
use std::{
    cell::Cell,
    process::{Command, Stdio},
};

/// One fake standard handle.
struct Fake {
    kind: Kind,
    inherit: Cell<bool>,
    clear_fails: bool,
    check_fails: bool,
}

impl Fake {
    /// An inheritable handle whose flag clears and reads back.
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            inherit: Cell::new(true),
            clear_fails: false,
            check_fails: false,
        }
    }

    fn clear_fails(mut self) -> Self {
        self.clear_fails = true;
        self
    }

    fn check_fails(mut self) -> Self {
        self.check_fails = true;
        self
    }

    fn not_inheritable(self) -> Self {
        self.inherit.set(false);
        self
    }
}

/// Standard input, output and error, in that order; None is a missing
/// handle.
struct FakeOps([Option<Fake>; 3]);

impl FakeOps {
    fn get(&self, which: StdHandle) -> Option<&Fake> {
        let [input, output, error] = &self.0;
        match which {
            StdHandle::Input => input.as_ref(),
            StdHandle::Output => output.as_ref(),
            StdHandle::Error => error.as_ref(),
        }
    }

    fn fake(&self, which: StdHandle) -> &Fake {
        self.get(which).expect("a handle std_handle returned")
    }
}

fn denied() -> io::Error {
    io::Error::from_raw_os_error(5)
}

impl HandleOps for FakeOps {
    type Raw = StdHandle;

    fn std_handle(&self, which: StdHandle) -> Option<StdHandle> {
        self.get(which).map(|_| which)
    }

    fn kind(&self, handle: StdHandle) -> Kind {
        self.fake(handle).kind
    }

    fn clear_inherit(&self, handle: StdHandle) -> io::Result<()> {
        let fake = self.fake(handle);
        if fake.clear_fails {
            return Err(denied());
        }
        fake.inherit.set(false);
        Ok(())
    }

    fn inheritable(&self, handle: StdHandle) -> io::Result<bool> {
        let fake = self.fake(handle);
        if fake.check_fails {
            return Err(denied());
        }
        Ok(fake.inherit.get())
    }
}

#[test]
fn piped_standard_handles_are_cleared_and_reported() {
    let ops = FakeOps([
        Some(Fake::new(Kind::Pipe)),
        Some(Fake::new(Kind::Pipe)),
        Some(Fake::new(Kind::Char)),
    ]);
    let protected = policy(&ops).expect("protected");
    assert_eq!(
        protected,
        Protected {
            stdin: true,
            stdout: true,
            stderr: false,
        }
    );
    for which in StdHandle::ALL {
        assert!(!ops.fake(which).inherit.get(), "{which:?}");
    }
}

#[test]
fn missing_handles_are_skipped() {
    let ops = FakeOps([None, Some(Fake::new(Kind::Pipe)), None]);
    let protected = policy(&ops).expect("protected");
    assert_eq!(
        protected,
        Protected {
            stdout: true,
            ..Protected::default()
        }
    );
}

/// A pipe that was never inheritable is protected even if clearing fails.
#[test]
fn a_pipe_that_reads_back_clear_is_protected() {
    let ops = FakeOps([
        Some(Fake::new(Kind::Pipe).not_inheritable().clear_fails()),
        None,
        None,
    ]);
    let protected = policy(&ops).expect("protected");
    assert!(protected.stdin, "{protected:?}");
}

#[test]
fn a_pipe_that_stays_inheritable_fails() {
    let ops = FakeOps([
        Some(Fake::new(Kind::Pipe)),
        Some(Fake::new(Kind::Pipe).clear_fails()),
        Some(Fake::new(Kind::Pipe)),
    ]);
    let error = policy(&ops).expect_err("stdout stays inheritable");
    assert_eq!(error.handle, StdHandle::Output);
    assert!(
        matches!(error.cause, Cause::Inheritable(Some(_))),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .starts_with("the stdout pipe stays inheritable: "),
        "{error}"
    );
}

#[test]
fn a_pipe_whose_flag_cannot_be_read_back_fails() {
    let ops = FakeOps([None, None, Some(Fake::new(Kind::Pipe).check_fails())]);
    let error = policy(&ops).expect_err("stderr unchecked");
    assert_eq!(error.handle, StdHandle::Error);
    assert!(matches!(error.cause, Cause::Unchecked(_)), "{error:?}");
    assert!(
        error
            .to_string()
            .starts_with("cannot check whether the stderr pipe is inheritable: "),
        "{error}"
    );
}

/// Consoles, files and unknown handles cannot hold a client's end of file,
/// so failing to clear or check them is ignored.
#[test]
fn failures_on_consoles_files_and_unknown_handles_are_ignored() {
    let failing = |kind| Some(Fake::new(kind).clear_fails().check_fails());
    let ops = FakeOps([
        failing(Kind::Char),
        failing(Kind::Disk),
        failing(Kind::Unknown),
    ]);
    assert_eq!(policy(&ops).expect("ignored"), Protected::default());
}

const CHILD: &str = "RESHIKI_STDIO_CHILD";
const CHILD_TEST: &str = "stdio::tests::child_protects_its_piped_standard_handles";
/// Written by the child once its handles are protected, through its own
/// stdout, which must keep working.
const CHILD_DONE: &str = "stdio child: standard pipes protected";

/// Runs [`child_protects_its_piped_standard_handles`] in a fresh test
/// process whose standard handles are inheritable pipes, so this shared
/// test process keeps its own handles as they are.
#[test]
fn piped_standard_handles_stop_being_inheritable() {
    let output = Command::new(std::env::current_exe().expect("the test executable"))
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .env(CHILD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run the child test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{:?}\n{stdout}\n{stderr}",
        output.status
    );
    // An exact filter that matches nothing also exits 0.
    assert!(stdout.contains(CHILD_DONE), "{stdout}\n{stderr}");
}

/// The child half of [`piped_standard_handles_stop_being_inheritable`];
/// it does nothing unless that test started it.
#[test]
#[ignore = "runs in a child process of piped_standard_handles_stop_being_inheritable"]
fn child_protects_its_piped_standard_handles() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    assert_eq!(standard_handle_inheritable(StdHandle::Output), Some(true));
    let protected = disinherit_standard_handles().expect("protected");
    assert_eq!(
        protected,
        Protected {
            stdin: true,
            stdout: true,
            stderr: true,
        }
    );
    for which in StdHandle::ALL {
        assert_eq!(standard_handle_inheritable(which), Some(false), "{which:?}");
    }
    println!("{CHILD_DONE}");
}
