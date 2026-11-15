---
section: Developer
issues: []
---

- The files under `tests/` build as one test binary, `it`, and no longer as
  103 binaries. Run one file's tests with
  `cargo test -p reshiki --test it theme_reference::`, not
  `--test theme_reference`. A new file needs a `mod` line in `tests/it.rs`.
  On an Apple Silicon Mac the integration tests built in 37 seconds instead of
  334, and their binaries take 0.2 GB instead of 7.0 GB.
  `agent_api_windows_pipes`, `metafile_worker` and `picture_memory` stay
  separate targets.
- CI compiles the workspace tests once per platform. The **rust build** job
  archives them, and the two **rust** partition jobs run the archive without
  compiling.
- On Windows, CI runs every ignored test of the application whose name does
  not start with `manual_`: about 140 renderer checks, where 13 hand-written
  steps ran about 70. Benchmarks, baseline comparisons and capture tools are now
  named `manual_…`, so their commands change, for example
  `manual_views_match_captured_baseline` and `manual_editing_workloads`. See
  [Opt-in renderer tests](development.md#opt-in-renderer-tests).
- Three capture tests that no longer passed are removed:
  `drawing_style_headless_snapshot`, `reaction_headless_snapshot` and
  `feedback_layout_and_visual_evidence`. They clicked fixed pixel positions or
  expected text of an older layout.
- The [module index](modules.md) lists every Rust module file with the first
  sentence of its `//!` comment. `python3 scripts/module_index.py` rewrites it,
  and a Python test fails while it is out of date.
