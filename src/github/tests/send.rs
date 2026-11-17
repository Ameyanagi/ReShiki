use crate::github::{
    Error, Repo, RepoKey,
    api::{self, Download},
    client::{Client, FILE_LIMIT},
    mock::{self, LOGIN, Mock, Race},
    send::{self, Choice, Outgoing, SendOutcome, default_message},
    workcopy::{self, FileState, RepoState},
};
use std::path::Path;

const REFS: &str = "/api/repos/octo-test/notes/git/refs/heads/main";

fn key() -> RepoKey {
    RepoKey {
        owner: LOGIN.into(),
        repo: "notes".into(),
    }
}

fn out(path: &str, bytes: &[u8], base: Option<&[u8]>) -> Outgoing {
    Outgoing {
        repo_path: path.into(),
        bytes: bytes.to_vec(),
        svg: format!("<svg>{path}</svg>").into_bytes(),
        base_blob: base.map(mock::sha),
        hash: workcopy::hash(bytes),
    }
}

/// `octo-test/notes` (id 1) with the given files, each in its own commit.
async fn notes(files: &[(&str, &[u8])]) -> (Mock, Client) {
    let mock = Mock::start(0).await;
    mock.model()
        .add_repo("notes")
        .remote_commit("README.md", Some(b"notes"));
    for (path, bytes) in files {
        mock.model().repo("notes").remote_commit(path, Some(bytes));
    }
    let client = mock.client();
    (mock, client)
}

fn remote(mock: &Mock, path: &str) -> Option<Vec<u8>> {
    mock.model().repo("notes").files().get(path).cloned()
}

fn commits(mock: &Mock) -> usize {
    mock.model().repo("notes").commits.len()
}

fn writes(mock: &Mock) -> usize {
    let model = mock.model();
    ["POST", "PATCH", "PUT"]
        .iter()
        .map(|method| model.count(method, "/api/repos"))
        .sum()
}

#[test]
fn the_default_message_names_the_drawings() {
    let paths = |names: &[&str]| {
        names
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(default_message(&paths(&["a.rsk"])), "Update a.rsk");
    assert_eq!(
        default_message(&paths(&["a.rsk", "dir/b.rsk"])),
        "Update a.rsk, b.rsk"
    );
    assert_eq!(
        default_message(&paths(&["a.rsk", "b.rsk", "c.rsk"])),
        "Update a.rsk, b.rsk, c.rsk"
    );
    assert_eq!(
        default_message(&paths(&["dir/a.rsk", "b.rsk", "c.rsk", "d.rsk"])),
        "Update a.rsk and 3 more"
    );
}

#[tokio::test]
async fn one_drawing_is_one_commit_with_its_preview() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let before = mock.model().repo("notes").head().unwrap();
    let files = [out("a.rsk", b"new", Some(b"old"))];
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();

    assert_eq!(remote(&mock, "a.rsk").unwrap(), b"new");
    assert_eq!(remote(&mock, "a.svg").unwrap(), b"<svg>a.rsk</svg>");
    assert_eq!(remote(&mock, "README.md").unwrap(), b"notes");
    let mut model = mock.model();
    let repo = model.repo("notes");
    assert_eq!(repo.head(), Some(outcome.commit.clone()));
    let commit = &repo.commits[&outcome.commit];
    assert_eq!(
        (commit.message.as_str(), commit.parent.as_ref()),
        ("Update a.rsk", Some(&before))
    );
    assert_eq!(
        outcome.sent,
        [(
            "a.rsk".to_owned(),
            mock::sha(b"new"),
            workcopy::hash(b"new")
        )]
    );
    assert!(outcome.conflicts.is_empty());
    assert_eq!(outcome.repo.key, key());
    // Fast-forward only.
    let patch = model.log.iter().find(|one| one.method == "PATCH").unwrap();
    let body: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({ "sha": outcome.commit, "force": false })
    );
}

