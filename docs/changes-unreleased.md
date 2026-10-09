# Unreleased changes

See [ReShiki 0.11.0](changes-0.11.md) for the latest release notes.

- **0.11.0 release validation:** document the tagged source, six-platform release checks, public-download checksums and macOS signing and native-worker verification. [Validation record](release-0.11.0-validation.md) · [PR #187](https://github.com/Ameyanagi/ReShiki/pull/187) · @Ameyanagi.
- **Experimental agent API (Nightly only):** `reshiki --mcp` lets Claude Code, Claude Desktop, Codex and other MCP clients import, analyze, render, compose, edit and export drawings, and save them only into folders you grant; `reshiki --cli` converts, renders, composes and analyzes from a terminal. Not in ReShiki 0.11.0; tools and results may change. [Setup guide](https://reshiki.com/guide/agents/) · [Reference](agent-api.md) · [Privacy](privacy-policy.md#optional-agent-api-and-command-line-experimental) · [Acceptance record](agent-api-p1-validation.md) · @Ameyanagi.
- **Windows signing integration:** verify SignPath-signed x64 and ARM64 applications and installers, and retain internal test-signed nightly artifacts using the existing nightly build. Public Windows downloads remain unsigned while the Foundation release certificate is pending. [Setup guide](signpath-setup.md) · [PR #270](https://github.com/Ameyanagi/ReShiki/pull/270) · @Ameyanagi.
- **Assistant setup and guided image exercise — under review:** explains Codex installation, sign-in, connection recovery and image transfer, then offers a small original ethanol image whose editable draft waits for Apply. [Setup guide](assistant-setup.md) · [Native review and saved-graph proof](changes/assistant-setup-review-2026-10-09.md) · [PR #280](https://github.com/Ameyanagi/ReShiki/pull/280) · @Ameyanagi.

![Assistant explains connection readiness and image-transfer guidance before a guided example](images/assistant-setup/assistant-setup-ready.jpg)

![The original ethanol image and known reference attach locally before Send](images/assistant-setup/assistant-local-example.jpg)

The known ethanol exercise is a first-use example. Model generation and visual
review do not establish chemical correctness for other images; inspect the draft
before Apply. This entry remains unreleased while the contribution is under review.

- **Assistant image benchmark — under review, PR pending:** records independent graph identity, unfinished drafts and failures for eight original synthetic images. Six exact completed graphs out of eleven attempted finite-reference runs remain separate from provisional previews and the known guided exercise. [Method and measured results](assistant-benchmark.md) · [Issue #94](https://github.com/Ameyanagi/ReShiki/issues/94) · @Ameyanagi.

![Provisional macrocycle preview retained after a structured-output failure; excluded from completed successes](assistant-benchmark-results/baseline-20261009/cyclic-gly4/run-2/provisional.png)

The small synthetic cohort does not establish accuracy on real scans or general
peptide/macrocycle reliability. The retained preview is unfinished even though
its graph matches the reference. See the report for failures, missing display
labels, uncertainty and not-run repeats.
