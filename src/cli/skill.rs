//! The Agent Skill folder `reshiki --cli skill` writes: its file layout and
//! the SKILL.md [`template`].
mod references;
mod template;

use super::{
    args::Skill,
    host::{CliError, labelled},
    io,
};
use serde_json::{Map, json};
use std::io::Write;
use template::{SkillInputs, render_skill_md};

/// The skill's entry point, with the frontmatter and the workflow.
pub(crate) const SKILL_MD: &str = "SKILL.md";
/// Every command's help, verbatim.
pub(crate) const CLI_REFERENCE: &str = "references/cli.md";
/// Each MCP tool's title, hints, parameters and result.
pub(crate) const TOOLS_REFERENCE: &str = "references/mcp-tools.md";
/// Budgets, error codes, file-error reasons, extensions and exit codes.
pub(crate) const LIMITS_REFERENCE: &str = "references/limits-and-errors.md";
/// MCP client setup with the executable filled in.
pub(crate) const SETUP_REFERENCE: &str = "references/setup.md";
/// The Proposal JSON Schema.
pub(crate) const PROPOSAL_SCHEMA: &str = "assets/proposal-schema.json";
/// A complete Proposal.
pub(crate) const PROPOSAL_EXAMPLE: &str = "assets/examples/esterification.json";

/// Every file of a generated skill, relative to its folder.
pub(crate) const FILES: [&str; 7] = [
    SKILL_MD,
    CLI_REFERENCE,
    TOOLS_REFERENCE,
    LIMITS_REFERENCE,
    SETUP_REFERENCE,
    PROPOSAL_SCHEMA,
    PROPOSAL_EXAMPLE,
];

/// Pure rendering, in the same order as FILES; no host or filesystem access.
fn files(inputs: &SkillInputs) -> Vec<(&'static str, Vec<u8>)> {
    [
        (SKILL_MD, render_skill_md(inputs)),
        (CLI_REFERENCE, references::cli()),
        (TOOLS_REFERENCE, references::tools()),
        (LIMITS_REFERENCE, references::limits()),
        (SETUP_REFERENCE, references::setup(&inputs.executable)),
        (PROPOSAL_SCHEMA, format!("{}\n", reshiki_agent::schema())),
        (
            PROPOSAL_EXAMPLE,
            include_str!("skill/esterification.json").into(),
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path, text.into_bytes()))
    .collect()
}

/// Resolve the executable as returned by current_exe, render and write the
/// skill, then optionally publish one labelled receipt. Never canonicalize:
/// Windows verbatim paths are unsuitable for many shells and MCP clients.
pub(crate) fn run(skill: Skill, out: &mut dyn Write) -> Result<(), CliError> {
    let executable = match skill.executable {
        Some(path) => path,
        None => std::env::current_exe().map_err(|error| {
            CliError::Failed(format!(
                "cannot find this executable: {error}; pass --executable PATH"
            ))
        })?,
    };
    let invalid_path = "this executable's path is not UTF-8 or contains control characters; pass --executable PATH";
    let executable = executable
        .to_str()
        .filter(|text| !text.chars().any(char::is_control))
        .ok_or_else(|| CliError::Failed(invalid_path.into()))?;
    let inputs = SkillInputs::live(
        skill.name,
        executable.into(),
        crate::updates::CURRENT_VERSION.into(),
    );
    let path = io::write_skill(&skill.folder, &inputs.name, &files(&inputs), skill.force)
        .map_err(CliError::Failed)?;
    if skill.receipt {
        let receipt = Map::from_iter([(
            "skill".into(),
            json!({
                "path": path.to_string_lossy(), "name": inputs.name, "files": FILES,
            }),
        )]);
        writeln!(out, "{}", labelled(receipt)).map_err(CliError::Stdout)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
