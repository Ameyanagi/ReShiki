# Windows/Linux launches open document tabs (#100)

On Windows and Linux, an ordinary secondary launch sends its native file paths
to the running desktop window. The window reuses a pristine Untitled drawing,
selects an existing native-file tab without reloading its edits, or opens each
new file in a tab. A launch without files requests restore, focus and user
attention. Platform activation permission still controls whether focus succeeds.

Every Office, LibreOffice and Microsoft 365 edit launch is a waiting proxy,
including the first launch. A separate GUI host owns the drawing tab. Each tab
keeps its own host, authorized source path and completion lease; the original
host adapters keep their existing save and temporary-directory protocols.
Closing one Office tab completes only its proxy. Pending saves delay close;
cancelled or failed saves keep the lease. A successful Save As to another file
detaches after writing; committed window exit completes all settled sessions.
If a proxy dies, its drawing and styles remain available as an unsaved ordinary
tab, its source target is revoked, and stale tagged save results cannot retarget
it. The GUI's separate source-path fence remains held until all in-flight save
references finish, preventing a successor edit session or an ordinary Save As
from becoming a competing writer. It is acquired at Commit before UI enqueue;
Prepare never fences a source. Unexpected proxy termination still lets an
external host adapter clean up its directory; this code cannot roll back that
adapter cleanup or a write already in progress.

## Transport and failure behavior

The local protocol accepts typed open/session actions, never arbitrary command
lines. Windows uses a named mutex/pipe with a current logon SID ACL, local-only
clients and verified user/logon/session/process-generation identity. Linux uses
private 0700 runtime directories, 0600 sockets/locks, flock election, peer UID
checks and PID/start-generation checks. A stale socket is removed only by its
lock owner after a refused connection; a live or unknown endpoint is retained.
Linux directories and stable source lock files remain in the runtime directory;
active memory, channels and connections are bounded independently of these
small persistent election artifacts.

Frames are limited to 256 KiB, with at most 64 absolute native paths, a 16-entry
UI channel, 64 ordinary request records and 32 Office leases including retained
terminal results. Unix filename bytes and Windows wide units are lossless.
Relative paths are resolved in the launching process's working directory.
Native connection/read/write operations have deadlines. UI admission has a
750 ms acknowledgement wait, ordinary handoff has a two-second budget, and
uncommitted Office reservations expire after five seconds.

UI admission waits for startup readiness and rejects temporarily busy/closing
states. Accepted reads and foreign-file engine imports block window/tab close
and update restart until their result is processed. Ordinary retries retain
request identity after a lost acknowledgement. A timed-out ordinary launch may
create an independent unregistered window: timeout availability cannot promise
global exactly-once delivery after an ambiguous final acknowledgement.

Office uses Prepare/Commit. Before Commit, a timed-out launch can choose a
private GUI while retaining an exclusive source-path claim. After Commit may
have arrived, it queries the same authenticated lease while that exact GUI
process is alive; IPC timeout does not complete editing or start a second
writer. Closed sessions are retained until proxy acknowledgement or confirmed
proxy death. A GUI crash ends its proxy with failure, matching the existing
host adapters' process-exit/recovery boundary.

Worker/headless/server/diagnostic entry points remain independent. Explicit
GUI actions such as shortcut examples retain their existing launch semantics.
Unsupported Office arguments fail before a GUI opens rather than bypassing the
waiting proxy. The macOS document-event route keeps its existing behavior, with
Office binding now attached to the document tab.

## Reproduction and evidence

The declared stacked base is #108 commit `7cab2287`. Pre-edit source hashes and
Windows App-state witnesses were retained outside Git: independent startup
App instances each owned their own file tab, and the old singleton Office
binding was replaced by a second Office startup. Those witnesses are source
and state evidence, not two live desktop windows or actual host save-back.

Windows native tests exercise election, authenticated pipe I/O, silent-peer
deadlines, private-endpoint source claims and actual non-GUI child processes.
The watched child survives handoff, unrelated activation, lost Commit ACK and
transient IPC loss, then exits only on its session's close or exact owner's
process death. State tests cover Busy retry, late expired/abandoned Prepare,
bounds, duplicate commits, retained completion, startup admission, delayed
foreign import, per-tab hosts, dirty-path conflicts, save failure/cancellation,
background saves, Save As, committed/cancelled exit and lost-proxy recovery.

Completed Windows checks (Rust 1.99.0, MSVC x64, dev/test debug information 0,
no incremental build, four jobs): the App suite passed 507 active tests with
55 opt-in tests ignored; the final desktop suite and strict lint/build receipts
are recorded in the PR validation. Runtime chemistry tests explicitly selected
the retained built base executable through `RESHIKI_INCHI_HELPER`, matching the
existing CI helper contract rather than attempting to relaunch a test harness.

The Linux native module is cross-checked for x86_64 GNU with Rust 1.99.0 and
pinned rustix 1.1.4 in an isolated metadata-only target. This is not Linux socket
runtime or desktop acceptance. The narrow `Desktop IPC runtime` Ubuntu job
runs `scripts/test_linux_desktop_ipc.py`: it copies the exact module/tests into
a temporary rustix-only package, prints source hashes and its dependency lock,
and runs strict Clippy plus real socket/flock/UID/child-generation tests.
It requires no display or GUI/clipboard dependencies. Its native result is
pending the branch's CI run.

**Pending acceptance:** matched real Windows/Linux shell-open interaction and
screenshots; simultaneous cold launch and first-Office-launch timing;
ordinary/Office A/Office B mixed tabs with actual host-specific save-back,
delayed host acknowledgement, cancellation, Save As, transient IPC loss and
host/proxy termination; Linux UID/mode/symlink/stale-socket runtime checks;
X11/Wayland activation and session-scoping checks; macOS Office regression.
These are required before claiming full issue acceptance. See
[visual-review.md](../visual-review.md).

For each native desktop, start the declared base with file A, edit it, then
launch file B, a duplicate A and a no-argument activation from a separate
working directory. Capture the same state in the candidate with process IDs,
argument lists and exit codes. Keep fixtures, renderer/zoom, data directories
and framing matched. Repeat with a dirty Untitled drawing and an invalid file.
For Office, observe the exact process each host launched: opening/closing other
tabs must not complete it, and its own close must complete it while ordinary
tabs stay open. Compare chemical graph/style and saved host payloads separately
from appearance. No test helper is an actual Office adapter or GUI capture.

Release-note caption: “Windows and Linux file launches open tabs in the running
editor, with each Office edit session saving back to its own document.”

Image alt text: “Two file launches before and after the change: the updated
editor contains both drawings in one window with separate document tabs.”
Image links await the actual matched captures.
