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
fn the_heap_budget_is_2_gib_or_whole_mib_from_256_to_16384() {
    const MIB: usize = 1024 * 1024;
    let error: Result<usize, String> =
        Err("RESHIKI_AGENT_HEAP_MB must be a whole number of MiB from 256 to 16384".into());
    for (value, expected) in [
        (None, Ok(2048 * MIB)),
        (Some("256"), Ok(256 * MIB)),
        (Some("4096"), Ok(4096 * MIB)),
        (Some("16384"), Ok(16384 * MIB)),
        (Some("0256"), Ok(256 * MIB)),
        (Some("255"), error.clone()),
        (Some("16385"), error.clone()),
        (Some("0"), error.clone()),
        (Some("1"), error.clone()),
        (Some(""), error.clone()),
        (Some(" 512"), error.clone()),
        (Some("512 "), error.clone()),
        (Some("+512"), error.clone()),
        (Some("-512"), error.clone()),
        (Some("512.0"), error.clone()),
        (Some("1e3"), error.clone()),
        (Some("512MiB"), error.clone()),
        (Some("99999999999999999999999"), error.clone()),
    ] {
        assert_eq!(
            heap_budget(value.map(OsString::from)),
            expected,
            "{value:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_non_utf8_heap_budget_is_refused() {
    use std::os::unix::ffi::OsStringExt;
    assert!(heap_budget(Some(OsString::from_vec(b"512\xff".to_vec()))).is_err());
}