#[tokio::test]
async fn uploads_go_four_at_a_time_before_the_tree_commit_and_branch() {
    let (mock, client) = notes(&[]).await;
    mock.model().delay = std::time::Duration::from_millis(40);
    let files: Vec<Outgoing> = (0..6)
        .map(|n| {
            out(
                &format!("d{n}.rsk"),
                format!("drawing {n}").as_bytes(),
                None,
            )
        })
        .collect();
    let outcome = send::send(&client, 1, "m", &files).await.unwrap();
    assert_eq!(outcome.sent.len(), 6);
    for n in 0..6 {
        let (drawing, preview) = (format!("d{n}.rsk"), format!("d{n}.svg"));
        assert_eq!(
            remote(&mock, &drawing).unwrap(),
            format!("drawing {n}").as_bytes()
        );
        assert_eq!(
            remote(&mock, &preview).unwrap(),
            format!("<svg>{drawing}</svg>").as_bytes()
        );
    }
    let model = mock.model();
    assert_eq!(model.most_busy, 4);
    let writes: Vec<&str> = (model.log.iter())
        .filter(|one| one.method != "GET")
        .map(|one| one.path.rsplit("/git/").next().unwrap())
        .collect();
    assert_eq!(writes[..12], ["blobs"; 12]);
    assert_eq!(writes[12..], ["trees", "commits", "refs/heads/main"]);
}

#[tokio::test]
async fn a_drawing_opened_from_an_older_listing_meets_the_change_on_send() {
    let (mock, client) = notes(&[("a.rsk", b"one")]).await;
    let dir = tempfile::tempdir().unwrap();
    // Listed, then changed on GitHub before it is opened from that listing.
    let head = api::head(&client, &key(), "main").await.unwrap();
    let top = api::tree(&client, &key(), &head.tree).await.unwrap();
    let blob = top
        .iter()
        .find(|entry| entry.name == "a.rsk")
        .unwrap()
        .sha
        .clone();
    mock.model()
        .repo("notes")
        .remote_commit("a.rsk", Some(b"two"));
    let download = api::download_listed(&client, &key(), head.commit.clone(), blob.clone())
        .await
        .unwrap();
    let mut state = RepoState::new(&Repo {
        id: 1,
        key: key(),
        private: true,
        default_branch: "main".into(),
        push: true,
    });
    workcopy::checkout(dir.path(), &key(), &mut state, "a.rsk", &download).unwrap();
    let file = &state.files["a.rsk"];
    assert_eq!(
        (&file.base_commit, file.base_blob.as_ref()),
        (&head.commit, Some(&blob))
    );
    // An edit sent from it never goes over GitHub's newer version.
    let outcome = send::send(&client, 1, "m", &[out("a.rsk", b"mine", Some(b"one"))])
        .await
        .unwrap();
    assert!(outcome.sent.is_empty());
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(
        outcome.conflicts[0].theirs.as_deref(),
        Some(b"two".as_slice())
    );
    assert_eq!(remote(&mock, "a.rsk").unwrap(), b"two");
}

#[tokio::test]
async fn several_drawings_new_and_changed_are_still_one_commit() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let before = commits(&mock);
    let files = [
        out("a.rsk", b"new", Some(b"old")),
        out("dir/deep/b c.rsk", b"fresh", None),
    ];
    let message = default_message(&["a.rsk".into(), "dir/deep/b c.rsk".into()]);
    let outcome = send::send(&client, 1, &message, &files).await.unwrap();
    assert_eq!(commits(&mock), before + 1);
    assert_eq!(outcome.sent.len(), 2);
    assert_eq!(remote(&mock, "dir/deep/b c.rsk").unwrap(), b"fresh");
    assert_eq!(
        remote(&mock, "dir/deep/b c.svg").unwrap(),
        b"<svg>dir/deep/b c.rsk</svg>"
    );
    let message = mock.model().repo("notes").commits[&outcome.commit]
        .message
        .clone();
    assert_eq!(message, "Update a.rsk, b c.rsk");
}

