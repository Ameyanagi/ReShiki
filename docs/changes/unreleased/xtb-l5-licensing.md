---
section: Developer
issues: []
---

- The Cargo feature `xtb` (off by default) links axtb, an LGPL-3.0-or-later implementation of the xTB methods; the macOS and Linux desktop releases turn it on. The licence audit fails if any default build contains a GPL-family crate, and notices include axtb's texts only in builds that contain it. See [xTB licensing](xtb-licensing.md).
