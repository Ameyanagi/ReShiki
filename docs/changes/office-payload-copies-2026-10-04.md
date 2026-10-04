# Office and LibreOffice payload copies

These internal refactors remove duplicated payload conversions and retained data
while preserving existing validation errors, request/ACK ordering, recovery
behavior and document formats. The baseline is
`81ca82101061ecc201545a3cae8d8257f03254d3`; the production/test change is
`d2dfbe26ae7437131c3f29c58a6596376e0f21a2`.
[PR #134](https://github.com/Ameyanagi/ReShiki/pull/134) is under review.

| Change                                                | Preserved contract                                                                                                                                |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Office readback uses the existing envelope comparator | Initial and post-update validation, exact error precedence, all native/PNG/revision/version/extent comparisons, and applied/ACK timing            |
| Companion launch reuses one decoded Buffer            | Worker input and recovery bytes; worker completion, lease touch and revision checks still precede recovery writes and launch                      |
| Office storage records retain XML length              | JavaScript string units, sequential reads, the 8192-unit margin and immutable historical records                                                  |
| Word/PowerPoint rollback cleanup                      | Same host reads/writes, geometry-field order, rollback and error wrapping                                                                         |
| LibreOffice binary storage bypasses base64 roundtrips | Native/PNG bytes, limits, metadata decoding, stream close/dispose order and exception behavior; public worker packets still require strict base64 |

Production code decreases by **15 physical and nonblank lines** in aggregate;
test code grows by 540 physical lines. The LibreOffice source gains three lines
to preserve unusual input-buffer and stream-close behavior. No particular heap
or latency reduction was measured.

## Regression checks

- **158 Office Node tests passed**, no failures, cancellations or skips, using
  Node 24.19.0. New cases cover PNG/extent mismatches with equal native/revision,
  missing/invalid readback error order, a 1 MiB opaque binary recovery payload,
  worker completion versus lease/revision failures, and XML string units versus
  UTF-8 byte length at the storage-capacity boundary.
- **15 source-extracted LibreOffice packet/storage tests passed** on Python
  3.12.12. The same behavior contracts passed against the baseline in **14
  cases**, omitting only the candidate's intentional no-base64 assertion.
  These tests cover unusual binary/metadata types, conversion after a mutable
  buffer's close callback, bounds, read/close/dispose failures, assignment order,
  exact exceptions and the unchanged public packet protocol.
- **171 complete controlled LibreOffice tests passed, no skips**, using the
  matching installed Linux Python UNO runtime described below.
- Office oxlint/oxfmt, Python Ruff checks and `git diff --check` passed.

```sh
node --test integrations/office/office.test.js integrations/office/taskpane.test.js integrations/office/taskpane-lifecycle.test.js integrations/office/host-adapters/tests/host-adapters.test.js integrations/office/host-adapters/tests/word-id-domain.test.js integrations/office/host-adapters/tests/word-ooxml.test.js
python3.12 -B -m unittest discover -s integrations/libreoffice/tests -p test_packet_storage.py -v
# With matching installed Python UNO bindings:
python3 -B -m unittest discover -s integrations/libreoffice/tests -p 'test_*.py' -v
```

## Real installed Linux UNO verification

A new Ubuntu 24.04 arm64 Docker container used the official Ubuntu
`noble-backports` LibreOffice **26.2.5.2 620(Build:2)** and matching Python UNO
packages, with Python 3.12.3. Writer, Calc, Impress, core, Python script provider
and `python3-uno` all have package version
`4:26.2.5.2-0ubuntu0.26.04.1~bpo24.04.1`. The headless modules are the matched
official `libreoffice-{core,writer,calc,impress}-nogui` packages. Private baseline
and candidate profiles were installed from their separately built OXT packages.
Every harness checks that the deployed extension source exactly matches the
selected source bytes before testing. No host desktop or user profile was used.

The existing `integrations/libreoffice/tests/roundtrip.py` passed for both
installed packages in ODT, ODS and ODP: two distinct objects per document and
two save/close/reopen cycles. The candidate also loaded, verified, saved and
reopened baseline-authored documents. A supplemental driver used the same
repository `verify` and object enumeration helpers to call real
`XEmbedPersist.reload((), ())` before and after Save As while each document was
open, then close/reopen and verify again. All three formats passed for each
package, with 12 explicit reload calls per package. The baseline additionally
read, reloaded and saved candidate-authored files successfully.

These checks preserve exact native and PNG transfer-data bytes, intrinsic
extent, distinct object identities, and actual host frame dimensions within
the existing 0.03 mm tolerance. An independent ZIP audit compared all embedded
native/PNG/metadata streams with the worker packet: **24 output ODF documents
and 48 object storages matched exactly**. Twelve real PyUNO binary stream reads
across candidate ODT/ODS/ODP returned `ByteSequence.value` as exact `bytes`, with
reported counts equal to their lengths and expected hashes.

The packet was generated by the real candidate debug ReShiki preview worker on
macOS from `tests/fixtures/chemdraw-captions/source.rsk`. Its native drawing is
77,245 bytes; PNG is 845,174 bytes and 5946 × 3902 pixels; extent is
`[12586, 8258]` hundredths of a millimetre. Linux persistence used `--packet`,
so this run does not verify Linux rendering or font behavior. The candidate
binary included the images/app/core/B01 changes but not the separately pending
chemistry/platform changes. No native data or PNG was invented for this check.

Exact runtime/source/package/worker hashes, commands, counts and evidence
locations are in the adjacent
[validation manifest](office-payload-copies-2026-10-04-manifest.json).
Large ODF/PNG artifacts remain outside Git in
`/tmp/reshiki-improvements-20261004/evidence/uno-c07/container-evidence/`.
That directory retains both installed OXT packages, packet provenance,
installation and test logs, per-case reports, all 24 output documents,
`storage-audit.json` with document hashes and object identities, and supplemental
driver sources. The container `reshiki-refactor-uno-20261004` is stopped and
retained; it had no bind mounts.

On 2026-10-04, all 68 saved evidence files were checked against
`evidence/uno-c07/SHA256SUMS.json`, together with each output document's recorded
hash. Baseline/candidate source hashes were checked against the immutable
commits above. The actual fixture and preview executable still matched their
captured provenance hashes. This verifies the local evidence named by the
manifest; the large artifacts are not distributed with the repository.

Reproduce the supported headless setup with matched packages and an isolated
profile, then run `roundtrip.py` once per selected source/package. Reuse an
actual preview-worker packet, not a synthetic PNG/native pair:

```sh
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends -t noble-backports libreoffice-core-nogui python3-uno libreoffice-writer-nogui libreoffice-calc-nogui libreoffice-impress-nogui libreoffice-script-provider-python
python3 -B integrations/libreoffice/build_extension.py /work/evidence/candidate.oxt
env SAL_USE_VCLPLUGIN=svp unopkg add --force --suppress-license -env:UserInstallation=file:///work/profiles/candidate /work/evidence/candidate.oxt
env SAL_USE_VCLPLUGIN=svp libreoffice -env:UserInstallation=file:///work/profiles/candidate --headless --norestore --nodefault --nofirststartwizard '--accept=pipe,name=reshiki-refactor-candidate;urp;StarOffice.ServiceManager'
python3 -B integrations/libreoffice/tests/roundtrip.py --uno-url 'uno:pipe,name=reshiki-refactor-candidate;urp;StarOffice.ComponentContext' --packet /work/evidence/preview-packet.json --output /work/evidence/candidate-roundtrip --incoming /work/evidence/baseline-roundtrip
```

Distribution packages were installed as root inside the dedicated container.
Extension installation, UNO tests and LibreOffice used the private unprivileged
`qa` account. The process-start command runs in the background; wait for its UNO
endpoint before starting the Python harness. The manifest records the
supplemental driver's exact commands and source hash.

## Acceptance limits

This verifies real installed Linux arm64 UNO persistence on LibreOffice 26.2.5.2.
Windows/macOS LibreOffice GUI behavior and other supported runtime combinations
remain user acceptance checks. Desktop double-click activation, actual ReShiki
editing/save-back, clipboard exchange and native close/Save/Discard/Cancel
dispatch were not exercised. Microsoft desktop Word/Excel/PowerPoint were not
launched; their insertion/edit/readback/rollback/ACK acceptance remains separate.
Near-64 MB native drawings were not exercised in the real host. Bounds and
abnormal read/close/dispose/error paths are established by controlled parity
regressions, not native fault injection. No visible drawing or UI change is
intended; validation uses bytes, protocol/error sequencing and persisted object
properties instead of appearance comparisons.