#[tokio::test]
async fn a_head_that_moved_elsewhere_is_built_on() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let moved = mock
        .model()
        .repo("notes")
        .remote_commit("other.rsk", Some(b"theirs"));
    let files = [out("a.rsk", b"new", Some(b"old"))];
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();
    assert!(outcome.conflicts.is_empty());
    let parent = mock.model().repo("notes").commits[&outcome.commit]
        .parent
        .clone();
    assert_eq!(parent, Some(moved));
    assert_eq!(remote(&mock, "other.rsk").unwrap(), b"theirs");
    assert_eq!(remote(&mock, "a.rsk").unwrap(), b"new");
}

#[tokio::test]
async fn a_drawing_changed_on_github_is_left_out_as_a_conflict() {
    let (mock, client) = notes(&[("dir/a.rsk", b"old a"), ("dir/b.rsk", b"old b")]).await;
    let theirs = mock
        .model()
        .repo("notes")
        .remote_commit("dir/a.rsk", Some(b"their a"));
    mock.model()
        .repo("notes")
        .remote_commit("dir/a.svg", Some(b"their svg"));
    let files = [
        out("dir/a.rsk", b"my a", Some(b"old a")),
        out("dir/b.rsk", b"my b", Some(b"old b")),
    ];
    let outcome = send::send(&client, 1, "Update b.rsk", &files)
        .await
        .unwrap();

    assert_eq!(
        outcome.sent,
        [(
            "dir/b.rsk".to_owned(),
            mock::sha(b"my b"),
            workcopy::hash(b"my b")
        )]
    );
    assert_eq!(remote(&mock, "dir/b.rsk").unwrap(), b"my b");
    // GitHub's version and its preview are untouched.
    assert_eq!(remote(&mock, "dir/a.rsk").unwrap(), b"their a");
    assert_eq!(remote(&mock, "dir/a.svg").unwrap(), b"their svg");
    let [conflict] = outcome.conflicts.as_slice() else {
        panic!("One conflict");
    };
    assert_eq!(conflict.repo_path, "dir/a.rsk");
    assert_eq!(conflict.blob, Some(mock::sha(b"their a")));
    assert_eq!(conflict.theirs.as_deref(), Some(b"their a".as_slice()));
    assert_eq!(conflict.mine, b"my a");
    assert_eq!(conflict.names, ["a.rsk", "a.svg", "b.rsk"]);
    assert_ne!(conflict.commit, theirs);
    assert!(
        mock.model()
            .repo("notes")
            .commits
            .contains_key(&conflict.commit)
    );
}

#[tokio::test]
async fn a_drawing_deleted_on_github_is_a_conflict_and_only_conflicts_make_no_commit() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let head = mock.model().repo("notes").remote_commit("a.rsk", None);
    let before = (commits(&mock), writes(&mock));
    let files = [out("a.rsk", b"mine", Some(b"old"))];
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();
    assert_eq!((commits(&mock), writes(&mock)), before);
    assert!(outcome.sent.is_empty());
    assert_eq!(outcome.commit, head);
    let conflict = &outcome.conflicts[0];
    assert_eq!(
        (conflict.blob.clone(), conflict.theirs.clone()),
        (None, None)
    );
    assert_eq!(conflict.commit, head);

    // A new drawing whose name GitHub already has is a conflict too.
    mock.model()
        .repo("notes")
        .remote_commit("b.rsk", Some(b"theirs"));
    let outcome = send::send(&client, 1, "m", &[out("b.rsk", b"mine", None)])
        .await
        .unwrap();
    assert_eq!(
        outcome.conflicts[0].theirs.as_deref(),
        Some(b"theirs".as_slice())
    );
    assert_eq!(remote(&mock, "b.rsk").unwrap(), b"theirs");
}

#[tokio::test]
async fn the_same_bytes_on_github_are_adopted_not_a_conflict() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let head = mock
        .model()
        .repo("notes")
        .remote_commit("a.rsk", Some(b"same"));
    let before = writes(&mock);
    let files = [out("a.rsk", b"same", Some(b"old"))];
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();
    assert!(outcome.conflicts.is_empty());
    assert_eq!(
        outcome.sent,
        [(
            "a.rsk".to_owned(),
            mock::sha(b"same"),
            workcopy::hash(b"same")
        )]
    );
    assert_eq!(outcome.commit, head);
    assert_eq!(writes(&mock), before);
}

