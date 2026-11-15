use crate::github::config::{configured, resolve};

#[test]
fn an_empty_or_absent_client_id_means_not_set_up() {
    assert_eq!(configured(None, ""), None);
    assert_eq!(configured(Some(String::new()), "built-in"), None);
    assert_eq!(configured(None, "built-in").as_deref(), Some("built-in"));
    assert_eq!(
        configured(Some("env".into()), "built-in").as_deref(),
        Some("env")
    );
}

#[test]
fn github_is_the_default_and_keeps_the_credential_store() {
    let ends = resolve(None, None);
    assert_eq!(ends.api("/user"), "https://api.github.com/user");
    assert_eq!(ends.web("/login/device"), "https://github.com/login/device");
    assert!(!ends.overridden);
}

#[test]
fn overrides_must_be_https_or_this_computer() {
    for address in [
        "https://ghe.example/api/v3/",
        "http://127.0.0.1:47123/api",
        "http://localhost:47123",
        "http://[::1]:47123",
    ] {
        let ends = resolve(Some(address.into()), None);
        assert_eq!(ends.api, address.trim_end_matches('/'));
        assert_eq!(ends.web, "https://github.com");
    }
    for address in [
        "http://example.com",
        "http://127.0.0.1.example.com",
        "ftp://localhost",
        "nonsense",
    ] {
        let ends = resolve(None, Some(address.into()));
        assert_eq!(ends.web, "https://github.com", "{address}");
        // Even a refused override keeps the real sign-in out of reach.
        assert!(ends.overridden);
    }
}

#[test]
fn any_override_keeps_the_sign_in_in_memory() {
    assert!(resolve(Some("http://127.0.0.1:1/api".into()), None).overridden);
    assert!(resolve(None, Some("http://127.0.0.1:1".into())).overridden);
}
