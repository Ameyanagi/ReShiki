use super::Limits;
use std::{
    io::{self, Read, Write},
    os::unix::{io::AsRawFd, process::CommandExt},
    process::{Command, ExitStatus, Stdio},
};

pub(super) fn spawn(command: &mut Command, limits: Limits) -> io::Result<Child> {
    command.process_group(0);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Capture the expected editor PID before fork. On Linux PDEATHSIG is tied
    // to the spawning thread, which stays alive throughout this synchronous
    // supervision. The post-registration check covers early parent exit.
    let parent = std::process::id() as libc::pid_t;
    #[cfg(target_os = "macos")]
    let max_fd = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    #[cfg(target_os = "macos")]
    if max_fd < 0 || max_fd > i32::MAX as libc::c_long {
        return Err(io::Error::other(
            "Could not bound lifetime-observer descriptor cleanup",
        ));
    }
    // SAFETY: after fork this closure uses only fixed data and libc kernel/FD
    // operations, with no allocation or locks on its successful path. The
    // macOS observer never returns to Rust runtime code after its second fork.
    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::_exit(75);
                }
                let limit = libc::rlimit {
                    rlim_cur: limits.address_bytes as libc::rlim_t,
                    rlim_max: limits.address_bytes as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            for (resource, value) in [
                (libc::RLIMIT_CPU, limits.cpu_seconds),
                (libc::RLIMIT_FSIZE, limits.file_bytes),
                (libc::RLIMIT_CORE, 0),
            ] {
                let limit = libc::rlimit {
                    rlim_cur: value as libc::rlim_t,
                    rlim_max: value as libc::rlim_t,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            #[cfg(target_os = "macos")]
            install_lifetime_observer(parent, max_fd as i32)?;
            Ok(())
        });
    }
    let raw = command.spawn()?;
    let mut child = Child {
        raw,
        limits,
        terminated: false,
        status: None,
    };
    for fd in [
        child.raw.stdin.as_ref().map(AsRawFd::as_raw_fd),
        child.raw.stdout.as_ref().map(AsRawFd::as_raw_fd),
        child.raw.stderr.as_ref().map(AsRawFd::as_raw_fd),
    ] {
        let fd = fd.ok_or_else(|| io::Error::other("Missing naming pipe"))?;
        // SAFETY: each FD is live and exclusively owned by this child wrapper.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 {
            child.terminate();
            return Err(io::Error::last_os_error());
        }
    }
    Ok(child)
}