#[tokio::test]
async fn a_send_whose_reply_was_lost_is_not_sent_twice() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    mock.model().apply_then_fail("PATCH", REFS);
    let files = [out("a.rsk", b"new", Some(b"old"))];
    assert_eq!(
        send::send(&client, 1, "Update a.rsk", &files).await,
        Err(Error::Offline)
    );
    assert_eq!(remote(&mock, "a.rsk").unwrap(), b"new");
    let before = commits(&mock);
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();
    assert!(outcome.conflicts.is_empty());
    assert_eq!(outcome.sent.len(), 1);
    assert_eq!(commits(&mock), before);
    assert_eq!(Some(outcome.commit), mock.model().repo("notes").head());
}

#[tokio::test]
async fn a_head_that_moves_during_the_send_is_tried_again() {
    let (mock, client) = notes(&[("a.rsk", b"old a"), ("b.rsk", b"old b")]).await;
    // Someone pushes b.rsk just before our branch update arrives.
    mock.model().races.push(Race {
        method: "PATCH",
        path: REFS,
        repo: "notes",
        file: "b.rsk",
        bytes: Some(b"their b".to_vec()),
    });
    let files = [
        out("a.rsk", b"my a", Some(b"old a")),
        out("b.rsk", b"my b", Some(b"old b")),
    ];
    let outcome = send::send(&client, 1, "Update", &files).await.unwrap();
    assert_eq!(mock.model().count("PATCH", REFS), 2);
    assert_eq!(remote(&mock, "a.rsk").unwrap(), b"my a");
    assert_eq!(remote(&mock, "b.rsk").unwrap(), b"their b");
    assert_eq!(outcome.sent.len(), 1);
    assert_eq!(outcome.conflicts[0].repo_path, "b.rsk");
    assert_eq!(Some(outcome.commit), mock.model().repo("notes").head());
    // Each drawing was uploaded once, however many rounds it took.
    assert_eq!(
        mock.model()
            .count("POST", "/api/repos/octo-test/notes/git/blobs"),
        4
    );
}

#[tokio::test]
async fn a_refusal_with_an_unmoved_head_fails_at_once() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let head = mock.model().repo("notes").head();
    mock.model().fail_next("PATCH", REFS, 422);
    let files = [out("a.rsk", b"new", Some(b"old"))];
    assert_eq!(
        send::send(&client, 1, "Update a.rsk", &files).await,
        Err(Error::Conflict)
    );
    assert_eq!(mock.model().count("PATCH", REFS), 1);
    assert_eq!(mock.model().repo("notes").head(), head);
    assert_eq!(remote(&mock, "a.rsk").unwrap(), b"old");
}

#[tokio::test]
async fn an_empty_repository_gets_a_readme_then_one_commit() {
    let mock = Mock::start(0).await;
    mock.model().add_repo("notes");
    let files = [out("a.rsk", b"a", None), out("dir/b.rsk", b"b", None)];
    let outcome = send::send(&mock.client(), 1, "First drawings", &files)
        .await
        .unwrap();
    let mut model = mock.model();
    let repo = model.repo("notes");
    assert_eq!(repo.commits.len(), 2);
    let names: Vec<String> = repo.files().into_keys().collect();
    assert_eq!(
        names,
        ["README.md", "a.rsk", "a.svg", "dir/b.rsk", "dir/b.svg"]
    );
    assert_eq!(repo.commits[&outcome.commit].message, "First drawings");
}

#[tokio::test]
async fn a_missing_branch_is_an_error_and_nothing_is_written() {
    let (mock, client) = notes(&[]).await;
    mock.model().repo("notes").default_branch = "trunk".into();
    let before = writes(&mock);
    let sent = send::send(&client, 1, "m", &[out("a.rsk", b"a", None)]).await;
    assert_eq!(sent, Err(Error::Rejected("Branch not found".into())));
    assert_eq!(writes(&mock), before);
}

