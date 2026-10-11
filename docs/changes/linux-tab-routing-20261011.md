# Linux file-open routing verification — 2026-10-11

Bounded runtime checks for [#100](https://github.com/Ameyanagi/ReShiki/issues/100)
passed using the published Linux x64 portable application from
[Nightly run 38123945065](https://github.com/Ameyanagi/ReShiki/actions/runs/38123945065).
These results establish request admission, process convergence and window-title
routing on an isolated X11 display. They do not complete the issue's desktop or
tab-state acceptance.

The tested release was
[`0.11.0-nightly.20261011.38123945065.1`](https://github.com/Ameyanagi/ReShiki/releases/tag/nightly-0.11.0-nightly.20261011.38123945065.1),
source commit `8e38151190eb2c3814df23a3eb166fb21c972d64`:

| Item                                                        | SHA-256                                                            |
| ----------------------------------------------------------- | ------------------------------------------------------------------ |
| Linux x64 `.tar.gz`, checked against published `SHA256SUMS` | `140a1afdc34991012ab7ea7de2dfc7dc3c0b2d71e2917d83541285f6e40550e8` |
| Executed `reshiki`                                          | `49dd7bcb4c9ddc04997d4f10575756c160c157af4b79b5e3dff97261d2a95fb4` |

The actual portable executable ran on a task-owned Xvfb display with separate
runtime, data, configuration, cache and temporary directories. Observations came
from process exits and read-only X11 window IDs/titles, without screenshots or
keyboard/pointer input. A and B were **different filenames with identical bytes**
copied from
[`adjustable-arc-desktop.rsk`](https://github.com/Ameyanagi/ReShiki/blob/8e38151190eb2c3814df23a3eb166fb21c972d64/tests/fixtures/adjustable-arc-desktop.rsk).

| Observed sequence, 09:37:29–09:37:36 UTC                                                                                            | Result                                                                                                                                                          |
| ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Start an empty editor; `--open A`; relative `--open ../fixtures/B-current.rsk` from another working directory; duplicate `--open A` | Each secondary exited 0. The same one window changed from Untitled to A, B, then A.                                                                             |
| Launch without a file                                                                                                               | Secondary exited 0; the existing A window remained. Window-manager focus was not observed.                                                                      |
| `--open` a deliberately invalid file                                                                                                | Secondary exited 0; one A window remained. This establishes admission, not diagnostic text or successful parsing.                                               |
| Simultaneous cold `--open A` / `--open B`                                                                                           | One resident process and one window remained; later A/B requests changed that window's title.                                                                   |
| Stop only the owned primary with `SIGSTOP`, then `--open B`                                                                         | A separate B fallback window appeared after 2.310 seconds. The stopped A window remained; both owned processes were subsequently resumed/terminated and waited. |
| Inspect the live IPC endpoint                                                                                                       | UID 1000 owned the directory (0700), socket (0600) and lock (0600).                                                                                             |

The fallback is an independent editor after an unresponsive host; it does not
establish exactly-once delivery after an ambiguous handoff. The log contains 12
observation rows across three scenarios, not 12 unit tests.

The first harness attempt passed an unsupported bare filename instead of
`--open PATH`. It created a second Untitled window and timed out after six seconds.
That failed attempt is retained. A fresh attempt corrected only the private
invocation; the application was unchanged. Xvfb's DRI3 warnings are also retained;
these checks make no GPU or rendered-pixel claim.

Cleanup confirmed all 14 owned application PIDs were absent and both owned Xvfb
servers had exited. Only the pre-existing display-manager/greeter processes and
X0 socket remained. All eight package files, including five license/notice files,
and six fixture copies were unchanged. No source build, installation or global
desktop configuration change was performed.

The host had no logged-in user graphical desktop, and no Linux computer-use
surface was available. **Tab count, duplicate-tab reuse, dirty edits or dirty
Untitled preservation, saved edit contents, diagnostic wording, real focus,
file-manager interaction, Wayland and Office-host save-back remain unobserved.**
Identical A/B contents and title changes cannot establish those behaviors.

[Machine-readable evidence](fixtures/linux-tab-routing-20261011/evidence.json)
records the package hashes, cleanup facts and original receipt/log hashes. The
[successful runtime log](fixtures/linux-tab-routing-20261011/runtime-pass.jsonl)
and [application warning log](fixtures/linux-tab-routing-20261011/ordinary-host.stderr.log)
are byte-exact copies. The
[first failed harness log](fixtures/linux-tab-routing-20261011/harness-first-failure.sanitized.jsonl)
replaces only the private task-directory prefix with `<TASK_ROOT>`; original
private records remain unchanged.