/// A macOS observer exists before the trusted worker execs. The worker waits
/// for registration of BOTH editor-exit and worker-exit notifications, then
/// checks its expected parent again. This closes the startup/dead-parent race.
/// The observer holds no response pipe or Rust spawn-error pipe. It also has an
/// independent 20-second lifetime bound if the editor is stopped or abandoned.
#[cfg(target_os = "macos")]
unsafe fn install_lifetime_observer(parent: libc::pid_t, max_fd: i32) -> io::Result<()> {
    // SAFETY: called only in the controlled post-fork pre-exec closure; all
    // monitor operations use stack storage and direct libc calls.
    unsafe {
        if libc::getppid() != parent {
            libc::_exit(75);
        }
        let worker = libc::getpid();
        let mut ack = [0; 2];
        if libc::pipe(ack.as_mut_ptr()) != 0 {
            return Err(io::Error::last_os_error());
        }
        let observer = libc::fork();
        if observer < 0 {
            libc::close(ack[0]);
            libc::close(ack[1]);
            return Err(io::Error::last_os_error());
        }
        if observer == 0 {
            let queue = libc::kqueue();
            // The host allows over a million FD numbers. Enumerate live FDs
            // in fixed-size batches instead of issuing a million close calls
            // per request. No allocation, locks or inherited pipe survive.
            let descriptors_closed = close_observer_descriptors(queue, ack[1], max_fd);
            let mut changes = [
                libc::kevent {
                    ident: parent as usize,
                    filter: libc::EVFILT_PROC,
                    flags: libc::EV_ADD | libc::EV_ONESHOT,
                    fflags: libc::NOTE_EXIT,
                    data: 0,
                    udata: std::ptr::null_mut(),
                },
                libc::kevent {
                    ident: worker as usize,
                    filter: libc::EVFILT_PROC,
                    flags: libc::EV_ADD | libc::EV_ONESHOT,
                    fflags: libc::NOTE_EXIT,
                    data: 0,
                    udata: std::ptr::null_mut(),
                },
            ];
            if queue >= 0
                && descriptors_closed
                && libc::kevent(
                    queue,
                    changes.as_mut_ptr(),
                    2,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                ) == 0
            {
                let byte = [1_u8];
                if libc::write(ack[1], byte.as_ptr().cast(), 1) == 1 {
                    libc::close(ack[1]);
                    let timeout = libc::timespec {
                        tv_sec: 20,
                        tv_nsec: 0,
                    };
                    let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();
                    // Any event, timeout or observation failure fails closed.
                    libc::kevent(queue, std::ptr::null(), 0, event.as_mut_ptr(), 1, &timeout);
                }
            }
            libc::kill(-worker, libc::SIGKILL);
            libc::_exit(75);
        }
        libc::close(ack[1]);
        let mut byte = [0_u8];
        let received = libc::read(ack[0], byte.as_mut_ptr().cast(), 1);
        libc::close(ack[0]);
        if received != 1 || byte[0] != 1 || libc::getppid() != parent {
            libc::kill(-worker, libc::SIGKILL);
            libc::_exit(75);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
unsafe fn close_observer_descriptors(queue: i32, ack: i32, max_fd: i32) -> bool {
    // SAFETY: only the single-threaded post-fork observer calls this. libproc
    // writes a fixed correctly sized stack buffer; no concurrent FD mutation
    // occurs. Each successful pass closes all listed non-control descriptors.
    unsafe {
        let mut descriptors = [libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0,
        }; 256];
        let entry_size = std::mem::size_of::<libc::proc_fdinfo>();
        for _ in 0..(max_fd as usize).div_ceil(256) + 1 {
            let bytes = libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDLISTFDS,
                0,
                descriptors.as_mut_ptr().cast(),
                std::mem::size_of_val(&descriptors) as i32,
            );
            if bytes <= 0
                || bytes as usize > std::mem::size_of_val(&descriptors)
                || !(bytes as usize).is_multiple_of(entry_size)
            {
                return false;
            }
            let count = bytes as usize / entry_size;
            let mut closed = 0;
            for descriptor in descriptors.iter().take(count) {
                if descriptor.proc_fd != queue && descriptor.proc_fd != ack {
                    libc::close(descriptor.proc_fd);
                    closed += 1;
                }
            }
            if closed == 0 {
                return true;
            }
        }
        false
    }
}

pub struct Child {
    raw: std::process::Child,
    limits: Limits,
    terminated: bool,
    status: Option<ExitStatus>,
}
impl Child {
    pub fn id(&self) -> u32 {
        self.raw.id()
    }
    pub fn write_input(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.raw
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("Input closed"))?
            .write(bytes)
    }
    pub fn close_input(&mut self) {
        self.raw.stdin.take();
    }
    pub fn read_output(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.raw
            .stdout
            .as_mut()
            .ok_or_else(|| io::Error::other("Output closed"))?
            .read(bytes)
    }
    pub fn read_errors(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.raw
            .stderr
            .as_mut()
            .ok_or_else(|| io::Error::other("Diagnostics closed"))?
            .read(bytes)
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.status.is_some() {
            return Ok(self.status);
        }
        // Observe without releasing the PID: a zombie still reserves both its
        // identity and the original PGID until group cleanup has completed.
        // SAFETY: zero is a valid initial siginfo_t, and waitid writes a complete
        // result. This owned child is the only reaper for its exact PID.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.id() as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ECHILD) {
                // An external reaper violates our ownership contract. Never
                // signal a remembered group after its child identity is gone.
                self.terminated = true;
            }
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(error);
        }
        // SAFETY: waitid initialized info; a zero PID means no exit was observed.
        if unsafe { info.si_pid() } != 0 {
            self.terminate();
            if self.status.is_none() {
                return Err(io::Error::other("Could not reap the owned naming worker"));
            }
        }
        Ok(self.status)
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    /// Polled leader RSS, not arbitrary descendant accounting. Trusted direct
    /// HotSpot/ReShiki workers must not daemonize or delegate parsing elsewhere.
    pub fn check(&self) -> io::Result<()> {
        if resident_bytes(self.id())? > self.limits.memory_bytes {
            return Err(io::Error::other(
                "Local parser exceeded its resident-memory limit",
            ));
        }
        Ok(())
    }
    /// Dedicated process-group cleanup. Deliberately not a hostile-process
    /// sandbox; a descendant that changes its process group is outside scope.
    pub fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        // SAFETY: the nonzero group belongs solely to this owned worker. No
        // method reaps it before this signal, so the leader PID remains reserved.
        unsafe {
            libc::kill(-(self.id() as libc::pid_t), libc::SIGKILL);
        }
        if self.status.is_none() {
            let _ = self.raw.kill();
            if let Ok(status) = self.raw.wait() {
                self.status = Some(status);
            }
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> io::Result<u64> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_taskinfo>();
    let pid = i32::try_from(pid).map_err(io::Error::other)?;
    // SAFETY: libproc receives a correctly sized writable buffer. The buffer is
    // read only when the kernel reports that it wrote the complete structure.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    if written != size as i32 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful complete write above initialized every field.
    Ok(unsafe { info.assume_init() }.pti_resident_size)
}

#[cfg(target_os = "linux")]
fn resident_bytes(pid: u32) -> io::Result<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
    linux_resident_snapshot(&status)
}

#[cfg(any(target_os = "linux", test))]
fn linux_resident_snapshot(status: &str) -> io::Result<u64> {
    let sample = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        // proc_pid_status omits task_mem when get_task_mm returns null. Exit
        // releases mm before the child becomes waitable, so an absent VmRSS
        // is a transient unavailable sample, not a measured zero or bad value.
        // The caller rechecks owned-child exit and bounds missing-sample retries.
        .ok_or_else(|| io::Error::from_raw_os_error(libc::ESRCH))?;
    let kb = sample
        .split_whitespace()
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| io::Error::other("Could not measure local parser resident memory"))?;
    kb.checked_mul(1024)
        .ok_or_else(|| io::Error::other("Invalid parser memory measurement"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn resident_bytes(_pid: u32) -> io::Result<u64> {
    Err(io::Error::other(
        "Resident-memory limits are unavailable on this Unix platform",
    ))
}

#[cfg(test)]
mod ownership_tests {
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
}
