---
section: Developer
issues: []
---

- The Linux arm64 release build no longer uses link-time optimization across
  crates: it builds with the `release` profile's settings (`lto = false`, 16
  codegen units). Its
  fat-LTO link peaks at 18.6 GiB and the runner has 16 GB, so the runner was
  shut down during the link and the nightly failed. The build now peaks at
  2.8 GiB. The portable archive is 36.7 MB instead of 35.8 MB and the
  executable 89.8 MB instead of 79.1 MB. See
  [Build locally](releasing.md#build-locally).