#[tokio::test]
async fn a_renamed_repository_and_a_new_default_branch_are_followed() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let main = mock.model().repo("notes").head().unwrap();
    {
        let mut model = mock.model();
        let repo = model.repo("notes");
        repo.refs.insert("trunk".into(), main.clone());
        repo.default_branch = "trunk".into();
        repo.name = "lab-book".into();
    }
    let outcome = send::send(&client, 1, "m", &[out("a.rsk", b"new", Some(b"old"))])
        .await
        .unwrap();
    let renamed = RepoKey {
        owner: LOGIN.into(),
        repo: "lab-book".into(),
    };
    assert_eq!(
        (
            outcome.repo.key.clone(),
            outcome.repo.default_branch.as_str()
        ),
        (renamed, "trunk")
    );
    let mut model = mock.model();
    let repo = model.repo("lab-book");
    assert_eq!(repo.refs["trunk"], outcome.commit);
    assert_eq!(repo.refs["main"], main);

    let mut state = RepoState::new(&Repo {
        id: 1,
        key: key(),
        private: true,
        default_branch: "main".into(),
        push: true,
    });
    send::apply(&mut state, &outcome);
    assert_eq!(
        (state.repo.as_str(), state.branch.as_str()),
        ("lab-book", "trunk")
    );
    assert_eq!(state.key(), outcome.repo.key);
}

#[tokio::test]
async fn a_drawing_over_the_limit_stops_the_send_before_any_upload() {
    let (mock, client) = notes(&[]).await;
    let mut big = out("dir/big.rsk", b"", None);
    big.svg = vec![b' '; FILE_LIMIT + 1];
    let sent = send::send(&client, 1, "m", &[out("a.rsk", b"a", None), big]).await;
    assert_eq!(sent, Err(Error::TooLarge(Some("big.rsk".into()))));
    assert_eq!(
        sent.unwrap_err().to_string(),
        "“big.rsk” is too large for GitHub."
    );
    assert!(mock.model().log.is_empty());
}

/// A working copy of `octo-test/notes` with `a.rsk` opened from GitHub.
fn opened(root: &Path, mock: &Mock, path: &str) -> RepoState {
    let mut model = mock.model();
    let repo = model.repo("notes");
    let mut state = RepoState::new(&Repo {
        id: repo.id,
        key: key(),
        private: true,
        default_branch: "main".into(),
        push: true,
    });
    let bytes = repo.files()[path].clone();
    let download = Download {
        commit: repo.head().unwrap(),
        blob: mock::sha(&bytes),
        bytes,
    };
    workcopy::checkout(root, &key(), &mut state, path, &download).unwrap();
    state
}

fn save(root: &Path, path: &str, bytes: &[u8]) {
    std::fs::write(workcopy::working_path(root, &key(), path), bytes).unwrap();
    workcopy::save_version(root, &key(), path, bytes, None).unwrap();
}

fn drawing(name: &str) -> Vec<u8> {
    let mut doc = crate::document::Document::from_native_file(include_bytes!(
        "../../../website/public/examples/benzofuran.rsk"
    ))
    .unwrap();
    // Any change will do: each name moves an atom somewhere else.
    doc.atoms[0].position.x += name.bytes().map(f32::from).sum::<f32>();
    doc.file_json().unwrap()
}

/// Send what is saved here, as the app does.
async fn send_saved(root: &Path, client: &Client, state: &RepoState) -> SendOutcome {
    let files = send::outgoing(root, &key(), state).unwrap();
    let paths: Vec<String> = files.iter().map(|file| file.repo_path.clone()).collect();
    send::send(client, state.repo_id, &default_message(&paths), &files)
        .await
        .unwrap()
}

