use crate::github::{
    RepoKey,
    api::{self, Folders},
    cache::Disk,
    client::Client,
    mock::{LOGIN, Mock},
};
use std::path::{Path, PathBuf};

fn key(owner: &str, repo: &str) -> RepoKey {
    RepoKey {
        owner: owner.into(),
        repo: repo.into(),
    }
}

/// A new sign-in as `login`, keeping replies in `cache`: as after a restart.
fn signed_in(mock: &Mock, login: &str, cache: &Path) -> Client {
    let mut tokens = mock.tokens();
    tokens.login = login.into();
    Client::new("mock-client".into(), mock.ends(), Some(tokens))
        .unwrap()
        .cached(Some(cache))
}

fn requests(mock: &Mock) -> usize {
    mock.model().log.len()
}

/// The files kept for `account`.
fn kept(cache: &Path, account: &str) -> Vec<PathBuf> {
    let folder = cache.join("github-override").join(account);
    (std::fs::read_dir(folder).into_iter().flatten())
        .map(|entry| entry.unwrap().path())
        .collect()
}

/// Choosing a repository, opening a folder (its names, dates and previews)
/// and a drawing: what the picker asks.
async fn browse(client: &Client) -> Vec<u8> {
    let repo = key(LOGIN, "lab-notes");
    let head = api::head(client, &repo, "main").await.unwrap();
    api::tree_all(client, &repo, &head.tree).await.unwrap();
    let mut folders = Folders::new(client, &repo, head.tree.clone());
    let blob = folders.blob("scheme/route.rsk").await.unwrap().unwrap();
    let date = api::last_commit(client, &repo, "scheme/route.rsk", &head.commit);
    date.await.unwrap().unwrap();
    api::blob(client, &repo, &blob).await.unwrap()
}

#[tokio::test]
async fn after_a_restart_only_the_branch_head_is_asked_and_github_says_unchanged() {
    let (mock, cache) = (Mock::start(0).await, tempfile::tempdir().unwrap());
    mock.model().seed_demo();
    let first = browse(&signed_in(&mock, LOGIN, cache.path())).await;
    // The ref and its commit, the whole tree, the top and `scheme` trees,
    // the drawing's last commit and the drawing.
    assert_eq!(requests(&mock), 7);

    let again = browse(&signed_in(&mock, LOGIN, cache.path())).await;
    assert_eq!(again, first);
    let model = mock.model();
    assert_eq!(model.log.len(), 8);
    let head = &model.log[7];
    assert_eq!(
        head.path,
        "/api/repos/octo-test/lab-notes/git/ref/heads/main"
    );
    assert!(head.headers.contains_key("if-none-match"));
    assert_eq!(model.unchanged, 1);
}

#[tokio::test]
async fn a_changed_branch_is_read_again_and_kept_with_its_new_etag() {
    let (mock, cache) = (Mock::start(0).await, tempfile::tempdir().unwrap());
    mock.model().seed_demo();
    let repo = key(LOGIN, "lab-notes");
    let client = signed_in(&mock, LOGIN, cache.path());
    let old = api::head(&client, &repo, "main").await.unwrap();
    let new = mock
        .model()
        .repo("lab-notes")
        .remote_commit("new.rsk", Some(b"{}"));

    let client = signed_in(&mock, LOGIN, cache.path());
    assert_eq!(api::head(&client, &repo, "main").await.unwrap().commit, new);
    assert_ne!(new, old.commit);
    assert_eq!(mock.model().unchanged, 0);
    assert_eq!(api::head(&client, &repo, "main").await.unwrap().commit, new);
    assert_eq!(mock.model().unchanged, 1);
}

#[tokio::test]
async fn objects_by_name_are_read_from_disk_without_asking() {
    let (mock, cache) = (Mock::start(0).await, tempfile::tempdir().unwrap());
    mock.model().seed_demo();
    let repo = key(LOGIN, "lab-notes");
    let head = api::head(&mock.client(), &repo, "main").await.unwrap();
    let blob = crate::github::mock::sha(include_bytes!(
        "../../../website/public/examples/benzofuran.rsk"
    ));
    let client = signed_in(&mock, LOGIN, cache.path());
    let tree = api::tree(&client, &repo, &head.tree).await.unwrap();
    let bytes = api::blob(&client, &repo, &blob).await.unwrap();
    let before = requests(&mock);

    let client = signed_in(&mock, LOGIN, cache.path());
    assert_eq!(api::tree(&client, &repo, &head.tree).await, Ok(tree));
    assert_eq!(api::blob(&client, &repo, &blob).await, Ok(bytes));
    assert_eq!(requests(&mock), before);
}

