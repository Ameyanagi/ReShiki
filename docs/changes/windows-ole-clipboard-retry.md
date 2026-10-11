# Windows OLE clipboard acquisition

Status: merged, unreleased in [PR #305](https://github.com/Ameyanagi/ReShiki/pull/305);
published in Nightly with Windows CLI/load checks and
[scoped GUI checks at 96 DPI / 100% scale](nightly-windows-20261011.md) passed.
Fresh Office and high or mixed DPI acceptance remain pending.

Embedded-drawing paste from Office can fail when another application briefly
holds the Windows clipboard. The existing Office/ChemDraw priority test failed
with `OpenClipboard Failed (0x800401D0)` in
[Windows CI job 114406347885](https://github.com/Ameyanagi/ReShiki/actions/runs/38117843258/job/114406347885).
That log does not distinguish `OleGetClipboard` from `IDataObject::GetData`.

Both acquisition calls now retry only `CLIPBRD_E_CANT_OPEN` on the existing STA,
up to 20 attempts with 25 ms between busy failures. This adds at most 475 ms of
sleep per acquisition; the Windows calls themselves can take additional time.
Other errors return immediately, and exhaustion retains the final original
error. The existing storage ownership and drawing/CDX/image priority remain
intact.

## Before and after evidence

Before, the linked CI job failed the existing
`office_chemdraw_storage_precedes_images_without_changing_image_only_or_native_paste`
test during a read. After, that test and the following regression cases pass
natively on Windows:

- A real `OpenClipboard` holder makes the first `OleGetClipboard` call fail
  with `0x800401D0`; after coordinated release, acquisition returns the expected
  CDX bytes. Fixture acquisition and notification waits are bounded, and a guard
  closes the clipboard on the holder thread during success, error, or unwind.
- An `IDataObject` fixture returns two busy errors before successfully returning
  CDX. Persistent contention stops at exactly 20 retrievals with the same HRESULT.
- The existing nonbusy acquisition failure returns `E_FAIL` after one retrieval.
  Existing embedded drawing, standalone chemistry, image-only and DIB assertions
  remain active.

The native validation used Windows 11 and the installed Rust 1.98.1 MSVC
toolchain, based on source `50453022fb59f8f79b03996f2d4e5cb24f68654e`. The exported
two-file implementation/test patch has SHA-256
`1a7f32b39e943017044512dac464b16ff085d94d2d7caf6d73896e7a26acfc61`.

```text
cargo +1.98.1-x86_64-pc-windows-msvc test --locked --no-default-features \
  -p reshiki-windows --lib -j2 ole::embedded::tests -- --nocapture
6 passed; 0 failed

cargo +1.98.1-x86_64-pc-windows-msvc test --locked --no-default-features \
  -p reshiki-windows --lib -j2
27 passed; 0 failed; 1 existing ignored

cargo +1.98.1-x86_64-pc-windows-msvc clippy --locked --no-default-features \
  -p reshiki-windows --all-targets -j2 -- -D warnings
passed
```

An initial full-suite attempt encountered clipboard contention while publishing
the new fixture, before its read began. Fixture publication now retries only the
busy HRESULT. The final holder cleanup and timeout revision then passed both
test commands above. The final retry helper returns its twentieth acquisition
directly and passes the Windows crate's strict Clippy checks. Earlier diagnostics
were retained; no tests were disabled or
whole tests retried by the implementation.

## CI qualification

PR #305 at `f90eec446543b7adfdf48b15f9d4a1fcdf09f895` qualified with
19 latest successful checks and four expected skips.
[CI run 38121740755](https://github.com/Ameyanagi/ReShiki/actions/runs/38121740755)
passed. Its Windows Rust 1.99.0 job passed strict Clippy and both new cases:

- `ole_get_clipboard_recovers_after_real_clipboard_contention`
- `embedded_get_data_retries_only_transient_clipboard_contention`

The Windows native library finished with 27 passed, zero failed and one existing
ignored test; application tests finished with 746 passed, zero failed and
98 ignored. These CI results supplement the Rust 1.98.1 native receipt above.

## Published Nightly and Windows portable checks

[Nightly run 38123945065](https://github.com/Ameyanagi/ReShiki/actions/runs/38123945065)
succeeded and published
[0.11.0-nightly.20261011.38123945065.1](https://github.com/Ameyanagi/ReShiki/releases/tag/nightly-0.11.0-nightly.20261011.38123945065.1)
from `8e38151190eb2c3814df23a3eb166fb21c972d64`. All six platform builds passed;
the run finished with 12 successful jobs, two intentional skips and zero failures.
Public Windows packages are unsigned; internal test-signing artifacts are not
the published downloads.

The downloaded public Windows x64 ZIP matched both its published checksum and
the Actions artifact used for native checks. Its SHA-256 is
`4e2f18ccadeedc3b84e0c08fed6ba119cd72df2ac20454f152a2c3c2a47af160`.

Native Windows Codex 0.156.1 ran the portable validator once in Session 2 using
`gpt-6-sol` with `xhigh` reasoning and YOLO enabled for that run; `gpt-6.1-sol`
was unsupported in that session. All five commands passed: CLI information,
ethanol conversion, MOL analysis, native RSK rendering and graphics information.
All four output-format checks passed: information JSON, analysis JSON, ethanol
MOL and native RSK PNG. The retained `results.json` has SHA-256
`a8afc23d4c17da0b8567473af7378cea4587c79a9fd521d5cb63171c84d90721`.

An earlier source guard exited 128 before invoking the validator. That failure
receipt is preserved; command-scoped Git trust resolved the guard without
changing global settings. The successful source guard confirmed the exact
commit and clean tracked files.

This has no intended visual output change, so a drawing screenshot comparison
does not demonstrate the retry fix. The portable validator did not launch the
GUI. The subsequent [actual desktop review](nightly-windows-20261011.md) passed
arc editing/Undo/Redo, tab activation, local ethanol insertion and floating NMR
checks at measured 96 DPI / 100% scale. Fresh Office clipboard/save-back and
high or mixed DPI acceptance remain pending. Join/Co(en)3
accuracy remains TODO and [issue #246](https://github.com/Ameyanagi/ReShiki/issues/246)
stays open.

Release-note caption: Retry brief Windows clipboard contention when acquiring
embedded drawings from Office.
