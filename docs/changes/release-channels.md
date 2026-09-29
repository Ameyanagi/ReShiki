# Stable downloads and optional nightly builds

The homepage's primary action says **Download stable**, and its release badge says **Stable release**. The README and installation guide also link directly to `/releases/latest`. Nightly builds remain available through a smaller homepage link and a separate installation section for testers.

GitHub's **Latest** designation identifies the stable release. It does not pin that release above newer nightlies in the chronological all-releases list. Nightly publication retains `--prerelease --latest=false`.

## Validation

- Base: `60718b3` on `main`.
- The homepage still requests GitHub's `/releases/latest` endpoint and refuses draft or prerelease data and non-stable version tags. Release data can update only the displayed version, never the fixed download destination. A failed request leaves a working stable download link.
- `bun run docs:check`: 16 Astro files, no errors, warnings, or hints.
- `bun run docs:build`: 81 pages built; 6,619 local links/assets and all 750 palette colors verified. Astro emitted `use astro:head-inject` bundler warnings while completing the build.
- `bun run lint:js`, formatting, and `git diff --check` passed. Inspection of the built homepage and installation guide confirmed the stable labels, fixed `/releases/latest` destinations, and the `nightly-builds` anchor.
- Matched browser screenshots and review: pending parent review. No desktop application behavior changes in this PR.

## Release-note material

Caption: **Download the latest stable ReShiki from the homepage or README; optional nightly builds have separate testing instructions.**

Visual evidence: pending browser captures.
