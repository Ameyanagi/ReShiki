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

For a fresh headless reproduction, first create baseline inputs, then run the
candidate against them, then read candidate files with the baseline. Use a
checkout of PR #134 and an actual preview-worker JSON packet; replace
`/path/to/preview-packet.json` with that existing packet's host path. The source
trees, extension packages, profiles and output directories are separate.
From the host checkout, prepare a new isolated container:

```sh
docker run --init --detach --name reshiki-uno-pr134-reproduce --memory 2g --cpus 2 --pids-limit 512 ubuntu:24.04 sleep infinity
docker exec reshiki-uno-pr134-reproduce mkdir -p /work/baseline /work/candidate /work/evidence /work/profiles
git archive 81ca82101061ecc201545a3cae8d8257f03254d3 integrations/libreoffice | docker exec -i reshiki-uno-pr134-reproduce tar -x -C /work/baseline
git archive HEAD integrations/libreoffice | docker exec -i reshiki-uno-pr134-reproduce tar -x -C /work/candidate
docker cp /path/to/preview-packet.json reshiki-uno-pr134-reproduce:/work/evidence/preview-packet.json
docker exec -it reshiki-uno-pr134-reproduce bash
```

Run the following inside that container as root. Distribution packages are
installed as root; extension installation, UNO harnesses and LibreOffice use
the private unprivileged `qa` account. Verify the extension source hashes before
building the packages:

| Source                                                          | Expected SHA-256                                                   |
| --------------------------------------------------------------- | ------------------------------------------------------------------ |
| `/work/baseline/integrations/libreoffice/extension/reshiki.py`  | `c2de0f37a89e4ae54662fe63a2f32e3c4f5043b70da0cf2a85347e102661a816` |
| `/work/candidate/integrations/libreoffice/extension/reshiki.py` | `bd282131c4eb87d69c2660f94d28fab6967c5347f0e16c2260c08e64d494a312` |

```sh
set -eu
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends -t noble-backports libreoffice-core-nogui python3-uno libreoffice-writer-nogui libreoffice-calc-nogui libreoffice-impress-nogui libreoffice-script-provider-python
useradd --create-home --shell /bin/bash qa
chown -R qa:qa /work
sha256sum /work/baseline/integrations/libreoffice/extension/reshiki.py /work/candidate/integrations/libreoffice/extension/reshiki.py
for selected_profile in baseline candidate; do
  runuser -u qa -- /usr/bin/python3 -B "/work/$selected_profile/integrations/libreoffice/build_extension.py" "/work/evidence/$selected_profile.oxt"
  runuser -u qa -- env SAL_USE_VCLPLUGIN=svp /usr/bin/unopkg add --force --suppress-license "-env:UserInstallation=file:///work/profiles/$selected_profile" "/work/evidence/$selected_profile.oxt"
done

# Start one isolated profile and wait at most 30 seconds for its UNO endpoint.
start_office() {
  selected_profile=$1
  runuser -u qa -- env SAL_USE_VCLPLUGIN=svp /usr/bin/libreoffice "-env:UserInstallation=file:///work/profiles/$selected_profile" --headless --norestore --nodefault --nofirststartwizard "--accept=pipe,name=reshiki-refactor-$selected_profile;urp;StarOffice.ServiceManager" > "/work/evidence/$selected_profile-soffice.log" 2>&1 &
  office_pid=$!
  runuser -u qa -- /usr/bin/python3 -B - "$selected_profile" <<'PY'
import sys, time, uno
ctx = uno.getComponentContext()
resolver = ctx.ServiceManager.createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", ctx)
url = f"uno:pipe,name=reshiki-refactor-{sys.argv[1]};urp;StarOffice.ComponentContext"
deadline = time.monotonic() + 30
while True:
    try:
        resolver.resolve(url)
        break
    except Exception:
        if time.monotonic() >= deadline:
            raise
        time.sleep(0.25)
PY
}

# Documents are closed by the harness before stopping its selected process.
stop_office() {
  runuser -u qa -- /usr/bin/python3 -B - "$1" <<'PY'
import sys, uno
ctx = uno.getComponentContext()
resolver = ctx.ServiceManager.createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", ctx)
remote = resolver.resolve(f"uno:pipe,name=reshiki-refactor-{sys.argv[1]};urp;StarOffice.ComponentContext")
desktop = remote.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", remote)
assert desktop.terminate()
PY
  wait "$office_pid"
}

# 1. No --incoming: create baseline roundtrip-2.odt, .ods and .odp first.
start_office baseline
runuser -u qa -- /usr/bin/python3 -B /work/baseline/integrations/libreoffice/tests/roundtrip.py --uno-url 'uno:pipe,name=reshiki-refactor-baseline;urp;StarOffice.ComponentContext' --packet /work/evidence/preview-packet.json --output /work/evidence/baseline-roundtrip
stop_office baseline

# 2. The candidate reads the baseline files and creates its own roundtrip-2 files.
start_office candidate
runuser -u qa -- /usr/bin/python3 -B /work/candidate/integrations/libreoffice/tests/roundtrip.py --uno-url 'uno:pipe,name=reshiki-refactor-candidate;urp;StarOffice.ComponentContext' --packet /work/evidence/preview-packet.json --output /work/evidence/candidate-roundtrip --incoming /work/evidence/baseline-roundtrip
stop_office candidate

# 3. Reverse direction, using the baseline source and installed baseline profile.
start_office baseline
runuser -u qa -- /usr/bin/python3 -B /work/baseline/integrations/libreoffice/tests/roundtrip.py --uno-url 'uno:pipe,name=reshiki-refactor-baseline;urp;StarOffice.ComponentContext' --packet /work/evidence/preview-packet.json --output /work/evidence/baseline-imports-candidate --incoming /work/evidence/candidate-roundtrip
stop_office baseline
```

Each `roundtrip.py` run verifies its installed source matches its source tree.
The first run creates the required `baseline-roundtrip/roundtrip-2.*` files;
the second creates `candidate-roundtrip/roundtrip-2.*`. The final repository
harness command imports the latter into `baseline-imports-candidate/imported.*`.
It is a fresh reproduction of the reverse persistence direction using only the
repository harness, rather than the recorded supplemental reload driver's
output.

The recorded 24-file evidence additionally used the retained
`reload_save_as.py` driver, including this reverse check with the baseline
process running. Its source SHA-256 is
`44f93d2cf4c44bfdad55e7a524f51e13ec91119073254b1d0a6c4baf48839b03`;
copy that driver from the evidence directory to `/work/evidence/` to replay the
explicit reload/live Save As checks. Unlike `roundtrip.py`, it requires existing
`roundtrip-2.*` inputs and never creates them:

```sh
start_office baseline
runuser -u qa -- /usr/bin/python3 -B /work/evidence/reload_save_as.py --source-tree /work/baseline --uno-url 'uno:pipe,name=reshiki-refactor-baseline;urp;StarOffice.ComponentContext' --packet /work/evidence/preview-packet.json --incoming /work/evidence/candidate-roundtrip --output /work/evidence/baseline-reading-candidate
stop_office baseline
```

After the fresh runs and any optional supplemental check, leave the container
shell, copy out its `/work/evidence/` reports/documents, and stop the new
`reshiki-uno-pr134-reproduce` container from the host. The original retained
`reshiki-refactor-uno-20261004` evidence container is separate.

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
