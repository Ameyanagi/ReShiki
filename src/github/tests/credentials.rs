use crate::github::{
    api,
    client::Client,
    credentials::{self, Tokens, memory},
    mock::Mock,
};

fn tokens() -> Tokens {
    Tokens {
        login: "octo-test".into(),
        access: "a".into(),
        access_expires_at: Some(10),
        refresh: Some("r".into()),
        refresh_expires_at: None,
    }
}

#[tokio::test]
async fn the_sign_in_is_stored_loaded_and_deleted() {
    assert_eq!(credentials::load().await, Ok(None));
    credentials::store(&tokens()).await.unwrap();
    assert_eq!(credentials::load().await, Ok(Some(tokens())));
    credentials::delete().await.unwrap();
    assert_eq!(credentials::load().await, Ok(None));
    // Deleting nothing is not an error.
    credentials::delete().await.unwrap();
}

#[tokio::test]
async fn a_store_that_fails_still_signs_in_for_this_run() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    memory::fail(true);
    assert!(credentials::load().await.is_err());
    let mut fresh = mock.tokens();
    fresh.login.clear();
    let client = Client::new("mock-client".into(), mock.ends(), Some(fresh)).unwrap();
    let account = api::account(&client).await.unwrap();
    assert_eq!(account.login, "octo-test");
    assert!(!client.remember(&account.login).await && !client.remembered().await);
    // The sign-in still works.
    assert_eq!(api::user(&client).await.as_deref(), Ok("octo-test"));
    // Signing out has nothing to forget, so the store is not asked.
    assert_eq!(client.sign_out().await, Ok(()));
    memory::fail(false);
    assert_eq!(credentials::load().await, Ok(None));
}

#[tokio::test]
async fn a_working_store_remembers_the_sign_in_with_its_login() {
    let mock = Mock::start(0).await;
    mock.model().seed_demo();
    let mut fresh = mock.tokens();
    fresh.login.clear();
    let client = Client::new("mock-client".into(), mock.ends(), Some(fresh.clone())).unwrap();
    // Naming the user keeps nothing: a canceled sign-in is never stored.
    let account = api::account(&client).await.unwrap();
    assert_eq!(credentials::load().await, Ok(None));
    assert!(client.remember(&account.login).await);
    fresh.login = "octo-test".into();
    assert_eq!(credentials::load().await, Ok(Some(fresh)));
    assert_eq!(client.sign_out().await, Ok(()));
    assert_eq!(credentials::load().await, Ok(None));
}

/// By hand on macOS, Windows x64 and Linux x64: the real Keychain, Credential
/// Manager or Secret Service, under a test entry of its own.
#[tokio::test]
#[ignore = "Writes to this computer's credential store"]
async fn the_operating_system_store_round_trips() {
    credentials::os_delete().await.unwrap();
    assert_eq!(credentials::os_read().await, Ok(None));
    let bytes = serde_json::to_vec(&tokens()).unwrap();
    credentials::os_write(bytes.clone()).await.unwrap();
    credentials::os_write(bytes.clone()).await.unwrap();
    assert_eq!(credentials::os_read().await, Ok(Some(bytes)));
    credentials::os_delete().await.unwrap();
    assert_eq!(credentials::os_read().await, Ok(None));
    credentials::os_delete().await.unwrap();
}
