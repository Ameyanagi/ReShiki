# Windows antivirus evidence

Issue [#62](https://github.com/Ameyanagi/ReShiki/issues/62) needs dated protection
results for exact release files. `scripts/windows_security_evidence.py` collects
one file at a time on Windows and summarizes collected records on any platform.
It does not establish that a reported detection was incorrect, or that signing
would resolve it. Browser delivery, installation, native execution, Bitdefender
and reputation prompts require separate observations.

## Collect a row

Use Python 3.11+ and Windows PowerShell on an isolated Windows test host with its
normal protection enabled. The helper reads Defender status/preferences/threat
history, Authenticode and Mark-of-the-Web, and retains raw Operational event XML.
It does not install or run the target, download files, change policy, update
definitions, unblock files or restore quarantine. `--scan` requests a custom
Defender scan; without it the command only collects observations. Defender's
existing remediation and cloud/sample-submission policy applies unchanged.

Record a UTC start before downloading through the browser. Record the browser
version, download URL and actual prompts/screenshots separately. Preserve the
downloaded file's Mark-of-the-Web and use the expected hash from the qualified
release's `SHA256SUMS`, even if quarantine has already removed the file.

The following is the **v0.10.0 x64 installer identity**, verified in the
2026-10-02 investigation; it is not a scan result. Run from the repository:

```powershell
$downloadStarted = [DateTimeOffset]::UtcNow.ToString('o')
# Now download the exact asset through the browser into C:\ReShiki-AV.
py -3 scripts/windows_security_evidence.py collect C:\ReShiki-AV\reshiki-0.10.0-windows-x64-setup.exe `
  --sha256 98693627713a1a85ca2890b9a6c3c1a83fe49d4017cb4d31bcfa9454bc5153aa `
  --case-id D-X64-SETUP --release-tag v0.10.0 `
  --source-commit c8ffe8c01ca9d561f06805ddaf253f436f5ecbbe `
  --architecture x64 --kind container --phase downloaded-container `
  --source-url https://github.com/Ameyanagi/ReShiki/releases/download/v0.10.0/reshiki-0.10.0-windows-x64-setup.exe `
  --since $downloadStarted --scan --timeout 600 --output C:\ReShiki-AV\evidence\D-X64-SETUP
```

An existing output directory is refused. `evidence.json` is checkpointed during
collection; `report.json` gives the evaluated row. Exit 0 means only that the
exact-file scan completed with no detection observed under the recorded
conditions. Exit 1 means detection evidence or incomplete evidence; exit 2 is an
invocation/report error. Timeouts do not disable Defender or cancel another scan;
the service's scan may continue after the collector stops waiting.

Repeat for both architectures' installer and ZIP, then the normally extracted
application as its **own row with its own SHA-256**. Capture installed application,
uninstaller and any detected temporary setup component separately after an
authorized native installation walkthrough. `--container-sha256` records the
parent package identity as supplied by the operator; it does not prove archive
membership. Compare the installed application's hash against the qualified
portable payload hash. PE machine is read separately, so an x86 installer stub
is not confused with its x64/ARM64 application payload.

For a future candidate, freeze all selected source/dependency/signing/packaging
changes first and calculate new expected hashes. Do not reuse the example hash
or carry an earlier scan result over to rebuilt, resigned or repacked bytes.

## Read the matrix

```powershell
py -3 scripts/windows_security_evidence.py summarize `
  C:\ReShiki-AV\evidence\D-X64-SETUP\evidence.json `
  C:\ReShiki-AV\evidence\D-X64-ZIP\evidence.json `
  C:\ReShiki-AV\evidence\D-X64-PAYLOAD\evidence.json `
  --output C:\ReShiki-AV\matrix-x64
```

This reevaluates raw evidence into `matrix.json` and `matrix.csv`; it does not
trust previously written verdicts. The matrix covers **only the supplied rows**,
not the full supported-platform acceptance list. Review JSON/raw records alongside
the CSV, including definition versions, host architecture/build and exact paths.
Logs can contain local paths and protection-policy details; review before sharing.

- `scan_completed_no_detection` requires matching exact-target event 1000 and
  event 1001 Scan IDs, successful command completion, unchanged expected file
  hashes, active normal Defender status and complete threat/policy/event data.
- `detection_recorded` retains a target-path detection/action even if the command
  returned 0, quarantine removed the file, or a record present before the scan
  disappeared afterward. Historical threat records contain
  paths, not necessarily the scanned sample's hash: review timing and resources
  before attributing an old detection to new bytes at the same path.
- `incomplete` includes missing targets, hash changes, passive/disabled or unknown
  protection, stale definitions, unreadable/truncated logs, cancellation, missing
  completion, protection failure/recovery events, history deletion attempts or
  disappearing records, policy changes and unattributed new detections. Any reported
  exclusion requires review; the helper does not guess whether it applies.

Both policy snapshots must include all five threat-severity default actions and
the paired threat-ID/action overrides. Only the security-intelligence default (0)
and Clean, Quarantine, Remove or Block (1/2/3/10) qualify automatically. Allow (6)
can suppress detection events; user-defined, non-remediating, unknown, missing or
ambiguous actions require review. Captured policy changes also require review
even without a configuration event. Evidence from older collectors that omitted
these fields remains readable but reevaluates as incomplete; collect fresh
evidence rather than filling historical gaps with today's settings.

Signature status is recorded separately and does not affect the antivirus verdict.
Unknown-publisher/UAC, SmartScreen, Smart App Control and runtime-loader failures
are separate observations; a scan alone cannot establish protected installation.

## CI and remaining native acceptance

The existing release workflow builds on Windows Server 2022 x64 and Windows 11
ARM. It runs package/runtime/installer checks but does not collect Defender scan
evidence. The helper can first inspect either runner without `--scan`; if the
provider/policy/logs are unavailable it records an incomplete row. A future scan
step belongs after the final package is created, with evidence preserved even on
failure. No release workflow or publication gate is changed by this helper.

A server/developer runner's static scan is supplementary. Actual protected browser
download, extraction, standard-user installation/upgrade/uninstall, native x64 and
ARM64 launch/chemistry, clean-client runtime dependencies, and affected Bitdefender
product behavior remain required. Record Windows 10 client coverage separately
while that platform remains supported. Do not bypass an execution or policy block
to complete the matrix. A new clean scan does not explain historical detections.

Run the portable regression checks with:

```sh
uv run --no-project --python 3.12 python -m unittest tests.test_windows_security_evidence
```

The tests use synthetic event/status fixtures, not malware or real scan results.
When PowerShell is available, they also invoke the probe with mocked OS queries
to check that action policies are preserved and unsupported properties fail.

Collector validation on 2026-10-02 also used an ordinary text file on Windows 11
Pro 25H2 x64 (26200.9457), Windows PowerShell 5.1 and Python 3.12.13 in an elevated
SSH session. Defender platform 4.18.26080.4, engine 1.1.26080.3 and definitions
1.459.510.0 were active and unchanged. The collector matched native events 1000
and 1001 to the text file and its unchanged hash. Wrong-hash and missing-file
rows skipped scanning and remained incomplete; the combined matrix returned a
nonzero status. All 12 regression tests also passed on that host. This verifies
collection mechanics, not ReShiki release acceptance, standard-user behavior or
GUI interaction. No application/installer bytes were scanned or executed.

Subsequent review added the action-policy requirements above. The earlier native
text-scan evidence omitted those fields and now reevaluates as incomplete. All 19
updated regression tests passed on macOS/PowerShell 7 and Windows/PowerShell 5.1.
A separate read-only Windows snapshot confirmed five default actions of 0 and no
threat-specific overrides; it does not establish policy during the earlier scan.
No new native scan or policy change was performed for this follow-up.

See Microsoft's [Defender event reference](https://learn.microsoft.com/en-us/defender-endpoint/troubleshoot-microsoft-defender-antivirus),
[Start-MpScan](https://learn.microsoft.com/en-us/powershell/module/defender/start-mpscan)
and [Get-MpComputerStatus](https://learn.microsoft.com/en-us/powershell/module/defender/get-mpcomputerstatus)
for the underlying commands and scan/detection event fields. Microsoft's
[remediation guidance](https://learn.microsoft.com/en-us/defender-endpoint/configure-remediation-microsoft-defender-antivirus)
and [action values](https://learn.microsoft.com/en-us/powershell/module/defender/set-mppreference#description)
explain the policy checks.
