use super::*;
use std::{
    process::Stdio,
    time::{Duration, Instant},
};

fn limits() -> Limits {
    Limits {
        address_bytes: 16 * 1024 * 1024 * 1024,
        memory_bytes: 768 * 1024 * 1024,
        cpu_seconds: 20,
        file_bytes: 8 * 1024 * 1024,
    }
}
fn fixture(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "tests::child_fixture", "--nocapture"])
        .env("RESHIKI_PROCESS_TEST_CHILD", mode);
    command
}
#[cfg(windows)]
fn wait_bounded(child: &mut Child) -> std::process::ExitStatus {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if started.elapsed() > Duration::from_secs(3) {
            child.terminate();
            panic!("The bounded worker did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn invalid_resource_policy_fails_before_starting_a_child() {
    let mut command = Command::new("unused");
    let mut value = limits();
    value.memory_bytes = 0;
    assert!(spawn(&mut command, value).is_err());
}
#[test]
fn actual_termination_reaps_direct_child() {
    let mut child = spawn(&mut fixture("sleep"), limits()).unwrap();
    child.terminate();
    assert!(!child.wait().unwrap().success());
    assert!(child.try_wait().unwrap().is_some());
}
#[cfg(target_os = "macos")]
#[test]
fn observer_closes_more_than_one_descriptor_batch_and_failed_exec_pipe() {
    use std::{io::Read, os::unix::net::UnixStream};
    // These close-on-exec descriptors exist in the observer BEFORE exec. More
    // than 256 live FDs requires multiple bounded proc_pidinfo passes.
    let files: Vec<_> = (0..600)
        .map(|_| std::fs::File::open("/dev/null").unwrap())
        .collect();
    let (mut reader, writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let mut child = spawn(&mut fixture("sleep"), limits()).unwrap();
    drop(writer);
    let started = Instant::now();
    loop {
        match reader.read(&mut [0_u8; 1]) {
            Ok(0) => break,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
            result => panic!("Unexpected inherited descriptor result: {result:?}"),
        }
        assert!(started.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.try_wait().unwrap().is_none());
    child.terminate();
    let started = Instant::now();
    let mut missing = Command::new("/definitely-missing-reshiki-process-executable");
    assert_eq!(
        spawn(&mut missing, limits()).err().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    drop(files);
}
#[cfg(unix)]
#[test]
fn actual_child_resident_limit_is_observable() {
    let mut child = spawn(&mut fixture("sleep"), limits()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    child.check().unwrap();
    let mut tiny = limits();
    tiny.memory_bytes = 1;
    let mut small = spawn(&mut fixture("sleep"), tiny).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        small
            .check()
            .unwrap_err()
            .to_string()
            .contains("resident-memory")
    );
    child.terminate();
    small.terminate();
}

#[test]
fn child_fixture() {
    match std::env::var("RESHIKI_PROCESS_TEST_CHILD").as_deref() {
        Ok("sleep") => std::thread::sleep(Duration::from_secs(30)),
        Ok("eof") => {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes).unwrap();
            std::fs::write(
                std::env::var_os("RESHIKI_PROCESS_TEST_EOF").unwrap(),
                b"eof",
            )
            .unwrap();
            std::thread::sleep(Duration::from_secs(30));
        }
        #[cfg(windows)]
        Ok("chunked") => {
            use std::io::{Read, Write};
            // Each output is gated by input, making the empty connected pipe
            // interval deterministic instead of relying on a startup sleep.
            let mut signal = [0_u8; 1];
            std::io::stderr().write_all(b"ready").unwrap();
            std::io::stderr().flush().unwrap();
            std::io::stdin().read_exact(&mut signal).unwrap();
            std::io::stdout().write_all(b"first-output").unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(b"first-error").unwrap();
            std::io::stderr().flush().unwrap();
            std::io::stdin().read_exact(&mut signal).unwrap();
            std::io::stdout().write_all(b"second-output").unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(b"second-error").unwrap();
            std::io::stderr().flush().unwrap();
            // Avoid the test harness adding a success line to this protocol.
            std::process::exit(0);
        }
        #[cfg(windows)]
        Ok("memory") => {
            // BEFORE reading stdin: a post-spawn assignment could lose this race.
            let mut bytes = Vec::<u8>::new();
            if bytes.try_reserve_exact(128 * 1024 * 1024).is_err() {
                std::process::exit(86);
            }
        }
        #[cfg(windows)]
        Ok("spawn") => {
            // BEFORE reading stdin: the precreated single-process job must
            // refuse an attempted descendant at startup.
            let mut command = fixture("sleep");
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if command.spawn().is_err() {
                std::process::exit(86);
            }
        }
        _ => (),
    }
}

#[cfg(windows)]
#[test]
fn actual_child_output_survives_empty_intervals_between_gated_chunks() {
    fn chunk(child: &mut Child, errors: bool, expected: &[u8]) -> Vec<u8> {
        let started = Instant::now();
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 128];
        while captured.len() < expected.len() {
            let result = if errors {
                child.read_errors(&mut buffer)
            } else {
                child.read_output(&mut buffer)
            };
            match result {
                Ok(0) => panic!("A connected child pipe was reported as EOF"),
                Ok(count) => captured.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                Err(error) => panic!("Could not read child response: {error}"),
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured, expected);
        captured
    }
    let mut child = spawn(&mut fixture("chunked"), limits()).unwrap();
    chunk(&mut child, true, b"ready");
    // The harness's startup line precedes the fixture's ready signal. Drain
    // it, then the child cannot produce stdout until we explicitly signal it.
    let mut buffer = [0_u8; 128];
    let started = Instant::now();
    loop {
        match child.read_output(&mut buffer) {
            Ok(0) => panic!("The live child stdout must remain connected"),
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("Could not drain fixture startup: {error}"),
        }
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    assert_eq!(
        child.read_output(&mut buffer).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(child.write_input(b"1").unwrap(), 1);
    let mut output = chunk(&mut child, false, b"first-output");
    let mut errors = chunk(&mut child, true, b"first-error");
    assert_eq!(
        child.read_output(&mut buffer).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        child.read_errors(&mut buffer).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(child.write_input(b"2").unwrap(), 1);
    child.close_input();
    let mut output_closed = false;
    let mut errors_closed = false;
    let started = Instant::now();
    let status = loop {
        for (closed, captured, is_error) in [
            (&mut output_closed, &mut output, false),
            (&mut errors_closed, &mut errors, true),
        ] {
            if *closed {
                continue;
            }
            let result = if is_error {
                child.read_errors(&mut buffer)
            } else {
                child.read_output(&mut buffer)
            };
            match result {
                Ok(0) => *closed = true,
                Ok(count) => captured.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                Err(error) => panic!("Could not read final child response: {error}"),
            }
        }
        if let Some(status) = child.try_wait().unwrap()
            && output_closed
            && errors_closed
        {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success());
    assert_eq!(output, b"first-outputsecond-output");
    assert_eq!(errors, b"first-errorsecond-error");
    child.terminate();
}

#[cfg(windows)]
#[test]
fn actual_job_commit_limit_applies_before_any_input_is_read() {
    let mut policy = limits();
    policy.memory_bytes = 32 * 1024 * 1024;
    let mut child = spawn(&mut fixture("memory"), policy).unwrap();
    let status = wait_bounded(&mut child);
    assert_eq!(
        status.code(),
        Some(86),
        "128 MiB must be refused by the precreated 32 MiB job"
    );
}
#[cfg(windows)]
#[test]
fn actual_job_process_limit_applies_before_any_input_is_read() {
    let mut child = spawn(&mut fixture("spawn"), limits()).unwrap();
    assert_eq!(wait_bounded(&mut child).code(), Some(86));
}

/// Separate test process stands in for the editor. Killing it cannot execute
/// any Rust Drop cleanup, so the worker must have independent lifetime control.
#[test]
fn supervisor_fixture() {
    let Some(pid_file) = std::env::var_os("RESHIKI_PROCESS_TEST_SUPERVISOR") else {
        return;
    };
    let mode = std::env::var("RESHIKI_PROCESS_TEST_MODE").unwrap();
    let mut command = fixture(if mode == "eof" { "eof" } else { "sleep" });
    if let Some(path) = std::env::var_os("RESHIKI_PROCESS_TEST_EOF") {
        command.env("RESHIKI_PROCESS_TEST_EOF", path);
    }
    let mut child = spawn(&mut command, limits()).unwrap();
    std::fs::write(pid_file, child.id().to_string()).unwrap();
    if mode == "eof" {
        child.close_input();
    }
    std::thread::sleep(Duration::from_secs(30));
    child.terminate();
}
fn child_has_exited(pid: u32) -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        // SAFETY: read-only observation of the separately recorded child;
        // the complete initialized buffer is read only after an exact write.
        let written = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size as i32,
            )
        };
        if written == size as i32 {
            return unsafe { info.assume_init() }.pbi_status == libc::SZOMB;
        }
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
        true
    }
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => {
                stat.rsplit_once(')')
                    .and_then(|(_, tail)| tail.split_whitespace().next())
                    == Some("Z")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => panic!("Could not independently observe worker lifetime: {error}"),
        }
    }
    #[cfg(windows)]
    {
        use ::windows::Win32::{
            Foundation::WAIT_OBJECT_0,
            System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
        };
        // SAFETY: read-only lifetime observation; the owned handle is closed.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) };
        let Ok(process) = process else {
            return true;
        };
        let owned = windows::owned(process).unwrap();
        (unsafe { WaitForSingleObject(windows::handle(&owned), 0) }) == WAIT_OBJECT_0
    }
}
#[test]
fn killing_supervisor_at_startup_and_after_eof_ends_worker_independently() {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    for mode in ["startup", "eof"] {
        let root = std::env::temp_dir().join(format!(
            "reshiki-process-parent-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let pid_file = root.join("pid");
        let eof_file = root.join("eof");
        let mut supervisor = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::supervisor_fixture", "--nocapture"])
            .env("RESHIKI_PROCESS_TEST_SUPERVISOR", &pid_file)
            .env("RESHIKI_PROCESS_TEST_MODE", mode)
            .env("RESHIKI_PROCESS_TEST_EOF", &eof_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !pid_file.exists() || (mode == "eof" && !eof_file.exists()) {
            if started.elapsed() > Duration::from_secs(3) {
                let _ = supervisor.kill();
                let _ = supervisor.wait();
                panic!("Supervisor fixture failed to reach {mode}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let pid: u32 = std::fs::read_to_string(&pid_file).unwrap().parse().unwrap();
        supervisor.kill().unwrap();
        supervisor.wait().unwrap();
        let started = Instant::now();
        while !child_has_exited(pid) {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "Worker {pid} survived its killed supervisor ({mode})"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
