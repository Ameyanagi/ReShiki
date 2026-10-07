//! Only a first token of exactly `--mcp` or `--cli` enters a headless mode.
//! No test here spawns an argv that reaches the GUI.
#[path = "common/headless.rs"]
mod headless;

use std::{
    process::{Output, Stdio},
    time::Duration,
};

fn run(args: &[&str], limit: Duration) -> Output {
    headless::wait_with_watchdog(headless::spawn(args, Stdio::null()), limit)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn cli_info_prints_json() {
    let output = run(&["--cli", "info"], Duration::from_secs(60));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).expect("info JSON");
    assert_eq!(info["api"]["stability"], "experimental");
}

#[test]
fn cli_without_a_command_is_a_usage_error() {
    let output = run(&["--cli"], Duration::from_secs(60));
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    assert!(text(&output.stderr).starts_with("Experimental:"));
}

#[test]
fn unknown_cli_commands_are_usage_errors() {
    let output = run(&["--cli", "bogus"], Duration::from_secs(60));
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("unknown command `bogus`"));
}

#[test]
fn mcp_is_a_placeholder() {
    let output = run(&["--mcp"], Duration::from_secs(60));
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        text(&output.stderr),
        "reshiki --mcp: not available in this build\n"
    );
}

/// The same ethanol check as scripts/check_runtime_dependencies.py.
#[test]
fn a_later_headless_token_never_diverts_the_engine_check() {
    for args in [
        &["--engine-check", "--mcp"][..],
        &["--engine-check", "--cli", "info"],
    ] {
        let output = run(args, Duration::from_secs(120));
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            text(&output.stderr)
        );
        let response: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("engine response JSON");
        assert_eq!(response["analysis"]["formula"], "C2H6O", "{args:?}");
        assert_eq!(response["analysis"]["smiles"], "CCO", "{args:?}");
    }
}

/// A process left holding the output pipes cannot outlast the watchdog.
#[cfg(unix)]
#[test]
#[should_panic(expected = "did not exit and close its output")]
fn the_watchdog_bounds_output_held_open_by_a_descendant() {
    let child = std::process::Command::new("sh")
        .args(["-c", "sleep 5 & exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start sh");
    headless::wait_with_watchdog(child, Duration::from_millis(200));
}

#[cfg(windows)]
#[test]
fn a_later_cli_token_never_diverts_graphics_info() {
    let output = run(&["--graphics-info", "--cli"], Duration::from_secs(60));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let info: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("graphics info JSON");
    assert!(info.get("automatic_preference").is_some(), "{info}");
}
