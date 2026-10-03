# Linux clipboard transport

This safe Rust crate routes clipboard operations by the actual GUI window's
display handle. X11 uses a separate ReShiki `--clipboard-worker` process.
Wayland shares the standard data device already owned by Iced's clipboard.
Both routes validate every representation before changing the selection.

X11 uses `CLIPBOARD`, `TARGETS`, `TIMESTAMP`, `MULTIPLE` and bounded `INCR`
transfers. Its pending worker is killed on cancellation or timeout; an
acknowledged owner stays alive until another copy replaces it.

Wayland uses `wl_data_device` on the GUI's existing connection with a focused
seat and its real keyboard/pointer input serial. The pinned Smithay 0.7.3 patch
provides a command-only client for the existing owner, so text fields and rich
copies share one data device. Cloned clients cannot keep the worker or borrowed
display alive after Iced drops its owner. There is no global data-control
protocol, second Wayland client, helper window or clipboard utility.

A write succeeds only after the focused seat receives an offer containing the
new source's private random generation MIME. Enqueueing the request or receiving
a display sync callback is insufficient. Cancellation, focus loss, rejection or
timeout preserves Cut's source objects. Before publication these failures also
preserve the previous clipboard. After a request has been sent, Wayland cannot
atomically restore a previous foreign owner if the receipt is lost; failure
cleanup therefore never clears the selection or drops a source still serving a
consumer. Primary selections are not changed by drawing Copy/Paste.

Wayland ownership lasts with the GUI clipboard owner. Persistence after ReShiki
exits depends on the desktop clipboard manager and must be tested separately.
The standalone worker is X11-only. The optional LibreOffice adapter must publish
through its host's `SystemClipboard` and use workers only to validate/render data.

Native drawings use `application/x-reshiki-drawing+json` with the
`dev.reshiki.drawing` alias. Image, PDF, SVG, CDX, CDXML, MOL, SMILES and UTF-8 text
representations use their explicit MIME types. Native/chemical formats precede
pictures and text on ordinary Paste; Paste picture considers only raster formats.
Advertising a picture does not claim it is editable chemistry.

Decoded data is limited to 64 MiB combined, the JSON request/response to 128 MiB,
and offers to 128 formats, including the private Wayland receipt format. At most
16 concurrent requests or pipe transfers can run. A Wayland operation or transfer
has a fixed five-second deadline; the X11 worker request deadline is ten seconds.
X11 sends at most 64 KiB per chunk. Wayland pipes are nonblocking and process at
most 64 KiB or 128 system calls per dispatch. Retained Wayland source snapshots
and read buffers share a 128 MiB cap. Up to 32 live sources and incoming offers
are retained; offer metadata is bounded before SCTK stores MIME names. Each source
serves its own immutable bytes even when a later copy replaces it. MIME aliases
share allocations; binary reads preserve their exact bytes.

All transport dependencies were already present in the workspace lockfile.
The vendored Smithay source includes its MIT license, original manifest and
per-file provenance in `vendor/smithay-clipboard/UPSTREAM.json`. No dependency
versions were upgraded for this integration.

## Checks

On Linux, run `cargo test -p reshiki-linux` and
`cargo clippy -p reshiki-linux --all-targets -- -D warnings`.
The shared backend's unit tests run with `cargo test -p smithay-clipboard --lib`;
also check it with `cargo clippy -p smithay-clipboard --lib --tests -- -D warnings`.
`cargo tree -i smithay-clipboard` must show one patched instance shared by
`reshiki-linux` and Iced through `window_clipboard`/`clipboard_wayland`.
The X11 tests replace the clipboard and must run on a disposable server:

```sh
xvfb-run -a cargo test -p reshiki-linux -- --ignored --test-threads=1
cargo build -p reshiki-linux --example clipboard_worker
xvfb-run -a python3 native/linux/tests/worker_smoke.py target/debug/examples/clipboard_worker
```

The process smoke script tests only X11. Never aim it at a user's existing
desktop clipboard. Standard Wayland acceptance requires a real focused ReShiki
window: Copy, Cut, Copy Image, editable/native and PNG transfer to an external
consumer, text-field clipboard behavior, ownership replacement, focus loss and
app-exit lifetime. Run this on GNOME and another Wayland compositor; Xvfb and
unit tests do not establish those GUI results. Tests also cover command-owner
lifetime, registry replacement, cancelled requests, memory limits, short binary
reads/writes, zero writes, interrupted calls and partial-read expiry.
