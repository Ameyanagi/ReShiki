# ReShiki · Reaction Cycle

The approved identity is an open molecular ring ending in a reaction arrow. It represents iteration and transformation. The color is **rust**, inspired by red iron oxide and ReShiki's Rust implementation.

The tagline is **“Chemical drawing, reinvented.”**, matching the `reshiki-promo` project.

Open `index.html` for a side-by-side preview of all variants.

| Asset | Use |
| --- | --- |
| `mark-light.png`, `mark-dark.png` | Standalone symbol; transparent PNGs for light and dark surfaces |
| `wordmark-light.png` | Horizontal logo on a transparent background |
| `wordmark-dark.png` | Horizontal logo on an opaque charcoal background |
| `app-icon-light.png` | App/profile icon; transparent outside the rounded ivory tile |
| `app-icon-dark.png` | Fully opaque square dark app/profile icon; charcoal fills the entire canvas |
| `promotion-light.png`, `promotion-dark.png` | Opaque promotional banners with a wordmark, tagline, and website |
| `exports/light/`, `exports/dark/` | App PNGs at 16, 24, 32, 48, 64, 128, 256, 512, and 1024 pixels; social banners at 1200 × 630 |
| `reshiki.icns`, `reshiki-dark.icns` | Multi-resolution macOS icons |
| `reshiki.ico`, `reshiki-dark.ico` | Multi-resolution Windows icons |
| `runtime/` | Compact assets embedded in the desktop application |

## Palette

| Color | Target | Use |
| --- | --- | --- |
| Rust | `#B5472B` | Primary mark on light backgrounds |
| Warm rust | `#E98C68` | Mark on dark backgrounds |
| Charcoal | `#202623` | Dark surface and light-mode lettering |
| Ivory | `#F7F7F2` | Light surface and dark-mode lettering |

These are the exact CSS palette values. Generated raster artwork may vary slightly. The high-resolution masters are PNG files, not vector artwork.

Keep the symbol upright and preserve its proportions, open right side, internal bond, and arrowhead. Leave clear space around it; do not crop into the ring. Use the standalone symbol for small placements and the full wordmark when the name needs to be readable. Avoid golden or metallic copper treatments.

The light icon is the default packaged application icon. The dark icon is supplied as an alternate; the desktop application does not switch its installed icon automatically.

The dark app icon has a solid background at every size, with no transparent patches. Platforms can apply their own corner mask. Symbol-only PNGs retain transparency for use over other backgrounds.

## Rebuild exports

From the repository root on macOS, run `bun scripts/export_branding.mjs`. This uses `sips` and `iconutil` to resize and package the approved PNGs, and refreshes the app and website resources. It does not regenerate the artwork. The built-in image generation prompts are preserved in `prompts.md`.
