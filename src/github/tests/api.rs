use crate::github::{
    Entry, Error, RepoKey,
    api::{self, Folders},
    mock::{LOGIN, Mock},
};

fn key(repo: &str) -> RepoKey {
    RepoKey {
        owner: LOGIN.into(),
        repo: repo.into(),
    }
}

fn names(entries: &[Entry]) -> Vec<(&str, bool)> {
    entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.folder))
        .collect()
}

#[tokio::test]
async fn the_account_lists_every_repository_across_pages() {
    let mock = Mock::start(0).await;
    for n in 0..150 {
        mock.model().add_repo(&format!("repo-{n}")).push = n != 0;
    }
    let account = api::account(&mock.client()).await.unwrap();
    assert_eq!(account.login, LOGIN);
    assert_eq!(account.repos.len(), 150);
    assert!(!account.needs_install());
    let first = &account.repos[0];
    assert_eq!(
        (first.key.clone(), first.id, first.private, first.push),
        (key("repo-0"), 1, true, false)
    );
    assert!(account.repos[1].push);
    assert_eq!(first.default_branch, "main");
    assert_eq!(
        mock.model()
            .count("GET", "/api/user/installations/1/repositories"),
        2
    );
}

#[tokio::test]
async fn no_reachable_repository_means_the_app_needs_installing() {
    let mock = Mock::start(0).await;
    let account = api::account(&mock.client()).await.unwrap();
    assert!(account.needs_install());
    // Names no folder here could have are left out.
    mock.model().add_repo("what?");
    mock.model().add_repo("con");
    assert!(api::account(&mock.client()).await.unwrap().needs_install());
}

#[tokio::test]
async fn a_new_repository_is_private_and_ready_to_send_to() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    let repo = api::create_repo(&client, "schemes").await.unwrap();
    assert_eq!(repo.key, key("schemes"));
    assert!(repo.private && repo.push);
    let sent: serde_json::Value = serde_json::from_slice(&mock.model().log[0].body).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({ "name": "schemes", "private": true, "auto_init": true })
    );
    assert!(
        api::head(&client, &repo.key, &repo.default_branch)
            .await
            .is_ok()
    );
    assert_eq!(
        api::create_repo(&client, "schemes").await,
        Err(Error::Conflict)
    );
}

#[tokio::test]
async fn a_folder_lists_its_drawings_previews_and_folders_only() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    mock.model()
        .repo("lab-notes")
        .remote_commit("README.md", Some(b"notes"));
    mock.model()
        .repo("lab-notes")
        .remote_commit("scheme/deep/x.rsk", Some(b"x"));
    let (client, repo) = (mock.client(), key("lab-notes"));
    let head = api::head(&client, &repo, "main").await.unwrap();
    assert_eq!(
        Some(head.commit.clone()),
        mock.model().repo("lab-notes").head()
    );
    let top = api::tree(&client, &repo, &head.tree).await.unwrap();
    assert_eq!(
        names(&top),
        [("ring.rsk", false), ("ring.svg", false), ("scheme", true)]
    );

    let mut folders = Folders::new(&client, &repo, head.tree);
    let deep = folders.list("scheme/deep").await.unwrap().unwrap();
    assert_eq!(names(deep), [("x.rsk", false)]);
    let scheme = folders.list("scheme").await.unwrap().unwrap();
    assert_eq!(names(scheme), [("route.rsk", false), ("deep", true)]);
    assert_eq!(folders.list("nowhere/else").await, Ok(None));
    assert_eq!(folders.list("ring.rsk").await, Ok(None));
    assert!(folders.blob("scheme/route.rsk").await.unwrap().is_some());
    assert_eq!(folders.blob("scheme/deep").await, Ok(None));
    // Each folder was read once.
    let prefix = format!("/api/repos/{LOGIN}/lab-notes/git/trees/");
    assert_eq!(mock.model().count("GET", &prefix), 4);
}

