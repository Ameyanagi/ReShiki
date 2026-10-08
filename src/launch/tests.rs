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
    let serve = |level| {
        Ok(McpRequest::Serve {
            level,
            read: Vec::new(),
            write: Vec::new(),
        })
    };
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

#[test]
fn grant_folders_repeat_and_keep_their_order() {
    use McpUsageError::{Attach, Duplicate, MissingFolder, MissingLevel, Unknown};
    let folders = |tokens: &[&str]| tokens.iter().map(PathBuf::from).collect::<Vec<_>>();
    let serve = |level, read: &[&str], write: &[&str]| {
        Ok(McpRequest::Serve {
            level,
            read: folders(read),
            write: folders(write),
        })
    };
    for (tokens, expected) in [
        (
            &["--allow-read", "/in"][..],
            serve(Level::Warn, &["/in"], &[]),
        ),
        (&["--allow-write", "out"], serve(Level::Warn, &[], &["out"])),
        (
            &[
                "--allow-read",
                "/a",
                "--allow-write",
                "/w",
                "--allow-read",
                "b",
                "--log-level",
                "debug",
                "--allow-read",
                "/a",
            ],
            serve(Level::Debug, &["/a", "b", "/a"], &["/w"]),
        ),
        // A folder is any token, even one that looks like an option.
        (
            &["--allow-read", "--log-level"],
            serve(Level::Warn, &["--log-level"], &[]),
        ),
        (&["--allow-read", "/in", "--help"], Ok(McpRequest::Help)),
        (&["--allow-read"], Err(MissingFolder("--allow-read"))),
        (&["--allow-write"], Err(MissingFolder("--allow-write"))),
        (&["--allow-read", ""], Err(MissingFolder("--allow-read"))),
        (
            &["--allow-write", "/w", "--allow-write", ""],
            Err(MissingFolder("--allow-write")),
        ),
        (&["--allow-read", "/in", "--attach"], Err(Attach)),
        (&["--allow-read", "/in", "--log-level"], Err(MissingLevel)),
        (
            &["--help", "--allow-write", "/w", "--help"],
            Err(Duplicate("--help")),
        ),
        (
            &["--allow-read=/in"],
            Err(Unknown("--allow-read=/in".into())),
        ),
        (&["--allow-dir", "/in"], Err(Unknown("--allow-dir".into()))),
    ] {
        assert_eq!(mcp_request(&args(tokens)), expected, "{tokens:?}");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_folders_are_kept_as_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let folder = OsString::from_vec(b"/in\xff".to_vec());
    assert_eq!(
        mcp_request(&[OsString::from("--allow-read"), folder.clone()]),
        Ok(McpRequest::Serve {
            level: Level::Warn,
            read: vec![PathBuf::from(folder)],
            write: Vec::new(),
        })
    );
}

#[test]
fn the_banner_counts_folders_and_lists_them_only_at_debug() {
    let grants = GrantSummary {
        read: vec!["/in".into(), "/a \"b\"\nc".into()],
        write: vec!["/out".into()],
    };
    for level in [Level::Error, Level::Warn, Level::Info] {
        assert_eq!(
            banner("1.2.3", &grants, level),
            "1.2.3; granted folders: 2 read, 1 write",
            "{level:?}"
        );
    }
    assert_eq!(
        banner("1.2.3", &grants, Level::Debug),
        r#"1.2.3; granted folders: read ["/in", "/a \"b\"\nc"], write ["/out"]"#
    );
    let none = GrantSummary {
        read: Vec::new(),
        write: Vec::new(),
    };
    assert_eq!(
        banner("1.2.3", &none, Level::Warn),
        "1.2.3; granted folders: 0 read, 0 write"
    );
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
    assert_eq!(
        McpUsageError::MissingFolder("--allow-write").to_string(),
        "reshiki --mcp: --allow-write needs a folder"
    );
    assert!(MCP_USAGE.starts_with("Experimental: "));
    assert!(MCP_USAGE.contains(
        "Usage: reshiki --mcp [--log-level <level>] [--allow-read <folder>]... \
         [--allow-write <folder>]...\n"
    ));
}

#[test]
fn mcp_exits_0_only_when_every_request_was_answered() {
    let log = Log::silent();
    for (reason, delivered, unanswered, code) in [
        (Quit::Eof, true, 0, 0),
        (Quit::Eof, true, 1, 1),
        (Quit::Eof, false, 0, 1),
        (Quit::Eof, false, 2, 1),
        (Quit::WriterFailed, true, 0, 1),
        (Quit::WriterFailed, false, 3, 1),
    ] {
        assert_eq!(
            exit_code(reason, delivered, unanswered, &log),
            code,
            "{reason:?} delivered={delivered} unanswered={unanswered}"
        );
    }
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
