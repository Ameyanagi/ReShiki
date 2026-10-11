# Wayland fixture publication precondition

Status: under review with [PR #280](https://github.com/Ameyanagi/ReShiki/pull/280).
Author: @Ameyanagi, project creator and maintainer. This follow-up changes the
private Python compositor fixture, without changing ReShiki's clipboard or
Assistant implementation.

## Failure and evidence

The original GNOME matrix job in
[run 37920045540](https://github.com/Ameyanagi/ReShiki/actions/runs/37920045540/job/113785520303)
failed the foreign-clipboard preservation assertion after focus-loss Cut. It
tested PR head `a9140833594b4098932d371036e0c3ba4182ab69` through merge
`5a305cae21536e567d390861fe7fdd40a0326521` onto Nightly source
`51fa0991da2507bb00b27c1b420e807468de6423`. Installation, harness checks and
application compilation succeeded. The uploaded `wayland-gnome` artifact shows:

- `receiver.log:366` queues the foreign source's `set_selection` at protocol
  timestamp `626444.086`. Focus leaves at `626463.050`; `receiver.log:374`
  receives cancellation at `626463.190`, before Ctrl+X at `626511.156`.
- `app.log:2196–2213` receives the prior ReShiki selection before Cut. The app's
  only two `set_selection` requests are the earlier Copies. It sends none during
  or after the gated Cut.
- App states 10/11 report a focus error with three atoms, two bonds and revision
  1. The saved `failed-cut.rsk` retains the exact original atom and bond arrays.
- The receiver reads the prior neutral graph, SHA-256
  `0e7a130f8379ea974ffbfb736d586fc372923b40f14b2fc6b3469b93260ffc3d`.
  The intended foreign graph has oxygen charge +1 and SHA-256
  `0e58ab1c03cebe4d7d97dffc5749f8edc0c324b263494b084ee19c5adc55f5bc`.

The original producer returned as soon as GTK queued local clipboard content,
then the harness transferred focus without establishing compositor acceptance.
An asynchronous publication/focus race is supported by this trace. The exact
reason for the Ubuntu-patched compositor's cancellation and its frequency remain
unproven from this attempt. This is not evidence of an Assistant regression or
ReShiki overwriting the foreign clipboard. The Sway job passed all 11 checks on
the same tested merge; later GNOME checks were unrun after the failure.

## Fixture change

Every foreign publication now includes a fresh MIME marker and the marker's
bytes. The producer calls `Gdk.Display.sync()` while it remains focused to order
its queued requests. That barrier is not treated as acceptance: the harness waits
up to five seconds for an **incoming** `data_offer`, that offer's unique marker,
and the matching `selection` from the requested data device. It rejects source
cancellation or a missing receipt before transferring focus.

Only after this receipt does the harness start a separate GTK reader, with the
publisher still alive. The reader must report the marker and every published
MIME with the expected MIME name and exact payload SHA-256. A local GTK read,
`changed` signal, `is_local` flag or successful queue operation is insufficient.
The helper covers the sentinel, focus-loss foreign graph, cancellation
replacement and paste fixture publications. It never republishes after Cut to
conceal a preservation failure. Existing post-Cut graph and clipboard assertions
remain in place.

Each publication retains its queue result, incoming selection receipt,
independent read, expected hashes and success or failure in `publication-N.json`.
`receipts.json` and `gates.json` are written as the run proceeds, including failed
checks, rather than waiting for the success-only final manifest. Raw protocol
logs and native drawings remain uploaded by the existing workflow.

## Validation and remaining scope

Source-only validation on macOS passes nine Python tests. They cover incoming
marker/offer/device matching, outgoing requests and sync callbacks that cannot
prove acceptance, old markers and reused offer IDs, cancellation, bounded receipt
timeout, exact marker/payload checks, and retained failure receipts. A cancelled
publication, missing receipt or stale reader payload prevents Cut; cancellation
and timeout also prevent reader creation. Ruff lint/format and whitespace checks
pass. These tests use protocol fixtures and mocked process boundaries; they do
not claim a live compositor run.

Run the focused tests with:

```sh
uv run --locked python -m unittest tests.test_wayland_publication -v
uv run --locked ruff check native/linux/tests/gtk_receiver.py native/linux/tests/wayland_session.py tests/test_wayland_publication.py
uv run --locked ruff format --check native/linux/tests/gtk_receiver.py native/linux/tests/wayland_session.py tests/test_wayland_publication.py
```

**Fresh GNOME and Sway workflow results remain required and pending.** They must
verify the actual GTK/Mutter and GTK/wlroots protocol receipts and all original
11 checks after this fixture change. No local GUI, Cargo build, workflow rerun or
model request was performed for this follow-up. The preserved macOS Assistant
screenshots, signed executable, guided graph and benchmark measurements remain
unchanged; a drawing before/after comparison does not apply to this fixture-only
protocol synchronization.

Original helper, tests and review documentation are offered under both MIT and
Apache-2.0, consistent with [CONTRIBUTING](../../CONTRIBUTING.md#license-agreement).
No third-party source is copied or bundled by this follow-up.
