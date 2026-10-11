//! Conservative memory headroom probes. Failed/unknown observations never imply
//! unlimited memory. Linux container and process ceilings constrain host RAM.
#[cfg(target_os = "linux")]
use std::fs;

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn minimum(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    values.into_iter().flatten().min()
}

#[cfg(target_os = "linux")]
pub(super) fn memory_headroom() -> Option<u64> {
    let available = fs::read_to_string("/proc/meminfo").ok().and_then(|info| {
        kib_field(&info, "MemAvailable:").or_else(|| kib_field(&info, "MemFree:"))
    });
    let process = fs::read_to_string("/proc/self/limits")
        .ok()
        .and_then(|limits| {
            let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
            process_limits_headroom(&limits, &status)
        });
    let mut container = Vec::new();
    if let Ok(groups) = fs::read_to_string("/proc/self/cgroup") {
        for line in groups.lines() {
            let fields: Vec<_> = line.splitn(3, ':').collect();
            if let [_, controllers, path] = fields.as_slice() {
                let (base, limit_name, used_name) = if controllers.is_empty() {
                    ("/sys/fs/cgroup", "memory.max", "memory.current")
                } else if controllers.split(',').any(|c| c == "memory") {
                    (
                        "/sys/fs/cgroup/memory",
                        "memory.limit_in_bytes",
                        "memory.usage_in_bytes",
                    )
                } else {
                    continue;
                };
                container.push(cgroup_headroom(
                    std::path::Path::new(base),
                    path,
                    limit_name,
                    used_name,
                ));
            }
        }
    }
    minimum([available, process].into_iter().chain(container))
}

#[cfg(any(target_os = "linux", test))]
fn process_limits_headroom(limits: &str, status: &str) -> Option<u64> {
    minimum(
        [
            ("Max address space", "VmSize:"),
            ("Max data size", "VmData:"),
        ]
        .into_iter()
        .map(|(name, usage)| {
            let limit = limits
                .lines()
                .find_map(|line| line.strip_prefix(name))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?;
            Some(observed_limit_headroom(limit, kib_field(status, usage)))
        }),
    )
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn observed_limit_headroom(limit: u64, used: Option<u64>) -> u64 {
    // A known finite constraint must not turn into an unconstrained fallback
    // when its usage probe fails. Unknown usage conservatively denies work.
    limit.saturating_sub(used.unwrap_or(limit))
}

#[cfg(any(target_os = "linux", test))]
fn cgroup_headroom(
    base: &std::path::Path,
    path: &str,
    limit_name: &str,
    used_name: &str,
) -> Option<u64> {
    if path.split('/').any(|part| part == "..") {
        return None;
    }
    let mut directory = base.join(path.trim_start_matches('/'));
    let mut headroom = None;
    // Include the mount root even when a cgroup namespace presents a different
    // relative leaf path. Every readable ancestor can impose a tighter limit.
    loop {
        let read = |name| {
            std::fs::read_to_string(directory.join(name))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
        };
        let current = read(limit_name).map(|limit| observed_limit_headroom(limit, read(used_name)));
        headroom = minimum([headroom, current]);
        if directory == base || !directory.pop() {
            break;
        }
    }
    headroom
}

#[cfg(target_os = "macos")]
pub(super) fn memory_headroom() -> Option<u64> {
    let (total, available) = mach_memory();
    let host = available.map(|bytes| bytes.min(total.map_or(bytes, |total| total / 2)));
    // A failed host probe must not discard an observed process ceiling.
    minimum([host, process_headroom()])
}

#[cfg(target_os = "macos")]
fn mach_memory() -> (Option<u64>, Option<u64>) {
    use std::ffi::{c_char, c_void};
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old: *mut c_void,
            length: *mut usize,
            new: *mut c_void,
            new_length: usize,
        ) -> i32;
        fn mach_host_self() -> u32;
        static mach_task_self_: u32;
        fn mach_port_deallocate(task: u32, port: u32) -> i32;
        fn host_page_size(host: u32, page: *mut usize) -> i32;
        fn host_statistics(host: u32, flavor: i32, values: *mut i32, count: *mut u32) -> i32;
    }
    struct Host(u32);
    impl Drop for Host {
        fn drop(&mut self) {
            // SAFETY: mach_host_self returns a send right owned by this scope.
            // mach_task_self_ is libSystem's initialized current-task port.
            unsafe {
                mach_port_deallocate(mach_task_self_, self.0);
            }
        }
    }
    let mut total = 0u64;
    let mut bytes = std::mem::size_of_val(&total);
    // SAFETY: the static C string and writable u64 buffer remain valid for this
    // call. Null new-data arguments make this a read-only sysctl observation.
    let total = (unsafe {
        sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut total as *mut u64).cast(),
            &mut bytes,
            std::ptr::null_mut(),
            0,
        )
    } == 0
        && bytes == std::mem::size_of::<u64>())
    .then_some(total);
    // SAFETY: mach_host_self has no pointer arguments and returns a send right.
    let host = Host(unsafe { mach_host_self() });
    if host.0 == 0 {
        return (total, None);
    }
    let mut page = 0usize;
    // HOST_VM_INFO's legacy layout is fixed at 15 natural_t words. Only the
    // free/inactive page fields are read; free already includes speculative
    // pages. Layout, flavor and ownership checked against the Darwin SDK's
    // mach/{vm_statistics.h,host_info.h,mach_host.h,mach_init.h}.
    let mut values = [0u32; 15];
    let mut count = values.len() as u32;
    // SAFETY: page and the complete aligned 15-word buffer are writable for
    // these synchronous calls. The APIs neither retain nor free their pointers.
    let ok = unsafe {
        host_page_size(host.0, &mut page) == 0
            && host_statistics(host.0, 2, values.as_mut_ptr().cast(), &mut count) == 0
    };
    let available = if ok && count == 15 && page > 0 {
        available_pages(values[0], values[2], page as u64)
    } else {
        None
    };
    (total, available)
}

