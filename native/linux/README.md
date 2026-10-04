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

### Disposable GNOME and Sway integration

The `Wayland clipboard` workflow runs the actual application under GNOME Shell
46/Mutter 46 and Sway on Ubuntu 24.04. GNOME uses its nested Wayland compositor
inside a private Xvfb server; Sway uses a headless output. Clients have `DISPLAY`
unset and Xwayland is disabled. A new D-Bus session and isolated runtime/config/
data directories keep these checks separate from any existing desktop.

```sh
cargo build --locked --no-default-features --features wayland-qa --bin reshiki
bash scripts/wayland_clipboard_qa.sh gnome target/debug/reshiki artifacts/wayland-qa/gnome
bash scripts/wayland_clipboard_qa.sh sway target/debug/reshiki artifacts/wayland-qa/sway
```

The script requires a new output directory and the workflow's compositor, GTK4,
input and Mesa packages. It sends real compositor keyboard events for Copy,
Cut, Copy Image, Paste, Save and Undo. A separate focused GTK window reads exact
native JSON/PNG/SVG bytes and publishes foreign clipboard items on a real F12
event. Ordinary Copy's SMILES plaintext is also read and checked. Saved native
drawings verify deletion, restoration and editable paste.

The default-off `wayland-qa` feature observes normal App updates and offers two
one-shot scheduling delays in the existing owner worker: before write submission
and after the focused data device receives the actual generation marker, before
delivery to the caller. No hook supplies an acknowledgement, serial or MIME.
The GUI remains responsive; Save while the receipt is held proves Cut has not
deleted the selection. Cancellation drops the real request; focus loss before
submission and cancellation/replacement after publication retain the correct
source or foreign owner. A six-second watchdog bounds a lost test controller;
the normal five-second request deadline is neither extended nor frozen.

Artifacts retain protocol traces with nonzero input serials, real receipt
markers, package versions, executable SHA-256, independent payload hashes and
saved drawings. Default/release builds do not enable `wayland-qa`.

App-exit checks create a fresh receiver to avoid a previous consumer's cache.
Sway without a clipboard manager loses the selection. Mutter's built-in manager
can preserve one preferred standard image/text MIME, so GNOME checks its exact
surviving bytes separately from the unavailable private drawing/receipt MIME.
When ordinary Copy supplies chemical plaintext, the manager can prefer that
text over PNG/SVG; each surviving payload must match its bytes before app exit.
These tests establish the standard Wayland path and the tested compositor's
manager behavior; third-party clipboard-manager extensions retain their own
desktop-specific persistence policy.
