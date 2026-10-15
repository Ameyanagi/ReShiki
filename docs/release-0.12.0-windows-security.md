# v0.12.0 Windows security investigation

## Release identity

The four Windows assets were downloaded from the official
[v0.12.0 release](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.12.0) on
2026-10-15 and matched its `SHA256SUMS`. Both portable packages identify source
commit `4bfadead3ee131ab74a355c854ed2c0c3c5f4084` in `build.json` and report
`signed: false`. Direct PE inspection found an empty certificate directory in
both setup programs and both extracted `reshiki.exe` files.

The [release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/38576354535)
passed its package and native installer checks with Windows signing skipped.
Those checks are not antivirus or browser-reputation acceptance. See the
[release validation record](release-0.12.0-validation.md).

## VirusTotal observations

These are manually observed completed public web reports, not native endpoint
tests. Times below are UTC on **2026-10-15**. The links identify the exact SHA-256
and may show a newer analysis when opened later.

| File                                                                                                                      | Analysis time | Detections / reported engines | Detecting vendors                                                                                     |
| ------------------------------------------------------------------------------------------------------------------------- | ------------- | ----------------------------- | ----------------------------------------------------------------------------------------------------- |
| [x64 setup](https://www.virustotal.com/gui/file/99c72cf8b7f66273d2e9d29f35bc7c1e029c36851d27e53a106b102c55d75e55)         | 09:44:39      | 3/63                          | Arctic Wolf: `Unsafe`; CrowdStrike Falcon: `Win/malicious_confidence_60% (D)`; SecureAge: `Malicious` |
| [ARM64 setup](https://www.virustotal.com/gui/file/58d1d2afec48a514207c9472ad122f4aa340be00a78ae385b4d30b9465c05c65)       | 09:50:58      | 2/71                          | Arctic Wolf: `Unsafe`; CrowdStrike Falcon: `Win/malicious_confidence_60% (D)`                         |
| [x64 application](https://www.virustotal.com/gui/file/f8e2ca68b56c3e97945f9cd27640c82b2a7819c7ca3231073284896132e91856)   | 09:46:44      | 1/61                          | Trapmine: `Malicious.high.ml.score`                                                                   |
| [ARM64 application](https://www.virustotal.com/gui/file/2d3df73defabacf21c2990895ee37390c1872ad424daf6ca6bc80b9262629376) | 09:48:17      | 0/67                          | None reported                                                                                         |
| [x64 ZIP](https://www.virustotal.com/gui/file/765766c47cd68bf1604886f207fe9e7dcc9a59369cb310e4090665f23bdfdcf1)           | 09:48:33      | 0/65                          | None reported                                                                                         |
| [ARM64 ZIP](https://www.virustotal.com/gui/file/004cf54257e3b57535669a014872ed6736764ecf3b61164517db5b9e822ba198)         | 09:49:11      | 0/63                          | None reported                                                                                         |

Microsoft reported `Undetected` in all six reports above. Bitdefender reported
`Undetected` in five; its x64-setup scan timed out. The initial ARM64-setup report
at 09:46:44 was 3/42, including Microsoft's `Trojan:Win32/Wacatac.B!ml` and a
Bitdefender timeout. A repeat analysis of the **unchanged SHA-256** produced the
2/71 result above, with both engines reporting `Undetected`. The initial detection
remains part of the evidence. A requested repeat analysis of the x64 setup stopped
at a CAPTCHA, so its row retains the original completed result.

An archive's zero detections do not establish that its extracted executable is
accepted: the x64 ZIP and its application have different results here. Engine
timeouts and changing definitions also limit comparisons. No particular source
function or false positive has been established by these labels.

## Native Defender observations

All six exact files above were scanned individually on an x64 Windows 11 Pro
25H2 host (build `26200.9457`) on 2026-10-15, between 10:15:34 and 10:17:37 UTC.
The evidence collector reported `scan_completed_no_detection` for every file.
Each row retained an unchanged expected SHA-256 and matching Defender event 1000
and 1001 scan IDs for that target, with successful scan-command completion and
no collector findings requiring review.

Defender platform `4.18.26090.9`, engine `1.1.26090.9` and definitions
`1.461.35.0` stayed unchanged across the observations. Defender was in `Normal`
mode with real-time, behavior, on-access, IOAV and antivirus protection active.
No exclusions or threat-specific action overrides were configured, and all five
severity default actions used the security-intelligence default. Existing cloud
reporting and sample-submission settings were preserved. No protection policy
was changed for these scans.

The files were transferred from the already verified release downloads, and
only `reshiki.exe` was extracted from each portable ZIP. The transferred files
had no `Zone.Identifier` stream. Scanning was conducted through an elevated SSH
session; no ReShiki executable or installer was launched. Smart App Control was
already off and PUA protection was in audit mode. Therefore these results do not
cover SmartScreen, browser download reputation, Smart App Control enforcement,
PUA blocking, standard-user installation or application execution. The ARM64
files were scanned as data on the x64 host; no ARM64 runtime acceptance is implied.

The local raw event XML, before/after policy snapshots, file identities and
reports are retained under
`artifacts/windows-security/v0.12.0/native/20261015T101326Z/`. That directory is
ignored by Git because raw host evidence can contain local paths and policy
details. The web observations are retained alongside it under `web/`.

The host's existing Defender history also contains an older detection for a file
named `reshiki-0.9.1-windows-x64.zip`: `Trojan:Win32/Suschil!rfn`, initially
detected on 2026-10-03 at 04:57:15 UTC. The record reports successful remediation,
an inactive threat and `DidThreatExecute=false`. It does not retain a file hash,
so the filename alone cannot identify the exact historical release bytes. This
record does not describe v0.12.0 and does not establish which warning originally
blocked the reported installation. Only Windows Defender was registered as an
antivirus provider on the inspected host.

## Remediation status

The live SignPath portal was rechecked on 2026-10-15: the production certificate
remains `CSR PENDING`, and the release policy remains `INVALID`. The integration
has passed [test signing](https://github.com/Ameyanagi/ReShiki/actions/runs/37822934801),
but production signing requires the Foundation's setup review, an issued
certificate, the certificate pin and a successful approved production run. See
[SignPath setup](signpath-setup.md). Test certificates are not publicly trusted.

Vendor review requests were submitted on **2026-10-15** after maintainer approval:
CrowdStrike, Arctic Wolf and Trapmine requests were sent by email, and SecureAge's
web form accepted the x64 installer with a `Report Submitted.` confirmation.
These submissions request review; no vendor clearance has been confirmed.
The SignPath setup review follow-up remains an unsent draft. The vendor requests provide
the exact hashes, public release/source links, analysis dates and labels, and ask
for correction if erroneous or specific findings if warranted.

CrowdStrike acknowledged receipt at 11:39 UTC and requested samples through
Hybrid Analysis in addition to the SHA-256 hashes already supplied. Public
searches covering the last 30 days returned no results for either installer
hash. Following maintainer approval of the uploads and terms, both samples were
accepted by Hybrid Analysis with matching SHA-256 values:

- [x64 setup](https://hybrid-analysis.com/sample/99c72cf8b7f66273d2e9d29f35bc7c1e029c36851d27e53a106b102c55d75e55),
  submitted at 12:11 UTC for Quick Scan and a Windows 10 64-bit sandbox analysis.
- [ARM64 setup](https://hybrid-analysis.com/sample/58d1d2afec48a514207c9472ad122f4aa340be00a78ae385b4d30b9465c05c65),
  submitted at 12:12 UTC for Quick Scan. The offered sandbox environments did not
  include ARM64, so no ARM64 runtime analysis was requested.

Both report links were sent in the existing CrowdStrike review thread. These
uploads complete the sample-submission step requested in the acknowledgment;
they do not establish a corrected classification. Analysis was still in progress
when the links were sent. Early MetaDefender results on both pages included
Aurora `Malware_60.0` and Filseclab `UDS:Trojan.Generic.jyce`; these observations
are separate from the VirusTotal results above and are not vendor review findings.
At 12:14 UTC, the ARM64 page no longer displayed analysis in progress and showed
2/24 MetaDefender detections, `MALICIOUS` / `Trojan.Generic`, and a threat score
of 100/100. The x64 page displayed 2/23 detections and the same classification
and score while its sandbox analysis remained in progress. No specific harmful
application behavior or false positive has been established by these results.

### Completed x64 sandbox report

At 12:34 UTC, the [x64 sandbox report](https://hybrid-analysis.com/sample/99c72cf8b7f66273d2e9d29f35bc7c1e029c36851d27e53a106b102c55d75e55/6ad0c2daa00c5a913c058cd5)
was complete, with a score of **76/100**, the `MALICIOUS` / `Trojan.Generic`
classification, one malicious indicator, 14 suspicious indicators and 217
informative indicators. The MetaDefender detections remained 2/23. The ARM64
Quick Scan page remained at 100/100 and 2/24; it has no runtime analysis, so these
two scores should not be treated as equivalent measurements.

The malicious indicator was **"Writes data to a remote process"**. Its expanded
details attributed writes of `0xFB8` bytes followed by eight bytes to each of two
parent/child transitions: `setup.exe` to its extracted `setup.tmp`, and
`setup.tmp` to `reshiki.exe`. The report does not by itself establish the reason
for these writes or identify a ReShiki source function responsible for them.

The registry-command indicator included the Restart Manager registration,
ReShiki's OLE `LocalServer32` command and drawing-file open command. These
registrations are consistent with the installer configuration. Other flags
included executable extraction, temporary executable extensions, file deletion,
debugger checks and VM-related registry access; the labels alone do not establish
malicious intent.

The report recorded no relevant DNS requests, contacted hosts or HTTP requests.
Its process tree shows `reshiki.exe` followed by `WerFault.exe`, and its warnings
explicitly report a process crash. The public report also says some low-level
data is hidden and some lookups timed out. Consequently the run is not evidence
of successful normal application operation or a complete safety verdict.

### Investigation of memory writes and startup failure

The `0xFB8` plus eight-byte write pattern has a specific alternative explanation
in Windows process initialization. [BSI's Windows application compatibility
analysis](https://www.bsi.bund.de/SharedDocs/Downloads/DE/BSI/Cyber-Sicherheit/SiSyPHus/Workpackage3_Analyse_ACI.pdf?__blob=publicationFile&v=2),
section 2.1.1.2, page 26, describes `CreateProcessInternalW` writing a SHIM data
block into a child process and then writing the block address to the child's
`PEB.pShimData`. An [NSFOCUS debugger investigation](https://blog.nsfocus.net/x64-win10-shim/)
observed these two writes while launching Paint, Notepad and Calculator, with
`0xFB8`-byte SHIM blocks on that x64 Windows 10 version. This is evidence for a
plausible Windows mechanism, not attribution of this sample's writes. Confirming
it requires the actual buffers, destination addresses and call stacks; structure
layouts differ across Windows versions. The inspected Inno Setup 6.7.3
[loader](https://github.com/jrsoftware/issrc/blob/4adf37ed7f3fd2bd11c6836ba056e3de170fbabf/Projects/SetupLdr.dpr#L200)
and [installer](https://github.com/jrsoftware/issrc/blob/4adf37ed7f3fd2bd11c6836ba056e3de170fbabf/Projects/Src/Setup.RedirFunc.pas#L220)
launch paths use ordinary `CreateProcess` calls.

The sandbox application process has the same SHA-256 as the released x64
application. Its loaded modules include the Visual C++ runtime, DirectX 12 and
VMware OpenGL libraries. The final entries in its 169 recorded file operations
include `vm3dgl64.dll`, `vm3dglhelper64.dll`, `winmm.dll` and `winmmbase.dll`, then
two further opens of `vm3dgl64.dll` immediately before `WerFault.exe`. The public
view supplies no exception code or crash stack. These observations support
investigating graphics initialization, but do not establish the faulting module.

The exact released x64 application was also launched in an isolated data/profile
directory on the Windows 11 host. CLI help and graphics diagnostics exited
successfully. Diagnostics found only the DX12 CPU adapter, Microsoft Basic
Render Driver, and selected Tiny Skia. The GUI stayed alive for 20 seconds and a
second observation found a responsive hidden main window; both probe-owned
processes were then stopped. No corresponding new Application crash event or
WER report was found. This was SSH session 0, so the observation does not verify
interactive display or reproduce the older sandbox's Windows/VMware environment.
Raw observations are retained in
`artifacts/windows-security/v0.12.0/runtime/runtime-summary.json` and its sibling
files.

At 12:50 UTC, the Windows SHIM explanation and its verification criteria were
sent in the existing CrowdStrike review thread. That follow-up also requested
the sandbox exception code, faulting module/address or crash stack. It explicitly
left both the static classification and crash cause unresolved. At that time,
the only received vendor messages remained the two acknowledgments below.

Arctic Wolf acknowledged receipt at 11:44 UTC. Its required VirusTotal report
links, expected application/installer behavior, developer name and contact email
are present in the sent request. The acknowledgment is not a classification
decision; it also states that a further response is not guaranteed.

At 16:01 UTC, the Aurora `Malware_60.0` observations and both Hybrid Analysis
installer links were added to the existing Arctic Wolf review. [OPSWAT's engine
naming notice](https://www.opswat.com/blog/metascan-multiscanning-anti-malware-vendor-name-updates-coming-october-1st)
identifies Aurora as Arctic Wolf's anti-malware engine. This follow-up asks for
review of those classifications on the unchanged installer hashes; it is not a
vendor decision.

| Vendor      | Review channel                                                                                                                   | Scope                     |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| CrowdStrike | `VTscanner@crowdstrike.com`, listed on its [contact page](https://www.crowdstrike.com/en-us/contact-us/)                         | Both setup programs       |
| Arctic Wolf | `virustotal@arcticwolf.com`, listed in [VirusTotal's vendor directory](https://docs.virustotal.com/docs/false-positive-contacts) | Both setup programs       |
| SecureAge   | False Positive form on its [contact page](https://www.secureage.com/contact-us)                                                  | x64 setup                 |
| Trapmine    | `fp@trapmine.com`, listed in its [scanner notice](https://www.trapmine.com/)                                                     | Extracted x64 application |

The sent-message identifiers and SecureAge confirmation are retained locally in
`artifacts/windows-security/v0.12.0/review/submissions.json`. Record each vendor's
finding and recheck the same file hashes before marking its detection resolved.

[VirusTotal directs classification disputes to the detecting vendors](https://docs.virustotal.com/docs/false-positive-contacts);
it cannot change their verdicts itself. A newly signed build needs its own hashes
and observations. Signing identifies the publisher and can help Windows
reputation, but [does not guarantee that a new binary avoids SmartScreen warnings](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

## Windows startup candidate

The local startup change restricts automatic Windows adapter discovery and
Iced's subsequent WGPU initialization to Direct3D 12. Previously, discovery
initialized every enabled backend before deciding whether to use Tiny Skia.
The new preference avoids unnecessary OpenGL/Vulkan driver initialization and
retains the CPU/no-adapter software fallback. Effective explicit `ICED_BACKEND`
preferences and all existing `WGPU_BACKEND` values remain respected. Graphics
diagnostics also skip discovery when an explicit renderer list cannot select
WGPU; see the [Windows guide](windows.md#release-performance-and-debugging).

A native Windows x64 release build completed successfully at 13:10 UTC on
2026-10-15, using pinned Rust 1.99.0, `--locked` and
`RUSTFLAGS=-C target-feature=+crt-static`. The source snapshot was commit
`0bbfce713353f05977679a4cbfee92645b314959` plus the reviewed startup/documentation
patch. The unsigned, unpublished application is 70,577,664 bytes with SHA-256
`882395b2e3e1dbdafe225088c8bbbdbef2ddb8ccd3e08976c11e6c219f86efb3`.
Its direct PE imports contain no Visual C++ runtime DLL dependency.

At 13:12 UTC, all 11 focused graphics-diagnostic cases passed: default DX12,
explicit software including its alias, explicit WGPU/DX12 and Vulkan choices,
empty/invalid WGPU values, and empty/comma-only/invalid Iced choices. Automatic
and explicitly selected Tiny Skia launches each remained alive for ten seconds,
created a responsive main window, and exited with status zero when that probe's
window was closed. No new matching Application crash events were returned.
These observations were in noninteractive SSH session 0 on a CPU-only Windows
11 VM; they do not validate interactive display, hardware GPU performance or
ARM64 execution. The candidate, build log, source hashes and raw observations
are retained under `artifacts/windows-security/v0.12.0/runtime/candidate/`.

The exact candidate's Defender scan completed at 13:13 UTC with
`scan_completed_no_detection`, unchanged SHA-256 and matching scan ID
`{2F5272F1-874C-4136-8D16-D5020520DEA3}`. Signatures `1.461.37.0` remained
unchanged during this scan; the collector confirmed normal protection before
and after it. The file remains unsigned. This result applies to the candidate
application only, not to an installer or other antivirus products.

This is a targeted startup robustness change. It does not modify installer
process creation or establish that the reported memory writes are harmful, and
it does not itself clear any vendor verdict.

### Direct application sandbox comparison

After explicit approval of both public uploads and the service terms, the
original x64 application and this candidate were submitted directly to Hybrid
Analysis's Windows 10 64-bit environment with the same default scan settings on
2026-10-15. This avoids comparing an installer-launched original with a directly
launched candidate. Both upload hashes matched the local files.

| Application                                   | Sandbox submission (UTC) | Report                                                                                                                                                   |
| --------------------------------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Released x64 application, `f8e2ca68…32e91856` | 15:32:30                 | [Original runtime report](https://hybrid-analysis.com/sample/f8e2ca68b56c3e97945f9cd27640c82b2a7819c7ca3231073284896132e91856/6ad0f20d55954be7400ab0c0)  |
| DX12 startup candidate, `882395b2…9f86efb3`   | 15:33:27                 | [Candidate runtime report](https://hybrid-analysis.com/sample/882395b2e3e1dbdafe225088c8bbbdbef2ddb8ccd3e08976c11e6c219f86efb3/6ad0f24741906f1b1a01ff54) |

Both application scans reported zero detections among 26 MetaDefender engines.
By 15:42 UTC, both runtime reports were available:

| Observation                                      | Released application                                          | DX12 startup candidate                                             |
| ------------------------------------------------ | ------------------------------------------------------------- | ------------------------------------------------------------------ |
| Runtime score                                    | 0/100, no verdict                                             | 0/100, no specific threat; page title says Clean                   |
| Indicators: malicious / suspicious / informative | 0 / 7 / 50                                                    | 0 / 9 / 82                                                         |
| Application startup                              | Windows application-error dialog; memory could not be written | ReShiki editor visibly rendered in screenshot 3, with Ready status |
| Crash evidence                                   | Explicit crash warning and three `WerFault.exe` processes     | No crash warning; one application process and no `WerFault.exe`    |
| VMware OpenGL modules                            | `vm3dgl64.dll` and `vm3dglhelper64.dll` loaded                | Neither appears in the displayed 76-module list                    |

The screenshots show the same Windows 10 build, `16299.rs3_release.170928-1534`.
The candidate still loads Windows' `opengl32.dll` and `glu32.dll`; the absence of
the VMware libraries from its displayed module list is consistent with avoiding
that initialization path. Its modules include
Direct3D 12 and WARP. Its only displayed network request was a successful HTTPS
GET to `api.github.com/repos/Ameyanagi/ReShiki/releases/latest`, matching the
stable-update check in `src/updates.rs`.

This comparison, one run per build, supports that the startup change resolves
the reproduced failure in this sandbox environment. It does not establish
repeatability or identical VM configuration, identify the original faulting
instruction or prove that every flagged API is benign. Both public reports
hide low-level data, and the candidate report also omits some file operations.
The candidate's nine suspicious indicators remain recorded. Its generic
signed-certificate indicator does not override the native Authenticode result:
the candidate is unsigned. These application results do not clear the earlier
installer verdicts. The latest scoped vendor-message check at 15:34 UTC still
returned only the two automatic acknowledgments. Structured observations are
retained in `artifacts/windows-security/v0.12.0/web/startup-comparison.json`.

At 15:43 UTC the two completed report links, exact hashes and comparison were
sent in the existing CrowdStrike review thread. The message retains the request
to review both original installer classifications and verify the SHIM data
hypothesis; it does not ask to treat the candidate result as installer clearance.

## Installer remediation work

### Controlled installer candidate

At 16:04 UTC on 2026-10-15, a local x64 setup was built with the original
`packaging/windows/reshiki.iss`, its original notice and icon, and the exact DX12
application candidate above. The remaining payload came from the verified
v0.12.0 portable ZIP; `build.json` records the local candidate provenance.
The official Inno Setup 6.7.3 compiler distribution matched the release
workflow's pinned SHA-256 and had a valid Pyrsys B.V. Authenticode signature.
The setup was compiled but not executed on the local Windows host.

The resulting unsigned installer is **19,336,901 bytes**, SHA-256
`dc14f057f07b70a12ec0598dd91a1bbb2afe63bbabff326264cca121daee3984`.
Its normal metadata-normalization step preserved file length and every byte
outside the existing version resource and PE checksum, including the compressed
payload. Native version fields report `ReShiki` and `0.12.0`.

The exact-file Defender scan returned `scan_completed_no_detection`, with
matching events 1000/1001 and scan ID
`{0FC429D7-A3EA-40EB-998B-1164A7FB5624}`. Definitions `1.461.37.0` stayed
unchanged. Normal real-time, behavior, on-access and IOAV protection remained
enabled, with no exclusions or policy changes. This is static-scan evidence,
not installation, SmartScreen or other-vendor acceptance. Local provenance,
the installer and raw observations are retained under
`artifacts/windows-security/v0.12.0/installer-comparison/20261015T155812Z/`.

After approval of the new uploads and public-sharing terms, [VirusTotal's
candidate report](https://www.virustotal.com/gui/file/dc14f057f07b70a12ec0598dd91a1bbb2afe63bbabff326264cca121daee3984)
completed at 16:10:09 UTC with **3/69**: Arctic Wolf `Unsafe`, CrowdStrike Falcon
`Win/malicious_confidence_60% (D)` and Microsoft `Trojan:Win32/Wacatac.B!ml`.
SecureAge and BitDefender reported `Undetected`. Preserve the Microsoft web-engine
detection alongside the local Defender no-detection result; they are distinct
observations. The startup change has not cleared the installer classifications.
The [candidate Windows 10 sandbox analysis](https://hybrid-analysis.com/sample/dc14f057f07b70a12ec0598dd91a1bbb2afe63bbabff326264cca121daee3984/6ad0fb4a731e1acb5d07b442)
was submitted at 16:11:54 UTC and observed complete by 16:21 UTC. It remains
`MALICIOUS` / `Trojan.Generic`, with a score of **81/100**, one malicious,
14 suspicious and 227 informative indicators in the detailed runtime view.
MetaDefender reports **2/24**: Aurora `Malware_60.0` and Filseclab
`UDS:Trojan.Generic.jyce`. The overview's runtime row separately displays 59
suspicious indicators; the counts above refer to the detailed report.

Screenshot 10 shows the installed editor fully drawn. The four-process tree
contains the setup loader, its temporary inner executable and two application
launches, with no recorded `WerFault.exe` or process-crash warning. The sole
malicious indicator still records `0xFB8` plus eight-byte remote writes for
the loader-to-inner and inner-to-application launches. The Windows SHIM
explanation above remains unconfirmed because the public report does not expose
the necessary buffers or call stacks.

The application contacted `api.github.com` on TCP port 443 and received HTTP 200
for `GET /repos/Ameyanagi/ReShiki/releases/latest`, matching the update-check
endpoint in `src/updates.rs`. This run reached behavior the crashing original
could not demonstrate. Its higher score is not evidence that the startup change
introduced malware, and successful installation is not vendor clearance.
This is one sandbox run with some low-level data and file operations hidden.
Structured observations are retained in
`artifacts/windows-security/v0.12.0/web/installer-candidate-comparison.json`.

The completed comparison and new candidate hash were sent in the existing
CrowdStrike review thread at 16:26 UTC. The request retains both the static
classification review and the specific SHIM-attribution question. The latest
scoped inbox check at 16:17 UTC returned only the earlier automatic
acknowledgments. Microsoft and Filseclab review text is prepared locally and
awaits maintainer authorization; neither review has been submitted.

### Signature coverage

The existing signing workflow covers the application and outer setup only.
Inno Setup documents that [`SignedUninstaller=yes`](https://jrsoftware.org/ishelp/topic_setup_signeduninstaller.htm)
also signs temporary Setup self-copies. A separate native capture confirmed the
supported detached workflow: compilation emitted an unsigned inner cache file
and stopped with the expected request for a signature. No product installer or
captured inner executable was run for that observation.

A separate local signing extension is prepared to stage the application and
inner executable in one signing request, verify both returned signatures and
unchanged executable contents, and recompile with the signed cache before the
outer setup request. It still requires configuration approval and SignPath
test-policy validation on both architectures. The nightly startup fix retains
the existing application/outer-setup signing workflow. No evidence establishes
that unsigned inner files caused the antivirus detections.
At 16:23 UTC the portal still showed the production certificate as `CSR PENDING`
and the release policy as `INVALID`. See [SignPath setup](signpath-setup.md) for
the existing integration and production activation requirements.

## Remaining acceptance

Use the [Windows evidence collector](windows-security-evidence.md) to retain
exact-file Defender evidence under normal protection. Separately observe browser
download, extraction, standard-user installation, launch, upgrade and uninstall
on supported Windows clients, including the security product that originally
blocked the user. A static scan does not establish those behaviors. Do not close
the issue solely because a web scan changes or a certificate becomes available.
