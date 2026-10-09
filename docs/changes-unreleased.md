# Unreleased changes

See [ReShiki 0.11.0](changes-0.11.md) for the latest release notes.

- **Chemical names (under review):** resolve chemical names into editable native structures, or look up a selected molecule's source systematic name and synonyms with explicit online consent and exact chemical-identity checks. Uses OPSIN and PubChem services for the documented supported domain; novel structures may have no source result. [Guide](chemical-naming.md) · [Desktop review and reusable images](chemical-naming-visual-review.md) · [Issue #50](https://github.com/Ameyanagi/ReShiki/issues/50) · @Ameyanagi. PR link will be recorded when published.
- **0.11.0 release validation:** document the tagged source, six-platform release checks, public-download checksums and macOS signing and native-worker verification. [Validation record](release-0.11.0-validation.md) · [PR #187](https://github.com/Ameyanagi/ReShiki/pull/187) · @Ameyanagi.
- **Experimental agent API (Nightly only):** `reshiki --mcp` lets Claude Code, Claude Desktop, Codex and other MCP clients import, analyze, render, compose, edit and export drawings, and save them only into folders you grant; `reshiki --cli` converts, renders, composes and analyzes from a terminal. Not in ReShiki 0.11.0; tools and results may change. [Setup guide](https://reshiki.com/guide/agents/) · [Reference](agent-api.md) · [Privacy](privacy-policy.md#optional-agent-api-and-command-line-experimental) · [Acceptance record](agent-api-p1-validation.md) · @Ameyanagi.
- **Windows signing integration:** verify SignPath-signed x64 and ARM64 applications and installers, and retain internal test-signed nightly artifacts using the existing nightly build. Public Windows downloads remain unsigned while the Foundation release certificate is pending. [Setup guide](signpath-setup.md) · [PR #270](https://github.com/Ameyanagi/ReShiki/pull/270) · @Ameyanagi.

## Chemical names (under review)

Resolve chemical names into editable native structures, or look up a selected
molecule's source systematic name and synonyms with explicit online consent
and exact chemical-identity checks. Contribution: @Ameyanagi; [issue #50](https://github.com/Ameyanagi/ReShiki/issues/50), PR pending publication.

| Before                                                                                       | New editable structure preview                                                                                                |
| -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| ![The old Import panel rejects ethanol as invalid SMILES](images/chemical-naming/before.png) | ![OPSIN ethanol result in an editable native CCO preview, with three atoms and two bonds](images/chemical-naming/preview.png) |

![Complete selected ethanol graph with PubChem CID 702, systematic name, synonyms and source actions](images/chemical-naming/source-lookup.png)

These original desktop examples use the online OPSIN and PubChem services;
structure-to-name retrieves a database record, so novel graphs may have no
result. The [desktop review](chemical-naming-visual-review.md) records differing
capture settings, the saved native graph and reproduction steps.
