//! Unix escape cases. Linux CI exercises cap-std's openat2 path; macOS its
//! manual, component-at-a-time resolution.
use super::*;
use crate::access::{GrantError, WriteMode, root::Root};
use std::{
    collections::BTreeMap,
    os::unix::fs::{PermissionsExt as _, symlink},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

const BOUND: Duration = Duration::from_secs(2);

fn bounded_read(grants: &Grants, path: String) -> Result<Vec<u8>, AccessError> {
    let grants = grants.clone();
    run_with_timeout(BOUND, move || grants.read(&path, LIMIT, FORMATS))
}

fn bounded_write(
    grants: &Grants,
    path: String,
    mode: WriteMode,
) -> Result<WriteReceipt, AccessError> {
    let grants = grants.clone();
    run_with_timeout(BOUND, move || {
        grants.write_atomic(&path, b"written by the escape suite", mode, FORMATS)
    })
}

fn code<T: std::fmt::Debug>(result: Result<T, AccessError>) -> &'static str {
    result.unwrap_err().code()
}

#[test]
fn a_symlinked_file_to_outside_is_refused_for_reads_and_writes() {
    let sandbox = Sandbox::new();
    symlink(sandbox.secret(), sandbox.root.join("absolute.mol")).unwrap();
    symlink("../outside/secret.mol", sandbox.root.join("relative.mol")).unwrap();
    let grants = sandbox.grants();
    for name in ["absolute.mol", "relative.mol"] {
        let path = sandbox.path(name);
        let read = grants.read(&path, LIMIT, FORMATS);
        assert_eq!(code(read), "path_escapes_root", "{name}");
        let replace = grants.write_atomic(&path, b"x", WriteMode::Replace, FORMATS);
        assert_eq!(code(replace), "not_a_regular_file", "{name}");
        let create = grants.write_atomic(&path, b"x", WriteMode::CreateNew, FORMATS);
        assert_eq!(code(create), "file_exists", "{name}");
        assert!(
            fs::symlink_metadata(sandbox.root.join(name))
                .unwrap()
                .is_symlink()
        );
    }
    assert!(temps(&sandbox.root).is_empty());
    sandbox.assert_outside_untouched();
}

#[test]
fn a_symlinked_directory_to_outside_is_refused_mid_path() {
    let sandbox = Sandbox::new();
    symlink(&sandbox.outside, sandbox.root.join("absolute")).unwrap();
    symlink("../outside", sandbox.root.join("relative")).unwrap();
    let grants = sandbox.grants();
    for link in ["absolute", "relative"] {
        let read = grants.read(&sandbox.path(&format!("{link}/secret.mol")), LIMIT, FORMATS);
        assert_eq!(code(read), "path_escapes_root", "{link}");
        for (mode, name) in [
            (WriteMode::CreateNew, "new.rsk"),
            (WriteMode::Replace, "new.rsk"),
            (WriteMode::Replace, "secret.mol"),
        ] {
            let path = sandbox.path(&format!("{link}/{name}"));
            let write = grants.write_atomic(&path, b"x", mode, FORMATS);
            assert_eq!(code(write), "path_escapes_root", "{link}/{name} {mode:?}");
        }
    }
    assert!(temps(&sandbox.root).is_empty());
    sandbox.assert_outside_untouched();
}

#[test]
fn an_absolute_symlink_back_inside_the_root_never_yields_outside_bytes() {
    let sandbox = Sandbox::new();
    let sub = sandbox.root.join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sandbox.root.join("real.mol"), INSIDE).unwrap();
    fs::write(sub.join("secret.mol"), INSIDE).unwrap();
    symlink(sandbox.root.join("real.mol"), sandbox.root.join("back.mol")).unwrap();
    symlink(&sub, sandbox.root.join("backdir")).unwrap();
    let grants = sandbox.grants();
    let mut rows = Vec::new();
    for path in ["back.mol", "backdir/secret.mol"] {
        let read = grants.read(&sandbox.path(path), LIMIT, FORMATS);
        rows.push((format!("read {path}"), read_outcome(&read)));
    }
    let write = grants.write_atomic(
        &sandbox.path("backdir/new.rsk"),
        b"x",
        WriteMode::CreateNew,
        FORMATS,
    );
    rows.push(("create backdir/new.rsk".into(), write_outcome(&write)));
    print_outcomes(
        ["case", "outcome"],
        "absolute symlinks pointing back inside the root",
        &rows,
    );
    sandbox.assert_outside_untouched();
}