#[tokio::test]
async fn each_account_and_signed_out_reading_keep_their_own() {
    let (mock, cache) = (Mock::start(0).await, tempfile::tempdir().unwrap());
    mock.model().seed_shared();
    let repo = key("alice", "ligands");
    let ring = include_bytes!("../../../website/public/examples/benzofuran.rsk");
    let blob = crate::github::mock::sha(ring);
    let reader = |n| match n {
        0 => signed_in(&mock, LOGIN, cache.path()),
        1 => signed_in(&mock, "Bob", cache.path()),
        _ => (Client::public(mock.ends()).unwrap()).cached(Some(cache.path())),
    };
    for n in 0..3 {
        // Never what another account kept.
        assert_eq!(api::blob(&reader(n), &repo, &blob).await.unwrap(), ring);
        assert_eq!(requests(&mock), n + 1);
    }
    assert_eq!(kept(cache.path(), LOGIN).len(), 1);
    assert_eq!(kept(cache.path(), "bob").len(), 1);
    assert_eq!(kept(cache.path(), "_public").len(), 1);
    // Each reads its own again with no request, signed out too.
    for n in 0..3 {
        assert_eq!(api::blob(&reader(n), &repo, &blob).await.unwrap(), ring);
    }
    assert_eq!(requests(&mock), 3);
}

#[tokio::test]
async fn past_the_cap_the_least_recently_used_go_first() {
    let cache = tempfile::tempdir().unwrap();
    // 64 + 2 bytes of head and 100 of reply: two fit, three do not.
    let disk = Disk::new(cache.path(), true).with_cap(400);
    let pause = || std::thread::sleep(std::time::Duration::from_millis(5));
    disk.store(LOGIN, "/a", None, &[1; 100]).await;
    pause();
    disk.store(LOGIN, "/b", None, &[2; 100]).await;
    pause();
    assert!(disk.load(LOGIN, "/a").await.is_some());
    pause();
    disk.store("bob", "/c", Some("\"c\""), &[3; 100]).await;
    assert!(disk.load(LOGIN, "/b").await.is_none());
    assert_eq!(disk.load(LOGIN, "/a").await.unwrap().bytes, [1; 100]);
    let c = disk.load("bob", "/c").await.unwrap();
    assert_eq!((c.etag.as_deref(), c.bytes), (Some("\"c\""), vec![3; 100]));
}

#[tokio::test]
async fn an_entry_that_cannot_be_read_whole_is_a_miss_and_removed() {
    let cache = tempfile::tempdir().unwrap();
    let disk = Disk::new(cache.path(), true);
    let damages: [fn(&Path); 3] = [
        |file| std::fs::write(file, b"not an entry").unwrap(),
        // Cut short, as by a crash before the write reached the disk.
        |file| {
            let bytes = std::fs::read(file).unwrap();
            std::fs::write(file, &bytes[..bytes.len() - 1]).unwrap();
        },
        |file| std::fs::write(file, b"").unwrap(),
    ];
    for (n, damage) in damages.into_iter().enumerate() {
        let path = format!("/entry-{n}");
        disk.store(LOGIN, &path, Some("\"e\""), b"reply").await;
        let file = kept(cache.path(), LOGIN).pop().unwrap();
        damage(&file);
        assert!(disk.load(LOGIN, &path).await.is_none());
        assert!(!file.exists());
    }

    // Through a client, it is asked of GitHub again and kept again.
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    let repo = key(LOGIN, "lab-notes");
    let head = api::head(&mock.client(), &repo, "main").await.unwrap();
    api::tree(&signed_in(&mock, LOGIN, cache.path()), &repo, &head.tree)
        .await
        .unwrap();
    let file = kept(cache.path(), LOGIN).pop().unwrap();
    std::fs::write(&file, b"garbage").unwrap();
    let before = requests(&mock);
    let client = signed_in(&mock, LOGIN, cache.path());
    assert!(api::tree(&client, &repo, &head.tree).await.is_ok());
    assert_eq!(requests(&mock), before + 1);
    assert_ne!(std::fs::read(&file).unwrap(), b"garbage");
}
