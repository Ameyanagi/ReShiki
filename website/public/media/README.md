# ReShiki promo

`reshiki-promo.mp4` and `reshiki-promo-poster.png` are copies of `out/reshiki-promo.mp4` and `out/thumbnail.png` from the sibling `reshiki-promo` project, imported on 2026-09-23. The video is unchanged: 83.2 seconds, 1920 × 1080, H.264 video and AAC audio, with English captions burned in.

Video SHA-256: `f769c30b0ae1560fe55cbf97c65d1964efae5e5552adbb88f7c7661d252816cc`

The homepage uses the poster until the viewer starts playback. Native controls, inline playback, a direct download link, and `preload="none"` keep it usable on mobile without downloading the full video on page load. These are normal static files included in the existing GitHub Pages build; no video service or Git LFS is required.

To refresh the video, copy the two rendered files from the promo project, update this checksum, and run `bun run docs:build` from the ReShiki repository root.
