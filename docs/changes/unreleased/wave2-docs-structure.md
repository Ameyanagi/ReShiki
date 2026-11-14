---
section: Developer
issues: []
---

- **Website sidebar.** The site's sidebar follows a learning path: Start,
  Draw, Finish and share, Chemistry tools, AI and Reference, then Release
  notes (newest first), Integrations, Developer and Engineering notes. The
  Python package, the npm package and the SciFinder page are now in the
  sidebar, and the npm package README is published on the site.
- **Records leave the site.** 29 review, validation and release-validation
  records moved from `docs/` to `docs/changes/`. Their old website addresses
  redirect to the records on GitHub. New release validation records go in
  `docs/changes/release-<version>-validation.md`.
- **Release notes template.** [Writing and terms](writing-style.md#release-notes)
  sets the sections of every release note. Build and verification details now
  live in the release validation records.
- **README and build guide.** The README is a short landing page that names
  `uv add reshiki` and `bun add @ameyanagi/reshiki`. [Build and test](development.md)
  lists the prerequisites for macOS, Windows and Linux, the test variables in
  sh and PowerShell, CI's cargo-nextest commands and common failures.
- **Smaller manual images.** The user guide's screenshots are lossless WebP,
  which cuts them from 18.15 MB to 4.55 MB.
