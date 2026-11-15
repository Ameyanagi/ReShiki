use crate::github::{
    Repo, RepoKey,
    api::Download,
    workcopy::{self, FileState, RepoState},
};
use std::path::Path;

fn key() -> RepoKey {
    RepoKey {
        owner: "octo-test".into(),
        repo: "lab-notes".into(),
    }
}

fn state() -> RepoState {
    RepoState::new(&Repo {
        id: 7,
        key: key(),
        private: true,
        default_branch: "main".into(),
        push: true,
    })
}

fn download(bytes: &[u8]) -> Download {
    Download {
        commit: "c1".into(),
        blob: "b1".into(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn a_working_copy_knows_its_repository_and_path() {
    let root = Path::new("/data/github");
    let path = workcopy::working_path(root, &key(), "scheme/route.rsk");
    assert_eq!(
        path,
        root.join("octo-test/lab-notes/files/scheme/route.rsk")
    );
    assert_eq!(
        workcopy::locate(root, &path),
        Some((key(), "scheme/route.rsk".into()))
    );
    for other in [
        "/data/elsewhere/octo-test/lab-notes/files/a.rsk",
        "/data/github/octo-test/lab-notes/state.json",
        "/data/github/octo-test/lab-notes/versions/abc/1.rsk",
        "/data/github/octo-test/lab-notes/files",
        "/data/github/octo-test",
    ] {
        assert_eq!(workcopy::locate(root, Path::new(other)), None, "{other}");
    }
}

#[test]
fn only_names_every_platform_can_hold_are_valid() {
    for good in [
        "a.rsk",
        "scheme/route (2).rsk",
        "日本/環.rsk",
        ".hidden/a.rsk",
        "CONTROL.rsk",
        "com10.rsk",
    ] {
        assert!(workcopy::valid_repo_path(good), "{good}");
    }
    for bad in [
        "",
        "/a.rsk",
        "a//b.rsk",
        "../a.rsk",
        "a/../b.rsk",
        "a/./b.rsk",
        "a\\b.rsk",
        "a\0.rsk",
        "CON",
        "con.rsk",
        "dir/Aux.tar.rsk",
        "COM1.rsk",
        "lpt9",
        "a:b.rsk",
        "what?.rsk",
        "a*.rsk",
        "a<b>.rsk",
        "a|b.rsk",
        "quote\".rsk",
        "trailing./a.rsk",
        "space /a.rsk",
        "tab\t.rsk",
    ] {
        assert!(!workcopy::valid_repo_path(bad), "{bad:?}");
    }
}

#[tokio::test]
async fn the_state_file_round_trips_and_a_newer_one_is_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert_eq!(workcopy::load(root, &key()), Ok(None));
    let mut state = state();
    workcopy::checkout(root, &key(), &mut state, "dir/a.rsk", &download(b"one")).unwrap();
    workcopy::persist(root, &key(), state.clone(), 1)
        .await
        .unwrap();
    assert_eq!(workcopy::load(root, &key()), Ok(Some(state.clone())));
    let file = root.join("octo-test/lab-notes/state.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["repo_id"], 7);
    assert_eq!(json["files"]["dir/a.rsk"]["base_blob"], "b1");
    assert_eq!(
        json["files"]["dir/a.rsk"]["conflict"],
        serde_json::Value::Null
    );

    let newer = br#"{"version":2,"something":"new"}"#;
    std::fs::write(&file, newer).unwrap();
    assert_eq!(
        workcopy::load(root, &key()),
        Err("Made by a newer ReShiki.".into())
    );
    let refused = workcopy::persist(root, &key(), state, 2).await;
    assert_eq!(refused, Err("Made by a newer ReShiki.".into()));
    assert_eq!(std::fs::read(&file).unwrap(), newer);
}

#[tokio::test]
async fn a_late_older_write_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (mut older, mut newer) = (state(), state());
    older.branch = "older".into();
    newer.branch = "newer".into();
    workcopy::persist(root, &key(), newer.clone(), 5)
        .await
        .unwrap();
    workcopy::persist(root, &key(), older.clone(), 4)
        .await
        .unwrap();
    assert_eq!(workcopy::load(root, &key()), Ok(Some(newer)));
    // Another repository counts by itself.
    let other = RepoKey {
        owner: "octo-test".into(),
        repo: "other".into(),
    };
    workcopy::persist(root, &other, older.clone(), 1)
        .await
        .unwrap();
    assert_eq!(workcopy::load(root, &other), Ok(Some(older)));
}

#[test]
fn unsent_is_what_differs_from_the_last_sync() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = state();
    let a = workcopy::checkout(root, &key(), &mut state, "a.rsk", &download(b"one")).unwrap();
    let b = workcopy::checkout(root, &key(), &mut state, "dir/b.rsk", &download(b"two")).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), b"one");
    assert_eq!(state.files["a.rsk"].base_hash, workcopy::hash(b"one"));
    let unsent = |state: &RepoState| workcopy::unsent(root, &key(), state).unwrap();
    assert!(unsent(&state).is_empty());
    // A save that changed nothing.
    std::fs::write(&a, b"one").unwrap();
    assert!(unsent(&state).is_empty());
    // A save with no state write, or an edit made outside ReShiki.
    std::fs::write(&a, b"edited").unwrap();
    assert_eq!(unsent(&state), ["a.rsk"]);
    // A missing file is not a deletion to send.
    std::fs::remove_file(&b).unwrap();
    assert_eq!(unsent(&state), ["a.rsk"]);
    // A drawing that cannot be read is an error, never "nothing to send".
    std::fs::create_dir(&b).unwrap();
    assert!(workcopy::unsent(root, &key(), &state).is_err());
    std::fs::remove_dir(&b).unwrap();
    // A drawing not on GitHub yet.
    std::fs::write(workcopy::working_path(root, &key(), "new.rsk"), b"new").unwrap();
    state.files.insert("new.rsk".into(), FileState::default());
    assert_eq!(unsent(&state), ["a.rsk", "new.rsk"]);
}

