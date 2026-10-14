# Archival review probes

These scripts are archival review probes. They are kept to reproduce past review records.
They never run in CI and are not part of the release or test tooling.

- [`probe_windows_window.ps1`](probe_windows_window.ps1) checks Windows editor bounds, monitor work area and DPI; see the [window-fit review](../../docs/changes/windows-window-fit.md).
- [`verify_rear_opacity_evidence.py`](verify_rear_opacity_evidence.py) checks retained C60 fixture hashes, history states and optional exports; see the [fixture record](../../tests/fixtures/rear-opacity/README.md) and [rear-opacity review](../../docs/rear-opacity-review.md).