#[test]
fn a_relative_symlink_staying_inside_the_root_is_allowed() {
    let sandbox = Sandbox::new();
    let sub = sandbox.root.join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sandbox.root.join("real.mol"), INSIDE).unwrap();
    fs::write(sub.join("secret.mol"), INSIDE).unwrap();
    symlink("real.mol", sandbox.root.join("alias.mol")).unwrap();
    symlink("sub", sandbox.root.join("aliasdir")).unwrap();
    symlink("../real.mol", sub.join("up.mol")).unwrap();
    let grants = sandbox.grants();
    for path in ["alias.mol", "aliasdir/secret.mol", "sub/up.mol"] {
        assert_eq!(
            grants.read(&sandbox.path(path), LIMIT, FORMATS).unwrap(),
            INSIDE,
            "{path}"
        );
    }
    grants
        .write_atomic(
            &sandbox.path("aliasdir/new.rsk"),
            b"new",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap();
    assert_eq!(fs::read(sub.join("new.rsk")).unwrap(), b"new");
    sandbox.assert_outside_untouched();
}

#[test]
fn a_symlink_loop_fails_within_two_seconds() {
    let sandbox = Sandbox::new();
    symlink("self.mol", sandbox.root.join("self.mol")).unwrap();
    symlink("b.mol", sandbox.root.join("a.mol")).unwrap();
    symlink("a.mol", sandbox.root.join("b.mol")).unwrap();
    symlink("dirloop", sandbox.root.join("dirloop")).unwrap();
    let grants = sandbox.grants();
    let mut rows = Vec::new();
    for path in ["self.mol", "a.mol", "dirloop/x.mol"] {
        let read = bounded_read(&grants, sandbox.path(path));
        rows.push((format!("read {path}"), read_outcome(&read)));
        assert!(read.is_err(), "{path}");
    }
    for (mode, path) in [
        (WriteMode::CreateNew, "self.mol"),
        (WriteMode::Replace, "self.mol"),
        (WriteMode::CreateNew, "dirloop/x.rsk"),
    ] {
        let write = bounded_write(&grants, sandbox.path(path), mode);
        rows.push((format!("{mode:?} {path}"), write_outcome(&write)));
        assert!(write.is_err(), "{path} {mode:?}");
    }
    print_outcomes(["case", "outcome"], "symlink loops", &rows);
    sandbox.assert_outside_untouched();
}

#[test]
fn a_dangling_symlink_is_never_a_create_new_target() {
    let sandbox = Sandbox::new();
    let target = sandbox.outside.join("created.rsk");
    symlink(&target, sandbox.root.join("absolute.rsk")).unwrap();
    symlink("../outside/created.rsk", sandbox.root.join("relative.rsk")).unwrap();
    let grants = sandbox.grants();
    for name in ["absolute.rsk", "relative.rsk"] {
        let path = sandbox.path(name);
        let create = grants.write_atomic(&path, b"x", WriteMode::CreateNew, FORMATS);
        assert_eq!(code(create), "file_exists", "{name}");
        let replace = grants.write_atomic(&path, b"x", WriteMode::Replace, FORMATS);
        assert_eq!(code(replace), "not_a_regular_file", "{name}");
        assert!(
            fs::symlink_metadata(sandbox.root.join(name))
                .unwrap()
                .is_symlink()
        );
    }
    assert!(!target.exists());
    assert!(temps(&sandbox.root).is_empty());
    sandbox.assert_outside_untouched();
}