#[tokio::test]
async fn a_drawing_downloads_with_its_blob_and_commit() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    let (client, repo) = (mock.client(), key("lab-notes"));
    let got = api::download(&client, &repo, "main", "scheme/route.rsk")
        .await
        .unwrap();
    assert_eq!(
        got.bytes,
        mock.model().repo("lab-notes").files()["scheme/route.rsk"]
    );
    assert_eq!(got.blob, crate::github::mock::sha(&got.bytes));
    assert_eq!(Some(got.commit), mock.model().repo("lab-notes").head());
    // The file's own bytes are asked for, not base64 in JSON.
    let accept = {
        let model = mock.model();
        let blob = model
            .log
            .iter()
            .find(|one| one.path.contains("/git/blobs/"));
        blob.unwrap().headers["accept"].clone()
    };
    assert_eq!(accept, "application/vnd.github.raw+json");
    let gone = api::download(&client, &repo, "main", "scheme/gone.rsk").await;
    assert_eq!(gone, Err(Error::NotFound));
}

#[tokio::test]
async fn an_empty_repository_and_a_missing_branch_differ() {
    let mock = Mock::start(0).await;
    mock.model().add_repo("empty");
    mock.model().seed_demo();
    let client = mock.client();
    assert_eq!(
        api::head(&client, &key("empty"), "main").await,
        Err(Error::Conflict)
    );
    assert_eq!(
        api::head(&client, &key("lab-notes"), "trunk").await,
        Err(Error::NotFound)
    );
    api::init_readme(&client, &key("empty")).await.unwrap();
    assert!(api::head(&client, &key("empty"), "main").await.is_ok());
    assert_eq!(
        mock.model().repo("empty").files()["README.md"],
        b"# empty\n"
    );
}

#[tokio::test]
async fn a_repository_is_found_by_id_after_a_rename() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    let client = mock.client();
    mock.model().repo("lab-notes").name = "lab-book".into();
    mock.model().repo("lab-book").default_branch = "trunk".into();
    let repo = api::repo(&client, 1).await.unwrap();
    assert_eq!(
        (repo.key, repo.default_branch.as_str()),
        (key("lab-book"), "trunk")
    );
    assert_eq!(api::repo(&client, 9).await, Err(Error::NotFound));
}

#[tokio::test]
async fn a_drawing_has_its_commits_and_its_bytes_at_each() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    let path = "scheme/my route (2).rsk";
    let first = mock
        .model()
        .repo("lab-notes")
        .remote_commit(path, Some(b"one"));
    mock.model()
        .repo("lab-notes")
        .remote_commit("ring.rsk", Some(b"other"));
    let second = mock
        .model()
        .repo("lab-notes")
        .remote_commit(path, Some(b"two"));
    let deleted = mock.model().repo("lab-notes").remote_commit(path, None);
    let (client, repo) = (mock.client(), key("lab-notes"));

    let versions = api::file_commits(&client, &repo, path, &deleted, 1)
        .await
        .unwrap();
    let commits: Vec<&str> = versions
        .iter()
        .map(|version| version.commit.as_str())
        .collect();
    assert_eq!(commits, [&deleted, &second, &first]);
    let newest = &versions[0];
    assert_eq!(newest.message, format!("Remote change to {path}"));
    assert_eq!(newest.login.as_deref(), Some(LOGIN));
    assert_eq!(
        (newest.author.as_str(), newest.date.as_str()),
        ("Octo Test", "2026-11-15T14:02:00Z")
    );
    // Counted from the head given, however the branch moves later.
    let earlier = api::file_commits(&client, &repo, path, &first, 1)
        .await
        .unwrap();
    assert_eq!(earlier.len(), 1);
    assert_eq!(
        api::file_commits(&client, &repo, path, &deleted, 2).await,
        Ok(vec![])
    );

    assert_eq!(
        api::raw_at(&client, &repo, path, &first).await.unwrap(),
        b"one"
    );
    assert_eq!(
        api::raw_at(&client, &repo, path, &second).await.unwrap(),
        b"two"
    );
    assert_eq!(
        api::raw_at(&client, &repo, path, &deleted).await,
        Err(Error::NotFound)
    );
}

#[tokio::test]
async fn a_star_is_one_put() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    api::star(&client, "Ameyanagi", "ReShiki").await.unwrap();
    assert!(mock.model().starred.contains("Ameyanagi/ReShiki"));
    assert_eq!(mock.model().log[0].method, "PUT");
    mock.model()
        .fail_next("PUT", "/api/user/starred/Ameyanagi/ReShiki", 403);
    assert_eq!(
        api::star(&client, "Ameyanagi", "ReShiki").await,
        Err(Error::NoAccess)
    );
}

