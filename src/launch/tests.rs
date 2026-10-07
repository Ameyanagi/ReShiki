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