#[cfg(target_os = "macos")]
fn process_headroom() -> Option<u64> {
    #[repr(C)]
    struct Limit {
        current: u64,
        maximum: u64,
    }
    unsafe extern "C" {
        fn getrlimit(resource: i32, limit: *mut Limit) -> i32;
    }
    let limit = [2, 5]
        .into_iter()
        .filter_map(|resource| {
            let mut limit = Limit {
                current: 0,
                maximum: 0,
            };
            // SAFETY: Darwin rlimit has two u64 fields. The stack object is writable
            // for the complete call; getrlimit neither retains nor frees its pointer.
            let result = unsafe { getrlimit(resource, &mut limit) };
            (result == 0 && limit.current < (1u64 << 60)).then_some(limit.current)
        })
        .min()?;
    let pid = std::process::id().to_string();
    if limit == 0 {
        return Some(0);
    }
    let resident = command("/bin/ps", &["-o", "rss=", "-p", &pid])
        .and_then(|output| output.trim().parse::<u64>().ok())
        .and_then(|kib| kib.checked_mul(1024));
    Some(observed_limit_headroom(limit, resident))
}

#[cfg(windows)]
pub(super) fn memory_headroom() -> Option<u64> {
    // Native observation includes physical RAM, current-process commit headroom
    // and virtual address headroom without starting an interpreter.
    // https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/ns-sysinfoapi-memorystatusex
    #[repr(C)]
    #[derive(Default)]
    struct MemoryStatus {
        length: u32,
        load: u32,
        total_physical: u64,
        available_physical: u64,
        total_commit: u64,
        available_commit: u64,
        total_virtual: u64,
        available_virtual: u64,
        extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
    }
    let mut status = MemoryStatus {
        length: std::mem::size_of::<MemoryStatus>() as u32,
        ..Default::default()
    };
    // SAFETY: repr(C) exactly matches MEMORYSTATUSEX. The initialized length
    // describes the writable stack buffer; the API retains no pointer.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then(|| {
        status
            .available_physical
            .min(status.available_commit)
            .min(status.available_virtual)
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn memory_headroom() -> Option<u64> {
    None
}

#[cfg(any(target_os = "linux", test))]
fn kib_field(input: &str, name: &str) -> Option<u64> {
    input
        .lines()
        .find_map(|line| line.strip_prefix(name))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

#[cfg(any(target_os = "macos", test))]
fn available_pages(free: u32, inactive: u32, page: u64) -> Option<u64> {
    (u64::from(free) + u64::from(inactive)).checked_mul(page)
}

#[cfg(target_os = "macos")]
fn command(program: &str, args: &[&str]) -> Option<String> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        };
        if let Some(status) = status {
            if !status.success() {
                return None;
            }
            let mut output = String::new();
            child
                .stdout
                .take()?
                .take(16 * 1024)
                .read_to_string(&mut output)
                .ok()?;
            return Some(output);
        }
        if start.elapsed() > Duration::from_millis(250) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_samples_are_checked_and_constrained_by_the_smallest_ceiling() {
        assert_eq!(
            kib_field("MemAvailable:  1024 kB\n", "MemAvailable:"),
            Some(1024 * 1024)
        );
        assert_eq!(
            kib_field("MemAvailable: 18446744073709551615 kB", "MemAvailable:"),
            None
        );
        assert_eq!(minimum([Some(32_000), Some(512), None]), Some(512));
        assert_eq!(available_pages(13, 20, 16384), Some(33 * 16384));
        assert_eq!(available_pages(u32::MAX, u32::MAX, u64::MAX), None);
    }

    #[test]
    fn finite_process_data_and_address_space_limits_are_both_observed() {
        let status = "VmSize:  8 kB\nVmData:  2 kB\n";
        let limits = "Max address space 16384 16384 bytes\nMax data size 4096 4096 bytes\n";
        assert_eq!(process_limits_headroom(limits, status), Some(2048));
        let limits = "Max address space unlimited unlimited bytes\nMax data size 4096 4096 bytes\n";
        assert_eq!(process_limits_headroom(limits, status), Some(2048));
        assert_eq!(process_limits_headroom(limits, "VmData:  5 kB\n"), Some(0));
        assert_eq!(process_limits_headroom(limits, ""), Some(0));
        assert_eq!(observed_limit_headroom(0, None), 0);
        assert_eq!(observed_limit_headroom(4096, None), 0);
        assert_eq!(observed_limit_headroom(4096, Some(1024)), 3072);
    }

    #[test]
    fn nested_cgroup_memory_and_exhausted_ancestors_constrain_host_headroom() {
        let root =
            std::env::temp_dir().join(format!("reshiki-policy-cgroup-{}", std::process::id()));
        let leaf = root.join("parent/leaf");
        std::fs::create_dir_all(&leaf).unwrap();
        for (directory, limit, used) in [
            (&root, "1000", "200"),
            (&root.join("parent"), "600", "150"),
            (&leaf, "10000", "9500"),
        ] {
            std::fs::write(directory.join("memory.max"), limit).unwrap();
            std::fs::write(directory.join("memory.current"), used).unwrap();
        }
        assert_eq!(
            cgroup_headroom(&root, "/parent/leaf", "memory.max", "memory.current"),
            Some(450)
        );
        std::fs::write(root.join("memory.max"), "max").unwrap();
        assert_eq!(
            cgroup_headroom(&root, "/parent/leaf", "memory.max", "memory.current"),
            Some(450)
        );
        std::fs::write(leaf.join("memory.current"), "10001").unwrap();
        assert_eq!(
            cgroup_headroom(&root, "/parent/leaf", "memory.max", "memory.current"),
            Some(0)
        );
        std::fs::remove_file(leaf.join("memory.current")).unwrap();
        assert_eq!(
            cgroup_headroom(&root, "/parent/leaf", "memory.max", "memory.current"),
            Some(0)
        );
        assert_eq!(
            cgroup_headroom(&root, "/../escape", "memory.max", "memory.current"),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
