use crate::github::{
    Error,
    auth::{Poll, parse_code, parse_poll, poll, refresh, request_code},
    config::resolve,
    credentials::Tokens,
    mock::Mock,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn the_code_reply_is_read_and_only_our_site_is_opened() {
    let ends = resolve(None, None);
    let reply = |uri: &str| {
        format!(
            r#"{{"device_code":"secret-device","user_code":"ABCD-1234","verification_uri":"{uri}","expires_in":900,"interval":5}}"#
        )
    };
    let code = parse_code(reply("https://github.com/login/device").as_bytes(), &ends).unwrap();
    assert_eq!(code.user_code, "ABCD-1234");
    assert_eq!(code.interval, Duration::from_secs(5));
    assert_eq!(code.verification_uri, "https://github.com/login/device");
    assert!(!format!("{code:?}").contains("secret-device"));
    for elsewhere in [
        "https://evil.example/login/device",
        "https://github.com.evil.example/x",
    ] {
        let code = parse_code(reply(elsewhere).as_bytes(), &ends).unwrap();
        assert_eq!(code.verification_uri, "https://github.com/login/device");
    }
    assert!(matches!(
        parse_code(b"<html>", &ends),
        Err(Error::Rejected(_))
    ));
}

#[test]
fn every_poll_answer_has_one_meaning() {
    let poll = |json: &str| parse_poll(json.as_bytes(), at(1000));
    assert_eq!(
        poll(r#"{"error":"authorization_pending"}"#),
        Ok(Poll::Pending)
    );
    assert_eq!(
        poll(r#"{"error":"slow_down","interval":10}"#),
        Ok(Poll::SlowDown(Duration::from_secs(10)))
    );
    assert_eq!(poll(r#"{"error":"expired_token"}"#), Ok(Poll::Expired));
    assert_eq!(
        poll(r#"{"error":"incorrect_device_code"}"#),
        Ok(Poll::Expired)
    );
    assert_eq!(poll(r#"{"error":"access_denied"}"#), Ok(Poll::Denied));
    for refused in [
        r#"{"error":"device_flow_disabled"}"#,
        "{}",
        "not json",
        r#"{"access_token":""}"#,
    ] {
        assert!(
            matches!(poll(refused), Err(Error::Rejected(_))),
            "{refused}"
        );
    }
    let Ok(Poll::Done(tokens)) = poll(
        r#"{"access_token":"a","expires_in":28800,"refresh_token":"r","refresh_token_expires_in":100,"token_type":"bearer","scope":""}"#,
    ) else {
        panic!("A full reply signs in");
    };
    assert_eq!(tokens.access_expires_at, Some(29800));
    assert_eq!(tokens.refresh.as_deref(), Some("r"));
    assert_eq!(tokens.refresh_expires_at, Some(1100));
    let Ok(Poll::Done(tokens)) = poll(r#"{"access_token":"a","token_type":"bearer"}"#) else {
        panic!("A reply without expiry signs in");
    };
    assert_eq!((tokens.access_expires_at, tokens.refresh), (None, None));
}

#[test]
fn tokens_never_print() {
    let tokens = Tokens {
        login: "octo-test".into(),
        access: "secret-access".into(),
        access_expires_at: Some(1),
        refresh: Some("secret-refresh".into()),
        refresh_expires_at: Some(2),
    };
    let printed = format!("{tokens:?} {:?}", Poll::Done(tokens.clone()));
    assert!(
        printed.contains("octo-test") && !printed.contains("secret"),
        "{printed}"
    );
}

#[tokio::test]
async fn the_device_flow_waits_slows_down_and_signs_in() {
    let mock = Mock::start(0).await;
    let ends = mock.ends();
    mock.model().script.extend(["pending", "slow_down", "ok"]);
    let code = request_code("mock-client", &ends).await.unwrap();
    assert_eq!(code.user_code, "ABCD-1234");
    assert_eq!(code.verification_uri, format!("{}/login/device", mock.base));
    assert_eq!(poll("mock-client", &ends, &code).await, Ok(Poll::Pending));
    assert_eq!(
        poll("mock-client", &ends, &code).await,
        Ok(Poll::SlowDown(Duration::from_secs(10)))
    );
    let Ok(Poll::Done(tokens)) = poll("mock-client", &ends, &code).await else {
        panic!("An approved code signs in");
    };
    assert!(mock.model().access.contains(&tokens.access));
    assert!(tokens.access_expires_at.is_some() && tokens.refresh.is_some());

    let model = mock.model();
    let sent = String::from_utf8(model.log[1].body.clone()).unwrap();
    assert!(
        sent.contains("urn:ietf:params:oauth:grant-type:device_code") && sent.contains("device-1")
    );
    for request in &model.log {
        assert_eq!(request.headers["accept"], "application/json");
        assert!(!request.headers.contains_key("authorization"));
        assert!(!String::from_utf8_lossy(&request.body).contains("secret"));
    }
}

#[tokio::test]
async fn an_expired_or_refused_code_ends_the_sign_in() {
    let mock = Mock::start(0).await;
    let ends = mock.ends();
    mock.model()
        .script
        .extend(["expired", "denied", "incorrect"]);
    let code = request_code("mock-client", &ends).await.unwrap();
    assert_eq!(poll("mock-client", &ends, &code).await, Ok(Poll::Expired));
    assert_eq!(poll("mock-client", &ends, &code).await, Ok(Poll::Denied));
    assert_eq!(poll("mock-client", &ends, &code).await, Ok(Poll::Expired));
    // The page the browser opens approves the code.
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(&code.verification_uri)
        .send()
        .await
        .unwrap();
    assert!(matches!(
        poll("mock-client", &ends, &code).await,
        Ok(Poll::Done(_))
    ));
    mock.model()
        .fail_next("POST", "/login/oauth/access_token", 503);
    assert_eq!(poll("mock-client", &ends, &code).await, Err(Error::Offline));
}

#[tokio::test]
async fn refresh_rotates_both_tokens_without_a_secret() {
    let mock = Mock::start(0).await;
    let ends = mock.ends();
    let old = mock.tokens();
    let old_refresh = old.refresh.unwrap();
    let fresh = refresh("mock-client", &ends, &old_refresh).await.unwrap();
    assert_ne!(fresh.access, old.access);
    assert_ne!(fresh.refresh.as_deref(), Some(old_refresh.as_str()));
    assert!(mock.model().access.contains(&fresh.access));
    let body: serde_json::Value = serde_json::from_slice(&mock.model().log[0].body).unwrap();
    let mut sent: Vec<&String> = body.as_object().unwrap().keys().collect();
    sent.sort();
    assert_eq!(sent, ["client_id", "grant_type", "refresh_token"]);
    // GitHub retires a used refresh token.
    assert_eq!(
        refresh("mock-client", &ends, &old_refresh).await,
        Err(Error::SignInExpired)
    );
}

#[tokio::test]
async fn a_web_only_override_on_this_computer_still_signs_in() {
    let mock = Mock::start(0).await;
    let ends = resolve(None, Some(mock.base.clone()));
    assert_eq!(ends.api, "https://api.github.com");
    assert!(request_code("mock-client", &ends).await.is_ok());
}
