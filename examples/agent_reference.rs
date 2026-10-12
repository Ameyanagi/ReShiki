//! Rebuild the agent reference from the CLI and operation contracts.
//! cargo run --example agent_reference -- docs/agent-reference.md
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .context("Usage: agent_reference OUTPUT.md")?;
    std::fs::write(path, reshiki::cli::agent_reference())?;
    Ok(())
}
