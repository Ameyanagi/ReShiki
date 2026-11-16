---
section: Developer
issues: []
---

- The Linux arm64 release build uses thin link-time optimization, as the macOS
  builds do. Its fat-LTO link peaks at 18.6 GiB and the runner has 16 GB, so
  the runner was shut down during the link and the nightly failed. The thin-LTO
  link peaks at 8.5 GiB; the executable is 84.8 MB instead of 79.1 MB. See
  [Build locally](releasing.md#build-locally).