#[test]
fn a_fifo_is_refused_without_blocking() {
    let sandbox = Sandbox::new();
    make_fifo(&sandbox.root.join("pipe.mol"));
    let grants = sandbox.grants();
    // Without O_NONBLOCK the open would wait for a writer forever.
    let read = bounded_read(&grants, sandbox.path("pipe.mol"));
    assert_eq!(code(read), "not_a_regular_file");
    let create = bounded_write(&grants, sandbox.path("pipe.mol"), WriteMode::CreateNew);
    assert_eq!(code(create), "file_exists");
    let replace = bounded_write(&grants, sandbox.path("pipe.mol"), WriteMode::Replace);
    assert_eq!(code(replace), "not_a_regular_file");
    assert!(temps(&sandbox.root).is_empty());
    sandbox.assert_outside_untouched();
}

#[test]
fn a_hard_link_to_outside_is_readable_as_a_documented_limitation() {
    let sandbox = Sandbox::new();
    fs::hard_link(sandbox.secret(), sandbox.root.join("linked.mol")).unwrap();
    let grants = sandbox.grants();
    // Accepted and documented under "Residual risks" in access.rs: a
    // multiply-linked file cannot be told apart from an ordinary one.
    assert_eq!(
        grants
            .read(&sandbox.path("linked.mol"), LIMIT, FORMATS)
            .unwrap(),
        SENTINEL
    );
}

#[test]
fn replacing_a_hard_link_to_outside_leaves_the_outside_bytes_unchanged() {
    let sandbox = Sandbox::new();
    fs::hard_link(sandbox.secret(), sandbox.root.join("linked.mol")).unwrap();
    let grants = sandbox.grants();
    let receipt = grants
        .write_atomic(
            &sandbox.path("linked.mol"),
            b"replacement",
            WriteMode::Replace,
            FORMATS,
        )
        .unwrap();
    assert!(receipt.replaced);
    assert_eq!(
        fs::read(sandbox.root.join("linked.mol")).unwrap(),
        b"replacement"
    );
    assert!(temps(&sandbox.root).is_empty());
    sandbox.assert_outside_untouched();
}

/// Stops the flipping thread even when an assertion fails mid-race.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// How long a race keeps reading past its iterations for the flipper to
/// make progress and for both layouts to be observed.
const RACE_GRACE: Duration = Duration::from_secs(10);

