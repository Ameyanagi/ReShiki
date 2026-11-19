---
section: Developer
issues: []
---

- The macOS disk image build converts a fresh copy of the intermediate image and
  tries each `hdiutil` step up to five times with a growing wait, after the
  0.16.0 tag build failed three times with "Resource temporarily unavailable"
  ([Releasing](releasing.md)).
