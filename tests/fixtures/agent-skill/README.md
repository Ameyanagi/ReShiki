# Agent Skill golden

`reshiki/` pins every file `reshiki --cli skill` writes: SKILL.md, the four
references (verbatim command help, the MCP tool catalog, limits and errors,
client setup), the Proposal schema and the example Proposal. It is rendered
with the executable `/opt/reshiki/reshiki` and the application version
normalized to `<app>`, so it does not depend on the operating system or the
build. `golden_files_match_the_live_renderers` in src/cli/skill/tests.rs
renders the same inputs and compares every file byte for byte.

`assets/proposal-schema.json` must stay byte-equal to
`../agent-contract/proposal-schema.json`. `assets/examples/esterification.json`
is src/cli/skill/esterification.json, a copy of
docs/images/agent-api-p1/esterification.json.

Regenerate the folder only for an intentional change to the skill's content
or to a contract it renders (command help, tool catalog, budgets, error codes),
and in the same PR as that change, so the diff shows exactly what agents will
read. Never regenerate it to make an unexplained failure pass. From the
repository root:

```sh
cargo test --locked --no-default-features --lib cli::skill::tests::regenerate_the_golden -- --ignored --exact
```
