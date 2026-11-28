//! Normal local integration entry point. Artifact CI uses the worker example
//! so Cargo cannot rebuild the executable being verified.
#![cfg(not(target_arch = "wasm32"))]

#[path = "common/xtb_packaged.rs"]
mod packaged;

#[test]
fn relocated_packaged_executable_runs_all_xtb_methods_and_cli_optimization() {
    let source = std::env::var_os("RESHIKI_TEST_PACKAGED_APP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_reshiki").into());
    packaged::check_relocated_app(&source);
}
