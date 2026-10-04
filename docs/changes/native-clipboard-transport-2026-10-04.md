# Platform PR validation and CI routing

Validation date: 2026-10-04. Baseline: `81ca82101061ecc201545a3cae8d8257f03254d3`; candidate platform changes were tested in a combined integration checkout on macOS arm64. The platform production source is **−2 physical / −6 nonblank lines**, including the new private process helper and module declaration; tests are excluded. Published PR #139 subsequently passed Linux X11 and real GNOME/Sway clipboard CI, as recorded below. macOS/Windows application-dialog, Office, and physical-printer acceptance are not established by these results.

## Completed portable checks

The following local receipts returned status 0. Their hashes for all eight relevant changed source/Cargo files match this candidate. The unit receipt used a freshly selected candidate InChI helper; the platform protocol and pipe tests all passed.

| Command                                                             | Result                                                                                         | Receipt                                 |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | --------------------------------------- |
| `cargo check --locked --all-targets --all-features`                 | Passed                                                                                         | `combined-full-check-fixed.json/.log`   |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed, no allowed warnings                                                                    | `final-combined-clippy-fixed.json/.log` |
| `cargo test --locked --lib --bin reshiki -- --test-threads=1`       | **820 passed, 0 failed, 69 ignored**: library 327 passed/15 ignored; app 493 passed/54 ignored | `combined-full-lib-bin-fixed.json/.log` |

Receipts are retained locally under `/tmp/reshiki-improvements-20261004/evidence/`. The check/unit combined diff hash is `3dbf9fa2ca8b14bd9df70ef4c262d77ce424263c4874d149f4aa2fa87e1b5f16`; the final strict-Clippy/allocation diff hash is `6d1ebed1392f5be339e174cca1a78f5756428777bd3422a44363b2f7d6e0d7d5`. The platform files are identical across those receipts. Direct Rustfmt and diff whitespace checks also passed.

## Measured CDX alias allocation

The isolated optimized test passed on macOS arm64:

```sh
cargo test --locked --release --lib clipboard::tests::cdx_alias_allocation_metrics -- --ignored --exact --test-threads=1 --nocapture
```

Input was **1,048,576 raw bytes**, encoded to **1,398,104 bytes**. All three aliases still serialize their payloads, totaling **4,194,312 payload bytes** and **4,194,512 exact JSON wire bytes**. The test asserts byte equality with the old String protocol for both editable and fallback paths; wire sizes are checked outside the generation allocation snapshots.

| Path                             | Retained requested bytes | Peak requested bytes above baseline | Total requested allocation bytes | Allocation count | Base64 encodes | Observed generation time |
| -------------------------------- | -----------------------: | ----------------------------------: | -------------------------------: | ---------------: | -------------: | -----------------------: |
| Editable, old Strings            |                4,194,549 |                           5,592,653 |                        5,592,653 |                8 |              0 |               422.917 µs |
| Editable, shared String          |                1,398,333 |                           1,398,333 |                        1,398,333 |                6 |              0 |                25.667 µs |
| Image fallback, old Strings      |                4,194,549 |                           4,194,549 |                        4,194,549 |                7 |              3 |             1,196.417 µs |
| Image fallback, shared String    |                1,398,333 |                           1,398,333 |                        1,398,333 |                6 |              1 |               395.416 µs |
| Single large read, old String    |                1,398,138 |                           1,398,138 |                        1,398,138 |                2 |              0 |               190.792 µs |
| Single large read, shared String |                1,398,178 |                           1,398,202 |                        1,398,202 |                4 |              0 |               199.625 µs |

Sharing saves **2,796,216 retained requested bytes** for these three aliases, about 66.7%. The single large read adds only **40 retained bytes and 64 peak/allocated bytes** rather than copying the encoded payload. This supports the private `Arc<String>` choice over `Arc<str>`: the String buffer is moved intact, while aliases share ownership. An editable source clone is included equally in both generation measurements; raw fixture storage and prior retained test outputs are excluded through the live-byte baseline.

The allocator counters measure requested Rust allocation sizes, excluding allocator headers, allocator-internal temporaries, native allocations, and RSS. Timings are one isolated optimized observation of alias construction/deserialization, excluding engine conversion and JSON serialization; they are not an end-to-end latency benchmark or a performance threshold. The receipt is `platform-alias.json/.log`, with the final strict-Clippy diff hash above. No process RSS or native-memory improvement is claimed.

## Completed published-candidate Linux checks