#[tokio::test]
async fn a_star_check_reads_204_as_starred_and_404_as_not() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    assert_eq!(
        api::starred(&client, "Ameyanagi", "ReShiki").await,
        Ok(false)
    );
    api::star(&client, "Ameyanagi", "ReShiki").await.unwrap();
    assert_eq!(
        api::starred(&client, "Ameyanagi", "ReShiki").await,
        Ok(true)
    );
    mock.model()
        .fail_next("GET", "/api/user/starred/Ameyanagi/ReShiki", 500);
    assert_eq!(
        api::starred(&client, "Ameyanagi", "ReShiki").await,
        Err(Error::Offline)
    );
}

#[tokio::test]
async fn an_object_name_that_is_not_hex_is_refused() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    mock.model()
        .repo("lab-notes")
        .refs
        .insert("main".into(), "../../user".into());
    let head = api::head(&mock.client(), &key("lab-notes"), "main").await;
    assert!(matches!(head, Err(Error::Rejected(_))));
}

fn alice(repo: &str) -> RepoKey {
    RepoKey {
        owner: "alice".into(),
        repo: repo.into(),
    }
}

#[tokio::test]
async fn signed_out_a_public_repository_lists_without_a_token() {
    let mock = Mock::start(0).await;
    mock.model().seed_shared();
    let client = crate::github::client::Client::public(mock.ends()).unwrap();
    let repo = api::repo_named(&client, &alice("ligands")).await.unwrap();
    // Signed out, nothing may be sent: the repository is read only.
    assert!(!repo.push && !repo.private);
    let head = api::head(&client, &repo.key, &repo.default_branch)
        .await
        .unwrap();
    let top = api::tree(&client, &repo.key, &head.tree).await.unwrap();
    assert_eq!(
        names(&top),
        [("index.rsk", false), ("aromatics", true), ("esters", true)]
    );
    assert!(
        mock.model()
            .log
            .iter()
            .all(|one| !one.headers.contains_key("authorization"))
    );
    // A private repository is not found, signed out or not its owner's.
    assert_eq!(
        api::repo_named(&client, &alice("notes")).await,
        Err(Error::NotFound)
    );
    assert_eq!(
        api::repo_named(&mock.client(), &alice("notes")).await,
        Err(Error::NotFound)
    );
    // Signed in, someone else's public repository is read only too.
    let signed_in = api::repo_named(&mock.client(), &alice("ligands"))
        .await
        .unwrap();
    assert!(!signed_in.push);
    // Calls that need a sign-in say so.
    assert_eq!(api::user(&client).await, Err(Error::NotSignedIn));
}

#[tokio::test]
async fn signed_out_the_hourly_limit_is_a_rate_limit() {
    let mock = Mock::start(0).await;
    mock.model().seed_shared();
    mock.model().signed_out_left = Some(1);
    let client = crate::github::client::Client::public(mock.ends()).unwrap();
    assert!(api::repo_named(&client, &alice("ligands")).await.is_ok());
    let Err(Error::RateLimited(Some(wait))) = api::repo_named(&client, &alice("ligands")).await
    else {
        panic!("not rate limited");
    };
    assert!(wait.as_secs() > 60);
    // A signed-in user is not held to it.
    assert!(
        api::repo_named(&mock.client(), &alice("ligands"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_branch_or_commit_downloads_the_file_as_it_was_there() {
    let mock = Mock::start(0).await;
    mock.model().seed_shared();
    let draft = mock.model().repo("ligands").refs["draft"].clone();
    let client = crate::github::client::Client::public(mock.ends()).unwrap();
    let key = alice("ligands");
    let by_branch = api::commit(&client, &key, "draft").await.unwrap();
    assert_eq!(by_branch.commit, draft);
    assert_eq!(api::commit(&client, &key, &draft).await, Ok(by_branch));
    let old = api::download_at(&client, &key, "draft", "esters/esterification.rsk")
        .await
        .unwrap();
    assert_eq!(old.commit, draft);
    // `furan.rsk` came after the branch.
    assert_eq!(
        api::download_at(&client, &key, "draft", "aromatics/furan.rsk").await,
        Err(Error::NotFound)
    );
    assert_eq!(
        api::commit(&client, &key, "no-such-branch").await,
        Err(Error::NotFound)
    );
}
