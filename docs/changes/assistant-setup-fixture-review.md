# Simulated Assistant setup desktop checks

Status: native macOS arm64 checks completed on the preserved signed
`43e3d80dde9c9f1da1be04e2525182aea390cc75` application. These are account-free
**simulated backend fixtures**, separate from the actual signed-in Ready/example
desktop exercise. The [review gallery](assistant-setup-review-2026-10-09.md#simulated-setup-states-in-the-actual-native-app)
shows the original captures. The real account stayed signed in, its installation
remained present, and no Send or inference occurred.

ReShiki's [documented executable override](../assistant-setup.md#if-it-does-not-connect)
selects the absolute `RESHIKI_CODEX` path before ordinary Codex discovery. Launch
the same signed bundle executable directly with process-local environment
variables, after quitting the previous QA instance. For example on macOS:

```sh
/usr/bin/env \
  RESHIKI_CODEX=/absolute/path/to/signed-out-codex \
  RESHIKI_DATA_DIR=/absolute/path/to/disposable-profile/signed-out \
  '/absolute/path/to/ReShiki.app/Contents/MacOS/reshiki'
```

Use a separate disposable data directory for each fixture. `RESHIKI_DATA_DIR`
selects ReShiki's preferences/appearance/theme location and skips legacy data
migration. It does not redirect the Codex account. Instead, the inspected fixture
executable completely replaces the real Codex process. The scripts below neither
read credentials nor contact a service, and cannot perform inference. Leave HOME,
CODEX_HOME and global model configuration unchanged. No real account logout,
credential deletion, installation removal or launchctl environment change is
needed. A terminal environment change does not affect an already running app;
quit each fixture app before launching the next one.

Save each exact script below with its displayed filename in a disposable directory
and make it executable with `chmod +x /absolute/path/to/fixture`. The signed-out,
broken and stalled fixtures use the system's available Python 3 through
`/usr/bin/env`; this is a test dependency only. The setup child receives the normal
Codex app-server arguments, which these scripts intentionally ignore.
[The evidence receipt](../images/assistant-setup/provenance.json) pins the exact
review fixture hashes. Exact documented scripts and four offline subprocess
preflights were checked separately. Native interaction now confirms the following:

| Simulated fixture                | Observed native result                                                                         |
| -------------------------------- | ---------------------------------------------------------------------------------------------- |
| signed-out-codex                 | Sign-in guidance and Test connection; Send disabled                                            |
| broken-codex                     | Safe version/update/retry guidance; Back to drawing closes Assistant and preserves blank input |
| missing-interpreter-codex        | Install Codex with native Windows codex.exe guidance                                           |
| stalled-codex                    | Checking Codex; Cancel check was clicked                                                       |
| stalled-codex after cancellation | Cancelled card and Test connection; Stop removed                                               |

Every capture used blank Untitled input, 100%, JACS / ACS, Arial 10 pt, keyboard
drawing on and original 2560 × 1704 JPEG bytes. Every foreground process quit with
exit code 0; process-name checks found no corresponding fixture child afterward.
No real account, global configuration or credentials were changed. Other-platform
native GUI checks remain unverified.

## Sign-in required: signed-out-codex

```python
#!/usr/bin/env python3
"""Disposable setup-only backend fixture. Does not read accounts or call any service."""
import json
import sys
for line in sys.stdin:
    event = json.loads(line)
    if 'id' not in event:
        continue
    method = event.get('method')
    if method == 'initialize':
        result = {}
    elif method == 'account/read':
        result = {'account': None, 'requiresOpenaiAuth': True}
    else:
        print(json.dumps({'id': event['id'], 'error': {'code': -32601, 'message': 'Setup fixture only'}}), flush=True)
        continue
    print(json.dumps({'id': event['id'], 'result': result}), flush=True)
```

Open Assistant and expect **Sign in to Codex**, with sign-in guidance and retry.
Use **Test connection** to repeat only the local simulated handshake. Return to
drawing and make an ordinary edit to check that drawing remains available.
Do not invoke the real `codex login` command for this fixture check.

## Broken handshake: broken-codex

```python
#!/usr/bin/env python3
"""Disposable malformed app-server fixture. No accounts, credentials or service calls."""
import sys
for line in sys.stdin:
    print('fixture malformed JSON', flush=True)
    break
```

Expect **Connection needs attention** and the safe handshake/retry message.
The raw malformed response must not appear in the setup card. Test retry and
ordinary drawing separately.

## In-flight cancellation: stalled-codex

```python
#!/usr/bin/env python3
"""Disposable stalled handshake for cancellation screenshots. No service calls."""
import sys
import time
for line in sys.stdin:
    time.sleep(60)
    break
```

While **Checking Codex…** is visible, choose **Cancel check** before the 60-second
handshake timeout. Expect the cancelled state and retry action; ordinary drawing
remains available. This is cancellation of a setup check, not a model request.

## POSIX installation failure: missing-interpreter-codex

```sh
#!/reshiki-fixture/no-codex-interpreter
# Disposable POSIX installation-failure fixture; never accesses an account.
```

The existing absolute fixture is executable but its deliberately absent
interpreter makes the OS subprocess launch return NotFound. Expect **Install
Codex**. This is a POSIX simulated installation failure; the real installed Codex
has not been removed. A missing override file would instead produce the distinct
invalid-executable guidance, so keep this fixture file present.

Label each capture **simulated setup backend fixture**, preserve its original
bytes, and record fixture name/hash, source commit, signed executable hash,
platform, blank input, zoom, keyboard drawing state and exact actions. The linked
review now records the observed native results without treating these fixtures
as an actual account logout or removed Codex installation.
