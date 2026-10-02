# Linux clipboard transport

This safe Rust crate owns Linux clipboard selections in a separate ReShiki
`--clipboard-worker` process. A write validates and decodes every representation
before changing the selection, then acknowledges only after the desktop server
has processed publication. The application kills a pending worker on timeout or
task cancellation. An acknowledged owner stays alive until another copy replaces
it, including after a short-lived LibreOffice bridge command exits.

X11 uses `CLIPBOARD`, `TARGETS`, `TIMESTAMP`, `MULTIPLE` and bounded `INCR`
transfers. Wayland prefers `ext-data-control-v1`, with `wlr-data-control-v1` as a
fallback, on the first advertised seat. A compositor must permit that protocol;
ordinary `wl_data_device` access from an unrelated helper is insufficient. A
configured but unsupported Wayland session reports an actionable error instead
of silently writing an unrelated XWayland selection. Primary selections are not
changed.

Native drawings use `application/x-reshiki-drawing+json` with the
`dev.reshiki.drawing` alias. Image, PDF, SVG, CDX, CDXML, MOL, SMILES and UTF-8 text
representations use their explicit MIME types. Native/chemical formats precede
pictures and text on ordinary Paste; Paste picture considers only raster formats.
Advertising a picture does not claim it is editable chemistry.

Decoded data is limited to 64 MiB combined, the JSON request/response to 128 MiB,
and offers to 128 formats. At most 16 concurrent transfers can run. Each transfer
has a fixed five-second deadline; the application's request deadline is ten
seconds. X11 sends at most 64 KiB per chunk. Wayland writes use nonblocking FDs
and bounded chunks, so a stalled consumer cannot stop clipboard dispatch. The
read FD alone is made nonblocking when requesting data from another provider.
An immutable completed stream remains usable if a one-shot provider exits.

All transport dependencies were already present in the workspace lockfile.
No shell clipboard utilities, compositor settings, display forwarding or
third-party code copies are needed at runtime.

## Checks

On Linux, run `cargo test -p reshiki-linux` and
`cargo clippy -p reshiki-linux --all-targets -- -D warnings`.
The X11 tests replace the clipboard and must run on a disposable server:

```sh
xvfb-run -a cargo test -p reshiki-linux -- --ignored --test-threads=1
cargo build -p reshiki-linux --example clipboard_worker
xvfb-run -a python3 native/linux/tests/worker_smoke.py target/debug/examples/clipboard_worker
```

The same process smoke script can run in an isolated Wayland compositor with a
data-control protocol. Never aim these tests at a user's existing desktop
clipboard. Unit tests exercise Wayland pipe backpressure and source cancellation;
Xvfb evidence establishes the X11 protocol behavior, not a native GUI workflow or
a compositor-specific Wayland test.
