use super::*;

#[test]
fn first_number_uses_only_the_first_prefix_match_and_first_token() {
    assert_eq!(
        first_number_after("Other: 8\nField:  12 34 bytes\nField: 56\n", "Field:"),
        Some(12)
    );
    assert_eq!(first_number_after("Other: 8\n", "Field:"), None);
    assert_eq!(first_number_after("Field:\nField: 56\n", "Field:"), None);
    assert_eq!(
        first_number_after("Field: unlimited 12\nField: 56\n", "Field:"),
        None
    );
    assert_eq!(
        first_number_after("Field: 18446744073709551615 bytes\n", "Field:"),
        Some(u64::MAX)
    );
    assert_eq!(
        first_number_after("Field: 18446744073709551616\nField: 56\n", "Field:"),
        None
    );
}

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
    let root = std::env::temp_dir().join(format!("reshiki-policy-cgroup-{}", std::process::id()));
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