#[tokio::test]
async fn what_is_saved_here_is_what_is_sent_with_a_rendered_preview() {
    let (old, new) = (drawing("old"), drawing("new"));
    let (mock, client) = notes(&[("a.rsk", &old), ("b.rsk", &old)]).await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = opened(root, &mock, "a.rsk");
    state.files.extend(opened(root, &mock, "b.rsk").files);
    assert_eq!(send::outgoing(root, &key(), &state), Ok(vec![]));

    save(root, "a.rsk", &new);
    let files = send::outgoing(root, &key(), &state).unwrap();
    let [file] = files.as_slice() else {
        panic!("Only the changed drawing goes out");
    };
    assert_eq!((file.repo_path.as_str(), &file.bytes), ("a.rsk", &new));
    assert_eq!(file.base_blob, Some(mock::sha(&old)));
    assert_eq!(file.hash, workcopy::hash(&new));
    assert!(
        String::from_utf8(file.svg.clone())
            .unwrap()
            .contains("<svg")
    );

    let outcome = send_saved(root, &client, &state).await;
    assert_eq!(remote(&mock, "a.rsk").unwrap(), new);
    assert_eq!(remote(&mock, "a.svg").unwrap(), file.svg);
    assert_eq!(remote(&mock, "b.svg"), None);
    let message = mock.model().repo("notes").commits[&outcome.commit]
        .message
        .clone();
    assert_eq!(message, "Update a.rsk");
}

#[tokio::test]
async fn a_save_made_during_the_send_stays_unsent_with_its_version() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = opened(root, &mock, "a.rsk");
    save(root, "a.rsk", b"first");
    let sent_at = workcopy::now_id();
    let files = [out("a.rsk", b"first", Some(b"old"))];
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    save(root, "a.rsk", b"second");

    send::record(root, &key(), &mut state, 1, &outcome, &sent_at)
        .await
        .unwrap();
    let file = &state.files["a.rsk"];
    assert_eq!(file.base_commit, outcome.commit);
    assert_eq!(file.base_blob, Some(mock::sha(b"first")));
    assert_eq!(file.base_hash, workcopy::hash(b"first"));
    assert_eq!(workcopy::load(root, &key()), Ok(Some(state.clone())));
    assert_eq!(workcopy::unsent(root, &key(), &state).unwrap(), ["a.rsk"]);
    let kept = workcopy::snapshots(root, &key(), "a.rsk").unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(std::fs::read(&kept[0].path).unwrap(), b"second");
}

#[tokio::test]
async fn versions_are_dropped_only_after_the_state_is_written() {
    let (mock, client) = notes(&[("a.rsk", b"old")]).await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = opened(root, &mock, "a.rsk");
    save(root, "a.rsk", b"first");
    let files = [out("a.rsk", b"first", Some(b"old"))];
    let outcome = send::send(&client, 1, "Update a.rsk", &files)
        .await
        .unwrap();
    let state_file = root.join("octo-test/notes/state.json");
    std::fs::write(&state_file, br#"{"version":9}"#).unwrap();
    let sent_at = workcopy::now_id();
    let recorded = send::record(root, &key(), &mut state, 1, &outcome, &sent_at).await;
    assert_eq!(recorded, Err("Made by a newer ReShiki.".into()));
    assert_eq!(workcopy::snapshots(root, &key(), "a.rsk").unwrap().len(), 1);

    std::fs::remove_file(&state_file).unwrap();
    send::record(root, &key(), &mut state, 2, &outcome, &sent_at)
        .await
        .unwrap();
    assert_eq!(workcopy::snapshots(root, &key(), "a.rsk"), Ok(vec![]));
    assert_eq!(workcopy::unsent(root, &key(), &state), Ok(vec![]));
}

/// `a.rsk` opened, changed on GitHub to "theirs", saved here as "mine", and
/// sent: one conflict, recorded.
async fn conflicted(root: &Path) -> (Mock, Client, RepoState, send::Conflict) {
    let (mock, client) = notes(&[("dir/a.rsk", &drawing("old"))]).await;
    let mut state = opened(root, &mock, "dir/a.rsk");
    mock.model()
        .repo("notes")
        .remote_commit("dir/a.rsk", Some(&drawing("theirs")));
    save(root, "dir/a.rsk", &drawing("mine"));
    let outcome = send_saved(root, &client, &state).await;
    send::apply(&mut state, &outcome);
    let mark = state.files["dir/a.rsk"].conflict.clone().unwrap();
    let conflict = outcome.conflicts.into_iter().next().unwrap();
    assert_eq!(
        (mark.commit, mark.blob),
        (conflict.commit.clone(), conflict.blob.clone())
    );
    (mock, client, state, conflict)
}

#[tokio::test]
async fn keep_mine_then_send_goes_over_githubs_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (mock, client, mut state, conflict) = conflicted(root).await;
    let copy = send::resolve(root, &key(), &mut state, &conflict, Choice::KeepMine);
    assert_eq!(copy, Ok(None));
    assert_eq!(state.files["dir/a.rsk"].conflict, None);
    assert_eq!(
        workcopy::unsent(root, &key(), &state).unwrap(),
        ["dir/a.rsk"]
    );
    let outcome = send_saved(root, &client, &state).await;
    assert!(outcome.conflicts.is_empty());
    assert_eq!(remote(&mock, "dir/a.rsk").unwrap(), drawing("mine"));
    // The other version is still in the history.
    let parent = mock.model().repo("notes").commits[&outcome.commit]
        .parent
        .clone();
    assert_eq!(parent, Some(conflict.commit));
}

