---
section: Changes
issues: []
---

- **Updates are kept in one folder and cleaned up.** An update downloads to
  `updates/<version>/` in the data folder (`~/.reshiki/updates/` on macOS and
  Linux) and is verified there, instead of in a hidden `.reshiki-update-…`
  folder beside the app. ReShiki removes it once the update is installed, with
  the previous version of the app, and removes whatever an interrupted update
  left, old `.reshiki-update-…` folders included, the next time it starts. The
  last check for updates is in `updates/stable.json` or
  `updates/nightly.json`. See [Updates](https://reshiki.com/guide/install/#updates).