/// Flip `root/sub` between a real directory and symlinks to outside/ while
/// reading `sub/secret.mol`. The run only counts when the flipper completed a
/// cycle during the reads and the reads saw both the directory and a symlink;
/// a descheduled flipper would otherwise pass without any concurrent swap.
fn directory_swap_race(iterations: usize) {
    let sandbox = Sandbox::new();
    let sub = sandbox.root.join("sub");
    let parked_dir = sandbox.root.join("parked-dir");
    let parked_relative = sandbox.root.join("parked-relative");
    let parked_absolute = sandbox.root.join("parked-absolute");
    fs::create_dir(&parked_dir).unwrap();
    fs::write(parked_dir.join("secret.mol"), INSIDE).unwrap();
    symlink("../outside", &parked_relative).unwrap();
    symlink(&sandbox.outside, &parked_absolute).unwrap();
    let grants = sandbox.grants();
    let request = sandbox.path("sub/secret.mol");

    let stop = StopOnDrop(Arc::new(AtomicBool::new(false)));
    let cycles = Arc::new(AtomicU64::new(0));
    let (installed, first_layout) = mpsc::channel();
    let flipper = {
        let stop = Arc::clone(&stop.0);
        let cycles = Arc::clone(&cycles);
        thread::spawn(move || {
            let parked = [&parked_dir, &parked_relative, &parked_dir, &parked_absolute];
            let mut installed = Some(installed);
            while !stop.load(Ordering::Relaxed) {
                for entry in parked {
                    fs::rename(entry, &sub).unwrap();
                    if let Some(installed) = installed.take() {
                        let _ = installed.send(());
                    }
                    fs::rename(&sub, entry).unwrap();
                }
                cycles.fetch_add(1, Ordering::Relaxed);
            }
        })
    };
    first_layout
        .recv_timeout(BOUND)
        .expect("the flipper installed no layout");
    let window_start = cycles.load(Ordering::Relaxed);
    let raced = |outcomes: &BTreeMap<String, usize>| {
        cycles.load(Ordering::Relaxed) > window_start
            && outcomes.contains_key(INSIDE_READ)
            && outcomes.contains_key("path_escapes_root")
    };
    let read = |outcomes: &mut BTreeMap<String, usize>| {
        let read = grants.read(&request, LIMIT, FORMATS);
        *outcomes.entry(read_outcome(&read)).or_default() += 1;
    };
    let started = Instant::now();
    let mut outcomes = BTreeMap::new();
    for _ in 0..iterations {
        read(&mut outcomes);
    }
    let grace_end = Instant::now() + RACE_GRACE;
    while !raced(&outcomes) && !flipper.is_finished() && Instant::now() < grace_end {
        read(&mut outcomes);
    }
    let elapsed = started.elapsed();
    let window_cycles = cycles.load(Ordering::Relaxed) - window_start;
    drop(stop);
    flipper.join().unwrap();
    let reads: usize = outcomes.values().sum();
    let rows: Vec<_> = outcomes
        .iter()
        .map(|(outcome, count)| (outcome.clone(), format!("{count} reads")))
        .collect();
    print_outcomes(
        ["outcome", "count"],
        &format!(
            "{reads} reads of sub/secret.mol against {window_cycles} flip cycles in {elapsed:?}"
        ),
        &rows,
    );
    assert!(
        window_cycles > 0,
        "no flip cycle completed during the reads"
    );
    assert!(
        outcomes.contains_key(INSIDE_READ),
        "no read saw sub as the real directory"
    );
    assert!(
        outcomes.contains_key("path_escapes_root"),
        "no read saw sub as a symlink to outside"
    );
    sandbox.assert_outside_untouched();
}

#[test]
fn a_directory_swapped_for_a_symlink_mid_read_never_yields_outside_bytes() {
    directory_swap_race(2_000);
}

#[test]
#[ignore = "long race: 200,000 reads"]
fn a_directory_swapped_for_a_symlink_mid_read_never_yields_outside_bytes_long() {
    directory_swap_race(200_000);
}

#[test]
fn a_root_replaced_by_a_symlink_to_outside_while_acquiring_is_refused() {
    let sandbox = Sandbox::new();
    let swap = || {
        fs::rename(&sandbox.root, sandbox.base.join("moved")).unwrap();
        symlink(&sandbox.outside, &sandbox.root).unwrap();
    };
    // The hook runs between canonicalization and the open, so the open
    // lands on outside/; the post-open recheck refuses it and no root (and
    // so no Grants) comes back.
    let error = Root::open_with_hook(&sandbox.root, &swap).unwrap_err();
    assert_eq!(error, GrantError::Changed(sandbox.root.clone()));
    sandbox.assert_outside_untouched();
}

#[test]
fn a_root_swapped_after_opening_keeps_resolving_in_the_original_folder() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.root.join("secret.mol"), INSIDE).unwrap();
    let grants = sandbox.grants();
    let moved = sandbox.base.join("moved");
    fs::rename(&sandbox.root, &moved).unwrap();
    symlink(&sandbox.outside, &sandbox.root).unwrap();

    assert_eq!(
        grants
            .read(&sandbox.path("secret.mol"), LIMIT, FORMATS)
            .unwrap(),
        INSIDE
    );
    grants
        .write_atomic(
            &sandbox.path("new.rsk"),
            b"new",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap();
    grants
        .write_atomic(
            &sandbox.path("secret.mol"),
            b"replaced",
            WriteMode::Replace,
            FORMATS,
        )
        .unwrap();
    assert_eq!(fs::read(moved.join("new.rsk")).unwrap(), b"new");
    assert_eq!(fs::read(moved.join("secret.mol")).unwrap(), b"replaced");
    sandbox.assert_outside_untouched();
}

