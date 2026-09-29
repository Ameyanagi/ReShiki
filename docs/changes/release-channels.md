# Stable downloads and optional nightly builds

The homepage's primary action says **Download stable**, and its release badge says **Stable release**. The README and installation guide also link directly to `/releases/latest`. Nightly builds remain available through a smaller homepage link and a separate installation section for testers.

GitHub's **Latest** designation identifies the stable release. It does not pin that release above newer nightlies in the chronological all-releases list. Nightly publication retains `--prerelease --latest=false`.

## Validation

- Base: `60718b3` on `main`.
- The homepage still requests GitHub's `/releases/latest` endpoint and refuses draft or prerelease data and non-stable version tags. Release data can update only the displayed version, never the fixed download destination. A failed request leaves a working stable download link.
- `bun run docs:check`: 16 Astro files, no errors, warnings, or hints.
- `bun run docs:build`: 81 pages built; 6,620 local links/assets and all 750 palette colors verified. Astro emitted `use astro:head-inject` bundler warnings while completing the build.
- `bun run lint:js`, formatting, and `git diff --check` passed. Inspection of the built homepage and installation guide confirmed the stable labels, fixed `/releases/latest` destinations, and the `nightly-builds` anchor.
- Native Microsoft Edge interaction review passed on macOS: **Nightly builds** opened `/guide/install/#nightly-builds`, with readable, unclipped instructions. **Download stable** opened GitHub's `v0.9.1` release with the **Latest** label. The homepage displayed **Stable release · v0.9.1**.

## Browser evidence

This is an after-only example of the new release-channel presentation, not a matched before/after bug comparison. The input was the production website build from `096b7e66791407b4b2593a2b37a39649ac541e75`, served locally. The reviewer opened the homepage in Microsoft Edge on macOS 26.5.1 arm64, followed both download paths, returned to the homepage, and saved a full-page capture through Edge's own capture UI. The JPEG below is unedited, 2674 × 4989 pixels (602,247 bytes), captured on September 29, 2026. Browser zoom and viewport were not separately recorded. The saved image was inspected for readability, clipping, and unrelated content. No desktop application behavior changes in this PR.

![ReShiki homepage with an orange Download stable button, a Stable release version badge, and a smaller Nightly builds link beside the installation guide.](../images/release-channels/homepage.jpg)

## Release-note material

Caption: **Download the latest stable ReShiki from the homepage or README; optional nightly builds have separate testing instructions.**

Reusable image: [`docs/images/release-channels/homepage.jpg`](../images/release-channels/homepage.jpg).
