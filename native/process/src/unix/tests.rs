use super::*;
#[test]
fn linux_resident_snapshot_distinguishes_no_mm_from_invalid_values() {
    assert_eq!(
        linux_resident_snapshot("State:\tS (sleeping)\nVmRSS:\t2048 kB\n").unwrap(),
        2 * 1024 * 1024
    );
    // mm is released before EXIT_ZOMBIE, so neither state is required for
    // the kernel's absent-field indication to receive bounded exit grace.
    for snapshot in ["State:\tR (running)\n", "State:\tZ (zombie)\n"] {
        assert_eq!(
            linux_resident_snapshot(snapshot)
                .unwrap_err()
                .raw_os_error(),
            Some(libc::ESRCH)
        );
    }
    for snapshot in [
        "VmRSS:\t\n",
        "VmRSS:\tnot-a-number kB\n",
        "VmRSS:\t18446744073709551616 kB\n",
        "VmRSS:\t18446744073709551615 kB\n",
    ] {
        assert_eq!(
            linux_resident_snapshot(snapshot)
                .unwrap_err()
                .raw_os_error(),
            None,
            "A present invalid value must remain a hard measurement error"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_unreaped_exit_is_a_transient_sample_before_owned_cleanup() {
    let limits = Limits {
        address_bytes: 16 * 1024 * 1024 * 1024,
        memory_bytes: 768 * 1024 * 1024,
        cpu_seconds: 20,
        file_bytes: 8 * 1024 * 1024,
    };
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 37"]);
    let mut child = spawn(&mut command, limits).unwrap();
    let started = std::time::Instant::now();
    loop {
        // SAFETY: this observes the owned child without reaping it. Its
        // PID and original group remain reserved until Child::try_wait.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.id() as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            },
            0
        );
        // SAFETY: successful waitid initialized the siginfo buffer.
        if unsafe { info.si_pid() } != 0 {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let snapshot = std::fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap();
    assert!(snapshot.lines().any(|line| line.starts_with("State:\tZ")));
    assert!(!snapshot.lines().any(|line| line.starts_with("VmRSS:")));
    assert_eq!(child.check().unwrap_err().raw_os_error(), Some(libc::ESRCH));
    let status = child.try_wait().unwrap().unwrap();
    assert_eq!(status.code(), Some(37));
    assert!(child.terminated);
    child.terminate();
    assert_eq!(child.try_wait().unwrap(), Some(status));
}

#[test]
fn observed_exit_completes_group_cleanup_before_releasing_child_identity() {
    let limits = Limits {
        address_bytes: 16 * 1024 * 1024 * 1024,
        memory_bytes: 768 * 1024 * 1024,
        cpu_seconds: 20,
        file_bytes: 8 * 1024 * 1024,
    };
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 37"]);
    let mut child = spawn(&mut command, limits).unwrap();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert_eq!(status.code(), Some(37));
    assert!(
        child.terminated,
        "Returning a reaped status must permanently disable group signaling"
    );
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // SAFETY: read-only observation of this exact, now-reaped test child.
    assert_eq!(
        unsafe {
            libc::waitid(
                libc::P_PID,
                child.id() as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        },
        -1
    );
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    child.terminate();
    assert_eq!(child.try_wait().unwrap(), Some(status));
}
