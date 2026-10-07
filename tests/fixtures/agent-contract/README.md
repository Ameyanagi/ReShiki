# In-app Codex contract goldens

These files are the exact bytes ReShiki's in-app Codex assistant sends to Codex
today. They were generated once from unmodified main 40c8d82a with
`serde_json::Value::to_string()`, one minified line ending in a single LF:

- `codex-dynamic-tools.json`: `reshiki::assistant::canvas_tools::definitions()`,
  the `dynamicTools` array of `thread/start` (src/assistant/codex.rs).
- `proposal-schema.json`: `reshiki::assistant::schema()`, the `canvas_preview`
  inputSchema and the Proposal turn outputSchema.
- `critique-schema.json`: `reshiki::assistant::review::schema()`, the visual
  critique turn outputSchema.

tests/assistant_contract.rs compares them byte for byte.

Never regenerate these files. A diff is a contract change that needs owner
approval.
