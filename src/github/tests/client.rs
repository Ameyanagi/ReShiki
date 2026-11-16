use crate::github::{
    Error, api,
    client::{Accept, Client},
    credentials::{self, memory},
    mock::Mock,
};
use reqwest::Method;
use std::time::Duration;

const TOKEN_PATH: &str = "/login/oauth/access_token";

#[tokio::test]
async fn every_call_carries_the_sign_in_and_the_api_version() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    api::user(&client).await.unwrap();
    let model = mock.model();
    let headers = &model.log[0].headers;
    assert!(headers["authorization"].starts_with("Bearer access-"));
    assert_eq!(headers["accept"], "application/vnd.github+json");
    assert_eq!(headers["x-github-api-version"], "2022-11-28");
    assert_eq!(
        headers["user-agent"],
        concat!("ReShiki/", env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn signed_out_calls_never_reach_github() {
    let mock = Mock::start(0).await;
    let client = Client::new("mock-client".into(), mock.ends(), None).unwrap();
    assert_eq!(api::user(&client).await, Err(Error::NotSignedIn));
    assert!(mock.model().log.is_empty());
}

#[tokio::test]
async fn a_reply_over_the_limit_is_refused() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    let get = |limit| client.send(Method::GET, "/user", None, Accept::Json, limit);
    assert_eq!(get(5).await, Err(Error::TooLarge(None)));
    assert!(get(64).await.is_ok());
}

#[tokio::test]
async fn each_status_has_one_meaning() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    for (status, error) in [
        (403, Error::NoAccess),
        (404, Error::NotFound),
        (409, Error::Conflict),
        (422, Error::Conflict),
        (429, Error::RateLimited(None)),
        (500, Error::Offline),
        (503, Error::Offline),
        (302, Error::Rejected("GitHub refused the request.".into())),
        (400, Error::Rejected("GitHub refused the request.".into())),
    ] {
        mock.model().fail_next("GET", "/api/user", status);
        assert_eq!(api::user(&client).await, Err(error), "{status}");
    }
    // A redirect is not followed.
    assert_eq!(mock.model().count("GET", "/api/user"), 9);
}

#[tokio::test]
async fn rate_limits_carry_their_wait_and_are_not_no_access() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    let secondary = r#"{"message":"You have exceeded a secondary rate limit."}"#;
    let soon = (crate::github::auth::unix_now() + 600).to_string();
    for (headers, body, wait) in [
        (vec![("Retry-After", "240".to_owned())], "{}", Some(240)),
        (vec![], secondary, None),
        (vec![("X-RateLimit-Remaining", "0".to_owned())], "{}", None),
    ] {
        mock.model()
            .fail_with("GET", "/api/user", 403, headers, body);
        let wait = wait.map(Duration::from_secs);
        assert_eq!(api::user(&client).await, Err(Error::RateLimited(wait)));
    }
    let headers = vec![
        ("X-RateLimit-Remaining", "0".to_owned()),
        ("X-RateLimit-Reset", soon),
    ];
    mock.model()
        .fail_with("GET", "/api/user", 403, headers, "{}");
    let Err(Error::RateLimited(Some(wait))) = api::user(&client).await else {
        panic!("A spent limit names its wait");
    };
    assert!((590..=600).contains(&wait.as_secs()));
    assert_eq!(
        Error::RateLimited(Some(Duration::from_secs(240))).to_string(),
        "Try again in 4 min"
    );
    assert_eq!(
        Error::RateLimited(Some(Duration::from_secs(5))).to_string(),
        "Try again in 1 min"
    );
}

