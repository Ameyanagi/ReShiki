# Unreleased changes

See [ReShiki 0.11.0](changes-0.11.md) for the latest release notes.

- **0.11.0 release validation:** document the tagged source, six-platform release checks, public-download checksums and macOS signing and native-worker verification. [Validation record](release-0.11.0-validation.md) · [PR #187](https://github.com/Ameyanagi/ReShiki/pull/187) · @Ameyanagi.
- **Experimental agent API (Nightly only):** `reshiki --mcp` lets Claude Code, Claude Desktop, Codex and other MCP clients import, analyze, render, compose, edit and export drawings, and save them only into folders you grant; `reshiki --cli` converts, renders, composes and analyzes from a terminal. Not in ReShiki 0.11.0; tools and results may change. [Setup guide](https://reshiki.com/guide/agents/) · [Reference](agent-api.md) · [Privacy](privacy-policy.md#optional-agent-api-and-command-line-experimental) · [Acceptance record](agent-api-p1-validation.md) · @Ameyanagi.
- **Windows signing integration:** verify SignPath-signed x64 and ARM64 applications and installers, and retain internal test-signed nightly artifacts using the existing nightly build. Public Windows downloads remain unsigned while the Foundation release certificate is pending. [Setup guide](signpath-setup.md) · [PR #270](https://github.com/Ameyanagi/ReShiki/pull/270) · @Ameyanagi.
- **Drawing-tool defaults (under review):** clearer chain icons and steering hints, explicit orbital atom snapping with Option/Alt bypass, transparent atom-label clearance, and closer click-added lone pairs. Existing orbital frames and manual/saved mark offsets stay exact. [Visual review](drawing-tool-defaults-review.md) · [#250](https://github.com/Ameyanagi/ReShiki/issues/250) · [#90](https://github.com/Ameyanagi/ReShiki/issues/90) · [#249](https://github.com/Ameyanagi/ReShiki/issues/249) · @Ameyanagi.

![Click-added methanol lone pair sits closer to O while CH4O and CO properties remain unchanged](images/drawing-tool-defaults/lone-pair-click-after.jpg)

Reusable caption: Click-added lone pairs sit closer to visible atom labels and
avoid nearby chemical ink; manual and previously saved offsets stay exact.