#[test]
fn a_name_that_differs_only_by_case_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    workcopy::checkout(
        dir.path(),
        &key(),
        &mut state,
        "dir/a.rsk",
        &download(b"one"),
    )
    .unwrap();
    assert!(state.collides("dir/A.rsk") && state.collides("DIR/a.rsk"));
    assert!(!state.collides("dir/a.rsk") && !state.collides("a.rsk"));
    for refused in ["dir/A.rsk", "../a.rsk", "CON.rsk"] {
        let opened = workcopy::checkout(dir.path(), &key(), &mut state, refused, &download(b"x"));
        assert_eq!(opened, Err("Can't open this name here".into()));
    }
    assert_eq!(state.files.len(), 1);
}

#[tokio::test]
async fn saves_are_kept_as_versions_up_to_fifty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = Some(workcopy::hash(b"base"));
    let key = key();
    let save = |bytes: &'static [u8]| {
        workcopy::snapshot(root, &key, "dir/a.rsk", bytes.to_vec(), base.clone())
    };
    // What GitHub already has is not a version.
    assert_eq!(save(b"base").await, Ok(None));
    let first = save(b"one").await.unwrap().unwrap();
    assert_eq!(std::fs::read(&first.path).unwrap(), b"one");
    assert!(
        first
            .path
            .starts_with(root.join("octo-test/lab-notes/versions"))
    );
    // The same save again is not a second version.
    assert_eq!(save(b"one").await, Ok(None));
    let second = save(b"two").await.unwrap().unwrap();
    assert!(second.id > first.id);
    assert_eq!(
        workcopy::snapshots(root, &key, "dir/a.rsk"),
        Ok(vec![first.clone(), second])
    );
    // Another drawing has its own.
    assert_eq!(workcopy::snapshots(root, &key, "a.rsk"), Ok(vec![]));

    for n in 0..60u8 {
        let saved = workcopy::save_version(root, &key, "dir/a.rsk", &[n], None);
        assert!(saved.unwrap().is_some());
    }
    let kept = workcopy::snapshots(root, &key, "dir/a.rsk").unwrap();
    assert_eq!(kept.len(), 50);
    assert_eq!(std::fs::read(&kept[49].path).unwrap(), [59]);
    assert_eq!(std::fs::read(&kept[0].path).unwrap(), [10]);

    // A send drops what was saved before it, and keeps what came after.
    let sent_at = kept[48].id.clone();
    workcopy::prune_sent(root, &key, "dir/a.rsk", &sent_at).unwrap();
    assert_eq!(
        workcopy::snapshots(root, &key, "dir/a.rsk").unwrap(),
        [kept[49].clone()]
    );
}

#[tokio::test]
async fn a_renamed_repository_keeps_the_folder_it_was_first_opened_in() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    workcopy::persist(root, &key(), state(), 1).await.unwrap();
    let renamed = Repo {
        id: 7,
        key: RepoKey {
            owner: "octo-test".into(),
            repo: "lab-book".into(),
        },
        private: true,
        default_branch: "main".into(),
        push: true,
    };
    assert_eq!(workcopy::folder(root, &renamed), key());
    let other = Repo { id: 8, ..renamed };
    assert_eq!(workcopy::folder(root, &other), other.key);
}
