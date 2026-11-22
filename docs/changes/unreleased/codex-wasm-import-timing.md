---
section: Fixes
issues: [1372]
---

- Headless molecule imports now run in plain `wasm32-unknown-unknown` hosts
  without JavaScript clock imports. These hosts retain bounded chemistry work
  and memory allowances. Browser builds keep their JavaScript-backed clocks
  through the `reshiki-io/browser` feature; native timing is unchanged.
