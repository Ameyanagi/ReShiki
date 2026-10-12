//! The Agent Skill folder `reshiki --cli skill` writes: its file layout and
//! the SKILL.md [`template`].
mod template;

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
