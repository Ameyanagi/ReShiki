# Windows OLE clipboard acquisition

Status: under review; fresh Nightly acceptance is pending.

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

This has no intended visual output change, so a drawing screenshot comparison
does not demonstrate the fix. CI Rust 1.99 qualification and fresh Nightly GUI,
Office and display-scale acceptance remain pending.

Release-note caption: Retry brief Windows clipboard contention when acquiring
embedded drawings from Office.