#[tokio::test]
async fn a_refused_token_is_refreshed_once_and_the_call_retried_once() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    client.remember("octo-test").await;
    mock.model().access.clear();
    assert_eq!(api::user(&client).await.as_deref(), Ok("octo-test"));
    assert_eq!(mock.model().count("POST", TOKEN_PATH), 1);
    assert_eq!(mock.model().count("GET", "/api/user"), 2);
    // The rotated pair is the one kept, under the same login.
    let stored = credentials::load().await.unwrap().unwrap();
    assert_eq!(stored.login, "octo-test");
    assert!(mock.model().access.contains(&stored.access));

    // A token GitHub still refuses after a refresh ends the sign-in.
    mock.model().fail_next("GET", "/api/user", 401);
    mock.model().fail_next("GET", "/api/user", 401);
    assert_eq!(api::user(&client).await, Err(Error::SignInExpired));
    assert_eq!(mock.model().count("GET", "/api/user"), 4);
}

#[tokio::test]
async fn concurrent_calls_share_one_refresh() {
    let mock = Mock::start(0).await;
    let mut tokens = mock.tokens();
    tokens.access_expires_at = Some(crate::github::auth::unix_now() + 30);
    let client = Client::new("mock-client".into(), mock.ends(), Some(tokens.clone())).unwrap();
    let (one, two) = tokio::join!(api::user(&client), api::installations(&client));
    assert!(one.is_ok() && two.is_ok());
    assert_eq!(mock.model().count("POST", TOKEN_PATH), 1);
    // Neither call used the token that was about to expire.
    let old = format!("Bearer {}", tokens.access);
    assert!(
        mock.model()
            .log
            .iter()
            .all(|one| one.headers.get("authorization") != Some(&old))
    );
}

#[tokio::test]
async fn a_rejected_refresh_signs_out_and_forgets_the_sign_in() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    assert!(client.remember("octo-test").await);
    mock.model().access.clear();
    mock.model().refresh.clear();
    let error = api::user(&client).await.unwrap_err();
    assert_eq!(error, Error::SignInExpired);
    assert_eq!(credentials::load().await, Ok(None));
    assert_eq!(api::user(&client).await, Err(Error::NotSignedIn));
    assert!(!format!("{error:?} {error}").contains("access-"));
}

#[tokio::test]
async fn a_refresh_that_cannot_reach_github_keeps_the_sign_in() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    client.remember("octo-test").await;
    mock.model().access.clear();
    mock.model().fail_next("POST", TOKEN_PATH, 502);
    assert_eq!(api::user(&client).await, Err(Error::Offline));
    assert!(credentials::load().await.unwrap().is_some());
    // Back online, the same refresh token still works.
    assert_eq!(api::user(&client).await.as_deref(), Ok("octo-test"));
}

#[tokio::test]
async fn a_rotated_pair_that_cannot_be_stored_stays_usable() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    assert!(client.remember("octo-test").await);
    mock.model().access.clear();
    memory::fail(true);
    assert_eq!(api::user(&client).await.as_deref(), Ok("octo-test"));
    assert!(!client.remembered().await);
    // The used refresh token is never sent again.
    assert_eq!(api::user(&client).await.as_deref(), Ok("octo-test"));
    assert_eq!(mock.model().count("POST", TOKEN_PATH), 1);
}

#[tokio::test]
async fn signing_out_during_a_refresh_leaves_nothing_stored() {
    let mock = Mock::start(0).await;
    let mut tokens = mock.tokens();
    tokens.access_expires_at = Some(0);
    let client = Client::new("mock-client".into(), mock.ends(), Some(tokens)).unwrap();
    // The call starts its refresh first; the sign-out waits for it.
    let (call, signed_out) = tokio::join!(api::user(&client), client.sign_out());
    assert!(call.is_ok() && signed_out.is_ok());
    assert_eq!(mock.model().count("POST", TOKEN_PATH), 1);
    assert_eq!(credentials::load().await, Ok(None));
    assert_eq!(api::user(&client).await, Err(Error::NotSignedIn));
}

#[tokio::test]
async fn a_sign_out_that_cannot_forget_says_so() {
    let mock = Mock::start(0).await;
    let client = mock.client();
    client.remember("octo-test").await;
    memory::fail(true);
    assert!(client.sign_out().await.is_err());
    assert_eq!(api::user(&client).await, Err(Error::NotSignedIn));
}