#[tokio::test]
async fn keep_githubs_keeps_mine_as_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (_mock, client, mut state, conflict) = conflicted(root).await;
    // Edits made after the conflict was found are the ones kept.
    std::fs::write(
        workcopy::working_path(root, &key(), "dir/a.rsk"),
        drawing("mine, later"),
    )
    .unwrap();
    let copy = send::resolve(root, &key(), &mut state, &conflict, Choice::KeepTheirs);
    assert_eq!(copy, Ok(None));
    let here = std::fs::read(workcopy::working_path(root, &key(), "dir/a.rsk")).unwrap();
    assert_eq!(here, drawing("theirs"));
    let file = &state.files["dir/a.rsk"];
    assert_eq!(
        (file.conflict.clone(), file.base_blob.clone()),
        (None, conflict.blob.clone())
    );
    assert_eq!(workcopy::unsent(root, &key(), &state), Ok(vec![]));
    let versions = workcopy::snapshots(root, &key(), "dir/a.rsk").unwrap();
    let newest = std::fs::read(&versions.last().unwrap().path).unwrap();
    assert_eq!(newest, drawing("mine, later"));
    assert!(send_saved(root, &client, &state).await.sent.is_empty());
}

#[tokio::test]
async fn keep_githubs_deletion_removes_the_drawing_here() {
    let (mock, client) = notes(&[("a.rsk", &drawing("old"))]).await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = opened(root, &mock, "a.rsk");
    mock.model().repo("notes").remote_commit("a.rsk", None);
    save(root, "a.rsk", &drawing("mine"));
    let conflict = send_saved(root, &client, &state).await.conflicts.remove(0);
    send::resolve(root, &key(), &mut state, &conflict, Choice::KeepTheirs).unwrap();
    assert!(!workcopy::working_path(root, &key(), "a.rsk").exists());
    assert!(state.files.is_empty());
    assert_eq!(workcopy::snapshots(root, &key(), "a.rsk").unwrap().len(), 1);
}

#[tokio::test]
async fn keep_both_saves_mine_under_the_next_free_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (mock, client, mut state, conflict) = conflicted(root).await;
    std::fs::write(
        workcopy::working_path(root, &key(), "dir/a.rsk"),
        drawing("mine, later"),
    )
    .unwrap();
    let copy = send::resolve(root, &key(), &mut state, &conflict, Choice::KeepBoth).unwrap();
    assert_eq!(copy.as_deref(), Some("dir/a (2).rsk"));
    let read = |path: &str| std::fs::read(workcopy::working_path(root, &key(), path)).unwrap();
    assert_eq!(read("dir/a (2).rsk"), drawing("mine, later"));
    assert_eq!(read("dir/a.rsk"), drawing("theirs"));
    assert_eq!(state.files["dir/a (2).rsk"], FileState::default());
    assert_eq!(
        workcopy::unsent(root, &key(), &state).unwrap(),
        ["dir/a (2).rsk"]
    );

    let outcome = send_saved(root, &client, &state).await;
    assert!(outcome.conflicts.is_empty());
    assert_eq!(
        remote(&mock, "dir/a (2).rsk").unwrap(),
        drawing("mine, later")
    );
    assert!(remote(&mock, "dir/a (2).svg").is_some());
    assert_eq!(remote(&mock, "dir/a.rsk").unwrap(), drawing("theirs"));
}