[PR #139](https://github.com/Ameyanagi/ReShiki/pull/139) was validated at published head `ac714639f59437de888a0827f36fd9262b3b5061`. The jobs checked out test merge `350f5dcae7c92614061b68e20e4108f247b4f82b`, whose parents are main `409e08051affa9e320771e515e7243202934d686` and that PR head. All eight published platform source/Cargo blobs matched the reviewed candidate. These CI identities supplement the historical local receipts above.

| Native job                   | Result                                                                                                                                                                         | Public evidence                                                                             |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| Linux X11, private Xvfb      | **10 passed, 0 failed**, 19.43 s; includes the buffered-event/deadline regressions, progressing and stalled INCR, owner replacement/exit, and oversized input/target rejection | [X11 job](https://github.com/Ameyanagi/ReShiki/actions/runs/37209867827/job/111458734182)   |
| Real GNOME Wayland clipboard | Passed; 13 candidate source hashes and 25 received payload lengths/SHA-256 verified; seven nonzero publication serials                                                         | [GNOME job](https://github.com/Ameyanagi/ReShiki/actions/runs/37209867791/job/111458691011) |
| Real Sway Wayland clipboard  | Passed; 13 candidate source hashes and 24 received payload lengths/SHA-256 verified; seven nonzero publication serials                                                         | [Sway job](https://github.com/Ameyanagi/ReShiki/actions/runs/37209867791/job/111458691188)  |

The [Wayland run](https://github.com/Ameyanagi/ReShiki/actions/runs/37209867791) retains `wayland-gnome` (artifact `11306630059`, archive SHA-256 `6d3a8d0c032a92545cfa10ddb7a73d031f44d6860ff563c0fe1859f2817bc24f`) and `wayland-sway` (artifact `11306605121`, archive SHA-256 `c1cb6aaf4bf430342811095361c544e36d1412f6e8f387dcb496b834ba2c80d4`). Downloaded archive hashes match GitHub's recorded digests. Both manifests identify the test merge above and binary SHA-256 `63a02e93bbaf06cf6a1f053e39c07a67eb2d5f7280e6bad73d4853745148c89b`; the changed Wayland GUI source has SHA-256 `493128e883c6e37a79c4b1b55173d41928bd380e4c12644d1a276a070bdbfb29`.

The first native/PNG/SVG receipt in each compositor contains the same exact payloads:

| Payload             |  Bytes | SHA-256                                                            |
| ------------------- | -----: | ------------------------------------------------------------------ |
| Native drawing JSON |  1,011 | `06e7f142e13369ae5081a1a811bcbbdd5c6cd680f636ce3209cfa4f8524c882c` |
| PNG                 | 14,294 | `befcbcc8d9c5b2c6b9aa686b68749edba9e4ae70c5299f71c55121ef0681c362` |
| SVG                 |    889 | `f68afb46f8ed099c5a9a547b4faf3892477ef1b59b8d2d1b35a452549e6478fc` |

Both compositor manifests record real keyboard Copy, foreign-owner replacement, focus-loss Cut preservation, cancellation/source retention, cancellation cleanup preserving the foreign owner, Cut awaiting the real receipt, saved Cut/Undo/native-Paste graphs, Copy Image PNG, and a fresh receiver after app exit. These establish the tested clipboard paths and compositor-manager lifetime behavior; they do not measure broad application speedups or RSS.

## Existing automatic coverage

- `.github/workflows/checks.yml` runs on every PR. `scripts/ci_scope.py` selects native checks for these Cargo/src/native changes. The Rust matrix covers macOS 14 arm64, Windows Server 2022 x64/MSVC, and Ubuntu 22.04 x64, including workspace Clippy/check/tests. The separate `linux-x11` job has a 10-minute timeout and runs all ignored `x11::tests::` serially on private Xvfb. The two new cases are picked up by its existing filter.
- `.github/workflows/wayland-clipboard.yml` matches `Cargo.toml`, `src/**`, and `native/linux/**`, so this PR triggers both GNOME and Sway jobs on Ubuntu 24.04, each bounded to 25 minutes. Both use the actual application and an independent GTK receiver. No workflow modification is required.
- `.github/workflows/contribution-license.yml` checks both completed checkbox lines using exact text matching. The draft PR body preserves the two template lines verbatim with `[x]`.

## Exact Linux commands

Use a Linux checkout of the PR commit. The existing X11 CI job installs `xvfb xauth libwayland-dev libxkbcommon-dev pkg-config` on Ubuntu 22.04, with Rust 1.99.0. Its exact regression command is:

```sh
xvfb-run -a -s '-screen 0 1024x768x24 -nolisten tcp' cargo test -p reshiki-linux --lib --locked --no-default-features x11::tests:: -- --ignored --test-threads=1
```

Additional ordinary crate checks and standalone-worker smoke from the existing fixtures:

```sh
cargo test -p reshiki-linux --locked --no-default-features
cargo clippy -p reshiki-linux --all-targets --locked --no-default-features -- -D warnings
cargo build -p reshiki-linux --locked --example clipboard_worker
xvfb-run -a -s '-screen 0 1024x768x24 -nolisten tcp' python3 native/linux/tests/worker_smoke.py target/debug/examples/clipboard_worker
```

The smoke script validates acknowledged owner lifetime, exact native/image aliases, INCR, atomic rejection, cancellation before complete input, Unicode text, and owner replacement. It changes only the private Xvfb selection when invoked this way. The new queue test joins only after completion and has bounded recovery wakes; it fails rather than hanging on an indefinitely blocked owner thread.

For Wayland, use Ubuntu 24.04 and the exact package list in `.github/workflows/wayland-clipboard.yml`; build with its default-off QA feature:

```sh
cargo build --locked --no-default-features --features wayland-qa --bin reshiki
bash scripts/wayland_clipboard_qa.sh gnome target/debug/reshiki artifacts/wayland-qa/gnome
bash scripts/wayland_clipboard_qa.sh sway target/debug/reshiki artifacts/wayland-qa/sway
```

Each output path must not exist before its invocation. The script creates a new D-Bus session and isolated runtime/config/cache/data directories. GNOME uses a nested Wayland compositor inside private Xvfb; Sway uses a headless output. Xwayland is disabled and clipboard clients have `DISPLAY` unset. This validates real nonzero input serials, generation receipts, exact native/PNG/SVG transfer, Copy/Cut/Copy Image/Paste/Save/Undo, delayed-receipt semantics, focus loss, cancellation, replacement, and owner exit. Artifacts `wayland-gnome` and `wayland-sway` retain `manifest.json`, package versions, binary/source SHA-256, saved drawings, MIME bytes/hashes, and protocol traces. Mutter's built-in manager persistence and Sway's no-manager loss are checked separately; third-party desktop manager persistence remains outside these results.

## Dispatch capabilities after publication

Both workflows already support `workflow_dispatch`. `checks.yml` accepts booleans `live_reference` and `capture_aromatic`; set both false for the ordinary platform scope. `wayland-clipboard.yml` has no inputs and always runs its two-compositor matrix. There is no existing dispatch input to select only the X11 job. Dispatching Checks therefore runs its full ordinary workflow, including X11; PR-triggered checks should normally suffice.

If root chooses an explicit rerun after publishing the branch, these are the commands (substitute the actual published PR branch if root uses a different name):

```sh
gh workflow run checks.yml --repo Ameyanagi/ReShiki --ref perf/native-clipboard-transport -f live_reference=false -f capture_aromatic=false
gh workflow run wayland-clipboard.yml --repo Ameyanagi/ReShiki --ref perf/native-clipboard-transport
gh run list --repo Ameyanagi/ReShiki --branch perf/native-clipboard-transport --workflow checks.yml --limit 5
gh run list --repo Ameyanagi/ReShiki --branch perf/native-clipboard-transport --workflow wayland-clipboard.yml --limit 5
```

Match the resulting run's head SHA to the final PR head; a passing baseline run does not validate the candidate. Do not dispatch Nightly or Release for these transport checks. No workflow dispatch has been performed for this validation note.

## Native limits and unique result note

**Current verified candidate evidence:** the completed check, strict Clippy, 820 portable unit tests, exact serialized-byte tests, pipe EOF/drain tests, isolated alias measurements, and published-candidate Xvfb/GNOME/Sway results above. The portable run includes `cdx_aliases_share_payload_without_changing_serialized_protocol`, `capture_keeps_exact_prefix_and_accepts_short_or_empty_streams`, `writing_closes_stdin_for_eof_dependent_worker_and_preserves_output`, and `print_errors_keep_prefix_and_drain_remaining_bytes_to_eof`. The ten ignored native X11 tests passed under private Xvfb; their Linux evidence is separate from the macOS all-features run.

**Linux:** the candidate's X11 native regression job and both real GNOME/Sway jobs passed. Xvfb validates the bounded X11 transport cases; the independent compositor runs validate actual Wayland focused-seat ownership, transfer, cancellation, and app-exit behavior. Non-Linux compilation alone cannot validate these target-gated paths. Third-party desktop clipboard-manager persistence remains outside the tested compositor configurations.

**macOS:** portable helper tests prove pipe mechanics, caps, EOF and exit status. Existing `tests/test_native_clipboard.py` uses a private pasteboard; `tests/test_native_print.py` builds the main-thread `reshiki-macos` native-print executable and saves private PDFs rather than printer jobs. Those tests do not establish application dialog cancellation or temporary-file lifetime at the shared transport call boundary. Retain focused app-level native clipboard/print acceptance and do not label headless/native geometry checks as real printer acceptance.

**Windows:** the Rust matrix validates compilation, exact app JSON serialization, and existing native tests on its disposable runner. This candidate changes no Windows FFI, COM thread ownership, clipboard format mapping, printing raster cache, or GDI+ backing-vector lifetime. A macOS run cannot establish Windows runtime behavior; no physical printer or Office application result is claimed. Deferred raster caching remains unchanged because aggregate decoded image retention is not bounded by the compressed snapshot limit.
