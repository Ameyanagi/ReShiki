use crate::github::{
    RepoKey,
    api::{self, Folders},
    cache::Disk,
    client::Client,
    config,
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

/// A disk cache in `cache` for a test server.
fn disk(cache: &Path) -> Disk {
    Disk::new(
        cache,
        &config::resolve(Some("http://127.0.0.1:1".into()), None),
    )
}

/// The file of `path` kept for `account`, under the one server's folder.
fn entry(cache: &Path, account: &str, path: &str) -> PathBuf {
    let server = std::fs::read_dir(cache).unwrap().next().unwrap().unwrap();
    let name = crate::github::workcopy::hash(path.as_bytes());
    server.path().join(account).join(name)
}

/// The files kept for `account`, under the one server's folder in `cache`.
fn kept(cache: &Path, account: &str) -> Vec<PathBuf> {
    let server = std::fs::read_dir(cache).unwrap().next().unwrap().unwrap();
    (std::fs::read_dir(server.path().join(account))
        .into_iter()
        .flatten())
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
    // A refusal is never answered from what was kept.
    let path = "/api/repos/octo-test/lab-notes/git/ref/heads/main";
    mock.model().fail_next("GET", path, 404);
    let client = signed_in(&mock, LOGIN, cache.path());
    let gone = api::head(&client, &repo, "main").await;
    assert_eq!(gone, Err(crate::github::Error::NotFound));
}

#[tokio::test]
async fn a_drawing_changed_on_github_reads_new_after_a_restart_and_in_history() {
    let (mock, cache) = (Mock::start(0).await, tempfile::tempdir().unwrap());
    mock.model().seed_demo();
    let repo = key(LOGIN, "lab-notes");
    let client = signed_in(&mock, LOGIN, cache.path());
    api::download(&client, &repo, "main", "ring.rsk")
        .await
        .unwrap();
    let old = api::file_commits(&client, &repo, "ring.rsk", "main", 1);
    assert_eq!(old.await.unwrap().len(), 1);
    let new = mock
        .model()
        .repo("lab-notes")
        .remote_commit("ring.rsk", Some(b"{}"));

    let client = signed_in(&mock, LOGIN, cache.path());
    let download = api::download(&client, &repo, "main", "ring.rsk")
        .await
        .unwrap();
    assert_eq!(
        (download.commit, download.bytes),
        (new.clone(), b"{}".to_vec())
    );
    let versions = api::file_commits(&client, &repo, "ring.rsk", "main", 1);
    let versions = versions.await.unwrap();
    assert_eq!((versions.len(), &versions[0].commit), (2, &new));
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
        // A managed user's login has an underscore.
        2 => signed_in(&mock, "mona-cat_octo", cache.path()),
        _ => (Client::public(mock.ends()).unwrap()).cached(Some(cache.path())),
    };
    for n in 0..4 {
        // Never what another account kept.
        assert_eq!(api::blob(&reader(n), &repo, &blob).await.unwrap(), ring);
        assert_eq!(requests(&mock), n + 1);
    }
    for account in [LOGIN, "bob", "mona-cat_octo", "_public"] {
        assert_eq!(kept(cache.path(), account).len(), 1, "{account}");
    }
    // Each reads its own again with no request, signed out too.
    for n in 0..4 {
        assert_eq!(api::blob(&reader(n), &repo, &blob).await.unwrap(), ring);
    }
    assert_eq!(requests(&mock), 4);
    // A login named as signed-out reading is kept nowhere.
    let odd = signed_in(&mock, "_public", cache.path());
    assert_eq!(api::blob(&odd, &repo, &blob).await.unwrap(), ring);
    assert_eq!(requests(&mock), 5);
}

#[tokio::test]
async fn past_the_cap_the_least_recently_used_go_first() {
    let cache = tempfile::tempdir().unwrap();
    // 64 + 2 bytes of head and 100 of reply: two fit, three do not.
    let disk = disk(cache.path()).with_cap(400);
    disk.store(LOGIN, "/a", None, &[1; 100]).await;
    disk.store(LOGIN, "/b", None, &[2; 100]).await;
    // Writing an entry again replaces it: nothing more is kept, nothing goes.
    disk.store(LOGIN, "/b", None, &[2; 100]).await;
    assert!(entry(cache.path(), LOGIN, "/a").exists());
    // Used a minute and a second ago, whatever the clock's resolution.
    let ago = |seconds| std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
    for (path, seconds) in [("/a", 60), ("/b", 1)] {
        let file = entry(cache.path(), LOGIN, path);
        let file = std::fs::File::options().write(true).open(file).unwrap();
        file.set_modified(ago(seconds)).unwrap();
    }
    assert!(disk.load(LOGIN, "/a").await.is_some());
    disk.store("bob", "/c", Some("\"c\""), &[3; 100]).await;
    assert!(disk.load(LOGIN, "/b").await.is_none());
    assert_eq!(disk.load(LOGIN, "/a").await.unwrap().bytes, [1; 100]);
    let c = disk.load("bob", "/c").await.unwrap();
    assert_eq!((c.etag.as_deref(), c.bytes), (Some("\"c\""), vec![3; 100]));
}

#[tokio::test]
async fn an_entry_that_cannot_be_read_whole_is_a_miss_and_removed() {
    let cache = tempfile::tempdir().unwrap();
    let disk = disk(cache.path());
    let damages: [fn(&Path); 4] = [
        |file| std::fs::write(file, b"not an entry").unwrap(),
        // Cut short, as by a crash before the write reached the disk.
        |file| {
            let bytes = std::fs::read(file).unwrap();
            std::fs::write(file, &bytes[..bytes.len() - 1]).unwrap();
        },
        |file| std::fs::write(file, b"").unwrap(),
        // An ETag no request could carry, the reply intact.
        |file| {
            let mut bytes = std::fs::read(file).unwrap();
            bytes[66] = 0;
            std::fs::write(file, bytes).unwrap();
        },
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
    let (mock, cache) = (Mock::start(0).await, tempfile::tempdir().unwrap());
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

#[tokio::test(flavor = "multi_thread")]
async fn writes_at_once_are_each_counted_once() {
    let cache = tempfile::tempdir().unwrap();
    // 16 entries of 166 bytes fit; counted twice, some would go.
    let disk = disk(cache.path()).with_cap(16 * 166 + 100);
    let writes = (0..16).map(|n| {
        let disk = disk.clone();
        tokio::spawn(async move { disk.store(LOGIN, &format!("/{n}"), None, &[1; 100]).await })
    });
    for write in writes.collect::<Vec<_>>() {
        write.await.unwrap();
    }
    assert_eq!(kept(cache.path(), LOGIN).len(), 16);
    assert_eq!(disk.counted_bytes(), Some(16 * 166));
}

#[cfg(unix)]
#[tokio::test]
async fn eviction_never_reaches_through_a_linked_folder() {
    let (cache, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let drawing = outside.path().join("drawing.rsk");
    std::fs::write(&drawing, [0; 500]).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    (std::fs::File::options().write(true).open(&drawing).unwrap())
        .set_modified(old)
        .unwrap();
    let disk = disk(cache.path()).with_cap(400);
    disk.store(LOGIN, "/a", None, &[1; 100]).await;
    let server = entry(cache.path(), LOGIN, "/a");
    let server = server.parent().unwrap().parent().unwrap();
    std::os::unix::fs::symlink(outside.path(), server.join("linked")).unwrap();
    for path in ["/b", "/c"] {
        disk.store(LOGIN, path, None, &[1; 100]).await;
    }
    assert!(drawing.exists());
    assert!(!entry(cache.path(), LOGIN, "/a").exists());
}
