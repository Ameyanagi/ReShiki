use super::*;

fn args(tokens: &[&str]) -> Vec<OsString> {
    tokens.iter().map(OsString::from).collect()
}

#[test]
fn only_an_exact_first_headless_token_leaves_the_gui_route() {
    for tokens in [
        &[][..],
        &["--open", "a.rsk"],
        &["--open", "--mcp"],
        &["--open", "--cli"],
        &["--open", "a", "--mcp"],
        &["--engine-check", "--mcp"],
        &["--shortcut-examples", "--cli"],
        &["--office-edit", "--open", "x"],
        &["--graphics-info", "--mcp"],
        &["--ole-server", "-Embedding", "--cli"],
        &["-psn_0_123"],
        &["mcp"],
        &["cli"],
        &["--mcp=1"],
        &["--MCP"],
        &[" --mcp"],
    ] {
        assert_eq!(mode(args(tokens)), Launch::Gui, "{tokens:?}");
    }
}

#[test]
fn headless_modes_keep_the_rest_verbatim() {
    for (tokens, expected) in [
        (&["--mcp"][..], Launch::Mcp(Vec::new())),
        (
            &["--mcp", "--log-level", "debug"],
            Launch::Mcp(args(&["--log-level", "debug"])),
        ),
        (
            &["--mcp", "--open", "x"],
            Launch::Mcp(args(&["--open", "x"])),
        ),
        (&["--cli"], Launch::Cli(Vec::new())),
        (&["--cli", "info"], Launch::Cli(args(&["info"]))),
    ] {
        assert_eq!(mode(args(tokens)), expected, "{tokens:?}");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_tokens_are_compared_and_kept_as_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let token = OsString::from_vec(b"--mcp\xff".to_vec());
    assert_eq!(mode([token.clone()]), Launch::Gui);
    assert_eq!(
        mode([OsString::from("--cli"), token.clone()]),
        Launch::Cli(vec![token])
    );
}

#[test]
fn mcp_options_follow_the_grammar() {
    use McpUsageError::{Attach, Duplicate, InvalidLevel, MissingLevel, Unknown};
    let serve = |level| Ok(McpRequest::Serve { level });
    for (tokens, expected) in [
        (&[][..], serve(Level::Warn)),
        (&["--log-level", "error"], serve(Level::Error)),
        (&["--log-level", "warn"], serve(Level::Warn)),
        (&["--log-level", "info"], serve(Level::Info)),
        (&["--log-level", "debug"], serve(Level::Debug)),
        (&["--help"], Ok(McpRequest::Help)),
        (&["-h"], Ok(McpRequest::Help)),
        (&["--log-level", "debug", "--help"], Ok(McpRequest::Help)),
        (&["--help", "--log-level", "info"], Ok(McpRequest::Help)),
        (&["--log-level"], Err(MissingLevel)),
        (&["--log-level", "loud"], Err(InvalidLevel("loud".into()))),
        (&["--log-level", "WARN"], Err(InvalidLevel("WARN".into()))),
        (&["--log-level", ""], Err(InvalidLevel(String::new()))),
        (
            &["--log-level=debug"],
            Err(Unknown("--log-level=debug".into())),
        ),
        (
            &["--log-level", "warn", "--log-level", "debug"],
            Err(Duplicate("--log-level")),
        ),
        (&["--help", "--help"], Err(Duplicate("--help"))),
        (&["-h", "--help"], Err(Duplicate("--help"))),
        (&["--attach"], Err(Attach)),
        (&["--log-level", "info", "--attach"], Err(Attach)),
        (&["--help", "--attach"], Err(Attach)),
        (&["--bogus"], Err(Unknown("--bogus".into()))),
        (&["--help", "--bogus"], Err(Unknown("--bogus".into()))),
        (&["--cli"], Err(Unknown("--cli".into()))),
        (&["--mcp"], Err(Unknown("--mcp".into()))),
        (&["--open", "x"], Err(Unknown("--open".into()))),
        (&["debug"], Err(Unknown("debug".into()))),
    ] {
        assert_eq!(mcp_request(&args(tokens)), expected, "{tokens:?}");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_mcp_tokens_are_usage_errors() {
    use std::os::unix::ffi::OsStringExt;
    let token = OsString::from_vec(b"debug\xff".to_vec());
    assert_eq!(
        mcp_request(&[OsString::from("--log-level"), token.clone()]),
        Err(McpUsageError::InvalidLevel("debug\u{fffd}".into()))
    );
    assert_eq!(
        mcp_request(&[token]),
        Err(McpUsageError::Unknown("debug\u{fffd}".into()))
    );
}

#[test]
fn mcp_usage_errors_name_the_problem() {
    assert_eq!(
        McpUsageError::Attach.to_string(),
        "reshiki --mcp --attach connects to a running ReShiki app and is not available in this \
         version"
    );
    assert_eq!(
        McpUsageError::InvalidLevel("loud".into()).to_string(),
        "reshiki --mcp: unknown log level `loud`; expected one of error, warn, info or debug"
    );
    assert_eq!(
        McpUsageError::Duplicate("--log-level").to_string(),
        "reshiki --mcp: --log-level given more than once"
    );
    assert!(MCP_USAGE.starts_with("Experimental: "));
    assert!(MCP_USAGE.contains("Usage: reshiki --mcp [--log-level <level>]\n"));
}
