# ReShiki videos

These videos and posters are unchanged copies from the sibling `reshiki-promo` project, imported on 2026-09-28.

- **Full tour:** `reshiki-promo.mp4`, from `out/reshiki-full-0.9.1.mp4`, is 165.59 seconds (displayed as 2:46).
- **Release highlights:** `reshiki-0.9.1.mp4` is 130.45 seconds (displayed as 2:10). It covers the broader 0.9 features; the 0.9.1 patch fixes Windows dropdown shadows.
- Both are 1920 × 1080, with H.264 video, AAC audio, and English captions burned in.

| Website file               | Source file                    | SHA-256                                                            |
| -------------------------- | ------------------------------ | ------------------------------------------------------------------ |
| `reshiki-promo.mp4`        | `out/reshiki-full-0.9.1.mp4`   | `1ff808ef08f075528065483b65620151f41c7b899edf3c10f61414bc4cbf504b` |
| `reshiki-promo-poster.png` | `out/thumbnail-full-0.9.1.png` | `45092518c988befa54bf25ee9e24cb54307dd7d518ee01dccf9a363b64dd3e30` |
| `reshiki-0.9.1.mp4`        | `out/reshiki-0.9.1.mp4`        | `c26a65ae1c88d544217485bd590b267f4413ef05235bfb97fd74c358f2b9cb88` |
| `reshiki-0.9.1-poster.png` | `out/thumbnail-0.9.1.png`      | `78505bef819e38582370d544bea54af98445558d57cb69cb1d1815ab5a207804` |

The homepage features the full tour and links to the release-highlights player in the 0.9.1 changelog. The README uses the full-tour poster as a link to the homepage player.

Both players display their posters until playback starts. Native controls, inline playback, direct download links, and `preload="none"` let visitors choose when to load the videos. The full tour keeps its existing `/media/reshiki-promo.mp4` URL so older links continue to work. These static files are included in the existing GitHub Pages build.

To refresh a video, copy the rendered MP4 and matching poster from the promo project, update its duration and checksum here and in the page captions, and run `bun run docs:check` and `bun run docs:build` from the ReShiki repository root. Verify playback and mobile layout before publishing.
