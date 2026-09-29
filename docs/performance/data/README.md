# Performance measurement dataset

[Release-to-current report](../release-to-current.md) · [all measurements](measurements.csv) · [run metadata](manifest.json) · [benchmark output](logs/)

`measurements.csv` preserves **every reported aggregate row** from the successful
runs selected for the three optimization reports and the matched v0.9.1/current
rerun. The original slow nightly baseline is included. This is a checkpoint
history, not a benchmark of every commit or an archive of individual iteration
samples: the harnesses emitted aggregate statistics only.

Each row identifies the run, comparison series, platform, source checkpoint and
full commit, benchmark family, renderer, workload, warmup/sample counts, median,
p95, optional mean, and supporting log. All times are milliseconds. Empty means
were not reported; they are not zero. Values are preserved at the source log's
precision, including timings rounded to zero. Percentiles are the harness's
reported order statistics, not newly calculated estimates.

`manifest.json` records machine/build details, exact source revisions, renderer
limitations, per-run notes and SHA-256 hashes. The source hashes refer to original
local logs. Committed logs retain their complete test-output block, including
success status; compiler output and machine-specific paths preceding that block
are removed. The original Windows nightly baseline was UTF-16 and is normalized
to UTF-8 here. The committed log hashes cover those normalized files.

The three series are deliberately separate:

- `historical`: the completed runs supporting the original canvas, editing and
  background-I/O reports. Older Windows logs did not record the selected renderer.
- `matched_release_current`: actual stable **v0.9.1 (`167d893c`)** versus measured
  current **`ee2ce1a`**, rerun with corresponding harnesses. Windows canvas uses
  explicitly selected `tiny-skia` on both revisions; macOS uses its wgpu build.
- `rejected_experiment`: the theme-library trial, retained for transparency. It
  regressed and was reverted; it is not part of the delivered improvements.

The old editing harness misspelled the two-atom `methanol_*` workload as
`ethanol_*`. The `workload` column normalizes that typo; `workload_original` and
the raw logs retain it. It did not use a different molecule.

Canvas timings cover geometry/text preparation through the real canvas API.
Editing's `oxygen_hotkey` is an **O then C pair**, excluding task execution and
presentation. Inspector `panel` and full `view` are separate measurements.
Autosave `dispatch` returns before writing, while `roundtrip` includes completion;
the updated build's `synchronous` row is a cost control, not an old app run.
The stable `recovery_tick` row includes its synchronous durable write.

## Reproduction

The [stable harness patch](v091-harness.patch) adds benchmark-only modules and
helper visibility to the exact v0.9.1 source. It includes the synchronous autosave
Tick workload and a headless-renderer selector; it contains no runtime
optimization. Apply it in a clean checkout of `167d893c5007ea665456652897791844c5ef9fce`.
The current harnesses are in `src/canvas/performance.rs`, `src/app/performance.rs`,
`src/app/inspector.rs`, `src/app/autosave.rs`, and `src/app/files.rs` at the measured
current revision, plus the benchmark-only renderer selector in this PR.

Run optimized builds serially on the same machine, without a profiler or other
build/benchmark competing for CPU time. Canvas uses three warmups and 20 measured
iterations; editing, inspector, recovery and file/library workloads use three
warmups and 30 iterations. The gallery fixture is unchanged between these runs.
Provide the packaged InChI helper through `RESHIKI_INCHI_HELPER` for full-analysis
rows and an isolated `RESHIKI_DATA_DIR` for application preferences/recovery.

```sh
cargo test --release --locked --bin reshiki \
  canvas::performance::loaded_canvas_workloads -- --ignored --exact --nocapture
cargo test --release --locked --bin reshiki \
  app::performance::editing_workloads -- --ignored --exact --nocapture
cargo test --release --locked --bin reshiki \
  app::inspector::tests::inspector_workloads -- --ignored --exact --nocapture
```

On Windows add `--target x86_64-pc-windows-msvc` and set
`$env:RESHIKI_PERF_RENDERER='tiny-skia'` before the canvas command. The harness
passes that selector directly to `Headless::new` and prints the actual renderer;
`ICED_BACKEND` alone does not select this headless benchmark renderer.

The stable recovery test is
`app::performance::historical_recovery_workloads`; the updated recovery test is
`app::autosave::tests::recovery_workloads`. File/library measurements use
`app::files::performance::file_library_workloads` on the updated build only.
There is no historical file/library benchmark to compare against.

`collect.py`, run from the repository root, assembles completed local logs under
`artifacts/performance/` into this dataset, falling back to the committed logs
when the original local files are unavailable. It excludes incomplete runs and all
profiler-attached timings. The dataset contains no claim of FPS, actual
key-to-display latency, faster disks, or measurements on the user's physical
Intel Iris Xe GPU.