#[test]
fn a_chmod_000_file_is_os_denied() {
    let sandbox = Sandbox::new();
    if running_privileged(&sandbox.base) {
        println!("skipped: permission bits do not bind this process (running privileged)");
        return;
    }
    let locked = sandbox.root.join("locked.mol");
    fs::write(&locked, INSIDE).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let read = sandbox
        .grants()
        .read(&sandbox.path("locked.mol"), LIMIT, FORMATS);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(code(read), "os_denied");
}

#[test]
fn a_read_only_parent_fails_the_write_without_a_temporary_file() {
    let sandbox = Sandbox::new();
    if running_privileged(&sandbox.base) {
        println!("skipped: permission bits do not bind this process (running privileged)");
        return;
    }
    let parent = sandbox.root.join("readonly");
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("existing.rsk"), b"old").unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();
    let grants = sandbox.grants();
    let results = [
        (WriteMode::CreateNew, "new.rsk"),
        (WriteMode::Replace, "new.rsk"),
        (WriteMode::Replace, "existing.rsk"),
    ]
    .map(|(mode, name)| {
        let path = sandbox.path(&format!("readonly/{name}"));
        (mode, name, grants.write_atomic(&path, b"x", mode, FORMATS))
    });
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    for (mode, name, result) in results {
        assert_eq!(code(result), "os_denied", "{name} {mode:?}");
    }
    assert!(temps(&parent).is_empty());
    assert!(!parent.join("new.rsk").exists());
    assert_eq!(fs::read(parent.join("existing.rsk")).unwrap(), b"old");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_unicode_and_case_variants_of_a_root_fail_closed() {
    use std::os::unix::fs::MetadataExt as _;
    let sandbox = Sandbox::new();
    // Every spelling is built from fixture-owned names, so neither the temp
    // folder's location nor the volume's case sensitivity matters.
    let granted = sandbox.base.join("Granted").join("caf\u{e9}");
    fs::create_dir_all(&granted).unwrap();
    fs::write(granted.join("a.mol"), INSIDE).unwrap();
    let grants = Grants::open(
        std::slice::from_ref(&granted),
        std::slice::from_ref(&granted),
    )
    .unwrap();
    assert_eq!(
        grants
            .read(&text(&granted.join("a.mol")), LIMIT, FORMATS)
            .unwrap(),
        INSIDE
    );
    let identity = |path: &Path| {
        fs::metadata(path)
            .ok()
            .map(|found| (found.dev(), found.ino()))
    };
    let folder = identity(&granted).unwrap();
    // Matching is exact, so each spelling fails closed, whether or not this
    // volume resolves it to the granted folder (APFS is normalization-
    // insensitive and case-insensitive unless formatted case-sensitive).
    let mut rows = Vec::new();
    for (case, variant) in [
        ("NFD name", sandbox.base.join("Granted").join("cafe\u{301}")),
        (
            "case-variant name",
            sandbox.base.join("Granted").join("CAF\u{c9}"),
        ),
        (
            "case-variant prefix",
            sandbox.base.join("GRANTED").join("caf\u{e9}"),
        ),
    ] {
        let read = grants.read(&text(&variant.join("a.mol")), LIMIT, FORMATS);
        assert_eq!(code(read), "path_not_granted", "{case}");
        let write = grants.write_atomic(
            &text(&variant.join("b.rsk")),
            b"x",
            WriteMode::CreateNew,
            FORMATS,
        );
        assert_eq!(code(write), "path_not_granted", "{case}");
        let volume = if identity(&variant) == Some(folder) {
            "an alias of the granted folder"
        } else {
            "not an alias on this volume; only the lexical refusal ran"
        };
        rows.push((case.to_owned(), volume.to_owned()));
    }
    print_outcomes(
        ["spelling", "on this volume"],
        "macOS root spellings",
        &rows,
    );
    assert!(!granted.join("b.rsk").exists());
    sandbox.assert_outside_untouched();
}
