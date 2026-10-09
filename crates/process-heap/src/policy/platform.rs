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
    let info = fs::read_to_string("/proc/meminfo").ok()?;
    let available = kib_field(&info, "MemAvailable:").or_else(|| kib_field(&info, "MemFree:"));
    let process = fs::read_to_string("/proc/self/limits")
        .ok()
        .and_then(|limits| {
            let limit = limits
                .lines()
                .find_map(|line| line.strip_prefix("Max address space"))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?;
            let used = fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| kib_field(&s, "VmSize:"))?;
            Some(limit.saturating_sub(used))
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
        let current = read(limit_name)
            .zip(read(used_name))
            .map(|(limit, used)| limit.saturating_sub(used));
        headroom = minimum([headroom, current]);
        if directory == base || !directory.pop() {
            break;
        }
    }
    headroom
}

#[cfg(target_os = "macos")]
pub(super) fn memory_headroom() -> Option<u64> {
    let total = command("/usr/sbin/sysctl", &["-n", "hw.memsize"])
        .and_then(|s| s.trim().parse::<u64>().ok());
    let available = command("/usr/bin/vm_stat", &[]).and_then(|s| mach_available(&s));
    minimum([
        Some(available?),
        total.map(|bytes| bytes / 2),
        process_headroom(),
    ])
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
    let resident = command("/bin/ps", &["-o", "rss=", "-p", &pid])?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)?;
    Some(limit.saturating_sub(resident))
}

#[cfg(windows)]
pub(super) fn memory_headroom() -> Option<u64> {
    command(
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory",
        ],
    )
    .and_then(|s| s.trim().parse::<u64>().ok())
    .and_then(|kib| kib.checked_mul(1024))
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
fn mach_available(input: &str) -> Option<u64> {
    let page = input
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    let mut pages = 0u64;
    for field in ["Pages free:", "Pages inactive:", "Pages speculative:"] {
        let number = input
            .lines()
            .find_map(|line| line.strip_prefix(field))?
            .trim()
            .trim_end_matches('.')
            .parse::<u64>()
            .ok()?;
        pages = pages.checked_add(number)?;
    }
    pages.checked_mul(page)
}

#[cfg(any(target_os = "macos", windows))]
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
        let mach = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 10.\nPages inactive: 20.\nPages speculative: 3.\n";
        assert_eq!(mach_available(mach), Some(33 * 16384));
        assert_eq!(mach_available("missing"), None);
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
        assert_eq!(
            cgroup_headroom(&root, "/../escape", "memory.max", "memory.current"),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