#[tokio::test]
async fn keep_both_skips_names_taken_here_or_on_github() {
    for taken in ["a (2).rsk", "a (2).svg"] {
        // On GitHub.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (_mock, _client, mut state, mut conflict) = conflicted(root).await;
        conflict.names.push(taken.into());
        let copy = send::resolve(root, &key(), &mut state, &conflict, Choice::KeepBoth).unwrap();
        assert_eq!(copy.as_deref(), Some("dir/a (3).rsk"), "{taken} on GitHub");

        // On this computer.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (_mock, _client, mut state, conflict) = conflicted(root).await;
        std::fs::write(
            workcopy::working_path(root, &key(), &format!("dir/{taken}")),
            b"x",
        )
        .unwrap();
        let copy = send::resolve(root, &key(), &mut state, &conflict, Choice::KeepBoth).unwrap();
        assert_eq!(copy.as_deref(), Some("dir/a (3).rsk"), "{taken} here");
    }
}

#[tokio::test]
async fn the_conflict_names_come_from_githubs_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mock, client, state, _) = conflicted(dir.path()).await;
    mock.model()
        .repo("notes")
        .remote_commit("dir/a (2).svg", Some(b"<svg/>"));
    let conflict = send_saved(dir.path(), &client, &state)
        .await
        .conflicts
        .remove(0);
    assert_eq!(conflict.names, ["a (2).svg", "a.rsk"]);
}

#[tokio::test]
async fn newer_finds_each_drawing_changed_on_github_and_who_changed_it() {
    let (mock, client) = notes(&[("dir/a.rsk", b"a"), ("b.rsk", b"b")]).await;
    let based = |path: &str, bytes: &[u8]| (path.to_owned(), mock::sha(bytes));
    let files = [based("dir/a.rsk", b"a"), based("b.rsk", b"b")];
    let newer = || send::newer(&client, 1, &files);
    assert!(newer().await.unwrap().1.is_empty(), "nothing changed yet");

    // @alice changes one between two checks; a deletion is left for Send.
    let commit = mock.model().repo("notes").commit_as(
        "dir/a.rsk",
        Some(b"ethyl"),
        "alice",
        "Add an ethyl group",
    );
    mock.model().repo("notes").remote_commit("b.rsk", None);
    let head = mock.model().repo("notes").head().unwrap();
    let (_, found) = newer().await.unwrap();
    let [(conflict, by)] = found.as_slice() else {
        panic!("one drawing changed: {found:?}");
    };
    assert_eq!(conflict.repo_path, "dir/a.rsk");
    assert_eq!(conflict.commit, head);
    assert_eq!(conflict.blob.as_deref(), Some(mock::sha(b"ethyl").as_str()));
    assert_eq!(conflict.theirs.as_deref(), Some(b"ethyl".as_slice()));
    assert_eq!(conflict.names, ["a.rsk"]);
    let by = by.as_ref().unwrap();
    assert_eq!(by.commit, commit);
    assert_eq!(by.login.as_deref(), Some("alice"));
    assert_eq!(by.message, "Add an ethyl group");
    // Nothing is written.
    assert_eq!(writes(&mock), 0);
    // Found by id after a rename and a new default branch, as Send does.
    {
        let mut model = mock.model();
        let repo = model.repo("notes");
        let head = repo.head().unwrap();
        repo.refs.insert("trunk".into(), head);
        repo.default_branch = "trunk".into();
        repo.name = "renamed".into();
    }
    let (repo, found) = newer().await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        (repo.key.repo.as_str(), repo.default_branch.as_str()),
        ("renamed", "trunk")
    );
}
